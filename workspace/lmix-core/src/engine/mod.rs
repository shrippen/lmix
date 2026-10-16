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

/// Which live picture a Console reads (`Engine::live`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LivePicture {
    Program,
    /// A slot at tile size.
    Slot(usize),
    /// A slot as it arrives (the preview monitor).
    SlotFull(usize),
}

#[async_trait]
pub trait Engine: Send + Sync {
    /// Build the mixer with `slots` inputs; idempotent.
    async fn start(&self, slots: usize) -> Result<(), EngineError>;

    /// Identity of the running engine process; a new one means the engine
    /// restarted and lost the show. `None`: the engine cannot tell.
    async fn instance(&self) -> Option<String> {
        None
    }

    /// Whether the engine can colour-correct inputs (looks).
    fn looks_available(&self) -> bool {
        false
    }

    /// Carry out one command.
    async fn run(&self, command: &Command) -> Result<(), EngineError>;

    /// Latest frame as JPEG.
    async fn picture(&self, picture: Picture) -> Result<Vec<u8>, EngineError>;

    /// Video capture devices the engine can open.
    async fn devices(&self) -> Result<Vec<Device>, EngineError>;

    /// Audio levels, updated as the engine measures them.
    fn levels(&self) -> tokio::sync::watch::Receiver<meters::EngineLevels>;

    /// Start measuring capture-to-program latency for `seconds`; results
    /// arrive in `levels().program_ages_ms` (earlier samples are dropped).
    async fn measure_latency(&self, seconds: u32) -> Result<(), EngineError>;

    /// Errors of running flows (outputs failing).
    fn flow_errors(&self) -> tokio::sync::broadcast::Receiver<meters::FlowError>;

    /// Whether output `id` runs in the engine (lmix-core restarted while
    /// it kept streaming or recording).
    async fn output_running(&self, id: lmix_types::OutputId) -> bool;

    /// Live picture for a Console on the engine's machine: path of a Unix
    /// socket with raw frames (`unixfdsrc`), started on the first call.
    async fn live(&self, _picture: LivePicture) -> Result<std::path::PathBuf, EngineError> {
        Err(EngineError::NoPictures)
    }

    /// Position reports of the media players (clips and playlists).
    fn players(&self) -> tokio::sync::broadcast::Receiver<crate::clips::PlayerProgress> {
        tokio::sync::broadcast::channel(1).1
    }

    /// Machine statistics and QoS per flow, as far as the engine reports
    /// them.
    fn health(&self) -> meters::EngineHealth {
        meters::EngineHealth::default()
    }

    /// Link health of a running network output; `None` for other kinds
    /// or while unknown.
    async fn output_stats(&self, _id: lmix_types::OutputId) -> Option<lmix_types::OutputStats> {
        None
    }
}
