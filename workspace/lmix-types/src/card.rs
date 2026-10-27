//! Capture and playout cards (DeckLink): SDI and HDMI in and out
//! (SRC.1, OUT.4).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A video mode of a card connector, named as the card's driver names it
/// (`1080p25`). Interlaced modes run at half their field rate: `1080i50`
/// is 25 frames of two fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum CardMode {
    #[serde(rename = "720p50")]
    Hd720p50,
    #[serde(rename = "720p5994")]
    Hd720p5994,
    #[serde(rename = "720p60")]
    Hd720p60,
    #[serde(rename = "1080p2398")]
    Hd1080p2398,
    #[serde(rename = "1080p24")]
    Hd1080p24,
    #[serde(rename = "1080p25")]
    Hd1080p25,
    #[serde(rename = "1080p2997")]
    Hd1080p2997,
    #[serde(rename = "1080p30")]
    Hd1080p30,
    #[serde(rename = "1080p50")]
    Hd1080p50,
    #[serde(rename = "1080p5994")]
    Hd1080p5994,
    #[serde(rename = "1080p60")]
    Hd1080p60,
    #[serde(rename = "1080i50")]
    Hd1080i50,
    #[serde(rename = "1080i5994")]
    Hd1080i5994,
    /// 1080p25 as segmented frames (PsF): each frame goes as two fields,
    /// on the wire like `1080i50`.
    #[serde(rename = "1080psf25")]
    Hd1080psf25,
    #[serde(rename = "1080psf2997")]
    Hd1080psf2997,
    #[serde(rename = "2160p25")]
    Uhd2160p25,
    #[serde(rename = "2160p2997")]
    Uhd2160p2997,
    #[serde(rename = "2160p30")]
    Uhd2160p30,
    #[serde(rename = "2160p50")]
    Uhd2160p50,
    #[serde(rename = "2160p5994")]
    Uhd2160p5994,
    #[serde(rename = "2160p60")]
    Uhd2160p60,
}

/// How a mode scans its lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scan {
    Progressive,
    /// Two fields per frame, each from its own moment.
    Interlaced,
    /// Progressive frames sent as two fields (PsF).
    Segmented,
}

impl CardMode {
    pub const ALL: [CardMode; 21] = [
        Self::Hd720p50,
        Self::Hd720p5994,
        Self::Hd720p60,
        Self::Hd1080p2398,
        Self::Hd1080p24,
        Self::Hd1080p25,
        Self::Hd1080p2997,
        Self::Hd1080p30,
        Self::Hd1080p50,
        Self::Hd1080p5994,
        Self::Hd1080p60,
        Self::Hd1080i50,
        Self::Hd1080i5994,
        Self::Hd1080psf25,
        Self::Hd1080psf2997,
        Self::Uhd2160p25,
        Self::Uhd2160p2997,
        Self::Uhd2160p30,
        Self::Uhd2160p50,
        Self::Uhd2160p5994,
        Self::Uhd2160p60,
    ];

    /// The driver's name of the mode, e.g. `1080i50`.
    pub fn nick(self) -> &'static str {
        match self {
            Self::Hd720p50 => "720p50",
            Self::Hd720p5994 => "720p5994",
            Self::Hd720p60 => "720p60",
            Self::Hd1080p2398 => "1080p2398",
            Self::Hd1080p24 => "1080p24",
            Self::Hd1080p25 => "1080p25",
            Self::Hd1080p2997 => "1080p2997",
            Self::Hd1080p30 => "1080p30",
            Self::Hd1080p50 => "1080p50",
            Self::Hd1080p5994 => "1080p5994",
            Self::Hd1080p60 => "1080p60",
            Self::Hd1080i50 => "1080i50",
            Self::Hd1080i5994 => "1080i5994",
            Self::Hd1080psf25 => "1080psf25",
            Self::Hd1080psf2997 => "1080psf2997",
            Self::Uhd2160p25 => "2160p25",
            Self::Uhd2160p2997 => "2160p2997",
            Self::Uhd2160p30 => "2160p30",
            Self::Uhd2160p50 => "2160p50",
            Self::Uhd2160p5994 => "2160p5994",
            Self::Uhd2160p60 => "2160p60",
        }
    }

    /// The mode the card runs in. The driver has no PsF modes: a PsF
    /// signal is the interlaced mode of its field rate, the fields come
    /// from one frame.
    pub fn driver_mode(self) -> &'static str {
        match self {
            Self::Hd1080psf25 => Self::Hd1080i50.nick(),
            Self::Hd1080psf2997 => Self::Hd1080i5994.nick(),
            _ => self.nick(),
        }
    }

    /// The mode by its name.
    pub fn from_nick(nick: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.nick() == nick)
    }

    /// Width and height in pixels.
    pub fn size(self) -> (u32, u32) {
        match self {
            Self::Hd720p50 | Self::Hd720p5994 | Self::Hd720p60 => (1280, 720),
            Self::Uhd2160p25
            | Self::Uhd2160p2997
            | Self::Uhd2160p30
            | Self::Uhd2160p50
            | Self::Uhd2160p5994
            | Self::Uhd2160p60 => (3840, 2160),
            _ => (1920, 1080),
        }
    }

    pub fn scan(self) -> Scan {
        match self {
            Self::Hd1080i50 | Self::Hd1080i5994 => Scan::Interlaced,
            Self::Hd1080psf25 | Self::Hd1080psf2997 => Scan::Segmented,
            _ => Scan::Progressive,
        }
    }

    /// Pictures per second as GStreamer writes it: frames, for an
    /// interlaced mode fields (`1080i50` takes 50 pictures, two make a
    /// frame). A PsF mode takes frames: `1080psf25` 25.
    pub fn picture_rate(self) -> &'static str {
        match self {
            Self::Hd1080p2398 => "24000/1001",
            Self::Hd1080p24 => "24/1",
            Self::Hd1080p25 | Self::Hd1080psf25 | Self::Uhd2160p25 => "25/1",
            Self::Hd1080p2997 | Self::Hd1080psf2997 | Self::Uhd2160p2997 => "30000/1001",
            Self::Hd1080p30 | Self::Uhd2160p30 => "30/1",
            Self::Hd720p50 | Self::Hd1080p50 | Self::Hd1080i50 | Self::Uhd2160p50 => "50/1",
            Self::Hd720p5994 | Self::Hd1080p5994 | Self::Hd1080i5994 | Self::Uhd2160p5994 => {
                "60000/1001"
            }
            Self::Hd720p60 | Self::Hd1080p60 | Self::Uhd2160p60 => "60/1",
        }
    }
}

/// Embedded sound channels of a card connector: SDI carries up to 16.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub enum CardChannels {
    #[default]
    #[serde(rename = "2")]
    Two,
    #[serde(rename = "8")]
    Eight,
    #[serde(rename = "16")]
    Sixteen,
}

impl CardChannels {
    pub const ALL: [CardChannels; 3] = [Self::Two, Self::Eight, Self::Sixteen];

    pub fn is_default(&self) -> bool {
        *self == Self::default()
    }

    pub fn count(self) -> u32 {
        match self {
            Self::Two => 2,
            Self::Eight => 8,
            Self::Sixteen => 16,
        }
    }

    /// Channel pairs: 1, 4 or 8.
    pub fn pairs(self) -> u32 {
        self.count() / 2
    }

    /// The fewest channels that hold pair `pair` (1-based); `None` past 16.
    pub fn holding(pair: u32) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|c| (1..=c.pairs()).contains(&pair))
    }
}

/// Longest sound delay of a card output, ms: lip sync for a picture
/// that the next device shows late.
pub const MAX_CARD_SOUND_DELAY_MS: u32 = 500;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_go_by_the_drivers_names() {
        for mode in CardMode::ALL {
            assert_eq!(CardMode::from_nick(mode.nick()), Some(mode));
            let json = serde_json::to_string(&mode).unwrap();
            assert_eq!(json, format!("\"{}\"", mode.nick()));
        }
        assert_eq!(CardMode::from_nick("auto"), None);
    }

    #[test]
    fn an_interlaced_mode_takes_fields() {
        assert_eq!(CardMode::Hd1080i50.scan(), Scan::Interlaced);
        assert_eq!(CardMode::Hd1080i50.picture_rate(), "50/1");
        assert_eq!(CardMode::Hd1080p25.scan(), Scan::Progressive);
        assert_eq!(CardMode::Hd720p5994.size(), (1280, 720));
    }

    #[test]
    fn psf_runs_the_card_interlaced_at_the_frame_rate() {
        assert_eq!(CardMode::Hd1080psf25.scan(), Scan::Segmented);
        assert_eq!(CardMode::Hd1080psf25.driver_mode(), "1080i50");
        assert_eq!(CardMode::Hd1080psf25.picture_rate(), "25/1");
        assert_eq!(CardMode::Hd1080psf2997.driver_mode(), "1080i5994");
        assert_eq!(CardMode::Hd1080p25.driver_mode(), "1080p25");
    }

    #[test]
    fn a_pair_takes_the_fewest_channels_that_hold_it() {
        assert_eq!(CardChannels::holding(1), Some(CardChannels::Two));
        assert_eq!(CardChannels::holding(2), Some(CardChannels::Eight));
        assert_eq!(CardChannels::holding(4), Some(CardChannels::Eight));
        assert_eq!(CardChannels::holding(5), Some(CardChannels::Sixteen));
        assert_eq!(CardChannels::holding(8), Some(CardChannels::Sixteen));
        assert_eq!(CardChannels::holding(0), None);
        assert_eq!(CardChannels::holding(9), None);
        assert_eq!(
            serde_json::to_string(&CardChannels::Sixteen).unwrap(),
            "\"16\""
        );
    }
}
