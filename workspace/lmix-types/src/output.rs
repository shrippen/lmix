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
            OutputGroup::Streams => {
                matches!(spec, OutputSpec::Srt { .. } | OutputSpec::Rtmp { .. })
            }
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
    /// The mixer's preview as it composes it: an input or a layout, what
    /// a take puts on program (OUT.2), with program sound.
    Preview,
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

/// Longest file of a split recording (REC.4): a day.
pub const SPLIT_MINUTES_MAX: u32 = 24 * 60;
/// Smallest file of a split recording: below, a few seconds per file.
pub const SPLIT_MB_MIN: u32 = 10;
/// Largest file of a split recording: 1 TiB.
pub const SPLIT_MB_MAX: u32 = 1024 * 1024;

/// A recording's folder below the engine's media directory, one way
/// written: `./show//day 1/` → `show/day 1`. Refused: empty, or leaving
/// the media directory (`..`). A leading `/` counts as the media
/// directory's root, not the machine's.
pub fn normal_folder(folder: &str) -> Result<String, String> {
    let parts: Vec<&str> = folder
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.contains(&"..") {
        return Err(format!("folder {folder}: .. leaves the media directory"));
    }
    if parts.is_empty() {
        return Err("a folder in the media directory, not empty".into());
    }
    Ok(parts.join("/"))
}

/// The folder an engine writes a recording to: its normal form; a folder
/// of an older show that has none as it was (the engine refuses it).
pub fn engine_folder(folder: &str) -> String {
    normal_folder(folder).unwrap_or_else(|_| folder.to_string())
}

/// Longest text of a recording mark (REC.8).
pub const MARK_TEXT_MAX: usize = 200;

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
        /// A new file every so many minutes (REC.4); 0: one file.
        #[serde(default, skip_serializing_if = "is_zero")]
        split_minutes: u32,
        /// A new file once one holds so many MB (REC.4); 0: no limit.
        #[serde(default, skip_serializing_if = "is_zero")]
        split_mb: u32,
    },
    /// FLV over RTMP or RTMPS to a platform (STR.2): H.264 + AAC.
    /// `url` is the server, e.g. `rtmp://a.rtmp.youtube.com/live2`; the
    /// stream key goes apart so clients can hide it.
    Rtmp {
        url: String,
        #[serde(default)]
        key: String,
        #[serde(default = "default_kbps")]
        video_kbps: u32,
        /// Codec (H.264 only: FLV carries no other), rate control and
        /// keyframe interval (STR.3).
        #[serde(default, skip_serializing_if = "VideoEncoding::is_default")]
        encoding: VideoEncoding,
    },
    /// NDI on the network, uncompressed program (OUT.6).
    Ndi { name: String },
    /// Picture on a screen of the engine machine (OUT.3): projector, LED
    /// wall, confidence monitor. Picture only.
    Display { mode: DisplayMode },
    /// SDI or HDMI out of a playout card (DeckLink, OUT.4): uncompressed
    /// picture in `mode`, scaled to it, with the sound embedded.
    /// `device` is the card's device number. The output's sound is on
    /// channels 1/2; `more_pairs` gives pairs 2, 3, … a bus each (none:
    /// silent), as many as `channels` hold. `sound_delay_ms` plays the
    /// sound later than the picture.
    Card {
        device: u32,
        mode: crate::card::CardMode,
        #[serde(default, skip_serializing_if = "crate::card::CardChannels::is_default")]
        channels: crate::card::CardChannels,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        more_pairs: Vec<Option<crate::bus::BusId>>,
        #[serde(default, skip_serializing_if = "is_zero")]
        sound_delay_ms: u32,
    },
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
            OutputSpec::Srt { encoding, .. }
            | OutputSpec::Rtmp { encoding, .. }
            | OutputSpec::Record { encoding, .. } => Some(encoding),
            _ => None,
        }
    }

    /// Why the engine could not send this, if it could not (STR.2, STR.3).
    pub fn problem(&self) -> Option<String> {
        if let OutputSpec::Rtmp { url, encoding, .. } = self {
            if !(url.starts_with("rtmp://") || url.starts_with("rtmps://")) {
                return Some(format!("{url} is no rtmp:// or rtmps:// address"));
            }
            if encoding.codec != VideoCodec::H264 {
                return Some("RTMP carries H.264 only".into());
            }
        }
        self.encoding().and_then(VideoEncoding::problem)
    }

    /// The spec with a recording's folder in its normal form
    /// (`normal_folder`); other outputs as they are.
    pub fn with_normal_folder(mut self) -> Result<Self, String> {
        if let Self::Record { folder, .. } = &mut self {
            *folder = normal_folder(folder)?;
        }
        Ok(self)
    }

    /// What is wrong with a recording's splitting (REC.4).
    pub fn split_problem(&self) -> Option<String> {
        let Self::Record {
            split_minutes,
            split_mb,
            ..
        } = self
        else {
            return None;
        };
        if *split_minutes > SPLIT_MINUTES_MAX {
            return Some(format!(
                "a new file every {split_minutes} minutes, at most {SPLIT_MINUTES_MAX}"
            ));
        }
        let mb_range = SPLIT_MB_MIN..=SPLIT_MB_MAX;
        (*split_mb != 0 && !mb_range.contains(split_mb))
            .then(|| format!("a new file at {split_mb} MB, {SPLIT_MB_MIN}–{SPLIT_MB_MAX} MB or 0"))
    }

    /// What is wrong with a card output's sound: more pairs than its
    /// channels hold, a delay past `MAX_CARD_SOUND_DELAY_MS`.
    pub fn card_problem(&self) -> Option<String> {
        let Self::Card {
            channels,
            more_pairs,
            sound_delay_ms,
            ..
        } = self
        else {
            return None;
        };
        let room = channels.pairs() as usize - 1;
        if more_pairs.len() > room {
            return Some(format!(
                "{} channels hold {room} more pairs, not {}",
                channels.count(),
                more_pairs.len()
            ));
        }
        let max = crate::card::MAX_CARD_SOUND_DELAY_MS;
        (*sound_delay_ms > max)
            .then(|| format!("sound delay {sound_delay_ms} ms, at most {max} ms"))
    }

    /// Where an RTMP output publishes: server and stream key.
    pub fn rtmp_location(url: &str, key: &str) -> String {
        if key.is_empty() {
            url.to_string()
        } else {
            format!("{}/{key}", url.trim_end_matches('/'))
        }
    }

    pub fn default_name(&self) -> String {
        match self {
            Self::Srt { uri, .. } => format!("SRT {uri}"),
            Self::Rtmp { url, .. } => format!("RTMP {url}"),
            Self::Record { folder, .. } => format!("Record {folder}"),
            Self::Ndi { name } => format!("NDI {name}"),
            Self::Display { mode } => match mode {
                DisplayMode::Window => "Window".into(),
                DisplayMode::Fullscreen => "Fullscreen".into(),
                DisplayMode::Drm { connector } => format!("Screen {connector}"),
            },
            Self::Card { device, mode, .. } => {
                format!("Card {} {}", u64::from(*device) + 1, mode.nick())
            }
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
    /// A card output that would run, waiting while the cards are off.
    CardsOff,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Output {
    pub id: OutputId,
    pub number: u32,
    pub name: String,
    pub spec: OutputSpec,
    #[serde(default)]
    pub source: OutputSource,
    /// Sound from this bus instead of the source's own.
    #[serde(default)]
    pub bus: Option<crate::bus::BusId>,
    pub status: OutputStatus,
    /// Failed, and the failback is about to start it again (or is
    /// starting it): it counts as sending, its settings are locked.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub failback: bool,
}

/// What a running SRT, RTMP or recording output measures, about once a
/// second (STR.1: the streamer sees a weak link before the stream breaks;
/// M07: the menu shows the rate it sends).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OutputStats {
    pub output: OutputId,
    /// Measured rate: sent on the network (SRT, RTMP, with protocol
    /// framing) or written to disk (recording).
    pub kbps: u32,
    /// Link health, SRT only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub srt: Option<SrtLink>,
}

/// Link health of a running SRT output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SrtLink {
    /// A receiver is connected.
    pub connected: bool,
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
        let side = |v: u32| (FORMAT_MIN_PX..=FORMAT_MAX_PX).contains(&v) && v.is_multiple_of(2);
        side(self.width) && side(self.height)
    }

    /// `1920x1080`, as strom takes it.
    pub fn resolution(&self) -> String {
        format!("{}x{}", self.width, self.height)
    }
}

/// Where the multiview shows program (OUT.10); preview takes the other side.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProgramSide {
    Left,
    /// As the engine does by default: preview left, program right.
    #[default]
    Right,
}

/// Whether the multiview draws level meters on its tiles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MultiviewMeters {
    #[default]
    Shown,
    Hidden,
}

/// Layout of the engine's multiview (OUT.10).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MultiviewLayout {
    #[serde(default)]
    pub program: ProgramSide,
    #[serde(default)]
    pub meters: MultiviewMeters,
}

impl MultiviewLayout {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }
}

/// Sample rates the audio mixer runs at (AUD.17), Hz.
pub const SAMPLE_RATES: [u32; 2] = [48_000, 44_100];

/// The default: broadcast and video run at 48 kHz.
pub fn default_sample_rate() -> u32 {
    SAMPLE_RATES[0]
}

fn is_zero(value: &u32) -> bool {
    *value == 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CardMode;

    #[test]
    fn the_last_card_has_a_label() {
        let card = OutputSpec::Card {
            device: u32::MAX,
            mode: CardMode::Hd720p50,
            channels: Default::default(),
            more_pairs: Vec::new(),
            sound_delay_ms: 0,
        };
        assert_eq!(card.default_name(), "Card 4294967296 720p50");
    }

    fn record(split_minutes: u32, split_mb: u32) -> OutputSpec {
        OutputSpec::Record {
            folder: "show".into(),
            container: Container::Mkv,
            video_kbps: DEFAULT_VIDEO_KBPS,
            encoding: VideoEncoding::default(),
            split_minutes,
            split_mb,
        }
    }

    // REC.4: shows from before splitting read as one file, and one file
    // writes nothing new.
    #[test]
    fn a_recording_splits_only_when_asked() {
        let old: OutputSpec = serde_json::from_str(r#"{"kind":"record","folder":"show"}"#).unwrap();
        assert_eq!(old, record(0, 0));
        let json = serde_json::to_string(&old).unwrap();
        assert!(!json.contains("split"), "{json}");

        let json = serde_json::to_value(record(30, 2048)).unwrap();
        assert_eq!(json["split_minutes"], 30);
        assert_eq!(json["split_mb"], 2048);
    }

    // One folder, one way written: the engine's file names and the
    // protocol beside them agree (REC.8).
    #[test]
    fn a_recording_folder_has_one_form() {
        assert_eq!(normal_folder("show"), Ok("show".into()));
        assert_eq!(normal_folder("./show//day 1/"), Ok("show/day 1".into()));
        assert_eq!(normal_folder("/show/"), Ok("show".into()));
        assert!(normal_folder("show/../..").is_err());
        assert!(normal_folder("./").is_err());
        assert!(normal_folder("").is_err());
        let spec = OutputSpec::Record {
            folder: "show/".into(),
            container: Container::Mkv,
            video_kbps: DEFAULT_VIDEO_KBPS,
            encoding: VideoEncoding::default(),
            split_minutes: 0,
            split_mb: 0,
        };
        let OutputSpec::Record { folder, .. } = spec.with_normal_folder().unwrap() else {
            panic!("a recording");
        };
        assert_eq!(folder, "show");
    }

    #[test]
    fn splitting_stays_in_sane_ranges() {
        assert_eq!(record(0, 0).split_problem(), None);
        assert_eq!(
            record(SPLIT_MINUTES_MAX, SPLIT_MB_MAX).split_problem(),
            None
        );
        assert_eq!(record(1, SPLIT_MB_MIN).split_problem(), None);
        assert!(record(SPLIT_MINUTES_MAX + 1, 0).split_problem().is_some());
        assert!(record(0, SPLIT_MB_MIN - 1).split_problem().is_some());
        assert!(record(0, SPLIT_MB_MAX + 1).split_problem().is_some());
        let srt = OutputSpec::Srt {
            uri: "srt://x:1".into(),
            video_kbps: 1,
            encoding: VideoEncoding::default(),
        };
        assert_eq!(srt.split_problem(), None);
    }
}
