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

use std::path::PathBuf;
use std::sync::Arc;

use lmix_types::{Action, ActionError, Event, InputLevel, InputRef, InputStatus, Levels, ShowState};
use tokio::sync::{broadcast, Mutex};

use crate::engine::{meters, Engine, EngineError, Picture};
use crate::recent::Recent;
use crate::show::{self, Show};
use crate::switcher::{Command, Switcher};

/// Events kept for slow subscribers before they miss some.
const EVENT_BUFFER: usize = 256;
/// At most this often a `Levels` event (strom measures every 100 ms).
const LEVELS_INTERVAL: std::time::Duration = std::time::Duration::from_millis(100);
/// Silence: also stands in for "no reading" (JSON has no infinity).
pub const LEVEL_FLOOR_DB: f32 = -100.0;

pub struct Service {
    switcher: Mutex<Switcher>,
    engine: Arc<dyn Engine>,
    events: broadcast::Sender<Event>,
    /// Written after every change, read back after a crash (SHW.5).
    autosave: Option<PathBuf>,
    recent: Mutex<Recent>,
}

impl Service {
    pub fn new(slots: usize, engine: Arc<dyn Engine>, autosave: Option<PathBuf>) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Arc::new(Self {
            switcher: Mutex::new(Switcher::new(slots)),
            engine,
            events,
            recent: Mutex::new(Recent::load(autosave.as_deref())),
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

        let show = show::load(path)
            .await
            .map_err(|e| ActionError::ShowFile { message: e.to_string() })?;
        let mut sw = self.switcher.lock().await;
        let commands = sw.replace(show, None);
        let result = self.execute(&mut sw, &commands).await;
        self.publish(&sw);
        result.map(|()| true)
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub async fn state(&self) -> ShowState {
        self.switcher.lock().await.state()
    }

    /// Latest frame of an input; the switcher lock is not held while the
    /// engine fetches it.
    pub async fn input_picture(&self, input: &InputRef) -> Result<Result<Vec<u8>, EngineError>, ActionError> {
        let slot = self.switcher.lock().await.slot_for(input)?;
        Ok(self.engine.picture(Picture::Slot(slot)).await)
    }

    pub async fn program_picture(&self) -> Result<Vec<u8>, EngineError> {
        self.engine.picture(Picture::Program).await
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
                peak_db: engine.slots.get(&i.slot).copied().and_then(fresh).unwrap_or(LEVEL_FLOOR_DB),
            })
            .collect();
        Levels {
            inputs,
            program_peak_db: engine.main.and_then(fresh),
        }
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
            let outcome = self.engine.run(command).await;
            mark_slot(sw, command, outcome.is_ok());

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

    fn publish(&self, sw: &Switcher) {
        // No subscribers is fine.
        let _ = self.events.send(Event::State { state: sw.state() });
        let _ = self.events.send(Event::Tally { tally: sw.tally() });
    }
}

/// Reflect a `SetSlot` outcome in the input's status.
fn mark_slot(sw: &mut Switcher, command: &Command, ok: bool) {
    let Command::SetSlot { slot, source: Some(_) } = command else {
        return;
    };
    let Some(input) = sw.show().inputs.iter().find(|i| i.slot == *slot) else {
        return;
    };

    let status = if ok { InputStatus::Online } else { InputStatus::Offline };
    sw.set_status(input.id, status);
}

async fn open(sw: &mut Switcher, path: String) -> Result<Vec<Command>, ActionError> {
    let show = show::load(&PathBuf::from(&path))
        .await
        .map_err(|e| ActionError::ShowFile { message: e.to_string() })?;
    Ok(sw.replace(show, Some(path)))
}

async fn save(sw: &mut Switcher, path: Option<String>) -> Result<(), ActionError> {
    let path = path
        .or_else(|| sw.path().map(str::to_string))
        .ok_or(ActionError::NoShowPath)?;

    show::save(sw.show(), &PathBuf::from(&path))
        .await
        .map_err(|e| ActionError::ShowFile { message: e.to_string() })?;
    sw.set_path(path);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::null::NullEngine;
    use lmix_types::{InputRef, SourceSpec, TestPattern};

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
        assert_eq!(engine.log().len(), 2, "slot, audio: {:?}", engine.log());
        assert!(matches!(events.recv().await.unwrap(), Event::State { .. }));
        assert!(matches!(events.recv().await.unwrap(), Event::Tally { .. }));
    }

    #[tokio::test]
    async fn save_and_open_fill_recent_shows() {
        let dir = tempfile::tempdir().unwrap();
        let autosave = dir.path().join("autosave.lmix");
        let path = dir.path().join("s.lmix").display().to_string();
        let service = Service::new(4, Arc::new(NullEngine::default()), Some(autosave));

        service.handle(Action::SaveShow { path: Some(path.clone()) }).await.unwrap();
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
            .handle(Action::PreviewInput { input: InputRef::Number(1) })
            .await
            .unwrap();
        service.handle(Action::SaveShow { path: Some(path.clone()) }).await.unwrap();
        let saved = service.state().await;

        let engine = Arc::new(NullEngine::default());
        let fresh = Service::new(4, engine.clone(), None);
        let opened = fresh.handle(Action::OpenShow { path }).await.unwrap();

        assert_eq!(opened.inputs, saved.inputs);
        assert_eq!(opened.preview, saved.preview);
        assert_eq!(engine.log().len(), 4, "slot, program, preview, audio: {:?}", engine.log());
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
        assert_eq!(engine.log().len(), 3, "slot, program, audio: {:?}", engine.log());
    }

    #[tokio::test]
    async fn resume_without_autosave_is_noop() {
        let dir = tempfile::tempdir().unwrap();
        let service = Service::new(4, Arc::new(NullEngine::default()), Some(dir.path().join("none.lmix")));
        assert!(!service.resume().await.unwrap());
    }

    #[tokio::test]
    async fn save_without_path_fails() {
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        let err = service.handle(Action::SaveShow { path: None }).await.unwrap_err();
        assert_eq!(err, ActionError::NoShowPath);
    }
}
