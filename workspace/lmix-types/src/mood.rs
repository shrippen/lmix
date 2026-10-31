//! Moods (show mode, `docs/LASTENHEFT-SHOW.md` §3.16 MOOD).
//!
//! A mood says what runs on each layer of the show (light, music,
//! ambience, picture …). Moods come in mood groups: a base and its
//! variants, a variant changing only some layers and taking the rest from
//! the base (MOOD.2). Setting a mood compares layer by layer: what stays
//! the same runs on, what changes fades over (MOOD.4).
//!
//! ```text
//!   Tavern (base)   light: warm   music: lute   ambience: crowd
//!   └─ Night        light: dim    ·             ambience: crickets
//!   └─ Fight        ·             music: drums  ·
//! ```
//!
//! JSON of a show's moods, everything but names optional:
//!
//! ```json
//! {"layers": [{"id": "…", "name": "Light"}],
//!  "groups": [{"id": "…", "name": "Tavern",
//!              "base": {"<layer id>": {"kind": "actions", "actions": […]}},
//!              "variants": [{"id": "…", "name": "Night", "layers": {…}}]}],
//!  "collections": [{"id": "…", "name": "Act 1", "moods": [{"group": "…"}]}]}
//! ```

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::action::Action;
use crate::cue::CueId;
use crate::input::InputColor;
use crate::sound::SoundCue;
use crate::stage::PictureCue;

/// Layers of a show (MOOD.1).
pub const MOOD_LAYERS: usize = 32;

/// Mood groups of a show, and variants of one group.
pub const MOOD_GROUPS: usize = 256;

/// Collections of a show, and entries of one (MOOD.7).
pub const MOOD_COLLECTIONS: usize = 64;

/// Change time of a show without its own (MOOD.1).
pub const MOOD_CHANGE_MS: u32 = 3000;

/// Change time within a mood group of a show without its own (MOOD.1).
pub const MOOD_INSIDE_MS: u32 = 1000;

/// Id of a layer.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct LayerId(pub Uuid);

impl LayerId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for LayerId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for LayerId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Id of a mood group or of a variant; both live in one id space, so a
/// mood is found by one id.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct MoodId(pub Uuid);

impl MoodId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for MoodId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for MoodId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// One layer: a name, its place in `Moods::layers`, and the fade it takes
/// before the show's change time (MOOD.1).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MoodLayer {
    #[serde(default)]
    pub id: LayerId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade_ms: Option<u32>,
}

/// What a mood puts on a layer (MOOD.3).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayerContent {
    /// Nothing: what ran there fades out.
    #[default]
    Off,
    Sound {
        sound: SoundCue,
    },
    Picture {
        picture: PictureCue,
    },
    /// Actions run once when the layer takes them (light over HTTP, a
    /// scent over Home Assistant …).
    Actions {
        actions: Vec<Action>,
    },
}

/// A base mood and its variants (MOOD.2); on the Moods page "group".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MoodGroup {
    #[serde(default)]
    pub id: MoodId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<InputColor>,
    /// Every layer of the base; a layer not named is off.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub base: BTreeMap<LayerId, LayerContent>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub variants: Vec<MoodVariant>,
}

/// A variant: only the layers it changes; the rest comes from the base.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MoodVariant {
    #[serde(default)]
    pub id: MoodId,
    pub name: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub layers: BTreeMap<LayerId, LayerContent>,
}

/// A choice from the pools for an evening or a scene (MOOD.7): moods
/// (a group's base or a variant) and one-shots (pad cues, MOOD.8).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MoodCollection {
    #[serde(default)]
    pub id: MoodId,
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub moods: Vec<MoodId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub pads: Vec<CueId>,
}

/// The moods of a show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Moods {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<MoodLayer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub groups: Vec<MoodGroup>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub collections: Vec<MoodCollection>,
    /// What panic and hard stop set (MOOD.9): actions only, every other
    /// layer is off.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub panic: BTreeMap<LayerId, LayerContent>,
    /// Fade between moods of different groups (MOOD.1).
    #[serde(default = "default_change_ms")]
    pub change_ms: u32,
    /// Fade between moods of the same group.
    #[serde(default = "default_inside_ms")]
    pub inside_ms: u32,
}

fn default_change_ms() -> u32 {
    MOOD_CHANGE_MS
}

fn default_inside_ms() -> u32 {
    MOOD_INSIDE_MS
}

impl Default for Moods {
    fn default() -> Self {
        Self {
            layers: Vec::new(),
            groups: Vec::new(),
            collections: Vec::new(),
            panic: BTreeMap::new(),
            change_ms: MOOD_CHANGE_MS,
            inside_ms: MOOD_INSIDE_MS,
        }
    }
}

/// Where a mood is: its group and, for a variant, the variant.
#[derive(Clone, Copy, Debug)]
pub struct MoodPlace<'a> {
    pub group: &'a MoodGroup,
    pub variant: Option<&'a MoodVariant>,
}

impl MoodPlace<'_> {
    pub fn id(&self) -> MoodId {
        self.variant.map_or(self.group.id, |v| v.id)
    }

    /// "Tavern" or "Tavern / Night".
    pub fn name(&self) -> String {
        match self.variant {
            Some(v) => format!("{} / {}", self.group.name, v.name),
            None => self.group.name.clone(),
        }
    }
}

impl Moods {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// The mood of id `id`: a group's base or a variant.
    pub fn find(&self, id: MoodId) -> Option<MoodPlace<'_>> {
        self.groups.iter().find_map(|group| {
            if group.id == id {
                return Some(MoodPlace {
                    group,
                    variant: None,
                });
            }
            let variant = group.variants.iter().find(|v| v.id == id)?;
            Some(MoodPlace {
                group,
                variant: Some(variant),
            })
        })
    }

    /// The mood `text` names: an id, a group's name, or "group / variant"
    /// (names ignore case and spaces around the slash).
    pub fn lookup(&self, text: &str) -> Option<MoodPlace<'_>> {
        if let Ok(id) = text.trim().parse::<Uuid>() {
            return self.find(MoodId(id));
        }
        let same = |a: &str, b: &str| a.trim().eq_ignore_ascii_case(b.trim());
        let (group, variant) = match text.split_once('/') {
            Some((g, v)) => (g, Some(v)),
            None => (text, None),
        };
        let group = self.groups.iter().find(|g| same(&g.name, group))?;
        let variant = match variant {
            Some(v) => Some(group.variants.iter().find(|x| same(&x.name, v))?),
            None => None,
        };
        Some(MoodPlace { group, variant })
    }

    /// What mood `place` has on `layer`: the variant's own, else the
    /// base's, else off.
    pub fn content<'a>(&self, place: MoodPlace<'a>, layer: LayerId) -> &'a LayerContent {
        static OFF: LayerContent = LayerContent::Off;
        place
            .variant
            .and_then(|v| v.layers.get(&layer))
            .or_else(|| place.group.base.get(&layer))
            .unwrap_or(&OFF)
    }
}

/// Who put what runs on a layer (MOOD.5).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LayerOrigin {
    Mood { mood: MoodId },
    Panic,
}

/// One layer now, for the state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LayerNow {
    pub layer: LayerId,
    pub origin: LayerOrigin,
}

/// The mood set last and where each layer comes from, for the state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MoodNow {
    /// The mood set last; `None` before the first and after panic.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mood: Option<MoodId>,
    /// The mood before it, for "back".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub previous: Option<MoodId>,
    /// Layers with something on them, in the show's layer order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub layers: Vec<LayerNow>,
    /// Since when the last change fades, ms since the Unix epoch, and for
    /// how long; the page draws the progress from it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fading: Option<MoodFade>,
}

/// A change of mood in progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MoodFade {
    pub since_ms: u64,
    pub duration_ms: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tavern() -> (Moods, LayerId, LayerId) {
        let light = MoodLayer {
            id: LayerId::new(),
            name: "Light".into(),
            fade_ms: None,
        };
        let music = MoodLayer {
            id: LayerId::new(),
            name: "Music".into(),
            fade_ms: None,
        };
        let warm = LayerContent::Actions {
            actions: Vec::new(),
        };
        let group = MoodGroup {
            id: MoodId::new(),
            name: "Tavern".into(),
            color: None,
            base: [(light.id, warm), (music.id, LayerContent::Off)].into(),
            variants: vec![MoodVariant {
                id: MoodId::new(),
                name: "Night".into(),
                layers: [(light.id, LayerContent::Off)].into(),
            }],
        };
        let (l, m) = (light.id, music.id);
        let moods = Moods {
            layers: vec![light, music],
            groups: vec![group],
            ..Moods::default()
        };
        (moods, l, m)
    }

    #[test]
    fn a_variant_takes_the_layers_it_does_not_change_from_the_base() {
        let (moods, light, music) = tavern();
        let night = moods.lookup("tavern / NIGHT").expect("by names");
        assert_eq!(moods.content(night, light), &LayerContent::Off);
        let base = moods.lookup("Tavern").expect("the base");
        assert!(matches!(
            moods.content(base, light),
            LayerContent::Actions { .. }
        ));
        assert_eq!(moods.content(night, music), moods.content(base, music));
        assert_eq!(
            moods.find(night.id()).map(|p| p.name()),
            Some("Tavern / Night".into())
        );
        assert!(moods.lookup("Tavern / Day").is_none());
        assert!(moods.lookup(&night.id().to_string()).is_some());
    }

    #[test]
    fn moods_read_with_defaults_and_write_without_them() {
        let moods: Moods = serde_json::from_str("{}").expect("empty");
        assert!(moods.is_empty());
        assert_eq!(moods.change_ms, MOOD_CHANGE_MS);
        let (tavern, ..) = tavern();
        let json = serde_json::to_string(&tavern).expect("writes");
        let back: Moods = serde_json::from_str(&json).expect("reads");
        assert_eq!(back, tavern);
    }
}
