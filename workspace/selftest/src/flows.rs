//! Finding the vision mixer and its thumbnail taps in strom flows.

use strom_types::Flow;

const VISION_MIXER_BLOCK: &str = "builtin.vision_mixer";
const THUMBNAIL_BLOCK: &str = "builtin.thumbnail";

/// Block id of the flow's vision mixer, if it has one.
pub fn vision_mixer_block(flow: &Flow) -> Option<String> {
    flow.blocks
        .iter()
        .find(|b| b.block_definition_id == VISION_MIXER_BLOCK)
        .map(|b| b.id.clone())
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
