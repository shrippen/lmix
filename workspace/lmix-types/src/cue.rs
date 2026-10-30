//! Cues and cue lists (show mode, `docs/LASTENHEFT-SHOW.md` §3.1–3.2).
//!
//! A cue list is a prepared sequence; GO starts the standby cue and its
//! follow chain, then standby moves past the chain:
//!
//! ```text
//!   GO ─► [pre wait] action ─► [post wait] ─► next cue (follow: after_wait)
//!                    └─ duration ─► end    ─► next cue (follow: after_end)
//! ```
//!
//! JSON of a cue, everything but `kind` optional:
//!
//! ```json
//! {"number": "12", "name": "Rain", "kind": "wait", "duration_ms": 2000,
//!  "pre_wait_ms": 500, "follow": "after_end"}
//! ```

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::action::Action;
use crate::input::{InputColor, InputId, InputRef};
use crate::sound::{FadeCue, FadeTarget, SoundCue};
use crate::stage::PictureCue;

/// Cues per list and per group level (CUE.20).
pub const CUES: usize = 1000;

/// Children of one group (CUE.20).
pub const GROUP_CHILDREN: usize = 64;

/// Group levels below a list (CUE.20).
pub const GROUP_DEPTH: usize = 3;

/// Longest pre wait, post wait or wait duration: 1 h (CUE.2).
pub const CUE_WAIT_MAX_MS: u32 = 3_600_000;

/// Waits are whole steps of 10 ms (CUE.2).
pub const CUE_WAIT_STEP_MS: u32 = 10;

/// Default shortest gap between two GOs (RUN.6).
pub const GO_GUARD_DEFAULT_MS: u32 = 300;

/// Longest gap the GO guard may be set to.
pub const GO_GUARD_MAX_MS: u32 = 5000;

/// Default and longest panic time (SAF.2).
pub const PANIC_DEFAULT_MS: u32 = 1000;
pub const PANIC_MAX_MS: u32 = 10_000;

/// Stable identity of a cue; survives renumbering and reordering (CUE.1).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(transparent)]
pub struct CueId(pub Uuid);

impl CueId {
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

impl Default for CueId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for CueId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// What a cue does when its pre wait is over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum CueKind {
    /// Runs `actions` from the catalog (CUE.7).
    #[default]
    Actions,
    /// Does nothing: a stage direction or a section mark (CUE.8).
    Note,
    /// Only runs for `duration_ms` (CUE.9).
    Wait,
    /// Starts `target` without moving standby (CUE.10).
    Start,
    /// Starts its `children` as `mode` says (CUE.4).
    Group,
    /// Plays `sound` into zones (SND.1); ends with the file, or after
    /// `duration_ms` when set.
    Sound,
    /// Fades a cue, zone or stage (`fade`) over `duration_ms` (SND.6,
    /// VIS.4, VIS.7).
    Fade,
    /// Shows `picture` on a stage (VIS.2); ends after `duration_ms` when
    /// set, a video with `end: stop` at its end, else when stopped.
    Picture,
}

/// What starts the next cue (CUE.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Follow {
    /// Nothing: the next cue waits for GO.
    #[default]
    None,
    /// The next cue starts when the post wait is over.
    AfterWait,
    /// The next cue starts when this one ends.
    AfterEnd,
}

/// How a group starts its children (CUE.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GroupMode {
    /// All children start at once, offset only by their pre waits; their
    /// follows are ignored.
    #[default]
    Timeline,
    /// The first child starts, the others through their follows.
    First,
}

/// Which other cues a cue stops when it starts (CUE.11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StopScope {
    /// Running cues of the same list or group.
    Siblings,
    /// Running cues of the same list.
    List,
    /// Every running cue.
    All,
}

/// Stop others when the cue's action starts (CUE.11).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct StopOthers {
    pub scope: StopScope,
    /// Fade of what they play (sound and pictures, from S1 step 2).
    #[serde(default)]
    pub fade_ms: u32,
}

/// What a start cue starts: a cue (by number or id, in `list` or its
/// own list), or with only `list` a GO in that list (CUE.10, RUN.7).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CueTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub list: Option<InputRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cue: Option<String>,
}

/// One step of a cue list (CUE.1–4, 7–11).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Cue {
    #[serde(default)]
    pub id: CueId,
    /// Free text, may be empty; unique in the show when set. Without
    /// spaces is best for remote control.
    #[serde(default)]
    pub number: String,
    /// Empty: [`Cue::display_name`] makes one from kind and target.
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: CueKind,
    /// Several lines, e.g. a stage direction.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<InputColor>,
    /// Marked for a look later, e.g. in a rehearsal (SUI.12).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub flagged: bool,
    /// From the start to the action, 0–1 h in steps of 10 ms.
    #[serde(default)]
    pub pre_wait_ms: u32,
    /// From the action to `follow: after_wait`.
    #[serde(default)]
    pub post_wait_ms: u32,
    #[serde(default)]
    pub follow: Follow,
    /// `actions`: 1–16 actions of the catalog.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub actions: Vec<Action>,
    /// `wait`: how long it runs. `actions`: how long it counts as
    /// running after its actions, e.g. the length of the clip it starts,
    /// so `follow: after_end` waits for it; without, it ends at once.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u32>,
    /// `start`: what it starts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<CueTarget>,
    /// `group`: how the children start.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<GroupMode>,
    /// `group`: 1–64 cues.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub children: Vec<Cue>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_others: Option<StopOthers>,
    /// `sound`: what it plays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sound: Option<SoundCue>,
    /// `fade`: what it fades.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fade: Option<FadeCue>,
    /// `picture`: what it shows.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub picture: Option<PictureCue>,
}

impl Cue {
    /// An actions cue named `name`.
    pub fn actions(name: &str, actions: Vec<Action>) -> Self {
        Self {
            name: name.to_string(),
            actions,
            ..Self::empty(CueKind::Actions)
        }
    }

    /// A cue of `kind` with nothing set.
    pub fn empty(kind: CueKind) -> Self {
        Self {
            id: CueId::new(),
            number: String::new(),
            name: String::new(),
            kind,
            note: String::new(),
            color: None,
            flagged: false,
            pre_wait_ms: 0,
            post_wait_ms: 0,
            follow: Follow::None,
            actions: Vec::new(),
            duration_ms: None,
            target: None,
            mode: None,
            children: Vec::new(),
            stop_others: None,
            sound: None,
            fade: None,
            picture: None,
        }
    }

    /// The name, or one from kind and target when it is empty (CUE.1),
    /// e.g. "Wait 2.5 s" or "Start 12".
    pub fn display_name(&self) -> String {
        if !self.name.is_empty() {
            return self.name.clone();
        }
        match self.kind {
            CueKind::Actions => match self.actions.first().and_then(action_name) {
                Some(first) if self.actions.len() > 1 => format!("{first} …"),
                Some(first) => first,
                None => "Actions".into(),
            },
            CueKind::Note => "Note".into(),
            CueKind::Wait => format!(
                "Wait {:.1} s",
                f64::from(self.duration_ms.unwrap_or(0)) / 1000.0
            ),
            CueKind::Start => {
                let target = self.target.clone().unwrap_or_default();
                match (target.cue, target.list) {
                    (Some(cue), _) => format!("Start {cue}"),
                    (None, Some(list)) => format!("Start list {}", input_ref_text(&list)),
                    (None, None) => "Start".into(),
                }
            }
            CueKind::Group => "Group".into(),
            CueKind::Sound => match &self.sound {
                Some(sound) => file_name(&sound.file).to_string(),
                None => "Sound".into(),
            },
            CueKind::Picture => match &self.picture {
                Some(picture) => file_name(&picture.file).to_string(),
                None => "Picture".into(),
            },
            CueKind::Fade => match self.fade.as_ref().map(|f| &f.target) {
                Some(FadeTarget::Cue(cue)) => format!("Fade {cue}"),
                Some(FadeTarget::Zone(zone)) => format!("Fade zone {zone}"),
                Some(FadeTarget::Stage(stage)) => format!("Fade stage {stage}"),
                None => "Fade".into(),
            },
        }
    }
}

/// The `action` tag of an action, e.g. `cut_direct`.
fn action_name(action: &Action) -> Option<String> {
    serde_json::to_value(action)
        .ok()?
        .get("action")?
        .as_str()
        .map(str::to_string)
}

/// The last part of a path, e.g. `rain.flac`.
fn file_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

fn input_ref_text(reference: &InputRef) -> String {
    match reference {
        InputRef::Number(n) => n.to_string(),
        InputRef::Text(text) => text.clone(),
    }
}

/// Where `cue_standby` moves standby (RUN.2).
///
/// JSON: `"next"`, `"previous"`, `"next_sequence"`,
/// `"previous_sequence"`, `"start"`, `{"index": 3}` (1-based, top level)
/// or `{"cue": "12"}` (number or id).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StandbyMove {
    Next,
    Previous,
    /// Past the next follow chain.
    NextSequence,
    /// To the start of the chain before.
    PreviousSequence,
    Start,
    Index(u32),
    Cue(String),
}

/// How a running cue stops (RUN.4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum StopMode {
    /// With its fade (sound and pictures, from S1 step 2).
    #[default]
    Fade,
    /// At once.
    Hard,
}

/// When GO takes another press after one went through (RUN.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum GoRearm {
    /// After the GO guard.
    #[default]
    Time,
    /// After the GO guard and once the button that gave it is let go.
    /// Only ways that report letting go wait for it: MIDI, OSC
    /// (`/lmix/cue/release`), the Console's space key. Each waits on its
    /// own, so a lost release blocks only that one.
    Release,
}

/// A cue list as clients see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CueList {
    pub id: InputId,
    /// 1-based position among the lists.
    pub number: u32,
    pub name: String,
    pub cues: Vec<Cue>,
    /// 1-based top-level cue that GO starts; past the end when done.
    pub standby: u32,
    /// GO without a list goes here (RUN.7).
    pub active: bool,
    /// Where the list comes from, e.g. `script:Session 3.md`; such a list
    /// is edited there, not in a client (IMP.9).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Runs with every panic and hard stop (SAF.4).
    #[serde(default)]
    pub emergency: bool,
    /// A grid of pads: no standby, no GO (RUN.12).
    #[serde(default)]
    pub pads: bool,
}

/// Where a running cue is (RUN.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RunPhase {
    PreWait,
    Running,
    /// Ended, its post wait still counts down to the follow.
    PostWait,
}

/// A cue that runs (RUN.3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RunningCue {
    pub list: InputId,
    pub cue: CueId,
    pub number: String,
    pub name: String,
    pub phase: RunPhase,
    /// Since its start, when the state was made.
    pub elapsed_ms: u64,
    /// Pre wait plus duration; `null` while unknown (a group).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
    /// `total_ms` less `elapsed_ms`, done for the client (CTL.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remaining_ms: Option<u64>,
}

/// Something that would go wrong when a cue runs (SUI.5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CueWarning {
    pub list: InputId,
    pub cue: CueId,
    #[serde(flatten)]
    pub problem: CueProblem,
    /// The problem in English, for logs and simple clients.
    pub message: String,
}

/// What would go wrong (`CueWarning`), for clients that say it in
/// their language.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "problem", rename_all = "snake_case")]
pub enum CueProblem {
    /// A start cue's cue is in no list.
    StartCueGone { target: String },
    /// A start cue's list is gone.
    StartListGone,
    /// An HTTP or Home Assistant action names no target of the machine.
    TargetGone { target: String },
    /// Its target did not answer the last check or send (SUI.5).
    TargetUnreached { target: String, reason: String },
    /// The targets are locked: it would send nothing (RUN.9).
    TargetLocked { target: String },
    /// An action names an input that is not there.
    InputGone { input: String },
    /// An input a cue uses has no media file.
    MediaMissing { input: String },
}

impl CueProblem {
    /// In English.
    pub fn message(&self) -> String {
        match self {
            Self::StartCueGone { target } => format!("starts cue {target}, which is not there"),
            Self::StartListGone => "starts a cue list that is not there".into(),
            Self::TargetGone { target } => {
                format!("sends to target {target}, which this machine does not have")
            }
            Self::TargetUnreached { target, reason } => {
                format!("sends to target {target}, which did not answer: {reason}")
            }
            Self::TargetLocked { target } => {
                format!("sends to target {target}, but network out is locked")
            }
            Self::InputGone { input } => format!("uses input {input}, which is not there"),
            Self::MediaMissing { input } => {
                format!("uses input {input}, whose media file is missing")
            }
        }
    }
}

/// What happened to a cue (`Event::Cue`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "what", rename_all = "snake_case")]
pub enum CueEvent {
    Started,
    Ended,
    Stopped,
    /// A GO came sooner than the guard allows and was dropped (RUN.6).
    GoIgnored,
    /// Its action failed; times and follows go on (CUE.19).
    Failed {
        reason: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cue_needs_only_its_kind() {
        let cue: Cue = serde_json::from_str(r#"{"kind":"wait","duration_ms":2500}"#).unwrap();

        assert_eq!(cue.kind, CueKind::Wait);
        assert_eq!(cue.follow, Follow::None);
        assert_eq!(cue.display_name(), "Wait 2.5 s");
    }

    #[test]
    fn names_from_kind_and_target() {
        let start = Cue {
            target: Some(CueTarget {
                list: None,
                cue: Some("12".into()),
            }),
            ..Cue::empty(CueKind::Start)
        };
        let cut = Cue::actions("", vec![Action::Cut, Action::Cut]);

        assert_eq!(start.display_name(), "Start 12");
        assert_eq!(cut.display_name(), "cut …");
        assert_eq!(
            Cue::actions("Rain", vec![Action::Cut]).display_name(),
            "Rain"
        );
    }

    #[test]
    fn sound_and_fade_cues_name_what_they_touch() {
        let rain: Cue =
            serde_json::from_str(r#"{"kind":"sound","sound":{"file":"/media/fx/rain.flac"}}"#)
                .unwrap();
        let fade: Cue = serde_json::from_str(
            r#"{"kind":"fade","duration_ms":5000,
                "fade":{"target":{"cue":"12"},"gain_db":-60,"then":"stop"}}"#,
        )
        .unwrap();

        assert_eq!(rain.display_name(), "rain.flac");
        assert_eq!(fade.display_name(), "Fade 12");
        assert_eq!(fade.fade.unwrap().then, crate::sound::AfterFade::Stop);
    }

    #[test]
    fn standby_moves_as_json() {
        let moves: Vec<StandbyMove> =
            serde_json::from_str(r#"["next","previous_sequence",{"index":3},{"cue":"12a"}]"#)
                .unwrap();

        assert_eq!(
            moves,
            vec![
                StandbyMove::Next,
                StandbyMove::PreviousSequence,
                StandbyMove::Index(3),
                StandbyMove::Cue("12a".into()),
            ]
        );
    }
}
