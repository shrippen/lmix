//! Zones (show mode SND.10, S-E.4): named sound outputs such as "vorn" or
//! "hinten". The show names a zone and its channels; which device and
//! device channels it plays on is set per machine (`ZonePatch`), so a show
//! runs elsewhere after a new patch (SSH.3).
//!
//! ```text
//!   show: Zone "hinten", 2 channels
//!   this machine: hinten → device "alsa_output.usb-…", channels [3, 4], −3 dB
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Zones per show (SND.10: at least 8).
pub const ZONES: usize = 16;

/// Channels of a zone (SND.10).
pub const ZONE_CHANNELS: u8 = 8;

/// Devices a zone plays on at once (SND.10: more than one).
pub const ZONE_OUTPUTS: usize = 4;

/// Highest device channel a zone may use.
pub const DEVICE_CHANNELS: u8 = 64;

/// Level bounds of a zone's output, dB.
pub const ZONE_GAIN_MIN_DB: f32 = -60.0;
pub const ZONE_GAIN_MAX_DB: f32 = 12.0;

/// A zone of the show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Zone {
    pub name: String,
    /// 1–8.
    pub channels: u8,
}

/// Where a zone plays on this machine: one device.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ZoneOutput {
    /// The engine's audio output; empty for the machine's default.
    #[serde(default)]
    pub device: String,
    /// Device channel (1-based) per zone channel: `[3, 4]` puts a stereo
    /// zone on channels 3 and 4.
    pub channels: Vec<u8>,
    #[serde(default)]
    pub gain_db: f32,
}

/// A zone's outputs on this machine (machine settings).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ZonePatch {
    /// The zone's name.
    pub zone: String,
    pub outputs: Vec<ZoneOutput>,
}

/// A zone as clients see it: the show's, with this machine's outputs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ZoneState {
    pub name: String,
    pub channels: u8,
    /// Empty: the zone plays nowhere on this machine (SND.16 warns).
    pub outputs: Vec<ZoneOutput>,
}
