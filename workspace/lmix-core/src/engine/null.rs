//! Engine that only records commands: for tests and for running the API
//! without a media engine (`--engine none`).

use async_trait::async_trait;
use lmix_types::Device;
use std::sync::Mutex;

use super::{Engine, EngineError, Picture};
use crate::switcher::Command;

#[derive(Default)]
pub struct NullEngine {
    log: Mutex<Vec<Command>>,
    /// Never measures anything.
    levels: tokio::sync::watch::Sender<super::meters::EngineLevels>,
    errors: Option<tokio::sync::broadcast::Sender<super::meters::FlowError>>,
    /// Next command takes this long and reports this failure halfway
    /// (a stop that drains, as strom's `eos_on_stop` does).
    #[cfg(test)]
    slow: Mutex<Option<(std::time::Duration, super::meters::FlowError)>>,
    /// Engine process identity (`instance`); tests change it to restart.
    #[cfg(test)]
    instance: Mutex<Option<String>>,
    /// Refuse every command, as a fresh engine without the mixer does.
    #[cfg(test)]
    refusing: std::sync::atomic::AtomicBool,
    /// Refuse T-bar steps, as strom does while a take of its own runs.
    #[cfg(test)]
    refusing_t_bar: std::sync::atomic::AtomicBool,
    /// Refuse moving an output's sound, as strom does for a flow it lost.
    #[cfg(test)]
    refusing_output_sound: std::sync::atomic::AtomicBool,
    /// Media player reports a test sends.
    #[cfg(test)]
    players: Option<tokio::sync::broadcast::Sender<crate::clips::PlayerProgress>>,
    /// Stinger ends a test sends.
    #[cfg(test)]
    stingers: Option<tokio::sync::broadcast::Sender<super::meters::StingerEnd>>,
    /// Stinger takes run so far, as the engine's generations count them.
    #[cfg(test)]
    stinger_takes: std::sync::atomic::AtomicU64,
    /// Flows a test has stopped behind lmix-core's back.
    #[cfg(test)]
    stopped: Mutex<std::collections::HashSet<strom_types::FlowId>>,
    /// Commands running now, starts so far, and whether one came while a
    /// command ran.
    #[cfg(test)]
    commands_running: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    starts: std::sync::atomic::AtomicUsize,
    #[cfg(test)]
    start_during_command: std::sync::atomic::AtomicBool,
    /// Outputs started and not stopped or failed since.
    #[cfg(test)]
    running_outputs: Mutex<std::collections::HashSet<lmix_types::OutputId>>,
    /// The next query whether an output runs answers yes, then the output
    /// fails before the answer arrives (`fail_after_answer`).
    #[cfg(test)]
    failing_after_answer: std::sync::atomic::AtomicBool,
    /// Every output fails right after its start, as one on a busy card.
    #[cfg(test)]
    failing_outputs: std::sync::atomic::AtomicBool,
}

impl NullEngine {
    /// An engine whose media players a test can report for.
    #[cfg(test)]
    pub fn playing() -> Self {
        Self {
            players: Some(tokio::sync::broadcast::channel(16).0),
            ..Self::default()
        }
    }

    /// Report a media player's position as the engine would.
    #[cfg(test)]
    pub fn report(&self, progress: crate::clips::PlayerProgress) {
        if let Some(tx) = &self.players {
            let _ = tx.send(progress);
        }
    }

    /// An engine whose stingers a test can end.
    #[cfg(test)]
    pub fn stinging() -> Self {
        Self {
            stingers: Some(tokio::sync::broadcast::channel(16).0),
            ..Self::default()
        }
    }

    /// End the running stinger as the engine would.
    #[cfg(test)]
    pub fn end_stinger(&self, end: super::meters::StingerEnd) {
        let stingers = self
            .stingers
            .as_ref()
            .expect("engine built with stinging()");
        let _ = stingers.send(end);
    }

    /// An engine whose flows can be made to fail (tests).
    #[cfg(test)]
    pub fn failing() -> Self {
        Self {
            errors: Some(tokio::sync::broadcast::channel(16).0),
            ..Self::default()
        }
    }

    /// Report a flow failure as the engine would.
    #[cfg(test)]
    pub fn fail(&self, error: super::meters::FlowError) {
        self.running_outputs
            .lock()
            .expect("outputs lock")
            .retain(|id| super::flows::output_flow_id(*id) != error.flow);
        let errors = self.errors.as_ref().expect("engine built with failing()");
        let _ = errors.send(error);
    }

    /// Stop `flow` as something outside lmix-core would: it no longer
    /// runs.
    #[cfg(test)]
    pub fn stop_flow(&self, flow: strom_types::FlowId) {
        self.stopped.lock().expect("stopped lock").insert(flow);
    }

    /// How often the engine was started.
    #[cfg(test)]
    pub fn starts(&self) -> usize {
        self.starts.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Whether a start came while a slow command ran.
    #[cfg(test)]
    pub fn started_during_a_command(&self) -> bool {
        self.start_during_command
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// Make the next command slow, reporting `error` halfway through.
    #[cfg(test)]
    pub fn slow_next(&self, takes: std::time::Duration, error: super::meters::FlowError) {
        *self.slow.lock().expect("slow lock") = Some((takes, error));
    }

    /// Pretend the engine process is `instance` (a new one: restarted).
    #[cfg(test)]
    pub fn set_instance(&self, instance: &str) {
        *self.instance.lock().expect("instance lock") = Some(instance.into());
    }

    /// Let the next query whether an output runs answer yes and the output
    /// fail before the answer arrives, as strom's may.
    #[cfg(test)]
    pub fn fail_after_answer(&self) {
        self.failing_after_answer
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// Let every output fail right after its start (`true`) or run.
    #[cfg(test)]
    pub fn fail_outputs(&self, failing: bool) {
        self.failing_outputs
            .store(failing, std::sync::atomic::Ordering::SeqCst);
    }

    /// Refuse (`true`) or run every following command.
    #[cfg(test)]
    pub fn refuse(&self, refusing: bool) {
        self.refusing
            .store(refusing, std::sync::atomic::Ordering::SeqCst);
    }

    /// Refuse (`true`) or run every following T-bar step.
    #[cfg(test)]
    pub fn refuse_t_bar(&self, refusing: bool) {
        self.refusing_t_bar
            .store(refusing, std::sync::atomic::Ordering::SeqCst);
    }

    /// Refuse (`true`) or run every following change of an output's sound.
    #[cfg(test)]
    pub fn refuse_output_sound(&self, refusing: bool) {
        self.refusing_output_sound
            .store(refusing, std::sync::atomic::Ordering::SeqCst);
    }

    /// Commands received so far.
    #[cfg(test)]
    pub fn log(&self) -> Vec<Command> {
        self.log.lock().expect("log lock").clone()
    }
}

/// How long after its start a failing output fails (`fail_outputs`).
#[cfg(test)]
const FAILING_OUTPUT_AFTER: std::time::Duration = std::time::Duration::from_millis(1500);

#[cfg(test)]
impl NullEngine {
    /// Note an output's start or stop; with `fail_outputs` a started one
    /// fails a moment later.
    fn track_output(&self, command: &Command) {
        let mut running = self.running_outputs.lock().expect("outputs lock");
        match command {
            Command::StartOutput { id, .. } => {
                if !self
                    .failing_outputs
                    .load(std::sync::atomic::Ordering::SeqCst)
                {
                    running.insert(*id);
                    return;
                }
                // strom reports it a moment after the start succeeded.
                let errors = self.errors.clone().expect("engine built with failing()");
                let error = super::meters::FlowError {
                    flow: super::flows::output_flow_id(*id),
                    message: "card busy".into(),
                    kind: super::meters::FlowFailure::Error,
                };
                tokio::spawn(async move {
                    tokio::time::sleep(FAILING_OUTPUT_AFTER).await;
                    let _ = errors.send(error);
                });
            }
            Command::StopOutput { id } => {
                running.remove(id);
            }
            _ => {}
        }
    }
}

#[async_trait]
impl Engine for NullEngine {
    async fn start(&self, _slots: usize) -> Result<(), EngineError> {
        #[cfg(test)]
        {
            use std::sync::atomic::Ordering::SeqCst;
            self.starts.fetch_add(1, SeqCst);
            if self.commands_running.load(SeqCst) > 0 {
                self.start_during_command.store(true, SeqCst);
            }
        }
        Ok(())
    }

    fn players(&self) -> tokio::sync::broadcast::Receiver<crate::clips::PlayerProgress> {
        #[cfg(test)]
        if let Some(tx) = &self.players {
            return tx.subscribe();
        }
        tokio::sync::broadcast::channel(1).1
    }

    #[cfg(test)]
    fn stinger_generation(&self) -> u64 {
        self.stinger_takes.load(std::sync::atomic::Ordering::SeqCst)
    }

    fn stingers(&self) -> tokio::sync::broadcast::Receiver<super::meters::StingerEnd> {
        #[cfg(test)]
        if let Some(tx) = &self.stingers {
            return tx.subscribe();
        }
        tokio::sync::broadcast::channel(1).1
    }

    #[cfg(test)]
    async fn instance(&self) -> Option<String> {
        self.instance.lock().expect("instance lock").clone()
    }

    async fn run(&self, command: &Command) -> Result<(), EngineError> {
        self.log.lock().expect("log lock").push(command.clone());
        #[cfg(test)]
        {
            if self.refusing.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(EngineError::Refused("no mixer yet".into()));
            }
            if matches!(command, Command::Stinger { .. }) {
                self.stinger_takes
                    .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            if matches!(command, Command::TBar { .. })
                && self
                    .refusing_t_bar
                    .load(std::sync::atomic::Ordering::SeqCst)
            {
                return Err(EngineError::Refused("a take is running".into()));
            }
            if matches!(command, Command::SetOutputSound { .. })
                && self
                    .refusing_output_sound
                    .load(std::sync::atomic::Ordering::SeqCst)
            {
                return Err(EngineError::Refused("pipeline not running".into()));
            }
            self.track_output(command);
            let slow = self.slow.lock().expect("slow lock").take();
            if let Some((takes, error)) = slow {
                use std::sync::atomic::Ordering::SeqCst;
                self.commands_running.fetch_add(1, SeqCst);
                tokio::time::sleep(takes / 2).await;
                self.fail(error);
                tokio::time::sleep(takes / 2).await;
                self.commands_running.fetch_sub(1, SeqCst);
            }
        }
        Ok(())
    }

    async fn picture(&self, _picture: Picture) -> Result<Vec<u8>, EngineError> {
        Err(EngineError::NoPictures)
    }

    async fn devices(&self) -> Result<Vec<Device>, EngineError> {
        Ok(Vec::new())
    }

    fn levels(&self) -> tokio::sync::watch::Receiver<super::meters::EngineLevels> {
        self.levels.subscribe()
    }

    fn flow_errors(&self) -> tokio::sync::broadcast::Receiver<super::meters::FlowError> {
        match &self.errors {
            Some(errors) => errors.subscribe(),
            None => tokio::sync::broadcast::channel(1).1,
        }
    }

    async fn output_running(&self, _id: lmix_types::OutputId) -> bool {
        #[cfg(test)]
        {
            let running = self
                .running_outputs
                .lock()
                .expect("outputs lock")
                .contains(&_id);
            if self
                .failing_after_answer
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                self.fail(super::meters::FlowError {
                    flow: super::flows::output_flow_id(_id),
                    message: "card lost".into(),
                    kind: super::meters::FlowFailure::Error,
                });
                // The failure goes round before the answer comes back.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            }
            return running;
        }
        #[cfg(not(test))]
        false
    }

    #[cfg(test)]
    async fn flow_running(&self, flow: strom_types::FlowId) -> bool {
        !self.stopped.lock().expect("stopped lock").contains(&flow)
    }

    async fn measure_latency(&self, _seconds: u32) -> Result<(), EngineError> {
        Ok(())
    }
}
