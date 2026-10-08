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
use lmix_types::{Action, Effect, Event, InputRef, LiveFeed, TransitionSlot};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::config;

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
        type Core = super::CoreRust;

        /// Put input `number` into preview.
        #[qinvokable]
        fn preview(self: Pin<&mut Core>, number: i32);

        /// Swap preview and program.
        #[qinvokable]
        fn cut(self: Pin<&mut Core>);

        /// Put input `number` straight on program.
        #[qinvokable]
        #[cxx_name = "cutDirect"]
        fn cut_direct(self: Pin<&mut Core>, number: i32);

        /// Take preview to program with transition button `slot` (1–4).
        #[qinvokable]
        fn transition(self: Pin<&mut Core>, slot: i32);

        /// Button text of a transition effect as the state names it.
        #[qinvokable]
        #[cxx_name = "effectLabel"]
        fn effect_label(self: &Core, effect: &QString) -> QString;
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
    /// Keyed by the item's address.
    live: HashMap<usize, Live>,
    next_generation: u64,
}

impl cxx_qt::Initialize for qobject::Core {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().set_style(config::style());
        let thread = self.qt_thread();
        config::runtime().spawn(follow(thread));
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
                let Ok(Event::State { state }) = serde_json::from_str(&text) else {
                    continue;
                };
                let json = serde_json::to_string(&state).unwrap_or_default();
                if thread
                    .queue(move |core| core.set_state(QString::from(&json)))
                    .is_err()
                {
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

impl qobject::Core {
    fn act(self: Pin<&mut Self>, action: Action) {
        let thread = self.qt_thread();
        config::runtime().spawn(async move {
            let refused = post_action(&action).await;
            let text = refused.unwrap_or_default();
            let _ = thread.queue(move |core| core.set_message(QString::from(&text)));
        });
    }

    fn preview(self: Pin<&mut Self>, number: i32) {
        self.act(Action::PreviewInput {
            input: InputRef::Number(number.max(0) as u32),
        });
    }

    fn cut(self: Pin<&mut Self>) {
        self.act(Action::Cut);
    }

    fn cut_direct(self: Pin<&mut Self>, number: i32) {
        self.act(Action::CutDirect {
            input: InputRef::Number(number.max(0) as u32),
        });
    }

    fn transition(self: Pin<&mut Self>, slot: i32) {
        self.act(Action::Transition {
            slot: TransitionSlot(slot.clamp(1, 4) as u8),
        });
    }

    fn effect_label(&self, effect: &QString) -> QString {
        QString::from(&effect_label_of(&effect.to_string()))
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
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| format!("live pipeline does not start: {e}"))?;
    Ok(pipeline)
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

fn effect_label_of(effect: &str) -> String {
    serde_json::from_value::<Effect>(serde_json::Value::String(effect.to_string()))
        .map(|effect| effect.label().to_string())
        .unwrap_or_else(|_| effect.to_string())
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

    #[test]
    fn effect_label_from_state_name() {
        assert_eq!(effect_label_of("dip_to_black"), "Dip");
        assert_eq!(effect_label_of("unknown"), "unknown");
    }
}
