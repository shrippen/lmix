//! Engine drivers: turn switcher [`Command`]s into media-engine calls.
//!
//! The rest of lmix-core only sees this trait; strom's REST API stays
//! inside `strom.rs` (layer rule: no holes through the driver layer).

use async_trait::async_trait;
use lmix_types::Device;

use crate::switcher::Command;

pub mod flows;
pub mod meters;
pub mod null;
pub mod strom;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("engine unreachable: {0}")]
    Unreachable(String),
    #[error("engine refused: {0}")]
    Refused(String),
    #[error("no picture yet")]
    NotReady,
    #[error("engine has no pictures")]
    NoPictures,
}

/// Which picture to grab.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Picture {
    Program,
    Slot(usize),
}

#[async_trait]
pub trait Engine: Send + Sync {
    /// Build the mixer with `slots` inputs; idempotent.
    async fn start(&self, slots: usize) -> Result<(), EngineError>;

    /// Carry out one command.
    async fn run(&self, command: &Command) -> Result<(), EngineError>;

    /// Latest frame as JPEG.
    async fn picture(&self, picture: Picture) -> Result<Vec<u8>, EngineError>;

    /// Video capture devices the engine can open.
    async fn devices(&self) -> Result<Vec<Device>, EngineError>;

    /// Audio levels, updated as the engine measures them.
    fn levels(&self) -> tokio::sync::watch::Receiver<meters::EngineLevels>;

    /// Errors of running flows (outputs failing).
    fn flow_errors(&self) -> tokio::sync::broadcast::Receiver<meters::FlowError>;

    /// Whether output `id` runs in the engine (lmix-core restarted while
    /// it kept streaming or recording).
    async fn output_running(&self, id: lmix_types::OutputId) -> bool;
}
