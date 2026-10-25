//! Buses of the audio mixer (AUD.2, AUD.4): the master (program sound) and
//! the headphones are always there; the show adds as many buses as it
//! needs, each with a name (an in-ear mix, a stream without the presenter).
//! Every input sends to the headphones and to each bus at its own level.
//!
//! ```text
//!   input ─┬─ fader ─ master ─ program
//!          ├─ phones send ─ headphones (or the solos, AUD.4)
//!          └─ send per bus ─ bus "In-Ear Band" ─ an output, later a zone
//! ```
//!
//! A bus is not a zone (S-E.4): a zone is where the show's sound plays on
//! this machine, a bus is a mix of the inputs. A bus may later play into a
//! zone; then the zone's patch says which device it reaches.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::input::full_volume;

/// Buses besides the master and the headphones.
pub const BUSES: usize = 16;

/// Longest bus name, characters.
pub const BUS_NAME_MAX: usize = 40;

/// Stable identity of a bus; survives renaming and other buses going.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct BusId(pub Uuid);

impl BusId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for BusId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for BusId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// How an action names a bus. JSON: a number (`2`, 1-based position), an
/// id or a name (`"In-Ear Band"`). Ids win over names.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum BusRef {
    Number(u32),
    Text(String),
}

impl std::fmt::Display for BusRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Number(n) => n.fmt(f),
            Self::Text(t) => t.fmt(f),
        }
    }
}

/// A bus of the show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Bus {
    pub id: BusId,
    pub name: String,
    /// Sends taken before the channel fader and its on/off (a monitor mix
    /// presenters hear whatever is on air), else after them.
    #[serde(default)]
    pub pre_fader: bool,
    /// Master of the bus, 0–100 (100 = unity).
    #[serde(default = "full_volume")]
    pub volume: u8,
    #[serde(default)]
    pub mute: bool,
    /// Heard on the headphones (after its master); not saved in the show
    /// file.
    #[serde(default)]
    pub listen: bool,
}

/// A volume 0–100 (100 = unity gain) and mute: the master of program sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Fader {
    pub volume: u8,
    pub mute: bool,
}

impl Default for Fader {
    fn default() -> Self {
        Self {
            volume: full_volume(),
            mute: false,
        }
    }
}

/// The headphones (AUD.4): what every input sends to them, or the solos
/// while any input or bus is soloed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Phones {
    #[serde(default = "full_volume")]
    pub volume: u8,
    #[serde(default)]
    pub mute: bool,
    /// Sends before the channel faders; after them (default) the
    /// headphones hear program's balance.
    #[serde(default)]
    pub pre_fader: bool,
}

impl Default for Phones {
    fn default() -> Self {
        Self {
            volume: full_volume(),
            mute: false,
            pre_fader: false,
        }
    }
}

/// What an input sends to the headphones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PhonesSend {
    #[serde(default = "on")]
    pub on: bool,
    /// 0–100 (100 = unity).
    #[serde(default = "full_volume")]
    pub level: u8,
}

fn on() -> bool {
    true
}

impl Default for PhonesSend {
    fn default() -> Self {
        Self {
            on: true,
            level: full_volume(),
        }
    }
}

/// The level of a bus after its master, dBFS; `None` before the first
/// measurement.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BusPeak {
    pub bus: BusId,
    pub peak_db: Option<f32>,
}

/// The bus `reference` names among `buses`: a 1-based position, an id, or a
/// name (ids win, then names, ignoring case).
pub fn find_bus<'a>(buses: &'a [Bus], reference: &BusRef) -> Option<&'a Bus> {
    match reference {
        BusRef::Number(n) => usize::try_from(*n)
            .ok()
            .and_then(|n| n.checked_sub(1))
            .and_then(|i| buses.get(i)),
        BusRef::Text(text) => {
            let text = text.trim();
            buses.iter().find(|b| b.id.to_string() == text).or_else(|| {
                let text = text.to_lowercase();
                buses.iter().find(|b| b.name.to_lowercase() == text)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bus(name: &str) -> Bus {
        Bus {
            id: BusId::new(),
            name: name.into(),
            pre_fader: false,
            volume: 100,
            mute: false,
            listen: false,
        }
    }

    #[test]
    fn a_bus_is_found_by_number_id_or_name() {
        let buses = [bus("In-Ear Band"), bus("Stream")];
        let id = buses[1].id.to_string();
        for (reference, name) in [
            (BusRef::Number(1), Some("In-Ear Band")),
            (BusRef::Number(2), Some("Stream")),
            (BusRef::Number(0), None),
            (BusRef::Number(3), None),
            (BusRef::Text(id), Some("Stream")),
            (BusRef::Text("stream".into()), Some("Stream")),
            (BusRef::Text(" In-Ear Band ".into()), Some("In-Ear Band")),
            (BusRef::Text("Saal".into()), None),
        ] {
            assert_eq!(
                find_bus(&buses, &reference).map(|b| b.name.as_str()),
                name,
                "{reference}"
            );
        }
    }

    #[test]
    fn a_new_input_sends_to_the_headphones() {
        let send: PhonesSend = serde_json::from_str("{}").unwrap();
        assert_eq!(
            send,
            PhonesSend {
                on: true,
                level: 100
            }
        );
        let phones: Phones = serde_json::from_str("{}").unwrap();
        assert_eq!(phones, Phones::default());
    }
}
