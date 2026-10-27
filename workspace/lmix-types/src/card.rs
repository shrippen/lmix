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
    /// Two fields per frame.
    Interlaced,
}

impl CardMode {
    pub const ALL: [CardMode; 19] = [
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
            Self::Uhd2160p25 => "2160p25",
            Self::Uhd2160p2997 => "2160p2997",
            Self::Uhd2160p30 => "2160p30",
            Self::Uhd2160p50 => "2160p50",
            Self::Uhd2160p5994 => "2160p5994",
            Self::Uhd2160p60 => "2160p60",
        }
    }

    /// The mode by its driver name.
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
            _ => Scan::Progressive,
        }
    }

    /// Pictures per second as GStreamer writes it: frames, for an
    /// interlaced mode fields (`1080i50` takes 50 pictures, two make a
    /// frame).
    pub fn picture_rate(self) -> &'static str {
        match self {
            Self::Hd1080p2398 => "24000/1001",
            Self::Hd1080p24 => "24/1",
            Self::Hd1080p25 | Self::Uhd2160p25 => "25/1",
            Self::Hd1080p2997 | Self::Uhd2160p2997 => "30000/1001",
            Self::Hd1080p30 | Self::Uhd2160p30 => "30/1",
            Self::Hd720p50 | Self::Hd1080p50 | Self::Hd1080i50 | Self::Uhd2160p50 => "50/1",
            Self::Hd720p5994 | Self::Hd1080p5994 | Self::Hd1080i5994 | Self::Uhd2160p5994 => {
                "60000/1001"
            }
            Self::Hd720p60 | Self::Hd1080p60 | Self::Uhd2160p60 => "60/1",
        }
    }
}

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
}
