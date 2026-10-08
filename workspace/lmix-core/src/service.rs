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
use std::sync::Arc;
use std::time::Instant;

use lmix_types::{
    Action, ActionError, Event, InputLevel, InputRef, InputStatus, Latency, Levels, OutputId,
    OutputSpec, OutputStats, OutputStatus, ShowState, SignalLoss,
};
use strom_types::FlowId;
use tokio::sync::{broadcast, Mutex};

use crate::engine::flows::{output_flow_id, slot_id};
use crate::engine::meters::{FlowError, FlowFailure};
use crate::engine::{meters, Engine, EngineError, Picture};
use crate::recent::Recent;
use crate::show::{self, Show};
use crate::switcher::{Command, Switcher};

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
}

impl Service {
    pub fn new(slots: usize, engine: Arc<dyn Engine>, autosave: Option<PathBuf>) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Arc::new(Self {
            switcher: Mutex::new(Switcher::new(slots)),
            engine,
            events,
            recent: Mutex::new(Recent::load(autosave.as_deref())),
            pictures: std::sync::Mutex::new(HashMap::new()),
            touched: std::sync::Mutex::new(HashMap::new()),
            failing_back: std::sync::Mutex::new(HashSet::new()),
            output_stats: std::sync::Mutex::new(Vec::new()),
            autosave,
        })
    }

    /// Reopen the autosave of the last run, if any. The show keeps no file
    /// path, so a later save asks for one.
    pub async fn resume(&self) -> Result<bool, ActionError> {
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

    /// Run one action to the end; publishes the new state.
    pub async fn handle(&self, action: Action) -> Result<ShowState, ActionError> {
        let mut sw = self.switcher.lock().await;
        let file_action = matches!(action, Action::OpenShow { .. } | Action::SaveShow { .. });

        let commands = match action {
            Action::NewShow => sw.replace(Show::default(), None),
            Action::OpenShow { path } => open(&mut sw, path).await?,
            Action::SaveShow { path } => {
                save(&mut sw, path).await?;
                Vec::new()
            }
            other => sw.apply(other)?,
        };
        if let (true, Some(path)) = (file_action, sw.path().map(str::to_string)) {
            self.recent.lock().await.touch(&path).await;
        }

        let result = self.execute(&mut sw, &commands).await;
        self.publish(&sw);
        self.write_autosave(&sw).await;
        result.map(|()| sw.state())
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
        let commands = sw.restore_commands();
        let result = self.execute(&mut sw, &commands).await;
        self.publish(&sw);
        result
    }

    /// Run commands in order. A failing slot marks its input offline and
    /// the rest still run; the first error is returned.
    async fn execute(&self, sw: &mut Switcher, commands: &[Command]) -> Result<(), ActionError> {
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
    use lmix_types::{InputRef, SourceSpec, TestPattern};

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
    async fn save_without_path_fails() {
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        let err = service
            .handle(Action::SaveShow { path: None })
            .await
            .unwrap_err();
        assert_eq!(err, ActionError::NoShowPath);
    }
}
