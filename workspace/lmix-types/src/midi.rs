//! MIDI controllers (AUT.4): a note or controller bound to a shortcut,
//! a controller bound to the T-bar.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// What a MIDI message is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MidiKind {
    /// Note on (value: velocity) or off (value 0).
    Note,
    /// Control change (value 0–127).
    Cc,
}

/// One message from a controller, as lmix-core passes it on (to learn a
/// binding).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MidiMessage {
    pub kind: MidiKind,
    /// 1–16.
    pub channel: u8,
    /// Note or controller number, 0–127.
    pub number: u8,
    /// Velocity or controller value, 0–127.
    pub value: u8,
}

/// A note or controller; without a channel it matches on every channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct MidiBinding {
    pub kind: MidiKind,
    #[serde(default)]
    pub channel: Option<u8>,
    pub number: u8,
}

impl MidiBinding {
    /// Channel 1–16 if given, number 0–127.
    pub fn is_valid(&self) -> bool {
        self.number <= 127 && self.channel.is_none_or(|c| (1..=16).contains(&c))
    }

    /// Same note or controller as `message`.
    pub fn matches(&self, message: &MidiMessage) -> bool {
        self.kind == message.kind
            && self.number == message.number
            && self.channel.is_none_or(|c| c == message.channel)
    }

    /// Whether two bindings can fire on the same message.
    pub fn overlaps(&self, other: &MidiBinding) -> bool {
        self.kind == other.kind
            && self.number == other.number
            && match (self.channel, other.channel) {
                (Some(a), Some(b)) => a == b,
                _ => true,
            }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bindings_match_and_overlap() {
        let any = MidiBinding {
            kind: MidiKind::Note,
            channel: None,
            number: 36,
        };
        let ch2 = MidiBinding {
            channel: Some(2),
            ..any
        };
        let msg = |channel| MidiMessage {
            kind: MidiKind::Note,
            channel,
            number: 36,
            value: 100,
        };
        assert!(any.matches(&msg(5)));
        assert!(ch2.matches(&msg(2)) && !ch2.matches(&msg(3)));
        assert!(any.overlaps(&ch2));
        assert!(!ch2.overlaps(&MidiBinding {
            channel: Some(3),
            ..any
        }));
        assert!(!MidiBinding {
            kind: MidiKind::Cc,
            ..any
        }
        .matches(&msg(1)));
        assert!(!MidiBinding {
            channel: Some(17),
            ..any
        }
        .is_valid());
    }
}
