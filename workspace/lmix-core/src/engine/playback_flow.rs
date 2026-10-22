//! The playback flow (show mode SND, VIS): strom's Show Playback block
//! with one output per zone and per stage, each on this machine's devices
//! and screens (`ZonePatch`, `StagePatch`).
//!
//! ```text
//!   show:zone_out_0  ─► tee ─► audioconvert (zone ch → device ch) ─► caps ─► volume ─► pulsesink
//!                          └► … one branch per output of the zone
//!   show:zone_out_1  ─► fakesink                     (the zone plays nowhere here)
//!   show:stage_out_0 ─► tee ─► queue ─► videoconvert ─► glimagesink / waylandsink / kmssink
//!   show:stage_out_1 ─► fakesink                     (the stage shows nowhere here)
//! ```

use strom_types::block::{BlockInstance, Position};
use strom_types::element::{Element, Link};
use strom_types::{Flow, FlowId, PropertyValue};
use uuid::Uuid;

use lmix_types::stage::StageOutput;
use lmix_types::zone::ZoneOutput;

use crate::switcher::Playback;

/// Block id of the Show Playback block in the playback flow.
pub const PLAYBACK_BLOCK: &str = "show";

const SHOW_PLAYBACK_BLOCK: &str = "builtin.show_playback";

pub fn playback_flow_id() -> FlowId {
    Uuid::new_v5(&super::flows::FLOW_NAMESPACE, b"show-playback")
}

/// The flow for `playback`; `None` without zones and stages.
pub fn playback_flow(playback: &Playback) -> Option<Flow> {
    if playback.zones.is_empty() && playback.stages.is_empty() {
        return None;
    }
    let mut flow = Flow::new("lmix playback");
    flow.id = playback_flow_id();
    let zones: Vec<_> = playback
        .zones
        .iter()
        .map(|z| serde_json::json!({"name": z.name, "channels": z.channels}))
        .collect();
    let stages: Vec<_> = playback
        .stages
        .iter()
        .map(|s| serde_json::json!({"name": s.name, "width": s.width, "height": s.height}))
        .collect();
    let json = |items: Vec<serde_json::Value>| {
        PropertyValue::String(serde_json::Value::from(items).to_string())
    };
    flow.blocks.push(BlockInstance {
        id: PLAYBACK_BLOCK.into(),
        block_definition_id: SHOW_PLAYBACK_BLOCK.into(),
        name: None,
        properties: [
            ("zones".to_string(), json(zones)),
            ("stages".to_string(), json(stages)),
            (
                "framerate".to_string(),
                PropertyValue::String(playback.framerate.fraction().into()),
            ),
        ]
        .into(),
        position: Position { x: 0.0, y: 0.0 },
        runtime_data: None,
        computed_external_pads: None,
    });
    // Without zones the block has one of its own; it plays nowhere.
    if playback.zones.is_empty() {
        flow.elements
            .push(element("z0_nowhere", "fakesink", vec![]));
        flow.links.push(link(
            format!("{PLAYBACK_BLOCK}:zone_out_0"),
            "z0_nowhere".into(),
        ));
    }
    for (i, zone) in playback.zones.iter().enumerate() {
        let out = format!("{PLAYBACK_BLOCK}:zone_out_{i}");
        match zone.outputs.as_slice() {
            [] => {
                let sink = format!("z{i}_nowhere");
                flow.elements.push(element(&sink, "fakesink", vec![]));
                flow.links.push(link(out, sink));
            }
            [only] => {
                let first = add_output(&mut flow, i, 0, zone.channels, only);
                flow.links.push(link(out, first));
            }
            outputs => {
                let tee = format!("z{i}_tee");
                flow.elements.push(element(&tee, "tee", vec![]));
                flow.links.push(link(out, tee.clone()));
                for (o, output) in outputs.iter().enumerate() {
                    let queue = format!("z{i}o{o}_queue");
                    flow.elements.push(element(&queue, "queue", vec![]));
                    flow.links
                        .push(link(format!("{tee}:src_%u"), queue.clone()));
                    let first = add_output(&mut flow, i, o, zone.channels, output);
                    flow.links.push(link(queue, first));
                }
            }
        }
    }
    for (i, stage) in playback.stages.iter().enumerate() {
        add_stage(&mut flow, i, &stage.outputs);
    }
    Some(flow)
}

/// Stage `i` on its screens: one directly, more through a tee.
fn add_stage(flow: &mut Flow, i: usize, outputs: &[StageOutput]) {
    let out = format!("{PLAYBACK_BLOCK}:stage_out_{i}");
    if outputs.is_empty() {
        let sink = format!("s{i}_nowhere");
        flow.elements.push(element(&sink, "fakesink", vec![]));
        flow.links.push(link(out, sink));
        return;
    }
    let from = if outputs.len() > 1 {
        let tee = format!("s{i}_tee");
        flow.elements.push(element(&tee, "tee", vec![]));
        flow.links.push(link(out.clone(), tee.clone()));
        Some(tee)
    } else {
        None
    };
    for (o, output) in outputs.iter().enumerate() {
        let name = |part: &str| format!("s{i}o{o}_{part}");
        flow.elements
            .push(element(&name("conv"), "videoconvert", vec![]));
        flow.elements
            .push(super::flows::display_sink(&name("screen"), output.display));
        flow.links.push(link(name("conv"), name("screen")));
        match &from {
            Some(tee) => {
                flow.elements.push(element(&name("queue"), "queue", vec![]));
                flow.links
                    .push(link(format!("{tee}:src_%u"), name("queue")));
                flow.links.push(link(name("queue"), name("conv")));
            }
            None => flow.links.push(link(out.clone(), name("conv"))),
        }
    }
}

/// pulsesink's ring buffer: its default of 200 ms makes PipeWire run the
/// stream at ~90 ms, alone more than GO-to-heard may take (SND.12, 50 ms).
const PULSE_BUFFER_US: i64 = 20_000;
const PULSE_LATENCY_US: i64 = 5_000;

/// One output of zone `zone`; returns the element its sound goes into.
fn add_output(flow: &mut Flow, zone: usize, o: usize, channels: u8, output: &ZoneOutput) -> String {
    let name = |part: &str| format!("z{zone}o{o}_{part}");
    let device_channels = output.channels.iter().copied().max().unwrap_or(1);
    flow.elements.push(element(
        &name("map"),
        "audioconvert",
        vec![(
            "mix-matrix",
            PropertyValue::String(mix_matrix(channels, &output.channels)),
        )],
    ));
    flow.elements.push(element(
        &name("caps"),
        "capsfilter",
        vec![(
            "caps",
            PropertyValue::String(format!("audio/x-raw,channels={device_channels}")),
        )],
    ));
    flow.elements.push(element(
        &name("gain"),
        "volume",
        vec![(
            "volume",
            PropertyValue::Float(10f64.powf(f64::from(output.gain_db) / 20.0)),
        )],
    ));
    // The machine's default output (no device) stays autoaudiosink: it
    // falls back to ALSA without a sound server (SNF.1), and the whole
    // playback flow, stages included, must start there. Its default
    // buffer keeps GO-to-heard at ~110 ms; a named device gets the small
    // buffer.
    let sink = if output.device.is_empty() {
        element(&name("sink"), "autoaudiosink", vec![])
    } else {
        element(
            &name("sink"),
            "pulsesink",
            vec![
                ("device", PropertyValue::String(output.device.clone())),
                ("buffer-time", PropertyValue::Int(PULSE_BUFFER_US)),
                ("latency-time", PropertyValue::Int(PULSE_LATENCY_US)),
            ],
        )
    };
    flow.elements.push(sink);
    for (from, to) in [("map", "caps"), ("caps", "gain"), ("gain", "sink")] {
        flow.links.push(link(name(from), name(to)));
    }
    name("map")
}

/// `audioconvert`'s mix-matrix: a row per device channel up to the
/// highest one used, a column per zone channel; `device_channels` are
/// 1-based, one per zone channel.
fn mix_matrix(zone_channels: u8, device_channels: &[u8]) -> String {
    let rows = device_channels.iter().copied().max().unwrap_or(1);
    let row = |device: u8| {
        let cells: Vec<&str> = (0..zone_channels)
            .map(|z| {
                if device_channels.get(usize::from(z)) == Some(&device) {
                    "(float)1"
                } else {
                    "(float)0"
                }
            })
            .collect();
        format!("<{}>", cells.join(", "))
    };
    let rows: Vec<String> = (1..=rows).map(row).collect();
    format!("<{}>", rows.join(", "))
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
    use lmix_types::stage::StageState;
    use lmix_types::zone::ZoneState;
    use lmix_types::DisplayMode;

    fn zone(name: &str, channels: u8, outputs: Vec<ZoneOutput>) -> ZoneState {
        ZoneState {
            name: name.into(),
            channels,
            outputs,
        }
    }

    fn output(device: &str, channels: Vec<u8>) -> ZoneOutput {
        ZoneOutput {
            device: device.into(),
            channels,
            gain_db: 0.0,
        }
    }

    #[test]
    fn a_stereo_zone_on_channels_three_and_four() {
        assert_eq!(
            mix_matrix(2, &[3, 4]),
            "<<(float)0, (float)0>, <(float)0, (float)0>, \
             <(float)1, (float)0>, <(float)0, (float)1>>"
        );
        assert_eq!(mix_matrix(1, &[1]), "<<(float)1>>");
    }

    fn playback(zones: Vec<ZoneState>, stages: Vec<StageState>) -> Playback {
        Playback {
            zones,
            stages,
            framerate: lmix_types::Framerate::Fps25,
        }
    }

    #[test]
    fn stages_show_on_their_screens_or_nowhere() {
        let screen = |display| StageOutput { display };
        let stage = |name: &str, outputs| StageState {
            name: name.into(),
            width: 1920,
            height: 1080,
            outputs,
        };
        let flow = playback_flow(&playback(
            vec![],
            vec![
                stage(
                    "Back",
                    vec![screen(DisplayMode::Fullscreen), screen(DisplayMode::Window)],
                ),
                stage("Side", vec![]),
            ],
        ))
        .unwrap();
        let links: Vec<(&str, &str)> = flow
            .links
            .iter()
            .map(|l| (l.from.as_str(), l.to.as_str()))
            .collect();
        assert!(
            links.contains(&("show:zone_out_0", "z0_nowhere")),
            "the block's own zone"
        );
        assert!(links.contains(&("show:stage_out_0", "s0_tee")));
        assert!(links.contains(&("s0o1_queue", "s0o1_conv")));
        assert!(links.contains(&("show:stage_out_1", "s1_nowhere")));
        let kind = |id: &str| {
            flow.elements
                .iter()
                .find(|e| e.id == id)
                .unwrap()
                .element_type
                .clone()
        };
        assert_eq!(kind("s0o0_screen"), "waylandsink");
        assert_eq!(kind("s0o1_screen"), "glimagesink");
        let block = &flow.blocks[0];
        assert_eq!(block.block_definition_id, "builtin.show_playback");
        assert!(matches!(
            block.properties.get("framerate"),
            Some(PropertyValue::String(r)) if r == "25/1"
        ));
        assert!(playback_flow(&playback(vec![], vec![])).is_none());
    }

    #[test]
    fn each_zone_plays_on_its_outputs_or_nowhere() {
        let flow = playback_flow(&playback(
            vec![
                zone("Hall", 2, vec![output("", vec![1, 2])]),
                zone(
                    "Foyer",
                    1,
                    vec![output("usb-a", vec![1]), output("usb-b", vec![2])],
                ),
                zone("Stage", 2, vec![]),
            ],
            vec![],
        ))
        .unwrap();
        let links: Vec<(&str, &str)> = flow
            .links
            .iter()
            .map(|l| (l.from.as_str(), l.to.as_str()))
            .collect();

        assert!(links.contains(&("show:zone_out_0", "z0o0_map")));
        assert!(links.contains(&("show:zone_out_1", "z1_tee")));
        assert!(links.contains(&("z1o1_queue", "z1o1_map")));
        assert!(links.contains(&("show:zone_out_2", "z2_nowhere")));
        let sink = |id: &str| flow.elements.iter().find(|e| e.id == id).unwrap();
        // The default output finds its sink itself, ALSA without a sound
        // server (SNF.1).
        assert_eq!(sink("z0o0_sink").element_type, "autoaudiosink");
        assert!(matches!(
            sink("z1o1_sink").properties.get("device"),
            Some(PropertyValue::String(d)) if d == "usb-b"
        ));
        // A small ring buffer: GO to heard within SND.12.
        assert!(matches!(
            sink("z1o1_sink").properties.get("buffer-time"),
            Some(PropertyValue::Int(PULSE_BUFFER_US))
        ));
    }
}
