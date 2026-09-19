//! Console application state, shared between the eframe update loop and the
//! background tokio tasks that talk to the strom backend.

use std::collections::HashMap;
use strom_types::api::VisionMixerState;
use strom_types::{Flow, FlowId, PropertyValue, StromEvent};

pub const VISION_MIXER_BLOCK: &str = "builtin.vision_mixer";
pub const AUDIO_MIXER_BLOCK: &str = "builtin.mixer";
pub const THUMBNAIL_BLOCK: &str = "builtin.thumbnail";

/// Timed transitions offered by the Console's AUTO button. The wire names
/// are strom's `TransitionType` strings (backend/src/gst/transitions/mod.rs).
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum AutoTransition {
    Fade,
    DipToBlack,
    WipeLeft,
    WipeRight,
    SlideLeft,
    PushLeft,
}

impl AutoTransition {
    pub const ALL: [Self; 6] = [
        Self::Fade,
        Self::DipToBlack,
        Self::WipeLeft,
        Self::WipeRight,
        Self::SlideLeft,
        Self::PushLeft,
    ];

    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Fade => "fade",
            Self::DipToBlack => "dip_to_black",
            Self::WipeLeft => "wipe_left",
            Self::WipeRight => "wipe_right",
            Self::SlideLeft => "slide_left",
            Self::PushLeft => "push_left",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Fade => "Fade",
            Self::DipToBlack => "Dip to black",
            Self::WipeLeft => "Wipe left",
            Self::WipeRight => "Wipe right",
            Self::SlideLeft => "Slide left",
            Self::PushLeft => "Push left",
        }
    }
}

/// Messages sent from background tasks back to the UI thread.
pub enum AppMessage {
    FlowsLoaded(Vec<Flow>),
    Error(String),
    DevicesLoaded(Vec<strom_types::discovery::DeviceResponse>),
    /// A flow the Console just created from its source list.
    FlowCreated(Flow),
    VisionMixerStateLoaded(VisionMixerState),
    /// A WebSocket event from the backend - the authoritative source of
    /// truth for live state, never overridden by local optimistic updates.
    Event(StromEvent),
    /// A decoded thumbnail for one Vision Mixer input, ready to upload as a
    /// texture on the UI thread.
    ThumbnailDecoded(usize, egui::ColorImage),
    /// Authoritative PVW/PGM state returned directly by a `preview` or
    /// `transition` REST call - applied immediately for snappy feedback,
    /// the WS event that follows just confirms the same values.
    MixerUpdate {
        preview_input: Option<usize>,
        program_input: Option<usize>,
    },
    /// Initial audio mixer channel count + current fader/mute values.
    AudioMixerLoaded {
        num_channels: usize,
        properties: HashMap<String, PropertyValue>,
    },
}

/// Parse a mixer meter `element_id` of the form `{block_id}:meter:{N}` into
/// the 1-based channel number. `None` for other meters (main, aux, ...) or
/// another block.
pub fn meter_channel(element_id: &str, audio_block_id: &str) -> Option<usize> {
    element_id
        .strip_prefix(audio_block_id)?
        .strip_prefix(":meter:")?
        .parse()
        .ok()
}

/// A `builtin.mixer` block's channel strips, only as much as the Console's
/// simplified audio strip needs (not the full X32-style panel strom-frontend
/// has in `frontend/src/mixer/`).
#[derive(Default)]
pub struct AudioMixerState {
    pub num_channels: usize,
    /// 1-based channel index -> current fader value (0.0-2.0, 1.0 = unity).
    pub faders: HashMap<usize, f64>,
    pub mutes: HashMap<usize, bool>,
    /// 1-based channel index -> latest peak in dB (max over the channel's
    /// audio channels), from `MeterData` WS events.
    pub peaks_db: HashMap<usize, f64>,
}

impl AudioMixerState {
    pub fn from_properties(num_channels: usize, properties: &HashMap<String, PropertyValue>) -> Self {
        let mut faders = HashMap::new();
        let mut mutes = HashMap::new();
        for ch in 1..=num_channels {
            if let Some(PropertyValue::Float(v)) = properties.get(&format!("ch{ch}_fader")) {
                faders.insert(ch, *v);
            }
            if let Some(PropertyValue::Bool(v)) = properties.get(&format!("ch{ch}_mute")) {
                mutes.insert(ch, *v);
            }
        }
        Self {
            num_channels,
            faders,
            mutes,
            peaks_db: HashMap::new(),
        }
    }
}

/// A flow that has at least one Vision Mixer block, ready to be switched.
pub struct SwitchableFlow {
    pub flow_id: FlowId,
    pub name: String,
    pub running: bool,
    pub vision_mixer_block_id: String,
    pub audio_mixer_block_id: Option<String>,
}

pub fn find_switchable_flows(flows: &[Flow]) -> Vec<SwitchableFlow> {
    flows
        .iter()
        .filter_map(|flow| {
            let vision_mixer_block_id = flow
                .blocks
                .iter()
                .find(|b| b.block_definition_id == VISION_MIXER_BLOCK)
                .map(|b| b.id.clone())?;
            let audio_mixer_block_id = flow
                .blocks
                .iter()
                .find(|b| b.block_definition_id == AUDIO_MIXER_BLOCK)
                .map(|b| b.id.clone());
            Some(SwitchableFlow {
                flow_id: flow.id,
                name: flow.name.clone(),
                running: flow.running,
                vision_mixer_block_id,
                audio_mixer_block_id,
            })
        })
        .collect()
}

/// A selected flow's Vision Mixer target - what the Console is currently
/// driving.
#[derive(Clone)]
pub struct Selection {
    pub flow_id: FlowId,
    pub flow_name: String,
    pub vision_mixer_block_id: String,
    pub audio_mixer_block_id: Option<String>,
}

/// Find the `builtin.thumbnail` block feeding a given Vision Mixer input
/// index, by walking the flow's link graph backward from
/// `{vision_mixer_block_id}:video_in_{index}`. Returns `None` if that input
/// isn't fed through a thumbnail tap (no live preview available for it) -
/// this is a convention our own flows follow, not a strom requirement.
pub fn find_thumbnail_block(
    flow: &Flow,
    vision_mixer_block_id: &str,
    input_index: usize,
) -> Option<String> {
    let target = format!("{vision_mixer_block_id}:video_in_{input_index}");
    let link = flow.links.iter().find(|l| l.to == target)?;
    let (from_id, _pad) = link.from.split_once(':')?;
    flow.blocks
        .iter()
        .find(|b| b.id == from_id && b.block_definition_id == THUMBNAIL_BLOCK)
        .map(|b| b.id.clone())
}

/// Live mirror of a Vision Mixer's PVW/PGM bus state, updated from the
/// initial `GET .../state` fetch and then exclusively from
/// `VisionMixerStateChanged` WS events - never from local optimistic
/// assumptions (concurrent API operations, e.g. another client or curl,
/// must always win).
#[derive(Clone, Default)]
pub struct MixerState {
    pub program_input: Option<usize>,
    pub preview_input: Option<usize>,
    pub num_inputs: usize,
}

impl From<VisionMixerState> for MixerState {
    fn from(s: VisionMixerState) -> Self {
        Self {
            program_input: s.program_input,
            preview_input: s.preview_input,
            num_inputs: s.input_resolutions.len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_channel_parses_channels_only() {
        assert_eq!(meter_channel("mix:meter:3", "mix"), Some(3));
        assert_eq!(meter_channel("mix:meter:main", "mix"), None);
        assert_eq!(meter_channel("other:meter:3", "mix"), None);
    }
}
