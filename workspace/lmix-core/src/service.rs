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
    /// Written after every change, read back after a crash (SHW.5).
    autosave: Option<PathBuf>,
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
        })
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
        self.switcher.lock().await.state()
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
            .map(|i| InputLevel {
                input: i.id,
                peak_db: engine
                    .slots
                    .get(&i.slot)
                    .copied()
                    .and_then(fresh)
                    .unwrap_or(LEVEL_FLOOR_DB),
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
            Some(action) => self.handle(action).await.map(|_| ()),
            None => Ok(()),
        }
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
    /// `Command::Wait` (a stinger's cut and end) run later in the
    /// background.
    pub async fn handle(self: &Arc<Self>, action: Action) -> Result<ShowState, ActionError> {
        self.handle_as(action, Origin::Operator).await
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
            Action::CueGo { list } | Action::CueBack { list } => {
                let back = matches!(action, Action::CueBack { .. });
                let actions = {
                    let mut sw = self.switcher.lock().await;
                    let actions = sw.cue_step(list, back)?;
                    self.publish(&sw);
                    actions
                };
                return self.run_all(actions, origin).await;
            }
            _ => {}
        }
        // Packages copy media (gigabytes): without the switcher locked.
        if let Action::ExportPackage { path } = &action {
            let (show, show_path) = {
                let sw = self.switcher.lock().await;
                (sw.show().clone(), sw.path().map(str::to_string))
            };
            let templates = self
                .templates_dir
                .lock()
                .expect("templates dir lock")
                .clone();
            let out = PathBuf::from(path);
            let packed = tokio::task::spawn_blocking(move || {
                crate::package::export(&show, show_path.as_deref(), templates.as_deref(), &out)
            })
            .await
            .map_err(|e| ActionError::ShowFile {
                message: e.to_string(),
            })?
            .map_err(|message| ActionError::ShowFile { message })?;
            tracing::info!(%path, files = packed.files, bytes = packed.bytes, "package written");
            return Ok(self.state().await);
        }
        let mut unpacked = match &action {
            Action::ImportPackage { path, folder } => {
                // Refused before gigabytes are unpacked (checked again below).
                self.switcher.lock().await.check_unlocked()?;
                let templates = self
                    .templates_dir
                    .lock()
                    .expect("templates dir lock")
                    .clone();
                let (package, folder) = (PathBuf::from(path), PathBuf::from(folder));
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
                Some((show, file))
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
        let mut sw = self.switcher.lock().await;
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
            Action::OpenShow { .. } | Action::NewShow | Action::ImportPackage { .. }
        ) {
            sw.check_unlocked()?;
        }

        let commands = match action {
            Action::NewShow => sw.replace(Show::default(), None),
            Action::OpenShow { path } => open(&mut sw, path).await?,
            Action::ImportPackage { .. } => {
                let (show, file) = unpacked.take().expect("unpacked above");
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
            other => sw.apply(other)?,
        };
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
                Origin::Operator,
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
        if !later.is_empty() {
            self.run_later(later.to_vec());
        }
        // Triggered actions fire no triggers themselves (no loops).
        if origin == Origin::Operator && result.is_ok() {
            for (delay_ms, actions) in sw.fired_triggers(&before) {
                self.run_triggered(delay_ms, actions);
            }
        }
        result.map(|()| sw.state())
    }

    /// Actions of a shortcut or cue in order; the first error stops them.
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
                if let Err(e) = service.execute(&mut sw, &tail[..end]).await {
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

/// Who started an action: triggers fire only for the operator's.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Origin {
    Operator,
    Trigger,
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
    let show = show::load(&PathBuf::from(&path))
        .await
        .map_err(|e| ActionError::ShowFile {
            message: e.to_string(),
        })?;
    Ok(sw.replace(show, Some(path)))
}

async fn save(sw: &mut Switcher, path: Option<String>) -> Result<(), ActionError> {
    let path = path
        .or_else(|| sw.path().map(str::to_string))
        .ok_or(ActionError::NoShowPath)?;

    show::save(sw.show(), &PathBuf::from(&path))
        .await
        .map_err(|e| ActionError::ShowFile {
            message: e.to_string(),
        })?;
    sw.set_path(path);
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

    #[tokio::test]
    async fn a_shortcut_runs_its_actions_in_order() {
        use lmix_types::{InputRef, ShortcutSpec};
        let engine = Arc::new(NullEngine::default());
        let service = Service::new(4, engine.clone(), None);
        service.handle(bars()).await.unwrap();
        let shortcut = ShortcutSpec {
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
