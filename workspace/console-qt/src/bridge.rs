//! The QML side's only window to lmix-core: show state over WebSocket,
//! actions over HTTP, and live pictures over the Unix sockets lmix-core
//! hands out (`/api/live/…`), each drawn into a `GstGLQt6VideoItem` by
//! `unixfdsrc ! glupload ! qml6glsink` (or from the GPU, `cudaipcsrc`)
//! without a copy through the GUI.

use std::collections::HashMap;
use std::pin::Pin;
use std::time::Duration;

use cxx_qt::{CxxQtType, Threading};
use cxx_qt_lib::QString;
use futures_util::StreamExt;
use gstreamer as gst;
use gstreamer::prelude::*;
use lmix_types::{
    state::TILES_LEASE, Action, Event, InputRef, Latency, Levels, LiveFeed, LiveTiles,
    LiveTransport, OutputStats,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::latest::Latest;
use crate::{config, forms, state_patch};

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
        /// Last refusal or error as JSON: lmix-core's `{error, message}`
        /// (Texts.qml translates by `error.error`), or `{message}`; empty
        /// when the last action went through.
        #[qproperty(QString, message)]
        /// `KanteStyle.kind` from the command line (0 System, 1 Kante,
        /// 2 Kante Light), or -1: the one saved in the Console.
        #[qproperty(i32, style)]
        /// Counts the level measurements: a binding that reads it with
        /// `peak` or `channel` follows them.
        #[qproperty(i32, levels_tick, cxx_name = "levelsTick")]
        /// Where clips play: `{input id: [position ms, length ms]}`.
        #[qproperty(QString, positions)]
        /// Link health per output id: lmix `OutputStats` (JSON object).
        #[qproperty(QString, links)]
        /// The load of lmix-core's machine and how its cue timer runs:
        /// lmix `MachineLoad` as JSON, or empty while none came (RUN.8).
        #[qproperty(QString, machine)]
        /// Where the slots sit in the tile picture (`showLive(item,
        /// "tiles")`): `{columns, rows, cell_width, cell_height}` of lmix
        /// `LiveTiles`, or empty while no tile picture runs.
        #[qproperty(QString, tiles)]
        /// Capture devices (JSON list of lmix `Device`).
        #[qproperty(QString, devices)]
        /// Recently opened or saved show files (JSON list).
        #[qproperty(QString, recent)]
        /// Title templates (JSON list of lmix `Template`).
        #[qproperty(QString, templates)]
        /// lmix-core's address, for its pages (graphics preview).
        #[qproperty(QString, core_url, cxx_name = "coreUrl")]
        /// How the window opens: normal, maximized, fullscreen.
        #[qproperty(QString, window_mode, cxx_name = "windowMode")]
        /// `simple` or `advanced` from the command line, or empty.
        #[qproperty(QString, ui_mode, cxx_name = "uiMode")]
        /// The page asked for with `--page`, or empty.
        #[qproperty(QString, start_page, cxx_name = "startPage")]
        /// Last latency measurement: lmix `Latency` as JSON, or empty.
        #[qproperty(QString, latency)]
        #[qproperty(bool, measuring)]
        /// Global shortcuts: "" off, "on:<count>" bound, "off:<reason>"
        /// when the desktop has no portal or refused.
        #[qproperty(QString, global_keys, cxx_name = "globalKeys")]
        /// Another lmix window on this computer binds the global keys;
        /// this one tries again (AppShell).
        #[qproperty(bool, global_held, cxx_name = "globalHeld")]
        type Core = super::CoreRust;

        /// The show state changed: what changed, as `crate::state_patch`
        /// operations (JSON list). The first one sets the whole state.
        #[qsignal]
        #[cxx_name = "statePatched"]
        fn state_patched(self: Pin<&mut Core>, patch: QString);

        /// A global shortcut was pressed: the lmix shortcut id.
        #[qsignal]
        #[cxx_name = "globalShortcut"]
        fn global_shortcut(self: Pin<&mut Core>, id: QString);

        /// Bind these shortcuts globally (JSON list of `{id, label, key}`);
        /// an empty list unbinds.
        #[qinvokable]
        #[cxx_name = "setGlobalShortcuts"]
        fn set_global_shortcuts(self: Pin<&mut Core>, json: &QString);

        /// lmix-core answered an action of this Console: the action as
        /// JSON, and the refusal as in `message` (empty when it went
        /// through). One per action, also for the same refusal twice.
        #[qsignal]
        fn answered(self: Pin<&mut Core>, action: QString, refusal: QString);

        /// A note or controller from a MIDI controller at lmix-core: lmix
        /// `MidiMessage` as JSON (to learn a binding).
        #[qsignal]
        #[cxx_name = "midiReceived"]
        fn midi_received(self: Pin<&mut Core>, message: QString);

        /// Run an lmix action given as its JSON (the API's own format);
        /// false when it is no valid action.
        #[qinvokable]
        fn act(self: Pin<&mut Core>, action: &QString) -> bool;

        /// Add an input from the form (`options`: `forms::SourceOptions` as
        /// JSON); false while a needed field is empty.
        #[qinvokable]
        #[cxx_name = "addInput"]
        fn add_input(
            self: Pin<&mut Core>,
            kind: &QString,
            value: &QString,
            options: &QString,
            name: &QString,
        ) -> bool;

        /// Give input `number` a new source from the form; false while a
        /// needed field is empty.
        #[qinvokable]
        #[cxx_name = "replaceSource"]
        fn replace_source(
            self: Pin<&mut Core>,
            number: i32,
            kind: &QString,
            value: &QString,
            options: &QString,
        ) -> bool;

        /// Add an output from the form, as JSON (`forms::OutputForm`).
        /// False while a needed field is empty.
        #[qinvokable]
        #[cxx_name = "addOutput"]
        fn add_output(self: Pin<&mut Core>, form: &QString) -> bool;

        /// The `update_output` action the same form makes of output
        /// `output` (its id), as JSON for `act`; empty while a needed
        /// field is empty.
        #[qinvokable]
        #[cxx_name = "updateOutputAction"]
        fn update_output_action(self: &Core, output: &QString, form: &QString) -> QString;

        /// Fetch devices, recent shows and templates again.
        #[qinvokable]
        #[cxx_name = "refreshLists"]
        fn refresh_lists(self: Pin<&mut Core>);

        /// Measure capture-to-program latency for a few seconds.
        #[qinvokable]
        #[cxx_name = "measureLatency"]
        fn measure_latency(self: Pin<&mut Core>);

        /// Values of a form list as JSON (Texts.qml names them): sources,
        /// patterns, effects, colors, containers, outputs, durations.
        #[qinvokable]
        fn choices(self: &Core, list: &QString) -> QString;

        /// `h:mm:ss` since a Unix time in ms.
        #[qinvokable]
        #[cxx_name = "runningTime"]
        fn running_time(self: &Core, since_ms: f64) -> QString;

        /// `name` in at most `max` characters for a narrow strip (forms::short_name).
        #[qinvokable]
        #[cxx_name = "shortName"]
        fn short_name(self: &Core, name: &QString, max: i32) -> QString;

        /// Peak dBFS of an input id, `program`, `phones` or `bus:<id>`;
        /// NaN while none was measured.
        #[qinvokable]
        fn peak(self: &Core, key: &QString) -> f64;

        /// Peak dBFS of channel `index` of an input; NaN without it.
        #[qinvokable]
        fn channel(self: &Core, input: &QString, index: i32) -> f64;
    }

    unsafe extern "RustQt" {
        /// Show `source` ("program", "preview", "tiles" or an input id) in `item`, a
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
    /// The show state QML has, the base of the next patch.
    shown: serde_json::Value,
    message: QString,
    style: i32,
    levels_tick: i32,
    /// The last levels by `peak`'s keys, and the channels per input id:
    /// numbers for QML, not an object per measurement (garbage for its
    /// collector 10 times a second, 20 % of the GUI thread with 31 inputs).
    peaks: HashMap<String, f32>,
    channels: HashMap<String, Vec<f32>>,
    positions: QString,
    links: QString,
    machine: QString,
    tiles: QString,
    devices: QString,
    recent: QString,
    templates: QString,
    core_url: QString,
    window_mode: QString,
    ui_mode: QString,
    start_page: QString,
    latency: QString,
    measuring: bool,
    global_keys: QString,
    global_held: bool,
    /// Feeds the global shortcut binder; started on first use.
    global: Option<tokio::sync::watch::Sender<Vec<crate::portal::Bind>>>,
    /// This window binds the global keys of this computer (one window only).
    global_claim: Option<std::fs::File>,
    /// Keyed by the item's address.
    live: HashMap<usize, Live>,
    next_generation: u64,
}

impl cxx_qt::Initialize for qobject::Core {
    fn initialize(mut self: Pin<&mut Self>) {
        self.as_mut().set_style(config::style());
        self.as_mut()
            .set_core_url(QString::from(config::core_url()));
        self.as_mut()
            .set_window_mode(QString::from(config::window()));
        self.as_mut().set_ui_mode(QString::from(config::mode()));
        self.as_mut().set_start_page(QString::from(config::page()));
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
            let mut positions: HashMap<String, (u64, u64)> = HashMap::new();
            // Only the newest of each reaches the GUI thread (crate::latest).
            let state = Latest::new();
            let position = Latest::new();
            let level = Latest::new();
            while let Some(Ok(message)) = stream.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let queued = match serde_json::from_str(&text) {
                    Ok(Event::State { state: newer }) => {
                        if !state.put(serde_json::to_value(&newer).unwrap_or_default()) {
                            continue;
                        }
                        let state = state.clone();
                        thread.queue(move |mut core| {
                            if let Some(newer) = state.take() {
                                let ops = state_patch::diff(&core.shown, &newer);
                                if ops.is_empty() {
                                    return;
                                }
                                core.as_mut().rust_mut().shown = newer;
                                let patch = serde_json::to_string(&ops).unwrap_or_default();
                                core.state_patched(QString::from(&patch));
                            }
                        })
                    }
                    Ok(Event::Position {
                        input,
                        position_ms,
                        duration_ms,
                    }) => {
                        positions.insert(input.to_string(), (position_ms, duration_ms));
                        let json = serde_json::to_string(&positions).unwrap_or_default();
                        if !position.put(json) {
                            continue;
                        }
                        let position = position.clone();
                        thread.queue(move |core| {
                            if let Some(json) = position.take() {
                                core.set_positions(QString::from(&json));
                            }
                        })
                    }
                    Ok(Event::Levels { levels }) => {
                        if !level.put(levels) {
                            continue;
                        }
                        let level = level.clone();
                        thread.queue(move |mut core| {
                            if let Some(levels) = level.take() {
                                core.as_mut().take_levels(&levels);
                            }
                        })
                    }
                    Ok(Event::Outputs { stats }) => {
                        let json = links_json(&stats);
                        thread.queue(move |core| core.set_links(QString::from(&json)))
                    }
                    Ok(Event::Machine { load }) => {
                        let json = serde_json::to_string(&load).unwrap_or_default();
                        thread.queue(move |core| core.set_machine(QString::from(&json)))
                    }
                    Ok(Event::Midi { midi }) => {
                        let json = serde_json::to_string(&midi).unwrap_or_default();
                        thread.queue(move |core| core.midi_received(QString::from(&json)))
                    }
                    _ => continue,
                };
                if queued.is_err() {
                    return;
                }
            }
        }
        // An old load would warn about a machine nobody hears from.
        if thread
            .queue(|mut core| {
                core.as_mut().set_connected(false);
                core.set_machine(QString::default());
            })
            .is_err()
        {
            return;
        }
        tokio::time::sleep(RETRY).await;
    }
}

/// `peak`'s keys and values: input ids, `program`, `phones`, `bus:<id>`,
/// `zone:<name>`.
fn peaks_of(levels: &Levels) -> impl Iterator<Item = (String, f32)> + '_ {
    let inputs = levels
        .inputs
        .iter()
        .map(|l| (l.input.to_string(), l.peak_db));
    let program = levels.program_peak_db.map(|p| ("program".to_string(), p));
    let phones = levels.phones_peak_db.map(|p| ("phones".to_string(), p));
    let buses = levels
        .buses
        .iter()
        .filter_map(|b| Some((format!("bus:{}", b.bus), b.peak_db?)));
    let zones = levels
        .zones
        .iter()
        .filter_map(|z| Some((format!("zone:{}", z.zone), z.peak_db?)));
    inputs
        .chain(program)
        .chain(phones)
        .chain(buses)
        .chain(zones)
}

/// `{ "<output id>": OutputStats }`.
fn links_json(stats: &[OutputStats]) -> String {
    let map: serde_json::Map<String, serde_json::Value> = stats
        .iter()
        .filter_map(|s| Some((s.output.to_string(), serde_json::to_value(s).ok()?)))
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

    fn add_input(
        self: Pin<&mut Self>,
        kind: &QString,
        value: &QString,
        options: &QString,
        name: &QString,
    ) -> bool {
        let Some(source) =
            forms::source_with_options(&kind.to_string(), &value.to_string(), &options.to_string())
        else {
            return false;
        };
        let name = name.to_string().trim().to_string();
        self.send(Action::AddInput {
            source,
            name: (!name.is_empty()).then_some(name),
        });
        true
    }

    fn replace_source(
        self: Pin<&mut Self>,
        number: i32,
        kind: &QString,
        value: &QString,
        options: &QString,
    ) -> bool {
        let Some(source) =
            forms::source_with_options(&kind.to_string(), &value.to_string(), &options.to_string())
        else {
            return false;
        };
        self.send(Action::ReplaceSource {
            input: InputRef::Number(number.max(0) as u32),
            source,
        });
        true
    }

    fn add_output(self: Pin<&mut Self>, form: &QString) -> bool {
        let Some(action) = forms::add_output_action(&form.to_string()) else {
            return false;
        };
        self.send(action);
        true
    }

    fn update_output_action(&self, output: &QString, form: &QString) -> QString {
        forms::update_output_action(&output.to_string(), &form.to_string())
            .and_then(|action| serde_json::to_string(&action).ok())
            .map(|json| QString::from(&json))
            .unwrap_or_default()
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
            let templates = get_json::<serde_json::Value>("/api/templates")
                .await
                .unwrap_or_else(|| serde_json::json!([]))
                .to_string();
            let _ = thread.queue(move |mut core| {
                core.as_mut().set_devices(QString::from(&devices));
                core.as_mut().set_templates(QString::from(&templates));
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
            let text = latency
                .and_then(|l| serde_json::to_string(&l).ok())
                .unwrap_or_default();
            let _ = thread.queue(move |mut core| {
                core.as_mut().set_latency(QString::from(&text));
                core.set_measuring(false);
            });
        });
    }

    fn set_global_shortcuts(mut self: Pin<&mut Self>, json: &QString) {
        #[derive(serde::Deserialize)]
        struct Entry {
            id: String,
            label: String,
            key: String,
        }
        let binds: Vec<crate::portal::Bind> = serde_json::from_str::<Vec<Entry>>(&json.to_string())
            .unwrap_or_default()
            .into_iter()
            .map(|e| crate::portal::Bind {
                id: e.id,
                label: e.label,
                key: e.key,
            })
            .collect();
        if self.rust().global.is_some() {
            // Switched off: another window may bind now. Switched on again
            // while another holds the keys: bind nothing.
            let binds = if binds.is_empty() {
                self.as_mut().rust_mut().global_claim = None;
                self.as_mut().set_global_held(false);
                binds
            } else if self.rust().global_claim.is_some() {
                binds
            } else {
                match crate::portal::claim(&claim_dir()) {
                    Ok(Some(claim)) => {
                        self.as_mut().rust_mut().global_claim = Some(claim);
                        self.as_mut().set_global_held(false);
                        binds
                    }
                    Ok(None) => {
                        self.as_mut()
                            .set_global_keys(QString::from(KEYS_HELD_ELSEWHERE));
                        self.as_mut().set_global_held(true);
                        Vec::new()
                    }
                    Err(e) => {
                        tracing::warn!("global keys: lock file not usable: {e}");
                        self.as_mut()
                            .set_global_keys(QString::from(format!("off:{e}")));
                        self.as_mut().set_global_held(false);
                        Vec::new()
                    }
                }
            };
            if let Some(sender) = &self.rust().global {
                sender.send_if_modified(|now| {
                    let changed = *now != binds;
                    *now = binds;
                    changed
                });
            }
            return;
        }
        if binds.is_empty() {
            return;
        }
        // One window per computer binds: with two, a press would run twice.
        let claim = match crate::portal::claim(&claim_dir()) {
            Ok(Some(claim)) => claim,
            Ok(None) => {
                self.as_mut()
                    .set_global_keys(QString::from(KEYS_HELD_ELSEWHERE));
                self.as_mut().set_global_held(true);
                tracing::debug!("global keys: another lmix window binds them");
                return;
            }
            Err(e) => {
                tracing::warn!("global keys: lock file not usable: {e}");
                self.as_mut()
                    .set_global_keys(QString::from(format!("off:{e}")));
                return;
            }
        };
        tracing::info!("global keys: this window binds them");
        self.as_mut().rust_mut().global_claim = Some(claim);
        self.as_mut().set_global_held(false);
        let (sender, receiver) = tokio::sync::watch::channel(binds);
        let pressed = self.qt_thread();
        let status = self.qt_thread();
        config::runtime().spawn(crate::portal::run(
            receiver,
            move |id| {
                let _ = pressed.queue(move |core| core.global_shortcut(QString::from(&id)));
            },
            move |text| {
                let _ = status.queue(move |core| core.set_global_keys(QString::from(&text)));
            },
        ));
        self.as_mut().rust_mut().global = Some(sender);
    }

    fn choices(&self, list: &QString) -> QString {
        QString::from(&forms::values(&list.to_string()).unwrap_or_else(|| "[]".into()))
    }

    fn running_time(&self, since_ms: f64) -> QString {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        QString::from(&forms::running_time(since_ms.max(0.0) as u64, now_ms))
    }

    fn take_levels(mut self: Pin<&mut Self>, levels: &Levels) {
        {
            let mut rust = self.as_mut().rust_mut();
            rust.peaks.clear();
            rust.peaks.extend(peaks_of(levels));
            rust.channels.clear();
            rust.channels.extend(
                levels
                    .inputs
                    .iter()
                    .filter(|l| !l.channels_db.is_empty())
                    .map(|l| (l.input.to_string(), l.channels_db.clone())),
            );
        }
        let tick = self.levels_tick().wrapping_add(1);
        self.set_levels_tick(tick);
    }

    fn peak(&self, key: &QString) -> f64 {
        self.peaks
            .get(&key.to_string())
            .map_or(f64::NAN, |&p| p.into())
    }

    fn channel(&self, input: &QString, index: i32) -> f64 {
        let channels = self.channels.get(&input.to_string());
        let at = usize::try_from(index).ok();
        channels
            .zip(at)
            .and_then(|(c, i)| c.get(i))
            .map_or(f64::NAN, |&p| p.into())
    }

    fn short_name(&self, name: &QString, max: i32) -> QString {
        QString::from(&forms::short_name(&name.to_string(), max.max(1) as usize))
    }

    fn send(self: Pin<&mut Self>, action: Action) {
        let thread = self.qt_thread();
        let _ = actions().send((action, thread));
    }

    unsafe fn show_live(self: Pin<&mut Self>, item: *mut qobject::QQuickItem, source: &QString) {
        self.request_live(item as usize, source.to_string());
    }

    unsafe fn release_live(mut self: Pin<&mut Self>, item: *mut qobject::QQuickItem) {
        if let Some(live) = self.as_mut().rust_mut().live.remove(&(item as usize)) {
            if live.source == TILES {
                self.as_mut().set_tiles(QString::default());
            }
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
        // Dropped with the request (new source, item released): ends the
        // wait for the socket below.
        let (tx, mut rx) = tokio::sync::oneshot::channel();
        let old = self.as_mut().rust_mut().live.insert(
            item,
            Live {
                source: source.clone(),
                generation,
                pipeline: None,
                stop: Some(tx),
            },
        );
        if let Some(old) = old {
            stop(old);
        }

        let thread = self.qt_thread();
        config::runtime().spawn(async move {
            let feed = loop {
                tokio::select! {
                    _ = &mut rx => return,
                    found = live_feed(&source) => match found {
                        Ok(feed) => break feed,
                        Err(e) => tracing::debug!(%source, error = %e, "no live picture yet"),
                    },
                }
                tokio::select! {
                    _ = &mut rx => return,
                    _ = tokio::time::sleep(RETRY) => {}
                }
            };
            let _ = thread.queue(move |core| core.attach(item, generation, feed));
        });
    }

    /// Start the pipeline for `item` unless a newer request replaced it.
    fn attach(mut self: Pin<&mut Self>, item: usize, generation: u64, feed: Feed) {
        let current = self
            .rust()
            .live
            .get(&item)
            .is_some_and(|live| live.generation == generation);
        if !current {
            return;
        }
        let socket = &feed.socket;
        let pipeline = match pipeline(item, socket, feed.transport, feed.grid.is_some()) {
            Ok(pipeline) => pipeline,
            Err(e) => {
                tracing::warn!(%socket, error = %e, "live pipeline failed");
                self.as_mut().set_message(QString::from(&plain_message(&e)));
                return;
            }
        };
        let (tx, rx) = tokio::sync::oneshot::channel();
        let bus = pipeline.bus().expect("pipeline has a bus");
        let thread = self.qt_thread();
        let tiles = feed.grid.is_some();
        config::runtime().spawn(async move {
            let mut messages =
                bus.stream_filtered(&[gst::MessageType::Error, gst::MessageType::Eos]);
            // Asks on its own, so a slow lmix-core holds up no stop; ends
            // with this task.
            let _renewal = tiles.then(|| Renewal(config::runtime().spawn(renew_tiles())));
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
        if let Some(grid) = feed.grid {
            self.as_mut().set_tiles(QString::from(&grid));
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

/// Status of the global keys while another lmix window binds them; the
/// window tries again (AppShell) until that one is gone.
const KEYS_HELD_ELSEWHERE: &str = "off:another lmix window on this computer binds them";

/// Where the lock of the global keys lies (portal::claim): the user's
/// runtime directory, else `~/.cache/lmix`, both the user's own; only
/// without either a folder per user in the shared temp directory.
fn claim_dir() -> std::path::PathBuf {
    if let Some(dir) = std::env::var_os("XDG_RUNTIME_DIR") {
        return dir.into();
    }
    let dir = std::env::var_os("HOME")
        .map(|home| std::path::Path::new(&home).join(".cache/lmix"))
        .unwrap_or_else(|| {
            let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
            std::env::temp_dir().join(format!("lmix-{user}"))
        });
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn stop(mut live: Live) {
    if let Some(stop) = live.stop.take() {
        let _ = stop.send(());
    }
    if let Some(pipeline) = live.pipeline.take() {
        // The cells stop drawing its textures before they go.
        if live.source == TILES {
            crate::atlas_cell::ffi::atlas_clear();
        }
        let _ = pipeline.set_state(gst::State::Null);
    }
}

/// The frames of `socket` into `item`: `unixfdsrc ! glupload` from
/// system memory, `cudaipcsrc ! cudadownload` from the engine's GPU (a
/// copy on the GPU into a GL texture), then `glcolorconvert ! qml6glsink`.
/// Every frame passes the window's frame gate (`frame_gate`).
///
/// The tile picture (`tiles`) goes to the cells instead (`AtlasCell`),
/// as a texture from an appsink: a layer per tile would cost the window
/// a second frame after each one. `item`'s sink stays for its GL context,
/// shared with the scene graph, and gets no frames.
fn pipeline(
    item: usize,
    socket: &str,
    transport: LiveTransport,
    tiles: bool,
) -> Result<gst::Pipeline, String> {
    let make = |name: &str| {
        gst::ElementFactory::make(name)
            .build()
            .map_err(|_| format!("GStreamer element {name} missing"))
    };
    let (src, upload) = match transport {
        LiveTransport::UnixFd => {
            let src = make("unixfdsrc")?;
            src.set_property("socket-path", socket);
            (src, make("glupload")?)
        }
        LiveTransport::CudaIpc => {
            let src = make("cudaipcsrc")?;
            src.set_property("address", socket);
            (src, make("cudadownload")?)
        }
    };
    let queue = make("queue")?;
    queue.set_property("max-size-buffers", 2u32);
    queue.set_property_from_str("leaky", "downstream");
    let gl = make("capsfilter")?;
    gl.set_property(
        "caps",
        gst::Caps::builder("video/x-raw")
            .features(["memory:GLMemory"])
            .build(),
    );
    let convert = make("glcolorconvert")?;
    let sink = make("qml6glsink")?;
    // The engine paces the frames; drawing them on arrival keeps the
    // monitor as close to program as the transport allows.
    sink.set_property("sync", false);
    sink.set_property("widget", item as gst::glib::Pointer);

    let pipeline = gst::Pipeline::new();
    let elements = [&src, &queue, &upload, &gl, &convert];
    pipeline.add_many(elements).map_err(|e| e.to_string())?;
    gst::Element::link_many(elements).map_err(|e| e.to_string())?;
    // With the window's other pictures: one frame of the window for all.
    let member = crate::frame_gate::GATE.join();
    let drawn: gst::Element = if tiles {
        let tee = make("tee")?;
        let to_sink = make("queue")?;
        let to_cells = make("queue")?;
        let cells = cells_sink(member)?;
        sink.set_property("async", false);
        pipeline
            .add_many([&tee, &to_sink, &sink, &to_cells, cells.upcast_ref()])
            .map_err(|e| e.to_string())?;
        gst::Element::link_many([&convert, &tee, &to_sink, &sink]).map_err(|e| e.to_string())?;
        gst::Element::link_many([&tee, &to_cells, cells.upcast_ref()])
            .map_err(|e| e.to_string())?;
        if let Some(pad) = sink.static_pad("sink") {
            pad.add_probe(gst::PadProbeType::BUFFER, |_, _| gst::PadProbeReturn::Drop);
        }
        cells.upcast()
    } else {
        pipeline.add(&sink).map_err(|e| e.to_string())?;
        convert.link(&sink).map_err(|e| e.to_string())?;
        if let Some(pad) = sink.static_pad("sink") {
            pad.add_probe(gst::PadProbeType::BUFFER, move |_, _| {
                member.pass();
                gst::PadProbeReturn::Ok
            });
        }
        sink
    };
    if tracing::enabled!(tracing::Level::DEBUG) {
        measure_frames(&drawn, socket.to_string());
    }
    pipeline
        .set_state(gst::State::Playing)
        .map_err(|e| format!("live pipeline does not start: {e}"))?;
    Ok(pipeline)
}

/// The tile picture's frames to the cells: RGBA textures, each kept until
/// three more came (the scene graph may still draw the last one). Mapped
/// for GL here, as a sink would: a frame from the GPU copy
/// (`cudadownload`) is only put into its texture then.
fn cells_sink(member: crate::frame_gate::Member) -> Result<gstreamer_app::AppSink, String> {
    let sink = gstreamer_app::AppSink::builder()
        .caps(
            &gst::Caps::builder("video/x-raw")
                .features(["memory:GLMemory"])
                .field("format", "RGBA")
                .field("texture-target", "2D")
                .build(),
        )
        .sync(false)
        .max_buffers(1)
        .drop(true)
        .build();
    let kept = std::sync::Mutex::new(std::collections::VecDeque::new());
    sink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(move |sink| {
                use gstreamer_gl::prelude::*;
                let sample = sink.pull_sample().map_err(|_| gst::FlowError::Eos)?;
                let (Some(buffer), Some(info)) = (
                    sample.buffer_owned(),
                    sample
                        .caps()
                        .and_then(|caps| gstreamer_video::VideoInfo::from_caps(caps).ok()),
                ) else {
                    return Ok(gst::FlowSuccess::Ok);
                };
                let Ok(frame) = gstreamer_gl::GLVideoFrame::from_buffer_readable(buffer, &info)
                else {
                    return Ok(gst::FlowSuccess::Ok);
                };
                let Ok(memory) = frame.memory(0) else {
                    return Ok(gst::FlowSuccess::Ok);
                };
                let (texture, width, height) = (
                    memory.texture_id(),
                    memory.texture_width(),
                    memory.texture_height(),
                );
                // Drawn by the scene graph's context: done in GStreamer's first.
                if let Some(sync) = frame.buffer().meta::<gstreamer_gl::GLSyncMeta>() {
                    sync.set_sync_point(memory.context());
                    sync.wait_cpu(memory.context());
                }
                // Ready to draw: with the window's other pictures.
                member.pass();
                let mut kept = kept.lock().unwrap_or_else(|e| e.into_inner());
                kept.push_back(frame);
                if kept.len() > 3 {
                    kept.pop_front();
                }
                crate::atlas_cell::ffi::atlas_frame(texture, width, height);
                Ok(gst::FlowSuccess::Ok)
            })
            .build(),
    );
    Ok(sink)
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

/// Actions and the Core to answer, in the order the operator gave them.
type Queued = (Action, cxx_qt::CxxQtThread<qobject::Core>);

static ACTIONS: std::sync::OnceLock<tokio::sync::mpsc::UnboundedSender<Queued>> =
    std::sync::OnceLock::new();

/// Actions go to lmix-core one after another: a held PTZ key's stop must
/// never overtake its move (two requests in parallel can arrive either
/// way round).
fn actions() -> &'static tokio::sync::mpsc::UnboundedSender<Queued> {
    ACTIONS.get_or_init(|| {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Queued>();
        config::runtime().spawn(async move {
            while let Some((action, thread)) = rx.recv().await {
                let refused = post_action(&action).await;
                if let Some(reason) = &refused {
                    tracing::info!(?action, %reason, "lmix-core refused");
                }
                let text = refused.unwrap_or_default();
                let json = serde_json::to_string(&action).unwrap_or_default();
                let _ = thread.queue(move |mut core| {
                    core.as_mut().set_message(QString::from(&text));
                    core.answered(QString::from(&json), QString::from(&text));
                });
            }
        });
        tx
    })
}

/// `{message}` for an error the Console found itself.
fn plain_message(text: &str) -> String {
    serde_json::json!({ "message": text }).to_string()
}

/// Run an action; when lmix-core refused it, the reason as JSON
/// (`{error, message}` from lmix-core, else `{message}`).
async fn post_action(action: &Action) -> Option<String> {
    let resp = match config::http()
        .post(format!("{}/api/actions", config::core_url()))
        .json(action)
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(e) => return Some(plain_message(&e.to_string())),
    };
    if resp.status().is_success() {
        return None;
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    match serde_json::from_str::<serde_json::Value>(&body) {
        Ok(v) if v["message"].is_string() => Some(body),
        _ => Some(plain_message(&format!("lmix-core {status}: {body}"))),
    }
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

/// `showLive`'s source for the tile picture.
const TILES: &str = "tiles";
/// How often a shown tile picture is asked for again: three times a
/// lease, so one or two lost asks do not darken it.
const TILES_RENEW: Duration = Duration::from_secs(TILES_LEASE.as_secs() / 3);

/// A live picture's socket, how its frames come, and for the tile picture
/// its grid (JSON for QML).
struct Feed {
    socket: String,
    transport: LiveTransport,
    grid: Option<String>,
}

/// Socket of a live picture on this machine.
async fn live_feed(source: &str) -> Result<Feed, String> {
    let path = live_path(source);
    let resp = config::http()
        .get(format!("{}{path}", config::core_url()))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("{path}: {}", resp.status()));
    }
    if source == TILES {
        let tiles: LiveTiles = resp.json().await.map_err(|e| e.to_string())?;
        let grid = serde_json::json!({
            "columns": tiles.columns,
            "rows": tiles.rows,
            "cell_width": tiles.cell_width,
            "cell_height": tiles.cell_height,
        });
        return Ok(Feed {
            socket: tiles.socket,
            transport: tiles.transport,
            grid: Some(grid.to_string()),
        });
    }
    let feed: LiveFeed = resp.json().await.map_err(|e| e.to_string())?;
    Ok(Feed {
        socket: feed.socket,
        transport: feed.transport,
        grid: None,
    })
}

/// The tile picture stays lit a lease after the last ask: a shown one is
/// asked for again every `TILES_RENEW` (the first ask was just now).
async fn renew_tiles() {
    let mut renew = tokio::time::interval(TILES_RENEW);
    renew.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    renew.tick().await;
    loop {
        renew.tick().await;
        if let Err(e) = live_feed(TILES).await {
            tracing::debug!(error = %e, "tile lease not renewed");
        }
    }
}

/// Stops the renewal of a tile picture when its watch ends.
struct Renewal(tokio::task::JoinHandle<()>);

impl Drop for Renewal {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// `program`, `preview` (the mixer's), or an input id (tile size).
fn live_path(source: &str) -> String {
    match source {
        "program" | "preview" | TILES => format!("/api/live/{source}"),
        _ => format!("/api/live/inputs/{source}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zones_are_levelled_by_name() {
        let levels = Levels {
            zones: vec![
                lmix_types::ZonePeak {
                    zone: "Hall".into(),
                    peak_db: Some(-9.0),
                },
                lmix_types::ZonePeak {
                    zone: "Foyer".into(),
                    peak_db: None,
                },
            ],
            ..Default::default()
        };
        let peaks: HashMap<String, f32> = peaks_of(&levels).collect();
        assert_eq!(peaks, HashMap::from([("zone:Hall".to_string(), -9.0)]));
    }

    #[test]
    fn live_path_of_program_and_inputs() {
        assert_eq!(live_path("program"), "/api/live/program");
        assert_eq!(live_path("7d0c"), "/api/live/inputs/7d0c");
        assert_eq!(live_path("preview"), "/api/live/preview");
        assert_eq!(live_path("tiles"), "/api/live/tiles");
    }

    #[test]
    fn peaks_by_the_keys_qml_asks_for() {
        let input = "00000000-0000-0000-0000-000000000001";
        let bus = "00000000-0000-0000-0000-000000000002";
        let levels: Levels = serde_json::from_value(serde_json::json!({
            "inputs": [{ "input": input, "peak_db": -12.0, "channels_db": [-12.0, -14.0] }],
            "program_peak_db": -6.0,
            "phones_peak_db": null,
            "buses": [{ "bus": bus, "peak_db": -20.0 }, { "bus": "00000000-0000-0000-0000-000000000003", "peak_db": null }]
        }))
        .unwrap();
        let peaks: HashMap<String, f32> = peaks_of(&levels).collect();
        assert_eq!(peaks.len(), 3);
        assert_eq!(peaks[input], -12.0);
        assert_eq!(peaks["program"], -6.0);
        assert_eq!(peaks[&format!("bus:{bus}")], -20.0);
    }
}
