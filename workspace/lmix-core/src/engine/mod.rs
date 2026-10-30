//! Engine drivers: turn switcher [`Command`]s into media-engine calls.
//!
//! The rest of lmix-core only sees this trait; strom's REST API stays
//! inside `strom.rs` (layer rule: no holes through the driver layer).

use async_trait::async_trait;
use lmix_types::{Device, LiveTransport};

use crate::switcher::Command;
use flows::MixerBackend;

pub mod flows;
pub mod meters;
pub mod null;
pub mod output_rate;
pub mod playback_flow;
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
    /// The mixer's preview output (VSP): what a take puts on program.
    Preview,
    /// A slot at tile size.
    Slot(usize),
    /// Every slot in one picture (see `flows::tiles_grid`).
    Tiles,
}

impl LivePicture {
    /// How its frames reach the Console: the CUDA mixer's pictures in GPU
    /// memory. A slot's picture is scaled on the CPU (the fallback for
    /// inputs that are not in the tile picture).
    pub fn transport(self, backend: MixerBackend) -> LiveTransport {
        match (self, backend) {
            (LivePicture::Slot(_), _) => LiveTransport::UnixFd,
            (_, MixerBackend::Cuda) => LiveTransport::CudaIpc,
            _ => LiveTransport::UnixFd,
        }
    }
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

    /// Whether a running flow uses a capture card (lmix-core restarted
    /// while the engine kept the cards).
    async fn holds_cards(&self) -> bool {
        false
    }

    /// Whether `flow` runs in the engine now. An engine that cannot tell
    /// says yes.
    async fn flow_running(&self, _flow: strom_types::FlowId) -> bool {
        true
    }

    /// Live picture for a Console on the engine's machine: path of a Unix
    /// socket with raw frames (`unixfdsrc`), started on the first call.
    async fn live(&self, _picture: LivePicture) -> Result<std::path::PathBuf, EngineError> {
        Err(EngineError::NoPictures)
    }

    /// No Console watches the tile picture any more (its lease ran out):
    /// the engine may stop drawing it until `live` asks again. False while
    /// it is not dark yet (the engine refused), so it is tried again.
    async fn dim_tiles(&self) -> bool {
        true
    }

    /// How the frames of `picture` come over its socket.
    fn live_transport(&self, _picture: LivePicture) -> LiveTransport {
        LiveTransport::UnixFd
    }

    /// Position reports of the media players (clips and playlists).
    fn players(&self) -> tokio::sync::broadcast::Receiver<crate::clips::PlayerProgress> {
        tokio::sync::broadcast::channel(1).1
    }

    /// Ends of the stingers the engine plays: until then it holds the
    /// mixer and refuses takes.
    fn stingers(&self) -> tokio::sync::broadcast::Receiver<meters::StingerEnd> {
        tokio::sync::broadcast::channel(1).1
    }

    /// The stinger take the engine started last, as its ends name it.
    fn stinger_generation(&self) -> u64 {
        0
    }

    /// Ids of sounds and pictures that are over: played to their end,
    /// stopped, or gone with the playback flow.
    fn playback_ended(&self) -> tokio::sync::broadcast::Receiver<String> {
        tokio::sync::broadcast::channel(1).1
    }

    /// Machine statistics and QoS per flow, as far as the engine reports
    /// them.
    fn health(&self) -> meters::EngineHealth {
        meters::EngineHealth::default()
    }

    /// Measured rate (and SRT link health) of a running SRT, RTMP or
    /// recording output of `spec`; `None` for other kinds or while
    /// unknown.
    async fn output_stats(
        &self,
        _id: lmix_types::OutputId,
        _spec: &lmix_types::OutputSpec,
    ) -> Option<lmix_types::OutputStats> {
        None
    }
}
