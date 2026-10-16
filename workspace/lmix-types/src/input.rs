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
/// JSON: a number (`3`, 1-based position), `"preview"`, `"program"`,
/// `"dynamic1"`–`"dynamic4"` (AUT.5), an id (`"7d0c…"`), or a name
/// (`"Camera 1"`). Ids win over names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum InputRef {
    Number(u32),
    Text(String),
}

/// Reserved words of [`InputRef::Text`].
pub const REF_PREVIEW: &str = "preview";
pub const REF_PROGRAM: &str = "program";
/// `"dynamic1"`…: the input a dynamic slot holds (AUT.5).
pub const REF_DYNAMIC: &str = "dynamic";

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

/// What a file, URL or playlist input does by itself when it goes on
/// or off air (INP.11). On air is what tally calls program: program, a
/// layout on program, an overlay channel, and preview while the T-bar
/// mixes it in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AutoPlay {
    /// Play when it goes on air.
    #[serde(default)]
    pub play: bool,
    /// Back to the start of its item when it goes on air.
    #[serde(default)]
    pub restart: bool,
    /// Pause once it is off air (after the transition).
    #[serde(default)]
    pub pause: bool,
}

impl AutoPlay {
    pub fn is_off(&self) -> bool {
        *self == Self::default()
    }
}

/// What a click on an input (INP.8) or its GO (INP.9) does, named as
/// the action it runs on the input.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputPress {
    /// Into preview.
    PreviewInput,
    /// Straight to program.
    CutDirect,
    /// To program with the quick play effect, and play (SWI.8).
    QuickPlay,
    /// Nothing.
    None,
}

impl InputPress {
    pub const ALL: [InputPress; 4] = [
        InputPress::PreviewInput,
        InputPress::CutDirect,
        InputPress::QuickPlay,
        InputPress::None,
    ];

    /// A click puts the input into preview unless set otherwise.
    pub fn click() -> Self {
        Self::PreviewInput
    }

    /// GO is quick play unless set otherwise.
    pub fn go() -> Self {
        Self::QuickPlay
    }

    pub fn is_click(&self) -> bool {
        *self == Self::click()
    }

    pub fn is_go(&self) -> bool {
        *self == Self::go()
    }
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

/// Aux buses of the audio mixer, numbered 1–2 in actions.
pub const AUX_BUSES: usize = 2;

/// Which aux buses take their sends before the channel fader (and its
/// on/off): bus 1 is the monitor mix (in-ear, presenters hear
/// themselves whatever is on air), bus 2 a second mix after the fader.
pub const AUX_PRE_FADER: [bool; AUX_BUSES] = [true, false];

/// Master of an aux bus.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AuxBus {
    /// 0–100, 100 = unity gain.
    pub volume: u8,
    pub mute: bool,
}

impl Default for AuxBus {
    fn default() -> Self {
        Self {
            volume: VOLUME_MAX,
            mute: false,
        }
    }
}

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

/// Channel strip of an input with sound (AUD.3–7): what the audio mixer
/// does to it before the fader. Every section keeps its values while off.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct ChannelStrip {
    /// Input trim, dB (−20…+20).
    pub gain_db: f32,
    /// −1 left … 0 centre … 1 right.
    pub pan: f32,
    /// High-pass filter against rumble.
    pub hpf: Hpf,
    /// Four-band EQ.
    pub eq: Eq4,
    /// Noise gate.
    pub gate: Gate,
    pub compressor: Compressor,
    /// Sound later than the picture, ms (0…500): lip sync for a camera
    /// whose picture arrives late.
    pub delay_ms: u32,
}

impl Default for ChannelStrip {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            pan: 0.0,
            hpf: Hpf {
                on: false,
                hz: 80.0,
            },
            eq: Eq4 {
                on: false,
                gain_db: [0.0; 4],
            },
            gate: Gate {
                on: false,
                threshold_db: -40.0,
            },
            compressor: Compressor {
                on: false,
                threshold_db: -20.0,
                ratio: 4.0,
            },
            delay_ms: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Hpf {
    pub on: bool,
    /// Cutoff, Hz (20…500).
    pub hz: f32,
}

/// Bands at 80 Hz, 400 Hz, 2 kHz and 8 kHz.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Eq4 {
    pub on: bool,
    /// Gain per band, dB (−15…+15), low band first.
    pub gain_db: [f32; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Gate {
    pub on: bool,
    /// Below this the gate closes, dB (−60…0).
    pub threshold_db: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Compressor {
    pub on: bool,
    /// dB (−60…0).
    pub threshold_db: f32,
    /// 1…20 (n:1).
    pub ratio: f32,
}

impl ChannelStrip {
    /// Longest audio delay.
    pub const DELAY_MAX_MS: u32 = 500;

    /// The first value outside its range, by field name.
    pub fn out_of_range(&self) -> Option<&'static str> {
        let within = |v: f32, lo: f32, hi: f32| (lo..=hi).contains(&v);
        [
            (within(self.gain_db, -20.0, 20.0), "gain_db"),
            (within(self.pan, -1.0, 1.0), "pan"),
            (within(self.hpf.hz, 20.0, 500.0), "hpf.hz"),
            (
                self.eq.gain_db.iter().all(|g| within(*g, -15.0, 15.0)),
                "eq.gain_db",
            ),
            (
                within(self.gate.threshold_db, -60.0, 0.0),
                "gate.threshold_db",
            ),
            (
                within(self.compressor.threshold_db, -60.0, 0.0),
                "compressor.threshold_db",
            ),
            (within(self.compressor.ratio, 1.0, 20.0), "compressor.ratio"),
            (self.delay_ms <= Self::DELAY_MAX_MS, "delay_ms"),
        ]
        .into_iter()
        .find(|(ok, _)| !ok)
        .map(|(_, field)| field)
    }
}

/// Colour correction of an input's picture (a "look"), applied before
/// everything else: program, preview, tiles and layouts see it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct Look {
    /// -1–1, 0 = unchanged.
    pub brightness: f32,
    /// 0–2, 1 = unchanged.
    pub contrast: f32,
    /// 0–2, 1 = unchanged, 0 = grey.
    pub saturation: f32,
    /// -1–1 (±180°), 0 = unchanged.
    pub hue: f32,
    /// 0.1–3, 1 = unchanged.
    pub gamma: f32,
    /// -1–1, cooler to warmer, 0 = unchanged.
    pub temperature: f32,
    /// -1–1, green to magenta, 0 = unchanged.
    pub tint: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            brightness: 0.0,
            contrast: 1.0,
            saturation: 1.0,
            hue: 0.0,
            gamma: 1.0,
            temperature: 0.0,
            tint: 0.0,
        }
    }
}

impl Look {
    /// Whether the look leaves the picture as it is.
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    /// The first field outside its range, if any.
    pub fn out_of_range(&self) -> Option<&'static str> {
        let within = |v: f32, lo: f32, hi: f32| (lo..=hi).contains(&v);
        [
            (within(self.brightness, -1.0, 1.0), "brightness"),
            (within(self.contrast, 0.0, 2.0), "contrast"),
            (within(self.saturation, 0.0, 2.0), "saturation"),
            (within(self.hue, -1.0, 1.0), "hue"),
            (within(self.gamma, 0.1, 3.0), "gamma"),
            (within(self.temperature, -1.0, 1.0), "temperature"),
            (within(self.tint, -1.0, 1.0), "tint"),
        ]
        .into_iter()
        .find(|(ok, _)| !ok)
        .map(|(_, field)| field)
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

/// Color tag of an input tile, for grouping (cameras, clips, graphics):
/// the colour labels of Kante. Older shows said teal and pink.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum InputColor {
    Red,
    Orange,
    Yellow,
    Green,
    #[serde(alias = "teal")]
    Cyan,
    Blue,
    #[serde(alias = "pink")]
    Purple,
}

impl InputColor {
    pub const ALL: [InputColor; 7] = [
        Self::Red,
        Self::Orange,
        Self::Yellow,
        Self::Green,
        Self::Cyan,
        Self::Blue,
        Self::Purple,
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
    /// Play, restart, pause on and off air (file, URL, playlist).
    #[serde(default)]
    pub auto_play: AutoPlay,
    /// What a click on the input does (INP.8).
    #[serde(default = "InputPress::click")]
    pub click: InputPress,
    /// What its GO does (INP.9).
    #[serde(default = "InputPress::go")]
    pub go: InputPress,
    /// Solo on the monitor bus; not saved in the show file.
    #[serde(default)]
    pub listen: Listen,
    /// Channel strip (inputs with sound).
    #[serde(default)]
    pub strip: ChannelStrip,
    /// Send level to each aux bus, 0–100 (100 = unity); 0 sends nothing.
    #[serde(default)]
    pub sends: [u8; AUX_BUSES],
    /// Colour correction (inputs with a picture).
    #[serde(default)]
    pub look: Look,
    /// Actions when the input goes on or off air (AUT.7).
    #[serde(default)]
    pub triggers: Vec<crate::automation::Trigger>,
    /// Length of the item playing now (clips, playlists), as the player
    /// reports it; `None` until then and for other sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip_ms: Option<u64>,
    /// The camera's PTZ address, if it has one (PER.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ptz: Option<crate::ptz::PtzSpec>,
    /// Media files of the source that are not there (SHW.4), checked
    /// every few seconds.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_colour_names_still_load() {
        let colours: Vec<InputColor> = serde_json::from_str(r#"["teal","pink","cyan"]"#).unwrap();
        assert_eq!(
            colours,
            [InputColor::Cyan, InputColor::Purple, InputColor::Cyan]
        );
        assert_eq!(
            serde_json::to_string(&InputColor::Cyan).unwrap(),
            r#""cyan""#
        );
    }
}
