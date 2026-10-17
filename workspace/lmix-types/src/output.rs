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

/// Outputs that `start_outputs` and `stop_outputs` act on together
/// (STR.1: all stream targets at once).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OutputGroup {
    /// SRT streams.
    Streams,
    /// Recordings.
    Recordings,
    /// Every output.
    All,
}

impl OutputGroup {
    /// Whether an output of `spec` belongs to the group.
    pub fn holds(self, spec: &OutputSpec) -> bool {
        match self {
            OutputGroup::Streams => matches!(spec, OutputSpec::Srt { .. }),
            OutputGroup::Recordings => matches!(spec, OutputSpec::Record { .. }),
            OutputGroup::All => true,
        }
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
    /// The multiview: preview, program and the inputs (OUT.2, OUT.10),
    /// with program sound.
    Multiview,
}

/// Default video bitrate of encoded outputs.
pub const DEFAULT_VIDEO_KBPS: u32 = 6000;

fn default_kbps() -> u32 {
    DEFAULT_VIDEO_KBPS
}

/// Video codec of an encoded output (STR.3). The engine's recorder and
/// its MPEG-TS muxer take these two.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VideoCodec {
    #[default]
    H264,
    /// HEVC: about the same picture at a lower bitrate, heavier to encode.
    H265,
}

/// How the encoder holds its bitrate.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RateMode {
    /// Variable around the bitrate (default).
    #[default]
    Vbr,
    /// Constant: what platforms and fixed links ask for.
    Cbr,
}

/// Default keyframe interval of encoded outputs.
pub const DEFAULT_KEYFRAME_MS: u32 = 2_000;
/// Longest keyframe interval.
pub const KEYFRAME_MAX_MS: u32 = 10_000;

fn default_keyframe_ms() -> u32 {
    DEFAULT_KEYFRAME_MS
}

/// How an output encodes video beyond its bitrate (STR.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct VideoEncoding {
    #[serde(default)]
    pub codec: VideoCodec,
    #[serde(default)]
    pub rate: RateMode,
    /// Time between keyframes; 0 leaves it to the encoder.
    #[serde(default = "default_keyframe_ms")]
    pub keyframe_ms: u32,
}

impl Default for VideoEncoding {
    fn default() -> Self {
        Self {
            codec: VideoCodec::default(),
            rate: RateMode::default(),
            keyframe_ms: DEFAULT_KEYFRAME_MS,
        }
    }
}

impl VideoEncoding {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Why the engine could not encode this, if it could not.
    pub fn problem(&self) -> Option<String> {
        (self.keyframe_ms > KEYFRAME_MAX_MS).then(|| {
            format!(
                "keyframe interval {} ms, at most {KEYFRAME_MAX_MS} ms",
                self.keyframe_ms
            )
        })
    }
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
        /// Codec, rate control and keyframe interval (STR.3).
        #[serde(default, skip_serializing_if = "VideoEncoding::is_default")]
        encoding: VideoEncoding,
    },
    /// Recording, H.264 + AAC, into `folder` below the engine's media
    /// directory (REC.1).
    Record {
        folder: String,
        #[serde(default)]
        container: Container,
        #[serde(default = "default_kbps")]
        video_kbps: u32,
        /// Codec, rate control and keyframe interval (STR.3).
        #[serde(default, skip_serializing_if = "VideoEncoding::is_default")]
        encoding: VideoEncoding,
    },
    /// NDI on the network, uncompressed program (OUT.6).
    Ndi { name: String },
    /// Picture on a screen of the engine machine (OUT.3): projector, LED
    /// wall, confidence monitor. Picture only.
    Display { mode: DisplayMode },
}

/// How a display output reaches its screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum DisplayMode {
    /// A normal window on the desktop (also a confidence monitor).
    Window,
    /// Fullscreen on the desktop (Wayland; the compositor picks the
    /// screen, e.g. a KDE window rule).
    Fullscreen,
    /// Straight to a screen connector without desktop (DRM/KMS), e.g. a
    /// projector on a headless engine.
    Drm { connector: u32 },
}

impl OutputSpec {
    /// Its video encoding; `None` for outputs that send raw picture.
    pub fn encoding(&self) -> Option<&VideoEncoding> {
        match self {
            OutputSpec::Srt { encoding, .. } | OutputSpec::Record { encoding, .. } => {
                Some(encoding)
            }
            _ => None,
        }
    }

    pub fn default_name(&self) -> String {
        match self {
            Self::Srt { uri, .. } => format!("SRT {uri}"),
            Self::Record { folder, .. } => format!("Record {folder}"),
            Self::Ndi { name } => format!("NDI {name}"),
            Self::Display { mode } => match mode {
                DisplayMode::Window => "Window".into(),
                DisplayMode::Fullscreen => "Fullscreen".into(),
                DisplayMode::Drm { connector } => format!("Screen {connector}"),
            },
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
    /// Sound from this aux bus (1–7, A–G) instead of the source's own.
    #[serde(default)]
    pub aux: Option<u8>,
    pub status: OutputStatus,
}

/// Link health of a running network output (SRT), about once a second
/// (STR.1: the streamer sees a weak link before the stream breaks).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutputStats {
    pub output: OutputId,
    /// A receiver is connected.
    pub connected: bool,
    /// Current send rate.
    pub kbps: u32,
    /// Packets lost on the way (reported by the receiver), since start.
    pub packets_lost: u64,
    /// Packets dropped before sending because they were too late, since
    /// start: the link is too slow for the bitrate.
    pub packets_dropped: u64,
    /// Round-trip time to the receiver.
    pub rtt_ms: Option<u32>,
}

/// Frame rate of program (OUT.1); the rates strom's mixer offers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Framerate {
    #[serde(rename = "24000/1001")]
    Fps23_976,
    #[serde(rename = "24/1")]
    Fps24,
    #[serde(rename = "25/1")]
    Fps25,
    #[serde(rename = "30000/1001")]
    Fps29_97,
    #[default]
    #[serde(rename = "30/1")]
    Fps30,
    #[serde(rename = "50/1")]
    Fps50,
    #[serde(rename = "60000/1001")]
    Fps59_94,
    #[serde(rename = "60/1")]
    Fps60,
}

impl Framerate {
    /// As GStreamer writes it, e.g. `30000/1001`.
    pub fn fraction(self) -> &'static str {
        match self {
            Self::Fps23_976 => "24000/1001",
            Self::Fps24 => "24/1",
            Self::Fps25 => "25/1",
            Self::Fps29_97 => "30000/1001",
            Self::Fps30 => "30/1",
            Self::Fps50 => "50/1",
            Self::Fps59_94 => "60000/1001",
            Self::Fps60 => "60/1",
        }
    }
}

/// Smallest and largest program side, pixels (SD to 8K, upright too).
pub const FORMAT_MIN_PX: u32 = 320;
pub const FORMAT_MAX_PX: u32 = 7680;

/// Picture size and rate of program (OUT.1): every output, the
/// multiview and the overlays follow it. Upright (9:16) is allowed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MasterFormat {
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub framerate: Framerate,
}

impl Default for MasterFormat {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            framerate: Framerate::default(),
        }
    }
}

impl MasterFormat {
    /// Even sides within the limits (video formats subsample by 2).
    pub fn is_valid(&self) -> bool {
        let side = |v: u32| (FORMAT_MIN_PX..=FORMAT_MAX_PX).contains(&v) && v % 2 == 0;
        side(self.width) && side(self.height)
    }

    /// `1920x1080`, as strom takes it.
    pub fn resolution(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}
