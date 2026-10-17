//! Fixed OSC subscribers (show mode CTL.4): buttons of e.g. Bitfocus
//! Companion show standby and remaining time without a module of their
//! own and without subscribing.
//!
//! ```json
//! {"action": "set_feed", "feed": {"name": "Stream Deck", "target": "Companion",
//!   "messages": [
//!     {"on": "standby", "address": "/custom-variable/standby/value", "args": ["{number} {name}"]},
//!     {"on": "running", "address": "/custom-variable/left/value", "args": ["{remaining}"]}]}}
//! ```
//!
//! Placeholders: `{list}`, `{number}`, `{name}`, `{note}` (the standby
//! cue, or the running one), `{remaining}`, `{elapsed}`, `{total}` (s,
//! one decimal), `{mode}` (`fade` or `hard`, panic). An argument that is
//! one time placeholder alone goes as a float, everything else as text.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Subscribers per machine.
pub const FEEDS: usize = 8;

/// Messages per subscriber.
pub const FEED_MESSAGES: usize = 16;

/// Arguments per message.
pub const FEED_ARGS: usize = 8;

/// When a message goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FeedOn {
    /// A list's standby changed (and once when the subscriber is set).
    Standby,
    /// The newest running cue of a list, at the cue event rate while cues
    /// run, and once more when the last one ended (empty values).
    Running,
    /// A panic or hard stop.
    Panic,
}

/// One message of a subscriber.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FeedMessage {
    pub on: FeedOn,
    /// OSC address, may hold placeholders: `/lmix/{list}/standby`.
    pub address: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// A fixed OSC subscriber.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Feed {
    pub name: String,
    /// An `osc_udp` output target.
    pub target: String,
    pub messages: Vec<FeedMessage>,
}
