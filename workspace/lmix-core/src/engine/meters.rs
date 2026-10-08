//! Audio levels from the engine, per mixer slot and for the main mix.
//!
//! strom posts a `MeterData` event about ten times a second per audio
//! mixer channel (`mix:meter:<channel>`, before fader and mute) and for the
//! main mix (`mix:meter:main`, after them) on its WebSocket.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use strom_types::{FlowId, StromEvent};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

/// A meter that has not reported for this long counts as silent (slot
/// emptied, source stopped).
pub const STALE: Duration = Duration::from_secs(1);
const RECONNECT: Duration = Duration::from_secs(2);

/// Latest peak per slot and for the main mix, with when it was measured.
#[derive(Clone, Debug, Default)]
pub struct EngineLevels {
    pub slots: HashMap<usize, (f32, Instant)>,
    pub main: Option<(f32, Instant)>,
    pub monitor: Option<(f32, Instant)>,
}

/// Which meter a strom element id names.
#[derive(Debug, PartialEq, Eq)]
enum Meter {
    Slot(usize),
    Main,
    Monitor,
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
    // Channels count from 1, slots from 0.
    let channel: usize = name.parse().ok()?;
    channel.checked_sub(1).map(Meter::Slot)
}

/// Loudest channel; silence when there are none.
fn loudest(peaks: &[f64]) -> f32 {
    peaks.iter().copied().fold(f64::NEG_INFINITY, f64::max) as f32
}

/// Read strom's event stream forever and keep `levels` current.
pub async fn follow(
    ws_url: String,
    mixer: FlowId,
    mixer_block: &'static str,
    levels: watch::Sender<EngineLevels>,
) {
    loop {
        if let Ok((mut stream, _)) = tokio_tungstenite::connect_async(&ws_url).await {
            while let Some(Ok(message)) = stream.next().await {
                let Message::Text(text) = message else {
                    continue;
                };
                let Ok(StromEvent::MeterData {
                    flow_id,
                    element_id,
                    peak,
                    ..
                }) = serde_json::from_str(&text)
                else {
                    continue;
                };
                if flow_id != mixer {
                    continue;
                }
                let Some(meter) = parse_meter(&element_id, mixer_block) else {
                    continue;
                };
                let reading = (loudest(&peak), Instant::now());
                levels.send_modify(|l| match meter {
                    Meter::Slot(slot) => {
                        l.slots.insert(slot, reading);
                    }
                    Meter::Main => l.main = Some(reading),
                    Meter::Monitor => l.monitor = Some(reading),
                });
            }
        }
        tokio::time::sleep(RECONNECT).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meter_names() {
        assert_eq!(parse_meter("mix:meter:3", "mix"), Some(Meter::Slot(2)));
        assert_eq!(parse_meter("mix:meter:main", "mix"), Some(Meter::Main));
        assert_eq!(
            parse_meter("mix:meter:monitor", "mix"),
            Some(Meter::Monitor)
        );
        assert_eq!(parse_meter("mix:meter:aux1", "mix"), None);
        assert_eq!(parse_meter("mix:meter:0", "mix"), None);
        assert_eq!(parse_meter("other:meter:1", "mix"), None);
    }

    #[test]
    fn loudest_channel_wins() {
        assert_eq!(loudest(&[-30.0, -12.5]), -12.5);
        assert_eq!(loudest(&[]), f32::NEG_INFINITY);
    }
}
