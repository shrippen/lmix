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
    /// Media player reports a test sends.
    #[cfg(test)]
    players: Option<tokio::sync::broadcast::Sender<crate::clips::PlayerProgress>>,
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
        let errors = self.errors.as_ref().expect("engine built with failing()");
        let _ = errors.send(error);
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

    /// Refuse (`true`) or run every following command.
    #[cfg(test)]
    pub fn refuse(&self, refusing: bool) {
        self.refusing
            .store(refusing, std::sync::atomic::Ordering::SeqCst);
    }

    /// Commands received so far.
    #[cfg(test)]
    pub fn log(&self) -> Vec<Command> {
        self.log.lock().expect("log lock").clone()
    }
}

#[async_trait]
impl Engine for NullEngine {
    async fn start(&self, _slots: usize) -> Result<(), EngineError> {
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
            let slow = self.slow.lock().expect("slow lock").take();
            if let Some((takes, error)) = slow {
                tokio::time::sleep(takes / 2).await;
                self.fail(error);
                tokio::time::sleep(takes / 2).await;
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
        false
    }

    async fn measure_latency(&self, _seconds: u32) -> Result<(), EngineError> {
        Ok(())
    }
}
