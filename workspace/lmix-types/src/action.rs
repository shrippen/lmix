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

use crate::input::{InputColor, InputRef, SourceSpec};

/// One of the four transition buttons (SWI.3), 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TransitionSlot(pub u8);

/// Number of transition buttons.
pub const TRANSITION_SLOTS: u8 = 4;

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

/// Why an action was refused. Clients show `message` (API.9: errors with
/// a reason).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ActionError {
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
    #[error("show has no file yet; give a path")]
    NoShowPath,
    #[error("show file: {message}")]
    ShowFile { message: String },
    #[error("engine: {message}")]
    Engine { message: String },
}
