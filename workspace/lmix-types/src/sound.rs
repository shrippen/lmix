//! Sound cues and fades (show mode SND.1–8): a file played into zones,
//! faded and stopped at run time.
//!
//! ```text
//!   sound cue "Rain": rain.flac ─ ch 1 ─► Hall L ─┐
//!                               └ ch 2 ─► Hall R  ├─► this machine's devices
//!                               └ ch 1 ─► Foyer   ┘   (ZonePatch)
//!   fade cue: target "Rain", −20 dB over 5 s, then stop
//! ```
//!
//! JSON of a sound cue's `sound`, everything but `file` optional:
//!
//! ```json
//! {"file": "rain.flac", "gain_db": -6,
//!  "routes": [{"channel": 1, "zone": "Hall", "zone_channel": 1}],
//!  "fade_in_ms": 500, "fade_out_ms": 2000}
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Lowest level of cues, crosspoints and fades: silence (SND.13).
pub const SOUND_GAIN_MIN_DB: f32 = -60.0;

/// Highest level (SND.13).
pub const SOUND_GAIN_MAX_DB: f32 = 12.0;

/// Crosspoints of one sound cue (SND.5).
pub const SOUND_ROUTES: usize = 64;

/// Highest file channel a route may name (SND.1: up to 24 channels).
pub const SOUND_FILE_CHANNELS: u8 = 24;

/// Longest fade, fade in or fade out: 10 min.
pub const SOUND_FADE_MAX_MS: u32 = 600_000;

/// One crosspoint: a channel of the file into a channel of a zone (SND.5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SoundRoute {
    /// File channel, from 1.
    pub channel: u8,
    /// The zone's name.
    pub zone: String,
    /// Zone channel, from 1.
    pub zone_channel: u8,
    #[serde(default)]
    pub gain_db: f32,
}

/// What a sound cue plays (SND.1, SND.2, SND.5).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SoundCue {
    /// Absolute path of the file.
    pub file: String,
    /// Level of the whole cue, dB (0 = as in the file).
    #[serde(default)]
    pub gain_db: f32,
    /// Where its channels go. Empty: file channel n to channel n of the
    /// show's first zone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub routes: Vec<SoundRoute>,
    /// Fade in from silence when it starts.
    #[serde(default)]
    pub fade_in_ms: u32,
    /// Fade out when it is stopped (a panic uses the panic time).
    #[serde(default)]
    pub fade_out_ms: u32,
}

/// What a fade leaves behind (SND.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AfterFade {
    /// The target plays on at the new level.
    #[default]
    Keep,
    /// The target stops when the fade is over (cues only).
    Stop,
}

/// What a fade cue fades (SND.6, SND.11, VIS.4, VIS.7).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FadeTarget {
    /// A sound or picture cue, by number or id; looked for in the fade
    /// cue's list first.
    Cue(String),
    /// A zone, by name.
    Zone(String),
    /// A stage, by name.
    Stage(String),
}

/// What a fade cue does; it runs for the cue's `duration_ms`. Only the
/// values set change (SND.6): `gain_db` for a sound cue or a zone,
/// `opacity` for a picture cue, `brightness` for a stage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct FadeCue {
    pub target: FadeTarget,
    /// Level, dB; [`SOUND_GAIN_MIN_DB`] is silence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gain_db: Option<f32>,
    /// 0 (clear) to 1.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub opacity: Option<f32>,
    /// 0 (black) to 1 (VIS.7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub brightness: Option<f32>,
    /// `stop`: the target cue stops when the fade is over.
    #[serde(default)]
    pub then: AfterFade,
}
