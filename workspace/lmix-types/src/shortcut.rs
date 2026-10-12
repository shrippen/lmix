//! Shortcuts (AUT.1–3): a key and a button label for a list of actions
//! run in order, from the keyboard, the web page "Keys" or the API. A
//! shortcut belongs to the show or, `all_shows`, to lmix-core's settings;
//! where both bind a key or MIDI note, the show's wins.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::action::Action;
use crate::input::{InputColor, InputId};
use crate::midi::MidiBinding;

/// Actions per shortcut.
pub const SHORTCUT_ACTIONS: usize = 16;

/// A shortcut as clients see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Shortcut {
    /// Same id space as inputs and layouts.
    pub id: InputId,
    /// 1-based position in the list.
    pub number: u32,
    /// Button text (web page, controllers).
    pub label: String,
    /// Key on the Console, Qt notation (`F1`, `Ctrl+1`, `Space`); `None`
    /// for buttons only.
    #[serde(default)]
    pub key: Option<String>,
    /// Shown on the web page "Keys".
    #[serde(default)]
    pub web: bool,
    #[serde(default)]
    pub color: Option<InputColor>,
    /// Note or controller of a MIDI controller that runs it (AUT.4).
    #[serde(default)]
    pub midi: Option<MidiBinding>,
    /// Run in this order; the first failing one stops the rest.
    pub actions: Vec<Action>,
    /// From the settings, in every show (listed after the show's own).
    #[serde(default, skip_serializing_if = "is_false")]
    pub all_shows: bool,
    /// For all shows, but a shortcut of the show has its key: that one
    /// runs on the key.
    #[serde(default, skip_serializing_if = "is_false")]
    pub key_shadowed: bool,
    /// The same for its MIDI note or controller (also the show's T-bar).
    #[serde(default, skip_serializing_if = "is_false")]
    pub midi_shadowed: bool,
}

fn is_false(v: &bool) -> bool {
    !v
}

/// A shortcut as an action describes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ShortcutSpec {
    pub label: String,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub web: bool,
    #[serde(default)]
    pub color: Option<InputColor>,
    #[serde(default)]
    pub midi: Option<MidiBinding>,
    pub actions: Vec<Action>,
    /// Kept in the settings for every show instead of in this show.
    #[serde(default)]
    pub all_shows: bool,
}
