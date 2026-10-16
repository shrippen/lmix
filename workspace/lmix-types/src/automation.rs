//! Automation: triggers per input (AUT.7). Cue lists: [`crate::cue`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::action::Action;

/// Longest wait of a trigger before its actions.
pub const TRIGGER_DELAY_MAX_MS: u32 = 30_000;

/// Triggers per input.
pub const TRIGGERS: usize = 8;

/// Actions per trigger or cue.
pub const AUTOMATION_ACTIONS: usize = 16;

/// When a trigger fires.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TriggerOn {
    /// The input goes on program.
    Program,
    /// The input leaves program.
    ProgramOff,
    /// The input goes into preview.
    Preview,
    /// The input goes on an overlay channel.
    OverlayIn,
    /// The input leaves its overlay channel.
    OverlayOut,
    /// A clip or playlist input reaches its end (the last item ends), or
    /// `before_ms` before it (countdown).
    ClipEnd,
    /// A clip or playlist item has played `at_ms` (each item, each time
    /// it plays through that point).
    PlayTime,
}

/// Actions run when something happens to an input, after a delay. They
/// fire no triggers themselves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Trigger {
    pub on: TriggerOn,
    /// 0–30 000 ms.
    #[serde(default)]
    pub delay_ms: u32,
    /// With `clip_end`: fire this long before the end instead of at it,
    /// 0–60 000 ms.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub before_ms: Option<u32>,
    /// With `play_time`: the playing time that fires it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at_ms: Option<u32>,
    pub actions: Vec<Action>,
}
