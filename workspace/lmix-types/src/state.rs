//! Show state as clients see it, and the events that announce changes.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::action::{Effect, TransitionSlot, DEFAULT_OVERLAY_MS, OVERLAY_CHANNELS};
use crate::bus::{Bus, BusPeak, Fader, Phones};
use crate::input::{Input, InputId};
use crate::layout::{BoxRect, Layout, LayoutTemplate};
use crate::output::{Output, OutputStats};
use crate::shortcut::Shortcut;

/// What was removed last and `restore` brings back.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Restorable {
    pub kind: RemovedKind,
    pub name: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RemovedKind {
    Input,
    Layout,
    Bus,
    Output,
}

/// One transition button.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Transition {
    pub slot: TransitionSlot,
    pub effect: Effect,
    pub duration_ms: u32,
    /// The clip and cut point of a stinger (`effect` stinger).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stinger: Option<Stinger>,
}

/// Quick play (SWI.8): its own effect for taking an input to program
/// and playing it, apart from the transition buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct QuickPlay {
    pub effect: Effect,
    pub duration_ms: u32,
}

impl Default for QuickPlay {
    /// Half a second of fade.
    fn default() -> Self {
        Self {
            effect: Effect::Fade,
            duration_ms: 500,
        }
    }
}

/// Settings of one overlay channel (OVL.2): how it goes on and off, for
/// how long it stays, and where it sits.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OverlayChannel {
    /// `cut` or `fade` (the engine fades DSK layers only); going off
    /// runs it backwards.
    pub effect: Effect,
    pub duration_ms: u32,
    /// Goes off by itself this long after it went on; `None` stays on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_off_ms: Option<u32>,
    /// Picture in picture: the overlay in this rectangle (fractions of
    /// the picture, aspect kept); `None` fills the picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rect: Option<BoxRect>,
}

impl Default for OverlayChannel {
    /// Full screen, half a second of fade, no auto off.
    fn default() -> Self {
        Self {
            effect: Effect::Fade,
            duration_ms: DEFAULT_OVERLAY_MS,
            auto_off_ms: None,
            rect: None,
        }
    }
}

impl OverlayChannel {
    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    /// Length of going on or off: none for a cut.
    pub fn fade_ms(&self) -> u32 {
        match self.effect {
            Effect::Cut => 0,
            _ => self.duration_ms,
        }
    }
}

/// A stinger: a clip (with alpha) the engine plays to its end over
/// program; at `cut_ms` into it program cuts to preview underneath. The
/// button's `duration_ms` holds switching while the clip's length is not
/// known.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Stinger {
    pub input: InputId,
    pub cut_ms: u32,
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
    /// Effect and length of quick play (SWI.8).
    #[serde(default)]
    pub quick_play: QuickPlay,
    pub fade_to_black: bool,
    /// Mixer slots available for inputs.
    pub slots: usize,
    /// Outputs in order (OUT).
    #[serde(default)]
    pub outputs: Vec<Output>,
    /// What each overlay channel shows, channel 1 first.
    #[serde(default)]
    pub overlays: [Option<InputId>; OVERLAY_CHANNELS as usize],
    /// The input each overlay channel showed last, for `overlay_last`;
    /// runtime only, not saved.
    #[serde(default)]
    pub overlay_last: [Option<InputId>; OVERLAY_CHANNELS as usize],
    /// Effect, length, auto off and place of each overlay channel (OVL.2).
    #[serde(default)]
    pub overlay_channels: [OverlayChannel; OVERLAY_CHANNELS as usize],
    /// The input each overlay channel gets with the next take of preview
    /// (OVL.3); it has preview tally until then.
    #[serde(default)]
    pub overlay_preview: [Option<InputId>; OVERLAY_CHANNELS as usize],
    /// T-bar position (0 = at rest, below 1 = preview partly mixed in).
    #[serde(default)]
    pub t_bar: f32,
    /// The show's buses besides master and headphones, in their order
    /// (AUD.2).
    #[serde(default)]
    pub buses: Vec<Bus>,
    /// Master of program sound (AUD.2).
    #[serde(default)]
    pub master: Fader,
    /// Picture size and rate of program (OUT.1).
    #[serde(default)]
    pub format: crate::output::MasterFormat,
    /// Layout of the multiview (OUT.10).
    #[serde(default)]
    pub multiview: crate::output::MultiviewLayout,
    /// Sample rate of the audio mixer, Hz (AUD.17).
    #[serde(default = "crate::output::default_sample_rate")]
    pub sample_rate: u32,
    /// The headphones (AUD.4).
    #[serde(default)]
    pub phones: Phones,
    /// Delay of new inputs with sound, ms (AUD.6).
    #[serde(default)]
    pub default_delay_ms: u32,
    /// Whether the engine can apply looks (the GL mixer can).
    #[serde(default)]
    pub looks_available: bool,
    /// Layouts; `preview` and `program` may name one.
    #[serde(default)]
    pub layouts: Vec<Layout>,
    /// Layout templates (LAY.5), kept in lmix-core's settings for every
    /// show.
    #[serde(default)]
    pub layout_templates: Vec<LayoutTemplate>,
    /// Shortcuts of the show.
    #[serde(default)]
    pub shortcuts: Vec<Shortcut>,
    /// Cue lists of the show.
    #[serde(default)]
    pub cue_lists: Vec<crate::cue::CueList>,
    /// Cues that run now, in start order (RUN.3).
    #[serde(default)]
    pub running_cues: Vec<crate::cue::RunningCue>,
    /// Shortest gap between two GOs (RUN.6).
    #[serde(default)]
    pub go_guard_ms: u32,
    /// When GO takes another press (RUN.6).
    #[serde(default)]
    pub go_rearm: crate::cue::GoRearm,
    /// Panic time (SAF.2).
    #[serde(default)]
    pub panic_ms: u32,
    /// The show's script folder (SSH.2).
    #[serde(default)]
    pub script_folder: Option<String>,
    /// The show file's older format version, migrated on open; the next
    /// save over it keeps the old file as `<file>.v<n>.bak` (SHW.7).
    #[serde(default)]
    pub older_format: Option<u32>,
    /// What would go wrong when the cue lists run (SUI.5).
    #[serde(default)]
    pub cue_warnings: Vec<crate::cue::CueWarning>,
    /// Output targets of this machine, without secrets (EXT.1).
    #[serde(default)]
    pub targets: Vec<crate::target::Target>,
    /// Fixed OSC subscribers of this machine (CTL.4).
    #[serde(default)]
    pub feeds: Vec<crate::feed::Feed>,
    /// The show's zones with this machine's outputs (SND.10).
    #[serde(default)]
    pub zones: Vec<crate::zone::ZoneState>,
    /// The show's stages with this machine's screens (VIS.1).
    #[serde(default)]
    pub stages: Vec<crate::stage::StageState>,
    /// Locked against closing, loading and stopping (AUT.11).
    #[serde(default)]
    pub locked: bool,
    /// Ways in and out that are locked (`set_io_lock`, RUN.9).
    #[serde(default)]
    pub io_locked: Vec<crate::action::IoClass>,
    /// lmix uses the capture and playout cards (`set_cards`).
    #[serde(default)]
    pub cards_on: bool,
    /// MIDI controller that moves the T-bar (AUT.1).
    #[serde(default)]
    pub midi_tbar: Option<crate::midi::MidiBinding>,
    /// What `restore` would bring back.
    #[serde(default)]
    pub restorable: Option<Restorable>,
    /// Graphics channels (titles).
    #[serde(default)]
    pub graphics: Vec<crate::graphics::Graphics>,
    /// Saved title data sets, by name.
    #[serde(default)]
    pub datasets: Vec<crate::graphics::Dataset>,
    /// Data sources for title fields.
    #[serde(default)]
    pub data_sources: Vec<crate::graphics::DataSource>,
    /// What `"dynamic1"`–`"dynamic4"` name (AUT.5).
    #[serde(default)]
    pub dynamic_inputs: [Option<InputId>; 4],
    /// Dynamic values 1–4.
    #[serde(default)]
    pub dynamic_values: [String; 4],
}

/// Current API version (API.10: additive changes within one version).
pub const API_VERSION: u32 = 1;

/// Peak level of one input, before its fader and mute (what arrives).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct InputLevel {
    pub input: InputId,
    /// Loudest channel, dBFS.
    pub peak_db: f32,
    /// Each channel, dBFS: left, right (AUD.1); empty before the first
    /// measurement.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub channels_db: Vec<f32>,
}

/// Audio levels, sent about ten times a second while sound flows.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Levels {
    pub inputs: Vec<InputLevel>,
    /// Program mix after faders and mutes, dBFS; `None` before the first
    /// measurement.
    pub program_peak_db: Option<f32>,
    /// Headphones (their mix, or the solos while any is soloed).
    #[serde(default)]
    pub phones_peak_db: Option<f32>,
    /// Each bus after its master, in the show's order.
    #[serde(default)]
    pub buses: Vec<BusPeak>,
    /// Each zone after its level, in the show's order (SUI.8).
    #[serde(default)]
    pub zones: Vec<ZonePeak>,
}

/// A zone's level as it plays (SUI.8).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ZonePeak {
    pub zone: String,
    /// Loudest channel, dBFS; `None` before the first measurement.
    pub peak_db: Option<f32>,
}

/// Live picture for a client on the engine's machine (`GET /api/live/…`):
/// a Unix socket with raw frames, read as `transport` says.
/// Clients elsewhere use the JPEG pictures.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LiveFeed {
    pub socket: String,
    #[serde(default)]
    pub transport: LiveTransport,
}

/// How a live picture's frames come over its socket.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum LiveTransport {
    /// Frames in system memory, read with `unixfdsrc`.
    #[default]
    UnixFd,
    /// Frames in GPU memory of the engine's CUDA device, read with
    /// `cudaipcsrc`: they never leave the GPU.
    CudaIpc,
}

/// Every slot live in one picture (`GET /api/live/tiles`): slot `n` in cell
/// `n` of a grid of `columns` × `rows` cells, row by row, each picture
/// fitted into its cell. A Console shows its input tiles from this one
/// stream instead of one per input.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LiveTiles {
    pub socket: String,
    pub transport: LiveTransport,
    pub columns: u32,
    pub rows: u32,
    pub cell_width: u32,
    pub cell_height: u32,
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
        // Nearest rank: the ceil(n * q / 100)-th smallest sample.
        let at = |q: usize| {
            let rank = (sorted.len() * q).div_ceil(100).max(1);
            sorted.get(rank - 1).copied()
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
// Built once per change and shared; the size does not matter.
#[allow(clippy::large_enum_variant)]
pub enum Event {
    /// Full state after any change.
    State { state: ShowState },
    /// Tally per input in input-bar order: 0 off, 1 program, 2 preview.
    Tally { tally: Vec<u8> },
    /// Audio levels (AUD), about ten times a second.
    Levels { levels: Levels },
    /// Link health of the running network outputs, about once a second.
    Outputs { stats: Vec<OutputStats> },
    /// A note or controller message from a MIDI controller (to learn a
    /// binding).
    Midi { midi: crate::midi::MidiMessage },
    /// How busy the machine is and how the cue timer runs, about every
    /// two seconds (RUN.8).
    Machine { load: crate::machine::MachineLoad },
    /// Where a clip or playlist input plays, about five times a second
    /// while it plays (UI.5).
    Position {
        input: InputId,
        position_ms: u64,
        duration_ms: u64,
    },
    /// A title template reported something (`title_report`), e.g.
    /// `done` or `zero`.
    Title {
        graphics: InputId,
        layer: u32,
        report: String,
    },
    /// The running cues with their remaining time, at the cue event rate
    /// (default 4 Hz) while cues run, once more when none is left
    /// (CTL.2).
    Running { cues: Vec<crate::cue::RunningCue> },
    /// Panic (`mode: fade`, over the panic time) or hard stop (`hard`),
    /// from any control (SAF.1, SAF.3, CTL.2).
    Panic { mode: crate::cue::StopMode },
    /// How a message to an output target went (EXT.3, CTL.2).
    Target {
        target: String,
        #[serde(flatten)]
        result: crate::target::TargetResult,
    },
    /// A cue started, ended, stopped or failed, or a GO was dropped
    /// (RUN.6); `cue` is `null` for a GO.
    Cue {
        list: InputId,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cue: Option<crate::cue::CueId>,
        #[serde(flatten)]
        what: crate::cue::CueEvent,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_statistics() {
        let ages: Vec<u64> = (1..=100).rev().collect();
        let latency = Latency::of(&ages);

        // Nearest rank: the smallest value at least q % of the samples
        // are at or below.
        assert_eq!(latency.samples, 100);
        assert_eq!(latency.median_ms, Some(50));
        assert_eq!(latency.p95_ms, Some(95));
        assert_eq!(latency.max_ms, Some(100));
        assert_eq!(Latency::of(&[]).median_ms, None);
    }

    /// With 20 samples, one slow one is the 5 % above p95, not p95
    /// itself (the VM latency check takes about 20).
    #[test]
    fn one_slow_sample_in_twenty_is_above_p95() {
        let mut ages: Vec<u64> = (1..=19).collect();
        ages.push(500);
        let latency = Latency::of(&ages);
        assert_eq!(latency.p95_ms, Some(19));
        assert_eq!(latency.max_ms, Some(500));
        assert_eq!(Latency::of(&[7]).p95_ms, Some(7));
    }
}
