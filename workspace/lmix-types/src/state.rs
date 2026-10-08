//! Show state as clients see it, and the events that announce changes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::action::{Effect, TransitionSlot};
use crate::input::{Input, InputId};
use crate::output::{Output, OutputStats};

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
    /// Outputs in order (OUT).
    #[serde(default)]
    pub outputs: Vec<Output>,
}

/// Current API version (API.10: additive changes within one version).
pub const API_VERSION: u32 = 1;

/// Peak level of one input, before its fader and mute (what arrives).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InputLevel {
    pub input: InputId,
    /// Loudest channel, dBFS.
    pub peak_db: f32,
}

/// Audio levels, sent about ten times a second while sound flows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Levels {
    pub inputs: Vec<InputLevel>,
    /// Program mix after faders and mutes, dBFS; `None` before the first
    /// measurement.
    pub program_peak_db: Option<f32>,
    /// Monitor bus (program, or the solos while any input is soloed).
    #[serde(default)]
    pub monitor_peak_db: Option<f32>,
}

/// Latency from capture at a source to the program output of the mixer
/// (NFA.3), from the last measurement.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Latency {
    pub samples: usize,
    pub median_ms: Option<u64>,
    pub p95_ms: Option<u64>,
    pub max_ms: Option<u64>,
}

impl Latency {
    /// Statistics of `ages_ms` (unsorted).
    pub fn of(ages_ms: &[u64]) -> Self {
        let mut sorted = ages_ms.to_vec();
        sorted.sort_unstable();
        let at = |q: usize| {
            sorted
                .get((sorted.len() * q / 100).min(sorted.len().saturating_sub(1)))
                .copied()
        };
        Self {
            samples: sorted.len(),
            median_ms: at(50),
            p95_ms: at(95),
            max_ms: sorted.last().copied(),
        }
    }
}

/// Pushed to every subscriber (API.5, API.6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum Event {
    /// Full state after any change.
    State { state: ShowState },
    /// Tally per input in input-bar order: 0 off, 1 program, 2 preview.
    Tally { tally: Vec<u8> },
    /// Audio levels (AUD), about ten times a second.
    Levels { levels: Levels },
    /// Link health of the running network outputs, about once a second.
    Outputs { stats: Vec<OutputStats> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_statistics() {
        let ages: Vec<u64> = (1..=100).rev().collect();
        let latency = Latency::of(&ages);

        assert_eq!(latency.samples, 100);
        assert_eq!(latency.median_ms, Some(51));
        assert_eq!(latency.p95_ms, Some(96));
        assert_eq!(latency.max_ms, Some(100));
        assert_eq!(Latency::of(&[]).median_ms, None);
    }
}
