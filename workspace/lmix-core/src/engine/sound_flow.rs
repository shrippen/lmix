//! The sound flow (show mode SND): strom's Show Sound block with one
//! output per zone, each on this machine's devices (`ZonePatch`).
//!
//! ```text
//!   snd:zone_out_0 ─► tee ─► audioconvert (zone ch → device ch) ─► caps ─► volume ─► pulsesink
//!                         └► … one branch per output of the zone
//!   snd:zone_out_1 ─► fakesink            (the zone plays nowhere here)
//! ```

use strom_types::block::{BlockInstance, Position};
use strom_types::element::{Element, Link};
use strom_types::{Flow, FlowId, PropertyValue};
use uuid::Uuid;

use lmix_types::zone::{ZoneOutput, ZoneState};

/// Block id of the Show Sound block in the sound flow.
pub const SOUND_BLOCK: &str = "snd";

const SHOW_SOUND_BLOCK: &str = "builtin.show_sound";

pub fn sound_flow_id() -> FlowId {
    Uuid::new_v5(&super::flows::FLOW_NAMESPACE, b"show-sound")
}

/// The flow for `zones`; `None` without zones.
pub fn sound_flow(zones: &[ZoneState]) -> Option<Flow> {
    if zones.is_empty() {
        return None;
    }
    let mut flow = Flow::new("lmix sound");
    flow.id = sound_flow_id();
    let described: Vec<_> = zones
        .iter()
        .map(|z| serde_json::json!({"name": z.name, "channels": z.channels}))
        .collect();
    flow.blocks.push(BlockInstance {
        id: SOUND_BLOCK.into(),
        block_definition_id: SHOW_SOUND_BLOCK.into(),
        name: None,
        properties: [(
            "zones".to_string(),
            PropertyValue::String(serde_json::Value::from(described).to_string()),
        )]
        .into(),
        position: Position { x: 0.0, y: 0.0 },
        runtime_data: None,
        computed_external_pads: None,
    });
    for (i, zone) in zones.iter().enumerate() {
        let out = format!("{SOUND_BLOCK}:zone_out_{i}");
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
    Some(flow)
}

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
    let sink = if output.device.is_empty() {
        element(&name("sink"), "autoaudiosink", vec![])
    } else {
        element(
            &name("sink"),
            "pulsesink",
            vec![("device", PropertyValue::String(output.device.clone()))],
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

    #[test]
    fn each_zone_plays_on_its_outputs_or_nowhere() {
        let flow = sound_flow(&[
            zone("Hall", 2, vec![output("", vec![1, 2])]),
            zone(
                "Foyer",
                1,
                vec![output("usb-a", vec![1]), output("usb-b", vec![2])],
            ),
            zone("Stage", 2, vec![]),
        ])
        .unwrap();
        let links: Vec<(&str, &str)> = flow
            .links
            .iter()
            .map(|l| (l.from.as_str(), l.to.as_str()))
            .collect();

        assert!(links.contains(&("snd:zone_out_0", "z0o0_map")));
        assert!(links.contains(&("snd:zone_out_1", "z1_tee")));
        assert!(links.contains(&("z1o1_queue", "z1o1_map")));
        assert!(links.contains(&("snd:zone_out_2", "z2_nowhere")));
        let sink = |id: &str| flow.elements.iter().find(|e| e.id == id).unwrap();
        assert_eq!(sink("z0o0_sink").element_type, "autoaudiosink");
        assert!(matches!(
            sink("z1o1_sink").properties.get("device"),
            Some(PropertyValue::String(d)) if d == "usb-b"
        ));
        assert!(sound_flow(&[]).is_none());
    }
}
