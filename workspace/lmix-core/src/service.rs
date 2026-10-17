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

use lmix_types::{
    Action, ActionError, Event, InputId, InputLevel, InputRef, InputStatus, Latency, Levels,
    OutputId, OutputSpec, OutputStats, OutputStatus, ShowState, SignalLoss, Template,
};
use strom_types::FlowId;
use tokio::sync::{broadcast, Mutex, Notify};

use crate::engine::flows::{output_flow_id, slot_id};
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

/// Events kept for slow subscribers before they miss some.
const EVENT_BUFFER: usize = 256;
/// At most this often a `Levels` event (strom measures every 100 ms).
const LEVELS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
/// How often network outputs report their link health.
const OUTPUT_STATS_INTERVAL: std::time::Duration = std::time::Duration::from_secs(1);
/// Silence: also stands in for "no reading" (JSON has no infinity).
pub const LEVEL_FLOOR_DB: f32 = -100.0;

pub struct Service {
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
        Arc::new(Self {
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
        let read = read_settings(&path, "shortcuts for all shows", |bytes| {
            serde_json::from_slice::<Vec<crate::show::ShowShortcut>>(bytes)
                .map_err(|e| e.to_string())
        })
        .await;
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
    ) {
        let service = Arc::clone(self);
        tokio::spawn(async move {
            let result = crate::targets::send(&service.target_http, &target, &request).await;
            let reached = match &result {
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
        let commands = sw.replace(show, None);
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

    /// Live picture socket of an input's slot (see [`Engine::live`]).
    /// Live picture of an input: tile size, or as it arrives (`full`).
    pub async fn live_input(
        &self,
        input: &InputRef,
        full: bool,
    ) -> Result<Result<std::path::PathBuf, EngineError>, ActionError> {
        let slot = self.switcher.lock().await.slot_for(input)?;
        let picture = if full {
            LivePicture::SlotFull(slot)
        } else {
            LivePicture::Slot(slot)
        };
        Ok(self.engine.live(picture).await)
    }

    pub async fn live_program(&self) -> Result<std::path::PathBuf, EngineError> {
        self.engine.live(LivePicture::Program).await
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
        Levels {
            inputs,
            program_peak_db: engine.main.and_then(fresh),
            monitor_peak_db: engine.monitor.and_then(fresh),
            aux_peak_db: std::array::from_fn(|bus| engine.aux.get(&bus).copied().and_then(fresh)),
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
            let own_change = self
                .touched
                .lock()
                .expect("touched lock")
                .get(&error.flow)
                .is_some_and(Touch::is_recent);
            if error.kind == FlowFailure::Stopped && own_change {
                continue;
            }
            if self.output_failed(&error).await {
                continue;
            }
            self.slot_failed(&error).await;
        }
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
                message: error.message.clone(),
            },
        );
        self.publish(&sw);
        drop(sw);

        if self
            .failing_back
            .lock()
            .expect("failback lock")
            .insert(error.flow)
        {
            tokio::spawn(self.clone().restart_output(id));
        }
        true
    }

    /// Start a failed output again until it runs, or the operator stopped
    /// or removed it.
    async fn restart_output(self: Arc<Self>, id: OutputId) {
        let mut wait = FAILBACK_FIRST;
        loop {
            tokio::time::sleep(wait).await;
            wait = (wait * 2).min(FAILBACK_MAX);
            let mut sw = self.switcher.lock().await;
            let Some(command) = sw.restart_command(id) else {
                break;
            };
            if self.execute(&mut sw, &[command]).await.is_ok() {
                tracing::info!(output = %id, "output back");
                self.publish(&sw);
                break;
            }
        }
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
            Some(action) => self.handle_as(action, Origin::Held(hold)).await.map(|_| ()),
            None => Ok(()),
        }
    }

    /// GO from a button whose release is reported (`held`, RUN.6): OSC.
    pub async fn go_held(
        self: &Arc<Self>,
        action: Action,
        held: crate::cues::Hold,
    ) -> Result<ShowState, ActionError> {
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
                let config = self.switcher.lock().await.target(target)?;
                let request = crate::targets::Request {
                    method: *method,
                    path: path.clone(),
                    headers: headers.clone(),
                    body: body.clone(),
                };
                self.send_to_target(config, request);
                return Ok(self.state().await);
            }
            Action::HaService {
                target,
                service,
                entity,
                data,
            } => {
                let config = self.switcher.lock().await.target(target)?;
                let request =
                    crate::targets::ha_request(service, entity.as_deref(), data.as_ref())?;
                self.send_to_target(config, request);
                return Ok(self.state().await);
            }
            Action::TestTarget { name } => {
                let config = self.switcher.lock().await.target(name)?;
                let result = crate::targets::probe(&self.target_http, &config).await;
                self.reached(name, result.clone()).await;
                result.map_err(|reason| ActionError::BadTarget { reason })?;
                return Ok(self.state().await);
            }
            Action::CheckTargets => {
                let used = self.switcher.lock().await.used_targets();
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
            | Action::CueStop { .. }
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
                        Action::CueStop { list, cue, .. } => {
                            sw.cue_stop(list.as_ref(), cue.as_deref(), now)?
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
            Action::SetCueList { .. } | Action::RemoveCueList { .. }
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
                sw.replace(show, None)
            }
            Action::OpenShow { path } => open(&mut sw, path).await?,
            Action::ImportPackage { .. } => {
                let (show, file, settings) = unpacked.take().expect("unpacked above");
                if let Some(settings) = settings {
                    sw.merge_target_settings(settings)?;
                }
                sw.replace(show, Some(file.display().to_string()))
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
        if !self.engine_ready.load(Ordering::SeqCst) {
            // Outputs keep what was asked, so restore starts them.
            for command in commands {
                mark_output(sw, command, &Ok(()));
            }
            return Ok(());
        }
        let mut first_error = None;

        for command in commands {
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
                self.touched
                    .lock()
                    .expect("touched lock")
                    .insert(flow, Touch::Done(Instant::now()));
            }
            mark_slot(sw, command, outcome.is_ok());
            mark_output(sw, command, &outcome);

            let Err(e) = outcome else {
                continue;
            };
            tracing::warn!(?command, error = %e, "engine command failed");
            first_error.get_or_insert(ActionError::Engine {
                message: e.to_string(),
            });
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

/// lmix-core's last change of a flow.
#[derive(Clone, Copy, Debug)]
enum Touch {
    /// A command on it is still running.
    Running,
    Done(Instant),
}

impl Touch {
    fn is_recent(&self) -> bool {
        match self {
            Touch::Running => true,
            Touch::Done(at) => at.elapsed() < OWN_STOP_WINDOW,
        }
    }
}

/// Engine flow a command builds or stops.
fn flow_of(command: &Command) -> Option<FlowId> {
    match command {
        Command::SetSlot { slot, .. } => Some(slot_id(*slot)),
        Command::StartOutput { id, .. } | Command::StopOutput { id } => Some(output_flow_id(*id)),
        _ => None,
    }
}

/// Reflect a start or stop in the output's status.
fn mark_output(sw: &mut Switcher, command: &Command, outcome: &Result<(), EngineError>) {
    let (id, status) = match (command, outcome) {
        (Command::StartOutput { id, .. }, Ok(())) => {
            (*id, OutputStatus::Running { since_ms: now_ms() })
        }
        (Command::StopOutput { id }, Ok(())) => (*id, OutputStatus::Stopped),
        (Command::StartOutput { id, .. } | Command::StopOutput { id }, Err(e)) => (
            *id,
            OutputStatus::Failed {
                message: e.to_string(),
            },
        ),
        _ => return,
    };
    sw.set_output_status(id, status);
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// Reflect a `SetSlot` outcome in the input's status.
fn mark_slot(sw: &mut Switcher, command: &Command, ok: bool) {
    let Command::SetSlot {
        slot,
        source: Some(_),
    } = command
    else {
        return;
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
    let commands = sw.replace(loaded.show, Some(path));
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
    async fn a_stinger_cuts_and_ends_later_without_holding_the_switcher() {
        use lmix_types::{action::StingerSpec, Effect, InputRef, TransitionSlot};
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        let clip = SourceSpec::File {
            path: "sting.webm".into(),
        };
        service
            .handle(Action::AddInput {
                source: clip,
                name: Some("Sting".into()),
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
        let cut = || {
            engine
                .log()
                .iter()
                .any(|c| matches!(c, Command::Program { slot: Some(0), .. }))
        };
        let ended = || {
            engine
                .log()
                .iter()
                .any(|c| matches!(c, Command::Stinger { on: false, .. }))
        };

        service
            .handle(Action::Transition {
                slot: TransitionSlot(1),
            })
            .await
            .unwrap();
        assert!(!cut(), "no cut before the cut point");
        // The switcher stays free meanwhile.
        service.state().await;

        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        assert!(cut() && !ended());
        tokio::time::sleep(std::time::Duration::from_millis(700)).await;
        assert!(ended());
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
        service.handle(bars()).await.unwrap();
        assert_eq!(slot_commands(&engine), 3, "and actions run directly after");
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
                aux: None,
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
        service.handle(bars()).await.unwrap();
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
        assert!(new.contains("\"version\": 2"), "{new}");
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
}
