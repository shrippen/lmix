//! strom flows of a show.
//!
//! ```text
//!   slot flow 0: source ─► [caps] ─► inter_output "out" ┐
//!   slot flow 1: source ─► [caps] ─► inter_output "out" ├─► mixer flow:
//!   …                                                   ┘   inter_input × N ─► vision mixer
//! ```
//!
//! Flow ids are fixed per slot (UUID v5), so the `inter` channel name
//! `strom_<flow>_<block>` never changes: swapping a source rebuilds only
//! its slot flow, the mixer keeps running (INP.1, ROADMAP 2b.1).

use strom_types::block::{BlockInstance, Position};
use strom_types::element::{Element, Link};
use strom_types::{Flow, FlowId, PropertyValue};
use uuid::Uuid;

use lmix_types::SourceSpec;

/// Namespace of all lmix flow ids.
const FLOW_NAMESPACE: Uuid = Uuid::from_u128(0x6c6d_6978_2d63_6f72_652d_666c_6f77_7321);

const MEDIA_PLAYER_BLOCK: &str = "builtin.media_player";
const SRT_INPUT_BLOCK: &str = "builtin.mpegtssrt_input";
const LOCAL_INPUT_BLOCK: &str = "builtin.local_input";
const INTER_OUTPUT_BLOCK: &str = "builtin.inter_output";
const INTER_INPUT_BLOCK: &str = "builtin.inter_input";
const VISION_MIXER_BLOCK: &str = "builtin.vision_mixer";
const THUMBNAIL_BLOCK: &str = "builtin.thumbnail";

/// Block id of every slot flow's inter output.
const SLOT_OUT: &str = "out";
/// Block id of the vision mixer in the mixer flow.
pub const MIXER_BLOCK: &str = "vm";
/// Thumbnail tap on the program output: GUI monitor and picture checks.
pub const PGM_TAP: &str = "tap_pgm";
/// Subscriber buffer of the mixer's inter inputs: low latency, some jitter
/// headroom (strom offers 100–2000 ms).
const INTER_BUFFER_MS: &str = "100";

/// Format test patterns are rendered in; real sources keep theirs.
const PATTERN_WIDTH: u32 = 1280;
const PATTERN_HEIGHT: u32 = 720;
const PATTERN_FPS: u32 = 30;
const PATTERN_FORMAT: &str = "NV12";

/// Vision mixer compositor (`compositor_preference`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum MixerBackend {
    /// NVIDIA CUDA compositor.
    Cuda,
    /// OpenGL compositor (AMD, Intel).
    Gl,
    /// CPU compositor: emergency and VM tests only.
    Cpu,
}

impl MixerBackend {
    fn property(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Gl => "gpu",
            Self::Cpu => "cpu",
        }
    }
}

pub fn mixer_id() -> FlowId {
    Uuid::new_v5(&FLOW_NAMESPACE, b"mixer")
}

pub fn slot_id(slot: usize) -> FlowId {
    Uuid::new_v5(&FLOW_NAMESPACE, format!("slot-{slot}").as_bytes())
}

/// `inter` channel a slot flow publishes on (see strom `inter.rs`).
fn slot_channel(slot: usize) -> String {
    format!("strom_{}_{}", slot_id(slot), SLOT_OUT)
}

/// Mixer input that always shows black: program when lmix has nothing on
/// program (the engine always has some input there).
pub fn black_input(slots: usize) -> usize {
    slots
}

/// Mixer: one inter input per slot, plus black, into the vision mixer.
pub fn mixer_flow(slots: usize, backend: MixerBackend) -> Flow {
    let mut flow = Flow::new("lmix mixer");
    flow.id = mixer_id();

    for slot in 0..slots {
        let input = format!("in{slot}");
        flow.blocks.push(block(
            &input,
            INTER_INPUT_BLOCK,
            vec![
                ("channel", PropertyValue::String(slot_channel(slot))),
                ("max_time", PropertyValue::String(INTER_BUFFER_MS.into())),
            ],
        ));
        flow.links.push(link(
            format!("{input}:src"),
            format!("{MIXER_BLOCK}:video_in_{slot}"),
        ));
    }

    let black = black_input(slots);
    flow.elements.push(element(
        "black",
        "videotestsrc",
        vec![
            ("pattern", PropertyValue::String("black".into())),
            ("is-live", PropertyValue::Bool(true)),
        ],
    ));
    flow.elements.push(element(
        "black_caps",
        "capsfilter",
        vec![("caps", PropertyValue::String(pattern_caps()))],
    ));
    flow.links.push(link("black".into(), "black_caps".into()));
    flow.links.push(link(
        "black_caps".into(),
        format!("{MIXER_BLOCK}:video_in_{black}"),
    ));

    flow.blocks.push(block(
        MIXER_BLOCK,
        VISION_MIXER_BLOCK,
        vec![
            ("num_inputs", PropertyValue::Int(black as i64 + 1)),
            (
                "compositor_preference",
                PropertyValue::String(backend.property().into()),
            ),
        ],
    ));

    flow.blocks.push(block(PGM_TAP, THUMBNAIL_BLOCK, vec![]));
    flow.links.push(link(
        format!("{MIXER_BLOCK}:pgm_out"),
        format!("{PGM_TAP}:video_in"),
    ));
    flow
}

/// One slot: the source, published on the slot's inter channel.
pub fn slot_flow(slot: usize, source: &SourceSpec) -> Flow {
    let mut flow = Flow::new(format!("lmix slot {slot}"));
    flow.id = slot_id(slot);

    let out = add_source(&mut flow, source);
    flow.blocks.push(block(SLOT_OUT, INTER_OUTPUT_BLOCK, vec![]));
    flow.links.push(link(out, format!("{SLOT_OUT}:sink")));
    flow
}

/// Add the source and return the pad that carries its video.
fn add_source(flow: &mut Flow, source: &SourceSpec) -> String {
    match source {
        SourceSpec::Pattern { pattern } => {
            flow.elements.push(element(
                "src",
                "videotestsrc",
                vec![
                    ("pattern", PropertyValue::String(pattern.nick().into())),
                    ("is-live", PropertyValue::Bool(true)),
                ],
            ));
            flow.elements.push(element(
                "caps",
                "capsfilter",
                vec![("caps", PropertyValue::String(pattern_caps()))],
            ));
            flow.links.push(link("src".into(), "caps".into()));
            "caps".into()
        }
        SourceSpec::File { path: location } | SourceSpec::Url { url: location } => {
            let playlist = serde_json::to_string(&[location]).expect("playlist serializes");
            flow.blocks.push(block(
                "player",
                MEDIA_PLAYER_BLOCK,
                vec![
                    ("playlist", PropertyValue::String(playlist)),
                    ("decode", PropertyValue::Bool(true)),
                    ("loop_playlist", PropertyValue::Bool(true)),
                ],
            ));
            "player:video_out".into()
        }
        SourceSpec::Srt { uri } => {
            flow.blocks.push(block(
                "srt",
                SRT_INPUT_BLOCK,
                vec![
                    ("srt_uri", PropertyValue::String(uri.clone())),
                    ("decode", PropertyValue::Bool(true)),
                    ("num_video_tracks", PropertyValue::UInt(1)),
                    ("num_audio_tracks", PropertyValue::UInt(0)),
                ],
            ));
            "srt:video_out".into()
        }
        SourceSpec::Device { id } => {
            flow.blocks.push(block(
                "dev",
                LOCAL_INPUT_BLOCK,
                vec![
                    ("stream_mode", PropertyValue::String("video".into())),
                    ("video_device", PropertyValue::String(id.clone())),
                ],
            ));
            "dev:video_out".into()
        }
    }
}

fn pattern_caps() -> String {
    format!(
        "video/x-raw,format={PATTERN_FORMAT},width={PATTERN_WIDTH},height={PATTERN_HEIGHT},framerate={PATTERN_FPS}/1"
    )
}

fn block(id: &str, definition: &str, properties: Vec<(&str, PropertyValue)>) -> BlockInstance {
    BlockInstance {
        id: id.into(),
        block_definition_id: definition.into(),
        name: None,
        properties: properties
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        position: Position { x: 0.0, y: 0.0 },
        runtime_data: None,
        computed_external_pads: None,
    }
}

fn element(id: &str, kind: &str, properties: Vec<(&str, PropertyValue)>) -> Element {
    Element {
        id: id.into(),
        element_type: kind.into(),
        properties: properties
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        pad_properties: Default::default(),
        position: (0.0, 0.0),
    }
}

fn link(from: String, to: String) -> Link {
    Link { from, to }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::TestPattern;

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(slot_id(3), slot_id(3));
        assert_ne!(slot_id(3), slot_id(4));
        assert_ne!(slot_id(0), mixer_id());
    }

    #[test]
    fn mixer_subscribes_each_slot_channel() {
        let flow = mixer_flow(3, MixerBackend::Cpu);

        for slot in 0..3 {
            let input = flow.blocks.iter().find(|b| b.id == format!("in{slot}")).unwrap();
            assert!(matches!(
                input.properties.get("channel"),
                Some(PropertyValue::String(c)) if *c == slot_channel(slot)
            ));
            assert!(flow.links.iter().any(|l| l.to == format!("vm:video_in_{slot}")));
        }
    }

    #[test]
    fn mixer_has_black_after_slots() {
        let flow = mixer_flow(3, MixerBackend::Cpu);

        assert_eq!(black_input(3), 3);
        assert!(flow.links.iter().any(|l| l.from == "black_caps" && l.to == "vm:video_in_3"));
    }

    #[test]
    fn slot_publishes_on_its_channel() {
        let flow = slot_flow(2, &SourceSpec::Pattern { pattern: TestPattern::Ball });

        assert_eq!(flow.id, slot_id(2));
        assert!(flow.blocks.iter().any(|b| b.id == SLOT_OUT && b.block_definition_id == INTER_OUTPUT_BLOCK));
        assert!(flow.links.iter().any(|l| l.from == "caps" && l.to == "out:sink"));
        assert_eq!(slot_channel(2), format!("strom_{}_out", flow.id));
    }

    #[test]
    fn file_source_uses_player() {
        let flow = slot_flow(0, &SourceSpec::File { path: "/a b.mp4".into() });
        let player = flow.blocks.iter().find(|b| b.id == "player").unwrap();

        assert!(matches!(
            player.properties.get("playlist"),
            Some(PropertyValue::String(p)) if p == "[\"/a b.mp4\"]"
        ));
        assert!(flow.links.iter().any(|l| l.from == "player:video_out"));
    }
}
