//! Audio follows video (vMix "AFV"): an AFV channel is live only while its
//! input is on PGM; channels without AFV are left alone.
//!
//! ```text
//!   AFV on ch 1, 2, 3     PGM = input 1 (ch 2)
//!   -> ch 1 mute, ch 2 unmute, ch 3 mute
//! ```
//!
//! The Console applies it whenever PGM changes, whoever took (button,
//! hotkey, API, another client), because it follows the WS state.

use crate::sources::audio_channel;

/// Ramp for AFV switches that are not a timed transition (cuts, takes
/// from elsewhere): short, just enough to avoid a click.
pub const ANTI_CLICK_MS: u32 = 20;
use std::collections::BTreeSet;

/// Channels (1-based) with AFV switched on.
#[derive(Clone, Default, Debug)]
pub struct Afv {
    channels: BTreeSet<usize>,
}

impl Afv {
    pub fn is_on(&self, channel: usize) -> bool {
        self.channels.contains(&channel)
    }

    pub fn set(&mut self, channel: usize, on: bool) {
        if on {
            self.channels.insert(channel);
        } else {
            self.channels.remove(&channel);
        }
    }

    /// Mute state each AFV channel must have with `pgm` on program.
    pub fn mutes(&self, pgm: Option<usize>) -> Vec<(usize, bool)> {
        let live = pgm.map(audio_channel);
        self.channels
            .iter()
            .map(|&ch| (ch, Some(ch) != live))
            .collect()
    }
}

/// Mixer property holding channel `ch`'s mute.
pub fn mute_property(ch: usize) -> String {
    format!("ch{ch}_mute")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_program_channel_is_live() {
        let mut afv = Afv::default();
        for ch in 1..=3 {
            afv.set(ch, true);
        }

        assert_eq!(afv.mutes(Some(1)), vec![(1, true), (2, false), (3, true)]);
    }

    #[test]
    fn channels_without_afv_are_untouched() {
        let mut afv = Afv::default();
        afv.set(1, true);
        afv.set(3, true);
        afv.set(3, false);

        assert_eq!(afv.mutes(Some(1)), vec![(1, true)]);
    }

    #[test]
    fn no_program_mutes_every_afv_channel() {
        let mut afv = Afv::default();
        afv.set(2, true);

        assert_eq!(afv.mutes(None), vec![(2, true)]);
    }
}
