//! The service: one switcher, one engine, many clients.
//!
//! ```text
//!   API/WS ─► Service::handle ─► Switcher (lock) ─► Engine
//!                    │
//!                    └─► broadcast Event::State / Event::Tally ─► subscribers
//! ```
//!
//! Actions run one after another (the switcher lock is held while the
//! engine works), so every client sees the same order.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

use lmix_types::state::TILES_LEASE;
use lmix_types::{
    Action, ActionError, Event, InputId, InputLevel, InputRef, InputStatus, IoClass, Latency,
    Levels, LiveFeed, LiveTiles, OutputId, OutputSpec, OutputStats, OutputStatus, ShowState,
    SignalLoss, Template,
};
use strom_types::FlowId;
use tokio::sync::{broadcast, Mutex, Notify};

use crate::engine::flows::{self, output_flow_id, slot_id};
use crate::engine::meters::{FlowError, FlowFailure};
use crate::engine::{meters, Engine, EngineError, LivePicture, Picture};
use crate::recent::Recent;
use crate::show::{self, Show};
use crate::switcher::{Command, Switcher};
use crate::{datasource, titles};

/// How often lmix-core asks the engine whether it restarted.
const ENGINE_CHECK: std::time::Duration = std::time::Duration::from_secs(2);
/// A flow stopping this soon after lmix-core started, rebuilt or stopped
/// it is its own doing, not a failure.
const OWN_STOP_WINDOW: std::time::Duration = std::time::Duration::from_secs(1);
/// Failback retries: first after 2 s, doubling up to 10 s.
const FAILBACK_FIRST: std::time::Duration = std::time::Duration::from_secs(2);
const FAILBACK_MAX: std::time::Duration = std::time::Duration::from_secs(10);
/// A failed output started again counts as running once its flow has run
/// this long: one on a busy card starts and fails a second later.
const OUTPUT_HOLD: std::time::Duration = std::time::Duration::from_secs(5);

/// Events kept for slow subscribers before they miss some.
const EVENT_BUFFER: usize = 256;
/// At most this often a `Levels` event (strom measures every 100 ms).
const LEVELS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
/// How often network outputs report their link health.
const OUTPUT_STATS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
/// How often the machine's load is reported (`Event::Machine`, RUN.8).
const MACHINE_INTERVAL: std::time::Duration = std::time::Duration::from_secs(2);
/// Silence: also stands in for "no reading" (JSON has no infinity).
pub const LEVEL_FLOOR_DB: f32 = -100.0;

pub struct Service {
    /// Itself, for work it hands to a task of its own (the failback of an
    /// output a command left failed).
    me: std::sync::Weak<Service>,
    switcher: Mutex<Switcher>,
    engine: Arc<dyn Engine>,
    events: broadcast::Sender<Event>,
    /// Wakes [`Service::watch_cues`] when cue times changed.
    cue_wake: tokio::sync::Notify,
    /// Client for output targets (each request has its target's limit).
    target_http: reqwest::Client,
    /// Written after every change, read back after a crash (SHW.5).
    autosave: Option<PathBuf>,
    /// Players the operator moved (jump, item, restart, previous), and
    /// when: their position reports right after are no clip end.
    jumped: std::sync::Mutex<HashMap<strom_types::FlowId, Instant>>,
    /// Each player's last report, for `playback` in the state (API.4).
    playback: std::sync::Mutex<HashMap<InputId, (lmix_types::Playback, Instant)>>,
    /// Settings file of the shortcuts for all shows (AUT.3).
    all_shows_file: std::sync::Mutex<Option<PathBuf>>,
    /// Settings file of the layout templates (LAY.5).
    layout_templates_file: std::sync::Mutex<Option<PathBuf>>,
    /// Settings file of the output targets (`--targets`).
    targets_file: std::sync::Mutex<Option<PathBuf>>,
    /// lmix-script for the show's script folder (SSH.2), once configured.
    script: Mutex<Option<crate::script::ScriptRunner>>,
    recent: Mutex<Recent>,
    /// Last good frame per picture: a paused clip or still keeps its
    /// picture while the engine's tap has no new frame.
    pictures: std::sync::Mutex<HashMap<Picture, Vec<u8>>>,
    /// When lmix-core last changed each flow: its own stops are no failure.
    touched: std::sync::Mutex<HashMap<FlowId, Touch>>,
    /// Slots with a failback running.
    failing_back: std::sync::Mutex<HashSet<FlowId>>,
    /// Latest link health of the running network outputs.
    output_stats: std::sync::Mutex<Vec<OutputStats>>,
    /// False until the engine is up and fed the show: changes before that
    /// only touch the show, and `restore` hands the engine all of them.
    engine_ready: AtomicBool,
    /// Engine process the show was last fed to (`watch_engine`).
    engine_instance: std::sync::Mutex<Option<String>>,
    /// A failed command found a new engine process: check now, not at the
    /// next poll.
    engine_changed: Notify,
    /// Until when the tile picture stays lit (`watch_tiles`); `None` once
    /// it went dark.
    tiles_lease: Mutex<Option<tokio::time::Instant>>,
    /// A Console asked for the tiles (ends `watch_tiles`' wait while dark).
    tiles_asked: Notify,
    /// The templates folder (`--templates`), if any.
    templates_dir: std::sync::Mutex<Option<PathBuf>>,
    /// Built-in and folder templates as last scanned.
    templates: std::sync::Mutex<Vec<Template>>,
    /// When each data source was read: its file's time, or when an
    /// address was fetched.
    source_times: std::sync::Mutex<HashMap<InputId, Seen>>,
    /// Data sources at an address being fetched for the first time.
    fetching: std::sync::Mutex<HashSet<InputId>>,
    /// Fetches data sources from addresses.
    http: reqwest::Client,
    /// Sends PTZ commands; opened at the first one.
    ptz_socket: tokio::sync::OnceCell<tokio::net::UdpSocket>,
    /// Numbers VISCA-over-IP packets.
    ptz_seq: std::sync::atomic::AtomicU32,
}

/// What lmix-core knew of a data source when it read it.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Seen {
    File(Option<SystemTime>),
    Url(Instant),
}

/// Data sources at an address are fetched again this often.
const URL_REFRESH: std::time::Duration = std::time::Duration::from_secs(10);

/// How often lmix-core looks at the templates folder and data sources.
const FILES_CHECK: std::time::Duration = std::time::Duration::from_secs(1);
/// How long after a jump in a player its reports are no clip end.
const JUMP_SETTLE: std::time::Duration = std::time::Duration::from_millis(600);
/// Players report five times a second while their flow runs, but a busy
/// engine leaves gaps of seconds (a VM check lost a held clip's state for
/// one read); a report this old belongs to a player that is gone.
const PLAYBACK_STALE: std::time::Duration = std::time::Duration::from_secs(10);

/// How often the inputs' media files are looked for (SHW.4).
const MEDIA_CHECK: std::time::Duration = std::time::Duration::from_secs(3);

/// How often it moves auto-advancing data sources on when due.
const AUTO_CHECK: std::time::Duration = std::time::Duration::from_millis(250);

impl Service {
    pub fn new(slots: usize, engine: Arc<dyn Engine>, autosave: Option<PathBuf>) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        let mut switcher = Switcher::new(slots);
        switcher.set_looks_available(engine.looks_available());
        let templates = titles::scan(None);
        switcher.set_templates(titles::catalog(&templates));
        Arc::new_cyclic(|me| Self {
            me: me.clone(),
            switcher: Mutex::new(switcher),
            engine,
            events,
            cue_wake: tokio::sync::Notify::new(),
            target_http: crate::datasource::client(),
            recent: Mutex::new(Recent::load(autosave.as_deref())),
            pictures: std::sync::Mutex::new(HashMap::new()),
            touched: std::sync::Mutex::new(HashMap::new()),
            failing_back: std::sync::Mutex::new(HashSet::new()),
            output_stats: std::sync::Mutex::new(Vec::new()),
            engine_ready: AtomicBool::new(true),
            engine_instance: std::sync::Mutex::new(None),
            engine_changed: Notify::new(),
            tiles_lease: Mutex::new(None),
            tiles_asked: Notify::new(),
            templates_dir: std::sync::Mutex::new(None),
            templates: std::sync::Mutex::new(templates),
            source_times: std::sync::Mutex::new(HashMap::new()),
            fetching: std::sync::Mutex::new(HashSet::new()),
            http: datasource::client(),
            ptz_socket: tokio::sync::OnceCell::new(),
            ptz_seq: std::sync::atomic::AtomicU32::new(0),
            autosave,
            jumped: std::sync::Mutex::new(HashMap::new()),
            playback: std::sync::Mutex::new(HashMap::new()),
            all_shows_file: std::sync::Mutex::new(None),
            layout_templates_file: std::sync::Mutex::new(None),
            targets_file: std::sync::Mutex::new(None),
            script: Mutex::new(None),
        })
    }

    /// Keep shortcuts for all shows in `path` (`--shortcuts`): read now,
    /// written after each change. A file that cannot be read is left alone
    /// (renamed aside if it is broken) and the list starts empty.
    pub async fn use_shortcuts_file(&self, path: PathBuf) {
        let read = read_settings(&path, "shortcuts for all shows", parse_shortcuts).await;
        let mut sw = self.switcher.lock().await;
        sw.set_all_shows_shortcuts(read.clone().ok().flatten().unwrap_or_default());
        if read.is_ok() {
            *self.all_shows_file.lock().expect("shortcuts file lock") = Some(path);
        }
        self.publish(&sw);
    }

    /// Keep layout templates (LAY.5) in `path` (`--layout-templates`), as
    /// the shortcuts for all shows.
    pub async fn use_layout_templates_file(&self, path: PathBuf) {
        let read = read_settings(&path, "layout templates", crate::layouts::parse).await;
        let mut sw = self.switcher.lock().await;
        sw.set_layout_templates(read.clone().ok().flatten().unwrap_or_default());
        if read.is_ok() {
            *self
                .layout_templates_file
                .lock()
                .expect("templates file lock") = Some(path);
        }
        self.publish(&sw);
    }

    /// Keep the output targets in `path` (`--targets`), as the shortcuts
    /// for all shows.
    pub async fn use_targets_file(&self, path: PathBuf) {
        let read = read_settings(&path, "output targets", crate::targets::parse).await;
        let mut sw = self.switcher.lock().await;
        sw.set_target_settings(read.clone().ok().flatten().unwrap_or_default());
        sw.refresh_warnings();
        if read.is_ok() {
            *self.targets_file.lock().expect("targets file lock") = Some(path);
        }
        self.publish(&sw);
    }

    /// Run `tool` for the show's script folder, telling it `core_url`
    /// (SSH.2); from now on after every change of the folder.
    pub async fn use_script_tool(&self, tool: PathBuf, core_url: String) {
        *self.script.lock().await = Some(crate::script::ScriptRunner::new(tool, core_url));
        self.sync_script().await;
    }

    /// The tool runs for the show's script folder, or not at all.
    async fn sync_script(&self) {
        let folder = self.switcher.lock().await.show().script_folder.clone();
        if let Some(runner) = self.script.lock().await.as_mut() {
            runner.sync(folder.as_deref()).await;
        }
    }

    /// The targets hold secrets: the file is the user's alone (EXT.2).
    async fn write_targets(&self, sw: &mut Switcher) {
        if !sw.take_targets_changed() {
            return;
        }
        let Some(path) = self.targets_file.lock().expect("targets file lock").clone() else {
            return;
        };
        let text = crate::targets::to_bytes(&sw.target_settings());
        if let Err(e) = write_private_settings(&path, text).await {
            tracing::warn!(error = %e, file = %path.display(), "output targets not saved");
            sw.targets_unsaved();
        }
    }

    /// Send `request` to `target` in the background; how it went is an
    /// event, the caller never waits (EXT.3).
    fn send_to_target(
        self: &Arc<Self>,
        target: crate::targets::TargetConfig,
        request: crate::targets::Request,
        out: Outbound,
    ) {
        if out == Outbound::Locked {
            let _ = self.events.send(Event::Target {
                target: target.name,
                result: lmix_types::TargetResult::Locked,
            });
            return;
        }
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let result = crate::targets::send(&service.target_http, &target, &request).await;
            let reached = match &result {
                lmix_types::TargetResult::Locked => return,
                lmix_types::TargetResult::Failed { reason } => {
                    tracing::warn!(target = %target.name, %reason, "output target not reached");
                    Err(reason.clone())
                }
                lmix_types::TargetResult::Ok => Ok(()),
            };
            service.reached(&target.name, reached).await;
            let _ = service.events.send(Event::Target {
                target: target.name.clone(),
                result,
            });
        });
    }

    /// Note how target `name` answered; the warnings follow (SUI.5).
    async fn reached(&self, name: &str, result: Result<(), String>) {
        let mut sw = self.switcher.lock().await;
        if sw.set_reached(name, result) {
            self.publish(&sw);
        }
    }

    /// Best effort, like the autosave.
    async fn write_all_shows(&self, sw: &mut Switcher) {
        if !sw.take_all_shows_changed() {
            return;
        }
        let Some(path) = self
            .all_shows_file
            .lock()
            .expect("shortcuts file lock")
            .clone()
        else {
            return;
        };
        let written = match serde_json::to_vec_pretty(sw.all_shows_shortcuts()) {
            Ok(text) => write_settings(&path, text).await,
            Err(e) => Err(e.into()),
        };
        if let Err(e) = written {
            tracing::warn!(error = %e, file = %path.display(), "shortcuts for all shows not saved");
            // Tried again with the next change.
            sw.all_shows_unsaved();
        }
    }

    /// The same for the layout templates.
    async fn write_layout_templates(&self, sw: &mut Switcher) {
        if !sw.take_layout_templates_changed() {
            return;
        }
        let Some(path) = self
            .layout_templates_file
            .lock()
            .expect("templates file lock")
            .clone()
        else {
            return;
        };
        let text = crate::layouts::to_bytes(sw.layout_templates());
        if let Err(e) = write_settings(&path, text).await {
            tracing::warn!(error = %e, file = %path.display(), "layout templates not saved");
            sw.layout_templates_unsaved();
        }
    }

    /// Take templates from `dir` as well (`--templates`).
    pub async fn use_templates(&self, dir: PathBuf) {
        *self.templates_dir.lock().expect("templates dir lock") = Some(dir);
        self.rescan_templates().await;
    }

    /// All rows of a data source.
    pub async fn data_table(
        &self,
        source: &InputRef,
    ) -> Result<lmix_types::DataTable, ActionError> {
        self.switcher.lock().await.data_table(source)
    }

    /// Built-in and folder templates (GFX.19).
    pub fn templates(&self) -> Vec<Template> {
        self.templates.lock().expect("templates lock").clone()
    }

    /// A file below `/titles/` (templates, SDK, Kante).
    pub fn title_file(&self, path: &str) -> Option<Vec<u8>> {
        let dir = self
            .templates_dir
            .lock()
            .expect("templates dir lock")
            .clone();
        titles::file(dir.as_deref(), path)
    }

    /// Scan the folder again; a changed template reaches the graphics
    /// pages through the state (GFX.14).
    async fn rescan_templates(&self) {
        let dir = self
            .templates_dir
            .lock()
            .expect("templates dir lock")
            .clone();
        let Ok(found) = tokio::task::spawn_blocking(move || titles::scan(dir.as_deref())).await
        else {
            return;
        };
        if *self.templates.lock().expect("templates lock") == found {
            return;
        }
        let mut sw = self.switcher.lock().await;
        sw.set_templates(titles::catalog(&found));
        *self.templates.lock().expect("templates lock") = found;
        self.publish(&sw);
    }

    /// Read the data sources never read (just added, or a show just
    /// opened). Text files at once (local, quick); addresses and
    /// spreadsheets (unpacking takes a while) in the background, so they
    /// never hold up switching: the switcher is locked while this runs.
    async fn read_new_sources(self: &Arc<Self>, sw: &mut Switcher) {
        for (id, spec) in sw.unread_sources() {
            let place = datasource::resolve(&spec, sw.path());
            if place.is_url() || matches!(spec, lmix_types::DataSourceSpec::Sheet { .. }) {
                if self.fetching.lock().expect("fetching lock").insert(id) {
                    tokio::spawn(self.clone().fetch_new_source(id, spec, place));
                }
                continue;
            }
            let seen = self.seen(&place).await;
            sw.set_rows(id, datasource::read(&spec, &place, &self.http).await);
            self.source_times
                .lock()
                .expect("source times lock")
                .insert(id, seen);
        }
    }

    /// A data source at an address or a spreadsheet, read without the
    /// switcher locked.
    async fn fetch_new_source(
        self: Arc<Self>,
        id: InputId,
        spec: lmix_types::DataSourceSpec,
        place: datasource::Place,
    ) {
        let seen = self.seen(&place).await;
        let rows = datasource::read(&spec, &place, &self.http).await;
        let mut sw = self.switcher.lock().await;
        // Gone or read meanwhile (the show changed): nothing to do.
        if sw.unread_sources().iter().any(|(s, _)| *s == id) {
            sw.set_rows(id, rows);
            self.source_times
                .lock()
                .expect("source times lock")
                .insert(id, seen);
            self.publish(&sw);
        }
        self.fetching.lock().expect("fetching lock").remove(&id);
    }

    async fn seen(&self, place: &datasource::Place) -> Seen {
        if place.is_url() {
            Seen::Url(Instant::now())
        } else {
            Seen::File(datasource::modified(place).await)
        }
    }

    /// Read again the files that changed and the addresses due (DAT.2).
    async fn reread_changed_sources(&self) {
        let (sources, show) = {
            let sw = self.switcher.lock().await;
            (sw.source_files(), sw.path().map(str::to_string))
        };
        let mut read = Vec::new();
        for (id, spec) in sources {
            let place = datasource::resolve(&spec, show.as_deref());
            let known = self
                .source_times
                .lock()
                .expect("source times lock")
                .get(&id)
                .copied();
            let now = self.seen(&place).await;
            let due = match (known, now) {
                (Some(Seen::File(then)), Seen::File(time)) => then != time,
                (Some(Seen::Url(then)), Seen::Url(_)) => then.elapsed() >= URL_REFRESH,
                // Not read yet (handled with the action) or changed kind.
                _ => false,
            };
            if due {
                read.push((id, datasource::read(&spec, &place, &self.http).await, now));
            }
        }
        if read.is_empty() {
            return;
        }
        let mut sw = self.switcher.lock().await;
        let mut changed = false;
        for (id, rows, seen) in read {
            if sw.rows(id) != Some(&rows) {
                sw.set_rows(id, rows);
                changed = true;
            }
            self.source_times
                .lock()
                .expect("source times lock")
                .insert(id, seen);
        }
        if changed {
            self.publish(&sw);
        }
    }

    /// Run what the cue lists have due, each at its time (RUN.8): sleep
    /// to the next deadline, or until a cue action changed them.
    pub async fn watch_cues(self: Arc<Self>) {
        loop {
            let deadline = self.switcher.lock().await.cue_deadline();
            let woken = self.cue_wake.notified();
            match deadline {
                Some(at) => {
                    tokio::select! {
                        _ = tokio::time::sleep_until(at) => {}
                        _ = woken => continue,
                    }
                }
                None => {
                    woken.await;
                    continue;
                }
            }
            let out = {
                let mut sw = self.switcher.lock().await;
                let out = sw.cue_tick(tokio::time::Instant::now());
                // A start cue may move another list's standby.
                if sw.sync_standby() {
                    self.write_autosave(&sw).await;
                }
                // Every due step changes what runs (a wait's pre wait
                // ending makes no out, but clients show the phase).
                self.publish(&sw);
                out
            };
            self.run_cue_outs(out).await;
            self.preload_standby().await;
        }
    }

    /// Have the engine hold the standby cues' sounds and pictures ready.
    /// After a GO's actions: the cue it started has used its preload.
    async fn preload_standby(&self) {
        let mut sw = self.switcher.lock().await;
        if let Some(command) = sw.preload_commands() {
            if let Err(e) = self.execute(&mut sw, &[command]).await {
                tracing::debug!(error = %e, "standby not preloaded");
            }
        }
    }

    /// End sound and picture cues as the engine reports theirs over;
    /// forever.
    pub async fn watch_playback(self: Arc<Self>) {
        let mut ended = self.engine.playback_ended();
        loop {
            let id = match ended.recv().await {
                Ok(id) => id,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            let out = {
                let mut sw = self.switcher.lock().await;
                let out = sw.media_ended(&id, tokio::time::Instant::now());
                if !out.is_empty() {
                    self.publish(&sw);
                }
                out
            };
            self.cue_wake.notify_one();
            self.run_cue_outs(out).await;
        }
    }

    /// How late cue steps ran (RUN.8).
    pub async fn cue_lateness(&self) -> crate::cues::Lateness {
        self.switcher.lock().await.cue_lateness()
    }

    /// Running cues as an event and the fixed OSC subscribers (CTL.2,
    /// CTL.4), `hz` times a second while there is something to say.
    pub async fn watch_feeds(self: Arc<Self>, hz: u32) {
        let socket = match tokio::net::UdpSocket::bind(("0.0.0.0", 0)).await {
            Ok(socket) => socket,
            Err(e) => {
                tracing::warn!(error = %e, "no socket for OSC subscribers");
                return;
            }
        };
        let mut events = self.subscribe();
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(
            1000 / u64::from(hz.max(1)),
        ));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut tracker = crate::feeds::Tracker::default();
        let mut was_running = false;
        loop {
            let sends = tokio::select! {
                _ = tick.tick() => {
                    let state = {
                        let sw = self.switcher.lock().await;
                        let running = sw.cue_deadline().is_some();
                        if crate::feeds::idle(sw.feeds(), running || was_running, &tracker) {
                            continue;
                        }
                        sw.state()
                    };
                    let running = !state.running_cues.is_empty();
                    if running || was_running {
                        let _ = self.events.send(Event::Running {
                            cues: state.running_cues.clone(),
                        });
                    }
                    was_running = running;
                    tracker.tick(&state)
                }
                event = events.recv() => match event {
                    Ok(Event::Panic { mode }) => {
                        crate::feeds::panic(self.switcher.lock().await.feeds(), mode)
                    }
                    Ok(Event::State { .. }) => continue,
                    Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => return,
                },
            };
            self.send_feeds(&socket, sends).await;
        }
    }

    /// OSC packets to the addresses of `osc_udp` targets.
    async fn send_feeds(&self, socket: &tokio::net::UdpSocket, sends: Vec<crate::feeds::Send>) {
        if sends.is_empty() {
            return;
        }
        let targets = self.switcher.lock().await.target_settings().targets;
        for (name, packet) in sends {
            let Some(target) = targets.iter().find(|t| t.name == name) else {
                tracing::debug!(target = %name, "OSC subscriber names no target");
                continue;
            };
            if target.kind != lmix_types::TargetKind::OscUdp {
                tracing::debug!(target = %name, "OSC subscribers send over osc_udp only");
                continue;
            }
            for address in &target.addresses {
                if let Err(e) = socket.send_to(&packet, address.as_str()).await {
                    tracing::debug!(error = %e, %address, "OSC subscriber not reached");
                }
            }
        }
    }

    /// Run cue actions and send cue events. A failing action stops the
    /// rest of its cue and is reported; times and follows go on (CUE.19).
    async fn run_cue_outs(self: &Arc<Self>, out: Vec<crate::cues::Out>) {
        for o in out {
            match o {
                crate::cues::Out::Event { list, cue, what } => {
                    let _ = self.events.send(Event::Cue { list, cue, what });
                }
                crate::cues::Out::StopPlayback { fade_ms } => {
                    let stops = [
                        Action::StopSound { id: None, fade_ms },
                        Action::StopPicture { id: None, fade_ms },
                    ];
                    for stop in stops {
                        if let Err(e) = Box::pin(self.handle_as(stop, Origin::Operator)).await {
                            tracing::warn!(error = %e, "sounds or pictures did not stop");
                        }
                    }
                }
                crate::cues::Out::Panic { mode } => {
                    tracing::warn!(?mode, "panic");
                    let _ = self.events.send(Event::Panic { mode });
                }
                crate::cues::Out::Actions { list, cue, actions } => {
                    for action in actions {
                        let Err(e) = Box::pin(self.handle_as(action, Origin::Operator)).await
                        else {
                            continue;
                        };
                        tracing::warn!(error = %e, %cue, "cue action failed; the rest of the cue is skipped");
                        let what = lmix_types::CueEvent::Failed {
                            reason: e.to_string(),
                        };
                        let _ = self.events.send(Event::Cue {
                            list,
                            cue: Some(cue),
                            what,
                        });
                        break;
                    }
                }
            }
        }
    }

    /// Follow the templates folder and data source files, and move
    /// auto-advancing data sources on (DAT.5).
    pub async fn watch_titles(self: Arc<Self>) {
        // Files and addresses on their own: a slow download must not hold
        // up the rows moving on.
        let files = {
            let service = self.clone();
            async move {
                let mut tick = tokio::time::interval(FILES_CHECK);
                loop {
                    tick.tick().await;
                    service.rescan_templates().await;
                    service.reread_changed_sources().await;
                }
            }
        };
        let rows = async move {
            let mut tick = tokio::time::interval(AUTO_CHECK);
            loop {
                tick.tick().await;
                let due = self.switcher.lock().await.due_sources(Instant::now());
                for id in due {
                    let next = Action::DataNext {
                        source: InputRef::Text(id.to_string()),
                    };
                    // At the last row without looping it just stays there.
                    let _ = self.handle_as(next, Origin::Trigger).await;
                }
            }
        };
        tokio::join!(files, rows);
    }

    /// The engine is not there yet: keep changes in the show until
    /// `restore` runs (start-up, where clients may act before the engine
    /// answers).
    pub fn await_engine(&self) {
        self.engine_ready.store(false, Ordering::SeqCst);
    }

    /// Reopen the autosave of the last run, if any. The show keeps no file
    /// path, so a later save asks for one.
    pub async fn resume(self: &Arc<Self>) -> Result<bool, ActionError> {
        let Some(path) = &self.autosave else {
            return Ok(false);
        };
        if !path.exists() {
            return Ok(false);
        }

        let show = show::load(path).await.map_err(|e| ActionError::ShowFile {
            message: e.to_string(),
        })?;
        let mut sw = self.switcher.lock().await;
        let commands = sw.replace(show, None)?;
        let result = self.execute(&mut sw, &commands).await;
        self.read_new_sources(&mut sw).await;
        self.publish(&sw);
        result.map(|()| true)
    }

    /// Engine address as WHIP senders reach it (see `--public-url`).
    pub async fn set_ingest_base(&self, url: String) {
        self.switcher.lock().await.set_ingest_base(url);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub async fn state(&self) -> ShowState {
        let mut state = self.switcher.lock().await.state();
        let mut playback = self.playback.lock().expect("playback lock");
        playback.retain(|_, (_, at)| at.elapsed() < PLAYBACK_STALE);
        for input in &mut state.inputs {
            input.playback = playback.get(&input.id).map(|(p, _)| *p);
        }
        state
    }

    /// Latest frame of an input; the switcher lock is not held while the
    /// engine fetches it.
    pub async fn input_picture(
        &self,
        input: &InputRef,
    ) -> Result<Result<Vec<u8>, EngineError>, ActionError> {
        let (slot, loss) = {
            let sw = self.switcher.lock().await;
            let slot = sw.slot_for(input)?;
            (slot, sw.signal_loss_of(slot))
        };
        Ok(self.picture(Picture::Slot(slot), loss).await)
    }

    /// Live picture socket of an input's slot at tile size (see
    /// [`Engine::live`]).
    pub async fn live_input(
        &self,
        input: &InputRef,
    ) -> Result<Result<LiveFeed, EngineError>, ActionError> {
        let slot = self.switcher.lock().await.slot_for(input)?;
        Ok(self.live_feed(LivePicture::Slot(slot)).await)
    }

    pub async fn live_program(&self) -> Result<LiveFeed, EngineError> {
        self.live_feed(LivePicture::Program).await
    }

    /// The mixer's preview: an input, a layout, whatever a take brings.
    pub async fn live_preview(&self) -> Result<LiveFeed, EngineError> {
        self.live_feed(LivePicture::Preview).await
    }

    /// Every slot in one picture, and where each one sits in it.
    pub async fn live_tiles(&self) -> Result<LiveTiles, EngineError> {
        // Every ask is a lease (see `watch_tiles`), held while it lights.
        let mut lease = self.tiles_lease.lock().await;
        *lease = Some(tokio::time::Instant::now() + TILES_LEASE);
        self.tiles_asked.notify_one();
        let feed = self.live_feed(LivePicture::Tiles).await?;
        drop(lease);
        let (columns, rows) = flows::tiles_grid(self.switcher.lock().await.slots());
        let (cell_width, cell_height) = flows::TILE_CELL;
        Ok(LiveTiles {
            socket: feed.socket,
            transport: feed.transport,
            columns: columns as u32,
            rows: rows as u32,
            cell_width,
            cell_height,
        })
    }

    /// Tiles nobody asked for in a lease go dark (`Engine::dim_tiles`):
    /// with them strom's CPU mixer needs 480 % of a core instead of 300 %
    /// (30.10.2026). It sleeps until the lease runs out, never polls. It
    /// begins with one lease: a takeover keeps tiles lit that a Console of
    /// the lmix-core before reads, and that Console asks this one next.
    pub async fn watch_tiles(self: Arc<Self>) {
        *self.tiles_lease.lock().await = Some(tokio::time::Instant::now() + TILES_LEASE);
        loop {
            let mut lease = self.tiles_lease.lock().await;
            match *lease {
                None => {
                    drop(lease);
                    self.tiles_asked.notified().await;
                }
                Some(until) if tokio::time::Instant::now() < until => {
                    drop(lease);
                    tokio::time::sleep_until(until).await;
                }
                // Held while the engine darkens: an ask waits, then lights.
                // Still lit (strom refused): again a lease later, no polling.
                Some(_) => {
                    let dark = self.engine.dim_tiles().await;
                    *lease = (!dark).then(|| tokio::time::Instant::now() + TILES_LEASE);
                }
            }
        }
    }

    async fn live_feed(&self, picture: LivePicture) -> Result<LiveFeed, EngineError> {
        let socket = self.engine.live(picture).await?;
        Ok(LiveFeed {
            socket: socket.to_string_lossy().into_owned(),
            transport: self.engine.live_transport(picture),
        })
    }

    pub async fn program_picture(&self) -> Result<Vec<u8>, EngineError> {
        // Program never stalls; holding only bridges the tap's gaps.
        self.picture(Picture::Program, SignalLoss::Hold).await
    }

    /// Fresh frame; while the engine has none, the last one (`Hold`) or
    /// nothing (`Black`), as the mixer shows it.
    async fn picture(&self, which: Picture, loss: SignalLoss) -> Result<Vec<u8>, EngineError> {
        let fresh = self.engine.picture(which).await;
        let mut last = self.pictures.lock().expect("picture cache lock");
        match fresh {
            Ok(jpeg) => {
                last.insert(which, jpeg.clone());
                Ok(jpeg)
            }
            Err(EngineError::NotReady) if loss == SignalLoss::Hold => {
                last.get(&which).cloned().ok_or(EngineError::NotReady)
            }
            Err(e) => Err(e),
        }
    }

    /// Current levels per input (by slot) and of program.
    pub async fn levels(&self) -> Levels {
        let engine = self.engine.levels().borrow().clone();
        let fresh = |(db, at): (f32, std::time::Instant)| {
            (at.elapsed() < meters::STALE).then_some(db.max(LEVEL_FLOOR_DB))
        };
        let sw = self.switcher.lock().await;
        let inputs = sw
            .show()
            .inputs
            .iter()
            .filter(|i| i.source.has_audio())
            .map(|i| {
                let peak_db = engine.slots.get(&i.slot).copied().and_then(fresh);
                // Channels only while the reading is fresh, like the peak.
                let channels_db = match (peak_db, engine.slot_channels.get(&i.slot)) {
                    (Some(_), Some(channels)) => {
                        channels.iter().map(|db| db.max(LEVEL_FLOOR_DB)).collect()
                    }
                    _ => Vec::new(),
                };
                InputLevel {
                    input: i.id,
                    peak_db: peak_db.unwrap_or(LEVEL_FLOOR_DB),
                    channels_db,
                }
            })
            .collect();
        // The headphones are metered as the monitor (their mix or the
        // solos).
        let buses = sw
            .show()
            .buses
            .iter()
            .map(|bus| lmix_types::BusPeak {
                bus: bus.id,
                peak_db: engine.buses.get(&bus.id).copied().and_then(fresh),
            })
            .collect();
        let zones = sw
            .show()
            .zones
            .iter()
            .enumerate()
            .map(|(i, zone)| lmix_types::ZonePeak {
                zone: zone.name.clone(),
                peak_db: engine.zones.get(&i).copied().and_then(fresh),
            })
            .collect();
        Levels {
            inputs,
            program_peak_db: engine.main.and_then(fresh),
            phones_peak_db: engine.monitor.and_then(fresh),
            buses,
            zones,
        }
    }

    /// Notice when the engine process changes (crash and restart) and feed
    /// the new one the whole show, running outputs included. Runs for the
    /// process life once the engine was first fed.
    pub async fn watch_engine(self: Arc<Self>, slots: usize) {
        let first = self.engine.instance().await;
        self.set_instance(first);
        loop {
            tokio::select! {
                _ = tokio::time::sleep(ENGINE_CHECK) => {}
                _ = self.engine_changed.notified() => {}
            }
            let Some(current) = self.engine.instance().await else {
                continue;
            };
            if !self.is_new_instance(&current) {
                continue;
            }
            tracing::warn!("engine restarted, feeding it the show again");
            // Changes until the restore only touch the show.
            self.await_engine();
            // A command that holds the switcher already still talks to the
            // engine; the start waits for it, so neither overwrites what
            // the other recorded (the mixer's buses).
            drop(self.switcher.lock().await);
            while let Err(e) = self.engine.start(slots).await {
                tracing::warn!(error = %e, "engine not ready, retrying");
                tokio::time::sleep(ENGINE_CHECK).await;
            }
            if let Err(e) = self.restore().await {
                tracing::error!(error = %e, "restoring show to the restarted engine failed");
            }
            self.set_instance(Some(current));
        }
    }

    fn set_instance(&self, instance: Option<String>) {
        *self.engine_instance.lock().expect("engine instance lock") = instance;
    }

    /// `current` differs from the engine the show was fed to; the first
    /// instance seen counts as known.
    fn is_new_instance(&self, current: &str) -> bool {
        let mut known = self.engine_instance.lock().expect("engine instance lock");
        match known.as_deref() {
            Some(known) => known != current,
            None => {
                *known = Some(current.to_string());
                false
            }
        }
    }

    /// After a failed command: did the engine restart under us? Then the
    /// show waits for the restore like at start-up (instead of an error in
    /// the up to `ENGINE_CHECK` before the watcher notices).
    async fn engine_restarted(&self) -> bool {
        let Some(current) = self.engine.instance().await else {
            return false;
        };
        let known = self.engine_instance.lock().expect("engine instance lock");
        known.as_deref().is_some_and(|known| known != current)
    }

    /// Cards the engine still uses after lmix-core restarted stay on: a
    /// restart in a show keeps the card outputs and inputs going. Before
    /// `restore`.
    pub async fn adopt_cards(&self) {
        if self.engine.holds_cards().await {
            tracing::info!("the engine uses the cards: they stay on");
            let mut sw = self.switcher.lock().await;
            sw.adopt_cards();
            self.publish(&sw);
        }
    }

    /// Outputs still running in the engine after lmix-core restarted count
    /// as running again (their start time is unknown: now).
    pub async fn adopt_running_outputs(&self) {
        let ids = self.switcher.lock().await.output_ids();
        let mut running = Vec::new();
        for id in ids {
            if self.engine.output_running(id).await {
                running.push(id);
            }
        }
        if running.is_empty() {
            return;
        }
        let mut sw = self.switcher.lock().await;
        for id in running {
            sw.set_output_status(id, OutputStatus::Running { since_ms: now_ms() });
        }
        // A card output on air keeps the cards on, also when the engine
        // could not say it uses them (`adopt_cards`).
        // Switched on, the card inputs the restore showed black get their
        // cards again.
        if sw.runs_a_card_output() {
            let on = sw.set_cards(true);
            if let Err(e) = self.execute(&mut sw, &on).await {
                tracing::warn!("cards not taken back: {e}");
            }
        }
        self.publish(&sw);
    }

    /// Where missing media files are in `folder`, by file name: old path
    /// to new. Looked up without the mixer locked, each with a time limit.
    async fn find_moved_media(&self, folder: &str) -> HashMap<String, String> {
        let missing = self.switcher.lock().await.missing_media();
        let mut found = HashMap::new();
        for path in missing.into_iter().flat_map(|(_, files)| files) {
            let Some(name) = std::path::Path::new(&path).file_name() else {
                continue;
            };
            let candidate = std::path::Path::new(folder).join(name);
            let is_file = tokio::time::timeout(
                std::time::Duration::from_secs(2),
                tokio::fs::metadata(&candidate),
            )
            .await
            .is_ok_and(|m| m.is_ok_and(|m| m.is_file()));
            if is_file {
                found.insert(path, candidate.display().to_string());
            }
        }
        found
    }

    /// Look for the inputs' media files every few seconds (SHW.4): the
    /// state lists the missing ones per input. Checked without the
    /// switcher locked.
    pub async fn watch_media(self: Arc<Self>) {
        let mut tick = tokio::time::interval(MEDIA_CHECK);
        loop {
            tick.tick().await;
            self.check_media().await;
        }
    }

    async fn check_media(&self) {
        let files = self.switcher.lock().await.media_files();
        let mut missing = HashMap::new();
        for (id, paths) in files {
            let mut gone = Vec::new();
            for path in paths {
                if !tokio::fs::try_exists(&path).await.unwrap_or(false) {
                    gone.push(path);
                }
            }
            missing.insert(id, gone);
        }
        let mut sw = self.switcher.lock().await;
        if sw.set_missing(missing) {
            sw.refresh_warnings();
            self.publish(&sw);
        }
    }

    /// Follow engine failures for the process life: failed outputs are
    /// marked, failed inputs go offline and come back by themselves
    /// (failback) once their source returns.
    pub async fn watch_failures(self: Arc<Self>) {
        let mut errors = self.engine.flow_errors();
        loop {
            let error = match errors.recv().await {
                Ok(error) => error,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            if error.kind == FlowFailure::Stopped && self.own_stop(error.flow).await {
                continue;
            }
            if self.output_failed(&error).await {
                continue;
            }
            self.slot_failed(&error).await;
        }
    }

    /// Whether a stop of `flow` is lmix-core's own: during a command on it,
    /// or just after one. After its own stop or emptying that stop is it;
    /// after building it, the old flow's, unless the flow does not run now
    /// (stopped behind lmix-core's back right after it).
    async fn own_stop(&self, flow: FlowId) -> bool {
        let touch = self.touch_of(flow);
        match touch {
            // The old flow drains while the command builds anew.
            Some(Touch::Running) => true,
            Some(done @ Touch::Done { builds, .. }) if done.is_recent() => {
                // A command that came meanwhile rebuilds the flow (strom
                // answers 404 in between): the stop is its old flow's.
                !builds || self.engine.flow_running(flow).await || self.touch_of(flow) != touch
            }
            _ => false,
        }
    }

    fn touch_of(&self, flow: FlowId) -> Option<Touch> {
        self.touched
            .lock()
            .expect("touched lock")
            .get(&flow)
            .copied()
    }

    /// A running output that fails or is stopped behind lmix-core's back is
    /// marked failed and restarted (failback).
    async fn output_failed(self: &Arc<Self>, error: &FlowError) -> bool {
        let mut sw = self.switcher.lock().await;
        let failed = sw
            .output_ids()
            .into_iter()
            .find(|id| output_flow_id(*id) == error.flow);
        let Some(id) = failed else {
            return false;
        };
        if !sw.output_is_running(id) {
            return true;
        }
        tracing::warn!(output = %id, message = %error.message, "output failed, restarting");
        sw.set_output_status(
            id,
            OutputStatus::Failed {
                message: output_failure(&error.message),
            },
        );
        self.fail_back_output(&mut sw, id);
        self.publish(&sw);
        true
    }

    /// Start failed output `id` again until it runs (`restart_output`),
    /// unless that is going on already. Marked in the switcher under its
    /// lock: from now on its settings are locked (`update_output`).
    fn fail_back_output(&self, sw: &mut Switcher, id: OutputId) {
        let Some(me) = self.me.upgrade() else {
            return;
        };
        if self
            .failing_back
            .lock()
            .expect("failback lock")
            .insert(output_flow_id(id))
        {
            sw.begin_failback(id);
            tokio::spawn(me.restart_output(id));
        }
    }

    /// Start a failed output again until it runs, or the operator stopped
    /// or removed it. It stays failed until a start holds (`OUTPUT_HOLD`).
    async fn restart_output(self: Arc<Self>, id: OutputId) {
        let mut wait = FAILBACK_FIRST;
        loop {
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(FAILBACK_MAX);
            let mut sw = self.switcher.lock().await;
            let Some(command) = sw.restart_command(id) else {
                self.end_failback(&mut sw, id);
                break;
            };
            let failure = sw.output_failure(id);
            if self.execute(&mut sw, &[command]).await.is_err() {
                continue;
            }
            // A card output waits for the cards instead.
            if sw.output_waits_for_cards(id) {
                self.end_failback(&mut sw, id);
                break;
            }

            // Still failed while the start may not hold.
            let since_ms = now_ms();
            sw.hold_output(id, failure);
            drop(sw);
            tokio::time::sleep(OUTPUT_HOLD).await;
            // Asked under the lock: a failure meanwhile waits for it and
            // starts a failback of its own.
            let mut sw = self.switcher.lock().await;
            let running = self.engine.output_running(id).await;

            // Stopped or removed meanwhile, or back; else again, later.
            let done = match sw.restart_command(id) {
                None => true,
                Some(_) if running => {
                    tracing::info!(output = %id, "output back");
                    sw.set_output_status(id, OutputStatus::Running { since_ms });
                    self.publish(&sw);
                    true
                }
                // Not held: failed again.
                Some(_) => {
                    let failure = sw.output_failure(id);
                    sw.set_output_status(id, OutputStatus::Failed { message: failure });
                    false
                }
            };
            if done {
                self.end_failback(&mut sw, id);
                break;
            }
        }
    }

    /// The failback of output `id` is over, its hold too. Called under
    /// the switcher's lock: a failure after it starts a failback of its own.
    fn end_failback(&self, sw: &mut Switcher, id: OutputId) {
        sw.end_failback(id);
        self.failing_back
            .lock()
            .expect("failback lock")
            .remove(&output_flow_id(id));
    }

    async fn slot_failed(self: &Arc<Self>, error: &FlowError) {
        let mut sw = self.switcher.lock().await;
        let Some(slot) = (0..sw.slots()).find(|s| slot_id(*s) == error.flow) else {
            return;
        };
        let Some(input) = sw
            .show()
            .inputs
            .iter()
            .find(|i| i.slot == slot)
            .map(|i| i.id)
        else {
            return;
        };
        tracing::warn!(slot, message = %error.message, "input failed, failing back");
        sw.set_status(input, InputStatus::Offline);
        self.publish(&sw);
        drop(sw);

        if self
            .failing_back
            .lock()
            .expect("failback lock")
            .insert(error.flow)
        {
            tokio::spawn(self.clone().fail_back(slot));
        }
    }

    /// Rebuild a failed slot until it runs again or its input is gone.
    async fn fail_back(self: Arc<Self>, slot: usize) {
        let mut wait = FAILBACK_FIRST;
        loop {
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(FAILBACK_MAX);
            let mut sw = self.switcher.lock().await;
            let Some(input) = sw.show().inputs.iter().find(|i| i.slot == slot).cloned() else {
                break;
            };
            let command = Command::SetSlot {
                slot,
                source: Some(input.source.clone()),
            };
            if self.execute(&mut sw, &[command]).await.is_ok() {
                tracing::info!(slot, "input back");
                self.publish(&sw);
                break;
            }
        }
        self.failing_back
            .lock()
            .expect("failback lock")
            .remove(&slot_id(slot));
    }

    /// Publish levels while the engine measures; runs for the process life.
    pub async fn forward_levels(self: Arc<Self>) {
        let mut readings = self.engine.levels();
        while readings.changed().await.is_ok() {
            let levels = self.levels().await;
            let _ = self.events.send(Event::Levels { levels });
            tokio::time::sleep(LEVELS_INTERVAL).await;
        }
    }

    /// Publish the link health of running SRT outputs once a second;
    /// runs for the process life.
    pub async fn forward_output_stats(self: Arc<Self>) {
        loop {
            tokio::time::sleep(OUTPUT_STATS_INTERVAL).await;
            let running: Vec<OutputId> = {
                let sw = self.switcher.lock().await;
                sw.show()
                    .outputs
                    .iter()
                    .filter(|o| matches!(o.spec, OutputSpec::Srt { .. }))
                    .filter(|o| sw.output_is_running(o.id))
                    .map(|o| o.id)
                    .collect()
            };
            let mut stats = Vec::with_capacity(running.len());
            for id in running {
                stats.extend(self.engine.output_stats(id).await);
            }
            let unchanged = {
                let mut last = self.output_stats.lock().expect("output stats lock");
                let unchanged = stats.is_empty() && last.is_empty();
                *last = stats.clone();
                unchanged
            };
            if !unchanged {
                let _ = self.events.send(Event::Outputs { stats });
            }
        }
    }

    /// Report the machine's load and how the cue timer runs about every
    /// two seconds (RUN.8), so the Console warns when cue times may not
    /// hold. Without `/proc` (not Linux) nothing is reported; a read that
    /// fails later (no file handle left) skips that report.
    pub async fn watch_machine(self: Arc<Self>) {
        let mut sampler = crate::machine::Sampler::default();
        let mut tick = tokio::time::interval(MACHINE_INTERVAL);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut reported = false;
        let mut failing = false;
        loop {
            tick.tick().await;
            let (Ok(stat), Ok(loadavg)) = (
                tokio::fs::read_to_string("/proc/stat").await,
                tokio::fs::read_to_string("/proc/loadavg").await,
            ) else {
                if !reported {
                    tracing::warn!(
                        "no /proc/stat or /proc/loadavg: the machine's load is not reported"
                    );
                    return;
                }
                if !failing {
                    failing = true;
                    tracing::warn!("reading the machine's load failed; trying again");
                }
                continue;
            };
            reported = true;
            failing = false;
            let over_budget = self.cue_lateness().await.over_budget;
            let load = sampler.sample(
                &stat,
                &loadavg,
                over_budget,
                crate::realtime::priority(),
                std::time::Instant::now(),
            );
            let _ = self.events.send(Event::Machine { load });
        }
    }

    /// Latest link health of the running network outputs.
    pub fn output_stats(&self) -> Vec<OutputStats> {
        self.output_stats.lock().expect("output stats lock").clone()
    }

    pub async fn measure_latency(&self, seconds: u32) -> Result<(), EngineError> {
        self.engine.measure_latency(seconds).await
    }

    /// A message from a MIDI controller: pass it on to clients, then run
    /// what it is bound to (`pressed`: a note on, or a controller crossing
    /// to pressed).
    pub async fn midi(
        self: &Arc<Self>,
        message: lmix_types::MidiMessage,
        pressed: bool,
    ) -> Result<(), ActionError> {
        let _ = self.events.send(Event::Midi { midi: message });
        let hold = crate::cues::Hold::of(&format!(
            "midi {} {:?} {}",
            message.channel, message.kind, message.number
        ));
        if !pressed {
            self.release(hold).await;
        }
        let action = {
            let sw = self.switcher.lock().await;
            crate::midi::action_for(
                &message,
                pressed,
                sw.midi_shortcut(&message),
                sw.midi_moves_tbar(&message),
            )
        };
        match action {
            Some(action) => self.go_held(action, hold, IoClass::Midi).await.map(|_| ()),
            None => Ok(()),
        }
    }

    /// GO from a button whose release is reported (`held`, RUN.6): OSC.
    pub async fn go_held(
        self: &Arc<Self>,
        action: Action,
        held: crate::cues::Hold,
        class: IoClass,
    ) -> Result<ShowState, ActionError> {
        self.check_way_in(&action, class).await?;
        self.handle_as(action, Origin::Held(held)).await
    }

    /// Button `held` was let go: it may give GO again (RUN.6).
    pub async fn release(&self, held: crate::cues::Hold) {
        self.switcher.lock().await.cue_release(held);
    }

    /// Fire `clip_end` triggers and note clip lengths as the media players
    /// report their position; forever.
    pub async fn watch_players(self: Arc<Self>) {
        let mut reports = self.engine.players();
        let mut watch = crate::clips::ClipWatch::default();
        loop {
            let report = match reports.recv().await {
                Ok(report) => report,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(broadcast::error::RecvError::Closed) => return,
            };
            // Reports shortly after a jump may still show the old place.
            let jumped = self
                .jumped
                .lock()
                .expect("jumped lock")
                .get(&report.flow)
                .is_some_and(|at| at.elapsed() < JUMP_SETTLE);
            if jumped {
                watch.forget(report.flow);
            }
            let change = watch.update(report);
            let fired = {
                let mut sw = self.switcher.lock().await;
                let Some(slot) = (0..sw.slots())
                    .find(|slot| crate::engine::flows::slot_id(*slot) == report.flow)
                else {
                    continue;
                };
                if report.duration_ms > 0 && sw.set_clip_length(slot, report.duration_ms) {
                    self.publish(&sw);
                }
                // For the transport bars; no state change, no autosave.
                if let Some(input) = sw.input_at_slot(slot) {
                    self.playback.lock().expect("playback lock").insert(
                        input,
                        (
                            lmix_types::Playback {
                                state: report.state,
                                position_ms: report.position_ms,
                                duration_ms: report.duration_ms,
                                item: report.index + 1,
                                items: report.items,
                            },
                            Instant::now(),
                        ),
                    );
                    let _ = self.events.send(Event::Position {
                        input,
                        position_ms: report.position_ms,
                        duration_ms: report.duration_ms,
                    });
                }
                sw.clip_triggers(slot, &change)
            };
            for (delay_ms, actions) in fired {
                self.run_triggered(delay_ms, actions);
            }
        }
    }

    /// Free switching when the engine reports a stinger's end; forever.
    /// Until then the switcher refuses takes (strom would too).
    pub async fn watch_stingers(self: Arc<Self>) {
        let mut ends = self.engine.stingers();
        loop {
            match ends.recv().await {
                Ok(end) => {
                    tracing::debug!(?end, "stinger ended");
                    self.switcher.lock().await.stinger_ended(end.generation);
                }
                // A missed end may be an older take's; the current take's
                // limit frees the lock if it was its own.
                Err(broadcast::error::RecvError::Lagged(missed)) => {
                    tracing::warn!(missed, "stinger ends missed");
                }
                Err(broadcast::error::RecvError::Closed) => return,
            }
        }
    }

    /// What the engine reports about its machine and flows.
    pub fn engine_health(&self) -> crate::engine::meters::EngineHealth {
        self.engine.health()
    }

    /// Capture-to-program latency from the latest measurement.
    pub fn latency(&self) -> Latency {
        Latency::of(&self.engine.levels().borrow().program_ages_ms)
    }

    pub async fn recent(&self) -> Vec<String> {
        self.recent.lock().await.paths().to_vec()
    }

    pub async fn devices(&self) -> Result<Vec<lmix_types::Device>, EngineError> {
        self.engine.devices().await
    }

    pub async fn tally(&self) -> Vec<u8> {
        self.switcher.lock().await.tally()
    }

    /// [`Service::handle`] for an action that came in by `class` (RUN.9):
    /// refused while that way is locked, a panic excepted.
    pub async fn handle_from(
        self: &Arc<Self>,
        action: Action,
        class: IoClass,
    ) -> Result<ShowState, ActionError> {
        self.check_way_in(&action, class).await?;
        self.handle(action).await
    }

    /// Refuse `action` if it came in by a locked `class`.
    async fn check_way_in(&self, action: &Action, class: IoClass) -> Result<(), ActionError> {
        let is_panic = |a: &Action| matches!(a, Action::CuePanic { .. } | Action::CueHardStop);
        let sw = self.switcher.lock().await;
        if !sw.io_locked(class) || is_panic(action) {
            return Ok(());
        }
        // A pad or button that only stops the show is a panic too (MIDI
        // has no other way to one).
        if let Action::RunShortcut { target } = action {
            if sw
                .shortcut_actions(target)
                .is_ok_and(|actions| !actions.is_empty() && actions.iter().all(is_panic))
            {
                return Ok(());
            }
        }
        Err(ActionError::IoLocked { class })
    }

    /// Target `name`, and whether a request to it goes out.
    async fn target_and_out(
        &self,
        name: &str,
    ) -> Result<(crate::targets::TargetConfig, Outbound), ActionError> {
        let sw = self.switcher.lock().await;
        let out = if sw.io_locked(IoClass::Targets) {
            Outbound::Locked
        } else {
            Outbound::Open
        };
        Ok((sw.target(name)?, out))
    }

    /// Run one action; publishes the new state. Commands after a
    /// `Command::Wait` (a stinger's cut and end, a clip's automatic pause
    /// after a fade) run later in the background.
    pub async fn handle(self: &Arc<Self>, action: Action) -> Result<ShowState, ActionError> {
        // These may give the show another script folder (SSH.2).
        let folder = matches!(
            action,
            Action::SetScriptFolder { .. }
                | Action::OpenShow { .. }
                | Action::NewShow { .. }
                | Action::ImportPackage { .. }
        );
        let result = self.handle_as(action, Origin::Operator).await;
        if folder {
            self.sync_script().await;
        }
        result
    }

    /// Send a PTZ action to the input's camera (PER.2). Nothing in the
    /// show changes; the camera does not answer back here.
    async fn ptz(&self, action: &Action, message: Vec<u8>) -> Result<(), ActionError> {
        let input = match action {
            Action::PtzMove { input, .. }
            | Action::PtzZoom { input, .. }
            | Action::PtzFocus { input, .. }
            | Action::PtzHome { input }
            | Action::PtzPreset { input, .. } => input,
            _ => unreachable!("a PTZ action"),
        };
        let spec = self.switcher.lock().await.ptz_of(input)?;
        let failed = |e: std::io::Error| ActionError::BadPtz {
            reason: format!("{}: {e}", spec.address),
        };
        // An IP needs no lookup (a held key must not wait for DNS).
        let target = match spec.address.parse::<std::net::SocketAddr>() {
            Ok(address) => address,
            Err(_) => tokio::net::lookup_host(&spec.address)
                .await
                .map_err(failed)?
                .next()
                .ok_or_else(|| failed(std::io::Error::other("no address")))?,
        };
        let socket = self
            .ptz_socket
            .get_or_try_init(|| tokio::net::UdpSocket::bind(("0.0.0.0", 0)))
            .await
            .map_err(failed)?;
        let seq = self.ptz_seq.fetch_add(1, Ordering::Relaxed);
        socket
            .send_to(&crate::ptz::frame(spec.protocol, seq, &message), target)
            .await
            .map_err(failed)?;
        Ok(())
    }

    async fn handle_as(
        self: &Arc<Self>,
        action: Action,
        origin: Origin,
    ) -> Result<ShowState, ActionError> {
        if let Some(message) = crate::ptz::message(&action) {
            self.ptz(&action, message?).await?;
            return Ok(self.state().await);
        }
        match &action {
            Action::RunShortcut { target } => {
                let actions = self.switcher.lock().await.shortcut_actions(target)?;
                return self.run_all(actions, origin).await;
            }
            Action::HttpRequest {
                target,
                method,
                path,
                headers,
                body,
            } => {
                let (config, out) = self.target_and_out(target).await?;
                let request = crate::targets::Request {
                    method: *method,
                    path: path.clone(),
                    headers: headers.clone(),
                    body: body.clone(),
                };
                self.send_to_target(config, request, out);
                return Ok(self.state().await);
            }
            Action::HaService {
                target,
                service,
                entity,
                data,
            } => {
                let (config, out) = self.target_and_out(target).await?;
                let request =
                    crate::targets::ha_request(service, entity.as_deref(), data.as_ref())?;
                self.send_to_target(config, request, out);
                return Ok(self.state().await);
            }
            Action::TestTarget { name } => {
                let (config, out) = self.target_and_out(name).await?;
                if out == Outbound::Locked {
                    return Err(ActionError::IoLocked {
                        class: IoClass::Targets,
                    });
                }
                let result = crate::targets::probe(&self.target_http, &config).await;
                self.reached(name, result.clone()).await;
                result.map_err(|reason| ActionError::BadTarget { reason })?;
                return Ok(self.state().await);
            }
            Action::CheckTargets => {
                let used = {
                    let sw = self.switcher.lock().await;
                    // Locked: nothing to probe, the cues warn already.
                    if sw.io_locked(IoClass::Targets) {
                        return Ok(sw.state());
                    }
                    sw.used_targets()
                };
                let probes = used
                    .iter()
                    .map(|t| crate::targets::probe(&self.target_http, t));
                let results = futures_util::future::join_all(probes).await;
                for (target, result) in used.iter().zip(results) {
                    self.reached(&target.name, result).await;
                }
                return Ok(self.state().await);
            }
            Action::CueGo { .. }
            | Action::CueStart { .. }
            | Action::CueToggle { .. }
            | Action::CueStop { .. }
            | Action::CuePause { .. }
            | Action::CueResume { .. }
            | Action::CueReset
            | Action::CuePanic { .. }
            | Action::CueHardStop => {
                let out = {
                    let mut sw = self.switcher.lock().await;
                    let now = tokio::time::Instant::now();
                    let out = match &action {
                        Action::CueGo { list } => {
                            let held = match origin {
                                Origin::Held(hold) => Some(hold),
                                _ => None,
                            };
                            sw.cue_go(list.as_ref(), now, held)?
                        }
                        Action::CueStart { list, cue } => sw.cue_start(list.as_ref(), cue, now)?,
                        Action::CueToggle { list, cue } => {
                            sw.cue_toggle(list.as_ref(), cue, now)?
                        }
                        Action::CueStop { list, cue, .. } => {
                            sw.cue_stop(list.as_ref(), cue.as_deref(), now)?
                        }
                        Action::CuePause { list, cue } => {
                            sw.cue_pause(list.as_ref(), cue.as_deref(), now)?
                        }
                        Action::CueResume { list, cue } => {
                            sw.cue_resume(list.as_ref(), cue.as_deref(), now)?
                        }
                        Action::CuePanic { ms } => sw.cue_panic(now, *ms)?,
                        Action::CueHardStop => sw.cue_hard_stop(now),
                        _ => sw.cue_reset(now),
                    };
                    if sw.sync_standby() {
                        self.write_autosave(&sw).await;
                    }
                    self.publish(&sw);
                    out
                };
                self.cue_wake.notify_one();
                // What is due at once runs before the answer, like a cut.
                self.run_cue_outs(out).await;
                self.preload_standby().await;
                return Ok(self.state().await);
            }
            _ => {}
        }
        // Settings files (SHW.8): read and written beside the switcher.
        match &action {
            Action::ExportSettings { path } => {
                let bytes = crate::settings::to_bytes(&self.switcher.lock().await.settings_file());
                write_settings(std::path::Path::new(path), bytes)
                    .await
                    .map_err(|e| ActionError::ShowFile {
                        message: format!("{path}: {e}"),
                    })?;
                return Ok(self.state().await);
            }
            Action::ImportSettings { path } => {
                let bytes = tokio::fs::read(path)
                    .await
                    .map_err(|e| ActionError::ShowFile {
                        message: format!("{path}: {e}"),
                    })?;
                let file =
                    crate::settings::parse(&bytes).map_err(|message| ActionError::ShowFile {
                        message: format!("{path}: {message}"),
                    })?;
                let mut sw = self.switcher.lock().await;
                sw.take_settings(file)?;
                sw.refresh_warnings();
                self.write_all_shows(&mut sw).await;
                self.write_layout_templates(&mut sw).await;
                self.write_targets(&mut sw).await;
                self.publish(&sw);
                return Ok(sw.state());
            }
            _ => {}
        }
        // Packages copy media (gigabytes): without the switcher locked.
        if let Action::ExportPackage { path, machine } = &action {
            let (show, show_path, settings) = {
                let sw = self.switcher.lock().await;
                let settings = machine.then(|| {
                    crate::targets::to_bytes(&crate::targets::without_secrets(
                        &sw.target_settings(),
                    ))
                });
                (sw.show().clone(), sw.path().map(str::to_string), settings)
            };
            let templates = self
                .templates_dir
                .lock()
                .expect("templates dir lock")
                .clone();
            let out = PathBuf::from(path);
            let packed = tokio::task::spawn_blocking(move || {
                crate::package::export(
                    &show,
                    show_path.as_deref(),
                    templates.as_deref(),
                    settings.as_deref(),
                    &out,
                )
            })
            .await
            .map_err(|e| ActionError::ShowFile {
                message: e.to_string(),
            })?
            .map_err(|message| ActionError::ShowFile { message })?;
            tracing::info!(%path, files = packed.files, bytes = packed.bytes, "package written");
            return Ok(self.state().await);
        }
        // Missing files looked up in the folder before the mixer is locked
        // (a slow network folder must not hold up switching).
        let mut relocated = match &action {
            Action::RelocateMedia { folder } => Some(self.find_moved_media(folder).await),
            _ => None,
        };
        let mut unpacked = match &action {
            Action::ImportPackage {
                path,
                folder,
                machine,
            } => {
                // Refused before gigabytes are unpacked (checked again below).
                self.switcher.lock().await.check_unlocked()?;
                let templates = self
                    .templates_dir
                    .lock()
                    .expect("templates dir lock")
                    .clone();
                let (package, folder) = (PathBuf::from(path), PathBuf::from(folder));
                let machine_at = folder.join(crate::package::MACHINE_FILE);
                let (show, file, notes) = tokio::task::spawn_blocking(move || {
                    crate::package::import(&package, &folder, templates.as_deref())
                })
                .await
                .map_err(|e| ActionError::ShowFile {
                    message: e.to_string(),
                })?
                .map_err(|message| ActionError::ShowFile { message })?;
                for note in notes {
                    tracing::info!(%note, "package import");
                }
                // Templates it brought.
                self.rescan_templates().await;
                let settings = if *machine {
                    let bytes =
                        tokio::fs::read(&machine_at)
                            .await
                            .map_err(|e| ActionError::ShowFile {
                                message: format!("{}: {e}", machine_at.display()),
                            })?;
                    Some(
                        crate::targets::parse(&bytes)
                            .map_err(|message| ActionError::ShowFile { message })?,
                    )
                } else {
                    None
                };
                Some((show, file, settings))
            }
            _ => None,
        };
        // A table to import is read before the switcher is locked: it may
        // come from an address.
        let mut imported = match &action {
            Action::ImportDatasets { path, .. } => {
                let show = self.switcher.lock().await.path().map(str::to_string);
                let spec = lmix_types::DataSourceSpec::Csv {
                    path: path.clone(),
                    header: true,
                    transpose: false,
                };
                let place = datasource::resolve(&spec, show.as_deref());
                Some(datasource::read(&spec, &place, &self.http).await)
            }
            _ => None,
        };
        // Templates to import are read before the switcher is locked.
        let mut layout_import = match &action {
            Action::ImportLayoutTemplates { path } => Some(
                tokio::fs::read(path)
                    .await
                    .map_err(|e| e.to_string())
                    .and_then(|bytes| crate::layouts::parse(&bytes))
                    .map_err(|e| ActionError::BadLayout {
                        reason: format!("{path}: {e}"),
                    }),
            ),
            _ => None,
        };
        let mut sw = self.switcher.lock().await;
        if let Origin::AutoOff { channel, since } = origin {
            if !sw.overlay_still_on(channel, since) {
                return Ok(sw.state());
            }
        }
        let before = sw.on_air();
        let file_action = matches!(
            action,
            Action::OpenShow { .. } | Action::SaveShow { .. } | Action::ImportPackage { .. }
        );
        let report = match &action {
            Action::TitleReport {
                graphics,
                layer,
                event,
            } => sw.graphics_id(graphics).map(|id| Event::Title {
                graphics: id,
                layer: *layer,
                report: event.clone(),
            }),
            _ => None,
        };
        if matches!(
            action,
            Action::OpenShow { .. } | Action::NewShow { .. } | Action::ImportPackage { .. }
        ) {
            sw.check_unlocked()?;
        }

        // A changed cue list may end a running group at once: the cue
        // timer has to look again.
        let cue_lists = matches!(
            action,
            Action::SetCueList { .. } | Action::UpdateCue { .. } | Action::RemoveCueList { .. }
        );
        let commands = match action {
            Action::NewShow { format } => {
                if format.is_some_and(|f| !f.is_valid()) {
                    return Err(ActionError::BadFormat {
                        min: lmix_types::output::FORMAT_MIN_PX,
                        max: lmix_types::output::FORMAT_MAX_PX,
                    });
                }
                let show = Show {
                    format: format.unwrap_or_default(),
                    ..Show::default()
                };
                sw.replace(show, None)?
            }
            Action::OpenShow { path } => open(&mut sw, path).await?,
            Action::ImportPackage { .. } => {
                let (show, file, settings) = unpacked.take().expect("unpacked above");
                if let Some(settings) = settings {
                    sw.merge_target_settings(settings)?;
                }
                sw.replace(show, Some(file.display().to_string()))?
            }
            Action::ImportDatasets { template, .. } => {
                let table = imported.take().unwrap_or_default();
                if let Some(error) = table.error {
                    return Err(ActionError::BadTitle { reason: error });
                }
                sw.import_datasets(&table.columns, &table.rows, template)?;
                Vec::new()
            }
            Action::ExportDatasets { path, template } => {
                let spec = lmix_types::DataSourceSpec::Csv {
                    path: path.clone(),
                    header: true,
                    transpose: false,
                };
                let datasource::Place::File(file) = datasource::resolve(&spec, sw.path()) else {
                    return Err(ActionError::BadTitle {
                        reason: "data sets go to a file, not an address".into(),
                    });
                };
                let (columns, rows) = sw.export_datasets(template.as_deref());
                datasource::write_csv(&file, &columns, &rows)
                    .await
                    .map_err(|e| ActionError::BadTitle {
                        reason: format!("{path}: {e}"),
                    })?;
                Vec::new()
            }
            Action::SaveShow { path } => {
                save(&mut sw, path).await?;
                Vec::new()
            }
            Action::ImportLayoutTemplates { .. } => {
                let templates = layout_import.take().expect("read above")?;
                sw.import_layout_templates(templates)?;
                Vec::new()
            }
            Action::ExportLayoutTemplates { path, templates } => {
                let templates = sw.layout_templates_named(&templates)?;
                let file = PathBuf::from(&path);
                write_settings(&file, crate::layouts::to_bytes(&templates))
                    .await
                    .map_err(|e| ActionError::BadLayout {
                        reason: format!("{path}: {e}"),
                    })?;
                Vec::new()
            }
            Action::RelocateMedia { folder } => {
                relocate(&mut sw, &folder, relocated.take().unwrap_or_default())?
            }
            Action::AppendShow { path, inputs } => {
                let other =
                    show::load(&PathBuf::from(&path))
                        .await
                        .map_err(|e| ActionError::ShowFile {
                            message: e.to_string(),
                        })?;
                sw.append(other, &inputs)?
            }
            other => sw.apply(other)?,
        };
        sw.refresh_warnings();
        if cue_lists {
            self.cue_wake.notify_one();
        }
        if let (true, Some(path)) = (file_action, sw.path().map(str::to_string)) {
            self.recent.lock().await.touch(&path).await;
        }

        let wait = commands
            .iter()
            .position(|c| matches!(c, Command::Wait { .. }))
            .unwrap_or(commands.len());
        let (now, later) = commands.split_at(wait);
        let result = self.execute(&mut sw, now).await;
        self.read_new_sources(&mut sw).await;
        if let (Some(report), Ok(())) = (report, &result) {
            // Like an input's triggers: none for what a trigger did.
            if let (
                Origin::Operator | Origin::Held(_),
                Event::Title {
                    graphics,
                    layer,
                    report: what,
                },
            ) = (origin, &report)
            {
                for (delay_ms, actions) in sw.title_triggers(*graphics, *layer, what) {
                    self.run_triggered(delay_ms, actions);
                }
            }
            let _ = self.events.send(report);
        }
        self.publish(&sw);
        self.write_autosave(&sw).await;
        self.write_all_shows(&mut sw).await;
        self.write_layout_templates(&mut sw).await;
        self.write_targets(&mut sw).await;
        if !later.is_empty() {
            self.run_later(later.to_vec());
        }
        for (delay_ms, channel, since) in sw.auto_offs(&before) {
            self.run_auto_off(delay_ms, channel, since);
        }
        // Triggered actions fire no triggers themselves (no loops).
        if origin != Origin::Trigger && result.is_ok() {
            for (delay_ms, actions) in sw.fired_triggers(&before) {
                self.run_triggered(delay_ms, actions);
            }
        }
        result.map(|()| sw.state())
    }

    /// Actions of a shortcut in order; the first error stops them.
    async fn run_all(
        self: &Arc<Self>,
        actions: Vec<Action>,
        origin: Origin,
    ) -> Result<ShowState, ActionError> {
        for action in actions {
            // Shortcuts and cues hold none of themselves, so this ends.
            Box::pin(self.handle_as(action, origin)).await?;
        }
        Ok(self.state().await)
    }

    /// A trigger's actions after its delay, in the background.
    fn run_triggered(self: &Arc<Self>, delay_ms: u32, actions: Vec<Action>) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(u64::from(delay_ms))).await;
            for action in actions {
                if let Err(e) = service.handle_as(action, Origin::Trigger).await {
                    tracing::warn!(error = %e, "trigger action failed; the rest is skipped");
                    break;
                }
            }
        });
    }

    /// Overlay `channel` off after `delay_ms` with its own effect, unless
    /// it went off or on again since (OVL.2).
    fn run_auto_off(self: &Arc<Self>, delay_ms: u32, channel: u8, since: u64) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(u64::from(delay_ms))).await;
            let off = Action::OverlayOut {
                channel,
                duration_ms: None,
            };
            if let Err(e) = service
                .handle_as(off, Origin::AutoOff { channel, since })
                .await
            {
                tracing::warn!(error = %e, channel, "overlay auto off failed");
            }
        });
    }

    /// Run `commands`, which start with a `Command::Wait`: each wait, then
    /// the commands up to the next one.
    fn run_later(self: &Arc<Self>, commands: Vec<Command>) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let mut rest = commands.as_slice();
            while let [Command::Wait { ms }, tail @ ..] = rest {
                tokio::time::sleep(std::time::Duration::from_millis(u64::from(*ms))).await;
                let end = tail
                    .iter()
                    .position(|c| matches!(c, Command::Wait { .. }))
                    .unwrap_or(tail.len());
                let mut sw = service.switcher.lock().await;
                let due = sw.still_due(&tail[..end]);
                if let Err(e) = service.execute(&mut sw, &due).await {
                    tracing::warn!(error = %e, "delayed engine commands failed");
                }
                service.publish(&sw);
                drop(sw);
                rest = &tail[end..];
            }
        });
    }

    /// Best effort: a failed autosave must not fail the action.
    async fn write_autosave(&self, sw: &Switcher) {
        let Some(path) = &self.autosave else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = tokio::fs::create_dir_all(dir).await;
        }
        if let Err(e) = show::save(sw.show(), path).await {
            tracing::warn!(error = %e, "autosave failed");
        }
    }

    /// Feed the engine the whole show (start-up, engine restart).
    pub async fn restore(&self) -> Result<(), ActionError> {
        let mut sw = self.switcher.lock().await;
        self.engine_ready.store(true, Ordering::SeqCst);
        let commands = sw.restore_commands();
        let result = self.execute(&mut sw, &commands).await;
        self.publish(&sw);
        result
    }

    /// Run commands in order. A failing slot marks its input offline and
    /// the rest still run; the first error is returned.
    async fn execute(&self, sw: &mut Switcher, commands: &[Command]) -> Result<(), ActionError> {
        let gated = sw.cards_gate(commands.to_vec());
        let commands = gated.as_slice();
        if !self.engine_ready.load(Ordering::SeqCst) {
            // Outputs keep what was asked, so restore starts them. No
            // stinger runs, so none will end.
            for command in commands {
                mark_output(sw, command, &Ok(()));
                if matches!(command, Command::Stinger { .. }) {
                    sw.stinger_finished();
                }
            }
            return Ok(());
        }
        let mut first_error = None;
        // After a refused T-bar step: what puts sound and players back.
        // Its sound goes out in place of the batch's own, which mixed the
        // bar in (no blend reaches the engine), the rest after the batch.
        let mut bar_fix: Vec<Command> = Vec::new();

        for command in commands {
            let fixed;
            let command = match bar_fix_for(&mut bar_fix, command) {
                Some(fix) => {
                    fixed = fix;
                    &fixed
                }
                None => command,
            };
            // A new or emptied slot must not show the old source's frame.
            if let Command::SetSlot { slot, .. } = command {
                self.forget_picture(Picture::Slot(*slot));
            }
            if let Some(flow) = flow_of(command) {
                self.touched
                    .lock()
                    .expect("touched lock")
                    .insert(flow, Touch::Running);
            }
            let outcome = self.engine.run(command).await;
            if let (
                Ok(()),
                Command::Seek { slot, .. }
                | Command::PlayItem { slot, .. }
                | Command::Transport {
                    slot,
                    transport: lmix_types::Transport::Restart | lmix_types::Transport::Previous,
                },
            ) = (&outcome, command)
            {
                self.jumped
                    .lock()
                    .expect("jumped lock")
                    .insert(crate::engine::flows::slot_id(*slot), Instant::now());
            }
            // The window counts from the end: a rebuild's stop event may
            // trail the command (a drained stop takes over a second).
            if let Some(flow) = flow_of(command) {
                self.touched.lock().expect("touched lock").insert(
                    flow,
                    Touch::Done {
                        at: Instant::now(),
                        builds: builds_flow(command),
                    },
                );
            }
            mark_slot(sw, command, outcome.is_ok());
            mark_output(sw, command, &outcome);
            if let (Ok(()), Command::SetFormat { .. }) = (&outcome, command) {
                sw.mixer_built();
            }
            // Only this take's end frees the lock, not an older one's.
            if let (Ok(()), Command::Stinger { .. }) = (&outcome, command) {
                sw.stinger_started(self.engine.stinger_generation());
            }

            let Err(e) = outcome else {
                continue;
            };
            // The show has the other sound already: the output starts anew
            // with it rather than go on with the old one unseen.
            if let Command::SetOutputSound { id, .. } = command {
                if let Some(start) = sw.output_start_command(*id) {
                    tracing::warn!(output = %id, error = %e, "output did not take the other sound, restarting it");
                    let flow = output_flow_id(*id);
                    self.touched
                        .lock()
                        .expect("touched lock")
                        .insert(flow, Touch::Running);
                    let restarted = self.engine.run(&start).await;
                    self.touched.lock().expect("touched lock").insert(
                        flow,
                        Touch::Done {
                            at: Instant::now(),
                            builds: true,
                        },
                    );
                    mark_output(sw, &start, &restarted);
                    if restarted.is_ok() {
                        continue;
                    }
                    // Failed: tried again until it runs, as after a crash
                    // (also across an engine restart, which restores only
                    // running outputs).
                    self.fail_back_output(sw, *id);
                    if let Err(e) = restarted {
                        first_error.get_or_insert(ActionError::Engine {
                            message: format!("output failed, starting it again: {e}"),
                        });
                    }
                    continue;
                }
            }
            // The engine cut instead (or could not); no end will come.
            if matches!(command, Command::Stinger { .. }) {
                sw.stinger_finished();
            }
            if matches!(command, Command::TBar { .. }) {
                bar_fix = sw.t_bar_refused();
            }
            tracing::warn!(?command, error = %e, "engine command failed");
            first_error.get_or_insert(ActionError::Engine {
                message: e.to_string(),
            });
        }

        for command in &bar_fix {
            if let Err(e) = self.engine.run(command).await {
                tracing::warn!(?command, error = %e, "engine command failed");
            }
        }

        if first_error.is_some() && self.engine_restarted().await {
            tracing::warn!("engine restarted during an action; it gets the show on restore");
            self.await_engine();
            self.engine_changed.notify_one();
            for command in commands {
                mark_output(sw, command, &Ok(()));
            }
            return Ok(());
        }
        first_error.map_or(Ok(()), Err)
    }

    fn forget_picture(&self, which: Picture) {
        self.pictures
            .lock()
            .expect("picture cache lock")
            .remove(&which);
    }

    fn publish(&self, sw: &Switcher) {
        // No subscribers is fine.
        let _ = self.events.send(Event::State { state: sw.state() });
        let _ = self.events.send(Event::Tally { tally: sw.tally() });
    }
}

/// The shortcuts for all shows as `--shortcuts` keeps them; a file from
/// before named buses gets them as a migrated show does, renamed actions
/// their new name.
fn parse_shortcuts(bytes: &[u8]) -> Result<Vec<crate::show::ShowShortcut>, String> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    crate::show::migrate_shortcuts(&mut value);
    crate::show::rename_actions(&mut value);
    serde_json::from_value(value).map_err(|e| e.to_string())
}

/// A settings file of lmix-core read at start: `Ok(None)` when it is
/// missing or unreadable (it starts empty and is written later), `Err`
/// when it is broken and could not be set aside (never written over).
async fn read_settings<T>(
    path: &std::path::Path,
    what: &str,
    parse: impl Fn(&[u8]) -> Result<T, String>,
) -> Result<Option<T>, ()> {
    let bytes = match tokio::fs::read(path).await {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            tracing::warn!(error = %e, file = %path.display(), "{what} not read");
            return Ok(None);
        }
    };
    match parse(&bytes) {
        Ok(read) => Ok(Some(read)),
        Err(e) => {
            let aside = path.with_extension("json.broken");
            tracing::warn!(error = %e, file = %path.display(), aside = %aside.display(), "{what} unreadable, set aside");
            match tokio::fs::rename(path, &aside).await {
                Ok(()) => Ok(None),
                Err(e) => {
                    tracing::warn!(error = %e, file = %path.display(), "{what} will not be saved");
                    Err(())
                }
            }
        }
    }
}

/// A settings file with secrets: like [`write_settings`], but readable
/// only by the user from the start.
async fn write_private_settings(path: &std::path::Path, bytes: Vec<u8>) -> std::io::Result<()> {
    use tokio::io::AsyncWriteExt;
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(dir).await?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&tmp)
        .await?;
    file.write_all(&bytes).await?;
    file.sync_all().await?;
    tokio::fs::rename(&tmp, path).await
}

/// Write a settings file whole (a temporary file renamed over it).
async fn write_settings(path: &std::path::Path, bytes: Vec<u8>) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        tokio::fs::create_dir_all(dir).await?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await
}

/// Who started an action: triggers fire only for the operator's and an
/// overlay's auto off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    Operator,
    /// A button whose release comes later (RUN.6).
    Held(crate::cues::Hold),
    Trigger,
    /// Overlay `channel` goes off by itself (OVL.2), if it still shows
    /// what went on at `since`.
    AutoOff {
        channel: u8,
        since: u64,
    },
}

/// Whether a request goes out to its target (RUN.9).
#[derive(Clone, Copy, Debug, PartialEq)]
enum Outbound {
    Open,
    Locked,
}

/// lmix-core's last change of a flow.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Touch {
    /// A command on it is still running.
    Running,
    /// Done at `at`; `builds`: it built the flow (not stopped or emptied
    /// it).
    Done { at: Instant, builds: bool },
}

impl Touch {
    fn is_recent(&self) -> bool {
        match self {
            Touch::Running => true,
            Touch::Done { at, .. } => at.elapsed() < OWN_STOP_WINDOW,
        }
    }
}

/// Engine flow a command builds or stops.
fn flow_of(command: &Command) -> Option<FlowId> {
    match command {
        Command::SetSlot { slot, .. } | Command::CardOff { slot } => Some(slot_id(*slot)),
        Command::StartOutput { id, .. } | Command::StopOutput { id } => Some(output_flow_id(*id)),
        _ => None,
    }
}

/// Whether `command` builds its flow (`flow_of`), rather than stopping or
/// emptying it.
fn builds_flow(command: &Command) -> bool {
    matches!(
        command,
        Command::SetSlot {
            source: Some(_),
            ..
        } | Command::CardOff { .. }
            | Command::StartOutput { .. }
    )
}

/// Reflect a start or stop in the output's status.
fn mark_output(sw: &mut Switcher, command: &Command, outcome: &Result<(), EngineError>) {
    let (id, status) = match (command, outcome) {
        (Command::StartOutput { id, .. }, Ok(())) => {
            (*id, OutputStatus::Running { since_ms: now_ms() })
        }
        // Stopped for the cards: it waits for them.
        (Command::StopOutput { id }, Ok(())) if sw.output_waits_for_cards(*id) => return,
        (Command::StopOutput { id }, Ok(())) => (*id, OutputStatus::Stopped),
        (Command::StartOutput { id, .. } | Command::StopOutput { id }, Err(e)) => (
            *id,
            OutputStatus::Failed {
                message: output_failure(&e.to_string()),
            },
        ),
        _ => return,
    };
    sw.set_output_status(id, status);
}

/// The engine's `message` for a failed output; for a card output that
/// lacks GStreamer's decklink2 plugin (until GStreamer 1.30) with the
/// way to build it.
fn output_failure(message: &str) -> String {
    if message.contains("decklink2") && message.contains("not found") {
        format!("{message} (build it with scripts/decklink-plugin.sh, then restart strom)")
    } else {
        message.to_string()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The sound of `fix` for the slot `command` sets, taken out of `fix`:
/// it goes out instead of `command`.
fn bar_fix_for(fix: &mut Vec<Command>, command: &Command) -> Option<Command> {
    let Command::SetAudio { slot, .. } = command else {
        return None;
    };
    let at = fix
        .iter()
        .position(|c| matches!(c, Command::SetAudio { slot: s, .. } if s == slot))?;
    Some(fix.remove(at))
}

/// Reflect a `SetSlot` outcome in the input's status.
fn mark_slot(sw: &mut Switcher, command: &Command, ok: bool) {
    let slot = match command {
        Command::SetSlot {
            slot,
            source: Some(_),
        }
        | Command::CardOff { slot } => slot,
        _ => return,
    };
    let Some(input) = sw.show().inputs.iter().find(|i| i.slot == *slot) else {
        return;
    };

    let status = if ok {
        InputStatus::Online
    } else {
        InputStatus::Offline
    };
    sw.set_status(input.id, status);
}

async fn open(sw: &mut Switcher, path: String) -> Result<Vec<Command>, ActionError> {
    let loaded = show::load_versioned(&PathBuf::from(&path))
        .await
        .map_err(|e| ActionError::ShowFile {
            message: e.to_string(),
        })?;
    let commands = sw.replace(loaded.show, Some(path))?;
    sw.set_older_format(loaded.older);
    Ok(commands)
}

/// Found media files (old path, new path) take the old paths' place,
/// input by input (SHW.4). Inputs on program are left alone (a new source
/// restarts them); the error says so if nothing else was found.
fn relocate(
    sw: &mut Switcher,
    folder: &str,
    found: HashMap<String, String>,
) -> Result<Vec<Command>, ActionError> {
    let program = sw.state_program();
    let mut commands = Vec::new();
    let (mut moved, mut on_air) = (0, 0);
    for (id, gone) in sw.missing_media() {
        let Some(source) = sw.source_of(id).cloned() else {
            continue;
        };
        let mut hits = 0;
        let mut swap = |path: &mut String| {
            if let Some(new) = found.get(path.as_str()).filter(|_| gone.contains(path)) {
                *path = new.clone();
                hits += 1;
            }
        };
        let source = match source {
            lmix_types::SourceSpec::File { mut path } => {
                swap(&mut path);
                lmix_types::SourceSpec::File { path }
            }
            lmix_types::SourceSpec::Playlist { mut paths } => {
                paths.iter_mut().for_each(&mut swap);
                lmix_types::SourceSpec::Playlist { paths }
            }
            _ => continue,
        };
        if hits == 0 {
            continue;
        }
        if program == Some(id) {
            on_air += 1;
            continue;
        }
        moved += hits;
        commands.extend(sw.apply(Action::ReplaceSource {
            input: lmix_types::InputRef::Text(id.to_string()),
            source,
        })?);
        sw.forget_missing(id);
    }
    if moved == 0 {
        return Err(ActionError::ShowFile {
            message: if on_air > 0 {
                "the files were found, but their input is on program: take it off and try again"
                    .into()
            } else {
                format!("none of the missing files is in {folder}")
            },
        });
    }
    Ok(commands)
}

async fn save(sw: &mut Switcher, path: Option<String>) -> Result<(), ActionError> {
    let path = path
        .or_else(|| sw.path().map(str::to_string))
        .ok_or(ActionError::NoShowPath)?;

    // Over a file of an older version: keep it beside (SHW.7).
    if let (Some(version), true) = (sw.older_format(), sw.path() == Some(path.as_str())) {
        show::backup(&PathBuf::from(&path), version)
            .await
            .map_err(|e| ActionError::ShowFile {
                message: format!("keeping the older file: {e}"),
            })?;
    }
    show::save(sw.show(), &PathBuf::from(&path))
        .await
        .map_err(|e| ActionError::ShowFile {
            message: e.to_string(),
        })?;
    sw.set_path(path);
    sw.set_older_format(None);
    Ok(())
}

#[cfg(test)]
mod tests {

    use super::*;
    use crate::engine::null::NullEngine;
    use lmix_types::{
        DisplayMode, InputRef, OutputRef, OutputSource, OutputSpec, SourceSpec, TestPattern,
    };

    /// A card output that fails for want of the decklink2 plugin says
    /// how to build it; other failures stay as they are.
    #[test]
    fn a_missing_decklink2_plugin_names_the_script() {
        let missing = "engine refused: 500: Failed to create element: decklink2sink not found: \
                       the DeckLink output needs GStreamer's decklink2 plugin";
        let message = output_failure(missing);
        assert!(message.starts_with(missing), "{message}");
        assert!(message.contains("scripts/decklink-plugin.sh"), "{message}");
        assert_eq!(
            output_failure("srt: connection refused"),
            "srt: connection refused"
        );
    }

    /// `restore_input` became `restore`: the shortcuts for all shows
    /// (`--shortcuts`) with it still load.
    #[test]
    fn shortcuts_for_all_shows_with_restore_input_load() {
        let file = serde_json::json!([{
            "id": InputId::new(),
            "label": "Back",
            "actions": [{"action": "restore_input"}],
        }]);
        let shortcuts = parse_shortcuts(&serde_json::to_vec(&file).unwrap()).unwrap();
        assert_eq!(shortcuts[0].actions, [Action::Restore]);
    }

    /// Shortcuts for all shows from before named buses still load: aux
    /// bus B becomes "Bus B", the name a migrated show gives it.
    #[test]
    fn shortcuts_for_all_shows_from_before_buses_load() {
        let file = serde_json::json!([{
            "id": InputId::new(),
            "label": "In-ear up",
            "actions": [
                {"action": "set_aux_send", "input": 1, "bus": 2, "level": 80},
                {"action": "set_monitor_volume", "volume": 60},
            ],
        }]);
        let shortcuts = parse_shortcuts(&serde_json::to_vec(&file).unwrap()).unwrap();
        assert_eq!(
            shortcuts[0].actions,
            [
                Action::SetSend {
                    input: InputRef::Number(1),
                    bus: lmix_types::BusRef::Text("Bus B".into()),
                    level: 80
                },
                Action::SetPhones {
                    volume: 60,
                    mute: false
                },
            ]
        );

        // An output from before takes its bus along (version 3 would read
        // it and drop `aux` unseen).
        let file = serde_json::json!([{
            "id": InputId::new(),
            "label": "In-ear out",
            "actions": [{"action": "add_output", "spec": {"kind": "srt", "uri": "srt://:9000?mode=listener"}, "aux": 2}],
        }]);
        let shortcuts = parse_shortcuts(&serde_json::to_vec(&file).unwrap()).unwrap();
        assert!(
            matches!(&shortcuts[0].actions[0], Action::AddOutput { bus: Some(lmix_types::BusRef::Text(b)), .. } if b == "Bus B"),
            "{:?}",
            shortcuts[0].actions
        );

        // A file of today stays as it is: bus 3 is the third bus there,
        // not aux C, and a request body is the other side's JSON.
        let file = serde_json::json!([{
            "id": InputId::new(),
            "label": "Stream down",
            "actions": [
                {"action": "fade_volume", "target": {"bus": 3}, "volume": 0, "duration_ms": 500},
                {"action": "http_request", "target": "old", "body": {"action": "set_monitor_volume", "volume": 10}},
            ],
        }]);
        let shortcuts = parse_shortcuts(&serde_json::to_vec(&file).unwrap()).unwrap();
        assert_eq!(
            shortcuts[0].actions[0],
            Action::FadeVolume {
                target: lmix_types::action::VolumeTarget::Bus(lmix_types::BusRef::Number(3)),
                volume: 0,
                duration_ms: 500
            }
        );
        assert!(
            matches!(&shortcuts[0].actions[1], Action::HttpRequest { body: Some(body), .. } if body["action"] == "set_monitor_volume"),
            "{:?}",
            shortcuts[0].actions[1]
        );
    }

    /// Slots the engine was told to build (other commands vary as
    /// features grow: audio, solo, signal loss).
    fn slot_commands(engine: &NullEngine) -> usize {
        engine
            .log()
            .iter()
            .filter(|c| {
                matches!(
                    c,
                    Command::SetSlot {
                        source: Some(_),
                        ..
                    }
                )
            })
            .count()
    }

    fn bars() -> Action {
        Action::AddInput {
            source: SourceSpec::Pattern {
                pattern: TestPattern::Bars,
            },
            name: Some("Bars".into()),
        }
    }

    /// Bars again, under a name of its own (names are one per input).
    fn more_bars() -> Action {
        Action::AddInput {
            source: SourceSpec::Pattern {
                pattern: TestPattern::Bars,
            },
            name: Some("Bars 2".into()),
        }
    }

    #[tokio::test]
    async fn an_osc_subscriber_gets_standby_remaining_time_and_panic() {
        use lmix_types::{Cue, CueKind, Feed, FeedMessage, FeedOn, TargetKind, TargetSpec};
        let companion = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = companion.local_addr().unwrap().to_string();
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        tokio::spawn(service.clone().watch_cues());
        tokio::spawn(service.clone().watch_feeds(20));
        let mut events = service.subscribe();
        service
            .handle(Action::SetTarget {
                target: TargetSpec {
                    name: "Companion".into(),
                    kind: TargetKind::OscUdp,
                    addresses: vec![address],
                    secret: None,
                    timeout_ms: 1000,
                },
            })
            .await
            .unwrap();
        let message = |on, address: &str, arg: &str| FeedMessage {
            on,
            address: address.into(),
            args: vec![arg.into()],
        };
        service
            .handle(Action::SetFeed {
                feed: Feed {
                    name: "Deck".into(),
                    target: "Companion".into(),
                    messages: vec![
                        message(FeedOn::Standby, "/standby", "{name}"),
                        message(FeedOn::Running, "/left", "{name} {remaining}"),
                        message(FeedOn::Panic, "/panic", "{mode}"),
                    ],
                },
            })
            .await
            .unwrap();
        let mut atmo = Cue::empty(CueKind::Wait);
        atmo.name = "Atmo".into();
        atmo.duration_ms = Some(60_000);
        service
            .handle(Action::AddCueList {
                name: "Show".into(),
                cues: vec![atmo, Cue::actions("Licht", vec![Action::Cut])],
                source: None,
            })
            .await
            .unwrap();
        let next = || {
            let companion = &companion;
            async move {
                let mut buf = [0u8; 512];
                let n = tokio::time::timeout(
                    std::time::Duration::from_secs(3),
                    companion.recv(&mut buf),
                )
                .await
                .expect("a packet in time")
                .unwrap();
                String::from_utf8_lossy(&buf[..n]).replace('\0', " ")
            }
        };
        assert!(next().await.contains("Atmo"), "standby first");

        service.handle(Action::CueGo { list: None }).await.unwrap();
        let mut got = Vec::new();
        for _ in 0..3 {
            got.push(next().await);
        }
        assert!(
            got.iter()
                .any(|p| p.starts_with("/standby") && p.contains("Licht")),
            "{got:?}"
        );
        assert!(
            got.iter()
                .any(|p| p.starts_with("/left") && p.contains("Atmo 5")),
            "remaining time: {got:?}"
        );
        let running = tokio::time::timeout(std::time::Duration::from_secs(3), async {
            loop {
                if let Ok(Event::Running { cues }) = events.recv().await {
                    return cues;
                }
            }
        })
        .await
        .unwrap();
        assert!(running[0].remaining_ms.unwrap() > 59_000);

        service.handle(Action::CuePanic { ms: None }).await.unwrap();
        let mut panic = String::new();
        for _ in 0..10 {
            panic = next().await;
            if panic.starts_with("/panic") {
                break;
            }
        }
        assert!(panic.contains("fade"), "{panic}");
    }

    #[tokio::test]
    async fn panic_runs_the_emergency_list_and_a_restart_starts_nothing() {
        use lmix_types::{Cue, StopMode};
        let dir = tempfile::tempdir().unwrap();
        let autosave = dir.path().join("autosave.lmix");
        let first = Service::new(4, Arc::new(NullEngine::default()), Some(autosave.clone()));
        first.handle(bars()).await.unwrap();
        let cut = Action::CutDirect {
            input: InputRef::Text("Bars".into()),
        };
        let mut atmo = Cue::empty(lmix_types::CueKind::Wait);
        atmo.name = "Atmo".into();
        atmo.duration_ms = Some(600_000);
        let cues = vec![
            Cue::actions("A", vec![Action::FadeToBlack { duration_ms: 0 }]),
            atmo,
            Cue::actions("C", vec![Action::FadeToBlack { duration_ms: 0 }]),
        ];
        for (name, cues) in [
            ("Show", cues),
            ("Notfall", vec![Cue::actions("Licht", vec![cut])]),
        ] {
            first
                .handle(Action::AddCueList {
                    name: name.into(),
                    cues,
                    source: None,
                })
                .await
                .unwrap();
        }
        first
            .handle(Action::SetEmergencyList {
                list: Some(InputRef::Text("Notfall".into())),
            })
            .await
            .unwrap();
        first.handle(Action::SetGoGuard { ms: 0 }).await.unwrap();
        first.handle(Action::CueGo { list: None }).await.unwrap();
        first.handle(Action::CueGo { list: None }).await.unwrap();
        assert_eq!(first.state().await.running_cues.len(), 1, "Atmo runs");
        let mut events = first.subscribe();

        let state = first.handle(Action::CuePanic { ms: None }).await.unwrap();

        assert!(state.running_cues.is_empty());
        assert_eq!(state.cue_lists[0].standby, 3, "standby stays");
        assert!(state.cue_lists[1].emergency);
        assert!(
            state.program.is_some(),
            "the emergency list cut Bars to program"
        );
        let mut panicked = false;
        while let Ok(event) = events.try_recv() {
            panicked |= matches!(
                event,
                Event::Panic {
                    mode: StopMode::Fade
                }
            );
        }
        assert!(panicked, "the panic was told");

        // "Crash": a new service over the same autosave.
        let second = Service::new(4, Arc::new(NullEngine::default()), Some(autosave));
        assert!(second.resume().await.unwrap());
        let state = second.state().await;
        assert_eq!(state.cue_lists[0].standby, 3, "standby where it was");
        assert!(state.running_cues.is_empty(), "nothing starts by itself");
    }

    #[tokio::test]
    async fn a_cue_calls_home_assistant_and_the_secret_stays_in_its_file() {
        use std::os::unix::fs::PermissionsExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let (seen_tx, mut seen) = tokio::sync::mpsc::channel(4);
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = vec![0; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let _ = seen_tx
                    .send(String::from_utf8_lossy(&buf[..n]).into_owned())
                    .await;
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                    .await;
            }
        });
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("targets.json");
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        service.use_targets_file(file.clone()).await;
        let mut events = service.subscribe();

        let state = service
            .handle(Action::SetTarget {
                target: lmix_types::TargetSpec {
                    name: "Haus".into(),
                    kind: lmix_types::TargetKind::Http,
                    addresses: vec![address],
                    secret: Some("geheim".into()),
                    timeout_ms: 1000,
                },
            })
            .await
            .unwrap();
        assert!(state.targets[0].has_secret);
        assert!(!serde_json::to_string(&state).unwrap().contains("geheim"));
        let saved = tokio::fs::read_to_string(&file).await.unwrap();
        assert!(saved.contains("geheim"));
        let mode = tokio::fs::metadata(&file)
            .await
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "only the user reads the secrets");

        let light = Action::HaService {
            target: "Haus".into(),
            service: "light.turn_on".into(),
            entity: Some("light.raum".into()),
            data: None,
        };
        service
            .handle(Action::AddCueList {
                name: "Show".into(),
                cues: vec![lmix_types::Cue::actions("Licht", vec![light])],
                source: None,
            })
            .await
            .unwrap();
        service.handle(Action::CueGo { list: None }).await.unwrap();

        let sent = tokio::time::timeout(std::time::Duration::from_secs(5), seen.recv())
            .await
            .unwrap()
            .unwrap();
        assert!(
            sent.starts_with("POST /api/services/light/turn_on"),
            "{sent}"
        );
        let reached = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                if let Ok(Event::Target { target, result }) = events.recv().await {
                    return (target, result);
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(reached, ("Haus".to_string(), lmix_types::TargetResult::Ok));
        assert!(matches!(
            service
                .handle(Action::HttpRequest {
                    target: "Gone".into(),
                    method: lmix_types::HttpMethod::Post,
                    path: String::new(),
                    headers: Default::default(),
                    body: None,
                })
                .await,
            Err(ActionError::UnknownTarget { .. })
        ));
    }

    #[tokio::test]
    async fn checking_targets_warns_until_they_answer() {
        use tokio::io::AsyncWriteExt;
        // Nothing listens there yet: the port of a dropped listener.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        let target = |name: &str| Action::SetTarget {
            target: lmix_types::TargetSpec {
                name: name.into(),
                kind: lmix_types::TargetKind::Http,
                addresses: vec![format!("http://127.0.0.1:{port}")],
                secret: None,
                timeout_ms: 500,
            },
        };
        service.handle(target("Haus")).await.unwrap();
        // Not used by a cue: not probed, no warning.
        service.handle(target("Keller")).await.unwrap();
        let light = Action::HaService {
            target: "Haus".into(),
            service: "light.turn_on".into(),
            entity: None,
            data: None,
        };
        service
            .handle(Action::AddCueList {
                name: "Show".into(),
                cues: vec![lmix_types::Cue::actions("Licht", vec![light])],
                source: None,
            })
            .await
            .unwrap();

        let state = service.handle(Action::CheckTargets).await.unwrap();

        let problems: Vec<_> = state.cue_warnings.iter().map(|w| &w.problem).collect();
        assert!(
            matches!(problems[..], [lmix_types::cue::CueProblem::TargetUnreached { target, .. }] if target == "Haus"),
            "{problems:?}"
        );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", port))
            .await
            .unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let _ = stream
                    .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                    .await;
            }
        });
        let state = service.handle(Action::CheckTargets).await.unwrap();
        assert!(state.cue_warnings.is_empty(), "{:?}", state.cue_warnings);
    }

    #[tokio::test(start_paused = true)]
    async fn cues_follow_in_time_and_a_failed_cue_goes_on() {
        use lmix_types::{Cue, CueEvent, Follow};
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine, None);
        tokio::spawn(service.clone().watch_cues());
        service.handle(bars()).await.unwrap();
        service
            .handle(Action::AddInput {
                source: SourceSpec::Pattern {
                    pattern: TestPattern::Ball,
                },
                name: Some("Two".into()),
            })
            .await
            .unwrap();
        let cut = |name: &str| Action::CutDirect {
            input: InputRef::Text(name.into()),
        };
        let mut a = Cue::actions("A", vec![cut("Bars")]);
        a.follow = Follow::AfterWait;
        a.post_wait_ms = 300;
        let mut b = Cue::actions("B", vec![cut("Two")]);
        b.follow = Follow::AfterWait;
        b.post_wait_ms = 200;
        let mut broken = Cue::actions("Broken", vec![cut("Gone")]);
        broken.follow = Follow::AfterEnd;
        let d = Cue::actions("D", vec![cut("Bars")]);
        service
            .handle(Action::AddCueList {
                name: "Show".into(),
                cues: vec![a, b, broken, d],
                source: None,
            })
            .await
            .unwrap();
        let mut events = service.subscribe();
        let program = || async {
            let state = service.state().await;
            let id = state.program?;
            state
                .inputs
                .iter()
                .find(|i| i.id == id)
                .map(|i| i.name.clone())
        };
        let ms = |ms| tokio::time::sleep(std::time::Duration::from_millis(ms));

        let state = service.handle(Action::CueGo { list: None }).await.unwrap();
        assert_eq!(program().await.as_deref(), Some("Bars"));
        assert_eq!(
            state.cue_lists[0].standby, 5,
            "standby past the whole chain"
        );

        ms(290).await;
        assert_eq!(program().await.as_deref(), Some("Bars"));
        assert_eq!(
            service.state().await.running_cues.len(),
            1,
            "A in its post wait"
        );
        ms(20).await;
        assert_eq!(program().await.as_deref(), Some("Two"));
        ms(200).await;
        assert_eq!(
            program().await.as_deref(),
            Some("Bars"),
            "D ran after the broken cue"
        );
        assert!(service.state().await.running_cues.is_empty());

        let mut failed = false;
        while let Ok(event) = events.try_recv() {
            failed |= matches!(
                event,
                Event::Cue {
                    what: CueEvent::Failed { .. },
                    ..
                }
            );
        }
        assert!(failed, "the broken cue was reported");
    }

    #[tokio::test(start_paused = true)]
    async fn a_stinger_goes_to_the_engine_and_overlays_follow_at_the_cut() {
        use lmix_types::{action::StingerSpec, Effect, InputRef, TransitionSlot};
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        for (name, source) in [
            (
                "Sting",
                SourceSpec::File {
                    path: "sting.webm".into(),
                },
            ),
            (
                "Logo",
                SourceSpec::Color {
                    color: "#ff0000".into(),
                },
            ),
        ] {
            service
                .handle(Action::AddInput {
                    source,
                    name: Some(name.into()),
                })
                .await
                .unwrap();
        }
        service
            .handle(Action::PreviewInput {
                input: InputRef::Text("Bars".into()),
            })
            .await
            .unwrap();
        // Prepared overlays go on with the take: here, at the cut.
        service
            .handle(Action::OverlayPreview {
                channel: 1,
                input: Some(InputRef::Text("Logo".into())),
            })
            .await
            .unwrap();
        service
            .handle(Action::SetTransition {
                slot: TransitionSlot(1),
                effect: Effect::Stinger,
                duration_ms: 1000,
                stinger: Some(StingerSpec {
                    input: InputRef::Text("Sting".into()),
                    cut_ms: 400,
                }),
            })
            .await
            .unwrap();
        assert!(engine
            .log()
            .iter()
            .any(|c| matches!(c, Command::StingerLibrary { clips } if clips.len() == 1)));
        let overlay = || {
            engine
                .log()
                .iter()
                .any(|c| matches!(c, Command::Overlay { slot: Some(2), .. }))
        };

        service
            .handle(Action::Transition {
                slot: TransitionSlot(1),
            })
            .await
            .unwrap();
        assert!(engine
            .log()
            .iter()
            .any(|c| matches!(c, Command::Stinger { to: 0, clip } if clip.file == "sting.webm")));
        assert!(!overlay(), "no overlay before the cut point");
        // The switcher stays free meanwhile.
        service.state().await;

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(overlay());
    }

    /// Red on program, Bars in preview, button 1 a stinger with the clip
    /// "Sting".
    async fn stinger_service(engine: Arc<NullEngine>) -> Arc<Service> {
        use lmix_types::{action::StingerSpec, Effect, InputRef, TransitionSlot};
        let service = Service::new(4, engine, None);
        service.handle(bars()).await.unwrap();
        let sting = SourceSpec::File {
            path: "sting.webm".into(),
        };
        let red = SourceSpec::Color {
            color: "#ff0000".into(),
        };
        for (name, source) in [("Sting", sting), ("Red", red)] {
            service
                .handle(Action::AddInput {
                    source,
                    name: Some(name.into()),
                })
                .await
                .unwrap();
        }
        service
            .handle(Action::CutDirect {
                input: InputRef::Text("Red".into()),
            })
            .await
            .unwrap();
        service
            .handle(Action::PreviewInput {
                input: InputRef::Text("Bars".into()),
            })
            .await
            .unwrap();
        service
            .handle(Action::SetTransition {
                slot: TransitionSlot(1),
                effect: Effect::Stinger,
                duration_ms: 1000,
                stinger: Some(StingerSpec {
                    input: InputRef::Text("Sting".into()),
                    cut_ms: 400,
                }),
            })
            .await
            .unwrap();
        service
    }

    /// Cut until it is accepted, at most `within`.
    async fn cut_within(service: &Arc<Service>, within: std::time::Duration) -> bool {
        let until = Instant::now() + within;
        while Instant::now() < until {
            if service.handle(Action::Cut).await.is_ok() {
                return true;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
        false
    }

    #[tokio::test]
    async fn a_stinger_holds_switching_until_the_engine_ends_it() {
        use crate::engine::meters::{StingerEnd, StingerOutcome};
        use lmix_types::TransitionSlot;
        let engine = Arc::new(NullEngine::stinging());
        let service = stinger_service(engine.clone()).await;
        tokio::spawn(service.clone().watch_stingers());
        let sting = Action::Transition {
            slot: TransitionSlot(1),
        };

        service.handle(sting.clone()).await.unwrap();
        let wait = std::time::Duration::from_millis(1200);
        assert!(
            !cut_within(&service, wait).await,
            "past the clip, still held"
        );

        engine.end_stinger(StingerEnd {
            outcome: StingerOutcome::Completed,
            generation: 1,
        });
        let soon = std::time::Duration::from_millis(500);
        assert!(cut_within(&service, soon).await, "free at the end");

        // A late end of the first take does not free the second.
        service.handle(sting.clone()).await.unwrap();
        engine.end_stinger(StingerEnd {
            outcome: StingerOutcome::Completed,
            generation: 1,
        });
        assert!(!cut_within(&service, soon).await, "an older take's end");
        engine.end_stinger(StingerEnd {
            outcome: StingerOutcome::Failed,
            generation: 2,
        });
        assert!(cut_within(&service, soon).await, "its own end");

        // A stinger the engine could not play ends at once (it cut).
        engine.refuse(true);
        assert!(service.handle(sting).await.is_err());
        engine.refuse(false);
        assert!(service.handle(Action::Cut).await.is_ok());
    }

    #[tokio::test]
    async fn missed_stinger_ends_keep_the_lock() {
        use crate::engine::meters::{StingerEnd, StingerOutcome};
        use lmix_types::TransitionSlot;
        let engine = Arc::new(NullEngine::stinging());
        let service = stinger_service(engine.clone()).await;
        tokio::spawn(service.clone().watch_stingers());
        tokio::task::yield_now().await;
        service
            .handle(Action::Transition {
                slot: TransitionSlot(1),
            })
            .await
            .unwrap();

        // More ends of an older take than the watcher can hold: it misses
        // some, and they may have been anyone's.
        for _ in 0..40 {
            engine.end_stinger(StingerEnd {
                outcome: StingerOutcome::Completed,
                generation: 0,
            });
        }
        let soon = std::time::Duration::from_millis(300);
        assert!(!cut_within(&service, soon).await, "still held");
    }

    #[tokio::test(start_paused = true)]
    async fn a_trigger_runs_after_its_delay_and_fires_no_more() {
        use lmix_types::{InputRef, Trigger, TriggerOn};
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        let bars = InputRef::Text("Bars".into());
        service
            .handle(Action::SetTriggers {
                input: bars.clone(),
                triggers: vec![Trigger {
                    on: TriggerOn::Program,
                    delay_ms: 2000,
                    before_ms: None,
                    at_ms: None,
                    actions: vec![Action::FadeToBlack { duration_ms: 0 }],
                }],
            })
            .await
            .unwrap();

        service
            .handle(Action::CutDirect { input: bars })
            .await
            .unwrap();
        assert!(!service.state().await.fade_to_black);

        tokio::time::sleep(std::time::Duration::from_millis(2500)).await;
        assert!(service.state().await.fade_to_black, "trigger ran");
    }

    #[tokio::test(start_paused = true)]
    async fn a_clip_pauses_after_the_fade_unless_it_is_back_on_air() {
        use lmix_types::{AutoPlay, InputRef, SourceSpec, TransitionSlot, Transport};
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        service
            .handle(Action::AddInput {
                source: SourceSpec::File {
                    path: "/clip.mp4".into(),
                },
                name: Some("Clip".into()),
            })
            .await
            .unwrap();
        let clip = InputRef::Text("Clip".into());
        service
            .handle(Action::SetAutoPlay {
                input: clip.clone(),
                auto_play: AutoPlay {
                    pause: true,
                    ..Default::default()
                },
            })
            .await
            .unwrap();
        let pauses = || {
            engine
                .log()
                .iter()
                .filter(|c| {
                    matches!(
                        c,
                        Command::Transport {
                            slot: 1,
                            transport: Transport::Pause
                        }
                    )
                })
                .count()
        };
        let fade_to_bars = || async {
            service
                .handle(Action::CutDirect {
                    input: clip.clone(),
                })
                .await
                .unwrap();
            service
                .handle(Action::PreviewInput {
                    input: InputRef::Text("Bars".into()),
                })
                .await
                .unwrap();
            // Button 1: fade, 500 ms.
            service
                .handle(Action::Transition {
                    slot: TransitionSlot(1),
                })
                .await
                .unwrap();
        };

        fade_to_bars().await;
        assert_eq!(pauses(), 0, "seen while it fades out");
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        assert_eq!(pauses(), 1);

        // Back on program during the fade: it keeps playing.
        fade_to_bars().await;
        service.handle(Action::Cut).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(600)).await;
        assert_eq!(pauses(), 1);
    }

    #[tokio::test]
    async fn a_shortcut_runs_its_actions_in_order() {
        use lmix_types::{InputRef, ShortcutSpec};
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        let shortcut = ShortcutSpec {
            all_shows: false,
            label: "Bars on air".into(),
            key: None,
            web: false,
            color: None,
            midi: None,
            actions: vec![
                Action::PreviewInput {
                    input: InputRef::Text("Bars".into()),
                },
                Action::Cut,
            ],
        };
        service
            .handle(Action::AddShortcut { shortcut })
            .await
            .unwrap();

        let state = service
            .handle(Action::RunShortcut {
                target: InputRef::Number(1),
            })
            .await
            .unwrap();

        assert_eq!(state.program, Some(state.inputs[0].id));
        // A failing action stops the shortcut and reports why.
        assert!(matches!(
            service
                .handle(Action::RunShortcut {
                    target: InputRef::Number(1),
                })
                .await,
            Err(ActionError::OnProgram { .. })
        ));
    }

    #[tokio::test]
    async fn add_marks_input_online_and_publishes() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        let mut events = service.subscribe();

        let state = service.handle(bars()).await.unwrap();

        assert_eq!(state.inputs[0].status, InputStatus::Online);
        assert_eq!(slot_commands(&engine), 1, "{:?}", engine.log());
        assert!(matches!(events.recv().await.unwrap(), Event::State { .. }));
        assert!(matches!(events.recv().await.unwrap(), Event::Tally { .. }));
    }

    /// strom refuses a bar step while a take of its own still runs: the
    /// bar goes back to 0 and the sound stays with program, as on air.
    #[tokio::test]
    async fn a_refused_t_bar_goes_back_and_keeps_the_sound() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        let named = |name: &str| InputRef::Text(name.into());
        for name in ["A", "B"] {
            service
                .handle(Action::AddInput {
                    source: SourceSpec::Pattern {
                        pattern: TestPattern::Bars,
                    },
                    name: Some(name.into()),
                })
                .await
                .unwrap();
            service
                .handle(Action::SetAudioFollow {
                    input: named(name),
                    follow: true,
                })
                .await
                .unwrap();
        }
        service
            .handle(Action::CutDirect { input: named("A") })
            .await
            .unwrap();
        service
            .handle(Action::PreviewInput { input: named("B") })
            .await
            .unwrap();

        engine.refuse_t_bar(true);
        let answer = service.handle(Action::TBar { position: 0.5 }).await;

        assert!(answer.is_err(), "{answer:?}");
        assert_eq!(service.state().await.t_bar, 0.0);
        let log = engine.log();
        let bar = log
            .iter()
            .rposition(|c| matches!(c, Command::TBar { .. }))
            .expect("bar sent");
        let after: Vec<(usize, f64, bool)> = log[bar..]
            .iter()
            .filter_map(|c| match c {
                Command::SetAudio {
                    slot, gain, muted, ..
                } => Some((*slot, (*gain * 100.0).round() / 100.0, *muted)),
                _ => None,
            })
            .collect();
        assert!(
            after.iter().all(|&(_, gain, _)| gain == 1.0),
            "the blend reached the engine: {after:?}"
        );
    }

    #[tokio::test(start_paused = true)]
    async fn failed_input_goes_offline_and_fails_back() {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;
        let builds = slot_commands(&engine);

        engine.fail(FlowError {
            flow: slot_id(0),
            message: "source gone".into(),
            kind: FlowFailure::Error,
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        assert_eq!(service.state().await.inputs[0].status, InputStatus::Offline);

        tokio::time::sleep(FAILBACK_FIRST).await;
        assert_eq!(service.state().await.inputs[0].status, InputStatus::Online);
        assert_eq!(slot_commands(&engine), builds + 1, "slot rebuilt once");
    }

    #[tokio::test(start_paused = true)]
    async fn own_rebuild_stop_is_no_failure() {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;

        engine.fail(FlowError {
            flow: slot_id(0),
            message: "stopped".into(),
            kind: FlowFailure::Stopped,
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert_eq!(service.state().await.inputs[0].status, InputStatus::Online);
    }

    /// A stop right after a command of its own, of a flow that then does
    /// not run, is a failure: the input fails back.
    #[tokio::test(start_paused = true)]
    async fn a_foreign_stop_just_after_an_own_command_is_a_failure() {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;

        engine.stop_flow(slot_id(0));
        engine.fail(FlowError {
            flow: slot_id(0),
            message: "stopped".into(),
            kind: FlowFailure::Stopped,
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        assert_eq!(service.state().await.inputs[0].status, InputStatus::Offline);
    }

    /// Just after its own emptying or stop, a stop is lmix-core's own
    /// even though the flow does not run (a new show empties a slot before
    /// it builds it again); just after building it, only while it runs.
    #[tokio::test(start_paused = true)]
    async fn a_stop_after_an_own_emptying_is_own() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        let flow = slot_id(0);
        engine.stop_flow(flow);
        let touch = |builds| Touch::Done {
            at: Instant::now(),
            builds,
        };
        service.touched.lock().unwrap().insert(flow, touch(false));
        assert!(service.own_stop(flow).await);
        service.touched.lock().unwrap().insert(flow, touch(true));
        assert!(!service.own_stop(flow).await);
    }

    #[tokio::test(start_paused = true)]
    async fn stop_during_a_slow_own_rebuild_is_no_failure() {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;

        // The old flow drains for longer than the window while the
        // command still runs; its stop must not count as a failure.
        engine.slow_next(
            OWN_STOP_WINDOW * 3,
            FlowError {
                flow: slot_id(0),
                message: "stopped".into(),
                kind: FlowFailure::Stopped,
            },
        );
        service.handle(bars()).await.unwrap();
        tokio::time::sleep(FAILBACK_FIRST * 2).await;

        assert_eq!(service.state().await.inputs[0].status, InputStatus::Online);
        assert_eq!(slot_commands(&engine), 1, "no failback rebuild");
    }

    /// An output whose sound the engine could not move starts anew with
    /// the sound the show has now, instead of going on with the old one.
    #[tokio::test(start_paused = true)]
    async fn an_output_that_keeps_its_old_sound_starts_anew() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service
            .handle(Action::AddBus {
                name: Some("Mix minus".into()),
                pre_fader: false,
            })
            .await
            .unwrap();
        service
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Srt {
                    uri: "srt://cdn:9000".into(),
                    video_kbps: 6000,
                    encoding: Default::default(),
                },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Stream".into()),
            })
            .await
            .unwrap();
        let output = || lmix_types::OutputRef::Text("Stream".into());
        service
            .handle(Action::StartOutput { output: output() })
            .await
            .unwrap();
        engine.refuse_output_sound(true);
        service
            .handle(Action::SetOutputBus {
                output: output(),
                bus: Some(lmix_types::BusRef::Text("Mix minus".into())),
            })
            .await
            .unwrap();
        let state = service.state().await;
        let bus = state.buses[0].id;
        let log = engine.log();
        assert!(
            matches!(
                &log[log.len() - 2..],
                [Command::SetOutputSound { .. }, Command::StartOutput { bus: Some(b), .. }] if *b == bus
            ),
            "{log:?}"
        );
        assert_eq!(state.outputs[0].bus, Some(bus));
        assert!(matches!(
            state.outputs[0].status,
            lmix_types::OutputStatus::Running { .. }
        ));
    }

    /// The cards start off: a card input shows black with a banner and a
    /// started card output waits. On, both use the card; off again, both
    /// let go of it. An output stopped while it waits stays stopped.
    #[tokio::test]
    async fn cards_are_used_only_while_switched_on() {
        use lmix_types::{OutputRef, OutputStatus};
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        let card = SourceSpec::Card {
            device: 2,
            mode: None,
            pair: 1,
        };
        service
            .handle(Action::AddInput {
                source: card.clone(),
                name: Some("SDI".into()),
            })
            .await
            .unwrap();
        for (name, device) in [("Beamer", 1), ("Spare", 3)] {
            service
                .handle(Action::AddOutput {
                    spec: lmix_types::OutputSpec::Card {
                        device,
                        mode: lmix_types::CardMode::Hd1080p25,
                        channels: Default::default(),
                        more_pairs: Vec::new(),
                        sound_delay_ms: 0,
                    },
                    source: lmix_types::OutputSource::Program,
                    bus: None,
                    name: Some(name.into()),
                })
                .await
                .unwrap();
            service
                .handle(Action::StartOutput {
                    output: OutputRef::Text(name.into()),
                })
                .await
                .unwrap();
        }
        let status = |state: &lmix_types::ShowState, i: usize| state.outputs[i].status.clone();
        let starts = || {
            engine
                .log()
                .iter()
                .filter(|c| matches!(c, Command::StartOutput { .. }))
                .count()
        };

        // Off at the start.
        let state = service.state().await;
        assert!(!state.cards_on);
        assert!(engine.log().contains(&Command::CardOff { slot: 0 }));
        assert!(!engine.log().iter().any(|c| matches!(
            c,
            Command::SetSlot {
                source: Some(SourceSpec::Card { .. }),
                ..
            }
        )));
        assert_eq!(starts(), 0);
        assert_eq!(status(&state, 0), OutputStatus::CardsOff);

        // The spare one stops waiting.
        service
            .handle(Action::StopOutput {
                output: OutputRef::Text("Spare".into()),
            })
            .await
            .unwrap();

        service.handle(Action::SetCards { on: true }).await.unwrap();
        let state = service.state().await;
        assert!(state.cards_on);
        assert!(engine.log().contains(&Command::SetSlot {
            slot: 0,
            source: Some(card.clone()),
        }));
        assert_eq!(starts(), 1);
        assert!(matches!(status(&state, 0), OutputStatus::Running { .. }));
        assert_eq!(status(&state, 1), OutputStatus::Stopped);

        service
            .handle(Action::SetCards { on: false })
            .await
            .unwrap();
        let state = service.state().await;
        let log = engine.log();
        assert_eq!(
            log.last(),
            Some(&Command::StopOutput {
                id: state.outputs[0].id
            })
        );
        assert_eq!(
            log.iter()
                .filter(|c| **c == Command::CardOff { slot: 0 })
                .count(),
            2
        );
        assert_eq!(status(&state, 0), OutputStatus::CardsOff);
    }

    /// Card output "Beamer", started: it waits for the cards.
    async fn waiting_card_output() -> (Arc<NullEngine>, Arc<Service>) {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Card {
                    device: 1,
                    mode: lmix_types::CardMode::Hd1080p25,
                    channels: Default::default(),
                    more_pairs: Vec::new(),
                    sound_delay_ms: 0,
                },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Beamer".into()),
            })
            .await
            .unwrap();
        service
            .handle(Action::StartOutput {
                output: lmix_types::OutputRef::Text("Beamer".into()),
            })
            .await
            .unwrap();
        (engine, service)
    }

    /// lmix-core restarted while the engine used the cards: they stay on,
    /// the input keeps its card.
    #[tokio::test]
    async fn a_restart_keeps_cards_the_engine_uses() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.await_engine();
        let card = SourceSpec::Card {
            device: 2,
            mode: None,
            pair: 1,
        };
        service
            .handle(Action::AddInput {
                source: card.clone(),
                name: Some("SDI".into()),
            })
            .await
            .unwrap();
        engine.hold_cards();
        service.adopt_cards().await;
        service.restore().await.unwrap();
        service.adopt_running_outputs().await;
        let state = service.state().await;
        assert!(state.cards_on);
        let log = engine.log();
        assert!(log.contains(&Command::SetSlot {
            slot: 0,
            source: Some(card),
        }));
        assert!(!log.contains(&Command::CardOff { slot: 0 }));
    }

    /// A card output still running after a restart turns the cards on
    /// even when the engine could not say it uses them: it stays on air.
    #[tokio::test]
    async fn a_running_card_output_keeps_the_cards_on() {
        let dir = tempfile::tempdir().unwrap();
        let autosave = dir.path().join("autosave.lmix");
        let engine = Arc::new(NullEngine::default());
        let first = Service::new(4, engine.clone(), Some(autosave.clone()));
        let card = SourceSpec::Card {
            device: 2,
            mode: None,
            pair: 1,
        };
        first
            .handle(Action::AddInput {
                source: card.clone(),
                name: Some("SDI".into()),
            })
            .await
            .unwrap();
        first
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Card {
                    device: 1,
                    mode: lmix_types::CardMode::Hd1080p25,
                    channels: Default::default(),
                    more_pairs: Vec::new(),
                    sound_delay_ms: 0,
                },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Beamer".into()),
            })
            .await
            .unwrap();
        first.handle(Action::SetCards { on: true }).await.unwrap();
        first
            .handle(Action::StartOutput {
                output: lmix_types::OutputRef::Text("Beamer".into()),
            })
            .await
            .unwrap();

        let second = Service::new(4, engine.clone(), Some(autosave));
        assert!(second.resume().await.unwrap());
        second.adopt_cards().await;
        second.adopt_running_outputs().await;
        let state = second.state().await;
        assert!(state.cards_on);
        assert!(matches!(
            state.outputs[0].status,
            lmix_types::OutputStatus::Running { .. }
        ));
        assert!(!engine
            .log()
            .iter()
            .any(|c| matches!(c, Command::StopOutput { .. })));
        // The input, black after the restore, has its card again.
        let last_slot = engine.log().into_iter().rev().find(|c| {
            matches!(
                c,
                Command::SetSlot { slot: 0, .. } | Command::CardOff { slot: 0 }
            )
        });
        assert_eq!(
            last_slot,
            Some(Command::SetSlot {
                slot: 0,
                source: Some(card),
            })
        );
    }

    /// A card output failing back stops trying once the cards go off and
    /// waits for them.
    #[tokio::test(start_paused = true)]
    async fn switching_the_cards_off_ends_a_failback() {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;
        service
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Card {
                    device: 1,
                    mode: lmix_types::CardMode::Hd1080p25,
                    channels: Default::default(),
                    more_pairs: Vec::new(),
                    sound_delay_ms: 0,
                },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Beamer".into()),
            })
            .await
            .unwrap();
        service.handle(Action::SetCards { on: true }).await.unwrap();
        engine.fail_outputs(true);
        service
            .handle(Action::StartOutput {
                output: lmix_types::OutputRef::Text("Beamer".into()),
            })
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(4)).await;
        service
            .handle(Action::SetCards { on: false })
            .await
            .unwrap();

        let starts = || {
            engine
                .log()
                .iter()
                .filter(|c| matches!(c, Command::StartOutput { .. }))
                .count()
        };
        let before = starts();
        tokio::time::sleep(FAILBACK_MAX * 3).await;
        assert_eq!(starts(), before, "started while the cards are off");
        assert_eq!(
            service.state().await.outputs[0].status,
            lmix_types::OutputStatus::CardsOff
        );
    }

    /// Outputs stopped as a group stop waiting for the cards too.
    #[tokio::test]
    async fn a_group_stop_ends_the_wait_for_the_cards() {
        let (engine, service) = waiting_card_output().await;
        service
            .handle(Action::StopOutputs {
                group: lmix_types::OutputGroup::All,
            })
            .await
            .unwrap();
        service.handle(Action::SetCards { on: true }).await.unwrap();
        assert!(!engine
            .log()
            .iter()
            .any(|c| matches!(c, Command::StartOutput { .. })));
        assert_eq!(
            service.state().await.outputs[0].status,
            lmix_types::OutputStatus::Stopped
        );
    }

    /// The lock refuses switching the cards off, which stops outputs; on
    /// is allowed.
    #[tokio::test]
    async fn the_lock_keeps_the_cards_on() {
        let (_engine, service) = waiting_card_output().await;
        service.handle(Action::SetLock { on: true }).await.unwrap();
        service.handle(Action::SetCards { on: true }).await.unwrap();
        assert!(matches!(
            service.handle(Action::SetCards { on: false }).await,
            Err(ActionError::Locked)
        ));
        let state = service.state().await;
        assert!(state.cards_on);
        assert!(matches!(
            state.outputs[0].status,
            lmix_types::OutputStatus::Running { .. }
        ));
    }

    /// An output whose flow fails right after each start (a busy card)
    /// stays failed while it is started again, ever less often, and
    /// counts as running once a start holds.
    #[tokio::test(start_paused = true)]
    async fn an_output_failing_after_each_start_stays_failed() {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;
        service
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Ndi { name: "PGM".into() },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Card".into()),
            })
            .await
            .unwrap();
        engine.fail_outputs(true);
        service
            .handle(Action::StartOutput {
                output: lmix_types::OutputRef::Text("Card".into()),
            })
            .await
            .unwrap();

        let status = || async { service.state().await.outputs[0].status.clone() };
        let starts = || {
            engine
                .log()
                .iter()
                .filter(|c| matches!(c, Command::StartOutput { .. }))
                .count()
        };
        // From its first failure on (1.5 s after the start).
        let second = std::time::Duration::from_secs(1);
        tokio::time::sleep(second * 2).await;
        for _ in 0..60 {
            tokio::time::sleep(second).await;
            let now = status().await;
            assert!(
                matches!(now, lmix_types::OutputStatus::Failed { .. }),
                "{now:?} after {} starts",
                starts()
            );
        }
        // Started again with growing pauses, not every two seconds.
        assert!(starts() <= 10, "{} starts in a minute", starts());

        engine.fail_outputs(false);
        tokio::time::sleep(FAILBACK_MAX * 3).await;
        let now = status().await;
        assert!(
            matches!(now, lmix_types::OutputStatus::Running { .. }),
            "{now:?}"
        );
    }

    /// An output failing while its failback asks whether it runs is not
    /// taken for running: it fails back again.
    #[tokio::test(start_paused = true)]
    async fn an_output_failing_while_its_failback_asks_fails_back_again() {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;
        service
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Ndi { name: "PGM".into() },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Card".into()),
            })
            .await
            .unwrap();
        engine.fail_outputs(true);
        service
            .handle(Action::StartOutput {
                output: lmix_types::OutputRef::Text("Card".into()),
            })
            .await
            .unwrap();
        // Failed once; the failback's start holds, its question finds the
        // output running but it fails meanwhile.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        engine.fail_outputs(false);
        engine.fail_after_answer();

        tokio::time::sleep(FAILBACK_MAX * 3).await;
        let id = service.state().await.outputs[0].id;
        assert!(engine.output_running(id).await, "not started again");
        assert!(matches!(
            service.state().await.outputs[0].status,
            lmix_types::OutputStatus::Running { .. }
        ));
    }

    /// Between two tries of its failback a failed output keeps its
    /// settings: the next try would start it with new ones. Stopped, it
    /// can be changed.
    #[tokio::test(start_paused = true)]
    async fn an_output_failing_back_keeps_its_settings() {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;
        service
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Ndi { name: "PGM".into() },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Card".into()),
            })
            .await
            .unwrap();
        engine.fail_outputs(true);
        service
            .handle(Action::StartOutput {
                output: lmix_types::OutputRef::Text("Card".into()),
            })
            .await
            .unwrap();
        // Failed after 1.5 s; its failback tries again 2 s later.
        tokio::time::sleep(std::time::Duration::from_millis(1600)).await;
        let output = service.state().await.outputs[0].clone();
        assert!(
            matches!(output.status, lmix_types::OutputStatus::Failed { .. }),
            "{:?}",
            output.status
        );
        assert!(output.failback, "the state says a retry is coming");
        let update = || Action::UpdateOutput {
            output: lmix_types::OutputRef::Text("Card".into()),
            spec: lmix_types::OutputSpec::Ndi {
                name: "Other".into(),
            },
            source: lmix_types::OutputSource::Program,
            name: Some("Card".into()),
            bus: None,
        };
        assert!(matches!(
            service.handle(update()).await,
            Err(ActionError::OutputSending { .. })
        ));

        service
            .handle(Action::StopOutput {
                output: lmix_types::OutputRef::Text("Card".into()),
            })
            .await
            .unwrap();
        assert!(!service.state().await.outputs[0].failback);
        service.handle(update()).await.unwrap();
        // Its failback ends without starting it.
        tokio::time::sleep(FAILBACK_MAX * 3).await;
        assert!(!engine.output_running(output.id).await);
    }

    /// A service with output "Card" failed once and started again: its
    /// flow runs, the failback still holds it for failed.
    async fn holding_output() -> (Arc<NullEngine>, Arc<Service>, lmix_types::OutputId) {
        let engine = Arc::new(NullEngine::failing());
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_failures());
        tokio::task::yield_now().await;
        service
            .handle(Action::AddBus {
                name: Some("Mix minus".into()),
                pre_fader: false,
            })
            .await
            .unwrap();
        service
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Ndi { name: "PGM".into() },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Card".into()),
            })
            .await
            .unwrap();
        engine.fail_outputs(true);
        service
            .handle(Action::StartOutput {
                output: lmix_types::OutputRef::Text("Card".into()),
            })
            .await
            .unwrap();
        // Failed after 1.5 s, started again after 3.5 s, held until 8.5 s.
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        engine.fail_outputs(false);
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
        let id = service.state().await.outputs[0].id;
        assert!(engine.output_running(id).await);
        assert!(matches!(
            service.state().await.outputs[0].status,
            lmix_types::OutputStatus::Failed { .. }
        ));
        (engine, service, id)
    }

    /// Outputs stopped while a failback holds one stop its flow too.
    #[tokio::test(start_paused = true)]
    async fn stopping_outputs_stops_one_the_failback_holds() {
        let (engine, service, id) = holding_output().await;
        service
            .handle(Action::StopOutputs {
                group: lmix_types::OutputGroup::All,
            })
            .await
            .unwrap();
        tokio::time::sleep(FAILBACK_MAX * 3).await;
        assert!(!engine.output_running(id).await, "flow still runs");
        assert_eq!(
            service.state().await.outputs[0].status,
            lmix_types::OutputStatus::Stopped
        );
    }

    /// An output stopped while the failback holds it no longer counts as
    /// running.
    #[tokio::test(start_paused = true)]
    async fn stopping_an_output_the_failback_holds_ends_the_hold() {
        let (engine, service, id) = holding_output().await;
        service
            .handle(Action::StopOutput {
                output: lmix_types::OutputRef::Text("Card".into()),
            })
            .await
            .unwrap();
        tokio::time::sleep(FAILBACK_MAX * 3).await;
        assert!(!engine.output_running(id).await, "flow still runs");
        assert!(!service.switcher.lock().await.output_is_running(id));
    }

    /// A bus given to an output the failback holds reaches its flow.
    #[tokio::test(start_paused = true)]
    async fn a_bus_reaches_an_output_the_failback_holds() {
        let (engine, service, _) = holding_output().await;
        service
            .handle(Action::SetOutputBus {
                output: lmix_types::OutputRef::Text("Card".into()),
                bus: Some(lmix_types::BusRef::Text("Mix minus".into())),
            })
            .await
            .unwrap();
        assert!(engine
            .log()
            .iter()
            .any(|c| matches!(c, Command::SetOutputSound { bus: Some(_), .. })));
    }

    /// An output whose sound the engine could not move and whose start anew
    /// failed too is started again, with the new bus, until it runs.
    #[tokio::test(start_paused = true)]
    async fn an_output_that_could_not_take_the_other_sound_fails_back() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service
            .handle(Action::AddBus {
                name: Some("Mix minus".into()),
                pre_fader: false,
            })
            .await
            .unwrap();
        service
            .handle(Action::AddOutput {
                spec: lmix_types::OutputSpec::Srt {
                    uri: "srt://cdn:9000".into(),
                    video_kbps: 6000,
                    encoding: Default::default(),
                },
                source: lmix_types::OutputSource::Program,
                bus: None,
                name: Some("Stream".into()),
            })
            .await
            .unwrap();
        let output = || lmix_types::OutputRef::Text("Stream".into());
        service
            .handle(Action::StartOutput { output: output() })
            .await
            .unwrap();
        engine.refuse(true);
        let refused = service
            .handle(Action::SetOutputBus {
                output: output(),
                bus: Some(lmix_types::BusRef::Text("Mix minus".into())),
            })
            .await;
        assert!(
            matches!(&refused, Err(ActionError::Engine { message }) if message.starts_with("output failed")),
            "{refused:?}"
        );
        let state = service.state().await;
        let bus = state.buses[0].id;
        assert_eq!(state.outputs[0].bus, Some(bus));
        assert!(
            matches!(
                state.outputs[0].status,
                lmix_types::OutputStatus::Failed { .. }
            ),
            "{:?}",
            state.outputs[0].status
        );

        // The first try of the failback fails as well, a later one runs.
        tokio::time::sleep(FAILBACK_FIRST + std::time::Duration::from_millis(100)).await;
        engine.refuse(false);
        tokio::time::sleep(FAILBACK_MAX * 2).await;
        let state = service.state().await;
        assert!(
            matches!(
                state.outputs[0].status,
                lmix_types::OutputStatus::Running { .. }
            ),
            "{:?}",
            state.outputs[0].status
        );
        assert!(
            matches!(
                engine.log().last(),
                Some(Command::StartOutput { bus: Some(b), .. }) if *b == bus
            ),
            "{:?}",
            engine.log()
        );
    }

    /// A new format the engine did not build is asked for again when the
    /// operator chooses it again; once built, the same format is no change.
    #[tokio::test(start_paused = true)]
    async fn a_format_the_engine_refused_is_tried_again() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        let upright = || Action::SetMasterFormat {
            format: lmix_types::MasterFormat {
                width: 1080,
                height: 1920,
                ..Default::default()
            },
        };
        let formats = |engine: &NullEngine| {
            engine
                .log()
                .iter()
                .filter(|c| matches!(c, Command::SetFormat { .. }))
                .count()
        };
        engine.refuse(true);
        assert!(service.handle(upright()).await.is_err());
        engine.refuse(false);
        service.handle(upright()).await.unwrap();
        assert_eq!(formats(&engine), 2, "asked for again");
        service.handle(upright()).await.unwrap();
        assert_eq!(formats(&engine), 2, "built: no change");
    }

    /// The tile picture costs strom as much again as the rest: a Console
    /// that shows it asks every third of `TILES_LEASE`; a lease after the
    /// last ask it goes dark, once, without polling in between.
    #[tokio::test(start_paused = true)]
    async fn the_tiles_go_dark_a_lease_after_the_last_ask() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_tiles());
        let quarter = TILES_LEASE / 4;

        tokio::time::sleep(quarter * 2).await;
        // The null engine has no pictures; the ask counts all the same.
        let _ = service.live_tiles().await;
        tokio::time::sleep(quarter * 3).await;
        assert_eq!(engine.dims(), 0, "asked within the lease");

        tokio::time::sleep(quarter * 2).await;
        assert_eq!(engine.dims(), 1, "a lease after the last ask");
        tokio::time::sleep(TILES_LEASE * 3).await;
        assert_eq!(engine.dims(), 1, "dark once");

        let _ = service.live_tiles().await;
        tokio::time::sleep(TILES_LEASE + quarter).await;
        assert_eq!(engine.dims(), 2, "a Console came back and left");
    }

    /// Tiles the engine could not get dark are tried again a lease later,
    /// not polled; dark, they rest.
    #[tokio::test(start_paused = true)]
    async fn tiles_that_stay_lit_are_dimmed_again_a_lease_later() {
        let engine = Arc::new(NullEngine::default());
        engine.fail_dims(1);
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_tiles());
        let quarter = TILES_LEASE / 4;

        tokio::time::sleep(TILES_LEASE + quarter).await;
        assert_eq!(engine.dims(), 1, "refused");
        tokio::time::sleep(TILES_LEASE).await;
        assert_eq!(engine.dims(), 2, "tried again");
        tokio::time::sleep(TILES_LEASE * 3).await;
        assert_eq!(engine.dims(), 2, "dark: no more tries");
    }

    /// A takeover keeps tiles lit that a Console of the lmix-core before
    /// read: from the start it has one lease to ask again.
    #[tokio::test(start_paused = true)]
    async fn tiles_nobody_asks_for_go_dark_a_lease_after_the_start() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_tiles());

        tokio::time::sleep(TILES_LEASE + TILES_LEASE / 4).await;
        assert_eq!(engine.dims(), 1);
    }

    #[tokio::test(start_paused = true)]
    async fn a_restarted_engine_gets_the_show_again() {
        let engine = Arc::new(NullEngine::default());
        engine.set_instance("first");
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        tokio::spawn(service.clone().watch_engine(4));
        tokio::time::sleep(ENGINE_CHECK * 3).await;
        assert_eq!(slot_commands(&engine), 1, "same engine: nothing again");

        engine.set_instance("second");
        tokio::time::sleep(ENGINE_CHECK * 2).await;

        assert_eq!(
            slot_commands(&engine),
            2,
            "restarted engine: show fed again"
        );
        service.handle(more_bars()).await.unwrap();
        assert_eq!(slot_commands(&engine), 3, "and actions run directly after");
    }

    /// The start of a restarted engine waits for a command that is
    /// already running.
    #[tokio::test(start_paused = true)]
    async fn an_engine_start_waits_for_a_running_command() {
        let engine = Arc::new(NullEngine::failing());
        engine.set_instance("first");
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_engine(4));
        tokio::task::yield_now().await;

        engine.slow_next(
            ENGINE_CHECK * 3,
            FlowError {
                flow: slot_id(3),
                message: "elsewhere".into(),
                kind: FlowFailure::Error,
            },
        );
        let running = tokio::spawn({
            let service = service.clone();
            async move { service.handle(bars()).await }
        });
        tokio::task::yield_now().await;
        engine.set_instance("second");
        running.await.unwrap().unwrap();
        tokio::time::sleep(ENGINE_CHECK * 2).await;

        assert!(engine.starts() > 0, "the restarted engine was started");
        assert!(!engine.started_during_a_command());
    }

    #[tokio::test(start_paused = true)]
    async fn action_right_after_an_engine_restart_waits_for_restore() {
        let engine = Arc::new(NullEngine::default());
        engine.set_instance("first");
        let service = Service::new(4, engine.clone(), None);
        tokio::spawn(service.clone().watch_engine(4));
        tokio::task::yield_now().await;

        // strom restarted; the watcher has not polled yet.
        engine.set_instance("second");
        engine.refuse(true);
        let answer = service.handle(bars()).await;

        assert!(answer.is_ok(), "no 502: {answer:?}");
        assert_eq!(service.state().await.inputs.len(), 1, "in the show");
        engine.refuse(false);
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(
            slot_commands(&engine),
            2,
            "refused once, then fed by the restore at once"
        );
    }

    #[tokio::test]
    async fn actions_before_the_engine_wait_for_restore() {
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.await_engine();

        service.handle(bars()).await.unwrap();
        assert_eq!(service.state().await.inputs.len(), 1, "in the show");
        assert!(engine.log().is_empty(), "nothing sent before the engine");

        service
            .handle(Action::AddOutput {
                spec: OutputSpec::Display {
                    mode: DisplayMode::Window,
                },
                source: OutputSource::default(),
                name: Some("Screen".into()),
                bus: None,
            })
            .await
            .unwrap();
        service
            .handle(Action::StartOutput {
                output: OutputRef::Text("Screen".into()),
            })
            .await
            .unwrap();
        assert!(engine.log().is_empty(), "nothing sent before the engine");

        service.restore().await.unwrap();
        assert_eq!(slot_commands(&engine), 1, "restore hands it over");
        assert!(
            engine
                .log()
                .iter()
                .any(|c| matches!(c, Command::StartOutput { .. })),
            "and starts the output asked for"
        );
        service.handle(more_bars()).await.unwrap();
        assert_eq!(slot_commands(&engine), 2, "later actions run directly");
    }

    #[tokio::test]
    async fn save_and_open_fill_recent_shows() {
        let dir = tempfile::tempdir().unwrap();
        let autosave = dir.path().join("autosave.lmix");
        let path = dir.path().join("s.lmix").display().to_string();
        let service = Service::new(4, Arc::new(NullEngine::default()), Some(autosave));

        service
            .handle(Action::SaveShow {
                path: Some(path.clone()),
            })
            .await
            .unwrap();
        service.handle(bars()).await.unwrap();

        assert_eq!(service.recent().await, [path]);
    }

    #[tokio::test]
    async fn save_open_restores_identical_show() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.lmix").display().to_string();
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        service.handle(bars()).await.unwrap();
        service
            .handle(Action::PreviewInput {
                input: InputRef::Number(1),
            })
            .await
            .unwrap();
        service
            .handle(Action::SaveShow {
                path: Some(path.clone()),
            })
            .await
            .unwrap();
        let saved = service.state().await;

        let engine = Arc::new(NullEngine::default());
        let fresh = Service::new(4, engine.clone(), None);
        let opened = fresh.handle(Action::OpenShow { path }).await.unwrap();

        assert_eq!(opened.inputs, saved.inputs);
        assert_eq!(opened.preview, saved.preview);
        let log = engine.log();
        assert_eq!(slot_commands(&engine), 1, "{log:?}");
        assert!(
            log.iter().any(|c| matches!(c, Command::Preview { .. })),
            "{log:?}"
        );
    }

    #[tokio::test]
    async fn settings_go_to_a_file_and_back_without_secrets() {
        use lmix_types::{ShortcutSpec, TargetKind, TargetSpec};
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json").display().to_string();
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        let target = |secret: Option<&str>| Action::SetTarget {
            target: TargetSpec {
                name: "Haus".into(),
                kind: TargetKind::Http,
                addresses: vec!["http://ha:8123".into()],
                secret: secret.map(str::to_string),
                timeout_ms: 1000,
            },
        };
        service.handle(target(Some("geheim"))).await.unwrap();
        service
            .handle(Action::AddShortcut {
                shortcut: ShortcutSpec {
                    label: "Schwarz".into(),
                    key: Some("F9".into()),
                    web: false,
                    color: None,
                    midi: None,
                    actions: vec![Action::FadeToBlack { duration_ms: 0 }],
                    all_shows: true,
                },
            })
            .await
            .unwrap();

        service
            .handle(Action::SetZonePatch {
                patch: lmix_types::ZonePatch {
                    zone: "hinten".into(),
                    outputs: vec![lmix_types::ZoneOutput {
                        device: String::new(),
                        channels: vec![3],
                        gain_db: -3.0,
                    }],
                },
            })
            .await
            .unwrap();
        service
            .handle(Action::ExportSettings { path: file.clone() })
            .await
            .unwrap();
        let text = tokio::fs::read_to_string(&file).await.unwrap();
        assert!(text.contains("Schwarz") && text.contains("http://ha:8123"));
        assert!(!text.contains("geheim"), "no secrets in the file");

        let state = service.handle(Action::ResetSettings).await.unwrap();
        assert!(state.targets.is_empty() && state.shortcuts.is_empty());

        // Imported: the target comes back without a secret; one set again
        // before the next import stays.
        let state = service
            .handle(Action::ImportSettings { path: file.clone() })
            .await
            .unwrap();
        assert_eq!(state.shortcuts[0].label, "Schwarz");
        assert!(!state.targets[0].has_secret);
        service
            .handle(Action::SetZone {
                zone: lmix_types::Zone {
                    name: "hinten".into(),
                    channels: 1,
                },
            })
            .await
            .unwrap();
        let zones = service.state().await.zones;
        assert_eq!(
            zones[0].outputs[0].channels,
            vec![3],
            "the zone patch came back"
        );
        service.handle(target(Some("neu"))).await.unwrap();
        let state = service
            .handle(Action::ImportSettings { path: file.clone() })
            .await
            .unwrap();
        assert!(
            state.targets[0].has_secret,
            "a target of the same name keeps its secret"
        );

        tokio::fs::write(&file, b"{\"format\":\"lmix-show\",\"version\":1}")
            .await
            .unwrap();
        assert!(service
            .handle(Action::ImportSettings { path: file })
            .await
            .is_err());
        assert_eq!(
            service.state().await.shortcuts.len(),
            1,
            "a bad file changes nothing"
        );
    }

    #[tokio::test]
    async fn an_older_show_file_is_kept_when_saved_over() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("abend.lmix");
        let old = br#"{"format":"lmix-show","version":1,"show":{"inputs":[],"preview":null,"program":null,"transitions":[],
            "cue_lists":[{"id":"7d0c3c2e-2f43-4a39-9d5c-6c8f1f0b6a11","name":"Run","cues":[{"label":"Bars","actions":[{"action":"cut"}]}]}]}}"#;
        tokio::fs::write(&file, old).await.unwrap();
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        let path = file.display().to_string();

        let state = service
            .handle(Action::OpenShow { path: path.clone() })
            .await
            .unwrap();
        assert_eq!(state.older_format, Some(1));
        assert_eq!(state.cue_lists[0].cues[0].name, "Bars");

        let state = service
            .handle(Action::SaveShow { path: None })
            .await
            .unwrap();
        assert_eq!(state.older_format, None);
        let kept = dir.path().join("abend.lmix.v1.bak");
        assert_eq!(
            tokio::fs::read(&kept).await.unwrap(),
            old.to_vec(),
            "the old file, as it was"
        );
        let new = tokio::fs::read_to_string(&file).await.unwrap();
        assert!(new.contains("\"version\": 3"), "{new}");
    }

    #[tokio::test]
    async fn crash_recovery_resumes_autosave() {
        let dir = tempfile::tempdir().unwrap();
        let autosave = dir.path().join("auto").join("autosave.lmix");
        let first = Service::new(4, Arc::new(NullEngine::default()), Some(autosave.clone()));
        first.handle(bars()).await.unwrap();

        // "Crash": a new service over the same autosave file.
        let engine = Arc::new(NullEngine::default());
        let second = Service::new(4, engine.clone(), Some(autosave));
        let resumed = second.resume().await.unwrap();

        assert!(resumed);
        assert_eq!(second.state().await.inputs.len(), 1);
        assert_eq!(second.state().await.path, None);
        let log = engine.log();
        assert_eq!(slot_commands(&engine), 1, "{log:?}");
        assert!(
            log.iter().any(|c| matches!(c, Command::Program { .. })),
            "{log:?}"
        );
    }

    #[tokio::test]
    async fn a_new_show_starts_in_the_chosen_format() {
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        let upright = lmix_types::MasterFormat {
            width: 1080,
            height: 1920,
            framerate: lmix_types::Framerate::Fps25,
        };
        let state = service
            .handle(Action::NewShow {
                format: Some(upright),
            })
            .await
            .unwrap();
        assert_eq!(state.format, upright, "SHW.3");
        let bad = lmix_types::MasterFormat {
            width: 100,
            ..upright
        };
        assert!(matches!(
            service.handle(Action::NewShow { format: Some(bad) }).await,
            Err(ActionError::BadFormat { .. })
        ));
        let plain = service
            .handle(Action::NewShow { format: None })
            .await
            .unwrap();
        assert_eq!(plain.format, lmix_types::MasterFormat::default());
        let json = r#"{"action":"new_show"}"#;
        assert_eq!(
            serde_json::from_str::<Action>(json).unwrap(),
            Action::NewShow { format: None },
            "older clients still send it without a format"
        );
    }

    #[tokio::test]
    async fn shortcuts_for_all_shows_are_kept_in_the_settings() {
        use lmix_types::{InputRef, ShortcutSpec};
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config").join("shortcuts.json");
        let first = Service::new(4, Arc::new(NullEngine::default()), None);
        first.use_shortcuts_file(file.clone()).await;
        first
            .handle(Action::AddShortcut {
                shortcut: ShortcutSpec {
                    label: "Cut".into(),
                    key: Some("F12".into()),
                    web: false,
                    color: None,
                    midi: None,
                    actions: vec![Action::Cut],
                    all_shows: true,
                },
            })
            .await
            .unwrap();
        first
            .handle(Action::NewShow { format: None })
            .await
            .unwrap();

        // Another run (or a new show): still there.
        let second = Service::new(4, Arc::new(NullEngine::default()), None);
        second.use_shortcuts_file(file.clone()).await;
        let state = second.state().await;
        assert_eq!(state.shortcuts.len(), 1);
        assert!(state.shortcuts[0].all_shows);

        second
            .handle(Action::RemoveShortcut {
                target: InputRef::Number(1),
            })
            .await
            .unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap().trim(), "[]");

        // A broken file is set aside, not overwritten.
        std::fs::write(&file, "{").unwrap();
        let third = Service::new(4, Arc::new(NullEngine::default()), None);
        third.use_shortcuts_file(file.clone()).await;
        assert!(third.state().await.shortcuts.is_empty());
        assert!(file.with_extension("json.broken").exists());
    }

    #[tokio::test(start_paused = true)]
    async fn an_overlay_goes_off_by_itself_unless_it_went_on_again() {
        use lmix_types::{Effect, InputRef};
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        service.handle(bars()).await.unwrap();
        service
            .handle(Action::SetOverlay {
                channel: 1,
                effect: Effect::Fade,
                duration_ms: 500,
                auto_off_ms: Some(2000),
                rect: None,
            })
            .await
            .unwrap();
        let show = || Action::OverlayIn {
            channel: 1,
            input: InputRef::Number(1),
            duration_ms: None,
        };
        let shown = || async { service.state().await.overlays[0].is_some() };
        let wait = |ms| tokio::time::sleep(std::time::Duration::from_millis(ms));

        service.handle(show()).await.unwrap();
        wait(1500).await;
        assert!(shown().await);
        wait(600).await;
        assert!(!shown().await, "off after 2 s");

        // Off and on again within the time: the new one counts.
        service.handle(show()).await.unwrap();
        wait(1000).await;
        service
            .handle(Action::OverlayOut {
                channel: 1,
                duration_ms: None,
            })
            .await
            .unwrap();
        service.handle(show()).await.unwrap();
        wait(1500).await;
        assert!(shown().await, "the first auto off is over");
        wait(600).await;
        assert!(!shown().await);
    }

    #[tokio::test]
    async fn layout_templates_are_kept_in_the_settings_and_go_to_files() {
        use lmix_types::InputRef;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("config").join("layout-templates.json");
        let first = Service::new(4, Arc::new(NullEngine::default()), None);
        first.use_layout_templates_file(file.clone()).await;
        first.handle(bars()).await.unwrap();
        first
            .handle(Action::AddLayout {
                name: Some("Wide".into()),
                background: Some(InputRef::Number(1)),
                boxes: Vec::new(),
            })
            .await
            .unwrap();
        first
            .handle(Action::SaveLayoutTemplate {
                layout: InputRef::Number(1),
                name: None,
            })
            .await
            .unwrap();
        let export = dir.path().join("out.json");
        first
            .handle(Action::ExportLayoutTemplates {
                path: export.display().to_string(),
                templates: vec!["Wide".into()],
            })
            .await
            .unwrap();
        assert_eq!(
            crate::layouts::parse(&std::fs::read(&export).unwrap())
                .unwrap()
                .len(),
            1
        );

        // Another run: still there; the export comes back in.
        let second = Service::new(4, Arc::new(NullEngine::default()), None);
        second.use_layout_templates_file(file.clone()).await;
        assert_eq!(second.state().await.layout_templates[0].name, "Wide");
        second
            .handle(Action::RemoveLayoutTemplate {
                template: "Wide".into(),
            })
            .await
            .unwrap();
        assert!(crate::layouts::parse(&std::fs::read(&file).unwrap())
            .unwrap()
            .is_empty());
        second
            .handle(Action::ImportLayoutTemplates {
                path: export.display().to_string(),
            })
            .await
            .unwrap();
        assert_eq!(second.state().await.layout_templates.len(), 1);
        let missing = second
            .handle(Action::ImportLayoutTemplates {
                path: dir.path().join("none.json").display().to_string(),
            })
            .await;
        assert!(matches!(missing, Err(ActionError::BadLayout { .. })));

        // A broken file is set aside, not overwritten.
        std::fs::write(&file, "{").unwrap();
        let third = Service::new(4, Arc::new(NullEngine::default()), None);
        third.use_layout_templates_file(file.clone()).await;
        assert!(third.state().await.layout_templates.is_empty());
        assert!(file.with_extension("json.broken").exists());
    }

    #[tokio::test]
    async fn resume_without_autosave_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::new(
            4,
            Arc::new(NullEngine::default()),
            Some(dir.path().join("none.lmix")),
        );
        assert!(!service.resume().await.unwrap());
    }

    #[tokio::test]
    async fn ptz_actions_reach_the_camera() {
        let camera = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = camera.local_addr().unwrap().to_string();
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        service.handle(bars()).await.unwrap();
        let input = || InputRef::Number(1);

        // No address yet: refused with a reason.
        assert!(matches!(
            service.handle(Action::PtzHome { input: input() }).await,
            Err(ActionError::BadPtz { .. })
        ));
        service
            .handle(Action::SetPtz {
                input: input(),
                ptz: Some(lmix_types::PtzSpec {
                    address,
                    protocol: lmix_types::PtzProtocol::ViscaIp,
                }),
            })
            .await
            .unwrap();
        service
            .handle(Action::PtzPreset {
                input: input(),
                preset: 3,
                store: false,
            })
            .await
            .unwrap();

        let mut buf = [0u8; 64];
        let (n, _) = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            camera.recv_from(&mut buf),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(&buf[..2], &[0x01, 0x00], "VISCA over IP command");
        assert_eq!(&buf[8..n], &[0x81, 0x01, 0x04, 0x3F, 0x02, 0x03, 0xFF]);
        assert!(service
            .handle(Action::SetPtz {
                input: input(),
                ptz: Some(lmix_types::PtzSpec {
                    address: "camera".into(),
                    protocol: lmix_types::PtzProtocol::ViscaUdp,
                }),
            })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn player_positions_go_out_as_events() {
        let engine = Arc::new(NullEngine::playing());
        let service = Service::new(2, engine.clone(), None);
        service
            .handle(Action::AddInput {
                source: lmix_types::SourceSpec::File {
                    path: "/media/clip.webm".into(),
                },
                name: None,
            })
            .await
            .unwrap();
        let mut events = service.subscribe();
        tokio::spawn(service.clone().watch_players());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        engine.report(crate::clips::PlayerProgress {
            flow: crate::engine::flows::slot_id(0),
            position_ms: 1500,
            duration_ms: 9000,
            index: 0,
            items: 1,
            state: Some(lmix_types::PlaybackState::Paused),
        });

        let id = service.state().await.inputs[0].id;
        let position = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            loop {
                if let Ok(Event::Position {
                    input,
                    position_ms,
                    duration_ms,
                }) = events.recv().await
                {
                    return (input, position_ms, duration_ms);
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(position, (id, 1500, 9000));
        // The state reads the last report too (API.4).
        assert_eq!(
            service.state().await.inputs[0].playback,
            Some(lmix_types::Playback {
                state: Some(lmix_types::PlaybackState::Paused),
                position_ms: 1500,
                duration_ms: 9000,
                item: 1,
                items: 1,
            })
        );
    }

    #[tokio::test]
    async fn inputs_of_another_show_are_appended() {
        use lmix_types::{InputRef, SourceSpec, TestPattern};
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("other.lmix");
        let add = |name: &str| Action::AddInput {
            source: SourceSpec::Pattern {
                pattern: TestPattern::Bars,
            },
            name: Some(name.into()),
        };
        let other = Service::new(4, Arc::new(NullEngine::default()), None);
        for name in ["Cam 1", "Cam 2"] {
            other.handle(add(name)).await.unwrap();
        }
        other
            .handle(Action::SetVolume {
                input: InputRef::Number(2),
                volume: 40,
            })
            .await
            .unwrap();
        other
            .handle(Action::SaveShow {
                path: Some(file.display().to_string()),
            })
            .await
            .unwrap();

        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.handle(add("Here")).await.unwrap();
        let state = service
            .handle(Action::AppendShow {
                path: file.display().to_string(),
                inputs: vec![2],
            })
            .await
            .unwrap();
        let names: Vec<_> = state.inputs.iter().map(|i| i.name.as_str()).collect();
        assert_eq!(names, ["Here", "Cam 2"]);
        assert_eq!(state.inputs[1].audio.volume, 40);
        assert_ne!(state.inputs[1].id, other.state().await.inputs[1].id);
        // Its sound set as the mix says, in its slot.
        let slot = state.inputs[1].slot;
        assert!(engine
            .log()
            .iter()
            .any(|c| matches!(c, Command::SetAudio { slot: s, .. } if *s == slot)));
        // Again: a second name, not a second "Cam 2".
        let state = service
            .handle(Action::AppendShow {
                path: file.display().to_string(),
                inputs: vec![2],
            })
            .await
            .unwrap();
        assert_eq!(state.inputs[2].name, "Cam 2 (2)");

        // All of it does not fit the one slot left: nothing is added.
        let refused = service
            .handle(Action::AppendShow {
                path: file.display().to_string(),
                inputs: vec![],
            })
            .await;
        assert!(matches!(refused, Err(ActionError::SlotsFull { .. })));
        assert!(matches!(
            service
                .handle(Action::AppendShow {
                    path: file.display().to_string(),
                    inputs: vec![9],
                })
                .await,
            Err(ActionError::UnknownInput { .. })
        ));
        assert_eq!(service.state().await.inputs.len(), 3);
        // Not a show file at all.
        assert!(service
            .handle(Action::AppendShow {
                path: dir.path().display().to_string(),
                inputs: vec![],
            })
            .await
            .is_err());
    }

    #[tokio::test]
    async fn missing_media_are_marked_and_found_again() {
        use lmix_types::SourceSpec;
        let dir = tempfile::tempdir().unwrap();
        let moved = dir.path().join("moved");
        std::fs::create_dir(&moved).unwrap();
        std::fs::write(moved.join("intro.mp4"), b"x").unwrap();
        let there = dir.path().join("there.mp4");
        std::fs::write(&there, b"x").unwrap();
        let gone = dir
            .path()
            .join("old")
            .join("intro.mp4")
            .display()
            .to_string();

        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        for (name, source) in [
            ("Intro", SourceSpec::File { path: gone.clone() }),
            (
                "Stream",
                SourceSpec::File {
                    path: "https://example.org/live.m3u8".into(),
                },
            ),
            (
                "List",
                SourceSpec::Playlist {
                    paths: vec![there.display().to_string(), gone.clone()],
                },
            ),
        ] {
            service
                .handle(Action::AddInput {
                    source,
                    name: Some(name.into()),
                })
                .await
                .unwrap();
        }
        service.check_media().await;
        let state = service.state().await;
        assert_eq!(state.inputs[0].missing, std::slice::from_ref(&gone));
        assert!(state.inputs[1].missing.is_empty(), "an address is no file");
        assert_eq!(state.inputs[2].missing, std::slice::from_ref(&gone));

        assert!(service
            .handle(Action::RelocateMedia {
                folder: dir.path().join("nothing").display().to_string(),
            })
            .await
            .is_err());
        // On program: left alone (a new source would restart it).
        service
            .handle(Action::CutDirect {
                input: lmix_types::InputRef::Number(1),
            })
            .await
            .unwrap();
        let relocate = || Action::RelocateMedia {
            folder: moved.display().to_string(),
        };
        service.handle(relocate()).await.unwrap();
        let found = moved.join("intro.mp4").display().to_string();
        let state = service.state().await;
        assert_eq!(
            state.inputs[0].source,
            SourceSpec::File { path: gone.clone() }
        );
        assert!(
            state.inputs[2].missing.is_empty(),
            "found ones are no longer missing"
        );
        assert!(
            service.handle(relocate()).await.is_err(),
            "only the one on program is left"
        );
        service
            .handle(Action::CutDirect {
                input: lmix_types::InputRef::Number(2),
            })
            .await
            .unwrap();
        service.handle(relocate()).await.unwrap();
        service.check_media().await;
        let state = service.state().await;
        assert_eq!(
            state.inputs[0].source,
            SourceSpec::File {
                path: found.clone()
            }
        );
        assert_eq!(
            state.inputs[2].source,
            SourceSpec::Playlist {
                paths: vec![there.display().to_string(), found]
            }
        );
        assert!(state.inputs.iter().all(|i| i.missing.is_empty()));
    }

    #[tokio::test]
    async fn a_jump_back_is_no_clip_end() {
        use lmix_types::{InputRef, SourceSpec, Trigger, TriggerOn};
        let engine = Arc::new(NullEngine::playing());
        let service = Service::new(2, engine.clone(), None);
        service
            .handle(Action::AddInput {
                source: SourceSpec::File {
                    path: "/media/clip.webm".into(),
                },
                name: None,
            })
            .await
            .unwrap();
        service
            .handle(Action::SetTriggers {
                input: InputRef::Number(1),
                triggers: vec![Trigger {
                    on: TriggerOn::ClipEnd,
                    delay_ms: 0,
                    before_ms: None,
                    at_ms: None,
                    actions: vec![Action::FadeToBlack { duration_ms: 0 }],
                }],
            })
            .await
            .unwrap();
        tokio::spawn(service.clone().watch_players());
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let report = |position_ms| crate::clips::PlayerProgress {
            flow: crate::engine::flows::slot_id(0),
            position_ms,
            duration_ms: 10_000,
            index: 0,
            items: 1,
            state: None,
        };
        let settle = || tokio::time::sleep(std::time::Duration::from_millis(100));

        engine.report(report(8_000));
        settle().await;
        service
            .handle(Action::SeekInput {
                input: InputRef::Number(1),
                position_ms: 1_000,
            })
            .await
            .unwrap();
        engine.report(report(1_000));
        settle().await;
        assert!(!service.state().await.fade_to_black, "the jump is no end");

        // Played on to the end and round again: that is one.
        tokio::time::sleep(JUMP_SETTLE).await;
        engine.report(report(9_900));
        settle().await;
        engine.report(report(100));
        settle().await;
        assert!(service.state().await.fade_to_black);
    }

    #[tokio::test]
    async fn save_without_path_fails() {
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        let err = service
            .handle(Action::SaveShow { path: None })
            .await
            .unwrap_err();
        assert_eq!(err, ActionError::NoShowPath);
    }

    /// RUN.9: a locked way in refuses actions, a panic still goes
    /// through, and other ways in are not touched.
    #[tokio::test]
    async fn a_locked_way_in_refuses_all_but_a_panic() {
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        let lock = |on| Action::SetIoLock {
            class: IoClass::Osc,
            on,
        };
        let state = service.handle(lock(true)).await.unwrap();
        assert_eq!(state.io_locked, [IoClass::Osc]);

        assert_eq!(
            service
                .handle_from(Action::SetLock { on: false }, IoClass::Osc)
                .await
                .unwrap_err(),
            ActionError::IoLocked {
                class: IoClass::Osc
            }
        );
        assert!(service
            .handle_from(Action::SetLock { on: false }, IoClass::Tcp)
            .await
            .is_ok());
        assert!(service
            .handle_from(Action::CuePanic { ms: None }, IoClass::Osc)
            .await
            .is_ok());
        assert!(
            service
                .handle_from(lock(false), IoClass::Osc)
                .await
                .is_err(),
            "not unlocked from the way it locks"
        );
        service.handle(lock(false)).await.unwrap();
        assert!(service
            .handle_from(Action::SetLock { on: false }, IoClass::Osc)
            .await
            .is_ok());
    }

    /// RUN.9: MIDI locked refuses a pad, but a pad bound to the panic
    /// still stops the show (over MIDI a panic only comes as a shortcut).
    #[tokio::test]
    async fn a_midi_panic_pad_goes_through_the_lock() {
        use lmix_types::{MidiBinding, MidiKind, MidiMessage, ShortcutSpec};
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        for (number, label, actions) in [
            (36, "Lock", vec![Action::SetLock { on: true }]),
            (37, "Panic", vec![Action::CuePanic { ms: None }]),
        ] {
            let shortcut = ShortcutSpec {
                all_shows: false,
                label: label.into(),
                key: None,
                web: false,
                color: None,
                midi: Some(MidiBinding {
                    kind: MidiKind::Note,
                    channel: None,
                    number,
                }),
                actions,
            };
            service
                .handle(Action::AddShortcut { shortcut })
                .await
                .unwrap();
        }
        service
            .handle(Action::SetIoLock {
                class: IoClass::Midi,
                on: true,
            })
            .await
            .unwrap();
        let pad = |number| MidiMessage {
            kind: MidiKind::Note,
            channel: 1,
            number,
            value: 100,
        };

        assert_eq!(
            service.midi(pad(36), true).await.unwrap_err(),
            ActionError::IoLocked {
                class: IoClass::Midi
            }
        );
        assert!(!service.state().await.locked);
        service.midi(pad(37), true).await.unwrap();
    }

    /// RUN.9: with the targets locked a request is not sent, says so as
    /// an event, and the cues that send warn.
    #[tokio::test]
    async fn locked_targets_send_nothing_and_warn() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        service
            .handle(Action::SetTarget {
                target: lmix_types::TargetSpec {
                    name: "Haus".into(),
                    kind: lmix_types::TargetKind::Http,
                    addresses: vec![format!("http://127.0.0.1:{port}")],
                    secret: None,
                    timeout_ms: 500,
                },
            })
            .await
            .unwrap();
        let light = Action::HaService {
            target: "Haus".into(),
            service: "light.turn_on".into(),
            entity: None,
            data: None,
        };
        service
            .handle(Action::AddCueList {
                name: "Show".into(),
                cues: vec![lmix_types::Cue::actions("Licht", vec![light.clone()])],
                source: None,
            })
            .await
            .unwrap();
        let mut events = service.subscribe();

        let state = service
            .handle(Action::SetIoLock {
                class: IoClass::Targets,
                on: true,
            })
            .await
            .unwrap();
        let problems: Vec<_> = state.cue_warnings.iter().map(|w| &w.problem).collect();
        assert!(
            matches!(problems[..], [lmix_types::cue::CueProblem::TargetLocked { target }] if target == "Haus"),
            "{problems:?}"
        );

        service.handle(light).await.unwrap();
        let result = loop {
            if let Event::Target { result, .. } = events.recv().await.unwrap() {
                break result;
            }
        };
        assert_eq!(result, lmix_types::TargetResult::Locked);
        let accepted =
            tokio::time::timeout(std::time::Duration::from_millis(300), listener.accept()).await;
        assert!(accepted.is_err(), "the request went out");
        assert_eq!(
            service
                .handle(Action::TestTarget {
                    name: "Haus".into()
                })
                .await
                .unwrap_err(),
            ActionError::IoLocked {
                class: IoClass::Targets
            }
        );
        // "Check targets" probes nothing either.
        service.handle(Action::CheckTargets).await.unwrap();
        let accepted =
            tokio::time::timeout(std::time::Duration::from_millis(700), listener.accept()).await;
        assert!(accepted.is_err(), "the check went out");
    }
}
