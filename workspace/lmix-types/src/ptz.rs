//! PTZ cameras over the network (PER.2): VISCA over IP.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Where an input's camera takes PTZ commands.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct PtzSpec {
    /// `host:port` of the camera, e.g. `192.168.1.20:52381`.
    pub address: String,
    #[serde(default)]
    pub protocol: PtzProtocol,
}

/// How VISCA reaches the camera.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PtzProtocol {
    /// Sony's VISCA over IP: UDP with an 8-byte header (port 52381).
    #[default]
    ViscaIp,
    /// Plain VISCA messages in UDP (PTZOptics and others, often port
    /// 1259).
    ViscaUdp,
}

/// Focus as an action sets it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PtzFocus {
    Auto,
    /// Manual, holding where it is.
    Manual,
    Near,
    Far,
    /// End a near or far move.
    Stop,
}

/// Presets a camera keeps (VISCA 0–127; most cameras fewer).
pub const PTZ_PRESETS: u8 = 128;
