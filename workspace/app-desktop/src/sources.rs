//! Source definitions and the flow generator.
//!
//! The Console lets the user pick sources (test patterns, capture devices,
//! files, URLs) and turns that list into one strom `Flow`:
//!
//! ```text
//!   source_0 ─► [caps] ─► thumb_0 ─► vm:video_in_0 ┐
//!   source_1 ─► [caps] ─► thumb_1 ─► vm:video_in_1 ├─► vision mixer
//!   ...                                            ┘   (PGM + multiview)
//! ```
//!
//! Each input goes through a `builtin.thumbnail` tap, which is what the
//! Console polls for its monitors (see `state::find_thumbnail_block`).

use strom_types::block::{BlockInstance, Position};
use strom_types::element::{Element, Link};
use strom_types::{Flow, PropertyValue};

use crate::gpu_profile::MixerBackend;

pub const MIN_SOURCES: usize = 2;
pub const MAX_SOURCES: usize = strom_types::vision_mixer::MAX_NUM_INPUTS;

const TEST_WIDTH: u32 = 1280;
const TEST_HEIGHT: u32 = 720;
const TEST_FPS: u32 = 30;

const MEDIA_PLAYER_BLOCK: &str = "builtin.media_player";
const SRT_INPUT_BLOCK: &str = "builtin.mpegtssrt_input";
const LOCAL_INPUT_BLOCK: &str = "builtin.local_input";
const THUMBNAIL_BLOCK: &str = "builtin.thumbnail";
const VISION_MIXER_BLOCK: &str = "builtin.vision_mixer";
const VIDEO_ENCODER_BLOCK: &str = "builtin.videoenc";
const MIXER_ID: &str = "vm";

const COLUMN_SPACING: f32 = 240.0;
const ROW_SPACING: f32 = 120.0;

/// GStreamer `videotestsrc` patterns worth offering in a switcher test.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TestPattern {
    Smpte100,
    Smpte75,
    ColorChart,
    Black,
    White,
    Red,
    Green,
    Blue,
    Ramp,
    Checkers,
    ZonePlate,
    Circular,
    Pinwheel,
    Ball,
    Snow,
    Blink,
}

impl TestPattern {
    pub const ALL: [Self; 16] = [
        Self::Smpte100,
        Self::Smpte75,
        Self::ColorChart,
        Self::Black,
        Self::White,
        Self::Red,
        Self::Green,
        Self::Blue,
        Self::Ramp,
        Self::Checkers,
        Self::ZonePlate,
        Self::Circular,
        Self::Pinwheel,
        Self::Ball,
        Self::Snow,
        Self::Blink,
    ];

    /// `videotestsrc` `pattern` enum nick.
    fn nick(self) -> &'static str {
        match self {
            Self::Smpte100 => "smpte100",
            Self::Smpte75 => "smpte75",
            Self::ColorChart => "colors",
            Self::Black => "black",
            Self::White => "white",
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Ramp => "gradient",
            Self::Checkers => "checkers-8",
            Self::ZonePlate => "zone-plate",
            Self::Circular => "circular",
            Self::Pinwheel => "pinwheel",
            Self::Ball => "ball",
            Self::Snow => "snow",
            Self::Blink => "blink",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Smpte100 => "Color bars (SMPTE 100%)",
            Self::Smpte75 => "Color bars (SMPTE 75%)",
            Self::ColorChart => "Color chart",
            Self::Black => "Black",
            Self::White => "White",
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Ramp => "Ramp (gradient)",
            Self::Checkers => "Checkers",
            Self::ZonePlate => "Zone plate",
            Self::Circular => "Circular",
            Self::Pinwheel => "Pinwheel",
            Self::Ball => "Moving ball",
            Self::Snow => "Noise",
            Self::Blink => "Blink",
        }
    }
}

/// One switchable source as the user configured it.
#[derive(Clone, PartialEq, Debug)]
pub enum SourceSpec {
    Pattern(TestPattern),
    /// Capture device, `id` as returned by `/api/discovery/devices`.
    Device { id: String, name: String },
    /// Local file path, looped.
    File(String),
    /// http(s)/rtsp/... URL, handled by strom's media player.
    Url(String),
    /// MPEG-TS over SRT, decoded on the engine's GPU (e.g.
    /// `srt://:9000?mode=listener`).
    Srt(String),
}

impl SourceSpec {
    pub fn label(&self) -> String {
        match self {
            Self::Pattern(p) => p.label().to_string(),
            Self::Device { name, .. } => format!("Device: {name}"),
            Self::File(path) => format!("File: {path}"),
            Self::Url(url) => format!("URL: {url}"),
            Self::Srt(uri) => format!("SRT: {uri}"),
        }
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FlowBuildError {
    #[error("need at least {MIN_SOURCES} sources")]
    TooFew,
    #[error("at most {MAX_SOURCES} sources are supported")]
    TooMany,
}

fn pos(col: usize, row: usize) -> Position {
    Position {
        x: col as f32 * COLUMN_SPACING,
        y: row as f32 * ROW_SPACING,
    }
}

fn block(
    id: String,
    definition: &str,
    position: Position,
    properties: Vec<(&str, PropertyValue)>,
) -> BlockInstance {
    BlockInstance {
        id,
        block_definition_id: definition.to_string(),
        name: None,
        properties: properties
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        position,
        runtime_data: None,
        computed_external_pads: None,
    }
}

fn link(from: String, to: String) -> Link {
    Link { from, to }
}

/// Add the source-specific part of input `i` to `flow` and return the
/// output pad reference (`id` or `id:pad`) that feeds the thumbnail tap.
fn add_source(flow: &mut Flow, i: usize, spec: &SourceSpec) -> String {
    match spec {
        SourceSpec::Pattern(pattern) => {
            let src_id = format!("src{i}");
            let caps_id = format!("caps{i}");
            flow.elements.push(Element {
                id: src_id.clone(),
                element_type: "videotestsrc".to_string(),
                properties: [
                    (
                        "pattern".to_string(),
                        PropertyValue::String(pattern.nick().to_string()),
                    ),
                    ("is-live".to_string(), PropertyValue::Bool(true)),
                ]
                .into(),
                pad_properties: Default::default(),
                position: (0.0, i as f32 * ROW_SPACING),
            });
            // Pin resolution/rate so every pattern input looks alike to the mixer.
            flow.elements.push(Element {
                id: caps_id.clone(),
                element_type: "capsfilter".to_string(),
                properties: [(
                    "caps".to_string(),
                    PropertyValue::String(format!(
                        "video/x-raw,width={TEST_WIDTH},height={TEST_HEIGHT},framerate={TEST_FPS}/1"
                    )),
                )]
                .into(),
                pad_properties: Default::default(),
                position: (COLUMN_SPACING, i as f32 * ROW_SPACING),
            });
            flow.links.push(link(src_id, caps_id.clone()));
            caps_id
        }
        SourceSpec::Device { id, .. } => {
            let dev_id = format!("dev{i}");
            flow.blocks.push(block(
                dev_id.clone(),
                LOCAL_INPUT_BLOCK,
                pos(0, i),
                vec![
                    ("stream_mode", PropertyValue::String("video".to_string())),
                    ("video_device", PropertyValue::String(id.clone())),
                    // Left empty the pipewire source fails to fixate a mode.
                    (
                        "video_resolution",
                        PropertyValue::String(format!("{TEST_WIDTH}x{TEST_HEIGHT}")),
                    ),
                    (
                        "video_framerate",
                        PropertyValue::String(format!("{TEST_FPS}/1")),
                    ),
                ],
            ));
            format!("{dev_id}:video_out")
        }
        SourceSpec::File(location) | SourceSpec::Url(location) => {
            let player_id = format!("media{i}");
            let playlist = serde_json::to_string(&[location]).unwrap_or_default();
            flow.blocks.push(block(
                player_id.clone(),
                MEDIA_PLAYER_BLOCK,
                pos(0, i),
                vec![
                    ("playlist", PropertyValue::String(playlist)),
                    // Default is passthrough of the encoded stream.
                    ("decode", PropertyValue::Bool(true)),
                    ("loop_playlist", PropertyValue::Bool(true)),
                ],
            ));
            format!("{player_id}:video_out")
        }
        SourceSpec::Srt(uri) => {
            let srt_id = format!("srt{i}");
            flow.blocks.push(block(
                srt_id.clone(),
                SRT_INPUT_BLOCK,
                pos(0, i),
                vec![
                    ("srt_uri", PropertyValue::String(uri.clone())),
                    ("decode", PropertyValue::Bool(true)),
                    ("num_video_tracks", PropertyValue::UInt(1)),
                    ("num_audio_tracks", PropertyValue::UInt(0)),
                ],
            ));
            format!("{srt_id}:video_out")
        }
    }
}

/// Build the complete flow: sources -> thumbnail taps -> vision mixer.
pub fn build_flow(
    name: &str,
    sources: &[SourceSpec],
    backend: MixerBackend,
) -> Result<Flow, FlowBuildError> {
    if sources.len() < MIN_SOURCES {
        return Err(FlowBuildError::TooFew);
    }
    if sources.len() > MAX_SOURCES {
        return Err(FlowBuildError::TooMany);
    }

    let mut flow = Flow::new(name);
    let mixer_id = MIXER_ID.to_string();

    for (i, spec) in sources.iter().enumerate() {
        let source_out = add_source(&mut flow, i, spec);
        let thumb_id = format!("thumb{i}");

        flow.blocks
            .push(block(thumb_id.clone(), THUMBNAIL_BLOCK, pos(2, i), vec![]));
        flow.links
            .push(link(source_out, format!("{thumb_id}:video_in")));
        flow.links.push(link(
            format!("{thumb_id}:video_out"),
            format!("{mixer_id}:video_in_{i}"),
        ));
    }

    flow.blocks.push(block(
        mixer_id,
        VISION_MIXER_BLOCK,
        pos(3, 0),
        vec![
            ("num_inputs", PropertyValue::Int(sources.len() as i64)),
            (
                "compositor_preference",
                PropertyValue::String(backend.property().to_string()),
            ),
        ],
    ));

    Ok(flow)
}

/// One hardware-encoded mixer output: `vm:<mixer_pad> -> <encoder> -> <sink>`.
pub struct EncodedOutput {
    pub mixer_pad: &'static str,
    /// Thumbnail tap on the mixer output: proves it carries a picture.
    pub tap: &'static str,
    /// Encoder block id; a probe on it measures frames entering the encoder.
    pub encoder: &'static str,
    /// Terminating `fakesink`; a probe on it measures encoded frames.
    pub sink: &'static str,
}

/// PGM and multiview, each through its own hardware H.264 encoder: the
/// output load the M1b budget is measured against.
pub const ENCODED_OUTPUTS: [EncodedOutput; 2] = [
    EncodedOutput {
        mixer_pad: "pgm_out",
        tap: "tap_pgm",
        encoder: "enc_pgm",
        sink: "sink_pgm",
    },
    EncodedOutput {
        mixer_pad: "multiview_out",
        tap: "tap_mv",
        encoder: "enc_mv",
        sink: "sink_mv",
    },
];

/// Terminate the mixer outputs in hardware encoders (see [`ENCODED_OUTPUTS`]),
/// each behind a thumbnail tap.
/// `sync=true` on the sinks, so frames are consumed at real time like a
/// network or file output would.
pub fn add_encoders(flow: &mut Flow) {
    for (row, out) in ENCODED_OUTPUTS.iter().enumerate() {
        flow.blocks.push(block(
            out.encoder.to_string(),
            VIDEO_ENCODER_BLOCK,
            pos(4, row),
            vec![
                ("codec", PropertyValue::String("h264".into())),
                ("encoder_preference", PropertyValue::String("hardware".into())),
            ],
        ));
        flow.elements.push(Element {
            id: out.sink.to_string(),
            element_type: "fakesink".to_string(),
            properties: [("sync".to_string(), PropertyValue::Bool(true))].into(),
            pad_properties: Default::default(),
            position: (5.0 * COLUMN_SPACING, row as f32 * ROW_SPACING),
        });
        flow.blocks
            .push(block(out.tap.to_string(), THUMBNAIL_BLOCK, pos(4, row + ENCODED_OUTPUTS.len()), vec![]));
        flow.links.push(link(
            format!("{MIXER_ID}:{}", out.mixer_pad),
            format!("{}:video_in", out.tap),
        ));
        flow.links.push(link(
            format!("{}:video_out", out.tap),
            format!("{}:video_in", out.encoder),
        ));
        flow.links
            .push(link(format!("{}:encoded_out", out.encoder), out.sink.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patterns(n: usize) -> Vec<SourceSpec> {
        (0..n)
            .map(|i| SourceSpec::Pattern(TestPattern::ALL[i % TestPattern::ALL.len()]))
            .collect()
    }

    #[test]
    fn rejects_out_of_range_counts() {
        assert_eq!(build_flow("f", &patterns(1), MixerBackend::Cuda).unwrap_err(), FlowBuildError::TooFew);
        assert_eq!(build_flow("f", &patterns(MAX_SOURCES + 1), MixerBackend::Cuda).unwrap_err(), FlowBuildError::TooMany);
    }

    #[test]
    fn encoders_terminate_both_mixer_outputs() {
        let mut flow = build_flow("f", &patterns(2), MixerBackend::Cuda).unwrap();
        add_encoders(&mut flow);

        for out in &ENCODED_OUTPUTS {
            let from = format!("vm:{}", out.mixer_pad);
            let tap_in = format!("{}:video_in", out.tap);
            let tap_out = format!("{}:video_out", out.tap);
            let to = format!("{}:video_in", out.encoder);
            assert!(flow.links.iter().any(|l| l.from == from && l.to == tap_in));
            assert!(flow.links.iter().any(|l| l.from == tap_out && l.to == to));
            assert!(flow.links.iter().any(|l| l.to == out.sink));
        }
    }

    #[test]
    fn mixer_gets_the_chosen_backend() {
        let flow = build_flow("f", &patterns(2), MixerBackend::Gl).unwrap();
        let mixer = flow.blocks.iter().find(|b| b.block_definition_id == VISION_MIXER_BLOCK).unwrap();

        assert!(matches!(
            mixer.properties.get("compositor_preference"),
            Some(PropertyValue::String(s)) if s == "gpu"
        ));
    }

    #[test]
    fn every_input_gets_a_thumbnail_tap_into_the_mixer() {
        let flow = build_flow("f", &patterns(4), MixerBackend::Cuda).unwrap();

        for i in 0..4 {
            let target = format!("vm:video_in_{i}");
            let tap = flow.links.iter().find(|l| l.to == target).unwrap();
            assert_eq!(tap.from, format!("thumb{i}:video_out"));
        }
    }

    #[test]
    fn mixes_source_kinds() {
        let sources = vec![
            SourceSpec::Pattern(TestPattern::Black),
            SourceSpec::File("/tmp/a.mp4".into()),
            SourceSpec::Url("http://x/y.mp4".into()),
            SourceSpec::Device {
                id: "dev-1".into(),
                name: "Cam".into(),
            },
            SourceSpec::Srt("srt://:9000?mode=listener".into()),
        ];
        let flow = build_flow("f", &sources, MixerBackend::Cuda).unwrap();

        assert_eq!(flow.elements.len(), 2);
        assert!(flow.links.iter().any(|l| l.to == "thumb1:video_in" && l.from == "media1:video_out"));
        assert!(flow.links.iter().any(|l| l.to == "thumb3:video_in" && l.from == "dev3:video_out"));
        assert!(flow.links.iter().any(|l| l.to == "thumb4:video_in" && l.from == "srt4:video_out"));
    }

    #[test]
    fn file_playlist_is_json_array() {
        let flow = build_flow("f", &[SourceSpec::File("/a b.mp4".into()), SourceSpec::File("c".into())], MixerBackend::Cuda).unwrap();
        let player = flow.blocks.iter().find(|b| b.id == "media0").unwrap();

        assert!(matches!(
            player.properties.get("playlist"),
            Some(PropertyValue::String(s)) if s == "[\"/a b.mp4\"]"
        ));
    }
}
