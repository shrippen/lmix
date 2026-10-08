//! Engine drivers: turn switcher [`Command`]s into media-engine calls.
//!
//! The rest of lmix-core only sees this trait; strom's REST API stays
//! inside `strom.rs` (layer rule: no holes through the driver layer).

use async_trait::async_trait;

use crate::switcher::Command;

pub mod null;
pub mod strom;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("engine unreachable: {0}")]
    Unreachable(String),
    #[error("engine refused: {0}")]
    Refused(String),
}

#[async_trait]
pub trait Engine: Send + Sync {
    /// Build the mixer with `slots` inputs; idempotent.
    async fn start(&self, slots: usize) -> Result<(), EngineError>;

    /// Carry out one command.
    async fn run(&self, command: &Command) -> Result<(), EngineError>;
}
