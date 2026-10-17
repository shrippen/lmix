//! Output targets (show mode, `docs/LASTENHEFT-SHOW.md` §3.5, EXT).
//!
//! A target is a named address in the machine's settings: cues and
//! shortcuts name only the target, so a show runs on another machine
//! after a new patch, and secrets never go into a show file or script.
//!
//! ```json
//! {"action": "set_target", "target": {"name": "Haus", "kind": "http",
//!   "addresses": ["http://ha.local:8123"], "secret": "eyJ…"}}
//! {"action": "ha_service", "target": "Haus", "service": "light.turn_on",
//!   "entity": "light.wohnzimmer", "data": {"brightness_pct": 40}}
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Targets per machine.
pub const TARGETS: usize = 32;

/// Addresses per target (main and backups, sent to at once).
pub const TARGET_ADDRESSES: usize = 4;

/// Default and bounds of a target's time limit.
pub const TARGET_TIMEOUT_DEFAULT_MS: u32 = 3000;
pub const TARGET_TIMEOUT_MIN_MS: u32 = 100;
pub const TARGET_TIMEOUT_MAX_MS: u32 = 30_000;

/// What a target speaks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TargetKind {
    /// HTTP(S); addresses are base URLs.
    #[default]
    Http,
    /// OSC over UDP; addresses are `host:port` (OSC cues: S2).
    OscUdp,
    /// OSC over TCP (SLIP); addresses are `host:port` (OSC cues: S2).
    OscTcp,
}

/// A target as an action sets it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TargetSpec {
    pub name: String,
    #[serde(default)]
    pub kind: TargetKind,
    /// 1–4 addresses; a message goes to all of them (EXT.1).
    pub addresses: Vec<String>,
    /// Token or password, sent as `Authorization: Bearer`. Never shown
    /// back; `null` keeps the one set, `""` removes it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_ms: u32,
}

fn default_timeout() -> u32 {
    TARGET_TIMEOUT_DEFAULT_MS
}

/// A target as clients see it: without its secret.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Target {
    pub name: String,
    pub kind: TargetKind,
    pub addresses: Vec<String>,
    pub has_secret: bool,
    pub timeout_ms: u32,
}

/// HTTP method of a request.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    #[default]
    Post,
    Put,
    Patch,
    Delete,
}

/// How a message to a target went (`Event::Target`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum TargetResult {
    Ok,
    Failed { reason: String },
}
