//! The QML side's only window to lmix-core: show state over WebSocket,
//! actions over HTTP, and live pictures over the Unix sockets lmix-core
//! hands out (`/api/live/…`), each drawn into a `GstGLQt6VideoItem` by
//! `unixfdsrc ! glupload ! qml6glsink` without a copy through the GUI.

use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use futures_util::StreamExt;
use gstreamer as gst;
use gstreamer::prelude::*;
use lmix_types::{Action, Event, InputRef, Latency, Levels, LiveFeed, OutputStats};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::{config, forms};

#[cxx_qt::bridge]
pub mod qobject {
    unsafe extern "C++" {
        include!("cxx-qt-lib/qstring.h");
        type QString = cxx_qt_lib::QString;

        include!(<QtQuick/QQuickItem>);
        type QQuickItem;
    }

    extern "RustQt" {
        #[qobject]
        #[qml_element]
        #[qproperty(bool, connected)]
        #[qproperty(QString, state)]
        #[qproperty(QString, message)]
        /// `KanteStyle.kind`: 0 System, 1 Kante, 2 Kante Light.
        #[qproperty(i32, style)]
        /// Peak dBFS per input id, and `program` (JSON object).
        #[qproperty(QString, levels)]
        /// Link health per output id: `{rate, detail}` (JSON object).
        #[qproperty(QString, links)]
        /// Capture devices (JSON list of lmix `Device`).
        #[qproperty(QString, devices)]
        /// Recently opened or saved show files (JSON list).
        #[qproperty(QString, recent)]
        /// Last latency measurement, as the toolbar shows it.
        #[qproperty(QString, latency)]
        #[qproperty(bool, measuring)]
        type Core = super::CoreRust;

        /// Run an lmix action given as its JSON (the API's own format);
        /// false when it is no valid action.
        #[qinvokable]
        fn act(self: Pin<&mut Core>, action: &QString) -> bool;

        /// Add an input from the form; false while a needed field is empty.
        #[qinvokable]
        #[cxx_name = "addInput"]
        fn add_input(self: Pin<&mut Core>, kind: &QString, value: &QString, name: &QString)
            -> bool;

        /// Give input `number` a new source from the form; false while a
        /// needed field is empty.
        #[qinvokable]
        #[cxx_name = "replaceSource"]
        fn replace_source(
            self: Pin<&mut Core>,
            number: i32,
            kind: &QString,
            value: &QString,
        ) -> bool;

        /// Add an output from the form; `source` is "program" or an input
        /// id. False while a needed field is empty.
        #[qinvokable]
        #[cxx_name = "addOutput"]
        fn add_output(
            self: Pin<&mut Core>,
            kind: &QString,
            text: &QString,
            source: &QString,
            container: &QString,
            video_kbps: i32,
            name: &QString,
        ) -> bool;

        /// Fetch devices and recent shows again.
        #[qinvokable]
        #[cxx_name = "refreshLists"]
        fn refresh_lists(self: Pin<&mut Core>);

        /// Measure capture-to-program latency for a few seconds.
        #[qinvokable]
        #[cxx_name = "measureLatency"]
        fn measure_latency(self: Pin<&mut Core>);

        /// Choices of a form list as JSON `[{value, label}]`: sources,
        /// patterns, effects, colors, containers, outputs, durations.
        #[qinvokable]
        fn choices(self: &Core, list: &QString) -> QString;

        /// `h:mm:ss` since a Unix time in ms.
        #[qinvokable]
        #[cxx_name = "runningTime"]
        fn running_time(self: &Core, since_ms: f64) -> QString;
    }

    unsafe extern "RustQt" {
        /// Show `source` ("program" or an input id) in `item`, a
        /// `GstGLQt6VideoItem` whose scene graph is up.
        #[qinvokable]
        #[cxx_name = "showLive"]
        unsafe fn show_live(self: Pin<&mut Core>, item: *mut QQuickItem, source: &QString);

        /// Stop drawing into `item` (before it goes away).
        #[qinvokable]
        #[cxx_name = "releaseLive"]
        unsafe fn release_live(self: Pin<&mut Core>, item: *mut QQuickItem);
    }

    impl cxx_qt::Threading for Core {}
    impl cxx_qt::Initialize for Core {}
}

/// Wait between reconnect attempts, and before a broken picture is asked
/// for again.
const RETRY: Duration = Duration::from_secs(1);

/// One item's picture: the pipeline and what it shows.
struct Live {
    source: String,
    /// Bumped with every new request, so a late answer for an old source
    /// (or a released item) is dropped.
    generation: u64,
    pipeline: Option<gst::Pipeline>,
    /// Ends the bus watch of `pipeline`.
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}

#[derive(Default)]
pub struct CoreRust {
    connected: bool,
    state: QString,
    message: QString,
    style: i32,
    levels: QString,
    links: QString,
    devices: QString,
    recent: QString,
    latency: QString,
    measuring: bool,
    /// Keyed by the item's address.
    live: HashMap<usize, Live>,
    next_generation: u64,
}

impl cxx_qt::Initialize for qobject::Core {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().set_style(config::style());
        self.as_mut()
            .set_latency(QString::from(&forms::latency_text(None)));
        let thread = self.qt_thread();
        config::runtime().spawn(follow(thread));
        self.refresh_lists();
    }
}

/// Follow lmix-core's event stream forever, reconnecting when it drops.
async fn follow(thread: cxx_qt::CxxQtThread<qobject::Core>) {
    let url = config::events_url();
    loop {
        if let Ok((mut stream, _)) = connect_async(&url).await {
            let _ = thread.queue(|core| core.set_connected(true));
            while let Some(Ok(message)) = stream.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let queued = match serde_json::from_str(&text) {
                    Ok(Event::State { state }) => {
                        let json = serde_json::to_string(&state).unwrap_or_default();
                        thread.queue(move |core| core.set_state(QString::from(&json)))
                    }
                    Ok(Event::Levels { levels }) => {
                        let json = levels_json(&levels);
                        thread.queue(move |core| core.set_levels(QString::from(&json)))
                    }
                    Ok(Event::Outputs { stats }) => {
                        let json = links_json(&stats);
                        thread.queue(move |core| core.set_links(QString::from(&json)))
                    }
                    _ => continue,
                };
                if queued.is_err() {
                    return;
                }
            }
        }
        if thread.queue(|core| core.set_connected(false)).is_err() {
            return;
        }
        tokio::time::sleep(RETRY).await;
    }
}

/// `{ "<input id>": peak, "program": peak }`.
fn levels_json(levels: &Levels) -> String {
    let mut map = serde_json::Map::new();
    for level in &levels.inputs {
        map.insert(level.input.to_string(), level.peak_db.into());
    }
    if let Some(peak) = levels.program_peak_db {
        map.insert("program".into(), peak.into());
    }
    serde_json::Value::Object(map).to_string()
}

/// `{ "<output id>": {rate, detail} }`.
fn links_json(stats: &[OutputStats]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = stats
        .iter()
        .map(|s| {
            (
                s.output.to_string(),
                serde_json::json!({
                    "rate": forms::link_rate(s),
                    "detail": forms::link_detail(s),
                }),
            )
        })
        .collect();
    serde_json::Value::Object(map).to_string()
}

impl qobject::Core {
    fn act(self: Pin<&mut Self>, action: &QString) -> bool {
        match serde_json::from_str::<Action>(&action.to_string()) {
            Ok(action) => {
                self.send(action);
                true
            }
            Err(e) => {
                tracing::error!(%action, error = %e, "QML sent no valid action");
                false
            }
        }
    }

    fn add_input(self: Pin<&mut Self>, kind: &QString, value: &QString, name: &QString) -> bool {
        let Some(source) = forms::source_of(&kind.to_string(), &value.to_string()) else {
            return false;
        };
        let name = name.to_string().trim().to_string();
        self.send(Action::AddInput {
            source,
            name: (!name.is_empty()).then_some(name),
        });
        true
    }

    fn replace_source(self: Pin<&mut Self>, number: i32, kind: &QString, value: &QString) -> bool {
        let Some(source) = forms::source_of(&kind.to_string(), &value.to_string()) else {
            return false;
        };
        self.send(Action::ReplaceSource {
            input: InputRef::Number(number.max(0) as u32),
            source,
        });
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn add_output(
        self: Pin<&mut Self>,
        kind: &QString,
        text: &QString,
        source: &QString,
        container: &QString,
        video_kbps: i32,
        name: &QString,
    ) -> bool {
        let spec = forms::output_of(
            &kind.to_string(),
            &text.to_string(),
            &container.to_string(),
            video_kbps.max(0) as u32,
        );
        let Some(spec) = spec else {
            return false;
        };
        let name = name.to_string().trim().to_string();
        self.send(Action::AddOutput {
            spec,
            source: forms::output_source_of(&source.to_string()),
            name: (!name.is_empty()).then_some(name),
        });
        true
    }

    fn refresh_lists(self: Pin<&mut Self>) {
        let thread = self.qt_thread();
        config::runtime().spawn(async move {
            let devices = get_json::<serde_json::Value>("/api/devices")
                .await
                .unwrap_or_else(|| serde_json::json!([]))
                .to_string();
            let recent = get_json::<serde_json::Value>("/api/shows/recent")
                .await
                .unwrap_or_else(|| serde_json::json!([]))
                .to_string();
            let _ = thread.queue(move |mut core| {
                core.as_mut().set_devices(QString::from(&devices));
                core.set_recent(QString::from(&recent));
            });
        });
    }

    fn measure_latency(mut self: Pin<&mut Self>) {
        if self.rust().measuring {
            return;
        }
        self.as_mut().set_measuring(true);
        let thread = self.qt_thread();
        config::runtime().spawn(async move {
            let latency = measure().await;
            let text = forms::latency_text(latency.as_ref());
            let _ = thread.queue(move |mut core| {
                core.as_mut().set_latency(QString::from(&text));
                core.set_measuring(false);
            });
        });
    }

    fn choices(&self, list: &QString) -> QString {
        let json = match list.to_string().as_str() {
            "sources" => forms::source_kinds(),
            "patterns" => forms::patterns(),
            "effects" => forms::effects(),
            "colors" => forms::input_colors(),
            "containers" => forms::containers(),
            "outputs" => forms::output_kinds(),
            "durations" => serde_json::to_string(&forms::DURATION_PRESETS_MS).unwrap_or_default(),
            _ => "[]".into(),
        };
        QString::from(&json)
    }

    fn running_time(&self, since_ms: f64) -> QString {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        QString::from(&forms::running_time(since_ms.max(0.0) as u64, now_ms))
    }

    fn send(self: Pin<&mut Self>, action: Action) {
        let thread = self.qt_thread();
        config::runtime().spawn(async move {
            let refused = post_action(&action).await;
            if let Some(reason) = &refused {
                tracing::info!(?action, %reason, "lmix-core refused");
            }
            let text = refused.unwrap_or_default();
            let _ = thread.queue(move |core| core.set_message(QString::from(&text)));
        });
    }

    unsafe fn show_live(self: Pin<&mut Self>, item: *mut qobject::QQuickItem, source: &QString) {
        self.request_live(item as usize, source.to_string());
    }

    unsafe fn release_live(mut self: Pin<&mut Self>, item: *mut qobject::QQuickItem) {
        if let Some(live) = self.as_mut().rust_mut().live.remove(&(item as usize)) {
            stop(live);
        }
    }

    /// Ask lmix-core for the socket of `source`, then draw it into `item`.
    fn request_live(mut self: Pin<&mut Self>, item: usize, source: String) {
        let generation = {
            let mut rust = self.as_mut().rust_mut();
            rust.next_generation += 1;
            rust.next_generation
        };
        let old = self.as_mut().rust_mut().live.insert(
            item,
            Live {
                source: source.clone(),
                generation,
                pipeline: None,
                stop: None,
            },
        );
        if let Some(old) = old {
            stop(old);
        }

        let thread = self.qt_thread();
        config::runtime().spawn(async move {
            let socket = loop {
                match live_socket(&source).await {
                    Ok(socket) => break socket,
                    Err(e) => {
                        tracing::debug!(%source, error = %e, "no live picture yet");
                        tokio::time::sleep(RETRY).await;
                    }
                }
                if thread.is_destroyed() {
                    return;
                }
            };
            let _ = thread.queue(move |core| core.attach(item, generation, socket));
        });
    }

    /// Start the pipeline for `item` unless a newer request replaced it.
    fn attach(mut self: Pin<&mut Self>, item: usize, generation: u64, socket: String) {
        let current = self
            .rust()
            .live
            .get(&item)
            .is_some_and(|live| live.generation == generation);
        if !current {
            return;
        }
        let pipeline = match pipeline(item, &socket) {
            Ok(pipeline) => pipeline,
            Err(e) => {
                tracing::warn!(%socket, error = %e, "live pipeline failed");
                self.as_mut().set_message(QString::from(&e));
                return;
            }
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let bus = pipeline.bus().expect("pipeline has a bus");
        let thread = self.qt_thread();
        config::runtime().spawn(async move {
            let mut messages =
                bus.stream_filtered(&[gst::MessageType::Error, gst::MessageType::Eos]);
            tokio::select! {
                _ = rx => return,
                message = messages.next() => {
                    if let Some(message) = message {
                        tracing::info!(?message, "live picture broke, asking again");
                    }
                }
            }
            tokio::time::sleep(RETRY).await;
            let _ = thread.queue(move |core| core.retry(item, generation));
        });
        if let Some(live) = self.as_mut().rust_mut().live.get_mut(&item) {
            live.pipeline = Some(pipeline);
            live.stop = Some(tx);
        }
    }

    /// The engine dropped the picture (restart, rebuilt flow): ask again.
    fn retry(self: Pin<&mut Self>, item: usize, generation: u64) {
        let source = match self.rust().live.get(&item) {
            Some(live) if live.generation == generation => live.source.clone(),
            _ => return,
        };
        self.request_live(item, source);
    }
}

fn stop(mut live: Live) {
    if let Some(stop) = live.stop.take() {
        let _ = stop.send(());
    }
    if let Some(pipeline) = live.pipeline.take() {
        let _ = pipeline.set_state(gst::State::Null);
    }
}

/// `unixfdsrc ! glupload ! glcolorconvert ! qml6glsink` into `item`.
fn pipeline(item: usize, socket: &str) -> Result<gst::Pipeline, String> {
    let make = |name: &str| {
        gst::ElementFactory::make(name)
            .build()
            .map_err(|_| format!("GStreamer element {name} missing"))
    };
    let src = make("unixfdsrc")?;
    src.set_property("socket-path", socket);
    let queue = make("queue")?;
    queue.set_property("max-size-buffers", 2u32);
    queue.set_property_from_str("leaky", "downstream");
    let upload = make("glupload")?;
    let convert = make("glcolorconvert")?;
    let sink = make("qml6glsink")?;
    // The engine paces the frames; drawing them on arrival keeps the
    // monitor as close to program as the transport allows.
    sink.set_property("sync", false);
    sink.set_property("widget", item as gst::glib::Pointer);

    let pipeline = gst::Pipeline::new();
    let elements = [&src, &queue, &upload, &convert, &sink];
    pipeline.add_many(elements).map_err(|e| e.to_string())?;
    gst::Element::link_many(elements).map_err(|e| e.to_string())?;
    if tracing::enabled!(tracing::Level::DEBUG) {
        measure_frames(&sink, socket.to_string());
    }
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| format!("live pipeline does not start: {e}"))?;
    Ok(pipeline)
}

/// Report every few seconds how many frames reached `sink` and how old
/// they were: the engine stamps them in running time on the shared
/// monotonic clock and unixfdsrc maps that onto this pipeline, so the age
/// is engine stamp to GUI sink (not counting the display).
fn measure_frames(sink: &gst::Element, socket: String) {
    const EVERY: Duration = Duration::from_secs(5);
    let Some(pad) = sink.static_pad("sink") else {
        return;
    };
    let window = std::sync::Mutex::new((Vec::<u64>::new(), std::time::Instant::now()));
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let mut window = window.lock().expect("measure lock");
        let (ages, since) = &mut *window;
        let now = pad.parent_element().and_then(|e| e.current_running_time());
        if let (Some(buffer), Some(now)) = (info.buffer(), now) {
            if let Some(pts) = buffer.pts() {
                ages.push(now.saturating_sub(pts).mseconds());
            }
        }
        if since.elapsed() >= EVERY {
            let frames = ages.len();
            ages.sort_unstable();
            let p50 = ages.get(frames / 2).copied().unwrap_or(0);
            let p95 = ages.get(frames * 95 / 100).copied().unwrap_or(0);
            tracing::debug!(
                %socket,
                fps = frames as f64 / since.elapsed().as_secs_f64(),
                age_p50_ms = p50,
                age_p95_ms = p95,
                "live picture"
            );
            ages.clear();
            *since = std::time::Instant::now();
        }
        gst::PadProbeReturn::Ok
    });
}

/// Run an action; the reason when lmix-core refused it.
async fn post_action(action: &Action) -> Option<String> {
    let resp = match config::http()
        .post(format!("{}/api/actions", config::core_url()))
        .json(action)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => return Some(e.to_string()),
    };
    if resp.status().is_success() {
        return None;
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    let message = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_string));
    Some(message.unwrap_or_else(|| format!("lmix-core {status}: {body}")))
}

async fn get_json<T: serde::de::DeserializeOwned>(path: &str) -> Option<T> {
    let resp = config::http()
        .get(format!("{}{path}", config::core_url()))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    resp.json().await.ok()
}

/// Seconds a latency measurement started from the Console runs.
const LATENCY_SECONDS: u64 = 5;

/// Run a latency measurement; `None` when lmix-core or the engine cannot.
async fn measure() -> Option<Latency> {
    let resp = config::http()
        .post(format!(
            "{}/api/latency/measure?seconds={LATENCY_SECONDS}",
            config::core_url()
        ))
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    tokio::time::sleep(Duration::from_secs(LATENCY_SECONDS + 1)).await;
    get_json::<Latency>("/api/latency")
        .await
        .filter(|l| l.samples > 0)
}

/// Socket of a live picture on this machine.
async fn live_socket(source: &str) -> Result<String, String> {
    let path = live_path(source);
    let resp = config::http()
        .get(format!("{}{path}", config::core_url()))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("{path}: {}", resp.status()));
    }
    let feed: LiveFeed = resp.json().await.map_err(|e| e.to_string())?;
    Ok(feed.socket)
}

fn live_path(source: &str) -> String {
    match source {
        "program" => "/api/live/program".to_string(),
        input => format!("/api/live/inputs/{input}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn live_path_of_program_and_inputs() {
        assert_eq!(live_path("program"), "/api/live/program");
        assert_eq!(live_path("7d0c"), "/api/live/inputs/7d0c");
    }
}
