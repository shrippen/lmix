//! Stages and picture cues (show mode VIS.1–8): named picture surfaces of
//! the show, and videos or still images played onto them. Like zones, the
//! show names a stage; which screens show it is set per machine
//! (`StagePatch`).
//!
//! ```text
//!   show: Stage "Back", 1920 × 1080
//!   this machine: Back → fullscreen; Back → window (confidence monitor)
//!   picture cue "Forest": forest.mp4 on Back, layer 1, fade in 2 s
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::output::DisplayMode;

/// Stages per show (VIS.1: at least 4).
pub const STAGES: usize = 8;

/// Screens a stage shows on at once.
pub const STAGE_OUTPUTS: usize = 4;

/// Smallest and largest stage side, px.
pub const STAGE_MIN_PX: u32 = 16;
pub const STAGE_MAX_PX: u32 = 7680;

/// Highest picture layer (VIS.3: 1–999; 0 is the very bottom).
pub const PICTURE_LAYER_MAX: u16 = 999;

/// A stage of the show.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Stage {
    pub name: String,
    pub width: u32,
    pub height: u32,
}

/// One screen a stage shows on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StageOutput {
    pub display: DisplayMode,
}

/// A stage's screens on this machine (machine settings).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StagePatch {
    /// The stage's name.
    pub stage: String,
    pub outputs: Vec<StageOutput>,
}

/// A stage as clients see it: the show's, with this machine's screens.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StageState {
    pub name: String,
    pub width: u32,
    pub height: u32,
    /// Empty: the stage shows nowhere on this machine.
    pub outputs: Vec<StageOutput>,
}

/// How a picture fills its stage (VIS.5).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PictureFit {
    /// Whole picture, aspect kept, black where it does not fill.
    #[default]
    Fit,
    /// Stage filled, aspect kept, the picture cut where it overhangs.
    Fill,
    /// Stage filled, aspect changed.
    Stretch,
}

/// What a video does at its end (VIS.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PictureEnd {
    /// Its last frame stays until the cue is stopped.
    #[default]
    Hold,
    /// It leaves the stage, and the cue ends.
    Stop,
}

/// What a picture cue shows (VIS.2–5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PictureCue {
    /// Absolute path of the video or image.
    pub file: String,
    /// The stage's name.
    pub stage: String,
    /// 0–999; on the same layer the later cue is in front (VIS.3).
    #[serde(default = "first_layer")]
    pub layer: u16,
    /// 0–1 (VIS.4).
    #[serde(default = "opaque")]
    pub opacity: f32,
    #[serde(default)]
    pub fit: PictureFit,
    #[serde(default)]
    pub end: PictureEnd,
    /// Fade in from clear when it starts.
    #[serde(default)]
    pub fade_in_ms: u32,
    /// Fade out when it is stopped (a panic uses the panic time).
    #[serde(default)]
    pub fade_out_ms: u32,
}

fn first_layer() -> u16 {
    1
}

fn opaque() -> f32 {
    1.0
}
