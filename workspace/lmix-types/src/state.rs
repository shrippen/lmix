//! Show state as clients see it, and the events that announce changes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::action::{Effect, TransitionSlot};
use crate::input::{Input, InputId};

/// One transition button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Transition {
    pub slot: TransitionSlot,
    pub effect: Effect,
    pub duration_ms: u32,
}

/// Complete state of the running show (API.3, API.4).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShowState {
    /// API version of this document.
    pub api_version: u32,
    /// File the show was loaded from or saved to.
    pub path: Option<String>,
    /// Inputs in input-bar order.
    pub inputs: Vec<Input>,
    pub preview: Option<InputId>,
    pub program: Option<InputId>,
    pub transitions: Vec<Transition>,
    pub fade_to_black: bool,
    /// Mixer slots available for inputs.
    pub slots: usize,
}

/// Current API version (API.10: additive changes within one version).
pub const API_VERSION: u32 = 1;

/// Pushed to every subscriber (API.5, API.6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Full state after any change.
    State { state: ShowState },
    /// Tally per input in input-bar order: 0 off, 1 program, 2 preview.
    Tally { tally: Vec<u8> },
}
