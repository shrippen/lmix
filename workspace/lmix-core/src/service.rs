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

use lmix_types::{Action, ActionError, Event, InputStatus, ShowState};
use tokio::sync::{broadcast, Mutex};

use crate::engine::Engine;
use crate::show::{self, Show};
use crate::switcher::{Command, Switcher};

/// Events kept for slow subscribers before they miss some.
const EVENT_BUFFER: usize = 256;

pub struct Service {
    switcher: Mutex<Switcher>,
    engine: Arc<dyn Engine>,
    events: broadcast::Sender<Event>,
}

impl Service {
    pub fn new(slots: usize, engine: Arc<dyn Engine>) -> Arc<Self> {
        let (events, _) = broadcast::channel(EVENT_BUFFER);
        Arc::new(Self {
            switcher: Mutex::new(Switcher::new(slots)),
            engine,
            events,
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.events.subscribe()
    }

    pub async fn state(&self) -> ShowState {
        self.switcher.lock().await.state()
    }

    pub async fn tally(&self) -> Vec<u8> {
        self.switcher.lock().await.tally()
    }

    /// Run one action to the end; publishes the new state.
    pub async fn handle(&self, action: Action) -> Result<ShowState, ActionError> {
        let mut sw = self.switcher.lock().await;

        let commands = match action {
            Action::NewShow => sw.replace(Show::default(), None),
            Action::OpenShow { path } => open(&mut sw, path).await?,
            Action::SaveShow { path } => {
                save(&mut sw, path).await?;
                Vec::new()
            }
            other => sw.apply(other)?,
        };

        let result = self.execute(&mut sw, &commands).await;
        self.publish(&sw);
        result.map(|()| sw.state())
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
        let service = Service::new(4, engine.clone());
        let mut events = service.subscribe();

        let state = service.handle(bars()).await.unwrap();

        assert_eq!(state.inputs[0].status, InputStatus::Online);
        assert_eq!(engine.log().len(), 1);
        assert!(matches!(events.recv().await.unwrap(), Event::State { .. }));
        assert!(matches!(events.recv().await.unwrap(), Event::Tally { .. }));
    }

    #[tokio::test]
    async fn save_open_restores_identical_show() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("s.lmix").display().to_string();
        let service = Service::new(4, Arc::new(NullEngine::default()));
        service.handle(bars()).await.unwrap();
        service
            .handle(Action::PreviewInput { input: InputRef::Number(1) })
            .await
            .unwrap();
        service.handle(Action::SaveShow { path: Some(path.clone()) }).await.unwrap();
        let saved = service.state().await;

        let engine = Arc::new(NullEngine::default());
        let fresh = Service::new(4, engine.clone());
        let opened = fresh.handle(Action::OpenShow { path }).await.unwrap();

        assert_eq!(opened.inputs, saved.inputs);
        assert_eq!(opened.preview, saved.preview);
        assert_eq!(engine.log().len(), 2, "slot + preview: {:?}", engine.log());
    }

    #[tokio::test]
    async fn save_without_path_fails() {
        let service = Service::new(4, Arc::new(NullEngine::default()));
        let err = service.handle(Action::SaveShow { path: None }).await.unwrap_err();
        assert_eq!(err, ActionError::NoShowPath);
    }
}
