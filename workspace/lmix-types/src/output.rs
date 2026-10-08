//! Outputs (OUT, REC, STR): where program goes besides the mixer.
//!
//! Each output is configured in the show and started or stopped by the
//! operator (vMix "Stream" / "Record" buttons); running state is not saved.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::input::InputId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct OutputId(pub Uuid);

impl OutputId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for OutputId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for OutputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// An output by 1-based number, name or id.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum OutputRef {
    Number(u32),
    Text(String),
}

/// File format of a recording.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Container {
    /// Readable while being written, survives a crash (default).
    #[default]
    Mkv,
    Mp4,
    /// MPEG-TS.
    Ts,
}

impl Container {
    pub const ALL: [Container; 3] = [Self::Mkv, Self::Mp4, Self::Ts];

    /// strom recorder `container` value.
    pub fn nick(self) -> &'static str {
        match self {
            Self::Mkv => "mkv",
            Self::Mp4 => "mp4",
            Self::Ts => "mpegts",
        }
    }
}

/// What an output carries.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "from", content = "input", rename_all = "snake_case")]
pub enum OutputSource {
    /// The program mix (default).
    #[default]
    Program,
    /// One input as it arrives, before the mixer: ISO recording
    /// (MultiCorder, REC.4) or an ISO feed.
    Input(InputId),
}

/// Default video bitrate of encoded outputs.
pub const DEFAULT_VIDEO_KBPS: u32 = 6000;

fn default_kbps() -> u32 {
    DEFAULT_VIDEO_KBPS
}

/// What an output does with program.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OutputSpec {
    /// MPEG-TS over SRT, H.264 + AAC (STR.1), e.g. `srt://host:9000` or
    /// `srt://:9000?mode=listener`.
    Srt {
        uri: String,
        #[serde(default = "default_kbps")]
        video_kbps: u32,
    },
    /// Recording, H.264 + AAC, into `folder` below the engine's media
    /// directory (REC.1).
    Record {
        folder: String,
        #[serde(default)]
        container: Container,
        #[serde(default = "default_kbps")]
        video_kbps: u32,
    },
    /// NDI on the network, uncompressed program (OUT.6).
    Ndi { name: String },
}

impl OutputSpec {
    pub fn default_name(&self) -> String {
        match self {
            Self::Srt { uri, .. } => format!("SRT {uri}"),
            Self::Record { folder, .. } => format!("Record {folder}"),
            Self::Ndi { name } => format!("NDI {name}"),
        }
    }
}

/// Where an output stands.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OutputStatus {
    #[default]
    Stopped,
    Running {
        /// Unix time in ms when it started: clients show the running time.
        since_ms: u64,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Output {
    pub id: OutputId,
    pub number: u32,
    pub name: String,
    pub spec: OutputSpec,
    #[serde(default)]
    pub source: OutputSource,
    pub status: OutputStatus,
}
