//! Console application state, shared between the eframe update loop and the
//! background tokio tasks that talk to the strom backend.

use strom_types::{Flow, FlowId};

pub const VISION_MIXER_BLOCK: &str = "builtin.vision_mixer";
pub const AUDIO_MIXER_BLOCK: &str = "builtin.mixer";
pub const THUMBNAIL_BLOCK: &str = "builtin.thumbnail";

/// Messages sent from background tasks back to the UI thread.
pub enum AppMessage {
    FlowsLoaded(Vec<Flow>),
    Error(String),
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
