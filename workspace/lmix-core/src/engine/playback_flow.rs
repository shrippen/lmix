//! The playback flow (show mode SND, VIS): strom's Show Playback block
//! with one output per zone and per stage, each on this machine's devices
//! and screens (`ZonePatch`, `StagePatch`).
//!
//! ```text
//!   show:zone_out_0  ─► meter ─► tee ─► audioconvert (zone ch → device ch) ─► caps ─► volume ─► pulsesink
//!                                   └► … one branch per output of the zone
//!   show:zone_out_1  ─► meter ─► fakesink                 (the zone plays nowhere here)
//!   show:stage_out_0 ─► tee ─► queue ─► videoconvert ─► glimagesink / waylandsink / kmssink
//!   show:stage_out_1 ─► fakesink                     (the stage shows nowhere here)
//! ```

use std::collections::HashMap;

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
const METER_BLOCK: &str = "builtin.meter";

/// Block id of zone `i`'s meter; strom reports its level under this id.
pub fn zone_meter(i: usize) -> String {
    format!("zone_meter_{i}")
}

/// The zone whose meter `element_id` is.
pub fn metered_zone(element_id: &str) -> Option<usize> {
    element_id.strip_prefix("zone_meter_")?.parse().ok()
}

pub fn playback_flow_id() -> FlowId {
    Uuid::new_v5(&super::flows::FLOW_NAMESPACE, b"show-playback")
}

/// Speaker positions of each output device's channels, in the device's
/// order, by device name (PipeWire's `node.name`, what `pulsesink` takes).
pub type DeviceChannels = HashMap<String, Vec<String>>;

/// The flow for `playback`; `None` without zones and stages.
pub fn playback_flow(playback: &Playback, devices: &DeviceChannels) -> Option<Flow> {
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
        // Metered after the zone's level, as it is heard (SUI.8).
        let meter = zone_meter(i);
        flow.blocks.push(BlockInstance {
            id: meter.clone(),
            block_definition_id: METER_BLOCK.into(),
            name: None,
            properties: HashMap::new(),
            position: Position { x: 0.0, y: 0.0 },
            runtime_data: None,
            computed_external_pads: None,
        });
        flow.links.push(link(
            format!("{PLAYBACK_BLOCK}:zone_out_{i}"),
            format!("{meter}:audio_in"),
        ));
        let out = format!("{meter}:audio_out");
        match zone.outputs.as_slice() {
            [] => {
                let sink = format!("z{i}_nowhere");
                flow.elements.push(element(&sink, "fakesink", vec![]));
                flow.links.push(link(out, sink));
            }
            [only] => {
                let first = add_output(&mut flow, i, 0, zone.channels, only, devices);
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
                    let first = add_output(&mut flow, i, o, zone.channels, output, devices);
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
fn add_output(
    flow: &mut Flow,
    zone: usize,
    o: usize,
    channels: u8,
    output: &ZoneOutput,
    devices: &DeviceChannels,
) -> String {
    let name = |part: &str| format!("z{zone}o{o}_{part}");
    let layout = devices
        .get(&output.device)
        .and_then(|positions| DeviceLayout::of(positions, &output.channels))
        .unwrap_or_else(|| {
            if output.channels.iter().any(|c| *c > 2) {
                tracing::warn!(
                    device = %output.device,
                    positions = ?devices.get(&output.device),
                    "zone {zone} plays unpositioned on device channels {:?}: the sound \
                     server may put them elsewhere",
                    output.channels
                );
            }
            DeviceLayout::unpositioned(&output.channels)
        });
    flow.elements.push(element(
        &name("map"),
        "audioconvert",
        vec![(
            "mix-matrix",
            PropertyValue::String(layout.mix_matrix(channels, &output.channels)),
        )],
    ));
    flow.elements.push(element(
        &name("caps"),
        "capsfilter",
        vec![("caps", PropertyValue::String(layout.caps()))],
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

/// GStreamer's channel position bit for a PipeWire position name, for the
/// positions `pulsesink` passes on to the sound server.
fn position_bit(name: &str) -> Option<u32> {
    Some(match name {
        "FL" => 0,
        "FR" => 1,
        "FC" => 2,
        "LFE" => 3,
        "RL" => 4,
        "RR" => 5,
        "FLC" => 6,
        "FRC" => 7,
        "RC" => 8,
        "SL" => 10,
        "SR" => 11,
        "TFL" => 12,
        "TFR" => 13,
        "TFC" => 14,
        "TC" => 15,
        "TRL" => 16,
        "TRR" => 17,
        "TRC" => 20,
        _ => return None,
    })
}

/// The stream an output sends its device. The sound server routes by
/// position, not by index: device channel 3 of a 7.1 card is its centre,
/// so the stream carries the device's positions (a channel mask) and puts
/// a zone channel on the stream channel at that position. GStreamer orders
/// a masked stream by position, which need not be the device's order.
/// Without positions (unknown device, `AUX` channels of a pro-audio
/// profile) the stream goes unpositioned, up to the highest channel used.
struct DeviceLayout {
    channels: usize,
    mask: Option<u64>,
    /// Stream channel (0-based) per device channel (0-based).
    stream_channel: Vec<usize>,
}

impl DeviceLayout {
    fn of(positions: &[String], used: &[u8]) -> Option<Self> {
        let bits: Vec<u32> = positions
            .iter()
            .map(|p| position_bit(p))
            .collect::<Option<_>>()?;
        let mask = bits.iter().fold(0u64, |mask, bit| mask | 1 << bit);
        let fits = used
            .iter()
            .all(|&c| (1..=positions.len()).contains(&usize::from(c)));
        if positions.len() < 2 || mask.count_ones() as usize != positions.len() || !fits {
            return None;
        }
        let stream_channel = bits
            .iter()
            .map(|bit| (mask & ((1u64 << bit) - 1)).count_ones() as usize)
            .collect();
        Some(Self {
            channels: positions.len(),
            mask: Some(mask),
            stream_channel,
        })
    }

    fn unpositioned(used: &[u8]) -> Self {
        let channels = usize::from(used.iter().copied().max().unwrap_or(1));
        Self {
            channels,
            mask: None,
            stream_channel: (0..channels).collect(),
        }
    }

    fn caps(&self) -> String {
        match self.mask {
            Some(mask) => format!(
                "audio/x-raw,channels={},channel-mask=(bitmask)0x{mask:016x}",
                self.channels
            ),
            None => format!("audio/x-raw,channels={}", self.channels),
        }
    }

    /// `audioconvert`'s mix-matrix: a row per stream channel, a column per
    /// zone channel; `device_channels` are 1-based, one per zone channel.
    fn mix_matrix(&self, zone_channels: u8, device_channels: &[u8]) -> String {
        let row = |stream: usize| {
            let cells: Vec<&str> = (0..zone_channels)
                .map(|z| {
                    let device = device_channels.get(usize::from(z)).copied();
                    let target = device
                        .and_then(|d| usize::from(d).checked_sub(1))
                        .and_then(|d| self.stream_channel.get(d));
                    if target == Some(&stream) {
                        "(float)1"
                    } else {
                        "(float)0"
                    }
                })
                .collect();
            format!("<{}>", cells.join(", "))
        };
        let rows: Vec<String> = (0..self.channels).map(row).collect();
        format!("<{}>", rows.join(", "))
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
    use lmix_types::stage::StageState;
    use lmix_types::zone::ZoneState;
    use lmix_types::DisplayMode;

    fn zone(name: &str, channels: u8, outputs: Vec<ZoneOutput>) -> ZoneState {
        ZoneState {
            name: name.into(),
            channels,
            outputs,
            gain_db: 0.0,
            mute: false,
        }
    }

    fn output(device: &str, channels: Vec<u8>) -> ZoneOutput {
        ZoneOutput {
            device: device.into(),
            channels,
            gain_db: 0.0,
        }
    }

    fn positions(names: &str) -> Vec<String> {
        names.split(' ').map(str::to_string).collect()
    }

    /// The stream channels a zone's channels land on, row by row.
    fn rows(matrix: &str) -> Vec<Vec<bool>> {
        matrix
            .trim_start_matches("<<")
            .trim_end_matches(">>")
            .split(">, <")
            .map(|row| row.split(", ").map(|c| c == "(float)1").collect())
            .collect()
    }

    #[test]
    fn without_positions_device_channel_n_is_stream_channel_n() {
        let layout = DeviceLayout::unpositioned(&[3, 4]);
        assert_eq!(layout.caps(), "audio/x-raw,channels=4");
        assert_eq!(
            layout.mix_matrix(2, &[3, 4]),
            "<<(float)0, (float)0>, <(float)0, (float)0>, \
             <(float)1, (float)0>, <(float)0, (float)1>>"
        );
        assert_eq!(
            DeviceLayout::unpositioned(&[1]).mix_matrix(1, &[1]),
            "<<(float)1>>"
        );
    }

    #[test]
    fn a_stereo_zone_on_channels_three_and_four_of_a_7_1_device() {
        // Unpositioned, this landed on rear left/right (5/6): the sound
        // server routes by position, not by index (19.10.).
        let layout = DeviceLayout::of(&positions("FL FR FC LFE RL RR SL SR"), &[3, 4]).unwrap();
        assert_eq!(
            layout.caps(),
            "audio/x-raw,channels=8,channel-mask=(bitmask)0x0000000000000c3f"
        );
        let rows = rows(&layout.mix_matrix(2, &[3, 4]));
        assert_eq!(rows.len(), 8);
        assert_eq!(rows[2], [true, false], "FC");
        assert_eq!(rows[3], [false, true], "LFE");
        assert_eq!(rows.iter().flatten().filter(|c| **c).count(), 2);
    }

    #[test]
    fn device_order_is_not_stream_order() {
        // ALSA's 5.1: channel 3 is rear left, 5 the centre; GStreamer
        // orders the stream FL FR FC LFE RL RR.
        let layout = DeviceLayout::of(&positions("FL FR RL RR FC LFE"), &[3, 5]).unwrap();
        let rows = rows(&layout.mix_matrix(2, &[3, 5]));
        assert_eq!(rows[4], [true, false], "RL");
        assert_eq!(rows[2], [false, true], "FC");
    }

    #[test]
    fn aux_channels_and_short_devices_stay_unpositioned() {
        assert!(DeviceLayout::of(&positions("AUX0 AUX1 AUX2 AUX3"), &[3, 4]).is_none());
        assert!(DeviceLayout::of(&positions("FL FR"), &[3, 4]).is_none());
        assert!(DeviceLayout::of(&positions("MONO"), &[1]).is_none());
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
        let flow = playback_flow(
            &playback(
                vec![],
                vec![
                    stage(
                        "Back",
                        vec![screen(DisplayMode::Fullscreen), screen(DisplayMode::Window)],
                    ),
                    stage("Side", vec![]),
                ],
            ),
            &DeviceChannels::new(),
        )
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
        assert!(playback_flow(&playback(vec![], vec![]), &DeviceChannels::new()).is_none());
    }

    #[test]
    fn each_zone_plays_on_its_outputs_or_nowhere() {
        let flow = playback_flow(
            &playback(
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
            ),
            &[("usb-b".to_string(), positions("FL FR"))].into(),
        )
        .unwrap();
        let links: Vec<(&str, &str)> = flow
            .links
            .iter()
            .map(|l| (l.from.as_str(), l.to.as_str()))
            .collect();

        // Each zone is metered on its way out (SUI.8).
        assert!(links.contains(&("show:zone_out_0", "zone_meter_0:audio_in")));
        assert!(links.contains(&("zone_meter_0:audio_out", "z0o0_map")));
        assert!(links.contains(&("zone_meter_1:audio_out", "z1_tee")));
        assert!(links.contains(&("z1o1_queue", "z1o1_map")));
        assert!(links.contains(&("zone_meter_2:audio_out", "z2_nowhere")));
        assert!(flow
            .blocks
            .iter()
            .any(|b| b.id == "zone_meter_2" && b.block_definition_id == "builtin.meter"));
        assert_eq!(metered_zone("zone_meter_2"), Some(2));
        assert_eq!(metered_zone("show"), None);
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
        // A device that says its positions gets a positioned stream.
        let caps = |id: &str| match sink(id).properties.get("caps") {
            Some(PropertyValue::String(c)) => c.clone(),
            other => panic!("{id}: {other:?}"),
        };
        assert_eq!(caps("z1o0_caps"), "audio/x-raw,channels=1");
        assert_eq!(
            caps("z1o1_caps"),
            "audio/x-raw,channels=2,channel-mask=(bitmask)0x0000000000000003"
        );
    }
}
