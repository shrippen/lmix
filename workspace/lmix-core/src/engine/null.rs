//! Engine that only records commands: for tests and for running the API
//! without a media engine (`--engine none`).

use async_trait::async_trait;
use std::sync::Mutex;

use super::{Engine, EngineError, Picture};
use crate::switcher::Command;

#[derive(Default)]
pub struct NullEngine {
    log: Mutex<Vec<Command>>,
}

impl NullEngine {
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

    async fn run(&self, command: &Command) -> Result<(), EngineError> {
        self.log.lock().expect("log lock").push(command.clone());
        Ok(())
    }

    async fn picture(&self, _picture: Picture) -> Result<Vec<u8>, EngineError> {
        Err(EngineError::NoPictures)
    }
}
