//! Audio levels from the engine, per mixer slot and for the main mix.
//!
//! strom posts a `MeterData` event about ten times a second per audio
//! mixer channel (`mix:meter:<channel>`, before fader and mute) and for the
//! main mix (`mix:meter:main`, after them) on its WebSocket. Pipeline errors,
//! QoS reports and system statistics come the same way.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use strom_types::{FlowId, StromEvent, SystemStats};
use tokio::sync::{broadcast, watch};
use tokio_tungstenite::tungstenite::Message;

/// A meter that has not reported for this long counts as silent (slot
/// emptied, source stopped).
pub const STALE: Duration = Duration::from_secs(1);
const RECONNECT: Duration = Duration::from_secs(2);

/// Latest peak per slot and for the main mix, with when it was measured.
#[derive(Clone, Debug, Default)]
pub struct EngineLevels {
    pub slots: HashMap<usize, (f32, Instant)>,
    /// Peak of each channel per slot, as measured with `slots` (AUD.1).
    pub slot_channels: HashMap<usize, Vec<f32>>,
    pub main: Option<(f32, Instant)>,
    pub monitor: Option<(f32, Instant)>,
    /// Aux buses, 0-based.
    pub aux: HashMap<usize, (f32, Instant)>,
    /// Buffer ages at the mixer's program output from the latest latency
    /// measurement, ms (newest last, at most `LATENCY_SAMPLES`).
    pub program_ages_ms: Vec<u64>,
}

/// Health of the engine as its event stream reports it (OPS.3).
#[derive(Clone, Debug, Default)]
pub struct EngineHealth {
    /// CPU, memory and GPUs of the engine machine, about once a second.
    pub system: Option<SystemStats>,
    /// QoS of each flow since lmix-core started.
    pub qos: HashMap<FlowId, FlowQos>,
}

/// QoS reports of one flow, summed over its elements.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FlowQos {
    /// QoS events: an element got a buffer too late and dropped or
    /// skipped work.
    pub events: u64,
    /// Reports in which an element was falling behind.
    pub behind: u64,
    /// Average proportion of the latest report (below 1: too slow).
    pub proportion: f64,
}

/// Latency samples kept per measurement.
pub const LATENCY_SAMPLES: usize = 600;

/// Which meter a strom element id names.
#[derive(Debug, PartialEq, Eq)]
enum Meter {
    Slot(usize),
    Main,
    Monitor,
    Aux(usize),
}

fn parse_meter(element_id: &str, mixer_block: &str) -> Option<Meter> {
    let name = element_id
        .strip_prefix(mixer_block)?
        .strip_prefix(":meter:")?;
    match name {
        "main" => return Some(Meter::Main),
        "monitor" => return Some(Meter::Monitor),
        _ => {}
    }
    // Aux buses count from 1 as well.
    if let Some(bus) = name.strip_prefix("aux") {
        return bus.parse::<usize>().ok()?.checked_sub(1).map(Meter::Aux);
    }
    // Channels count from 1, slots from 0.
    let channel: usize = name.parse().ok()?;
    channel.checked_sub(1).map(Meter::Slot)
}

/// Loudest channel; silence when there are none.
fn loudest(peaks: &[f64]) -> f32 {
    peaks.iter().copied().fold(f64::NEG_INFINITY, f64::max) as f32
}

/// A flow failed while running (an output lost its target, a disk filled).
#[derive(Clone, Debug)]
pub struct FlowError {
    pub flow: FlowId,
    pub message: String,
    pub kind: FlowFailure,
}

fn playback_state(state: strom_types::mediaplayer::PlayerState) -> lmix_types::PlaybackState {
    use strom_types::mediaplayer::PlayerState;
    match state {
        PlayerState::Playing => lmix_types::PlaybackState::Playing,
        PlayerState::Paused => lmix_types::PlaybackState::Paused,
        PlayerState::Stopped => lmix_types::PlaybackState::Stopped,
    }
}

/// How a flow went down.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FlowFailure {
    /// A pipeline error (source gone, disk full, …).
    Error,
    /// The flow stopped; lmix-core's own rebuilds stop flows too.
    Stopped,
}

/// Read strom's event stream forever: keep `levels` current and pass on
/// pipeline errors.
pub async fn follow(
    ws_url: String,
    mixer: FlowId,
    mixer_block: &'static str,
    levels: watch::Sender<EngineLevels>,
    errors: broadcast::Sender<FlowError>,
    health: Arc<Mutex<EngineHealth>>,
    players: broadcast::Sender<crate::clips::PlayerProgress>,
) {
    // Last state each player reported, added to its position reports.
    let mut player_states = HashMap::new();
    loop {
        if let Ok((mut stream, _)) = tokio_tungstenite::connect_async(&ws_url).await {
            while let Some(Ok(message)) = stream.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                match serde_json::from_str(&text) {
                    Ok(StromEvent::MeterData {
                        flow_id,
                        element_id,
                        peak,
                        ..
                    }) if flow_id == mixer => {
                        record_meter(&levels, &element_id, mixer_block, &peak)
                    }
                    Ok(StromEvent::BufferAgeProbe {
                        flow_id, age_ms, ..
                    }) if flow_id == mixer => {
                        levels.send_modify(|l| {
                            if l.program_ages_ms.len() >= LATENCY_SAMPLES {
                                l.program_ages_ms.remove(0);
                            }
                            l.program_ages_ms.push(age_ms);
                        });
                    }
                    Ok(StromEvent::PipelineError { flow_id, error, .. }) => {
                        // Nobody listening is fine.
                        let _ = errors.send(FlowError {
                            flow: flow_id,
                            message: error,
                            kind: FlowFailure::Error,
                        });
                    }
                    Ok(StromEvent::MediaPlayerStateChanged { flow_id, state, .. }) => {
                        player_states.insert(flow_id, playback_state(state));
                    }
                    Ok(StromEvent::MediaPlayerPosition {
                        flow_id,
                        position_ns,
                        duration_ns,
                        current_file_index,
                        total_files,
                        ..
                    }) => {
                        let _ = players.send(crate::clips::PlayerProgress {
                            flow: flow_id,
                            position_ms: position_ns / 1_000_000,
                            duration_ms: duration_ns / 1_000_000,
                            index: current_file_index,
                            items: total_files,
                            state: player_states.get(&flow_id).copied(),
                        });
                    }
                    Ok(StromEvent::SystemStats(stats)) => {
                        health.lock().expect("health lock").system = Some(stats);
                    }
                    Ok(StromEvent::QoSStats {
                        flow_id,
                        event_count,
                        avg_proportion,
                        is_falling_behind,
                        ..
                    }) => {
                        let mut health = health.lock().expect("health lock");
                        let qos = health.qos.entry(flow_id).or_default();
                        qos.events += event_count;
                        qos.behind += u64::from(is_falling_behind);
                        qos.proportion = avg_proportion;
                    }
                    Ok(StromEvent::FlowStopped { flow_id }) => {
                        let _ = errors.send(FlowError {
                            flow: flow_id,
                            message: "stopped".into(),
                            kind: FlowFailure::Stopped,
                        });
                    }
                    _ => {}
                }
            }
        }
        tokio::time::sleep(RECONNECT).await;
    }
}

fn record_meter(
    levels: &watch::Sender<EngineLevels>,
    element_id: &str,
    mixer_block: &str,
    peak: &[f64],
) {
    let Some(meter) = parse_meter(element_id, mixer_block) else {
        return;
    };
    let reading = (loudest(peak), Instant::now());
    levels.send_modify(|l| match meter {
        Meter::Slot(slot) => {
            l.slots.insert(slot, reading);
            l.slot_channels
                .insert(slot, peak.iter().map(|p| *p as f32).collect());
        }
        Meter::Main => l.main = Some(reading),
        Meter::Monitor => l.monitor = Some(reading),
        Meter::Aux(bus) => {
            l.aux.insert(bus, reading);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_keep_each_channel() {
        let (levels, read) = watch::channel(EngineLevels::default());

        record_meter(&levels, "mix:meter:2", "mix", &[-12.0, -30.0]);

        let l = read.borrow();
        assert_eq!(l.slots[&1].0, -12.0, "loudest");
        assert_eq!(l.slot_channels[&1], [-12.0, -30.0]);
    }

    #[test]
    fn meter_names() {
        assert_eq!(parse_meter("mix:meter:3", "mix"), Some(Meter::Slot(2)));
        assert_eq!(parse_meter("mix:meter:main", "mix"), Some(Meter::Main));
        assert_eq!(
            parse_meter("mix:meter:monitor", "mix"),
            Some(Meter::Monitor)
        );
        assert_eq!(parse_meter("mix:meter:aux1", "mix"), Some(Meter::Aux(0)));
        assert_eq!(parse_meter("mix:meter:aux0", "mix"), None);
        assert_eq!(parse_meter("mix:meter:0", "mix"), None);
        assert_eq!(parse_meter("other:meter:1", "mix"), None);
    }

    #[test]
    fn loudest_channel_wins() {
        assert_eq!(loudest(&[-30.0, -12.5]), -12.5);
        assert_eq!(loudest(&[]), f32::NEG_INFINITY);
    }
}
