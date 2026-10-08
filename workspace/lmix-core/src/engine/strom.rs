//! strom driver (M2b step 2). Placeholder until the flow builder lands.

use async_trait::async_trait;

use super::{Engine, EngineError};
use crate::switcher::Command;

pub struct StromEngine {
    base_url: String,
}

impl StromEngine {
    pub fn new(base_url: String) -> Self {
        Self { base_url }
    }
}

#[async_trait]
impl Engine for StromEngine {
    async fn start(&self, _slots: usize) -> Result<(), EngineError> {
        Err(EngineError::Unreachable(format!(
            "strom driver not implemented yet ({})",
            self.base_url
        )))
    }

    async fn run(&self, _command: &Command) -> Result<(), EngineError> {
        Err(EngineError::Refused("strom driver not implemented yet".into()))
    }
}
