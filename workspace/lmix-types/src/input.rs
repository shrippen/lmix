//! Inputs: every source in a show (camera, file, network, test pattern).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Stable identity of an input; survives renaming and reordering (INP.2).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct InputId(pub Uuid);

impl InputId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for InputId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for InputId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// How an action names an input (INP.2, API.9).
///
/// JSON: a number (`3`, 1-based position), `"preview"`, `"program"`, an id
/// (`"7d0c…"`), or a name (`"Camera 1"`). Ids win over names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum InputRef {
    Number(u32),
    Text(String),
}

/// Reserved words of [`InputRef::Text`].
pub const REF_PREVIEW: &str = "preview";
pub const REF_PROGRAM: &str = "program";

/// `videotestsrc` patterns offered as test sources.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TestPattern {
    Bars,
    Ball,
    Black,
    Snow,
    Checkers,
}

impl TestPattern {
    /// `videotestsrc` `pattern` nick.
    pub fn nick(self) -> &'static str {
        match self {
            Self::Bars => "smpte100",
            Self::Ball => "ball",
            Self::Black => "black",
            Self::Snow => "snow",
            Self::Checkers => "checkers-8",
        }
    }
}

/// What an input shows. Engine specifics stay in `lmix-core`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SourceSpec {
    /// Test pattern with a test tone (SRC.7).
    Pattern { pattern: TestPattern },
    /// Local media file, looped (SRC.2).
    File { path: String },
    /// Network stream handled by the media player: http(s), rtsp, … (SRC.13).
    Url { url: String },
    /// MPEG-TS over SRT, e.g. `srt://:9000?mode=listener` (SRC.12).
    Srt { uri: String },
    /// Capture device id from discovery (SRC.1).
    Device { id: String },
    /// Solid color, `#rrggbb` (SRC.7).
    Color { color: String },
    /// Still image file, shown until replaced (SRC.4).
    Image { path: String },
    /// Media files played one after another, looped (SRC.3).
    Playlist { paths: Vec<String> },
    /// NDI source by its name, e.g. `HOST (Camera 1)` (SRC.10).
    Ndi { name: String },
    /// WHIP ingest: browsers, OBS or encoders push to the engine at
    /// `/whip/<endpoint>` (SRC.14).
    Whip { endpoint: String },
    /// Audio-only input: microphone or line-in by device id (AUD.1).
    AudioDevice { id: String },
}

/// What a transport button does to a file, URL or playlist input (SRC.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Transport {
    Play,
    Pause,
    /// Back to the start of the current item and play.
    Restart,
    Next,
    Previous,
}

impl SourceSpec {
    /// Whether the source can carry sound: test patterns bring a tone,
    /// files, URLs and SRT their audio track; devices are video only
    /// (a camera's sound is its own audio device).
    pub fn has_audio(&self) -> bool {
        !matches!(
            self,
            Self::Device { .. } | Self::Color { .. } | Self::Image { .. }
        )
    }

    /// Whether the source has a picture; audio inputs have none and cannot
    /// go to preview or program.
    pub fn has_video(&self) -> bool {
        !matches!(self, Self::AudioDevice { .. })
    }

    /// Whether play/pause/next apply (media player sources).
    pub fn has_transport(&self) -> bool {
        matches!(
            self,
            Self::File { .. } | Self::Url { .. } | Self::Playlist { .. }
        )
    }

    /// Default input name for a new source.
    pub fn default_name(&self) -> String {
        match self {
            Self::Pattern { pattern } => format!("Pattern {pattern:?}"),
            Self::File { path } => file_stem(path),
            Self::Url { url } => url.clone(),
            Self::Srt { uri } => uri.clone(),
            Self::Device { id } | Self::AudioDevice { id } => id.clone(),
            Self::Color { color } => format!("Color {color}"),
            Self::Image { path } => file_stem(path),
            Self::Playlist { paths } => match paths.first() {
                Some(first) => format!("{} (+{})", file_stem(first), paths.len() - 1),
                None => "Playlist".into(),
            },
            Self::Ndi { name } => name.clone(),
            Self::Whip { endpoint } => format!("WHIP {endpoint}"),
        }
    }
}

/// `#rrggbb` as `0xrrggbb`; `None` when malformed.
pub fn color_rgb(color: &str) -> Option<u32> {
    let hex = color.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    u32::from_str_radix(hex, 16).ok()
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Runtime health of an input (INP.13).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputStatus {
    /// Engine flow being built or started.
    #[default]
    Starting,
    Online,
    /// Source missing or failed; the slot shows a substitute frame.
    Offline,
}

/// One input as clients see it.
/// What an input shows when its source delivers no new frames (paused
/// clip, lost camera or stream).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SignalLoss {
    /// Keep the last frame.
    #[default]
    Hold,
    /// Go black shortly after the last frame.
    Black,
}

/// Highest input volume (vMix scale, 100 = unity gain).
pub const VOLUME_MAX: u8 = 100;

/// Audio settings of an input (AUD).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InputAudio {
    /// Sound on (vMix "M" button).
    pub on: bool,
    /// Audio follows video: heard only while on program.
    pub follow_video: bool,
    /// 0–100, 100 = unity gain.
    pub volume: u8,
}

impl Default for InputAudio {
    fn default() -> Self {
        Self {
            on: true,
            follow_video: false,
            volume: VOLUME_MAX,
        }
    }
}

/// Solo listening on the monitor bus (headphones), not on program.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Listen {
    #[default]
    Off,
    /// Pre-fader listen: the input as it arrives, even when muted.
    Pfl,
    /// After-fader listen: the input as it goes to program.
    Afl,
}

/// Where a push source (WHIP) is sent to, for sharing with the sender.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Ingest {
    /// WHIP endpoint for OBS and encoders.
    pub whip_url: String,
    /// Web page a guest opens in the browser to send camera and microphone.
    pub browser_url: String,
}

/// Color tag of an input tile, for grouping (cameras, clips, graphics).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputColor {
    Red,
    Orange,
    Yellow,
    Green,
    Teal,
    Blue,
    Purple,
    Pink,
}

impl InputColor {
    pub const ALL: [InputColor; 8] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Teal,
        Self::Blue,
        Self::Purple,
        Self::Pink,
    ];
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Input {
    pub id: InputId,
    /// 1-based position in the input bar (INP.3).
    pub number: u32,
    pub name: String,
    pub source: SourceSpec,
    /// Mixer slot the input feeds (0-based).
    pub slot: usize,
    pub status: InputStatus,
    #[serde(default)]
    pub color: Option<InputColor>,
    #[serde(default)]
    pub audio: InputAudio,
    /// False when the source has no sound (devices); audio settings are
    /// kept but do nothing.
    #[serde(default)]
    pub has_audio: bool,
    /// Where to send a WHIP source (B30); `None` for other sources.
    #[serde(default)]
    pub ingest: Option<Ingest>,
    /// Last frame or black when the source stalls.
    #[serde(default)]
    pub signal_loss: SignalLoss,
    /// Solo on the monitor bus; not saved in the show file.
    #[serde(default)]
    pub listen: Listen,
}

/// What a discovered device delivers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DeviceKind {
    /// Camera, capture card: [`SourceSpec::Device`].
    Video,
    /// Microphone, line-in: [`SourceSpec::AudioDevice`].
    Audio,
    /// NDI source on the network: [`SourceSpec::Ndi`] with the name.
    Ndi,
}

/// A device the engine found.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Device {
    /// Goes into the source (`id`, or `name` for NDI).
    pub id: String,
    pub name: String,
    /// OS media API, e.g. "V4L2"; the same camera can appear once per API.
    pub api: String,
    pub kind: DeviceKind,
}

impl Device {
    /// The source that shows this device.
    pub fn source(&self) -> SourceSpec {
        match self.kind {
            DeviceKind::Video => SourceSpec::Device {
                id: self.id.clone(),
            },
            DeviceKind::Audio => SourceSpec::AudioDevice {
                id: self.id.clone(),
            },
            DeviceKind::Ndi => SourceSpec::Ndi {
                name: self.name.clone(),
            },
        }
    }
}
