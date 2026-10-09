//! The action catalog (API.1): every operation of lmix is one [`Action`].
//!
//! JSON, tagged by `action`:
//!
//! ```json
//! {"action": "preview_input", "input": "Camera 1"}
//! {"action": "transition", "slot": 1}
//! ```
//!
//! The machine-readable catalog is the JSON schema of [`Action`]
//! (`GET /api/actions`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::input::{InputColor, InputRef, Listen, SignalLoss, SourceSpec, Transport};
use crate::output::{OutputRef, OutputSource, OutputSpec};

/// One of the four transition buttons (SWI.3), 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TransitionSlot(pub u8);

/// Number of transition buttons.
pub const TRANSITION_SLOTS: u8 = 4;
/// Overlay channels above program (OVL.1), numbered 1–4.
pub const OVERLAY_CHANNELS: u8 = 4;
/// Default fade of an overlay in or out.
pub const DEFAULT_OVERLAY_MS: u32 = 500;

/// Transition effects the engine can render (SWI.4, subset so far).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Cut,
    Fade,
    DipToBlack,
    SlideLeft,
    SlideRight,
    PushLeft,
    PushRight,
    WipeLeft,
    WipeRight,
}

impl Effect {
    pub const ALL: [Effect; 9] = [
        Effect::Cut,
        Effect::Fade,
        Effect::DipToBlack,
        Effect::SlideLeft,
        Effect::SlideRight,
        Effect::PushLeft,
        Effect::PushRight,
        Effect::WipeLeft,
        Effect::WipeRight,
    ];

    /// Short button label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cut => "Cut",
            Self::Fade => "Fade",
            Self::DipToBlack => "Dip",
            Self::SlideLeft => "Slide ◀",
            Self::SlideRight => "Slide ▶",
            Self::PushLeft => "Push ◀",
            Self::PushRight => "Push ▶",
            Self::WipeLeft => "Wipe ◀",
            Self::WipeRight => "Wipe ▶",
        }
    }
}

/// Every operation, whatever triggered it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    // ---- Inputs (INP) ----
    /// Add an input; it gets the next number and a free mixer slot.
    AddInput {
        source: SourceSpec,
        #[serde(default)]
        name: Option<String>,
    },
    /// Close an input.
    RemoveInput { input: InputRef },
    /// Change what an input shows, keeping its settings (INP.4).
    ReplaceSource { input: InputRef, source: SourceSpec },
    /// Rename an input.
    RenameInput { input: InputRef, name: String },
    /// Set or clear the color tag of an input (INP.3).
    SetInputColor {
        input: InputRef,
        #[serde(default)]
        color: Option<InputColor>,
    },
    /// Choose last frame or black for when the input's source stalls.
    SetSignalLoss { input: InputRef, mode: SignalLoss },
    /// Play, pause, restart or step a file, URL or playlist input.
    TransportInput {
        input: InputRef,
        transport: Transport,
    },

    // ---- Outputs (OUT, REC, STR) ----
    /// Add an output to the show; it starts stopped.
    AddOutput {
        spec: OutputSpec,
        /// Program (default) or one input (ISO, MultiCorder).
        #[serde(default)]
        source: OutputSource,
        #[serde(default)]
        name: Option<String>,
    },
    /// Remove an output (stops it first).
    RemoveOutput { output: OutputRef },
    /// Start streaming / recording / sending.
    StartOutput { output: OutputRef },
    /// Stop it.
    StopOutput { output: OutputRef },

    // ---- Audio (AUD) ----
    /// Switch an input's sound on or off.
    SetAudio { input: InputRef, on: bool },
    /// Audio follows video: the input is heard only while on program.
    SetAudioFollow { input: InputRef, follow: bool },
    /// Set an input's volume, 0–100 (100 = unity gain).
    SetVolume { input: InputRef, volume: u8 },
    /// Listen to an input on the monitor bus, before or after its fader.
    /// While any input is soloed, the monitor bus carries only the solos;
    /// otherwise it follows program.
    SetListen { input: InputRef, listen: Listen },
    /// End every solo: the monitor bus follows program again.
    ClearListen,

    /// Move an input to a 1-based position in the input bar (INP.3).
    MoveInput { input: InputRef, number: u32 },

    // ---- Switching (SWI) ----
    /// Put an input into preview.
    PreviewInput { input: InputRef },
    /// Swap preview and program instantly.
    Cut,
    /// Put an input on program instantly, skipping preview.
    CutDirect { input: InputRef },
    /// Take preview to program with transition button `slot` (1–4).
    Transition { slot: TransitionSlot },
    /// Configure transition button `slot`.
    SetTransition {
        slot: TransitionSlot,
        effect: Effect,
        duration_ms: u32,
    },
    /// Toggle fade to black of all sinks (SWI.9).
    FadeToBlack {
        #[serde(default = "default_ftb_ms")]
        duration_ms: u32,
    },
    /// Move the T-bar (SWI.3): mix preview over program by `position`
    /// (0–1). Reaching 1 takes preview to program; back at 0 leaves both
    /// as they were.
    TBar { position: f32 },

    // ---- Overlays (OVL) ----
    /// Show `input` on overlay `channel` (1–4), above program and its
    /// transitions; replaces what the channel showed.
    OverlayIn {
        channel: u8,
        input: InputRef,
        #[serde(default = "default_overlay_ms")]
        duration_ms: u32,
    },
    /// Take overlay `channel` off.
    OverlayOut {
        channel: u8,
        #[serde(default = "default_overlay_ms")]
        duration_ms: u32,
    },

    // ---- Show (SHW) ----
    /// Start an empty show.
    NewShow,
    /// Save the show; without `path` to its current file.
    SaveShow {
        #[serde(default)]
        path: Option<String>,
    },
    /// Open a show file, replacing the current show.
    OpenShow { path: String },
}

/// Default FTB fade.
pub const DEFAULT_FTB_MS: u32 = 1000;

fn default_ftb_ms() -> u32 {
    DEFAULT_FTB_MS
}

fn default_overlay_ms() -> u32 {
    DEFAULT_OVERLAY_MS
}

/// Why an action was refused. Clients show `message` (API.9: errors with
/// a reason).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ActionError {
    #[error("no output matches {reference}")]
    UnknownOutput { reference: String },
    #[error("no input matches {reference}")]
    UnknownInput { reference: String },
    #[error("all {slots} mixer slots are in use")]
    SlotsFull { slots: usize },
    #[error("transition slot {slot} does not exist (1–{max})")]
    BadTransitionSlot { slot: u8, max: u8 },
    #[error("nothing in preview")]
    NoPreview,
    #[error("{reference} is on program; it cannot be in preview at the same time")]
    OnProgram { reference: String },
    #[error("position {number} is outside 1–{max}")]
    BadPosition { number: u32, max: u32 },
    #[error("{reference} has no picture (audio input)")]
    NoVideo { reference: String },
    #[error("{reference} has no transport (not a file, URL or playlist)")]
    NoTransport { reference: String },
    #[error("color {color} is not #rrggbb")]
    BadColor { color: String },
    #[error("volume {volume} is outside 0–{max}")]
    BadVolume { volume: u8, max: u8 },
    #[error("overlay channel {channel} does not exist (1–{max})")]
    BadOverlay { channel: u8, max: u8 },
    #[error("T-bar position is outside 0–1")]
    BadTBar,
    #[error("show has no file yet; give a path")]
    NoShowPath,
    #[error("show file: {message}")]
    ShowFile { message: String },
    #[error("engine: {message}")]
    Engine { message: String },
}
