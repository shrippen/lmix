//! Show state machine: applies actions, decides what the engine must do.
//!
//! Pure: no I/O, no async. Every action either fails with a reason and
//! changes nothing, or changes the show and returns engine [`Command`]s.
//!
//! ```text
//!   Action ──► Switcher::apply ──► new state + [Command] ──► engine driver
//! ```

use lmix_types::action::{
    aux_index, AUTO_OFF_MAX_MS, DYNAMIC_SLOTS, OVERLAY_CHANNELS, TRANSITION_SLOTS,
};
use lmix_types::automation::{AUTOMATION_ACTIONS, TRIGGERS, TRIGGER_DELAY_MAX_MS};
use lmix_types::cue::{
    CueKind, CUES, CUE_WAIT_MAX_MS, CUE_WAIT_STEP_MS, GO_GUARD_MAX_MS, GROUP_CHILDREN, GROUP_DEPTH,
    PANIC_MAX_MS,
};
use lmix_types::input::{color_rgb, AUX_BUSES, REF_DYNAMIC, REF_PREVIEW, REF_PROGRAM, VOLUME_MAX};
use lmix_types::layout::{BORDER_MAX_PX, LAYOUTS, LAYOUT_BOXES, LAYOUT_TEMPLATES};
use lmix_types::shortcut::SHORTCUT_ACTIONS;
use lmix_types::state::API_VERSION;
use lmix_types::{
    Action, ActionError, AtEnd, AutoPlay, AuxBus, BoxCrop, BoxRect, BoxShown, BoxSpec,
    ChannelStrip, Effect, Ingest, Input, InputColor, InputId, InputPress, InputRef, InputStatus,
    Layout, LayoutBox, LayoutTemplate, Listen, Look, Output, OutputGroup, OutputId, OutputRef,
    OutputSource, OutputSpec, OutputStatus, OverlayChannel, PageRunning, QuickPlay, Shortcut,
    ShortcutSpec, ShowState, SignalLoss, SourceSpec, Stinger, StingerSpec, TemplateBox,
    TemplateInput, Transition, TransitionSlot, Transport, VolumeTarget, FADE_MAX_MS,
};
use lmix_types::{Cue, CueList, MidiKind, MidiMessage, Trigger, TriggerOn};

use crate::cues::{Guard, Hold, Out, Runner};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::show::{Show, ShowCueList, ShowInput, ShowLayout, ShowOutput, ShowShortcut};

mod graphics;
pub use graphics::{Rows, TemplateRef};

/// What the engine has to do after an action.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Feed `slot` from `source`, or empty it.
    SetSlot {
        slot: usize,
        source: Option<SourceSpec>,
    },
    /// Loop the player of `slot` or let it stop at the end (INP.10);
    /// `items` are its files, unchanged.
    SetAtEnd {
        slot: usize,
        items: Vec<String>,
        at_end: AtEnd,
    },
    /// Show `slot` in preview.
    Preview { slot: usize },
    /// Bring `slot` on program; `None` is black. `Effect::Cut` for a hard cut.
    Program {
        slot: Option<usize>,
        effect: Effect,
        duration_ms: u32,
    },
    /// Toggle fade to black.
    FadeToBlack { duration_ms: u32 },
    /// Solo `slot` on the monitor bus, or end its solo.
    SetListen { slot: usize, listen: Listen },
    /// Last frame (`Hold`) or black when the source of `slot` stalls.
    SetSignalLoss { slot: usize, mode: SignalLoss },
    /// Start output `id` (replaces a running one); its sound from aux
    /// bus `aux` (0-based) instead of the feed's.
    StartOutput {
        id: OutputId,
        spec: OutputSpec,
        feed: Feed,
        aux: Option<usize>,
    },
    /// Stop output `id`; stopping a stopped one is fine.
    StopOutput { id: OutputId },
    /// Channel strip of the audio mixer channel of `slot`.
    SetStrip { slot: usize, strip: ChannelStrip },
    /// Layout on engine composition `pip`: background slot and boxes.
    SetLayout {
        pip: usize,
        background: Option<usize>,
        boxes: Vec<EngineBox>,
    },
    /// Show composition `pip` in preview.
    PreviewLayout { pip: usize },
    /// Bring composition `pip` on program.
    ProgramLayout {
        pip: usize,
        effect: Effect,
        duration_ms: u32,
    },
    /// Colour correction of the picture of `slot`.
    SetLook { slot: usize, look: Look },
    /// Run JavaScript in the page of `slot` (web or graphics input).
    PageScript { slot: usize, script: String },
    /// Freeze the page of `slot`, or let it run.
    PageRunning { slot: usize, running: PageRunning },
    /// Stinger from program to `to` with `clip` (added to the library
    /// first if missing): the engine plays it and switches program at its
    /// cut point, frame-accurate.
    Stinger { to: usize, clip: LibraryClip },
    /// The engine's stinger clips: these and no others.
    StingerLibrary { clips: Vec<LibraryClip> },
    /// Hold the commands after it back by `ms`; the service runs them
    /// later, without blocking other actions.
    Wait { ms: u32 },
    /// Send of `slot` to aux bus `bus` (0-based), linear gain.
    SetAuxSend { slot: usize, bus: usize, gain: f64 },
    /// Master of aux bus `bus` (0-based), reached over `ramp_ms`.
    SetAuxBus {
        bus: usize,
        gain: f64,
        mute: bool,
        ramp_ms: u32,
    },
    /// Master of program sound, reached over `ramp_ms` (AUD.2, AUD.14).
    SetMaster { gain: f64, mute: bool, ramp_ms: u32 },
    /// Level of the monitor bus (AUD.4).
    SetMonitor { gain: f64 },
    /// Program in this format (OUT.1); the engine rebuilds its mixer only
    /// when it differs, and the commands after it set the mixer up again.
    SetFormat {
        format: lmix_types::MasterFormat,
        multiview: lmix_types::MultiviewLayout,
        /// Of the audio mixer, Hz (AUD.17).
        sample_rate: u32,
    },
    /// Mix `slot` (preview) over program by `position` (0–1), as the
    /// T-bar stands; 0 hides it again.
    TBar { slot: usize, position: f32 },
    /// Show `slot` on overlay `channel` (0-based) or take it off, faded
    /// over `duration_ms`.
    Overlay {
        channel: usize,
        slot: Option<usize>,
        duration_ms: u32,
    },
    /// Place overlay `channel` (0-based) in `rect`, aspect kept; `None`
    /// fills the picture (OVL.2).
    OverlayPlace {
        channel: usize,
        rect: Option<BoxRect>,
    },
    /// Play, pause, restart or step the media player of `slot`.
    Transport { slot: usize, transport: Transport },
    /// Jump within the item the slot's media player plays.
    Seek { slot: usize, position_ms: u64 },
    /// Play item `index` (0-based) of the slot's playlist.
    PlayItem { slot: usize, index: usize },
    /// Set the audio channel of `slot`: linear gain (1.0 = unity) and
    /// mute, faded over `ramp_ms`.
    SetAudio {
        slot: usize,
        gain: f64,
        muted: bool,
        ramp_ms: u32,
    },
    /// The show's zones and stages with this machine's outputs; the
    /// engine builds its playback flow for them (running sounds and
    /// pictures stop). Without zones and stages: none.
    SetPlayback { playback: Playback },
    /// Hold these sounds and pictures ready (decoded, waiting); the ones
    /// held before and not named go, unless they started meanwhile.
    SetPreload { preload: Preload },
    /// Play `sound` as `id`, replacing a sound of that id.
    PlaySound {
        id: String,
        sound: crate::playback::EngineSound,
    },
    FadeSound {
        id: String,
        gain_db: f32,
        duration_ms: u32,
        then: lmix_types::AfterFade,
    },
    /// Stop sound `id`, or every sound.
    StopSound { id: Option<String>, fade_ms: u32 },
    /// Show `picture` as `id`, replacing a picture of that id.
    ShowPicture {
        id: String,
        picture: crate::playback::EnginePicture,
    },
    FadePicture {
        id: String,
        opacity: f32,
        duration_ms: u32,
        then: lmix_types::AfterFade,
    },
    /// Take picture `id`, or every picture, off its stage.
    StopPicture { id: Option<String>, fade_ms: u32 },
    /// Stage `stage` (place in the show) to its brightness.
    SetStageLevel {
        stage: usize,
        brightness: f32,
        duration_ms: u32,
    },
    /// Zone `zone` (place in the show) to its level and mute.
    SetZoneLevel {
        zone: usize,
        gain_db: Option<f32>,
        mute: Option<bool>,
        duration_ms: u32,
    },
}

/// What the engine's playback flow is built for.
#[derive(Clone, Debug, PartialEq)]
pub struct Playback {
    pub zones: Vec<lmix_types::ZoneState>,
    pub stages: Vec<lmix_types::StageState>,
    /// Of the stages: the program's.
    pub framerate: lmix_types::Framerate,
}

/// Sounds and pictures the engine holds ready, by id.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Preload {
    pub sounds: Vec<(String, crate::playback::EngineSound)>,
    pub pictures: Vec<(String, crate::playback::EnginePicture)>,
}

/// A layout box as the engine places it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineBox {
    pub rect: BoxRect,
    pub slot: usize,
    pub border_px: u8,
    pub crop: BoxCrop,
}

/// A clip of the engine's stinger library: the file of a stinger button's
/// clip input and its cut point. One per file: strom keeps the settings
/// of a clip by its file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryClip {
    pub file: String,
    pub cut_ms: u32,
}

/// What stands in preview or program: an input's slot or a layout's
/// composition.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Bus {
    Slot(usize),
    Layout(usize),
}

/// What an output flow subscribes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
    Program,
    Multiview,
    /// One input's slot; `audio` when its source has sound.
    Slot {
        slot: usize,
        audio: bool,
    },
}

/// What is on air, before and after an action (triggers).
#[derive(Clone, Debug, PartialEq)]
pub struct OnAir {
    program: Option<InputId>,
    preview: Option<InputId>,
    overlays: Vec<Option<InputId>>,
    /// When each channel last went on (see `Switcher::overlay_since`).
    overlay_since: Vec<u64>,
}

/// Actions that manage or run shortcuts.
fn changes_shortcuts(action: &Action) -> bool {
    matches!(
        action,
        Action::AddShortcut { .. }
            | Action::SetShortcut { .. }
            | Action::RemoveShortcut { .. }
            | Action::RunShortcut { .. }
    )
}

/// Actions a cue or trigger may not hold: they run or change shortcuts,
/// cues or triggers (no loops).
fn runs_automation(action: &Action) -> bool {
    changes_shortcuts(action)
        || matches!(
            action,
            Action::SetTriggers { .. }
                | Action::SetTitleTriggers { .. }
                | Action::TitleReport { .. }
                | Action::AddCueList { .. }
                | Action::SetCueList { .. }
                | Action::RemoveCueList { .. }
                | Action::CueGo { .. }
                | Action::CueStart { .. }
                | Action::CueStop { .. }
                | Action::CueStandby { .. }
                | Action::CueReset
                | Action::SetActiveCueList { .. }
                | Action::SetGoGuard { .. }
                | Action::SetGoRearm { .. }
                | Action::CuePanic { .. }
                | Action::CueHardStop
                | Action::SetPanicTime { .. }
                | Action::SetEmergencyList { .. }
                | Action::SetScriptFolder { .. }
                | Action::SetZone { .. }
                | Action::RemoveZone { .. }
                | Action::SetZonePatch { .. }
                | Action::RemoveZonePatch { .. }
                | Action::SetStage { .. }
                | Action::RemoveStage { .. }
                | Action::SetStagePatch { .. }
                | Action::RemoveStagePatch { .. }
        )
}

/// At most `TRIGGERS` triggers (delay, actions), each waiting at most 30 s
/// with 1–16 actions that run no automation.
pub(crate) fn check_triggers<'a>(
    triggers: impl ExactSizeIterator<Item = (u32, &'a [Action])>,
) -> Result<(), ActionError> {
    let bad = |reason: String| Err(ActionError::BadTriggers { reason });
    if triggers.len() > TRIGGERS {
        return bad(format!("at most {TRIGGERS} triggers"));
    }
    for (delay_ms, actions) in triggers {
        if delay_ms > TRIGGER_DELAY_MAX_MS {
            return bad(format!("delays are at most {TRIGGER_DELAY_MAX_MS} ms"));
        }
        if actions.is_empty() || actions.len() > AUTOMATION_ACTIONS {
            return bad(format!("1–{AUTOMATION_ACTIONS} actions each"));
        }
        if actions.iter().any(runs_automation) {
            return bad("triggers cannot run shortcuts, cues or change automation".into());
        }
    }
    Ok(())
}

/// 0-based index of dynamic slot `slot` (1–8).
fn dynamic_index(slot: u8) -> Result<usize, ActionError> {
    if slot == 0 || slot > DYNAMIC_SLOTS {
        return Err(ActionError::BadDynamic {
            slot,
            max: DYNAMIC_SLOTS,
        });
    }
    Ok(usize::from(slot - 1))
}

/// The standby cue of a list, where an action names a cue (CTL.1).
const CUE_STANDBY: &str = "standby";

/// The active cue list, where an action names a list (CTL.1).
const LIST_ACTIVE: &str = "active";

/// A name and at most `CUES` well-formed cues (CUE.1–11, CUE.20).
fn check_cues(name: &str, cues: &[Cue]) -> Result<(), ActionError> {
    if name.trim().is_empty() {
        return Err(bad_cues("it needs a name".into()));
    }
    if cues.len() > CUES {
        return Err(bad_cues(format!("at most {CUES} cues")));
    }
    cues.iter().try_for_each(|cue| check_cue(cue, 0))
}

fn bad_cues(reason: String) -> ActionError {
    ActionError::BadCueList { reason }
}

/// One cue and its children, `depth` group levels down. Fields of
/// another kind are refused, so a typo in a script shows.
fn check_cue(cue: &Cue, depth: usize) -> Result<(), ActionError> {
    let named = cue.display_name();
    let bad = |what: &str| Err(bad_cues(format!("cue {named}: {what}")));
    for wait in [
        cue.pre_wait_ms,
        cue.post_wait_ms,
        cue.duration_ms.unwrap_or(0),
    ] {
        if wait > CUE_WAIT_MAX_MS || wait % CUE_WAIT_STEP_MS != 0 {
            return bad(&format!(
                "waits are 0–{CUE_WAIT_MAX_MS} ms in steps of {CUE_WAIT_STEP_MS} ms"
            ));
        }
    }
    let own = |field: bool, kind: CueKind| !field || cue.kind == kind;
    if !own(!cue.actions.is_empty(), CueKind::Actions)
        || !(cue.duration_ms.is_none()
            || matches!(
                cue.kind,
                CueKind::Wait
                    | CueKind::Actions
                    | CueKind::Sound
                    | CueKind::Fade
                    | CueKind::Picture
            ))
        || !own(cue.target.is_some(), CueKind::Start)
        || !own(cue.sound.is_some(), CueKind::Sound)
        || !own(cue.fade.is_some(), CueKind::Fade)
        || !own(cue.picture.is_some(), CueKind::Picture)
        || !own(
            cue.mode.is_some() || !cue.children.is_empty(),
            CueKind::Group,
        )
    {
        return bad("has fields of another kind");
    }
    match cue.kind {
        CueKind::Actions => {
            if cue.actions.is_empty() || cue.actions.len() > AUTOMATION_ACTIONS {
                return bad(&format!("1–{AUTOMATION_ACTIONS} actions"));
            }
            if cue.actions.iter().any(runs_automation) {
                return bad(
                    "actions cannot run shortcuts, cues or change automation; use a start cue",
                );
            }
        }
        CueKind::Note => {}
        CueKind::Wait if cue.duration_ms.is_none() => return bad("a wait needs duration_ms"),
        CueKind::Wait => {}
        CueKind::Start => {
            let target = cue.target.clone().unwrap_or_default();
            if target.list.is_none() && target.cue.is_none() {
                return bad("a start cue needs a target cue or list");
            }
        }
        CueKind::Group => {
            if cue.children.is_empty() || cue.children.len() > GROUP_CHILDREN {
                return bad(&format!("a group has 1–{GROUP_CHILDREN} cues"));
            }
            if depth + 1 > GROUP_DEPTH {
                return bad(&format!("groups are at most {GROUP_DEPTH} levels deep"));
            }
            for child in &cue.children {
                check_cue(child, depth + 1)?;
            }
        }
        CueKind::Sound | CueKind::Fade | CueKind::Picture => {
            if let Err(e) = crate::playback::check_cue(cue) {
                return bad(&e.to_string());
            }
        }
    }
    Ok(())
}

/// Ids and numbers set are unique in the whole show (CUE.1).
fn check_unique(lists: &[ShowCueList]) -> Result<(), ActionError> {
    fn walk<'a>(cues: &'a [Cue], all: &mut Vec<&'a Cue>) {
        for cue in cues {
            all.push(cue);
            walk(&cue.children, all);
        }
    }
    let mut all = Vec::new();
    for list in lists {
        walk(&list.cues, &mut all);
    }
    let mut ids = std::collections::HashSet::new();
    let mut numbers = std::collections::HashSet::new();
    for cue in all {
        if !ids.insert(cue.id) {
            return Err(bad_cues(format!("cue id {} twice", cue.id)));
        }
        if !cue.number.is_empty() && !numbers.insert(cue.number.as_str()) {
            return Err(bad_cues(format!("cue number {} twice", cue.number)));
        }
    }
    Ok(())
}

/// Longest countdown of a `clip_end` trigger.
const CLIP_COUNTDOWN_MAX_MS: u32 = 60_000;

/// Closed inputs kept for `restore_input`.
const REMOVED_KEPT: usize = 10;

/// Anti-click fade for audio changes outside a transition.
pub const AUDIO_RAMP_MS: u32 = 20;

/// strom's stinger takes one input to another, never a layout.
const STINGER_NO_LAYOUTS: &str = "only between inputs, not with layouts";

/// What one audio channel is set to.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mix {
    gain: f64,
    muted: bool,
}

/// Fader gain of a volume: squared, so the scale feels even to the ear
/// (50 ≈ -12 dB, 100 = unity).
pub fn fader_gain(volume: u8) -> f64 {
    let v = f64::from(volume) / f64::from(VOLUME_MAX);
    v * v
}

/// Show plus runtime state that is not saved.
pub struct Switcher {
    show: Show,
    slots: usize,
    path: Option<String>,
    fade_to_black: bool,
    /// T-bar position while it is pulled; runtime only.
    t_bar: f32,
    /// A stinger runs: switching is refused until the engine reports
    /// its end (`stinger_finished`), at the latest at this time.
    stinger_until: Option<Instant>,
    /// How long the lock holds without an end, counted again when the
    /// engine starts the take.
    stinger_hold: Duration,
    /// The engine's generation of the take behind the lock; only its
    /// end frees it.
    stinger_take: u64,
    /// Stinger library the engine has; `None` sends it again.
    stinger_sent: Option<Vec<LibraryClip>>,
    /// The engine can apply looks (GL mixer).
    looks_available: bool,
    /// Locked against closing, loading and stopping (AUT.11).
    locked: bool,
    /// Inputs closed last, with their place in the bar (AUT.10).
    removed: Vec<(usize, ShowInput)>,
    /// Standby and running cues of the cue lists.
    cues: Runner,
    /// What would go wrong when they run (SUI.5); see `refresh_warnings`.
    cue_warnings: Vec<lmix_types::cue::CueWarning>,
    /// Targets that did not answer their last check or send (SUI.5).
    unreached: std::collections::BTreeMap<String, String>,
    /// The show file's older version, until saved (SHW.7).
    older_format: Option<u32>,
    status: HashMap<InputId, InputStatus>,
    /// Audio channel settings the engine has, per slot.
    mix: HashMap<usize, Mix>,
    /// Solos; runtime only, like a desk's solo buttons.
    listen: HashMap<InputId, Listen>,
    /// Engine address as senders reach it, for WHIP ingest links.
    ingest_base: Option<String>,
    /// Output states; runtime only.
    output_status: HashMap<OutputId, OutputStatus>,
    /// Length of the item each player plays, by slot; runtime only.
    clip_ms: HashMap<usize, u64>,
    /// Templates, data source rows, title calls; runtime only.
    titles: graphics::TitleRuntime,
    /// What `"dynamic1"`… name, and the dynamic values (AUT.5); runtime.
    dynamic_inputs: [Option<InputId>; DYNAMIC_SLOTS as usize],
    dynamic_values: [String; DYNAMIC_SLOTS as usize],
    /// Shortcuts for all shows (AUT.3), from lmix-core's settings.
    all_shows: Vec<ShowShortcut>,
    /// `all_shows` changed since the service last saved it.
    all_shows_changed: bool,
    /// Output targets of this machine with their secrets (EXT.1), and
    /// the fixed OSC subscribers (CTL.4).
    targets: Vec<crate::targets::TargetConfig>,
    feeds: Vec<lmix_types::Feed>,
    /// Where the show's zones play on this machine (SND.10).
    zone_patches: Vec<lmix_types::ZonePatch>,
    stage_patches: Vec<lmix_types::StagePatch>,
    /// The zones and stages the engine got last; `None`: send them again.
    playback_sent: Option<Playback>,
    /// What the engine was last told to hold ready; `None`: tell it again.
    preload_sent: Option<Preload>,
    targets_changed: bool,
    /// Media files not found per input (SHW.4); the service checks.
    missing: HashMap<InputId, Vec<String>>,
    /// The input each overlay channel showed last (OVL.8); runtime only.
    overlay_last: [Option<InputId>; OVERLAY_CHANNELS as usize],
    /// Counts overlays going on; each channel keeps the count of its last
    /// time, so an auto off (OVL.2) finds out whether it is still due.
    overlays_shown: u64,
    overlay_since: [u64; OVERLAY_CHANNELS as usize],
    /// Layout templates (LAY.5), from lmix-core's settings.
    layout_templates: Vec<LayoutTemplate>,
    /// `layout_templates` changed since the service last saved them.
    layout_templates_changed: bool,
}

/// Where a shortcut is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    Show(usize),
    AllShows(usize),
}

impl Switcher {
    pub fn new(slots: usize) -> Self {
        Self {
            show: Show::default(),
            slots,
            path: None,
            fade_to_black: false,
            t_bar: 0.0,
            stinger_until: None,
            stinger_hold: Duration::ZERO,
            stinger_take: 0,
            stinger_sent: Some(Vec::new()),
            looks_available: false,
            locked: false,
            removed: Vec::new(),
            cues: Runner::default(),
            cue_warnings: Vec::new(),
            unreached: Default::default(),
            older_format: None,
            status: HashMap::new(),
            mix: HashMap::new(),
            listen: HashMap::new(),
            ingest_base: None,
            clip_ms: HashMap::new(),
            output_status: HashMap::new(),
            titles: graphics::TitleRuntime::default(),
            dynamic_inputs: Default::default(),
            dynamic_values: Default::default(),
            all_shows: Vec::new(),
            all_shows_changed: false,
            targets: Vec::new(),
            feeds: Vec::new(),
            zone_patches: Vec::new(),
            stage_patches: Vec::new(),
            playback_sent: None,
            preload_sent: None,
            targets_changed: false,
            missing: HashMap::new(),
            overlay_last: Default::default(),
            overlays_shown: 0,
            overlay_since: Default::default(),
            layout_templates: Vec::new(),
            layout_templates_changed: false,
        }
    }

    /// Media files of each input with files (file, playlist), for the
    /// service to look for.
    pub fn media_files(&self) -> Vec<(InputId, Vec<String>)> {
        self.show
            .inputs
            .iter()
            .filter_map(|i| match &i.source {
                SourceSpec::File { path } => Some((i.id, vec![path.clone()])),
                SourceSpec::Playlist { paths } => Some((i.id, paths.clone())),
                _ => None,
            })
            // Addresses are no files.
            .map(|(id, paths)| {
                (
                    id,
                    paths.into_iter().filter(|p| !p.contains("://")).collect(),
                )
            })
            .collect()
    }

    /// What is on program now, for the service.
    pub fn state_program(&self) -> Option<InputId> {
        self.show.program
    }

    /// Input `id` got a new source: nothing known missing until checked.
    pub fn forget_missing(&mut self, id: InputId) {
        self.missing.remove(&id);
    }

    /// Which media files are missing now; true if that changed.
    pub fn set_missing(&mut self, missing: HashMap<InputId, Vec<String>>) -> bool {
        // A check that started before a change (relocated, replaced) keeps
        // only paths the input still has.
        let now: HashMap<InputId, Vec<String>> = self.media_files().into_iter().collect();
        let missing: HashMap<_, _> = missing
            .into_iter()
            .map(|(id, files)| {
                let current = now.get(&id);
                let files: Vec<String> = files
                    .into_iter()
                    .filter(|f| current.is_some_and(|c| c.contains(f)))
                    .collect();
                (id, files)
            })
            .filter(|(_, files)| !files.is_empty())
            .collect();
        if missing == self.missing {
            return false;
        }
        self.missing = missing;
        true
    }

    /// Missing files per input, as last checked.
    pub fn missing_media(&self) -> Vec<(InputId, Vec<String>)> {
        self.missing
            .iter()
            .map(|(id, files)| (*id, files.clone()))
            .collect()
    }

    /// The source of input `id` (for relocating its files).
    pub fn source_of(&self, id: InputId) -> Option<&SourceSpec> {
        self.show
            .inputs
            .iter()
            .find(|i| i.id == id)
            .map(|i| &i.source)
    }

    /// Shortcuts for all shows, as the settings hold them.
    pub fn all_shows_shortcuts(&self) -> &[ShowShortcut] {
        &self.all_shows
    }

    /// Take shortcuts for all shows from the settings (start-up).
    pub fn set_all_shows_shortcuts(&mut self, shortcuts: Vec<ShowShortcut>) {
        self.all_shows = shortcuts;
        self.all_shows_changed = false;
        self.distinct_shortcut_ids();
    }

    /// A shortcut for all shows with the id of one of the show's (a show
    /// saved before it moved) gets a new one: ids name one shortcut.
    fn distinct_shortcut_ids(&mut self) {
        for s in &mut self.all_shows {
            if self.show.shortcuts.iter().any(|o| o.id == s.id) {
                s.id = InputId::new();
                self.all_shows_changed = true;
            }
        }
    }

    /// Output targets and subscribers as the settings hold them, secrets
    /// included.
    pub fn target_settings(&self) -> crate::targets::Settings {
        crate::targets::Settings {
            targets: self.targets.clone(),
            feeds: self.feeds.clone(),
            zone_patches: self.zone_patches.clone(),
            stage_patches: self.stage_patches.clone(),
        }
    }

    /// Take the output targets and subscribers from the settings
    /// (start-up).
    pub fn set_target_settings(&mut self, settings: crate::targets::Settings) {
        self.targets = settings.targets;
        self.feeds = settings.feeds;
        self.zone_patches = settings.zone_patches;
        self.stage_patches = settings.stage_patches;
        self.targets_changed = false;
    }

    /// Take targets and subscribers of a package (SSH.5).
    pub fn merge_target_settings(
        &mut self,
        from: crate::targets::Settings,
    ) -> Result<(), ActionError> {
        let mut settings = self.target_settings();
        crate::targets::merge(&mut settings, from)?;
        self.targets = settings.targets;
        self.feeds = settings.feeds;
        self.zone_patches = settings.zone_patches;
        self.stage_patches = settings.stage_patches;
        self.targets_changed = true;
        Ok(())
    }

    /// This machine's settings for a file, without secrets (SHW.8).
    pub fn settings_file(&self) -> crate::settings::SettingsFile {
        let carried = crate::targets::without_secrets(&self.target_settings());
        crate::settings::SettingsFile {
            shortcuts: self.all_shows.clone(),
            layout_templates: self.layout_templates.clone(),
            targets: carried.targets,
            feeds: carried.feeds,
            zone_patches: carried.zone_patches,
            stage_patches: carried.stage_patches,
        }
    }

    /// Take `file` instead of the current settings, all or nothing;
    /// targets of the same name keep their secret (SHW.8).
    pub fn take_settings(
        &mut self,
        file: crate::settings::SettingsFile,
    ) -> Result<(), ActionError> {
        let bad = |reason: String| Err(ActionError::BadShortcut { reason });
        let mut keys = std::collections::HashSet::new();
        for s in &file.shortcuts {
            if s.label.trim().is_empty()
                || s.actions.is_empty()
                || s.actions.len() > SHORTCUT_ACTIONS
                || s.actions.iter().any(changes_shortcuts)
            {
                return bad(format!(
                    "shortcut {}: label and 1–{SHORTCUT_ACTIONS} actions that change no shortcuts",
                    s.label
                ));
            }
            if let Some(key) = &s.key {
                if !keys.insert(key.clone()) {
                    return bad(format!("key {key} twice"));
                }
            }
        }
        // Targets: the file's, with the secrets of those of the same name.
        let mut next = crate::targets::Settings {
            targets: self
                .targets
                .iter()
                .filter(|t| file.targets.iter().any(|f| f.name == t.name))
                .cloned()
                .collect(),
            feeds: Vec::new(),
            zone_patches: Vec::new(),
            stage_patches: Vec::new(),
        };
        crate::targets::merge(
            &mut next,
            crate::targets::Settings {
                targets: file.targets,
                feeds: file.feeds,
                zone_patches: file.zone_patches,
                stage_patches: file.stage_patches,
            },
        )?;
        let templates = std::mem::take(&mut self.layout_templates);
        if let Err(e) = self.keep_layout_templates(file.layout_templates) {
            self.layout_templates = templates;
            return Err(e);
        }
        self.all_shows = file.shortcuts;
        self.distinct_shortcut_ids();
        self.targets = next.targets;
        self.feeds = next.feeds;
        self.zone_patches = next.zone_patches;
        self.stage_patches = next.stage_patches;
        self.all_shows_changed = true;
        self.targets_changed = true;
        Ok(())
    }

    /// Factory settings (SHW.8).
    pub fn reset_settings(&mut self) {
        self.all_shows.clear();
        self.layout_templates.clear();
        self.targets.clear();
        self.feeds.clear();
        self.zone_patches.clear();
        self.stage_patches.clear();
        self.all_shows_changed = true;
        self.layout_templates_changed = true;
        self.targets_changed = true;
    }

    /// The fixed OSC subscribers (CTL.4).
    pub fn feeds(&self) -> &[lmix_types::Feed] {
        &self.feeds
    }

    /// The target `name`, for sending.
    pub fn target(&self, name: &str) -> Result<crate::targets::TargetConfig, ActionError> {
        crate::targets::find(&self.targets, name).cloned()
    }

    /// Whether the targets changed since the last call; again after a
    /// failed save.
    pub fn take_targets_changed(&mut self) -> bool {
        std::mem::take(&mut self.targets_changed)
    }

    pub fn targets_unsaved(&mut self) {
        self.targets_changed = true;
    }

    /// Mark the shortcuts for all shows as changed again (a save failed).
    pub fn all_shows_unsaved(&mut self) {
        self.all_shows_changed = true;
    }

    /// Whether shortcuts for all shows changed since the last call.
    pub fn take_all_shows_changed(&mut self) -> bool {
        std::mem::take(&mut self.all_shows_changed)
    }

    /// Layout templates, as the settings hold them (LAY.5).
    pub fn layout_templates(&self) -> &[LayoutTemplate] {
        &self.layout_templates
    }

    /// Take layout templates from the settings (start-up).
    pub fn set_layout_templates(&mut self, templates: Vec<LayoutTemplate>) {
        self.layout_templates = templates;
        self.layout_templates_changed = false;
    }

    /// Mark the layout templates as changed again (a save failed).
    pub fn layout_templates_unsaved(&mut self) {
        self.layout_templates_changed = true;
    }

    /// Whether layout templates changed since the last call.
    pub fn take_layout_templates_changed(&mut self) -> bool {
        std::mem::take(&mut self.layout_templates_changed)
    }

    pub fn slots(&self) -> usize {
        self.slots
    }

    pub fn show(&self) -> &Show {
        &self.show
    }

    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: String) {
        self.path = Some(path);
    }

    pub fn set_ingest_base(&mut self, url: String) {
        self.ingest_base = Some(url.trim_end_matches('/').to_string());
    }

    /// Ingest links of a WHIP source.
    fn ingest_of(&self, source: &SourceSpec) -> Option<Ingest> {
        let (SourceSpec::Whip { endpoint }, Some(base)) = (source, &self.ingest_base) else {
            return None;
        };
        Some(Ingest {
            whip_url: format!("{base}/whip/{endpoint}"),
            browser_url: format!("{base}/player/whip-ingest"),
        })
    }

    /// Engine reports an input's health.
    pub fn set_status(&mut self, id: InputId, status: InputStatus) {
        self.status.insert(id, status);
    }

    /// Apply one action. File actions (new/open/save) are handled by the
    /// service, which needs I/O; it calls [`Self::replace`] for them.
    pub fn apply(&mut self, action: Action) -> Result<Vec<Command>, ActionError> {
        let Some(action) = self.pressed(action)? else {
            return Ok(Vec::new());
        };
        let ramp_ms = self.audio_ramp(&action);
        let leave_ms = self.leave_ms(&action);
        let airing = self.airing();
        let mut commands = self.apply_action(action)?;
        commands.extend(self.playback_commands());
        commands.extend(self.stinger_commands());
        let audio = self.audio_commands(ramp_ms);
        let (on, off) = self.auto_play_commands(&airing);
        // A stinger switches sound and players at its cut: after its wait.
        let cut = commands
            .iter()
            .position(|c| matches!(c, Command::Wait { .. }));
        match cut {
            Some(_) => {
                commands.extend(audio);
                commands.extend(off);
            }
            None => {
                commands.extend(audio);
                // The old picture is still seen while it fades out.
                if leave_ms > 0 && !off.is_empty() {
                    commands.push(Command::Wait { ms: leave_ms });
                }
                commands.extend(off);
            }
        }
        // Before the switch: a restarted clip shows no old frame.
        let at = cut.map_or(0, |c| c + 1);
        commands.splice(at..at, on);
        Ok(commands)
    }

    fn apply_action(&mut self, action: Action) -> Result<Vec<Command>, ActionError> {
        match action {
            action @ (Action::AddGraphics { .. }
            | Action::RemoveGraphics { .. }
            | Action::RenameGraphics { .. }
            | Action::TitleLoad { .. }
            | Action::TitlePlay { .. }
            | Action::TitleNext { .. }
            | Action::TitleStop { .. }
            | Action::TitleClear { .. }
            | Action::TitleUpdate { .. }
            | Action::TitleAdd { .. }
            | Action::TitleTransform { .. }
            | Action::TitleCall { .. }
            | Action::TitleBind { .. }
            | Action::TitleReport { .. }
            | Action::SetTitleTriggers { .. }
            | Action::SaveDataset { .. }
            | Action::RemoveDataset { .. }
            | Action::ApplyDataset { .. }
            | Action::DatasetNext { .. }
            | Action::DatasetPrevious { .. }
            | Action::AddDataSource { .. }
            | Action::RemoveDataSource { .. }
            | Action::DataRow { .. }
            | Action::DataNext { .. }
            | Action::DataPrevious { .. }
            | Action::SetDataAuto { .. }) => {
                let action = self.with_dynamic_values(action);
                self.apply_title(action)
            }
            Action::SetDynamicInput { slot, input } => {
                let at = dynamic_index(slot)?;
                self.dynamic_inputs[at] = match input {
                    Some(input) => Some(self.show.inputs[self.find(&input)?].id),
                    None => None,
                };
                Ok(Vec::new())
            }
            Action::SetDynamicValue { slot, value } => {
                self.dynamic_values[dynamic_index(slot)?] = value;
                Ok(Vec::new())
            }
            Action::AddInput { source, name } => self.add(source, name),
            Action::RemoveInput { input } => {
                self.check_unlocked()?;
                self.remove(&input)
            }
            Action::RestoreInput => self.restore_input(),
            Action::SetTriggers { input, triggers } => self.set_triggers(&input, triggers),
            Action::AddCueList { name, cues, source } => {
                check_cues(&name, &cues)?;
                let mut lists = self.show.cue_lists.clone();
                lists.push(ShowCueList {
                    id: InputId::new(),
                    name,
                    cues,
                    source,
                    standby: None,
                });
                check_unique(&lists)?;
                self.show.cue_lists = lists;
                Ok(Vec::new())
            }
            Action::SetCueList {
                list,
                name,
                cues,
                source,
            } => {
                let index = self.find_cue_list(&list)?;
                check_cues(&name, &cues)?;
                let mut lists = self.show.cue_lists.clone();
                let old = std::mem::replace(&mut lists[index].cues, cues);
                lists[index].name = name;
                lists[index].source = source;
                check_unique(&lists)?;
                self.show.cue_lists = lists;
                // Editing during the show keeps standby (on the same cue
                // when it is still there, IMP.5) and running cues.
                let entry = &self.show.cue_lists[index];
                self.cues.keep_standby(entry, &old);
                self.cues
                    .list_changed(&self.show.cue_lists, tokio::time::Instant::now());
                self.sync_standby();
                Ok(Vec::new())
            }
            Action::RemoveCueList { list } => {
                let index = self.find_cue_list(&list)?;
                self.show.cue_lists.remove(index);
                self.cues
                    .list_changed(&self.show.cue_lists, tokio::time::Instant::now());
                Ok(Vec::new())
            }
            Action::CueStandby { list, to } => {
                let index = self.cue_list_or_active(list.as_ref())?;
                self.cues.move_standby(&self.show.cue_lists, index, &to)?;
                self.sync_standby();
                Ok(Vec::new())
            }
            Action::SetPanicTime { ms } => {
                if ms > PANIC_MAX_MS {
                    return Err(bad_cues(format!("the panic time is 0–{PANIC_MAX_MS} ms")));
                }
                self.show.panic_ms = ms;
                Ok(Vec::new())
            }
            Action::SetEmergencyList { list } => {
                let id = match list {
                    Some(list) => Some(self.show.cue_lists[self.find_cue_list(&list)?].id),
                    None => None,
                };
                self.show.emergency_list = id;
                Ok(Vec::new())
            }
            Action::SetZone { zone } => {
                crate::zones::set_zone(&mut self.show.zones, zone)?;
                Ok(Vec::new())
            }
            Action::RemoveZone { name } => {
                if !self.show.zones.iter().any(|z| z.name == name) {
                    return Err(ActionError::BadZone {
                        reason: format!("no zone {name}"),
                    });
                }
                self.show.zones.retain(|z| z.name != name);
                Ok(Vec::new())
            }
            Action::SetZonePatch { patch } => {
                crate::zones::set_patch(&mut self.zone_patches, patch)?;
                self.targets_changed = true;
                Ok(Vec::new())
            }
            Action::RemoveZonePatch { zone } => {
                self.zone_patches.retain(|p| p.zone != zone);
                self.targets_changed = true;
                Ok(Vec::new())
            }
            Action::SetStage { stage } => {
                crate::stages::set_stage(&mut self.show.stages, stage)?;
                Ok(Vec::new())
            }
            Action::RemoveStage { name } => {
                if !self.show.stages.iter().any(|s| s.name == name) {
                    return Err(ActionError::BadStage {
                        reason: format!("no stage {name}"),
                    });
                }
                self.show.stages.retain(|s| s.name != name);
                Ok(Vec::new())
            }
            Action::SetStagePatch { patch } => {
                crate::stages::set_patch(&mut self.stage_patches, patch)?;
                self.targets_changed = true;
                Ok(Vec::new())
            }
            Action::RemoveStagePatch { stage } => {
                self.stage_patches.retain(|p| p.stage != stage);
                self.targets_changed = true;
                Ok(Vec::new())
            }
            Action::ShowPicture { id, picture } => Ok(vec![Command::ShowPicture {
                id,
                picture: crate::playback::engine_picture(&picture, &self.show.stages)?,
            }]),
            Action::FadePicture {
                id,
                opacity,
                duration_ms,
                then,
            } => {
                crate::playback::check_share(opacity)?;
                crate::playback::check_fade_time(duration_ms)?;
                Ok(vec![Command::FadePicture {
                    id,
                    opacity,
                    duration_ms,
                    then,
                }])
            }
            Action::StopPicture { id, fade_ms } => {
                crate::playback::check_fade_time(fade_ms)?;
                Ok(vec![Command::StopPicture { id, fade_ms }])
            }
            Action::SetStageLevel {
                stage,
                brightness,
                duration_ms,
            } => {
                let stage = crate::playback::stage_index(&self.show.stages, &stage)?;
                crate::playback::check_share(brightness)?;
                crate::playback::check_fade_time(duration_ms)?;
                Ok(vec![Command::SetStageLevel {
                    stage,
                    brightness,
                    duration_ms,
                }])
            }
            Action::PlaySound { id, sound } => Ok(vec![Command::PlaySound {
                id,
                sound: crate::playback::engine_sound(&sound, &self.show.zones)?,
            }]),
            Action::FadeSound {
                id,
                gain_db,
                duration_ms,
                then,
            } => {
                crate::playback::check_level(gain_db)?;
                crate::playback::check_fade_time(duration_ms)?;
                Ok(vec![Command::FadeSound {
                    id,
                    gain_db,
                    duration_ms,
                    then,
                }])
            }
            Action::StopSound { id, fade_ms } => {
                crate::playback::check_fade_time(fade_ms)?;
                Ok(vec![Command::StopSound { id, fade_ms }])
            }
            Action::SetZoneLevel {
                zone,
                gain_db,
                mute,
                duration_ms,
            } => {
                let zone = crate::playback::zone_index(&self.show.zones, &zone)?;
                if let Some(db) = gain_db {
                    crate::playback::check_level(db)?;
                }
                crate::playback::check_fade_time(duration_ms)?;
                Ok(vec![Command::SetZoneLevel {
                    zone,
                    gain_db,
                    mute,
                    duration_ms,
                }])
            }
            Action::ResetSettings => {
                self.reset_settings();
                Ok(Vec::new())
            }
            // Files: through the service.
            Action::ExportSettings { .. } | Action::ImportSettings { .. } => {
                Err(ActionError::ShowFile {
                    message: "run through the service".into(),
                })
            }
            Action::SetScriptFolder { folder } => {
                if let Some(folder) = &folder {
                    if !std::path::Path::new(folder).is_dir() {
                        return Err(bad_cues(format!("no folder {folder}")));
                    }
                }
                self.show.script_folder = folder;
                Ok(Vec::new())
            }
            Action::SetActiveCueList { list } => {
                let index = self.find_cue_list(&list)?;
                self.cues.set_active(self.show.cue_lists[index].id);
                Ok(Vec::new())
            }
            Action::SetTarget { target } => {
                // New settings: unknown until checked or sent to again.
                self.unreached.remove(target.name.trim());
                crate::targets::set(&mut self.targets, target)?;
                self.targets_changed = true;
                Ok(Vec::new())
            }
            Action::SetFeed { feed } => {
                crate::targets::set_feed(&mut self.feeds, feed)?;
                self.targets_changed = true;
                Ok(Vec::new())
            }
            Action::RemoveFeed { name } => {
                if !self.feeds.iter().any(|f| f.name == name) {
                    return Err(ActionError::BadTarget {
                        reason: format!("no subscriber {name}"),
                    });
                }
                self.feeds.retain(|f| f.name != name);
                self.targets_changed = true;
                Ok(Vec::new())
            }
            Action::RemoveTarget { name } => {
                crate::targets::find(&self.targets, &name)?;
                self.targets.retain(|t| t.name != name);
                self.unreached.remove(&name);
                self.targets_changed = true;
                Ok(Vec::new())
            }
            // They wait on the network: through the service.
            Action::TestTarget { .. }
            | Action::CheckTargets
            | Action::HttpRequest { .. }
            | Action::HaService { .. } => Err(ActionError::BadTarget {
                reason: "run through the service".into(),
            }),
            Action::SetGoGuard { ms } => {
                if ms > GO_GUARD_MAX_MS {
                    return Err(bad_cues(format!("the GO guard is 0–{GO_GUARD_MAX_MS} ms")));
                }
                self.show.go_guard_ms = ms;
                Ok(Vec::new())
            }
            Action::SetGoRearm { rearm } => {
                self.show.go_rearm = rearm;
                self.cues.release_all();
                Ok(Vec::new())
            }
            // They start cues in time: through the service (cue_go and
            // the like below).
            Action::CueGo { .. }
            | Action::CueStart { .. }
            | Action::CueStop { .. }
            | Action::CueReset
            | Action::CuePanic { .. }
            | Action::CueHardStop => Err(bad_cues("run through the service".into())),
            Action::SetLock { on } => {
                self.locked = on;
                Ok(Vec::new())
            }
            Action::SetMidiTbar { midi } => {
                if let Some(m) = &midi {
                    let bad = |reason: &str| ActionError::BadMidi {
                        reason: reason.into(),
                    };
                    if m.kind != MidiKind::Cc || !m.is_valid() {
                        return Err(bad("the T-bar needs a controller (cc 0–127, channel 1–16)"));
                    }
                    if self
                        .show
                        .shortcuts
                        .iter()
                        .any(|s| s.midi.is_some_and(|b| b.overlaps(m)))
                    {
                        return Err(bad("a shortcut has that controller"));
                    }
                }
                self.show.midi_tbar = midi;
                Ok(Vec::new())
            }
            Action::ReplaceSource { input, source } => self.replace_source(&input, source),
            Action::RenameInput { input, name } => self.rename(&input, name),
            Action::SetInputColor { input, color } => self.set_color(&input, color),
            Action::SetAudio { input, on } => self.set_audio(&input, |a| a.on = on),
            Action::SetAudioFollow { input, follow } => {
                self.set_audio(&input, |a| a.follow_video = follow)
            }
            Action::SetVolume { input, volume } => self.set_volume(&input, volume),
            Action::SetChannelStrip { input, strip } => self.set_strip(&input, strip),
            Action::SetLook { input, look } => self.set_look(&input, look),
            Action::PageCall { input, script } => Ok(vec![Command::PageScript {
                slot: self.page_slot(&input)?,
                script,
            }]),
            Action::SetPageRunning { input, running } => Ok(vec![Command::PageRunning {
                slot: self.page_slot(&input)?,
                running,
            }]),
            Action::SetAuxSend { input, bus, level } => self.set_aux_send(&input, bus, level),
            Action::SetAuxBus { bus, volume, mute } => {
                self.set_aux_bus(bus, volume, mute, AUDIO_RAMP_MS)
            }
            Action::SetMasterFormat { format } => {
                if !format.is_valid() {
                    return Err(ActionError::BadFormat {
                        min: lmix_types::output::FORMAT_MIN_PX,
                        max: lmix_types::output::FORMAT_MAX_PX,
                    });
                }
                if format == self.show.format {
                    return Ok(Vec::new());
                }
                self.show.format = format;
                let mut commands = self.mixer_rebuilt();
                commands.extend(self.page_commands());
                Ok(commands)
            }
            Action::SetSampleRate { hz } => {
                if !lmix_types::output::SAMPLE_RATES.contains(&hz) {
                    return Err(ActionError::BadSampleRate { hz });
                }
                if hz == self.show.sample_rate {
                    return Ok(Vec::new());
                }
                self.show.sample_rate = hz;
                Ok(self.mixer_rebuilt())
            }
            Action::SetMultiview { layout } => {
                if layout == self.show.multiview {
                    return Ok(Vec::new());
                }
                self.show.multiview = layout;
                Ok(self.mixer_rebuilt())
            }
            Action::SetMonitorVolume { volume } => {
                check_volume(volume)?;
                self.show.monitor_volume = volume;
                Ok(vec![Command::SetMonitor {
                    gain: fader_gain(volume),
                }])
            }
            Action::SetDefaultDelay { ms } => {
                if ms > ChannelStrip::DELAY_MAX_MS {
                    return Err(ActionError::BadStrip {
                        field: "delay_ms".into(),
                    });
                }
                self.show.default_delay_ms = ms;
                Ok(Vec::new())
            }
            Action::SetMaster { volume, mute } => {
                check_volume(volume)?;
                self.show.master = AuxBus { volume, mute };
                Ok(vec![self.master_command(AUDIO_RAMP_MS)])
            }
            Action::FadeVolume {
                target,
                volume,
                duration_ms,
            } => self.fade_volume(&target, volume, duration_ms),
            Action::SetListen { input, listen } => self.set_listen(&input, listen),
            Action::TransportInput { input, transport } => self.transport(&input, transport),
            Action::SeekInput { input, position_ms } => {
                let slot = self.transport_slot(&input)?;
                Ok(vec![Command::Seek { slot, position_ms }])
            }
            Action::PlayItem { input, item } => {
                let index = self.find(&input)?;
                let entry = &self.show.inputs[index];
                let SourceSpec::Playlist { paths } = &entry.source else {
                    return Err(ActionError::NoTransport {
                        reference: entry.name.clone(),
                    });
                };
                if item == 0 || item as usize > paths.len() {
                    return Err(ActionError::BadPosition {
                        number: item,
                        max: paths.len() as u32,
                    });
                }
                Ok(vec![Command::PlayItem {
                    slot: entry.slot,
                    index: item as usize - 1,
                }])
            }
            Action::SetSignalLoss { input, mode } => self.set_signal_loss(&input, mode),
            Action::SetAutoPlay { input, auto_play } => {
                let index = self.find(&input)?;
                let entry = &mut self.show.inputs[index];
                if !entry.source.has_transport() && !auto_play.is_off() {
                    return Err(ActionError::NoTransport {
                        reference: entry.name.clone(),
                    });
                }
                entry.auto_play = auto_play;
                Ok(Vec::new())
            }
            Action::SetAtEnd { input, at_end } => {
                let index = self.find(&input)?;
                let entry = &mut self.show.inputs[index];
                if !entry.source.has_transport() {
                    return Err(ActionError::NoTransport {
                        reference: entry.name.clone(),
                    });
                }
                entry.at_end = at_end;
                Ok(at_end_command(entry).into_iter().collect())
            }
            Action::SetInputPress { input, click, go } => {
                let index = self.find(&input)?;
                let entry = &mut self.show.inputs[index];
                entry.click = click.unwrap_or(entry.click);
                entry.go = go.unwrap_or(entry.go);
                Ok(Vec::new())
            }
            Action::AddOutput {
                spec,
                source,
                name,
                aux,
            } => self.add_output(spec, source, name, aux),
            Action::RemoveOutput { output } => {
                self.check_unlocked()?;
                self.remove_output(&output)
            }
            Action::StartOutput { output } => self.start_output(&output),
            Action::StopOutput { output } => {
                self.check_unlocked()?;
                self.stop_output(&output)
            }
            Action::StartOutputs { group } => self.start_outputs(group),
            Action::StopOutputs { group } => {
                self.check_unlocked()?;
                Ok(self.stop_outputs(group))
            }
            Action::ClearListen => Ok(self.clear_listen()),
            Action::MoveInput { input, number } => self.move_input(&input, number),
            Action::PreviewInput { input } => self.preview(&input),
            Action::Cut => self.take(Effect::Cut, 0),
            Action::CutDirect { input } => self.cut_direct(&input),
            Action::AddLayout {
                name,
                background,
                boxes,
            } => self.add_layout(name, background, boxes),
            Action::SetLayout {
                layout,
                background,
                boxes,
            } => self.set_layout(&layout, background, boxes),
            Action::ShowBox {
                layout,
                number,
                shown,
            } => self.show_box(&layout, number, shown),
            Action::RemoveLayout { layout } => self.remove_layout(&layout),
            Action::PreviewLayout { layout } => self.preview_layout(&layout),
            Action::SaveLayoutTemplate { layout, name } => {
                self.save_layout_template(&layout, name)?;
                Ok(Vec::new())
            }
            Action::LoadLayoutTemplate { template, name } => {
                self.load_layout_template(&template, name)
            }
            Action::RemoveLayoutTemplate { template } => {
                let index = self.find_layout_template(&template)?;
                self.layout_templates.remove(index);
                self.layout_templates_changed = true;
                Ok(Vec::new())
            }
            Action::AddShortcut { shortcut } => self.add_shortcut(shortcut),
            Action::SetShortcut { target, shortcut } => self.set_shortcut(&target, shortcut),
            Action::RemoveShortcut { target } => {
                match self.find_shortcut(&target)? {
                    Scope::Show(i) => {
                        self.show.shortcuts.remove(i);
                    }
                    Scope::AllShows(i) => {
                        self.all_shows.remove(i);
                        self.all_shows_changed = true;
                    }
                }
                Ok(Vec::new())
            }
            // The service runs a shortcut's actions one by one.
            Action::RunShortcut { .. } => Err(ActionError::BadShortcut {
                reason: "run through the service".into(),
            }),
            Action::SetPtz { input, ptz } => {
                if let Some(spec) = &ptz {
                    let port = spec.address.rsplit_once(':').map(|(_, p)| p.parse::<u16>());
                    if !matches!(port, Some(Ok(p)) if p > 0) {
                        return Err(ActionError::BadPtz {
                            reason: format!("\"{}\" is no host:port", spec.address),
                        });
                    }
                }
                let index = self.find(&input)?;
                self.show.inputs[index].ptz = ptz;
                Ok(Vec::new())
            }
            // The service sends these to the camera (`ptz_of`).
            Action::PtzMove { .. }
            | Action::PtzZoom { .. }
            | Action::PtzFocus { .. }
            | Action::PtzHome { .. }
            | Action::PtzPreset { .. } => Err(ActionError::BadPtz {
                reason: "sent by the service".into(),
            }),
            Action::Transition { slot } => self.transition(slot),
            Action::QuickPlay { input } => self.quick_play(input.as_ref()),
            Action::SetQuickPlay {
                effect,
                duration_ms,
            } => {
                if effect == Effect::Stinger {
                    return Err(ActionError::BadQuickPlay {
                        reason: "a stinger needs a transition button".into(),
                    });
                }
                self.show.quick_play = QuickPlay {
                    effect,
                    duration_ms,
                };
                Ok(Vec::new())
            }
            // `apply` turns these into what the input's setting names.
            Action::ClickInput { .. } | Action::GoInput { .. } => Ok(Vec::new()),
            Action::SetTransition {
                slot,
                effect,
                duration_ms,
                stinger,
            } => self.set_transition(slot, effect, duration_ms, stinger),
            Action::FadeToBlack { duration_ms } => self.toggle_ftb(duration_ms),
            Action::TBar { position } => self.t_bar(position),
            Action::OverlayIn {
                channel,
                input,
                duration_ms,
            } => self.overlay_in(channel, &input, duration_ms),
            Action::OverlayOut {
                channel,
                duration_ms,
            } => self.overlay_out(channel, duration_ms),
            Action::OverlayOutAll { duration_ms } => Ok(self.overlay_out_all(duration_ms)),
            Action::OverlayLast {
                channel,
                duration_ms,
            } => self.overlay_last(channel, duration_ms),
            Action::OverlayPreview { channel, input } => {
                self.overlay_preview(channel, input.as_ref())
            }
            Action::SetOverlay {
                channel,
                effect,
                duration_ms,
                auto_off_ms,
                rect,
            } => self.set_overlay(
                channel,
                OverlayChannel {
                    effect,
                    duration_ms,
                    auto_off_ms,
                    rect,
                },
            ),
            // The service reads and writes these files.
            Action::NewShow { .. }
            | Action::SaveShow { .. }
            | Action::OpenShow { .. }
            | Action::AppendShow { .. }
            | Action::RelocateMedia { .. }
            | Action::ImportDatasets { .. }
            | Action::ExportDatasets { .. }
            | Action::ExportPackage { .. }
            | Action::ImportPackage { .. }
            | Action::ExportLayoutTemplates { .. }
            | Action::ImportLayoutTemplates { .. } => Ok(Vec::new()),
        }
    }

    /// Swap in a whole show (new/open): empty all old slots, feed the new.
    /// Not while a stinger holds the program.
    pub fn replace(
        &mut self,
        show: Show,
        path: Option<String>,
    ) -> Result<Vec<Command>, ActionError> {
        self.check_no_stinger()?;
        let mut commands: Vec<Command> = self
            .show
            .inputs
            .iter()
            .map(|i| Command::SetSlot {
                slot: i.slot,
                source: None,
            })
            .collect();

        commands.extend(self.show.layouts.iter().map(|l| Command::SetLayout {
            pip: l.pip,
            background: None,
            boxes: Vec::new(),
        }));
        commands.extend(self.clear_listen());
        commands.extend(self.stop_all_outputs());
        self.show = show;
        self.older_format = None;
        // Cues of the old show stop without a word; nothing starts, and
        // standby is where the show kept it (SAF.6).
        self.cues = Runner::default();
        self.cues.restore_standby(&self.show.cue_lists);
        self.distinct_shortcut_ids();
        self.path = path;
        // Closed inputs belong to the old show.
        self.removed.clear();
        self.status.clear();
        self.titles.reset();
        self.dynamic_inputs = Default::default();
        self.overlay_last = Default::default();
        commands.extend(self.restore_commands());
        Ok(commands)
    }

    /// A new mixer for a new format or multiview, set up again; outputs
    /// restart. The sources keep running (no clip starts over).
    fn mixer_rebuilt(&mut self) -> Vec<Command> {
        let before = self.playback_sent.clone();
        let mut commands = self.restore_commands();
        commands.retain(|c| !matches!(c, Command::SetSlot { .. }));
        // Nor do sounds and pictures, unless the stages' rate changed:
        // their flow is not the mixer's.
        if self.playback_sent == before {
            commands
                .retain(|c| !matches!(c, Command::SetPlayback { .. } | Command::SetPreload { .. }));
        }
        commands
    }

    /// Web and graphics pages built again, at the new program size; they
    /// render at the size of their slot flow.
    fn page_commands(&self) -> Vec<Command> {
        self.show
            .inputs
            .iter()
            .filter(|i| {
                matches!(
                    i.source,
                    SourceSpec::Web { .. } | SourceSpec::Graphics { .. }
                )
            })
            .map(|i| Command::SetSlot {
                slot: i.slot,
                source: Some(i.source.clone()),
            })
            .collect()
    }

    /// Commands that rebuild the engine from the current show (start-up,
    /// engine restart): slots, program, preview, then every audio channel.
    pub fn restore_commands(&mut self) -> Vec<Command> {
        // A new or restarted mixer runs no stinger; no end will come.
        self.stinger_finished();
        // First: a new format builds a new mixer, the rest sets it up.
        let mut commands = vec![Command::SetFormat {
            format: self.show.format,
            multiview: self.show.multiview,
            sample_rate: self.show.sample_rate,
        }];
        commands.extend(self.picture_commands());
        self.mix.clear();
        commands.extend(self.audio_commands(AUDIO_RAMP_MS));
        // A reused mixer may still hold solos from before.
        commands.extend(self.listen_commands());
        commands.extend(self.running_output_commands());
        commands.extend(self.show.inputs.iter().map(|i| Command::SetSignalLoss {
            slot: i.slot,
            mode: i.signal_loss,
        }));
        commands.extend(self.overlay_commands());
        commands.extend(self.stinger_pause_commands());
        self.stinger_sent = None;
        commands.extend(self.stinger_commands());
        commands.extend(self.look_commands());
        for input in self.show.inputs.iter().filter(|i| i.source.has_audio()) {
            commands.push(Command::SetStrip {
                slot: input.slot,
                strip: input.strip,
            });
            commands.extend(send_commands(input));
        }
        commands.extend(self.aux_bus_commands());
        self.playback_sent = None;
        commands.extend(self.playback_commands());
        commands
    }

    /// Zones and stages for the engine, when they changed since it last
    /// got them, and what it holds ready on them.
    fn playback_commands(&mut self) -> Vec<Command> {
        let playback = Playback {
            zones: crate::zones::states(&self.show.zones, &self.zone_patches),
            stages: crate::stages::states(&self.show.stages, &self.stage_patches),
            framerate: self.show.format.framerate,
        };
        let mut commands = Vec::new();
        if self.playback_sent.as_ref() != Some(&playback) {
            self.playback_sent = Some(playback.clone());
            commands.push(Command::SetPlayback { playback });
            // A new flow holds nothing ready.
            self.preload_sent = None;
        }
        commands.extend(self.preload_commands());
        commands
    }

    /// What the engine should hold ready, when it changed: the standby
    /// cue of every list, if it plays a sound or shows a picture, so GO
    /// starts it at once (SND.12).
    pub fn preload_commands(&mut self) -> Option<Command> {
        let mut preload = Preload::default();
        for list in &self.show.cue_lists {
            let Some(cue) = list.cues.get(self.cues.standby(list.id)) else {
                continue;
            };
            let id = crate::cues::media_id(cue.id);
            match (&cue.sound, &cue.picture) {
                (Some(sound), _) if cue.kind == CueKind::Sound => {
                    if let Ok(sound) = crate::playback::engine_sound(sound, &self.show.zones) {
                        preload.sounds.push((id, sound));
                    }
                }
                (_, Some(picture)) if cue.kind == CueKind::Picture => {
                    if let Ok(picture) = crate::playback::engine_picture(picture, &self.show.stages)
                    {
                        preload.pictures.push((id, picture));
                    }
                }
                _ => {}
            }
        }
        if self.preload_sent.as_ref() == Some(&preload) {
            return None;
        }
        self.preload_sent = Some(preload.clone());
        Some(Command::SetPreload { preload })
    }

    fn aux_bus_commands(&self) -> Vec<Command> {
        self.show
            .aux_buses
            .iter()
            .enumerate()
            .map(|(bus, master)| Command::SetAuxBus {
                bus,
                gain: fader_gain(master.volume),
                mute: master.mute,
                ramp_ms: AUDIO_RAMP_MS,
            })
            .chain(std::iter::once(self.master_command(AUDIO_RAMP_MS)))
            .chain(std::iter::once(Command::SetMonitor {
                gain: fader_gain(self.show.monitor_volume),
            }))
            .collect()
    }

    fn master_command(&self, ramp_ms: u32) -> Command {
        Command::SetMaster {
            gain: fader_gain(self.show.master.volume),
            mute: self.show.master.mute,
            ramp_ms,
        }
    }

    /// Every overlay channel as the engine should have it, placed, without
    /// fade.
    fn overlay_commands(&self) -> Vec<Command> {
        let place = self
            .show
            .overlay_channels
            .iter()
            .enumerate()
            .map(|(channel, settings)| Command::OverlayPlace {
                channel,
                rect: settings.rect,
            });
        let shown = self
            .show
            .overlays
            .iter()
            .enumerate()
            .map(|(channel, input)| Command::Overlay {
                channel,
                slot: self.slot_of(*input),
                duration_ms: 0,
            });
        place.chain(shown).collect()
    }

    /// Solo state of every input with sound, as the engine should have it.
    fn listen_commands(&self) -> Vec<Command> {
        self.show
            .inputs
            .iter()
            .filter(|i| i.source.has_audio())
            .map(|i| Command::SetListen {
                slot: i.slot,
                listen: self.listen_of(i.id),
            })
            .collect()
    }

    fn listen_of(&self, id: InputId) -> Listen {
        self.listen.get(&id).copied().unwrap_or_default()
    }

    fn picture_commands(&self) -> Vec<Command> {
        let mut commands: Vec<Command> = self
            .show
            .inputs
            .iter()
            .map(|i| Command::SetSlot {
                slot: i.slot,
                source: Some(i.source.clone()),
            })
            .collect();
        commands.extend(self.show.inputs.iter().filter_map(held_end_command));

        // Compositions are lost with the engine; set them before use.
        commands.extend(self.show.layouts.iter().map(|l| self.layout_command(l)));
        let program = self.bus_of(self.show.program);
        commands.extend(bus_commands(program, None, Effect::Cut, 0));
        if program.is_none() {
            commands.push(Command::Program {
                slot: None,
                effect: Effect::Cut,
                duration_ms: 0,
            });
        }
        commands.extend(bus_commands(
            None,
            self.bus_of(self.show.preview),
            Effect::Cut,
            0,
        ));
        commands
    }

    pub fn state(&self) -> ShowState {
        let inputs = self
            .show
            .inputs
            .iter()
            .enumerate()
            .map(|(i, input)| Input {
                id: input.id,
                number: i as u32 + 1,
                name: input.name.clone(),
                source: input.source.clone(),
                slot: input.slot,
                status: self.status.get(&input.id).copied().unwrap_or_default(),
                color: input.color,
                audio: input.audio,
                has_audio: input.source.has_audio(),
                listen: self.listen_of(input.id),
                signal_loss: input.signal_loss,
                auto_play: input.auto_play,
                at_end: input.at_end,
                click: input.click,
                go: input.go,
                strip: input.strip,
                sends: input.sends,
                look: input.look,
                triggers: input.triggers.clone(),
                ingest: self.ingest_of(&input.source),
                clip_ms: input
                    .source
                    .has_transport()
                    .then(|| self.clip_ms.get(&input.slot).copied())
                    .flatten(),
                playback: None,
                ptz: input.ptz.clone(),
                missing: self.missing.get(&input.id).cloned().unwrap_or_default(),
            })
            .collect();

        ShowState {
            api_version: API_VERSION,
            path: self.path.clone(),
            inputs,
            preview: self.show.preview,
            program: self.show.program,
            transitions: self.show.transitions.clone(),
            quick_play: self.show.quick_play,
            fade_to_black: self.fade_to_black,
            slots: self.slots,
            outputs: self.outputs_state(),
            overlays: self.show.overlays,
            overlay_channels: self.show.overlay_channels,
            overlay_preview: self.show.overlay_preview,
            overlay_last: self
                .overlay_last
                .map(|id| id.filter(|id| self.slot_of(Some(*id)).is_some())),
            t_bar: self.t_bar,
            aux_buses: self.show.aux_buses,
            master: self.show.master,
            format: self.show.format,
            multiview: self.show.multiview,
            sample_rate: self.show.sample_rate,
            monitor_volume: self.show.monitor_volume,
            default_delay_ms: self.show.default_delay_ms,
            looks_available: self.looks_available,
            locked: self.locked,
            cue_lists: self
                .show
                .cue_lists
                .iter()
                .enumerate()
                .map(|(i, l)| CueList {
                    id: l.id,
                    number: i as u32 + 1,
                    name: l.name.clone(),
                    cues: l.cues.clone(),
                    standby: self.cues.standby(l.id) as u32 + 1,
                    active: self.cues.active(&self.show.cue_lists) == Some(i),
                    source: l.source.clone(),
                    emergency: self.show.emergency_list == Some(l.id),
                })
                .collect(),
            running_cues: self
                .cues
                .running(&self.show.cue_lists, tokio::time::Instant::now()),
            go_guard_ms: self.show.go_guard_ms,
            go_rearm: self.show.go_rearm,
            panic_ms: self.show.panic_ms,
            script_folder: self.show.script_folder.clone(),
            older_format: self.older_format,
            cue_warnings: self.cue_warnings.clone(),
            targets: self.targets.iter().map(|t| t.public()).collect(),
            feeds: self.feeds.clone(),
            zones: crate::zones::states(&self.show.zones, &self.zone_patches),
            stages: crate::stages::states(&self.show.stages, &self.stage_patches),
            restorable: self.removed.last().map(|(_, input)| input.name.clone()),
            shortcuts: self
                .show
                .shortcuts
                .iter()
                .map(|s| (s, false))
                .chain(self.all_shows.iter().map(|s| (s, true)))
                .enumerate()
                .map(|(i, (s, all_shows))| Shortcut {
                    id: s.id,
                    number: i as u32 + 1,
                    label: s.label.clone(),
                    key: s.key.clone(),
                    web: s.web,
                    color: s.color,
                    midi: s.midi,
                    actions: s.actions.clone(),
                    all_shows,
                    key_shadowed: all_shows && self.key_shadowed(s),
                    midi_shadowed: all_shows && self.midi_shadowed(s),
                })
                .collect(),
            midi_tbar: self.show.midi_tbar,
            layout_templates: self.layout_templates.clone(),
            layouts: self
                .show
                .layouts
                .iter()
                .enumerate()
                .map(|(i, l)| Layout {
                    id: l.id,
                    number: i as u32 + 1,
                    name: l.name.clone(),
                    background: l.background,
                    boxes: l.boxes.clone(),
                })
                .collect(),
            dynamic_inputs: self.dynamic_inputs,
            dynamic_values: self.dynamic_values.clone(),
            graphics: self.graphics_state(),
            datasets: self.show.datasets.clone(),
            data_sources: self.data_sources_state(),
        }
    }

    /// Tally per input: 0 off, 1 program, 2 preview (API.5).
    pub fn tally(&self) -> Vec<u8> {
        self.show
            .inputs
            .iter()
            .map(|i| match Some(i.id) {
                // An overlay or a layout's input is on air like program.
                id if id == self.show.program
                    || self.show.overlays.contains(&id)
                    || self.in_layout(self.show.program, i.id) =>
                {
                    1
                }
                // So is one prepared on an overlay with the next take (OVL.3).
                id if id == self.show.preview
                    || self.in_layout(self.show.preview, i.id)
                    || self.show.overlay_preview.contains(&id) =>
                {
                    2
                }
                _ => 0,
            })
            .collect()
    }

    // ---- Inputs ----

    fn add(
        &mut self,
        source: SourceSpec,
        name: Option<String>,
    ) -> Result<Vec<Command>, ActionError> {
        check_source(&source)?;
        self.check_graphics_source(&source)?;
        let slot = self
            .free_slot()
            .ok_or(ActionError::SlotsFull { slots: self.slots })?;
        let id = InputId::new();
        let name = name.unwrap_or_else(|| source.default_name());
        // A new input's sound comes the show's default delay late (AUD.6).
        let strip = ChannelStrip {
            delay_ms: self.show.default_delay_ms,
            ..ChannelStrip::default()
        };

        self.show.inputs.push(ShowInput {
            id,
            name,
            source: source.clone(),
            slot,
            color: None,
            audio: Default::default(),
            signal_loss: SignalLoss::default(),
            auto_play: AutoPlay::default(),
            at_end: AtEnd::default(),
            click: InputPress::click(),
            go: InputPress::go(),
            strip,
            sends: [0; AUX_BUSES],
            look: Look::default(),
            triggers: Vec::new(),
            ptz: None,
        });
        self.status.insert(id, InputStatus::Starting);

        // The mixer input may still carry the previous occupant's setting.
        let mut commands = vec![
            Command::SetSlot {
                slot,
                source: Some(source.clone()),
            },
            Command::SetSignalLoss {
                slot,
                mode: SignalLoss::default(),
            },
        ];
        if self.looks_available && source.has_video() {
            commands.push(Command::SetLook {
                slot,
                look: Look::default(),
            });
        }
        if source.has_audio() {
            commands.push(Command::SetStrip { slot, strip });
            commands.extend((0..AUX_BUSES).map(|bus| Command::SetAuxSend {
                slot,
                bus,
                gain: 0.0,
            }));
        }
        Ok(commands)
    }

    fn remove(&mut self, input: &InputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        // Program goes black with a cut, which a stinger holds off.
        if self.show.program == Some(self.show.inputs[index].id) {
            self.check_no_stinger()?;
        }
        let removed = self.show.inputs.remove(index);
        self.removed.push((index, removed.clone()));
        if self.removed.len() > REMOVED_KEPT {
            self.removed.remove(0);
        }
        let mut commands = Vec::new();

        // A removed input leaves preview empty and program black.
        if self.show.preview == Some(removed.id) {
            self.show.preview = None;
        }
        if self.show.program == Some(removed.id) {
            self.show.program = None;
            commands.push(Command::Program {
                slot: None,
                effect: Effect::Cut,
                duration_ms: 0,
            });
        }
        self.status.remove(&removed.id);
        // Layouts lose the input and are set anew.
        let mut changed = Vec::new();
        for (index, layout) in self.show.layouts.iter_mut().enumerate() {
            let before = (layout.background, layout.boxes.len());
            if layout.background == Some(removed.id) {
                layout.background = None;
            }
            layout.boxes.retain(|b| b.input != removed.id);
            if before != (layout.background, layout.boxes.len()) {
                changed.push(index);
            }
        }
        for index in changed {
            commands.push(self.layout_command(&self.show.layouts[index]));
        }
        // A button whose stinger clip goes becomes a cut.
        for button in &mut self.show.transitions {
            if button.stinger.is_some_and(|s| s.input == removed.id) {
                button.effect = Effect::Cut;
                button.stinger = None;
            }
        }
        for armed in &mut self.show.overlay_preview {
            if *armed == Some(removed.id) {
                *armed = None;
            }
        }
        for (channel, shown) in self.show.overlays.iter_mut().enumerate() {
            if *shown == Some(removed.id) {
                *shown = None;
                commands.push(Command::Overlay {
                    channel,
                    slot: None,
                    duration_ms: 0,
                });
            }
        }
        // ISO outputs of the input end with it.
        for output in self
            .show
            .outputs
            .iter()
            .filter(|o| o.source == OutputSource::Input(removed.id))
        {
            if self
                .output_status
                .get(&output.id)
                .is_some_and(|s| matches!(s, OutputStatus::Running { .. }))
            {
                commands.push(Command::StopOutput { id: output.id });
            }
        }
        let fed: Vec<OutputId> = self
            .show
            .outputs
            .iter()
            .filter(|o| o.source == OutputSource::Input(removed.id))
            .map(|o| o.id)
            .collect();
        for id in fed {
            self.output_status.insert(
                id,
                OutputStatus::Failed {
                    message: format!("input {} was removed", removed.name),
                },
            );
        }
        // A solo left on an empty channel would keep the monitor on silence.
        if self
            .listen
            .remove(&removed.id)
            .is_some_and(|l| l != Listen::Off)
        {
            commands.push(Command::SetListen {
                slot: removed.slot,
                listen: Listen::Off,
            });
        }

        commands.push(Command::SetSlot {
            slot: removed.slot,
            source: None,
        });
        Ok(commands)
    }

    fn replace_source(
        &mut self,
        input: &InputRef,
        source: SourceSpec,
    ) -> Result<Vec<Command>, ActionError> {
        check_source(&source)?;
        self.check_graphics_source(&source)?;
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        if !source.has_video() && [self.show.preview, self.show.program].contains(&Some(entry.id)) {
            return Err(ActionError::NoVideo {
                reference: entry.name.clone(),
            });
        }
        entry.source = source.clone();
        self.status.insert(entry.id, InputStatus::Starting);

        let mut commands = vec![Command::SetSlot {
            slot: entry.slot,
            source: Some(source.clone()),
        }];
        commands.extend(held_end_command(entry));
        // After a source without sound the channel may hold anything.
        if source.has_audio() {
            commands.push(Command::SetStrip {
                slot: entry.slot,
                strip: entry.strip,
            });
            commands.extend(send_commands(entry));
        }
        Ok(commands)
    }

    fn rename(&mut self, input: &InputRef, name: String) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        self.show.inputs[index].name = name;
        Ok(Vec::new())
    }

    fn set_color(
        &mut self,
        input: &InputRef,
        color: Option<InputColor>,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        self.show.inputs[index].color = color;
        Ok(Vec::new())
    }

    fn move_input(&mut self, input: &InputRef, number: u32) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let max = self.show.inputs.len() as u32;
        if number == 0 || number > max {
            return Err(ActionError::BadPosition { number, max });
        }

        let entry = self.show.inputs.remove(index);
        self.show.inputs.insert(number as usize - 1, entry);
        Ok(Vec::new())
    }

    // ---- Switching ----

    fn preview(&mut self, input: &InputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &self.show.inputs[index];
        no_video_check(entry)?;
        if self.show.program == Some(entry.id) {
            return Err(ActionError::OnProgram {
                reference: entry.name.clone(),
            });
        }

        self.show.preview = Some(entry.id);
        Ok(vec![Command::Preview { slot: entry.slot }])
    }

    /// Preview to program; the old program goes to preview (classic swap).
    /// Overlays prepared for it (OVL.3) go on with the same effect and
    /// length.
    fn take(&mut self, effect: Effect, duration_ms: u32) -> Result<Vec<Command>, ActionError> {
        let mut commands = self.swap_buses(effect, duration_ms)?;
        let fade_ms = if effect == Effect::Cut {
            0
        } else {
            duration_ms
        };
        for index in 0..usize::from(OVERLAY_CHANNELS) {
            let Some(id) = self.show.overlay_preview[index].take() else {
                continue;
            };
            if self.show.overlays[index] == Some(id) {
                continue;
            }
            let Some(slot) = self.slot_of(Some(id)) else {
                continue;
            };
            self.put_overlay(index, id);
            commands.push(Command::Overlay {
                channel: index,
                slot: Some(slot),
                duration_ms: fade_ms,
            });
        }
        Ok(commands)
    }

    fn swap_buses(
        &mut self,
        effect: Effect,
        duration_ms: u32,
    ) -> Result<Vec<Command>, ActionError> {
        self.check_no_stinger()?;
        let to = self
            .bus_of(self.show.preview)
            .ok_or(ActionError::NoPreview)?;
        let old_program = self.bus_of(self.show.program);

        std::mem::swap(&mut self.show.preview, &mut self.show.program);
        let (Bus::Slot(to), Some(Bus::Slot(_)) | None) = (to, old_program) else {
            return Ok(bus_commands(Some(to), old_program, effect, duration_ms));
        };
        let old_program = match old_program {
            Some(Bus::Slot(slot)) => Some(slot),
            _ => None,
        };
        Ok(program_then_preview(
            Some(to),
            old_program,
            effect,
            duration_ms,
        ))
    }

    /// Input straight to program. If it was in preview, the old program
    /// takes its place there.
    fn cut_direct(&mut self, input: &InputRef) -> Result<Vec<Command>, ActionError> {
        self.check_no_stinger()?;
        let index = self.find(input)?;
        let entry = &self.show.inputs[index];
        no_video_check(entry)?;
        let (id, to) = (entry.id, entry.slot);

        if self.show.preview != Some(id) {
            self.show.program = Some(id);
            return Ok(program_then_preview(Some(to), None, Effect::Cut, 0));
        }

        let old_program = self.bus_of(self.show.program);
        self.show.preview = self.show.program;
        self.show.program = Some(id);
        Ok(bus_commands(
            Some(Bus::Slot(to)),
            old_program,
            Effect::Cut,
            0,
        ))
    }

    /// T-bar (SWI.3): preview fades in over program as it is pulled; at
    /// the end it is a take, back at the start nothing happened.
    fn t_bar(&mut self, position: f32) -> Result<Vec<Command>, ActionError> {
        if !(0.0..=1.0).contains(&position) {
            return Err(ActionError::BadTBar);
        }
        self.check_no_stinger()?;
        if position >= 1.0 {
            self.t_bar = 0.0;
            return self.take(Effect::Cut, 0);
        }
        let slot = match self.bus_of(self.show.preview) {
            Some(Bus::Slot(slot)) => slot,
            Some(Bus::Layout(_)) => {
                return Err(ActionError::BadLayout {
                    reason: "the T-bar mixes inputs only; take a layout with Cut or a transition"
                        .into(),
                })
            }
            None => return Err(ActionError::NoPreview),
        };
        self.t_bar = position;
        Ok(vec![Command::TBar { slot, position }])
    }

    fn transition(&mut self, slot: TransitionSlot) -> Result<Vec<Command>, ActionError> {
        let button = *self.button(slot)?;
        if button.effect != Effect::Stinger {
            return self.take(button.effect, button.duration_ms);
        }
        let bad = |reason: &str| ActionError::BadStinger {
            reason: reason.into(),
        };
        let stinger = button.stinger.ok_or_else(|| bad("no clip set"))?;
        let (slot, file) = self
            .stinger_clip(stinger.input)
            .ok_or_else(|| bad("its clip input is gone"))?;
        // strom's stinger takes one input to another (DECISIONS 18.10., STG).
        let layout = |id| matches!(self.bus_of(id), Some(Bus::Layout(_)));
        if layout(self.show.program) || layout(self.show.preview) {
            return Err(bad(STINGER_NO_LAYOUTS));
        }
        let Some(Bus::Slot(to)) = self.bus_of(self.show.preview) else {
            return Err(ActionError::NoPreview);
        };

        // The engine plays the clip and switches program at its cut point;
        // overlays, sound and players follow then. A take starts the clip
        // after the engine's preroll.
        //   [Program to, Preview old] → [Stinger to, Preview old, Wait cut, …]
        let mut commands = self.take(Effect::Cut, 0)?;
        let swap = commands
            .iter()
            .take_while(|c| matches!(c, Command::Program { .. } | Command::Preview { .. }))
            .count();
        let clip = LibraryClip {
            file,
            cut_ms: stinger.cut_ms,
        };
        commands[0] = Command::Stinger { to, clip };
        let preroll_ms = crate::engine::flows::STINGER_PREROLL_MS;
        commands.insert(
            swap,
            Command::Wait {
                ms: stinger.cut_ms + preroll_ms,
            },
        );
        // Locked until the engine reports the end. The whole clip plays,
        // its length when the player reported it; a lost report frees
        // the lock after strom's own limit and a margin.
        let length_ms = self
            .clip_ms
            .get(&slot)
            .copied()
            .filter(|ms| *ms > 0)
            .unwrap_or(u64::from(button.duration_ms));
        let after_ms = u64::from(preroll_ms + crate::engine::flows::STINGER_FINISH_LIMIT_MS);
        self.stinger_hold = Duration::from_millis(length_ms + after_ms);
        self.stinger_until = Some(Instant::now() + self.stinger_hold);
        Ok(commands)
    }

    /// Quick play (SWI.8): `input` (preview when none) to program with
    /// the quick play effect, played first if it is a clip. Like
    /// `cut_direct`, preview stays unless the input was in it; on program
    /// already, it only plays.
    fn quick_play(&mut self, input: Option<&InputRef>) -> Result<Vec<Command>, ActionError> {
        self.check_no_stinger()?;
        let QuickPlay {
            effect,
            duration_ms,
        } = self.show.quick_play;
        let index = match input {
            Some(input) => Some(self.find(input)?),
            None => match self.show.preview {
                None => return Err(ActionError::NoPreview),
                // A layout in preview: taken, nothing to play.
                Some(id) => self.show.inputs.iter().position(|i| i.id == id),
            },
        };
        let Some(index) = index else {
            return self.take(effect, duration_ms);
        };
        let entry = &self.show.inputs[index];
        no_video_check(entry)?;
        let (id, slot) = (entry.id, entry.slot);
        // Automatic playback starts it already as it goes on air.
        let plays =
            entry.source.has_transport() && !(entry.auto_play.play && !self.airing().contains(&id));
        let mut commands = Vec::new();
        if plays {
            commands.push(Command::Transport {
                slot,
                transport: Transport::Play,
            });
        }
        if self.show.program == Some(id) {
            return Ok(commands);
        }
        if self.show.preview == Some(id) {
            commands.extend(self.take(effect, duration_ms)?);
        } else {
            self.show.program = Some(id);
            commands.extend(program_then_preview(Some(slot), None, effect, duration_ms));
        }
        Ok(commands)
    }

    /// `click_input` and `go_input` as the action the input's setting
    /// names (INP.8, INP.9); `None` when that is nothing.
    fn pressed(&self, action: Action) -> Result<Option<Action>, ActionError> {
        let (input, press) = match action {
            Action::ClickInput { input } => {
                let press = self.show.inputs[self.find(&input)?].click;
                (input, press)
            }
            Action::GoInput { input } => {
                let press = self.show.inputs[self.find(&input)?].go;
                (input, press)
            }
            action => return Ok(Some(action)),
        };
        Ok(match press {
            InputPress::PreviewInput => Some(Action::PreviewInput { input }),
            InputPress::CutDirect => Some(Action::CutDirect { input }),
            InputPress::QuickPlay => Some(Action::QuickPlay { input: Some(input) }),
            InputPress::None => None,
        })
    }

    // ---- Automation ----

    fn set_triggers(
        &mut self,
        input: &InputRef,
        triggers: Vec<Trigger>,
    ) -> Result<Vec<Command>, ActionError> {
        let bad = |reason: String| Err(ActionError::BadTriggers { reason });
        check_triggers(triggers.iter().map(|t| (t.delay_ms, t.actions.as_slice())))?;
        for trigger in &triggers {
            match (trigger.on, trigger.before_ms) {
                (TriggerOn::ClipEnd, Some(ms)) if ms > CLIP_COUNTDOWN_MAX_MS => {
                    return bad(format!(
                        "a countdown is at most {CLIP_COUNTDOWN_MAX_MS} ms before the end"
                    ));
                }
                (TriggerOn::ClipEnd, _) | (_, None) => {}
                (_, Some(_)) => return bad("only clip_end has a time before the end".into()),
            }
            match (trigger.on, trigger.at_ms) {
                (TriggerOn::PlayTime, None | Some(0)) => {
                    return bad("play_time needs at_ms, the playing time (at least 1 ms)".into());
                }
                (TriggerOn::PlayTime, _) | (_, None) => {}
                (_, Some(_)) => return bad("only play_time has a playing time".into()),
            }
        }
        let index = self.find(input)?;
        let plays = self.show.inputs[index].source.has_transport();
        if !plays
            && triggers
                .iter()
                .any(|t| matches!(t.on, TriggerOn::ClipEnd | TriggerOn::PlayTime))
        {
            return bad("clip_end and play_time need a file, URL or playlist input".into());
        }
        self.show.inputs[index].triggers = triggers;
        Ok(Vec::new())
    }

    /// Note the length of what the player in `slot` plays; true when it
    /// changed by more than a frame or two (clients hear of it).
    pub fn set_clip_length(&mut self, slot: usize, ms: u64) -> bool {
        const SAME_MS: u64 = 50;
        let old = self.clip_ms.insert(slot, ms);
        old.is_none_or(|old| old.abs_diff(ms) > SAME_MS)
    }

    /// `clip_end` triggers of the input in `slot` that `change` fires, as
    /// (delay, actions).
    pub fn clip_triggers(
        &self,
        slot: usize,
        change: &crate::clips::ClipChange,
    ) -> Vec<(u32, Vec<Action>)> {
        let Some(input) = self.show.inputs.iter().find(|i| i.slot == slot) else {
            return Vec::new();
        };
        input
            .triggers
            .iter()
            .filter(|t| match (t.on, t.before_ms, t.at_ms) {
                (TriggerOn::ClipEnd, None, _) => change.ended,
                (TriggerOn::ClipEnd, Some(ms), _) => change.crosses(u64::from(ms)),
                (TriggerOn::PlayTime, _, Some(ms)) => change.reaches(u64::from(ms)),
                _ => false,
            })
            .map(|t| (t.delay_ms, t.actions.clone()))
            .collect()
    }

    /// Triggers that fire for what changed between `before` and now, as
    /// (delay, actions).
    pub fn fired_triggers(&self, before: &OnAir) -> Vec<(u32, Vec<Action>)> {
        let now = self.on_air();
        let mut fired = Vec::new();
        for input in self.show.inputs.iter().filter(|i| !i.triggers.is_empty()) {
            let id = Some(input.id);
            let events = [
                (
                    TriggerOn::Program,
                    now.program == id && before.program != id,
                ),
                (
                    TriggerOn::ProgramOff,
                    before.program == id && now.program != id,
                ),
                (
                    TriggerOn::Preview,
                    now.preview == id && before.preview != id,
                ),
                (
                    TriggerOn::OverlayIn,
                    now.overlays.contains(&id) && !before.overlays.contains(&id),
                ),
                (
                    TriggerOn::OverlayOut,
                    before.overlays.contains(&id) && !now.overlays.contains(&id),
                ),
            ];
            for (on, happened) in events {
                if !happened {
                    continue;
                }
                fired.extend(
                    input
                        .triggers
                        .iter()
                        .filter(|t| t.on == on)
                        .map(|t| (t.delay_ms, t.actions.clone())),
                );
            }
        }
        fired
    }

    /// What is on air, to compare after an action.
    pub fn on_air(&self) -> OnAir {
        OnAir {
            program: self.show.program,
            preview: self.show.preview,
            overlays: self.show.overlays.to_vec(),
            overlay_since: self.overlay_since.to_vec(),
        }
    }

    /// Auto offs (OVL.2) of the channels that went on since `before`, as
    /// (delay, channel 1–8, when it went on).
    pub fn auto_offs(&self, before: &OnAir) -> Vec<(u32, u8, u64)> {
        (0..usize::from(OVERLAY_CHANNELS))
            .filter(|&i| {
                self.show.overlays[i].is_some()
                    && before.overlay_since.get(i) != Some(&self.overlay_since[i])
            })
            .filter_map(|i| {
                let ms = self.show.overlay_channels[i].auto_off_ms?;
                Some((ms, i as u8 + 1, self.overlay_since[i]))
            })
            .collect()
    }

    /// Whether overlay `channel` (1–8) still shows what went on at
    /// `since`: an auto off is due only then.
    pub fn overlay_still_on(&self, channel: u8, since: u64) -> bool {
        overlay_index(channel)
            .is_ok_and(|i| self.show.overlays[i].is_some() && self.overlay_since[i] == since)
    }

    /// GO in `list`, or in the active list (RUN.1, RUN.6, RUN.7); `held`:
    /// the button that gives it, if its release is reported.
    pub fn cue_go(
        &mut self,
        list: Option<&InputRef>,
        now: tokio::time::Instant,
        held: Option<Hold>,
    ) -> Result<Vec<Out>, ActionError> {
        let index = self.cue_list_or_active(list)?;
        let guard = Guard {
            ms: self.show.go_guard_ms,
            rearm: self.show.go_rearm,
            held,
        };
        self.cues.go(&self.show.cue_lists, index, now, guard)
    }

    /// Button `held` was let go: it may give GO again (RUN.6).
    pub fn cue_release(&mut self, held: Hold) {
        self.cues.release(held);
    }

    /// Start cue `cue` (number, id, or `standby`: the list's standby cue)
    /// without moving standby (CTL.1).
    pub fn cue_start(
        &mut self,
        list: Option<&InputRef>,
        cue: &str,
        now: tokio::time::Instant,
    ) -> Result<Vec<Out>, ActionError> {
        if cue == CUE_STANDBY {
            let index = self.cue_list_or_active(list)?;
            let id = self
                .cues
                .standby_cue(&self.show.cue_lists[index])
                .ok_or_else(|| bad_cues("no cues left; move standby back first".into()))?;
            return self
                .cues
                .start(&self.show.cue_lists, Some(index), &id.to_string(), now);
        }
        let index = list.map(|l| self.find_cue_list(l)).transpose()?;
        self.cues.start(&self.show.cue_lists, index, cue, now)
    }

    /// Stop one cue, a list's cues or every cue (RUN.4).
    pub fn cue_stop(
        &mut self,
        list: Option<&InputRef>,
        cue: Option<&str>,
        now: tokio::time::Instant,
    ) -> Result<Vec<Out>, ActionError> {
        let index = list.map(|l| self.find_cue_list(l)).transpose()?;
        self.cues.stop(&self.show.cue_lists, index, cue, now)
    }

    /// Stop every cue at once, every standby to the start (RUN.11).
    pub fn cue_reset(&mut self, now: tokio::time::Instant) -> Vec<Out> {
        self.cues.reset(now)
    }

    /// Panic over the show's panic time or `ms`, or a hard stop when one
    /// runs (SAF.1, CTL.1).
    pub fn cue_panic(
        &mut self,
        now: tokio::time::Instant,
        ms: Option<u32>,
    ) -> Result<Vec<Out>, ActionError> {
        if ms.is_some_and(|ms| ms > PANIC_MAX_MS) {
            return Err(bad_cues(format!("the panic time is 0–{PANIC_MAX_MS} ms")));
        }
        let emergency = self.emergency_index();
        let ms = ms.unwrap_or(self.show.panic_ms);
        Ok(self.cues.panic(&self.show.cue_lists, emergency, now, ms))
    }

    /// Hard stop (SAF.3).
    pub fn cue_hard_stop(&mut self, now: tokio::time::Instant) -> Vec<Out> {
        let emergency = self.emergency_index();
        self.cues.hard_stop(&self.show.cue_lists, emergency, now)
    }

    fn emergency_index(&self) -> Option<usize> {
        let id = self.show.emergency_list?;
        self.show.cue_lists.iter().position(|l| l.id == id)
    }

    /// The engine's sound or picture `id` is over; the cue it belongs to
    /// ends.
    pub fn media_ended(&mut self, id: &str, now: tokio::time::Instant) -> Vec<Out> {
        let Ok(cue) = id.parse::<Uuid>() else {
            return Vec::new();
        };
        self.cues
            .media_ended(&self.show.cue_lists, lmix_types::CueId(cue), now)
    }

    /// What the cue lists have due by `now`.
    pub fn cue_tick(&mut self, now: tokio::time::Instant) -> Vec<Out> {
        self.cues.tick(&self.show.cue_lists, now)
    }

    /// The opened file was of an older version (SHW.7).
    pub fn set_older_format(&mut self, version: Option<u32>) {
        self.older_format = version;
    }

    pub fn older_format(&self) -> Option<u32> {
        self.older_format
    }

    /// Check the cue lists again (SUI.5): after changes of the show, the
    /// targets or the missing media.
    pub fn refresh_warnings(&mut self) {
        let missing = |id| self.missing.get(&id).is_some_and(|m| !m.is_empty());
        let machine = crate::checks::Machine {
            targets: &self.targets,
            unreached: &self.unreached,
            missing: &missing,
        };
        self.cue_warnings = crate::checks::cue_warnings(&self.show, &machine);
    }

    /// How target `name` answered (`Err`: why not); whether that changes
    /// the warnings (SUI.5).
    pub fn set_reached(&mut self, name: &str, result: Result<(), String>) -> bool {
        let changed = match result {
            Ok(()) => self.unreached.remove(name).is_some(),
            Err(reason) => self.unreached.insert(name.to_string(), reason.clone()) != Some(reason),
        };
        if changed {
            self.refresh_warnings();
        }
        changed
    }

    /// The targets the cue lists send to, with their settings (SUI.5).
    pub fn used_targets(&self) -> Vec<crate::targets::TargetConfig> {
        crate::checks::used_targets(&self.show)
            .iter()
            .filter_map(|name| self.targets.iter().find(|t| &t.name == name).cloned())
            .collect()
    }

    /// Write standby into the show's lists; whether one moved (then the
    /// show is saved: a restart goes on there, SAF.6).
    pub fn sync_standby(&mut self) -> bool {
        let mut moved = false;
        for i in 0..self.show.cue_lists.len() {
            let at = self.cues.standby_cue(&self.show.cue_lists[i]);
            let list = &mut self.show.cue_lists[i];
            if list.standby != at {
                list.standby = at;
                moved = true;
            }
        }
        moved
    }

    /// How late cue steps ran (RUN.8).
    pub fn cue_lateness(&self) -> crate::cues::Lateness {
        self.cues.lateness()
    }

    /// When the cue lists have something due next.
    pub fn cue_deadline(&self) -> Option<tokio::time::Instant> {
        self.cues.deadline()
    }

    /// `list`, or the active list without one.
    fn cue_list_or_active(&self, list: Option<&InputRef>) -> Result<usize, ActionError> {
        match list {
            Some(list) => self.find_cue_list(list),
            None => self
                .cues
                .active(&self.show.cue_lists)
                .ok_or_else(|| bad_cues("no cue list".into())),
        }
    }

    /// A cue list by number (1-based), id or name; `active` is the active
    /// list (CTL.1).
    fn find_cue_list(&self, list: &InputRef) -> Result<usize, ActionError> {
        if matches!(list, InputRef::Text(text) if text == LIST_ACTIVE) {
            return self.cue_list_or_active(None);
        }
        crate::cues::find_list(&self.show.cue_lists, list).ok_or_else(|| {
            ActionError::UnknownCueList {
                reference: match list {
                    InputRef::Number(n) => n.to_string(),
                    InputRef::Text(t) => format!("\"{t}\""),
                },
            }
        })
    }

    // ---- Safety ----

    /// Whether the lock holds (closing, loading, stopping refused).
    pub fn check_unlocked(&self) -> Result<(), ActionError> {
        if self.locked {
            return Err(ActionError::Locked);
        }
        Ok(())
    }

    /// The input closed last, back with its settings: in its old slot
    /// when that is free, at its old place in the bar.
    fn restore_input(&mut self) -> Result<Vec<Command>, ActionError> {
        let (index, mut input) = self.removed.pop().ok_or(ActionError::NothingToRestore)?;
        let slot_free = !self.show.inputs.iter().any(|i| i.slot == input.slot);
        if !slot_free {
            let Some(slot) = self.free_slot() else {
                self.removed.push((index, input));
                return Err(ActionError::SlotsFull { slots: self.slots });
            };
            input.slot = slot;
        }
        let mut commands = vec![
            Command::SetSlot {
                slot: input.slot,
                source: Some(input.source.clone()),
            },
            Command::SetSignalLoss {
                slot: input.slot,
                mode: input.signal_loss,
            },
        ];
        commands.extend(held_end_command(&input));
        if input.source.has_audio() {
            commands.push(Command::SetStrip {
                slot: input.slot,
                strip: input.strip,
            });
            commands.extend(send_commands(&input));
        }
        if self.looks_available && input.source.has_video() {
            commands.push(Command::SetLook {
                slot: input.slot,
                look: input.look,
            });
        }
        self.status.insert(input.id, InputStatus::Starting);
        let at = index.min(self.show.inputs.len());
        self.show.inputs.insert(at, input);
        Ok(commands)
    }

    /// Add inputs of `other` (by number there, all if `numbers` is empty)
    /// with their settings, at the end, in free slots (SHW.3). Their
    /// triggers stay behind: their actions name inputs of the other show.
    /// All or none: too few free slots refuses the lot.
    pub fn append(&mut self, other: Show, numbers: &[u32]) -> Result<Vec<Command>, ActionError> {
        let chosen: Vec<ShowInput> = if numbers.is_empty() {
            other.inputs
        } else {
            numbers
                .iter()
                .map(|n| {
                    (*n as usize)
                        .checked_sub(1)
                        .and_then(|i| other.inputs.get(i))
                        .cloned()
                        .ok_or_else(|| ActionError::UnknownInput {
                            reference: n.to_string(),
                        })
                })
                .collect::<Result<_, _>>()?
        };
        let free = (0..self.slots)
            .filter(|s| !self.show.inputs.iter().any(|i| i.slot == *s))
            .count();
        if chosen.len() > free {
            return Err(ActionError::SlotsFull { slots: self.slots });
        }
        let mut commands = Vec::new();
        for mut input in chosen {
            input.id = InputId::new();
            input.slot = self.free_slot().expect("counted above");
            input.triggers.clear();
            // Names pick inputs in actions: a second "Cam 1" becomes "Cam 1 (2)".
            let base = input.name.clone();
            let mut n = 2;
            while self.show.inputs.iter().any(|i| i.name == input.name) {
                input.name = format!("{base} ({n})");
                n += 1;
            }
            // Back in through the same door as a closed input.
            self.removed.push((self.show.inputs.len(), input));
            commands.extend(self.restore_input()?);
        }
        // Their sound as the mix says (follow video, mute), like any change.
        commands.extend(self.audio_commands(AUDIO_RAMP_MS));
        Ok(commands)
    }

    // ---- Shortcuts ----

    fn add_shortcut(&mut self, spec: ShortcutSpec) -> Result<Vec<Command>, ActionError> {
        self.check_shortcut(&spec, None)?;
        self.keep_shortcut(spec, InputId::new());
        Ok(Vec::new())
    }

    /// Puts a checked shortcut at the end of its list.
    fn keep_shortcut(&mut self, spec: ShortcutSpec, id: InputId) {
        let entry = ShowShortcut {
            id,
            label: spec.label,
            key: spec.key,
            web: spec.web,
            color: spec.color,
            midi: spec.midi,
            actions: spec.actions,
        };
        if spec.all_shows {
            self.all_shows.push(entry);
            self.all_shows_changed = true;
        } else {
            self.show.shortcuts.push(entry);
        }
    }

    fn set_shortcut(
        &mut self,
        target: &InputRef,
        spec: ShortcutSpec,
    ) -> Result<Vec<Command>, ActionError> {
        let scope = self.find_shortcut(target)?;
        self.check_shortcut(&spec, Some(scope))?;
        let entry = match scope {
            Scope::Show(i) if !spec.all_shows => &mut self.show.shortcuts[i],
            Scope::AllShows(i) if spec.all_shows => {
                self.all_shows_changed = true;
                &mut self.all_shows[i]
            }
            // Moved between the show and the settings, at the end. A new
            // id: an older copy of the show (file, package) may still hold
            // the old one.
            Scope::Show(i) => {
                self.show.shortcuts.remove(i);
                self.keep_shortcut(spec, InputId::new());
                return Ok(Vec::new());
            }
            Scope::AllShows(i) => {
                self.all_shows.remove(i);
                self.all_shows_changed = true;
                self.keep_shortcut(spec, InputId::new());
                return Ok(Vec::new());
            }
        };
        entry.label = spec.label;
        entry.key = spec.key;
        entry.web = spec.web;
        entry.color = spec.color;
        entry.midi = spec.midi;
        entry.actions = spec.actions;
        Ok(Vec::new())
    }

    /// A shortcut for all shows whose key one of the show's has: the
    /// show's wins.
    fn key_shadowed(&self, shortcut: &ShowShortcut) -> bool {
        shortcut.key.is_some() && self.show.shortcuts.iter().any(|s| s.key == shortcut.key)
    }

    /// The same for its MIDI note or controller, also against the T-bar.
    fn midi_shadowed(&self, shortcut: &ShowShortcut) -> bool {
        shortcut.midi.is_some_and(|m| {
            self.show
                .shortcuts
                .iter()
                .any(|s| s.midi.is_some_and(|o| o.overlaps(&m)))
                || self.show.midi_tbar.is_some_and(|t| t.overlaps(&m))
        })
    }

    /// A label, a key and MIDI note free among the shortcuts it is kept
    /// with (a show's may take those of one for all shows), and actions
    /// that do not manage or run shortcuts themselves (no loops).
    fn check_shortcut(&self, spec: &ShortcutSpec, own: Option<Scope>) -> Result<(), ActionError> {
        let bad = |reason: String| Err(ActionError::BadShortcut { reason });
        if spec.label.trim().is_empty() {
            return bad("it needs a label".into());
        }
        if spec.actions.is_empty() || spec.actions.len() > SHORTCUT_ACTIONS {
            return bad(format!("1–{SHORTCUT_ACTIONS} actions"));
        }
        if spec.actions.iter().any(changes_shortcuts) {
            return bad("a shortcut cannot run or change shortcuts".into());
        }
        // The others it is kept with.
        let (list, own) = if spec.all_shows {
            let own = match own {
                Some(Scope::AllShows(i)) => Some(i),
                _ => None,
            };
            (&self.all_shows, own)
        } else {
            let own = match own {
                Some(Scope::Show(i)) => Some(i),
                _ => None,
            };
            (&self.show.shortcuts, own)
        };
        let key_taken = spec.key.as_ref().is_some_and(|key| {
            list.iter()
                .enumerate()
                .any(|(i, s)| Some(i) != own && s.key.as_ref() == Some(key))
        });
        if key_taken {
            return bad("another shortcut has that key".into());
        }
        if let Some(midi) = &spec.midi {
            if !midi.is_valid() {
                return bad("MIDI channel 1–16, number 0–127".into());
            }
            let taken = list
                .iter()
                .enumerate()
                .any(|(i, s)| Some(i) != own && s.midi.is_some_and(|m| m.overlaps(midi)));
            // A show's T-bar only shadows a shortcut for all shows.
            let tbar = !spec.all_shows && self.show.midi_tbar.is_some_and(|m| m.overlaps(midi));
            if taken || tbar {
                return bad(
                    "another shortcut or the T-bar has that MIDI note or controller".into(),
                );
            }
        }
        Ok(())
    }

    /// The shortcut a MIDI message runs.
    pub fn midi_shortcut(&self, message: &MidiMessage) -> Option<InputId> {
        let hit = |s: &&ShowShortcut| s.midi.is_some_and(|m| m.matches(message));
        // The show's first, then those for all shows it does not shadow.
        self.show
            .shortcuts
            .iter()
            .find(hit)
            .or_else(|| {
                self.all_shows
                    .iter()
                    .filter(|s| !self.midi_shadowed(s))
                    .find(hit)
            })
            .map(|s| s.id)
    }

    /// Whether a MIDI message moves the T-bar.
    pub fn midi_moves_tbar(&self, message: &MidiMessage) -> bool {
        self.show.midi_tbar.is_some_and(|m| m.matches(message))
    }

    /// Actions of a shortcut, for the service to run.
    pub fn shortcut_actions(&self, target: &InputRef) -> Result<Vec<Action>, ActionError> {
        Ok(match self.find_shortcut(target)? {
            Scope::Show(i) => &self.show.shortcuts[i],
            Scope::AllShows(i) => &self.all_shows[i],
        }
        .actions
        .clone())
    }

    /// A shortcut by number (1-based, the show's first), id or label (the
    /// show's first).
    fn find_shortcut(&self, target: &InputRef) -> Result<Scope, ActionError> {
        let local = self.show.shortcuts.len();
        let all: Vec<&ShowShortcut> = self.show.shortcuts.iter().chain(&self.all_shows).collect();
        let index = match target {
            InputRef::Number(n) => (*n as usize).checked_sub(1).filter(|i| *i < all.len()),
            InputRef::Text(text) => all
                .iter()
                .position(|s| s.id.to_string() == *text)
                .or_else(|| all.iter().position(|s| s.label == *text)),
        };
        index
            .map(|i| {
                if i < local {
                    Scope::Show(i)
                } else {
                    Scope::AllShows(i - local)
                }
            })
            .ok_or_else(|| ActionError::UnknownShortcut {
                reference: match target {
                    InputRef::Number(n) => n.to_string(),
                    InputRef::Text(t) => format!("\"{t}\""),
                },
            })
    }

    // ---- Layouts ----

    fn add_layout(
        &mut self,
        name: Option<String>,
        background: Option<InputRef>,
        boxes: Vec<BoxSpec>,
    ) -> Result<Vec<Command>, ActionError> {
        let pip = (0..LAYOUTS)
            .find(|p| !self.show.layouts.iter().any(|l| l.pip == *p))
            .ok_or(ActionError::LayoutsFull { max: LAYOUTS })?;
        let (background, boxes) = self.layout_parts(background.as_ref(), &boxes)?;
        let layout = ShowLayout {
            id: InputId::new(),
            name: name.unwrap_or_else(|| format!("Layout {}", self.show.layouts.len() + 1)),
            pip,
            background,
            boxes,
        };
        let command = self.layout_command(&layout);
        self.show.layouts.push(layout);
        Ok(vec![command])
    }

    /// A layout on air changes there at once (the engine morphs it).
    fn set_layout(
        &mut self,
        layout: &InputRef,
        background: Option<InputRef>,
        boxes: Vec<BoxSpec>,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find_layout(layout)?;
        let (background, boxes) = self.layout_parts(background.as_ref(), &boxes)?;
        let entry = &mut self.show.layouts[index];
        entry.background = background;
        entry.boxes = boxes;
        Ok(vec![self.layout_command(&self.show.layouts[index])])
    }

    /// LAY.2: a box out of the picture and back, as a layout change.
    fn show_box(
        &mut self,
        layout: &InputRef,
        number: u8,
        shown: BoxShown,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find_layout(layout)?;
        let entry = &mut self.show.layouts[index];
        let count = entry.boxes.len();
        let found = usize::from(number)
            .checked_sub(1)
            .and_then(|at| entry.boxes.get_mut(at))
            .ok_or_else(|| ActionError::BadLayout {
                reason: format!("no box {number}; the layout has {count}"),
            })?;
        found.hidden = match shown {
            BoxShown::Shown => false,
            BoxShown::Hidden => true,
            BoxShown::Toggle => !found.hidden,
        };
        Ok(vec![self.layout_command(&self.show.layouts[index])])
    }

    fn remove_layout(&mut self, layout: &InputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find_layout(layout)?;
        let id = self.show.layouts[index].id;
        if self.show.program == Some(id) {
            return Err(ActionError::BadLayout {
                reason: "it is on program; take something else first".into(),
            });
        }
        if self.show.preview == Some(id) {
            self.show.preview = None;
        }
        let removed = self.show.layouts.remove(index);
        Ok(vec![Command::SetLayout {
            pip: removed.pip,
            background: None,
            boxes: Vec::new(),
        }])
    }

    fn preview_layout(&mut self, layout: &InputRef) -> Result<Vec<Command>, ActionError> {
        let entry = &self.show.layouts[self.find_layout(layout)?];
        if self.show.program == Some(entry.id) {
            return Err(ActionError::OnProgram {
                reference: entry.name.clone(),
            });
        }
        self.show.preview = Some(entry.id);
        Ok(vec![Command::PreviewLayout { pip: entry.pip }])
    }

    /// Background and boxes of a layout from an action: inputs with a
    /// picture, each at most once, boxes inside the picture.
    fn layout_parts(
        &self,
        background: Option<&InputRef>,
        boxes: &[BoxSpec],
    ) -> Result<(Option<InputId>, Vec<LayoutBox>), ActionError> {
        let bad = |reason: String| ActionError::BadLayout { reason };
        if boxes.len() > LAYOUT_BOXES {
            return Err(bad(format!("at most {LAYOUT_BOXES} boxes")));
        }
        let picture = |input: &InputRef| -> Result<InputId, ActionError> {
            let entry = &self.show.inputs[self.find(input)?];
            no_video_check(entry)?;
            Ok(entry.id)
        };
        let background = background.map(picture).transpose()?;
        let mut used: Vec<InputId> = background.into_iter().collect();
        let mut parts = Vec::with_capacity(boxes.len());
        for spec in boxes {
            let input = picture(&spec.input)?;
            if used.contains(&input) {
                return Err(bad("an input can be in a layout only once".into()));
            }
            if !spec.rect.is_valid() {
                return Err(bad("a box lies outside the picture".into()));
            }
            if spec.border_px > BORDER_MAX_PX {
                return Err(bad(format!("frames are at most {BORDER_MAX_PX} px")));
            }
            if !spec.crop.is_valid() {
                return Err(bad("a crop must leave some of the input visible".into()));
            }
            used.push(input);
            parts.push(LayoutBox {
                rect: spec.rect,
                input,
                border_px: spec.border_px,
                crop: spec.crop,
                locked: spec.locked,
                hidden: spec.hidden,
            });
        }
        Ok((background, parts))
    }

    /// The engine composition of a layout, its inputs by slot.
    fn layout_command(&self, layout: &ShowLayout) -> Command {
        Command::SetLayout {
            pip: layout.pip,
            background: self.slot_of(layout.background),
            boxes: layout
                .boxes
                .iter()
                .filter(|b| !b.hidden)
                .filter_map(|b| {
                    Some(EngineBox {
                        rect: b.rect,
                        slot: self.slot_of(Some(b.input))?,
                        border_px: b.border_px,
                        crop: b.crop,
                    })
                })
                .collect(),
        }
    }

    /// A layout by number (1-based), id or name.
    fn find_layout(&self, layout: &InputRef) -> Result<usize, ActionError> {
        let index = match layout {
            InputRef::Number(n) => (*n as usize)
                .checked_sub(1)
                .filter(|i| *i < self.show.layouts.len()),
            InputRef::Text(text) => self
                .show
                .layouts
                .iter()
                .position(|l| l.id.to_string() == *text)
                .or_else(|| self.show.layouts.iter().position(|l| l.name == *text)),
        };
        index.ok_or_else(|| ActionError::UnknownLayout {
            reference: match layout {
                InputRef::Number(n) => n.to_string(),
                InputRef::Text(t) => format!("\"{t}\""),
            },
        })
    }

    // ---- Layout templates (LAY.5) ----

    /// A template by name.
    fn find_layout_template(&self, name: &str) -> Result<usize, ActionError> {
        self.layout_templates
            .iter()
            .position(|t| t.name == name)
            .ok_or_else(|| ActionError::UnknownLayoutTemplate { name: name.into() })
    }

    /// An input as a template keeps it.
    fn template_input(&self, id: InputId) -> Option<TemplateInput> {
        let index = self.show.inputs.iter().position(|i| i.id == id)?;
        Some(TemplateInput {
            number: index as u32 + 1,
            name: self.show.inputs[index].name.clone(),
        })
    }

    /// Keep layout `layout` as template `name` (its own by default).
    fn save_layout_template(
        &mut self,
        layout: &InputRef,
        name: Option<String>,
    ) -> Result<(), ActionError> {
        let entry = &self.show.layouts[self.find_layout(layout)?];
        let name = name
            .unwrap_or_else(|| entry.name.clone())
            .trim()
            .to_string();
        let template = LayoutTemplate {
            name,
            background: entry.background.and_then(|id| self.template_input(id)),
            boxes: entry
                .boxes
                .iter()
                .filter_map(|b| {
                    Some(TemplateBox {
                        rect: b.rect,
                        input: self.template_input(b.input)?,
                        border_px: b.border_px,
                        crop: b.crop,
                        locked: b.locked,
                    })
                })
                .collect(),
        };
        self.keep_layout_templates(vec![template])
    }

    /// Add or replace (by name) templates; all or none.
    fn keep_layout_templates(&mut self, templates: Vec<LayoutTemplate>) -> Result<(), ActionError> {
        let bad = |reason: String| ActionError::BadLayout { reason };
        for t in &templates {
            if t.name.trim().is_empty() {
                return Err(bad("a template needs a name".into()));
            }
            if t.boxes.len() > LAYOUT_BOXES {
                return Err(bad(format!("at most {LAYOUT_BOXES} boxes")));
            }
            if t.boxes
                .iter()
                .any(|b| !b.rect.is_valid() || b.border_px > BORDER_MAX_PX || !b.crop.is_valid())
            {
                return Err(bad(format!("template {}: a box is out of range", t.name)));
            }
        }
        let mut kept = self.layout_templates.clone();
        for template in templates {
            match kept.iter_mut().find(|t| t.name == template.name) {
                Some(old) => *old = template,
                None => kept.push(template),
            }
        }
        if kept.len() > LAYOUT_TEMPLATES {
            return Err(bad(format!("at most {LAYOUT_TEMPLATES} templates")));
        }
        self.layout_templates = kept;
        self.layout_templates_changed = true;
        Ok(())
    }

    /// Read templates from a file (LAY.5); ones of the same name are
    /// replaced.
    pub fn import_layout_templates(
        &mut self,
        templates: Vec<LayoutTemplate>,
    ) -> Result<(), ActionError> {
        self.keep_layout_templates(templates)
    }

    /// Templates by name for a file, all for none.
    pub fn layout_templates_named(
        &self,
        names: &[String],
    ) -> Result<Vec<LayoutTemplate>, ActionError> {
        if names.is_empty() {
            return Ok(self.layout_templates.clone());
        }
        names
            .iter()
            .map(|n| Ok(self.layout_templates[self.find_layout_template(n)?].clone()))
            .collect()
    }

    /// A new layout from a template. Each place takes the input of the
    /// name it had; one without takes the input at its number, if no other
    /// place has it, else the next input with a picture left over.
    fn load_layout_template(
        &mut self,
        template: &str,
        name: Option<String>,
    ) -> Result<Vec<Command>, ActionError> {
        let template = self.layout_templates[self.find_layout_template(template)?].clone();
        let wanted: Vec<&TemplateInput> = template
            .background
            .iter()
            .chain(template.boxes.iter().map(|b| &b.input))
            .collect();
        let pictures: Vec<InputId> = self
            .show
            .inputs
            .iter()
            .filter(|i| i.source.has_video())
            .map(|i| i.id)
            .collect();
        let by_name = |t: &TemplateInput| {
            self.show
                .inputs
                .iter()
                .find(|i| i.name == t.name && pictures.contains(&i.id))
                .map(|i| i.id)
        };
        let mut chosen: Vec<Option<InputId>> = wanted.iter().map(|t| by_name(t)).collect();
        for (place, t) in wanted.iter().enumerate() {
            if chosen[place].is_some() {
                continue;
            }
            let at_number = (t.number as usize)
                .checked_sub(1)
                .and_then(|i| self.show.inputs.get(i))
                .map(|i| i.id)
                .filter(|id| pictures.contains(id) && !chosen.contains(&Some(*id)));
            chosen[place] = at_number.or_else(|| {
                pictures
                    .iter()
                    .copied()
                    .find(|id| !chosen.contains(&Some(*id)))
            });
        }
        let ids = chosen
            .into_iter()
            .collect::<Option<Vec<InputId>>>()
            .ok_or_else(|| ActionError::BadLayout {
                reason: format!(
                    "template {} needs {} inputs with a picture",
                    template.name,
                    wanted.len()
                ),
            })?;
        let reference = |id: InputId| InputRef::Text(id.to_string());
        let mut ids = ids.into_iter();
        let background = template
            .background
            .as_ref()
            .and_then(|_| ids.next())
            .map(reference);
        let boxes = template
            .boxes
            .iter()
            .zip(ids)
            .map(|(b, id)| BoxSpec {
                rect: b.rect,
                input: reference(id),
                border_px: b.border_px,
                crop: b.crop,
                locked: b.locked,
                hidden: false,
            })
            .collect();
        self.add_layout(Some(name.unwrap_or(template.name)), background, boxes)
    }

    /// Slot of an input or composition of a layout.
    fn bus_of(&self, id: Option<InputId>) -> Option<Bus> {
        let id = id?;
        if let Some(slot) = self.slot_of(Some(id)) {
            return Some(Bus::Slot(slot));
        }
        self.show
            .layouts
            .iter()
            .find(|l| l.id == id)
            .map(|l| Bus::Layout(l.pip))
    }

    /// Whether layout `layout` (if it is one) shows `input`.
    fn in_layout(&self, layout: Option<InputId>, input: InputId) -> bool {
        self.show
            .layouts
            .iter()
            .find(|l| Some(l.id) == layout)
            .is_some_and(|l| {
                l.background == Some(input) || l.boxes.iter().any(|b| b.input == input && !b.hidden)
            })
    }

    /// The engine has finished the stinger (or it failed, or the mixer
    /// it ran on is gone): switching is free again.
    pub fn stinger_finished(&mut self) {
        self.stinger_until = None;
    }

    /// The engine started its take `generation` for the lock; the limit
    /// counts from now (the engine may have taken a while to start it).
    pub fn stinger_started(&mut self, generation: u64) {
        self.stinger_take = generation;
        if self.stinger_until.is_some() {
            self.stinger_until = Some(Instant::now() + self.stinger_hold);
        }
    }

    /// The engine's take `generation` ended: frees the lock if it is
    /// that take's, not after a newer take started.
    pub fn stinger_ended(&mut self, generation: u64) {
        if generation == self.stinger_take {
            self.stinger_finished();
        }
    }

    fn check_no_stinger(&self) -> Result<(), ActionError> {
        match self.stinger_until {
            Some(until) if Instant::now() < until => Err(ActionError::StingerPlaying),
            _ => Ok(()),
        }
    }

    fn set_transition(
        &mut self,
        slot: TransitionSlot,
        effect: Effect,
        duration_ms: u32,
        stinger: Option<StingerSpec>,
    ) -> Result<Vec<Command>, ActionError> {
        self.button(slot)?;
        let mut commands = Vec::new();
        let stinger = match (effect, stinger) {
            (Effect::Stinger, Some(spec)) => {
                let (stinger, clip) = self.stinger_of(&spec, duration_ms)?;
                self.check_cut_point(slot, stinger)?;
                // Paused until the button is pressed; it would loop unseen.
                commands.push(Command::Transport {
                    slot: clip,
                    transport: Transport::Pause,
                });
                Some(stinger)
            }
            (Effect::Stinger, None) => {
                return Err(ActionError::BadStinger {
                    reason: "choose a clip input".into(),
                })
            }
            _ => None,
        };
        let button = &mut self.show.transitions[slot.0 as usize - 1];
        button.effect = effect;
        button.duration_ms = duration_ms;
        button.stinger = stinger;
        Ok(commands)
    }

    /// A stinger clip must be a file, URL or playlist with a picture; the
    /// cut point lies within the button's duration.
    fn stinger_of(
        &self,
        spec: &StingerSpec,
        duration_ms: u32,
    ) -> Result<(Stinger, usize), ActionError> {
        let entry = &self.show.inputs[self.find(&spec.input)?];
        no_video_check(entry)?;
        if !entry.source.has_transport() {
            return Err(ActionError::BadStinger {
                reason: format!("{} is not a clip (file, URL or playlist)", entry.name),
            });
        }
        if spec.cut_ms > duration_ms {
            return Err(ActionError::BadStinger {
                reason: "the cut point lies after the end".into(),
            });
        }
        let stinger = Stinger {
            input: entry.id,
            cut_ms: spec.cut_ms,
        };
        Ok((stinger, entry.slot))
    }

    /// strom keeps one cut point per clip file: another button with the
    /// same clip must not ask for another one.
    fn check_cut_point(&self, slot: TransitionSlot, stinger: Stinger) -> Result<(), ActionError> {
        let file = self.stinger_clip(stinger.input).map(|(_, f)| f);
        let other = self.show.transitions.iter().find(|t| {
            t.slot != slot
                && t.stinger.is_some_and(|s| {
                    s.cut_ms != stinger.cut_ms && self.stinger_clip(s.input).map(|(_, f)| f) == file
                })
        });
        match other {
            Some(t) => Err(ActionError::BadStinger {
                reason: format!(
                    "button {} plays this clip with another cut point; a clip has one",
                    t.slot.0
                ),
            }),
            None => Ok(()),
        }
    }

    /// Slot and file of a stinger's clip input; a playlist plays its
    /// first item.
    fn stinger_clip(&self, input: InputId) -> Option<(usize, String)> {
        let entry = self.show.inputs.iter().find(|i| i.id == input)?;
        let file = crate::engine::flows::player_items(&entry.source)?
            .into_iter()
            .next()?;
        Some((entry.slot, file))
    }

    /// The stinger buttons' clips, once per file (the first button's cut
    /// point), in button order.
    fn stinger_library(&self) -> Vec<LibraryClip> {
        let mut clips: Vec<LibraryClip> = Vec::new();
        for stinger in self.show.transitions.iter().filter_map(|t| t.stinger) {
            let Some((_, file)) = self.stinger_clip(stinger.input) else {
                continue;
            };
            if clips.iter().any(|c| c.file == file) {
                continue;
            }
            clips.push(LibraryClip {
                file,
                cut_ms: stinger.cut_ms,
            });
        }
        clips
    }

    /// The stinger library for the engine, when it changed since it last
    /// got it (a button, its clip input or that input's source).
    fn stinger_commands(&mut self) -> Vec<Command> {
        let clips = self.stinger_library();
        if self.stinger_sent.as_ref() == Some(&clips) {
            return Vec::new();
        }
        self.stinger_sent = Some(clips.clone());
        vec![Command::StingerLibrary { clips }]
    }

    /// Clips of stinger buttons, paused (start-up, restore).
    fn stinger_pause_commands(&self) -> Vec<Command> {
        self.show
            .transitions
            .iter()
            .filter_map(|t| self.slot_of(Some(t.stinger?.input)))
            .map(|slot| Command::Transport {
                slot,
                transport: Transport::Pause,
            })
            .collect()
    }

    // ---- Audio ----

    fn set_audio(
        &mut self,
        input: &InputRef,
        change: impl FnOnce(&mut lmix_types::InputAudio),
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        change(&mut self.show.inputs[index].audio);
        Ok(Vec::new())
    }

    fn set_volume(&mut self, input: &InputRef, volume: u8) -> Result<Vec<Command>, ActionError> {
        check_volume(volume)?;
        self.set_audio(input, |a| a.volume = volume)
    }

    fn set_strip(
        &mut self,
        input: &InputRef,
        strip: ChannelStrip,
    ) -> Result<Vec<Command>, ActionError> {
        if let Some(field) = strip.out_of_range() {
            return Err(ActionError::BadStrip {
                field: field.into(),
            });
        }
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        if !entry.source.has_audio() {
            return Err(ActionError::NoAudio {
                reference: entry.name.clone(),
            });
        }
        entry.strip = strip;
        Ok(vec![Command::SetStrip {
            slot: entry.slot,
            strip,
        }])
    }

    fn set_aux_send(
        &mut self,
        input: &InputRef,
        bus: u8,
        level: u8,
    ) -> Result<Vec<Command>, ActionError> {
        let bus = aux_index(bus)?;
        check_volume(level)?;
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        if !entry.source.has_audio() {
            return Err(ActionError::NoAudio {
                reference: entry.name.clone(),
            });
        }
        entry.sends[bus] = level;
        Ok(vec![Command::SetAuxSend {
            slot: entry.slot,
            bus,
            gain: fader_gain(level),
        }])
    }

    fn set_aux_bus(
        &mut self,
        bus: u8,
        volume: u8,
        mute: bool,
        ramp_ms: u32,
    ) -> Result<Vec<Command>, ActionError> {
        let bus = aux_index(bus)?;
        check_volume(volume)?;
        self.show.aux_buses[bus] = AuxBus { volume, mute };
        Ok(vec![Command::SetAuxBus {
            bus,
            gain: fader_gain(volume),
            mute,
            ramp_ms,
        }])
    }

    /// A level fade (AUD.14): the engine ramps from where the level is.
    /// An input's fader goes out with the other channels (`audio_ramp`
    /// gives them the fade's time).
    fn fade_volume(
        &mut self,
        target: &VolumeTarget,
        volume: u8,
        duration_ms: u32,
    ) -> Result<Vec<Command>, ActionError> {
        if duration_ms > FADE_MAX_MS {
            return Err(ActionError::BadFade {
                duration_ms,
                max: FADE_MAX_MS,
            });
        }
        let ramp_ms = duration_ms.max(AUDIO_RAMP_MS);
        match target {
            VolumeTarget::Input(input) => self.set_volume(input, volume),
            VolumeTarget::Bus(bus) => {
                let mute = self.show.aux_buses[aux_index(*bus)?].mute;
                self.set_aux_bus(*bus, volume, mute, ramp_ms)
            }
            VolumeTarget::Master => {
                check_volume(volume)?;
                self.show.master.volume = volume;
                Ok(vec![self.master_command(ramp_ms)])
            }
        }
    }

    /// The engine says whether it can apply looks.
    pub fn set_looks_available(&mut self, available: bool) {
        self.looks_available = available;
    }

    /// The slot of a web or graphics input: one that shows a page.
    fn page_slot(&self, input: &InputRef) -> Result<usize, ActionError> {
        let entry = &self.show.inputs[self.find(input)?];
        match entry.source {
            SourceSpec::Web { .. } | SourceSpec::Graphics { .. } => Ok(entry.slot),
            _ => Err(ActionError::NoPage {
                reference: entry.name.clone(),
            }),
        }
    }

    fn set_look(&mut self, input: &InputRef, look: Look) -> Result<Vec<Command>, ActionError> {
        if let Some(field) = look.out_of_range() {
            return Err(ActionError::BadLook {
                field: field.into(),
            });
        }
        if !self.looks_available && !look.is_neutral() {
            return Err(ActionError::NoLooks);
        }
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        no_video_check(entry)?;
        entry.look = look;
        if !self.looks_available {
            return Ok(Vec::new());
        }
        Ok(vec![Command::SetLook {
            slot: entry.slot,
            look,
        }])
    }

    /// Every picture's look, for an engine that can apply them; a slot
    /// may still carry the look of a former input.
    fn look_commands(&self) -> Vec<Command> {
        if !self.looks_available {
            return Vec::new();
        }
        self.show
            .inputs
            .iter()
            .filter(|i| i.source.has_video())
            .map(|i| Command::SetLook {
                slot: i.slot,
                look: i.look,
            })
            .collect()
    }

    fn set_signal_loss(
        &mut self,
        input: &InputRef,
        mode: SignalLoss,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        entry.signal_loss = mode;
        Ok(vec![Command::SetSignalLoss {
            slot: entry.slot,
            mode,
        }])
    }

    // ---- Overlays ----

    /// Length of an overlay going on or off: `given` (an action's own
    /// fade), else the channel's effect (none for a cut).
    fn overlay_ms(&self, index: usize, given: Option<u32>) -> u32 {
        given.unwrap_or_else(|| self.show.overlay_channels[index].fade_ms())
    }

    /// Input `id` on channel `index`, remembered as its last; counted for
    /// the auto off.
    fn put_overlay(&mut self, index: usize, id: InputId) {
        self.show.overlays[index] = Some(id);
        self.overlay_last[index] = Some(id);
        self.overlays_shown += 1;
        self.overlay_since[index] = self.overlays_shown;
    }

    fn overlay_in(
        &mut self,
        channel: u8,
        input: &InputRef,
        duration_ms: Option<u32>,
    ) -> Result<Vec<Command>, ActionError> {
        let index = overlay_index(channel)?;
        let entry = &self.show.inputs[self.find(input)?];
        no_video_check(entry)?;
        let (id, slot) = (entry.id, entry.slot);
        self.put_overlay(index, id);
        Ok(vec![Command::Overlay {
            channel: index,
            slot: Some(slot),
            duration_ms: self.overlay_ms(index, duration_ms),
        }])
    }

    fn overlay_out(
        &mut self,
        channel: u8,
        duration_ms: Option<u32>,
    ) -> Result<Vec<Command>, ActionError> {
        let index = overlay_index(channel)?;
        if self.show.overlays[index].take().is_none() {
            return Ok(Vec::new());
        }
        Ok(vec![Command::Overlay {
            channel: index,
            slot: None,
            duration_ms: self.overlay_ms(index, duration_ms),
        }])
    }

    /// Every channel that shows something off, each like `overlay_out`.
    fn overlay_out_all(&mut self, duration_ms: Option<u32>) -> Vec<Command> {
        let shown: Vec<usize> = (0..usize::from(OVERLAY_CHANNELS))
            .filter(|&i| self.show.overlays[i].take().is_some())
            .collect();
        shown
            .into_iter()
            .map(|channel| Command::Overlay {
                channel,
                slot: None,
                duration_ms: self.overlay_ms(channel, duration_ms),
            })
            .collect()
    }

    /// The input `channel` showed last back on it, if it is empty.
    fn overlay_last(
        &mut self,
        channel: u8,
        duration_ms: Option<u32>,
    ) -> Result<Vec<Command>, ActionError> {
        let index = overlay_index(channel)?;
        if self.show.overlays[index].is_some() {
            return Ok(Vec::new());
        }
        let last = self.overlay_last[index]
            .filter(|id| self.slot_of(Some(*id)).is_some())
            .ok_or(ActionError::NoOverlayLast { channel })?;
        self.overlay_in(channel, &InputRef::Text(last.to_string()), duration_ms)
    }

    /// Prepare `input` on `channel` for the next take (OVL.3), or clear it.
    fn overlay_preview(
        &mut self,
        channel: u8,
        input: Option<&InputRef>,
    ) -> Result<Vec<Command>, ActionError> {
        let index = overlay_index(channel)?;
        let id = match input {
            Some(input) => {
                let entry = &self.show.inputs[self.find(input)?];
                no_video_check(entry)?;
                Some(entry.id)
            }
            None => None,
        };
        self.show.overlay_preview[index] = id;
        Ok(Vec::new())
    }

    /// Effect, length, auto off and place of a channel (OVL.2); a channel
    /// on air moves to its new place at once.
    fn set_overlay(
        &mut self,
        channel: u8,
        settings: OverlayChannel,
    ) -> Result<Vec<Command>, ActionError> {
        let index = overlay_index(channel)?;
        let bad = |reason: String| ActionError::BadOverlaySettings { channel, reason };
        if !matches!(settings.effect, Effect::Cut | Effect::Fade) {
            return Err(bad("the effect is cut or fade".into()));
        }
        if settings
            .auto_off_ms
            .is_some_and(|ms| ms == 0 || ms > AUTO_OFF_MAX_MS)
        {
            return Err(bad(format!(
                "auto off is 1–{AUTO_OFF_MAX_MS} ms after it went on"
            )));
        }
        if settings.rect.is_some_and(|r| !r.is_valid()) {
            return Err(bad("its place lies outside the picture".into()));
        }
        let moved = self.show.overlay_channels[index].rect != settings.rect;
        self.show.overlay_channels[index] = settings;
        Ok(moved
            .then_some(Command::OverlayPlace {
                channel: index,
                rect: settings.rect,
            })
            .into_iter()
            .collect())
    }

    // ---- Automatic playback (INP.11) ----

    /// Inputs on air for automatic playback: what tally calls program,
    /// and preview while the T-bar mixes it in.
    fn airing(&self) -> Vec<InputId> {
        let mut airing: Vec<InputId> = self
            .show
            .inputs
            .iter()
            .zip(self.tally())
            .filter(|(_, tally)| *tally == 1)
            .map(|(input, _)| input.id)
            .collect();
        if self.t_bar > 0.0 {
            airing.extend(self.show.preview);
        }
        airing
    }

    /// How long the old picture is still seen after `action`; automatic
    /// pauses wait for it. A stinger's cut is in its commands.
    fn leave_ms(&self, action: &Action) -> u32 {
        match action {
            Action::Transition { slot } => self
                .button(*slot)
                .ok()
                .filter(|b| b.effect != Effect::Cut)
                .map_or(0, |b| b.duration_ms),
            Action::QuickPlay { .. } if self.show.quick_play.effect != Effect::Cut => {
                self.show.quick_play.duration_ms
            }
            Action::OverlayIn {
                channel,
                duration_ms,
                ..
            }
            | Action::OverlayOut {
                channel,
                duration_ms,
            }
            | Action::OverlayLast {
                channel,
                duration_ms,
            } => overlay_index(*channel).map_or(0, |i| self.overlay_ms(i, *duration_ms)),
            // The longest of the channels going off.
            Action::OverlayOutAll { duration_ms } => (0..usize::from(OVERLAY_CHANNELS))
                .filter(|&i| self.show.overlays[i].is_some())
                .map(|i| self.overlay_ms(i, *duration_ms))
                .max()
                .unwrap_or(0),
            _ => 0,
        }
    }

    /// Transport commands for the players that went on air since
    /// `before` (play, restart) and for those that went off (pause).
    fn auto_play_commands(&self, before: &[InputId]) -> (Vec<Command>, Vec<Command>) {
        let now = self.airing();
        let (mut on, mut off) = (Vec::new(), Vec::new());
        for input in self.show.inputs.iter().filter(|i| i.source.has_transport()) {
            let (slot, auto) = (input.slot, input.auto_play);
            let (was, is) = (before.contains(&input.id), now.contains(&input.id));
            if is && !was {
                on.extend(match (auto.play, auto.restart) {
                    (true, true) => Some(Command::Transport {
                        slot,
                        transport: Transport::Restart,
                    }),
                    (true, false) => Some(Command::Transport {
                        slot,
                        transport: Transport::Play,
                    }),
                    (false, true) => Some(Command::Seek {
                        slot,
                        position_ms: 0,
                    }),
                    (false, false) => None,
                });
            } else if was && !is && auto.pause {
                off.push(Command::Transport {
                    slot,
                    transport: Transport::Pause,
                });
            }
        }
        (on, off)
    }

    /// Delayed commands as they stand now: the automatic pause of an
    /// input that is back on air is dropped (only INP.11 pauses later).
    pub fn still_due(&self, commands: &[Command]) -> Vec<Command> {
        let airing = self.airing();
        let on_air = |slot: usize| {
            self.input_at_slot(slot)
                .is_some_and(|id| airing.contains(&id))
        };
        commands
            .iter()
            .filter(|c| {
                !matches!(c, Command::Transport { slot, transport: Transport::Pause } if on_air(*slot))
            })
            .cloned()
            .collect()
    }

    // ---- Outputs ----

    fn add_output(
        &mut self,
        spec: OutputSpec,
        source: OutputSource,
        name: Option<String>,
        aux: Option<u8>,
    ) -> Result<Vec<Command>, ActionError> {
        if let Some(bus) = aux {
            aux_index(bus)?;
        }
        if let Some(reason) = spec.problem() {
            return Err(ActionError::BadEncoding { reason });
        }
        let output = ShowOutput {
            id: OutputId::new(),
            name: name.unwrap_or_default(),
            spec,
            source,
            aux,
        };
        self.feed_of(&output)?;
        let name = match (output.name.is_empty(), source) {
            (false, _) => output.name.clone(),
            (true, OutputSource::Program) => output.spec.default_name(),
            (true, OutputSource::Multiview) => {
                format!("{} (Multiview)", output.spec.default_name())
            }
            (true, OutputSource::Input(id)) => {
                let input = self
                    .show
                    .inputs
                    .iter()
                    .find(|i| i.id == id)
                    .map_or("", |i| &i.name);
                format!("{} ({input})", output.spec.default_name())
            }
        };
        self.show.outputs.push(ShowOutput { name, ..output });
        Ok(Vec::new())
    }

    /// Program, or the slot of the output's input; ISO outputs need an
    /// input with a picture.
    fn feed_of(&self, output: &ShowOutput) -> Result<Feed, ActionError> {
        let id = match output.source {
            OutputSource::Program => return Ok(Feed::Program),
            OutputSource::Multiview => return Ok(Feed::Multiview),
            OutputSource::Input(id) => id,
        };
        let input = self
            .show
            .inputs
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| ActionError::UnknownInput {
                reference: id.to_string(),
            })?;
        no_video_check(input)?;
        Ok(Feed::Slot {
            slot: input.slot,
            audio: input.source.has_audio(),
        })
    }

    fn remove_output(&mut self, output: &OutputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find_output(output)?;
        let removed = self.show.outputs.remove(index);
        self.output_status.remove(&removed.id);
        Ok(vec![Command::StopOutput { id: removed.id }])
    }

    /// Starting a running output restarts it (e.g. after a failure).
    fn start_output(&mut self, output: &OutputRef) -> Result<Vec<Command>, ActionError> {
        let entry = &self.show.outputs[self.find_output(output)?];
        Ok(vec![self.start_command(entry)?])
    }

    fn start_command(&self, output: &ShowOutput) -> Result<Command, ActionError> {
        Ok(Command::StartOutput {
            id: output.id,
            spec: output.spec.clone(),
            feed: self.feed_of(output)?,
            // Checked when the output was added.
            aux: output.aux.and_then(|bus| aux_index(bus).ok()),
        })
    }

    fn stop_output(&mut self, output: &OutputRef) -> Result<Vec<Command>, ActionError> {
        let id = self.show.outputs[self.find_output(output)?].id;
        Ok(vec![Command::StopOutput { id }])
    }

    /// Start commands for the outputs of `group` that do not run; an
    /// output that cannot start (its source is gone) is left out, the
    /// error only comes back when none could.
    fn start_outputs(&mut self, group: OutputGroup) -> Result<Vec<Command>, ActionError> {
        let mut first_error = None;
        let commands: Vec<Command> = self
            .show
            .outputs
            .iter()
            .filter(|o| group.holds(&o.spec) && !self.output_running(o.id))
            .filter_map(|o| match self.start_command(o) {
                Ok(command) => Some(command),
                Err(e) => {
                    first_error.get_or_insert(e);
                    None
                }
            })
            .collect();
        match first_error {
            Some(e) if commands.is_empty() => Err(e),
            _ => Ok(commands),
        }
    }

    fn stop_outputs(&mut self, group: OutputGroup) -> Vec<Command> {
        self.show
            .outputs
            .iter()
            .filter(|o| group.holds(&o.spec) && self.output_running(o.id))
            .map(|o| Command::StopOutput { id: o.id })
            .collect()
    }

    fn stop_all_outputs(&mut self) -> Vec<Command> {
        let running: Vec<OutputId> = self
            .show
            .outputs
            .iter()
            .filter(|o| self.output_running(o.id))
            .map(|o| o.id)
            .collect();
        self.output_status.clear();
        running
            .into_iter()
            .map(|id| Command::StopOutput { id })
            .collect()
    }

    /// Outputs that were running before an engine restart, to start again.
    fn running_output_commands(&self) -> Vec<Command> {
        self.show
            .outputs
            .iter()
            .filter(|o| self.output_running(o.id))
            .filter_map(|o| self.start_command(o).ok())
            .collect()
    }

    fn output_running(&self, id: OutputId) -> bool {
        matches!(
            self.output_status.get(&id),
            Some(OutputStatus::Running { .. })
        )
    }

    pub fn output_is_running(&self, id: OutputId) -> bool {
        self.output_running(id)
    }

    /// Start command for a failed output; `None` when it was stopped or
    /// removed meanwhile, or its input is gone.
    pub fn restart_command(&self, id: OutputId) -> Option<Command> {
        let failed = matches!(
            self.output_status.get(&id),
            Some(OutputStatus::Failed { .. })
        );
        let output = self
            .show
            .outputs
            .iter()
            .find(|o| o.id == id)
            .filter(|_| failed)?;
        self.start_command(output).ok()
    }

    /// The engine reports what an output does.
    pub fn set_output_status(&mut self, id: OutputId, status: OutputStatus) {
        if self.show.outputs.iter().any(|o| o.id == id) {
            self.output_status.insert(id, status);
        }
    }

    pub fn output_ids(&self) -> Vec<OutputId> {
        self.show.outputs.iter().map(|o| o.id).collect()
    }

    fn outputs_state(&self) -> Vec<Output> {
        self.show
            .outputs
            .iter()
            .enumerate()
            .map(|(i, o)| Output {
                id: o.id,
                number: i as u32 + 1,
                name: o.name.clone(),
                spec: o.spec.clone(),
                source: o.source,
                aux: o.aux,
                status: self.output_status.get(&o.id).cloned().unwrap_or_default(),
            })
            .collect()
    }

    fn find_output(&self, output: &OutputRef) -> Result<usize, ActionError> {
        let unknown = || ActionError::UnknownOutput {
            reference: match output {
                OutputRef::Number(n) => n.to_string(),
                OutputRef::Text(t) => format!("\"{t}\""),
            },
        };
        let outputs = &self.show.outputs;
        let index = match output {
            OutputRef::Number(n) => (*n as usize).checked_sub(1).filter(|i| *i < outputs.len()),
            OutputRef::Text(text) => outputs
                .iter()
                .position(|o| o.name == *text || o.id.to_string() == *text),
        };
        index.ok_or_else(unknown)
    }

    /// Signal-loss mode of the input in `slot`; hold for an empty slot.
    pub fn signal_loss_of(&self, slot: usize) -> SignalLoss {
        self.show
            .inputs
            .iter()
            .find(|i| i.slot == slot)
            .map_or_else(SignalLoss::default, |i| i.signal_loss)
    }

    fn transport(
        &mut self,
        input: &InputRef,
        transport: Transport,
    ) -> Result<Vec<Command>, ActionError> {
        Ok(vec![Command::Transport {
            slot: self.transport_slot(input)?,
            transport,
        }])
    }

    /// Slot of a file, URL or playlist input.
    fn transport_slot(&self, input: &InputRef) -> Result<usize, ActionError> {
        let entry = &self.show.inputs[self.find(input)?];
        if !entry.source.has_transport() {
            return Err(ActionError::NoTransport {
                reference: entry.name.clone(),
            });
        }
        Ok(entry.slot)
    }

    fn set_listen(
        &mut self,
        input: &InputRef,
        listen: Listen,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &self.show.inputs[index];
        self.listen.insert(entry.id, listen);
        Ok(vec![Command::SetListen {
            slot: entry.slot,
            listen,
        }])
    }

    fn clear_listen(&mut self) -> Vec<Command> {
        let soloed: Vec<usize> = self
            .show
            .inputs
            .iter()
            .filter(|i| self.listen_of(i.id) != Listen::Off)
            .map(|i| i.slot)
            .collect();
        self.listen.clear();
        soloed
            .into_iter()
            .map(|slot| Command::SetListen {
                slot,
                listen: Listen::Off,
            })
            .collect()
    }

    /// Audio follows the picture: a take fades sound as long as the
    /// transition; everything else only de-clicks.
    fn audio_ramp(&self, action: &Action) -> u32 {
        match action {
            Action::Transition { slot } => self
                .button(*slot)
                .map_or(AUDIO_RAMP_MS, |b| b.duration_ms.max(AUDIO_RAMP_MS)),
            Action::QuickPlay { .. } if self.show.quick_play.effect != Effect::Cut => {
                self.show.quick_play.duration_ms.max(AUDIO_RAMP_MS)
            }
            Action::FadeVolume {
                target: VolumeTarget::Input(_),
                duration_ms,
                ..
            } => (*duration_ms).clamp(AUDIO_RAMP_MS, FADE_MAX_MS),
            _ => AUDIO_RAMP_MS,
        }
    }

    /// Channels whose wanted setting differs from what the engine has.
    fn audio_commands(&mut self, ramp_ms: u32) -> Vec<Command> {
        let wanted: Vec<(usize, Mix)> = self
            .show
            .inputs
            .iter()
            .filter(|i| i.source.has_audio())
            .map(|i| (i.slot, self.mix_of(i)))
            .collect();
        self.mix
            .retain(|slot, _| wanted.iter().any(|(s, _)| s == slot));

        let mut commands = Vec::new();
        for (slot, mix) in wanted {
            if self.mix.get(&slot) == Some(&mix) {
                continue;
            }
            self.mix.insert(slot, mix);
            commands.push(Command::SetAudio {
                slot,
                gain: mix.gain,
                muted: mix.muted,
                ramp_ms,
            });
        }
        commands
    }

    fn mix_of(&self, input: &ShowInput) -> Mix {
        let audio = input.audio;
        let gain = fader_gain(audio.volume);
        let on_program = self.show.program == Some(input.id);
        if !audio.follow_video {
            return Mix {
                gain,
                muted: !audio.on,
            };
        }
        // While the T-bar is pulled (SWI.5), program sound fades out as
        // preview fades in, like the picture.
        let pulled = self.t_bar > 0.0;
        let share = if on_program && pulled {
            1.0 - f64::from(self.t_bar)
        } else if pulled && self.show.preview == Some(input.id) {
            f64::from(self.t_bar)
        } else if on_program {
            1.0
        } else {
            0.0
        };
        // Off air it is muted at its own level, as without the T-bar.
        let silent = share == 0.0;
        Mix {
            gain: if silent { gain } else { gain * share },
            muted: !audio.on || silent,
        }
    }

    /// strom refuses a fade during a stinger; it would invert the state.
    fn toggle_ftb(&mut self, duration_ms: u32) -> Result<Vec<Command>, ActionError> {
        self.check_no_stinger()?;
        self.fade_to_black = !self.fade_to_black;
        Ok(vec![Command::FadeToBlack { duration_ms }])
    }

    // ---- Lookup ----

    fn button(&self, slot: TransitionSlot) -> Result<&Transition, ActionError> {
        let bad = ActionError::BadTransitionSlot {
            slot: slot.0,
            max: TRANSITION_SLOTS,
        };
        if slot.0 == 0 {
            return Err(bad);
        }
        self.show.transitions.get(slot.0 as usize - 1).ok_or(bad)
    }

    fn free_slot(&self) -> Option<usize> {
        (0..self.slots).find(|s| !self.show.inputs.iter().any(|i| i.slot == *s))
    }

    /// Slot that carries `input`.
    /// `{value1}`… in title texts and function arguments replaced by the
    /// dynamic values (AUT.5).
    fn with_dynamic_values(&self, mut action: Action) -> Action {
        // One pass: a value that says `{value2}` stays as it is.
        let fill = |text: &mut String| {
            if !text.contains("{value") {
                return;
            }
            let mut out = String::new();
            let mut rest = text.as_str();
            while let Some(at) = rest.find("{value") {
                out.push_str(&rest[..at]);
                let tail = &rest[at..];
                let slot = tail
                    .get(6..8)
                    .filter(|t| t.ends_with('}'))
                    .and_then(|t| t[..1].parse::<usize>().ok())
                    .filter(|n| (1..=self.dynamic_values.len()).contains(n));
                match slot {
                    Some(n) => {
                        out.push_str(&self.dynamic_values[n - 1]);
                        rest = &tail[8..];
                    }
                    None => {
                        out.push_str("{value");
                        rest = &tail[6..];
                    }
                }
            }
            out.push_str(rest);
            *text = out;
        };
        match &mut action {
            Action::TitleLoad { data, .. } | Action::TitleUpdate { data, .. } => {
                data.values_mut().for_each(fill)
            }
            Action::TitleCall { args, .. } => args.iter_mut().for_each(fill),
            _ => {}
        }
        action
    }

    /// The input that feeds mixer slot `slot`.
    pub fn input_at_slot(&self, slot: usize) -> Option<InputId> {
        self.show
            .inputs
            .iter()
            .find(|i| i.slot == slot)
            .map(|i| i.id)
    }

    /// PTZ address of the input `input` names.
    pub fn ptz_of(&self, input: &InputRef) -> Result<lmix_types::PtzSpec, ActionError> {
        let entry = &self.show.inputs[self.find(input)?];
        entry.ptz.clone().ok_or_else(|| ActionError::BadPtz {
            reason: format!("{} has no PTZ address", entry.name),
        })
    }

    pub fn slot_for(&self, input: &InputRef) -> Result<usize, ActionError> {
        let index = self.find(input)?;
        Ok(self.show.inputs[index].slot)
    }

    fn slot_of(&self, id: Option<InputId>) -> Option<usize> {
        let id = id?;
        self.show.inputs.iter().find(|i| i.id == id).map(|i| i.slot)
    }

    /// Resolve an [`InputRef`] to an index in the input bar.
    fn find(&self, input: &InputRef) -> Result<usize, ActionError> {
        let unknown = || ActionError::UnknownInput {
            reference: match input {
                InputRef::Number(n) => n.to_string(),
                InputRef::Text(t) => format!("\"{t}\""),
            },
        };

        let index = match input {
            InputRef::Number(n) => (*n as usize)
                .checked_sub(1)
                .filter(|i| *i < self.show.inputs.len()),
            InputRef::Text(text) => self.find_text(text),
        };
        index.ok_or_else(unknown)
    }

    fn find_text(&self, text: &str) -> Option<usize> {
        let by_id = |id: Option<InputId>| self.show.inputs.iter().position(|i| Some(i.id) == id);

        if text == REF_PREVIEW {
            return by_id(self.show.preview);
        }
        if text == REF_PROGRAM {
            return by_id(self.show.program);
        }
        if let Some(slot) = text
            .strip_prefix(REF_DYNAMIC)
            .and_then(|n| n.parse::<u8>().ok())
            .filter(|n| (1..=DYNAMIC_SLOTS).contains(n))
        {
            return by_id(self.dynamic_inputs[usize::from(slot - 1)]);
        }
        if let Ok(uuid) = Uuid::parse_str(text) {
            return by_id(Some(InputId(uuid)));
        }
        self.show.inputs.iter().position(|i| i.name == text)
    }
}

/// Program change, then the new preview (if any). Preview comes second:
/// the engine refuses a preview that is still on program.
/// Program and preview as commands, inputs or layouts.
fn bus_commands(
    program: Option<Bus>,
    preview: Option<Bus>,
    effect: Effect,
    duration_ms: u32,
) -> Vec<Command> {
    let mut commands = Vec::new();
    match program {
        Some(Bus::Slot(slot)) => commands.push(Command::Program {
            slot: Some(slot),
            effect,
            duration_ms,
        }),
        Some(Bus::Layout(pip)) => commands.push(Command::ProgramLayout {
            pip,
            effect,
            duration_ms,
        }),
        None => {}
    }
    match preview {
        Some(Bus::Slot(slot)) => commands.push(Command::Preview { slot }),
        Some(Bus::Layout(pip)) => commands.push(Command::PreviewLayout { pip }),
        None => {}
    }
    commands
}

fn program_then_preview(
    program: Option<usize>,
    preview: Option<usize>,
    effect: Effect,
    duration_ms: u32,
) -> Vec<Command> {
    let mut commands = vec![Command::Program {
        slot: program,
        effect,
        duration_ms,
    }];
    if let Some(slot) = preview {
        commands.push(Command::Preview { slot });
    }
    commands
}

/// Reject sources the engine cannot build: a color that is not
/// `#rrggbb`, a web page that is not an http(s) address.
fn check_source(source: &SourceSpec) -> Result<(), ActionError> {
    match source {
        SourceSpec::Color { color } if color_rgb(color).is_none() => Err(ActionError::BadColor {
            color: color.clone(),
        }),
        SourceSpec::Web { url, .. } if !is_web_address(url) => {
            Err(ActionError::BadWebUrl { url: url.clone() })
        }
        _ => Ok(()),
    }
}

/// `http://` or `https://` with a host after it.
fn is_web_address(url: &str) -> bool {
    let lower = url.trim().to_ascii_lowercase();
    ["http://", "https://"].iter().any(|scheme| {
        lower
            .strip_prefix(scheme)
            .is_some_and(|rest| !rest.is_empty())
    })
}

/// Audio inputs have no picture for preview or program.
/// The player setting of `input`'s end for its slot; `None` for sources
/// without a player.
fn at_end_command(input: &ShowInput) -> Option<Command> {
    Some(Command::SetAtEnd {
        slot: input.slot,
        items: crate::engine::flows::player_items(&input.source)?,
        at_end: input.at_end,
    })
}

/// Restoring or rebuilding a slot: a held end has to be set again, a
/// loop is how the slot starts.
fn held_end_command(input: &ShowInput) -> Option<Command> {
    (!input.at_end.is_loop())
        .then(|| at_end_command(input))
        .flatten()
}

/// 0-based index of overlay `channel` (1–8).
fn overlay_index(channel: u8) -> Result<usize, ActionError> {
    if !(1..=OVERLAY_CHANNELS).contains(&channel) {
        return Err(ActionError::BadOverlay {
            channel,
            max: OVERLAY_CHANNELS,
        });
    }
    Ok(usize::from(channel - 1))
}

fn check_volume(volume: u8) -> Result<(), ActionError> {
    if volume > VOLUME_MAX {
        return Err(ActionError::BadVolume {
            volume,
            max: VOLUME_MAX,
        });
    }
    Ok(())
}

/// Every aux send of an input with sound, as the show has them.
fn send_commands(input: &ShowInput) -> impl Iterator<Item = Command> + '_ {
    input
        .sends
        .iter()
        .enumerate()
        .map(|(bus, level)| Command::SetAuxSend {
            slot: input.slot,
            bus,
            gain: fader_gain(*level),
        })
}

fn no_video_check(entry: &ShowInput) -> Result<(), ActionError> {
    if entry.source.has_video() {
        return Ok(());
    }
    Err(ActionError::NoVideo {
        reference: entry.name.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::{MidiBinding, TestPattern, VideoEncoding};

    const SLOTS: usize = 4;

    fn pattern(p: TestPattern) -> SourceSpec {
        SourceSpec::Pattern { pattern: p }
    }

    fn add(sw: &mut Switcher, name: &str) {
        sw.apply(Action::AddInput {
            source: pattern(TestPattern::Bars),
            name: Some(name.into()),
        })
        .unwrap();
    }

    fn named(name: &str) -> InputRef {
        InputRef::Text(name.into())
    }

    fn two_inputs() -> Switcher {
        let mut sw = Switcher::new(SLOTS);
        add(&mut sw, "A");
        add(&mut sw, "B");
        sw
    }

    #[test]
    fn dynamic_inputs_pick_then_act() {
        let mut sw = two_inputs();
        let dynamic = || InputRef::Text("dynamic2".into());

        assert!(matches!(
            sw.apply(Action::PreviewInput { input: dynamic() }),
            Err(ActionError::UnknownInput { .. })
        ));
        sw.apply(Action::SetDynamicInput {
            slot: 2,
            input: Some(named("B")),
        })
        .unwrap();
        sw.apply(Action::PreviewInput { input: dynamic() }).unwrap();
        let state = sw.state();
        assert_eq!(state.preview, Some(state.inputs[1].id));
        assert_eq!(state.dynamic_inputs[1], Some(state.inputs[1].id));

        sw.apply(Action::SetDynamicInput {
            slot: 2,
            input: None,
        })
        .unwrap();
        assert!(sw.apply(Action::PreviewInput { input: dynamic() }).is_err());
        for slot in [0, 5] {
            assert!(matches!(
                sw.apply(Action::SetDynamicValue {
                    slot,
                    value: "x".into()
                }),
                Err(ActionError::BadDynamic { .. })
            ));
        }
    }

    #[test]
    fn dynamic_values_fill_title_texts() {
        let mut sw = two_inputs();
        sw.set_templates(std::collections::HashMap::from([(
            "t".to_string(),
            TemplateRef {
                url: "/t".into(),
                revision: 0,
            },
        )]));
        sw.apply(Action::AddGraphics { name: None }).unwrap();
        sw.apply(Action::SetDynamicValue {
            slot: 1,
            value: "Guest".into(),
        })
        .unwrap();
        sw.apply(Action::TitleLoad {
            graphics: InputRef::Number(1),
            layer: 1,
            template: "t".into(),
            data: lmix_types::TitleData::from([(
                "name".to_string(),
                "Hello {value1}!".to_string(),
            )]),
            play: false,
        })
        .unwrap();

        assert_eq!(
            sw.state().graphics[0].layers[0].data["name"],
            "Hello Guest!"
        );
        assert_eq!(sw.state().dynamic_values[0], "Guest");
        // One pass: a value naming another slot stays literal.
        sw.apply(Action::SetDynamicValue {
            slot: 2,
            value: "{value1}".into(),
        })
        .unwrap();
        sw.apply(Action::TitleUpdate {
            graphics: InputRef::Number(1),
            layer: 1,
            data: lmix_types::TitleData::from([(
                "name".to_string(),
                "{value2} {value9} {value".to_string(),
            )]),
        })
        .unwrap();
        assert_eq!(
            sw.state().graphics[0].layers[0].data["name"],
            "{value1} {value9} {value"
        );
    }

    #[test]
    fn color_tags_an_input_without_engine_work() {
        let mut sw = two_inputs();

        let cmds = sw
            .apply(Action::SetInputColor {
                input: named("B"),
                color: Some(InputColor::Cyan),
            })
            .unwrap();

        assert!(cmds.is_empty());
        assert_eq!(sw.state().inputs[1].color, Some(InputColor::Cyan));
    }

    fn audio_of(cmds: &[Command]) -> Vec<(usize, bool, u32)> {
        cmds.iter()
            .filter_map(|c| match c {
                Command::SetAudio {
                    slot,
                    muted,
                    ramp_ms,
                    ..
                } => Some((*slot, *muted, *ramp_ms)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn follow_video_is_heard_only_on_program() {
        let mut sw = two_inputs();
        sw.apply(Action::SetTransition {
            slot: TransitionSlot(1),
            effect: Effect::Fade,
            duration_ms: 800,
            stinger: None,
        })
        .unwrap();

        let cmds = sw
            .apply(Action::SetAudioFollow {
                input: named("B"),
                follow: true,
            })
            .unwrap();
        assert_eq!(audio_of(&cmds), [(1, true, AUDIO_RAMP_MS)]);

        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(1),
            })
            .unwrap();
        assert_eq!(
            audio_of(&cmds),
            [(1, false, 800)],
            "fades in with the picture"
        );

        sw.apply(Action::PreviewInput { input: named("A") })
            .unwrap();
        let cmds = sw.apply(Action::Cut).unwrap();
        assert_eq!(audio_of(&cmds), [(1, true, AUDIO_RAMP_MS)]);
    }

    #[test]
    fn audio_off_and_volume() {
        let mut sw = two_inputs();

        let cmds = sw
            .apply(Action::SetAudio {
                input: named("A"),
                on: false,
            })
            .unwrap();
        assert_eq!(audio_of(&cmds), [(0, true, AUDIO_RAMP_MS)]);

        let cmds = sw
            .apply(Action::SetVolume {
                input: named("A"),
                volume: 50,
            })
            .unwrap();
        assert!(matches!(cmds[..], [Command::SetAudio { gain, .. }] if gain == 0.25));

        let err = sw
            .apply(Action::SetVolume {
                input: named("A"),
                volume: 101,
            })
            .unwrap_err();
        assert!(matches!(err, ActionError::BadVolume { .. }));
    }

    #[test]
    fn audio_inputs_stay_off_the_buses() {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::AudioDevice { id: "mic".into() },
            name: Some("Mic".into()),
        })
        .unwrap();

        for action in [
            Action::PreviewInput {
                input: named("Mic"),
            },
            Action::CutDirect {
                input: named("Mic"),
            },
        ] {
            assert!(matches!(sw.apply(action), Err(ActionError::NoVideo { .. })));
        }
        sw.apply(Action::PreviewInput { input: named("A") })
            .unwrap();
        let swap = Action::ReplaceSource {
            input: named("A"),
            source: SourceSpec::AudioDevice { id: "mic".into() },
        };
        assert!(matches!(sw.apply(swap), Err(ActionError::NoVideo { .. })));
    }

    #[test]
    fn signal_loss_defaults_to_hold_and_is_kept() {
        let mut sw = two_inputs();
        assert_eq!(sw.state().inputs[0].signal_loss, SignalLoss::Hold);

        let cmds = sw
            .apply(Action::SetSignalLoss {
                input: named("B"),
                mode: SignalLoss::Black,
            })
            .unwrap();

        assert_eq!(
            cmds,
            [Command::SetSignalLoss {
                slot: 1,
                mode: SignalLoss::Black
            }]
        );
        assert_eq!(sw.signal_loss_of(1), SignalLoss::Black);
        let restored = sw.restore_commands();
        assert!(restored.contains(&Command::SetSignalLoss {
            slot: 1,
            mode: SignalLoss::Black
        }));
    }

    fn overlay_in(sw: &mut Switcher, channel: u8, input: &str) -> Vec<Command> {
        sw.apply(Action::OverlayIn {
            channel,
            input: named(input),
            duration_ms: Some(500),
        })
        .unwrap()
    }

    #[test]
    fn overlay_goes_on_and_off_with_tally() {
        let mut sw = two_inputs();

        let cmds = overlay_in(&mut sw, 2, "B");

        assert_eq!(
            cmds,
            [Command::Overlay {
                channel: 1,
                slot: Some(1),
                duration_ms: 500
            }]
        );
        let b = sw.state().inputs[1].id;
        assert_eq!(
            sw.state().overlays,
            [None, Some(b), None, None, None, None, None, None]
        );
        assert_eq!(sw.tally(), [0, 1]);

        let cmds = sw
            .apply(Action::OverlayOut {
                channel: 2,
                duration_ms: Some(0),
            })
            .unwrap();

        assert_eq!(
            cmds,
            [Command::Overlay {
                channel: 1,
                slot: None,
                duration_ms: 0
            }]
        );
        assert_eq!(sw.tally(), [0, 0]);
        let again = sw.apply(Action::OverlayOut {
            channel: 2,
            duration_ms: Some(0),
        });
        assert_eq!(again, Ok(Vec::new()));
    }

    #[test]
    fn overlay_channel_and_source_are_checked() {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::AudioDevice { id: "mic".into() },
            name: Some("Mic".into()),
        })
        .unwrap();

        let bad = sw.apply(Action::OverlayIn {
            channel: 9,
            input: named("A"),
            duration_ms: Some(0),
        });
        let sound_only = sw.apply(Action::OverlayIn {
            channel: 1,
            input: named("Mic"),
            duration_ms: Some(0),
        });

        assert_eq!(bad, Err(ActionError::BadOverlay { channel: 9, max: 8 }));
        assert!(matches!(sound_only, Err(ActionError::NoVideo { .. })));
    }

    #[test]
    fn removed_input_leaves_its_overlay_and_restore_keeps_overlays() {
        let mut sw = two_inputs();
        overlay_in(&mut sw, 1, "A");
        overlay_in(&mut sw, 3, "B");

        let restored = sw.restore_commands();
        assert!(restored.contains(&Command::Overlay {
            channel: 2,
            slot: Some(1),
            duration_ms: 0
        }));
        assert!(restored.contains(&Command::Overlay {
            channel: 1,
            slot: None,
            duration_ms: 0
        }));

        let cmds = sw.apply(Action::RemoveInput { input: named("A") }).unwrap();

        assert!(cmds.contains(&Command::Overlay {
            channel: 0,
            slot: None,
            duration_ms: 0
        }));
        assert_eq!(sw.state().overlays[0], None);
    }

    #[test]
    fn all_overlays_go_off_and_the_last_input_comes_back() {
        let mut sw = two_inputs();
        overlay_in(&mut sw, 1, "A");
        overlay_in(&mut sw, 3, "B");
        let (a, b) = (sw.state().inputs[0].id, sw.state().inputs[1].id);

        let cmds = sw
            .apply(Action::OverlayOutAll {
                duration_ms: Some(300),
            })
            .unwrap();

        assert_eq!(
            cmds,
            [
                Command::Overlay {
                    channel: 0,
                    slot: None,
                    duration_ms: 300
                },
                Command::Overlay {
                    channel: 2,
                    slot: None,
                    duration_ms: 300
                },
            ]
        );
        assert_eq!(sw.state().overlays, [None; 8]);
        assert_eq!(
            sw.state().overlay_last,
            [Some(a), None, Some(b), None, None, None, None, None]
        );
        assert_eq!(
            sw.apply(Action::OverlayOutAll {
                duration_ms: Some(0)
            }),
            Ok(Vec::new())
        );

        let last = |channel| Action::OverlayLast {
            channel,
            duration_ms: Some(0),
        };
        assert_eq!(
            sw.apply(last(3)).unwrap(),
            [Command::Overlay {
                channel: 2,
                slot: Some(1),
                duration_ms: 0
            }]
        );
        assert_eq!(sw.state().overlays[2], Some(b));
        // Showing already: nothing to do.
        assert_eq!(sw.apply(last(3)), Ok(Vec::new()));
        assert_eq!(
            sw.apply(last(2)),
            Err(ActionError::NoOverlayLast { channel: 2 })
        );
        assert!(matches!(
            sw.apply(last(9)),
            Err(ActionError::BadOverlay { .. })
        ));

        // A closed input is not brought back, until it is restored.
        sw.apply(Action::RemoveInput { input: named("A") }).unwrap();
        assert_eq!(sw.state().overlay_last[0], None);
        assert_eq!(
            sw.apply(last(1)),
            Err(ActionError::NoOverlayLast { channel: 1 })
        );
        sw.apply(Action::RestoreInput).unwrap();
        assert!(sw.apply(last(1)).is_ok());
        assert_eq!(sw.state().overlays[0], Some(a));
    }

    fn set_overlay(channel: u8, settings: OverlayChannel) -> Action {
        Action::SetOverlay {
            channel,
            effect: settings.effect,
            duration_ms: settings.duration_ms,
            auto_off_ms: settings.auto_off_ms,
            rect: settings.rect,
        }
    }

    #[test]
    fn overlay_channels_have_their_own_effect_and_place() {
        let mut sw = two_inputs();
        let corner = BoxRect {
            x: 0.7,
            y: 0.05,
            w: 0.25,
            h: 0.25,
        };
        let pip = OverlayChannel {
            effect: Effect::Cut,
            duration_ms: 0,
            auto_off_ms: None,
            rect: Some(corner),
        };

        assert_eq!(
            sw.apply(set_overlay(2, pip)),
            Ok(vec![Command::OverlayPlace {
                channel: 1,
                rect: Some(corner)
            }])
        );
        assert_eq!(sw.apply(set_overlay(2, pip)), Ok(Vec::new()), "not moved");
        assert_eq!(sw.state().overlay_channels[1], pip);
        let on = |sw: &mut Switcher, channel, duration_ms| {
            sw.apply(Action::OverlayIn {
                channel,
                input: named("B"),
                duration_ms,
            })
            .unwrap()
        };
        let faded = |cmds: Vec<Command>| match cmds.as_slice() {
            [Command::Overlay { duration_ms, .. }] => *duration_ms,
            other => panic!("{other:?}"),
        };
        assert_eq!(faded(on(&mut sw, 2, None)), 0, "the channel cuts");
        assert_eq!(faded(on(&mut sw, 2, Some(300))), 300, "the action's own");
        assert_eq!(faded(on(&mut sw, 1, None)), 500, "default: half a second");
        let off = sw
            .apply(Action::OverlayOutAll { duration_ms: None })
            .unwrap();
        assert!(off.contains(&Command::Overlay {
            channel: 0,
            slot: None,
            duration_ms: 500
        }));
        assert!(off.contains(&Command::Overlay {
            channel: 1,
            slot: None,
            duration_ms: 0
        }));
        // Placed again when the engine is rebuilt.
        assert!(sw.restore_commands().contains(&Command::OverlayPlace {
            channel: 1,
            rect: Some(corner)
        }));

        let bad = |sw: &mut Switcher, settings| {
            matches!(
                sw.apply(set_overlay(1, settings)),
                Err(ActionError::BadOverlaySettings { channel: 1, .. })
            )
        };
        let fade = OverlayChannel::default();
        assert!(bad(
            &mut sw,
            OverlayChannel {
                effect: Effect::WipeLeft,
                ..fade
            }
        ));
        assert!(bad(
            &mut sw,
            OverlayChannel {
                auto_off_ms: Some(0),
                ..fade
            }
        ));
        assert!(bad(
            &mut sw,
            OverlayChannel {
                rect: Some(BoxRect { x: 0.9, ..corner }),
                ..fade
            }
        ));
        assert!(matches!(
            sw.apply(set_overlay(9, fade)),
            Err(ActionError::BadOverlay { .. })
        ));
    }

    #[test]
    fn auto_off_is_due_for_the_time_an_overlay_went_on() {
        let mut sw = two_inputs();
        sw.apply(set_overlay(
            2,
            OverlayChannel {
                auto_off_ms: Some(5000),
                ..OverlayChannel::default()
            },
        ))
        .unwrap();

        let before = sw.on_air();
        overlay_in(&mut sw, 1, "A");
        assert_eq!(sw.auto_offs(&before), [], "channel 1 stays");
        let before = sw.on_air();
        overlay_in(&mut sw, 2, "B");
        let [(5000, 2, since)] = sw.auto_offs(&before)[..] else {
            panic!("{:?}", sw.auto_offs(&before));
        };
        assert!(sw.overlay_still_on(2, since));
        assert_eq!(sw.auto_offs(&sw.on_air()), [], "nothing new");

        // Shown again (even the same input): the old auto off is over.
        let before = sw.on_air();
        overlay_in(&mut sw, 2, "B");
        let [(_, 2, again)] = sw.auto_offs(&before)[..] else {
            panic!("shown again");
        };
        assert!(!sw.overlay_still_on(2, since));
        assert!(sw.overlay_still_on(2, again));
        sw.apply(Action::OverlayOut {
            channel: 2,
            duration_ms: None,
        })
        .unwrap();
        assert!(!sw.overlay_still_on(2, again), "went off before");
        assert!(!sw.overlay_still_on(9, again));
    }

    #[test]
    fn overlay_prepared_in_preview_goes_on_with_the_next_take() {
        let mut sw = two_inputs();
        add(&mut sw, "Logo");
        let id = |sw: &Switcher, n: usize| sw.state().inputs[n].id;
        let prepare = |sw: &mut Switcher, channel, input: Option<&str>| {
            sw.apply(Action::OverlayPreview {
                channel,
                input: input.map(named),
            })
        };
        sw.apply(Action::PreviewInput { input: named("A") })
            .unwrap();
        assert_eq!(prepare(&mut sw, 1, Some("Logo")), Ok(Vec::new()));
        assert_eq!(sw.state().overlay_preview[0], Some(id(&sw, 2)));
        assert_eq!(sw.tally(), [2, 0, 2], "the logo has preview tally");

        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(1),
            })
            .unwrap();
        assert_eq!(
            cmds.last(),
            Some(&Command::Overlay {
                channel: 0,
                slot: Some(2),
                duration_ms: 500
            }),
            "with the take's fade"
        );
        let state = sw.state();
        assert_eq!(state.overlays[0], Some(id(&sw, 2)));
        assert_eq!(state.overlay_preview, [None; 8], "used up");
        assert_eq!(state.overlay_last[0], Some(id(&sw, 2)));
        assert_eq!(sw.tally(), [1, 0, 1]);

        // With a cut at once; the T-bar at its end is a cut.
        sw.apply(Action::OverlayOut {
            channel: 1,
            duration_ms: Some(0),
        })
        .unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        prepare(&mut sw, 3, Some("Logo")).unwrap();
        let cmds = sw.apply(Action::TBar { position: 1.0 }).unwrap();
        assert!(cmds.contains(&Command::Overlay {
            channel: 2,
            slot: Some(2),
            duration_ms: 0
        }));
        // Already on the channel: nothing more.
        prepare(&mut sw, 3, Some("Logo")).unwrap();
        let cmds = sw.apply(Action::Cut).unwrap();
        assert!(!cmds.iter().any(|c| matches!(c, Command::Overlay { .. })));
        assert_eq!(sw.state().overlay_preview[2], None);

        // Cleared, by hand or with the input; cut direct takes none along.
        prepare(&mut sw, 2, Some("Logo")).unwrap();
        prepare(&mut sw, 2, None).unwrap();
        assert_eq!(sw.state().overlay_preview, [None; 8]);
        prepare(&mut sw, 2, Some("A")).unwrap();
        sw.apply(Action::CutDirect { input: named("B") }).unwrap();
        assert_eq!(sw.state().overlays[1], None);
        sw.apply(Action::RemoveInput { input: named("A") }).unwrap();
        assert_eq!(sw.state().overlay_preview, [None; 8]);
        assert!(matches!(
            prepare(&mut sw, 0, Some("B")),
            Err(ActionError::BadOverlay { .. })
        ));
    }

    #[test]
    fn layout_templates_keep_places_and_find_their_inputs_again() {
        let mut sw = two_inputs();
        add(&mut sw, "C");
        let rect = BoxRect {
            x: 0.5,
            y: 0.0,
            w: 0.5,
            h: 0.5,
        };
        sw.apply(Action::AddLayout {
            name: Some("Interview".into()),
            background: Some(named("A")),
            boxes: vec![BoxSpec {
                rect,
                input: named("C"),
                border_px: 4,
                crop: BoxCrop::default(),
                locked: true,
                hidden: false,
            }],
        })
        .unwrap();
        sw.apply(Action::SaveLayoutTemplate {
            layout: InputRef::Number(1),
            name: None,
        })
        .unwrap();
        assert!(sw.take_layout_templates_changed());
        let template = &sw.state().layout_templates[0];
        assert_eq!(template.name, "Interview");
        assert_eq!(
            template.boxes[0].input,
            TemplateInput {
                number: 3,
                name: "C".into()
            }
        );

        // Another show: "C" by its name, "A" (gone) at its number, which
        // is free.
        sw.replace(Show::default(), None).unwrap();
        assert_eq!(sw.state().layout_templates.len(), 1, "kept apart");
        add(&mut sw, "C");
        add(&mut sw, "D");
        let cmds = sw
            .apply(Action::LoadLayoutTemplate {
                template: "Interview".into(),
                name: None,
            })
            .unwrap();
        assert!(matches!(cmds.as_slice(), [Command::SetLayout { .. }]));
        let state = sw.state();
        let layout = &state.layouts[0];
        assert_eq!(layout.name, "Interview");
        assert_eq!(layout.boxes[0].input, state.inputs[0].id, "C by name");
        assert_eq!(layout.boxes[0].rect, rect);
        assert!(layout.boxes[0].locked);
        assert_eq!(
            layout.background,
            Some(state.inputs[1].id),
            "number 1 is C's: the next one"
        );

        // Same name replaces; export picks by name; unknown ones fail.
        sw.apply(Action::SaveLayoutTemplate {
            layout: InputRef::Number(1),
            name: Some("Interview".into()),
        })
        .unwrap();
        assert_eq!(sw.layout_templates().len(), 1);
        assert_eq!(sw.layout_templates_named(&[]).unwrap().len(), 1);
        assert!(matches!(
            sw.layout_templates_named(&["Nope".into()]),
            Err(ActionError::UnknownLayoutTemplate { .. })
        ));
        sw.apply(Action::RemoveInput { input: named("D") }).unwrap();
        assert!(
            matches!(
                sw.apply(Action::LoadLayoutTemplate {
                    template: "Interview".into(),
                    name: Some("Two".into()),
                }),
                Err(ActionError::BadLayout { .. })
            ),
            "two places, one input"
        );
        sw.apply(Action::RemoveLayoutTemplate {
            template: "Interview".into(),
        })
        .unwrap();
        assert!(sw.state().layout_templates.is_empty());
        assert!(matches!(
            sw.apply(Action::RemoveLayoutTemplate {
                template: "Interview".into(),
            }),
            Err(ActionError::UnknownLayoutTemplate { .. })
        ));
        assert!(matches!(
            sw.import_layout_templates(vec![LayoutTemplate {
                name: " ".into(),
                background: None,
                boxes: Vec::new(),
            }]),
            Err(ActionError::BadLayout { .. })
        ));
    }

    /// A, B (patterns), Clip 1 and Clip 2 (files) with `auto`.
    fn with_clips(auto: AutoPlay) -> Switcher {
        let mut sw = two_inputs();
        for name in ["Clip 1", "Clip 2"] {
            sw.apply(Action::AddInput {
                source: SourceSpec::File {
                    path: format!("/{name}.mp4"),
                },
                name: Some(name.into()),
            })
            .unwrap();
            sw.apply(Action::SetAutoPlay {
                input: named(name),
                auto_play: auto,
            })
            .unwrap();
        }
        sw
    }

    const PLAY_AND_PAUSE: AutoPlay = AutoPlay {
        play: true,
        restart: false,
        pause: true,
    };

    fn transport(slot: usize, transport: Transport) -> Command {
        Command::Transport { slot, transport }
    }

    #[test]
    fn a_clip_loops_or_holds_its_end() {
        let mut sw = with_clips(AutoPlay::default());
        let hold = |name: &str| Action::SetAtEnd {
            input: named(name),
            at_end: AtEnd::Hold,
        };
        let held = Command::SetAtEnd {
            slot: 2,
            items: vec!["/Clip 1.mp4".into()],
            at_end: AtEnd::Hold,
        };
        assert_eq!(
            sw.apply(hold("Clip 1")).unwrap(),
            std::slice::from_ref(&held),
            "on air, no rebuild"
        );
        assert_eq!(sw.state().inputs[2].at_end, AtEnd::Hold);
        assert!(matches!(
            sw.apply(hold("A")),
            Err(ActionError::NoTransport { .. })
        ));
        // A rebuilt engine starts players looping: the held end comes back.
        let restore = sw.restore_commands();
        assert!(restore.contains(&held));
        assert_eq!(
            restore
                .iter()
                .filter(|c| matches!(c, Command::SetAtEnd { .. }))
                .count(),
            1,
            "looping clips need nothing"
        );
    }

    #[test]
    fn auto_play_is_set_kept_and_only_for_players() {
        let mut sw = with_clips(PLAY_AND_PAUSE);
        assert_eq!(sw.state().inputs[2].auto_play, PLAY_AND_PAUSE);
        assert_eq!(sw.state().inputs[0].auto_play, AutoPlay::default());
        assert!(matches!(
            sw.apply(Action::SetAutoPlay {
                input: named("A"),
                auto_play: PLAY_AND_PAUSE,
            }),
            Err(ActionError::NoTransport { .. })
        ));
        // Switching it off is fine anywhere (a source that was a clip).
        assert!(sw
            .apply(Action::SetAutoPlay {
                input: named("A"),
                auto_play: AutoPlay::default(),
            })
            .is_ok());
        // Without the switches nothing plays by itself.
        let mut plain = with_clips(AutoPlay::default());
        assert_eq!(
            plain
                .apply(Action::CutDirect {
                    input: named("Clip 1")
                })
                .unwrap(),
            [Command::Program {
                slot: Some(2),
                effect: Effect::Cut,
                duration_ms: 0
            }]
        );
    }

    #[test]
    fn a_take_plays_the_new_clip_first_and_pauses_the_old_after_the_fade() {
        let mut sw = with_clips(PLAY_AND_PAUSE);
        sw.apply(Action::CutDirect {
            input: named("Clip 1"),
        })
        .unwrap();
        sw.apply(Action::PreviewInput {
            input: named("Clip 2"),
        })
        .unwrap();

        // Button 1: fade, 500 ms.
        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(1),
            })
            .unwrap();

        assert_eq!(cmds[0], transport(3, Transport::Play));
        assert_eq!(
            cmds[1],
            Command::Program {
                slot: Some(3),
                effect: Effect::Fade,
                duration_ms: 500
            }
        );
        assert_eq!(
            cmds[cmds.len() - 2..],
            [Command::Wait { ms: 500 }, transport(2, Transport::Pause)]
        );

        // A cut pauses at once.
        let cmds = sw.apply(Action::Cut).unwrap();
        assert_eq!(cmds[0], transport(2, Transport::Play));
        assert_eq!(cmds.last(), Some(&transport(3, Transport::Pause)));
        assert!(!cmds.iter().any(|c| matches!(c, Command::Wait { .. })));
    }

    #[test]
    fn restart_starts_from_the_beginning_with_or_without_play() {
        let mut sw = with_clips(AutoPlay {
            play: true,
            restart: true,
            pause: false,
        });
        sw.apply(Action::SetAutoPlay {
            input: named("Clip 2"),
            auto_play: AutoPlay {
                restart: true,
                ..Default::default()
            },
        })
        .unwrap();

        let cmds = sw
            .apply(Action::CutDirect {
                input: named("Clip 1"),
            })
            .unwrap();
        assert_eq!(cmds[0], transport(2, Transport::Restart));

        let cmds = sw
            .apply(Action::CutDirect {
                input: named("Clip 2"),
            })
            .unwrap();
        assert_eq!(
            cmds[0],
            Command::Seek {
                slot: 3,
                position_ms: 0
            }
        );
        // Clip 1 does not pause by itself.
        assert!(!cmds.contains(&transport(2, Transport::Pause)));
    }

    #[test]
    fn overlays_and_layouts_count_as_on_air() {
        let mut sw = with_clips(PLAY_AND_PAUSE);
        sw.apply(Action::CutDirect {
            input: named("Clip 1"),
        })
        .unwrap();

        // On air already: as an overlay too, nothing new.
        let cmds = overlay_in(&mut sw, 1, "Clip 1");
        assert!(!cmds.iter().any(|c| matches!(c, Command::Transport { .. })));
        // Off program, still an overlay: keeps playing.
        let cmds = sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        assert!(!cmds.contains(&transport(2, Transport::Pause)));
        // Off the overlay as well: pauses after its fade.
        let cmds = sw
            .apply(Action::OverlayOut {
                channel: 1,
                duration_ms: Some(200),
            })
            .unwrap();
        assert_eq!(
            cmds[cmds.len() - 2..],
            [Command::Wait { ms: 200 }, transport(2, Transport::Pause)]
        );
        let cmds = overlay_in(&mut sw, 2, "Clip 2");
        assert_eq!(cmds[0], transport(3, Transport::Play));
        let cmds = sw
            .apply(Action::OverlayOutAll {
                duration_ms: Some(0),
            })
            .unwrap();
        assert_eq!(cmds.last(), Some(&transport(3, Transport::Pause)));

        // A box of a layout on program.
        let cmds = sw
            .apply(Action::AddLayout {
                name: Some("Two".into()),
                background: Some(named("A")),
                boxes: vec![BoxSpec {
                    input: named("Clip 2"),
                    rect: BoxRect {
                        x: 0.5,
                        y: 0.5,
                        w: 0.5,
                        h: 0.5,
                    },
                    border_px: 0,
                    crop: BoxCrop::default(),
                    locked: false,
                    hidden: false,
                }],
            })
            .unwrap();
        assert!(!cmds.iter().any(|c| matches!(c, Command::Transport { .. })));
        sw.apply(Action::PreviewLayout {
            layout: named("Two"),
        })
        .unwrap();
        let cmds = sw.apply(Action::Cut).unwrap();
        assert_eq!(cmds[0], transport(3, Transport::Play));
    }

    #[test]
    fn the_t_bar_plays_as_it_starts_and_pauses_back_at_zero() {
        let mut sw = with_clips(PLAY_AND_PAUSE);
        sw.apply(Action::CutDirect {
            input: named("Clip 1"),
        })
        .unwrap();
        sw.apply(Action::PreviewInput {
            input: named("Clip 2"),
        })
        .unwrap();
        let t_bar = |sw: &mut Switcher, position| sw.apply(Action::TBar { position }).unwrap();

        assert_eq!(t_bar(&mut sw, 0.2)[0], transport(3, Transport::Play));
        assert_eq!(t_bar(&mut sw, 0.5).len(), 1, "just the T-bar");
        assert_eq!(
            t_bar(&mut sw, 0.0).last(),
            Some(&transport(3, Transport::Pause))
        );
        t_bar(&mut sw, 0.5);
        // Taken at the end: the old clip pauses, the new one plays on.
        let cmds = t_bar(&mut sw, 1.0);
        assert_eq!(cmds.last(), Some(&transport(2, Transport::Pause)));
        assert!(!cmds.contains(&transport(3, Transport::Play)));
    }

    #[test]
    fn a_stinger_switches_players_at_its_cut() {
        let mut sw = with_stinger();
        for name in ["A", "B"] {
            sw.apply(Action::ReplaceSource {
                input: named(name),
                source: SourceSpec::File {
                    path: format!("/{name}.mp4"),
                },
            })
            .unwrap();
            sw.apply(Action::SetAutoPlay {
                input: named(name),
                auto_play: PLAY_AND_PAUSE,
            })
            .unwrap();
        }

        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(2),
            })
            .unwrap();

        assert_eq!(
            cmds[..4],
            [
                sting(1),
                Command::Preview { slot: 0 },
                Command::Wait { ms: 400 + PREROLL },
                transport(1, Transport::Play),
            ]
        );
        assert_eq!(cmds.last(), Some(&transport(0, Transport::Pause)));
    }

    #[test]
    fn a_late_pause_is_dropped_when_the_clip_is_back_on_air() {
        let mut sw = with_clips(PLAY_AND_PAUSE);
        sw.apply(Action::CutDirect {
            input: named("Clip 1"),
        })
        .unwrap();
        sw.apply(Action::PreviewInput {
            input: named("Clip 2"),
        })
        .unwrap();
        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(1),
            })
            .unwrap();
        let wait = cmds
            .iter()
            .position(|c| matches!(c, Command::Wait { .. }))
            .unwrap();
        let later = &cmds[wait + 1..];
        assert_eq!(sw.still_due(later), later);

        // Cut back during the fade.
        sw.apply(Action::Cut).unwrap();
        assert_eq!(sw.still_due(later), []);
    }

    /// Commands without the audio follow-video ones.
    fn picture_and_players(cmds: &[Command]) -> Vec<Command> {
        cmds.iter()
            .filter(|c| !matches!(c, Command::SetAudio { .. }))
            .cloned()
            .collect()
    }

    #[test]
    fn quick_play_fades_the_input_in_and_plays_it() {
        let mut sw = with_clips(AutoPlay::default());
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        assert_eq!(sw.state().quick_play, QuickPlay::default());
        sw.apply(Action::SetAudioFollow {
            input: named("Clip 1"),
            follow: true,
        })
        .unwrap();

        // Not in preview: straight over, preview stays.
        let cmds = sw
            .apply(Action::QuickPlay {
                input: Some(named("Clip 1")),
            })
            .unwrap();
        assert_eq!(
            picture_and_players(&cmds),
            [
                transport(2, Transport::Play),
                Command::Program {
                    slot: Some(2),
                    effect: Effect::Fade,
                    duration_ms: 500
                },
            ]
        );
        let state = sw.state();
        assert_eq!(state.program, Some(state.inputs[2].id));
        assert_eq!(state.preview, Some(state.inputs[1].id));
        // Its sound follows over the length of the fade.
        assert_eq!(audio_of(&cmds), [(2, false, 500)]);

        // On program already: it only plays.
        assert_eq!(
            sw.apply(Action::QuickPlay {
                input: Some(named("Clip 1")),
            }),
            Ok(vec![transport(2, Transport::Play)])
        );

        // No input: preview, taken as with a transition button.
        let cmds = sw.apply(Action::QuickPlay { input: None }).unwrap();
        assert_eq!(
            picture_and_players(&cmds),
            [
                Command::Program {
                    slot: Some(1),
                    effect: Effect::Fade,
                    duration_ms: 500
                },
                Command::Preview { slot: 2 },
            ]
        );
        assert_eq!(sw.tally(), [0, 1, 2, 0]);

        let mut empty = two_inputs();
        empty
            .apply(Action::AddInput {
                source: SourceSpec::AudioDevice { id: "mic".into() },
                name: Some("Mic".into()),
            })
            .unwrap();
        assert!(matches!(
            empty.apply(Action::QuickPlay {
                input: Some(named("Mic")),
            }),
            Err(ActionError::NoVideo { .. })
        ));
        assert_eq!(
            empty.apply(Action::QuickPlay { input: None }),
            Err(ActionError::NoPreview)
        );
    }

    #[test]
    fn quick_play_has_its_own_effect_but_no_stinger() {
        let mut sw = with_clips(AutoPlay::default());
        sw.apply(Action::SetQuickPlay {
            effect: Effect::WipeLeft,
            duration_ms: 1000,
        })
        .unwrap();
        assert_eq!(
            sw.state().quick_play,
            QuickPlay {
                effect: Effect::WipeLeft,
                duration_ms: 1000
            }
        );
        assert!(matches!(
            sw.apply(Action::SetQuickPlay {
                effect: Effect::Stinger,
                duration_ms: 1000,
            }),
            Err(ActionError::BadQuickPlay { .. })
        ));
        let cmds = sw
            .apply(Action::QuickPlay {
                input: Some(named("A")),
            })
            .unwrap();
        assert_eq!(
            cmds[0],
            Command::Program {
                slot: Some(0),
                effect: Effect::WipeLeft,
                duration_ms: 1000
            }
        );
        // The transition buttons are untouched.
        assert_eq!(sw.state().transitions[0].effect, Effect::Fade);
    }

    #[test]
    fn quick_play_with_automatic_playback_plays_once_and_pauses_the_old() {
        let mut sw = with_clips(PLAY_AND_PAUSE);
        sw.apply(Action::CutDirect {
            input: named("Clip 1"),
        })
        .unwrap();

        let cmds = sw
            .apply(Action::QuickPlay {
                input: Some(named("Clip 2")),
            })
            .unwrap();

        let plays = cmds
            .iter()
            .filter(|c| **c == transport(3, Transport::Play))
            .count();
        assert_eq!(plays, 1, "{cmds:?}");
        assert_eq!(cmds[0], transport(3, Transport::Play));
        assert_eq!(
            cmds[cmds.len() - 2..],
            [Command::Wait { ms: 500 }, transport(2, Transport::Pause)]
        );
    }

    #[test]
    fn click_and_go_do_what_the_input_says() {
        let mut sw = with_clips(AutoPlay::default());
        let clip = || named("Clip 1");
        let state = sw.state();
        assert_eq!(state.inputs[2].click, InputPress::PreviewInput);
        assert_eq!(state.inputs[2].go, InputPress::QuickPlay);

        // Defaults: click into preview, GO quick play.
        sw.apply(Action::ClickInput { input: clip() }).unwrap();
        assert_eq!(sw.tally(), [0, 0, 2, 0]);
        let cmds = sw.apply(Action::GoInput { input: clip() }).unwrap();
        assert_eq!(cmds[0], transport(2, Transport::Play));
        assert_eq!(sw.tally(), [0, 0, 1, 0]);

        // Click straight to program; GO left as it is.
        sw.apply(Action::SetInputPress {
            input: named("A"),
            click: Some(InputPress::CutDirect),
            go: None,
        })
        .unwrap();
        sw.apply(Action::ClickInput { input: named("A") }).unwrap();
        assert_eq!(sw.tally(), [1, 0, 0, 0]);
        assert_eq!(sw.state().inputs[0].go, InputPress::QuickPlay);

        // Nothing: no change, no error.
        sw.apply(Action::SetInputPress {
            input: clip(),
            click: Some(InputPress::None),
            go: Some(InputPress::None),
        })
        .unwrap();
        assert_eq!(
            sw.apply(Action::ClickInput { input: clip() }),
            Ok(Vec::new())
        );
        assert_eq!(sw.apply(Action::GoInput { input: clip() }), Ok(Vec::new()));
        assert_eq!(sw.tally(), [1, 0, 0, 0]);
        assert!(matches!(
            sw.apply(Action::GoInput {
                input: named("Nope")
            }),
            Err(ActionError::UnknownInput { .. })
        ));
    }

    #[test]
    fn t_bar_mixes_preview_in_and_takes_at_the_end() {
        let mut sw = two_inputs();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();

        let half = sw.apply(Action::TBar { position: 0.5 }).unwrap();

        assert_eq!(
            half,
            [Command::TBar {
                slot: 1,
                position: 0.5
            }]
        );
        assert_eq!(sw.state().t_bar, 0.5);
        assert_eq!(sw.tally(), [1, 2], "nothing taken yet");

        let end = sw.apply(Action::TBar { position: 1.0 }).unwrap();

        assert!(matches!(
            end.first(),
            Some(Command::Program {
                slot: Some(1),
                effect: Effect::Cut,
                ..
            })
        ));
        assert_eq!(sw.tally(), [2, 1], "preview went to program");
        assert_eq!(sw.state().t_bar, 0.0);
    }

    #[test]
    fn sound_that_follows_video_follows_the_t_bar() {
        let mut sw = two_inputs();
        for name in ["A", "B"] {
            sw.apply(Action::SetAudioFollow {
                input: named(name),
                follow: true,
            })
            .unwrap();
        }
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        let gains = |cmds: &[Command]| -> Vec<(usize, f64, bool)> {
            cmds.iter()
                .filter_map(|c| match c {
                    Command::SetAudio {
                        slot, gain, muted, ..
                    } => Some((*slot, (*gain * 100.0).round() / 100.0, *muted)),
                    _ => None,
                })
                .collect()
        };

        // Halfway (SWI.5): program fades out as preview fades in.
        let half = sw.apply(Action::TBar { position: 0.5 }).unwrap();
        assert_eq!(gains(&half), [(0, 0.5, false), (1, 0.5, false)]);
        // Back to the start: as before.
        let back = sw.apply(Action::TBar { position: 0.0 }).unwrap();
        assert_eq!(gains(&back), [(0, 1.0, false), (1, 1.0, true)]);
        // At the end: the take, B alone.
        sw.apply(Action::TBar { position: 0.8 }).unwrap();
        let end = sw.apply(Action::TBar { position: 1.0 }).unwrap();
        assert_eq!(gains(&end), [(0, 1.0, true), (1, 1.0, false)]);
    }

    #[test]
    fn t_bar_back_to_zero_and_bad_positions() {
        let mut sw = two_inputs();
        assert_eq!(
            sw.apply(Action::TBar { position: 0.3 }),
            Err(ActionError::NoPreview)
        );
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();

        let back = sw.apply(Action::TBar { position: 0.0 }).unwrap();

        assert_eq!(
            back,
            [Command::TBar {
                slot: 1,
                position: 0.0
            }]
        );
        assert_eq!(
            sw.apply(Action::TBar { position: 1.5 }),
            Err(ActionError::BadTBar)
        );
    }

    #[test]
    fn channel_strip_is_checked_kept_and_restored() {
        let mut sw = Switcher::new(SLOTS);
        sw.apply(Action::AddInput {
            source: SourceSpec::AudioDevice { id: "mic".into() },
            name: Some("Mic".into()),
        })
        .unwrap();
        sw.apply(Action::AddInput {
            source: SourceSpec::Color {
                color: "#ff0000".into(),
            },
            name: Some("Red".into()),
        })
        .unwrap();
        let mut strip = ChannelStrip {
            gain_db: 6.0,
            delay_ms: 80,
            ..Default::default()
        };
        strip.compressor.on = true;

        let cmds = sw
            .apply(Action::SetChannelStrip {
                input: named("Mic"),
                strip,
            })
            .unwrap();

        assert_eq!(cmds, [Command::SetStrip { slot: 0, strip }]);
        assert_eq!(sw.state().inputs[0].strip, strip);
        assert!(sw
            .restore_commands()
            .contains(&Command::SetStrip { slot: 0, strip }));

        let loud = ChannelStrip {
            gain_db: 30.0,
            ..Default::default()
        };
        assert_eq!(
            sw.apply(Action::SetChannelStrip {
                input: named("Mic"),
                strip: loud,
            }),
            Err(ActionError::BadStrip {
                field: "gain_db".into()
            })
        );
        assert!(matches!(
            sw.apply(Action::SetChannelStrip {
                input: named("Red"),
                strip,
            }),
            Err(ActionError::NoAudio { .. })
        ));
    }

    #[test]
    fn a_new_input_with_sound_resets_its_channel() {
        let mut sw = Switcher::new(SLOTS);

        let cmds = sw
            .apply(Action::AddInput {
                source: SourceSpec::AudioDevice { id: "mic".into() },
                name: None,
            })
            .unwrap();

        assert!(cmds.contains(&Command::SetStrip {
            slot: 0,
            strip: ChannelStrip::default()
        }));
    }

    #[test]
    fn a_source_with_sound_gets_the_inputs_strip() {
        let mut sw = Switcher::new(SLOTS);
        sw.apply(Action::AddInput {
            source: SourceSpec::Color {
                color: "#ff0000".into(),
            },
            name: Some("Red".into()),
        })
        .unwrap();

        let cmds = sw
            .apply(Action::ReplaceSource {
                input: named("Red"),
                source: SourceSpec::AudioDevice { id: "mic".into() },
            })
            .unwrap();

        assert!(cmds.contains(&Command::SetStrip {
            slot: 0,
            strip: ChannelStrip::default()
        }));
    }

    #[test]
    fn aux_sends_and_masters_are_kept_and_restored() {
        let mut sw = two_inputs();

        let cmds = sw
            .apply(Action::SetAuxSend {
                input: named("B"),
                bus: 2,
                level: 50,
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::SetAuxSend {
                slot: 1,
                bus: 1,
                gain: fader_gain(50)
            }]
        );
        assert_eq!(sw.state().inputs[1].sends, [0, 50, 0, 0, 0, 0, 0]);

        sw.apply(Action::SetAuxBus {
            bus: 1,
            volume: 80,
            mute: true,
        })
        .unwrap();
        assert_eq!(
            sw.state().aux_buses[0],
            AuxBus {
                volume: 80,
                mute: true
            }
        );

        let restore = sw.restore_commands();
        assert!(restore.contains(&Command::SetAuxSend {
            slot: 1,
            bus: 1,
            gain: fader_gain(50)
        }));
        assert!(restore.contains(&Command::SetAuxBus {
            bus: 0,
            gain: fader_gain(80),
            mute: true,
            ramp_ms: AUDIO_RAMP_MS,
        }));
    }

    #[test]
    fn a_new_format_rebuilds_and_restores() {
        let mut sw = two_inputs();
        let upright = lmix_types::MasterFormat {
            width: 1080,
            height: 1920,
            framerate: lmix_types::Framerate::Fps25,
        };

        let cmds = sw
            .apply(Action::SetMasterFormat { format: upright })
            .unwrap();

        assert_eq!(
            cmds[0],
            Command::SetFormat {
                format: upright,
                multiview: Default::default(),
                sample_rate: 48_000
            }
        );
        assert!(cmds
            .iter()
            .any(|c| matches!(c, Command::Preview { .. }) || matches!(c, Command::Program { .. })));
        assert!(
            !cmds.iter().any(|c| matches!(c, Command::SetSlot { .. })),
            "the sources keep running"
        );
        assert_eq!(sw.state().format, upright);
        assert!(
            sw.apply(Action::SetMasterFormat { format: upright })
                .unwrap()
                .is_empty(),
            "the same format again changes nothing"
        );
        for bad in [(1081, 1920), (1080, 100), (8000, 4320)] {
            let format = lmix_types::MasterFormat {
                width: bad.0,
                height: bad.1,
                ..upright
            };
            assert!(matches!(
                sw.apply(Action::SetMasterFormat { format }),
                Err(ActionError::BadFormat { .. })
            ));
        }
        let json = r#"{"action":"set_master_format","format":{"width":1280,"height":720,"framerate":"60000/1001"}}"#;
        assert!(serde_json::from_str::<Action>(json).is_ok());
    }

    #[test]
    fn new_inputs_get_the_default_delay_and_the_monitor_its_volume() {
        let mut sw = Switcher::new(SLOTS);
        sw.apply(Action::SetDefaultDelay { ms: 40 }).unwrap();

        let cmds = sw
            .apply(Action::AddInput {
                source: SourceSpec::AudioDevice { id: "mic".into() },
                name: Some("Mic".into()),
            })
            .unwrap();

        let strip = ChannelStrip {
            delay_ms: 40,
            ..ChannelStrip::default()
        };
        assert!(cmds.contains(&Command::SetStrip { slot: 0, strip }));
        assert_eq!(sw.state().inputs[0].strip.delay_ms, 40);
        assert!(matches!(
            sw.apply(Action::SetDefaultDelay { ms: 501 }),
            Err(ActionError::BadStrip { .. })
        ));

        let cmds = sw.apply(Action::SetMonitorVolume { volume: 50 }).unwrap();
        assert_eq!(
            cmds,
            [Command::SetMonitor {
                gain: fader_gain(50)
            }]
        );
        assert_eq!(sw.state().monitor_volume, 50);
        assert!(sw.restore_commands().contains(&Command::SetMonitor {
            gain: fader_gain(50)
        }));
    }

    #[test]
    fn levels_fade_over_their_time() {
        let mut sw = two_inputs();
        sw.apply(Action::SetAuxBus {
            bus: 2,
            volume: 80,
            mute: true,
        })
        .unwrap();
        let fade = |target, volume, duration_ms| Action::FadeVolume {
            target,
            volume,
            duration_ms,
        };

        let cmds = sw
            .apply(fade(VolumeTarget::Input(named("B")), 25, 3000))
            .unwrap();
        assert_eq!(audio_of(&cmds), [(1, false, 3000)]);
        assert_eq!(
            sw.state().inputs[1].audio.volume,
            25,
            "the end value at once"
        );

        let cmds = sw.apply(fade(VolumeTarget::Bus(2), 0, 4000)).unwrap();
        assert_eq!(
            cmds,
            [Command::SetAuxBus {
                bus: 1,
                gain: 0.0,
                mute: true,
                ramp_ms: 4000,
            }],
            "mute stays"
        );

        let cmds = sw.apply(fade(VolumeTarget::Master, 50, 0)).unwrap();
        assert_eq!(
            cmds,
            [Command::SetMaster {
                gain: fader_gain(50),
                mute: false,
                ramp_ms: AUDIO_RAMP_MS,
            }],
            "no fade shorter than the click guard"
        );
        assert_eq!(sw.state().master.volume, 50);
        assert!(sw.restore_commands().contains(&Command::SetMaster {
            gain: fader_gain(50),
            mute: false,
            ramp_ms: AUDIO_RAMP_MS,
        }));

        assert!(matches!(
            sw.apply(fade(VolumeTarget::Master, 50, FADE_MAX_MS + 1)),
            Err(ActionError::BadFade { .. })
        ));
        assert!(matches!(
            sw.apply(fade(VolumeTarget::Bus(8), 50, 100)),
            Err(ActionError::BadAuxBus { .. })
        ));
        assert!(matches!(
            sw.apply(fade(VolumeTarget::Input(named("B")), 101, 100)),
            Err(ActionError::BadVolume { .. })
        ));
        let json =
            r#"{"action":"fade_volume","target":{"input":"B"},"volume":0,"duration_ms":2000}"#;
        assert_eq!(
            serde_json::from_str::<Action>(json).unwrap(),
            fade(VolumeTarget::Input(InputRef::Text("B".into())), 0, 2000)
        );
        let json = r#"{"action":"fade_volume","target":"master","volume":0,"duration_ms":2000}"#;
        assert!(serde_json::from_str::<Action>(json).is_ok());
    }

    #[test]
    fn aux_bus_numbers_are_checked() {
        let mut sw = two_inputs();
        for bus in [0, 8] {
            assert!(matches!(
                sw.apply(Action::SetAuxBus {
                    bus,
                    volume: 50,
                    mute: false
                }),
                Err(ActionError::BadAuxBus { .. })
            ));
        }
        let srt = OutputSpec::Srt {
            uri: "srt://cdn:9000".into(),
            video_kbps: 6000,
            encoding: Default::default(),
        };
        assert!(matches!(
            sw.apply(Action::AddOutput {
                spec: srt,
                source: OutputSource::Program,
                name: None,
                aux: Some(8),
            }),
            Err(ActionError::BadAuxBus { .. })
        ));
    }

    #[test]
    fn an_output_takes_its_sound_from_an_aux_bus() {
        let mut sw = two_inputs();
        let srt = OutputSpec::Srt {
            uri: "srt://cdn:9000".into(),
            video_kbps: 6000,
            encoding: Default::default(),
        };
        sw.apply(Action::AddOutput {
            spec: srt,
            source: OutputSource::Program,
            name: Some("Mix minus".into()),
            aux: Some(1),
        })
        .unwrap();

        let cmds = sw
            .apply(Action::StartOutput {
                output: OutputRef::Number(1),
            })
            .unwrap();

        assert!(matches!(
            cmds[..],
            [Command::StartOutput {
                feed: Feed::Program,
                aux: Some(0),
                ..
            }]
        ));
        assert_eq!(sw.state().outputs[0].aux, Some(1));
    }

    const PREROLL: u32 = crate::engine::flows::STINGER_PREROLL_MS;

    /// The stinger of `with_stinger` to slot `to`.
    fn sting(to: usize) -> Command {
        Command::Stinger {
            to,
            clip: LibraryClip {
                file: "sting.webm".into(),
                cut_ms: 400,
            },
        }
    }

    fn library(clips: &[(&str, u32)]) -> Command {
        Command::StingerLibrary {
            clips: clips
                .iter()
                .map(|(file, cut_ms)| LibraryClip {
                    file: (*file).into(),
                    cut_ms: *cut_ms,
                })
                .collect(),
        }
    }

    /// A and B, B in preview, A on program, and the clip "Sting" (slot 2)
    /// on button 2: 1000 ms, cut at 400 ms.
    fn with_stinger() -> Switcher {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::File {
                path: "sting.webm".into(),
            },
            name: Some("Sting".into()),
        })
        .unwrap();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        let cmds = sw
            .apply(Action::SetTransition {
                slot: TransitionSlot(2),
                effect: Effect::Stinger,
                duration_ms: 1000,
                stinger: Some(StingerSpec {
                    input: named("Sting"),
                    cut_ms: 400,
                }),
            })
            .unwrap();
        assert_eq!(
            cmds,
            [
                Command::Transport {
                    slot: 2,
                    transport: Transport::Pause
                },
                library(&[("sting.webm", 400)]),
            ],
            "the clip waits paused, and goes to the engine's library"
        );
        sw
    }

    #[test]
    fn a_stinger_takes_preview_through_the_engine() {
        let mut sw = with_stinger();

        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(2),
            })
            .unwrap();

        // The engine switches program at the cut; the switcher only waits
        // for it with what follows the picture.
        assert_eq!(
            cmds[..3],
            [
                sting(1),
                Command::Preview { slot: 0 },
                Command::Wait { ms: 400 + PREROLL },
            ]
        );
        assert!(!cmds.iter().any(|c| matches!(c, Command::Program { .. })));
        assert_eq!(sw.state().program, Some(sw.state().inputs[1].id));
        assert_eq!(sw.state().preview, Some(sw.state().inputs[0].id));
        assert_eq!(sw.apply(Action::Cut), Err(ActionError::StingerPlaying));
        assert_eq!(
            sw.apply(Action::FadeToBlack { duration_ms: 500 }),
            Err(ActionError::StingerPlaying)
        );
        assert!(!sw.state().fade_to_black, "the fade did not flip");
    }

    #[test]
    fn a_stinger_locks_switching_until_the_engine_ends_it() {
        let mut sw = with_stinger();
        sw.apply(Action::Transition {
            slot: TransitionSlot(2),
        })
        .unwrap();
        let before = sw.state();

        // Past the clip, strom may still hold the mixer.
        let refused = [
            Action::Cut,
            Action::Transition {
                slot: TransitionSlot(1),
            },
            Action::Transition {
                slot: TransitionSlot(2),
            },
            Action::TBar { position: 0.5 },
            Action::FadeToBlack { duration_ms: 500 },
            Action::CutDirect { input: named("A") },
            Action::QuickPlay { input: None },
            Action::RemoveInput { input: named("B") },
        ];
        for action in refused {
            assert_eq!(
                sw.apply(action.clone()),
                Err(ActionError::StingerPlaying),
                "{action:?}"
            );
        }
        assert_eq!(
            sw.replace(Show::default(), None),
            Err(ActionError::StingerPlaying)
        );
        assert_eq!(sw.state(), before, "nothing changed");

        sw.stinger_finished();
        sw.apply(Action::Cut).unwrap();
        assert_eq!(sw.state().program, Some(sw.state().inputs[0].id));
    }

    #[test]
    fn a_lost_stinger_end_frees_the_lock_after_strom_gives_up() {
        let sting = Action::Transition {
            slot: TransitionSlot(2),
        };
        let locked = |sw: &Switcher| sw.stinger_until.unwrap() - Instant::now();
        let limit = crate::engine::flows::STINGER_FINISH_LIMIT_MS;
        let after = Duration::from_millis(u64::from(PREROLL + limit));

        // Without a reported length, the button's.
        let mut sw = with_stinger();
        sw.apply(sting.clone()).unwrap();
        assert!(locked(&sw) <= Duration::from_millis(1000) + after);
        assert!(locked(&sw) > Duration::from_millis(900) + after);

        let mut sw = with_stinger();
        sw.set_clip_length(2, 2500);
        sw.apply(sting.clone()).unwrap();
        assert!(locked(&sw) > Duration::from_millis(2400) + after);

        // The limit counts from the engine's start of the take.
        sw.stinger_until = Some(Instant::now());
        sw.stinger_started(1);
        assert!(locked(&sw) > Duration::from_millis(2400) + after);
        assert_eq!(sw.apply(Action::Cut), Err(ActionError::StingerPlaying));

        // At the limit, switching is free without the end.
        sw.stinger_until = Some(Instant::now());
        sw.apply(Action::Cut).unwrap();

        // A take that ended before its start was confirmed stays ended.
        sw.stinger_finished();
        sw.stinger_started(2);
        assert_eq!(sw.stinger_until, None);
    }

    #[test]
    fn a_late_end_of_an_older_take_keeps_the_lock() {
        let sting = Action::Transition {
            slot: TransitionSlot(2),
        };
        let mut sw = with_stinger();
        sw.apply(sting).unwrap();
        sw.stinger_started(2);

        sw.stinger_ended(1);
        assert_eq!(sw.apply(Action::Cut), Err(ActionError::StingerPlaying));

        sw.stinger_ended(2);
        sw.apply(Action::Cut).unwrap();
    }

    #[test]
    fn a_new_mixer_ends_the_stinger() {
        let mut sw = with_stinger();
        sw.apply(Action::Transition {
            slot: TransitionSlot(2),
        })
        .unwrap();

        sw.restore_commands();

        assert_eq!(sw.stinger_until, None, "no end comes from a new mixer");
        sw.apply(Action::Cut).unwrap();
    }

    #[test]
    fn a_stinger_refuses_layouts() {
        let mut sw = with_stinger();
        sw.apply(Action::AddLayout {
            name: Some("Two".into()),
            background: Some(named("A")),
            boxes: Vec::new(),
        })
        .unwrap();
        sw.apply(Action::PreviewLayout {
            layout: named("Two"),
        })
        .unwrap();
        let refused = Err(ActionError::BadStinger {
            reason: STINGER_NO_LAYOUTS.into(),
        });
        let sting = Action::Transition {
            slot: TransitionSlot(2),
        };

        assert_eq!(sw.apply(sting.clone()), refused, "layout in preview");
        sw.apply(Action::Cut).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        assert_eq!(sw.apply(sting), refused, "layout on program");
        assert_eq!(sw.stinger_until, None, "nothing plays");
    }

    #[test]
    fn the_stinger_library_has_each_clip_once() {
        let mut sw = with_stinger();
        let set = |slot, input: &str, cut_ms| Action::SetTransition {
            slot: TransitionSlot(slot),
            effect: Effect::Stinger,
            duration_ms: 1000,
            stinger: Some(StingerSpec {
                input: named(input),
                cut_ms,
            }),
        };
        sw.apply(Action::AddInput {
            source: SourceSpec::Playlist {
                paths: vec!["wipe.mov".into(), "other.mov".into()],
            },
            name: Some("Wipe".into()),
        })
        .unwrap();

        // A playlist plays its first item; a second button on the same
        // clip with the same cut point adds nothing.
        let cmds = sw.apply(set(3, "Wipe", 300)).unwrap();
        assert!(cmds.contains(&library(&[("sting.webm", 400), ("wipe.mov", 300)])));
        let cmds = sw.apply(set(1, "Sting", 400)).unwrap();
        assert!(!cmds
            .iter()
            .any(|c| matches!(c, Command::StingerLibrary { .. })));

        // strom keeps one cut point per clip.
        assert!(matches!(
            sw.apply(set(4, "Sting", 500)),
            Err(ActionError::BadStinger { .. })
        ));

        // A new source is a new clip.
        let cmds = sw
            .apply(Action::ReplaceSource {
                input: named("Wipe"),
                source: SourceSpec::File {
                    path: "push.mov".into(),
                },
            })
            .unwrap();
        assert!(cmds.contains(&library(&[("sting.webm", 400), ("push.mov", 300)])));
    }

    #[test]
    fn stinger_settings_are_checked() {
        let mut sw = with_stinger();
        let set = |input: &str, cut_ms| Action::SetTransition {
            slot: TransitionSlot(1),
            effect: Effect::Stinger,
            duration_ms: 1000,
            stinger: Some(StingerSpec {
                input: named(input),
                cut_ms,
            }),
        };

        for action in [set("A", 400), set("Sting", 1200)] {
            assert!(matches!(
                sw.apply(action),
                Err(ActionError::BadStinger { .. })
            ));
        }
        assert!(matches!(
            sw.apply(Action::SetTransition {
                slot: TransitionSlot(1),
                effect: Effect::Stinger,
                duration_ms: 1000,
                stinger: None,
            }),
            Err(ActionError::BadStinger { .. })
        ));
    }

    #[test]
    fn stinger_clips_wait_paused_and_leave_with_their_input() {
        let mut sw = with_stinger();

        let restored = sw.restore_commands();
        assert!(restored.contains(&Command::Transport {
            slot: 2,
            transport: Transport::Pause
        }));
        assert!(restored.contains(&library(&[("sting.webm", 400)])));

        let cmds = sw
            .apply(Action::RemoveInput {
                input: named("Sting"),
            })
            .unwrap();
        let button = sw.state().transitions[1];
        assert_eq!((button.effect, button.stinger), (Effect::Cut, None));
        assert!(cmds.contains(&library(&[])));
    }

    #[test]
    fn looks_need_an_engine_that_can_apply_them() {
        let mut sw = two_inputs();
        let warm = Look {
            temperature: 0.3,
            ..Default::default()
        };

        assert_eq!(
            sw.apply(Action::SetLook {
                input: named("A"),
                look: warm,
            }),
            Err(ActionError::NoLooks)
        );

        sw.set_looks_available(true);
        let cmds = sw
            .apply(Action::SetLook {
                input: named("A"),
                look: warm,
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::SetLook {
                slot: 0,
                look: warm
            }]
        );
        assert_eq!(sw.state().inputs[0].look, warm);
        assert!(sw.state().looks_available);
        assert!(sw.restore_commands().contains(&Command::SetLook {
            slot: 0,
            look: warm
        }));

        let wild = Look {
            gamma: 5.0,
            ..Default::default()
        };
        assert_eq!(
            sw.apply(Action::SetLook {
                input: named("A"),
                look: wild,
            }),
            Err(ActionError::BadLook {
                field: "gamma".into()
            })
        );
    }

    #[test]
    fn a_new_input_clears_the_look_of_its_slot() {
        let mut sw = Switcher::new(SLOTS);
        sw.set_looks_available(true);

        let cmds = sw
            .apply(Action::AddInput {
                source: pattern(TestPattern::Ball),
                name: None,
            })
            .unwrap();

        assert!(cmds.contains(&Command::SetLook {
            slot: 0,
            look: Look::default()
        }));
    }

    const TOP_RIGHT: BoxRect = BoxRect {
        x: 0.5,
        y: 0.0,
        w: 0.5,
        h: 0.5,
    };

    /// Shows the middle of an input.
    const ZOOM: BoxCrop = BoxCrop {
        left: 0.25,
        top: 0.25,
        right: 0.25,
        bottom: 0.25,
    };

    /// A, B; layout 1 = A behind, B top right zoomed in; A on program.
    fn with_layout() -> Switcher {
        let mut sw = two_inputs();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        let cmds = sw
            .apply(Action::AddLayout {
                name: Some("Interview".into()),
                background: Some(named("A")),
                boxes: vec![BoxSpec {
                    rect: TOP_RIGHT,
                    input: named("B"),
                    border_px: 4,
                    crop: ZOOM,
                    locked: true,
                    hidden: false,
                }],
            })
            .unwrap();
        // The editor's lock stays with the layout, the engine never sees it.
        assert!(sw.state().layouts[0].boxes[0].locked);
        assert_eq!(
            cmds,
            [Command::SetLayout {
                pip: 0,
                background: Some(0),
                boxes: vec![EngineBox {
                    rect: TOP_RIGHT,
                    slot: 1,
                    border_px: 4,
                    crop: ZOOM,
                }]
            }]
        );
        sw
    }

    #[test]
    fn a_layout_goes_through_preview_to_program() {
        let mut sw = with_layout();
        let layout = sw.state().layouts[0].id;

        let cmds = sw
            .apply(Action::PreviewLayout {
                layout: InputRef::Number(1),
            })
            .unwrap();
        assert_eq!(cmds, [Command::PreviewLayout { pip: 0 }]);
        assert_eq!(sw.tally(), [1, 2], "A on air, B only in the layout");

        let cmds = sw.apply(Action::Cut).unwrap();
        assert_eq!(
            cmds[..2],
            [
                Command::ProgramLayout {
                    pip: 0,
                    effect: Effect::Cut,
                    duration_ms: 0
                },
                Command::Preview { slot: 0 },
            ]
        );
        assert_eq!(sw.state().program, Some(layout));
        assert_eq!(sw.tally(), [1, 1], "both inputs of the layout on air");

        // Restore sets the composition before it goes on program.
        let restore = sw.restore_commands();
        let set = restore
            .iter()
            .position(|c| matches!(c, Command::SetLayout { pip: 0, .. }))
            .unwrap();
        let program = restore
            .iter()
            .position(|c| matches!(c, Command::ProgramLayout { pip: 0, .. }))
            .unwrap();
        assert!(set < program);

        assert!(matches!(
            sw.apply(Action::RemoveLayout {
                layout: InputRef::Number(1)
            }),
            Err(ActionError::BadLayout { .. })
        ));
    }

    #[test]
    fn the_t_bar_mixes_inputs_only() {
        let mut sw = with_layout();
        sw.apply(Action::PreviewLayout {
            layout: InputRef::Text("Interview".into()),
        })
        .unwrap();

        assert!(matches!(
            sw.apply(Action::TBar { position: 0.5 }),
            Err(ActionError::BadLayout { .. })
        ));
    }

    #[test]
    fn a_layout_box_hides_and_shows() {
        let mut sw = with_layout();
        sw.apply(Action::PreviewLayout {
            layout: InputRef::Number(1),
        })
        .unwrap();
        sw.apply(Action::Cut).unwrap();
        assert_eq!(sw.tally(), [1, 1], "A and B in the layout on air");
        let show = |shown| Action::ShowBox {
            layout: InputRef::Number(1),
            number: 1,
            shown,
        };

        // LAY.2: hidden, the box stays in the layout but is not drawn.
        let cmds = sw.apply(show(BoxShown::Hidden)).unwrap();
        assert!(matches!(&cmds[..], [Command::SetLayout { boxes, .. }] if boxes.is_empty()));
        assert!(sw.state().layouts[0].boxes[0].hidden);
        assert_eq!(sw.tally(), [1, 0], "a hidden box is not on air");
        let cmds = sw.apply(show(BoxShown::Toggle)).unwrap();
        assert!(matches!(&cmds[..], [Command::SetLayout { boxes, .. }] if boxes.len() == 1));
        assert_eq!(sw.tally(), [1, 1]);
        assert!(matches!(
            sw.apply(Action::ShowBox {
                layout: InputRef::Number(1),
                number: 2,
                shown: BoxShown::Shown
            }),
            Err(ActionError::BadLayout { .. })
        ));
    }

    #[test]
    fn layouts_are_checked() {
        let mut sw = with_layout();
        let spec = |input: &str, rect| BoxSpec {
            rect,
            input: named(input),
            border_px: 0,
            crop: BoxCrop::default(),
            locked: false,
            hidden: false,
        };
        let nothing_left = BoxSpec {
            crop: BoxCrop {
                left: 0.6,
                right: 0.4,
                ..BoxCrop::default()
            },
            ..spec("B", TOP_RIGHT)
        };
        let outside = BoxRect {
            x: 0.8,
            ..TOP_RIGHT
        };

        for boxes in [
            vec![spec("A", TOP_RIGHT)],
            vec![spec("B", outside)],
            vec![spec("B", TOP_RIGHT), spec("B", TOP_RIGHT)],
            vec![nothing_left],
        ] {
            assert!(matches!(
                sw.apply(Action::SetLayout {
                    layout: InputRef::Number(1),
                    background: Some(named("A")),
                    boxes,
                }),
                Err(ActionError::BadLayout { .. })
            ));
        }
        for _ in 1..LAYOUTS {
            sw.apply(Action::AddLayout {
                name: None,
                background: None,
                boxes: Vec::new(),
            })
            .unwrap();
        }
        assert_eq!(
            sw.apply(Action::AddLayout {
                name: None,
                background: None,
                boxes: Vec::new(),
            }),
            Err(ActionError::LayoutsFull { max: LAYOUTS })
        );
    }

    #[test]
    fn a_removed_input_leaves_its_layouts() {
        let mut sw = with_layout();

        let cmds = sw.apply(Action::RemoveInput { input: named("B") }).unwrap();

        assert!(cmds.contains(&Command::SetLayout {
            pip: 0,
            background: Some(0),
            boxes: Vec::new()
        }));
        assert!(sw.state().layouts[0].boxes.is_empty());
    }

    fn shortcut(label: &str, key: Option<&str>, actions: Vec<Action>) -> ShortcutSpec {
        ShortcutSpec {
            label: label.into(),
            key: key.map(Into::into),
            web: true,
            color: None,
            midi: None,
            actions,
            all_shows: false,
        }
    }

    #[test]
    fn midi_bindings_are_unique_and_found() {
        let mut sw = two_inputs();
        let note = |channel| MidiBinding {
            kind: MidiKind::Note,
            channel,
            number: 36,
        };
        let spec = |label: &str, midi| ShortcutSpec {
            midi: Some(midi),
            ..shortcut(label, None, vec![Action::Cut])
        };
        sw.apply(Action::AddShortcut {
            shortcut: spec("Pad", note(Some(10))),
        })
        .unwrap();
        // Any channel overlaps channel 10.
        assert!(matches!(
            sw.apply(Action::AddShortcut {
                shortcut: spec("Other", note(None)),
            }),
            Err(ActionError::BadShortcut { .. })
        ));
        sw.apply(Action::AddShortcut {
            shortcut: spec("Other", note(Some(1))),
        })
        .unwrap();
        let hit = MidiMessage {
            kind: MidiKind::Note,
            channel: 10,
            number: 36,
            value: 90,
        };
        assert_eq!(sw.midi_shortcut(&hit), Some(sw.state().shortcuts[0].id));

        let fader = MidiBinding {
            kind: MidiKind::Cc,
            channel: None,
            number: 7,
        };
        assert!(matches!(
            sw.apply(Action::SetMidiTbar {
                midi: Some(note(Some(2)))
            }),
            Err(ActionError::BadMidi { .. })
        ));
        sw.apply(Action::SetMidiTbar { midi: Some(fader) }).unwrap();
        assert_eq!(sw.state().midi_tbar, Some(fader));
        assert!(sw.midi_moves_tbar(&MidiMessage {
            kind: MidiKind::Cc,
            channel: 3,
            number: 7,
            value: 10,
        }));
        assert!(matches!(
            sw.apply(Action::AddShortcut {
                shortcut: spec(
                    "Fader",
                    MidiBinding {
                        channel: Some(1),
                        ..fader
                    }
                ),
            }),
            Err(ActionError::BadShortcut { .. })
        ));
    }

    #[test]
    fn shortcuts_are_kept_and_checked() {
        let mut sw = two_inputs();
        let take_b = vec![Action::PreviewInput { input: named("B") }, Action::Cut];
        sw.apply(Action::AddShortcut {
            shortcut: shortcut("B on air", Some("F2"), take_b.clone()),
        })
        .unwrap();

        let state = sw.state();
        assert_eq!(state.shortcuts[0].label, "B on air");
        assert_eq!(state.shortcuts[0].number, 1);
        assert_eq!(
            sw.shortcut_actions(&InputRef::Text("B on air".into()))
                .unwrap(),
            take_b
        );

        for spec in [
            shortcut("Other", Some("F2"), vec![Action::Cut]),
            shortcut(" ", None, vec![Action::Cut]),
            shortcut("Empty", None, Vec::new()),
            shortcut(
                "Loop",
                None,
                vec![Action::RunShortcut {
                    target: InputRef::Number(1),
                }],
            ),
        ] {
            assert!(matches!(
                sw.apply(Action::AddShortcut { shortcut: spec }),
                Err(ActionError::BadShortcut { .. })
            ));
        }

        // Its own key is fine when changing it.
        sw.apply(Action::SetShortcut {
            target: InputRef::Number(1),
            shortcut: shortcut("B now", Some("F2"), vec![Action::Cut]),
        })
        .unwrap();
        sw.apply(Action::RemoveShortcut {
            target: InputRef::Text("B now".into()),
        })
        .unwrap();
        assert!(sw.state().shortcuts.is_empty());
    }

    #[test]
    fn shortcuts_for_all_shows_stay_and_give_way() {
        let mut sw = two_inputs();
        let everywhere = |label: &str, key: &str, note: u8| ShortcutSpec {
            all_shows: true,
            midi: Some(MidiBinding {
                kind: MidiKind::Note,
                channel: Some(1),
                number: note,
            }),
            ..shortcut(label, Some(key), vec![Action::Cut])
        };
        sw.apply(Action::AddShortcut {
            shortcut: everywhere("Cut", "F1", 36),
        })
        .unwrap();
        assert!(sw.take_all_shows_changed());
        // A show's shortcut may take the same key: it wins.
        sw.apply(Action::AddShortcut {
            shortcut: shortcut("Show cut", Some("F1"), vec![Action::Cut]),
        })
        .unwrap();
        let state = sw.state();
        assert_eq!(
            state
                .shortcuts
                .iter()
                .map(|s| (s.number, s.all_shows, s.key_shadowed, s.midi_shadowed))
                .collect::<Vec<_>>(),
            [(1, false, false, false), (2, true, true, false)]
        );
        // Two for all shows may not share a key.
        assert!(matches!(
            sw.apply(Action::AddShortcut {
                shortcut: everywhere("Again", "F1", 37)
            }),
            Err(ActionError::BadShortcut { .. })
        ));
        let note = |n| MidiMessage {
            kind: MidiKind::Note,
            channel: 1,
            number: n,
            value: 100,
        };
        assert_eq!(sw.midi_shortcut(&note(36)), Some(state.shortcuts[1].id));

        // Moved into the show: out of the settings, with a new id.
        let id = state.shortcuts[1].id;
        sw.apply(Action::SetShortcut {
            target: InputRef::Number(2),
            shortcut: ShortcutSpec {
                key: Some("F3".into()),
                ..shortcut("Cut", None, vec![Action::Cut])
            },
        })
        .unwrap();
        assert!(sw.take_all_shows_changed());
        assert!(sw.all_shows_shortcuts().is_empty());
        assert_ne!(sw.state().shortcuts[1].id, id);
        assert!(!sw.state().shortcuts[1].all_shows);
        // Settings holding an id the show has (saved before a move): renamed.
        let in_show = sw.state().shortcuts[1].id;
        sw.set_all_shows_shortcuts(vec![crate::show::ShowShortcut {
            id: in_show,
            label: "Twin".into(),
            key: None,
            web: false,
            color: None,
            midi: None,
            actions: vec![Action::Cut],
        }]);
        assert_ne!(sw.all_shows_shortcuts()[0].id, in_show);
        assert!(sw.take_all_shows_changed());

        // A new show keeps the ones for all shows.
        sw.set_all_shows_shortcuts(vec![crate::show::ShowShortcut {
            id,
            label: "Cut".into(),
            key: Some("F1".into()),
            web: false,
            color: None,
            midi: None,
            actions: vec![Action::Cut],
        }]);
        sw.replace(crate::show::Show::default(), None).unwrap();
        let state = sw.state();
        assert_eq!(state.shortcuts.len(), 1);
        assert!(state.shortcuts[0].all_shows && !state.shortcuts[0].key_shadowed);
        sw.apply(Action::RemoveShortcut {
            target: InputRef::Number(1),
        })
        .unwrap();
        assert!(sw.take_all_shows_changed());
        assert!(sw.all_shows_shortcuts().is_empty());
    }

    #[test]
    fn a_closed_input_comes_back_with_its_settings() {
        let mut sw = two_inputs();
        sw.apply(Action::SetVolume {
            input: named("A"),
            volume: 40,
        })
        .unwrap();
        let a = sw.state().inputs[0].clone();
        sw.apply(Action::RemoveInput { input: named("A") }).unwrap();
        assert_eq!(sw.state().restorable.as_deref(), Some("A"));

        let cmds = sw.apply(Action::RestoreInput).unwrap();

        let back = &sw.state().inputs[0];
        assert_eq!((back.id, back.number, back.slot), (a.id, 1, 0));
        assert_eq!(back.audio.volume, 40);
        assert!(cmds.contains(&Command::SetSlot {
            slot: 0,
            source: Some(a.source.clone())
        }));
        assert_eq!(sw.state().restorable, None);
        assert_eq!(
            sw.apply(Action::RestoreInput),
            Err(ActionError::NothingToRestore)
        );
    }

    #[test]
    fn the_lock_refuses_closing_and_stopping() {
        let mut sw = two_inputs();
        sw.apply(Action::SetLock { on: true }).unwrap();
        assert!(sw.state().locked);

        assert_eq!(
            sw.apply(Action::RemoveInput { input: named("A") }),
            Err(ActionError::Locked)
        );
        assert_eq!(
            sw.apply(Action::StopOutput {
                output: OutputRef::Number(1)
            }),
            Err(ActionError::Locked)
        );
        // Switching still works.
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();

        sw.apply(Action::SetLock { on: false }).unwrap();
        sw.apply(Action::RemoveInput { input: named("A") }).unwrap();
    }

    fn cue(name: &str, action: Action) -> Cue {
        Cue::actions(name, vec![action])
    }

    /// Actions the outs ask to run.
    fn ran(out: &[Out]) -> Vec<Action> {
        out.iter()
            .flat_map(|o| match o {
                Out::Actions { actions, .. } => actions.clone(),
                Out::Event { .. } | Out::Panic { .. } | Out::StopPlayback { .. } => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn sounds_play_in_the_shows_zones_which_reach_the_engine() {
        let mut sw = two_inputs();
        let play = Action::PlaySound {
            id: "door".into(),
            sound: lmix_types::SoundCue {
                file: "/fx/door.wav".into(),
                ..Default::default()
            },
        };
        assert!(
            matches!(sw.apply(play.clone()), Err(ActionError::BadSound { .. })),
            "no zones yet"
        );
        let hall = Action::SetZone {
            zone: lmix_types::Zone {
                name: "Hall".into(),
                channels: 2,
            },
        };
        let zones_sent = |commands: &[Command]| {
            commands.iter().any(
                |c| matches!(c, Command::SetPlayback { playback } if playback.zones.len() == 1),
            )
        };
        assert!(zones_sent(&sw.apply(hall.clone()).unwrap()));
        assert!(
            !zones_sent(&sw.apply(hall).unwrap()),
            "unchanged zones are not sent again"
        );
        assert!(zones_sent(&sw.restore_commands()), "a restore sends them");
        let upright = lmix_types::MasterFormat {
            width: 1080,
            height: 1920,
            ..Default::default()
        };
        assert!(
            !zones_sent(
                &sw.apply(Action::SetMasterFormat { format: upright })
                    .unwrap()
            ),
            "a new mixer leaves the sounds playing"
        );

        let commands = sw.apply(play).unwrap();
        assert!(commands.iter().any(|c| matches!(
            c,
            Command::PlaySound { id, sound } if id == "door" && sound.uri == "file:///fx/door.wav"
        )));
        assert!(sw
            .apply(Action::SetZoneLevel {
                zone: "Foyer".into(),
                gain_db: Some(-6.0),
                mute: None,
                duration_ms: 0,
            })
            .is_err());
    }

    #[test]
    fn pictures_show_on_the_shows_stages_which_reach_the_engine() {
        let mut sw = two_inputs();
        let show = Action::ShowPicture {
            id: "forest".into(),
            picture: lmix_types::PictureCue {
                file: "/pics/forest.png".into(),
                stage: "Back".into(),
                layer: 1,
                opacity: 1.0,
                fit: Default::default(),
                end: Default::default(),
                fade_in_ms: 0,
                fade_out_ms: 0,
                repeat: Default::default(),
            },
        };
        assert!(matches!(
            sw.apply(show.clone()),
            Err(ActionError::BadStage { .. })
        ));
        let commands = sw
            .apply(Action::SetStage {
                stage: lmix_types::Stage {
                    name: "Back".into(),
                    width: 1920,
                    height: 1080,
                },
            })
            .unwrap();
        assert!(commands.iter().any(|c| matches!(
            c,
            Command::SetPlayback { playback } if playback.stages.len() == 1 && playback.zones.is_empty()
        )));
        let screen = lmix_types::StageOutput {
            display: lmix_types::DisplayMode::Fullscreen,
        };
        let commands = sw
            .apply(Action::SetStagePatch {
                patch: lmix_types::StagePatch {
                    stage: "Back".into(),
                    outputs: vec![screen],
                },
            })
            .unwrap();
        assert!(commands.iter().any(|c| matches!(
            c,
            Command::SetPlayback { playback } if playback.stages[0].outputs == [screen]
        )));
        assert_eq!(sw.state().stages[0].outputs, [screen]);
        assert_eq!(
            sw.target_settings().stage_patches.len(),
            1,
            "kept per machine"
        );

        let commands = sw.apply(show).unwrap();
        assert!(commands.iter().any(|c| matches!(
            c,
            Command::ShowPicture { id, picture } if id == "forest" && picture.stage == 0
                && picture.uri == "file:///pics/forest.png"
        )));
        assert!(sw
            .apply(Action::SetStageLevel {
                stage: "Back".into(),
                brightness: 2.0,
                duration_ms: 0,
            })
            .is_err());
    }

    #[test]
    fn the_standby_sound_or_picture_is_held_ready() {
        let mut sw = two_inputs();
        sw.apply(Action::SetZone {
            zone: lmix_types::Zone {
                name: "Hall".into(),
                channels: 2,
            },
        })
        .unwrap();
        let sound = |file: &str| Cue {
            sound: Some(lmix_types::SoundCue {
                file: file.into(),
                ..Default::default()
            }),
            ..Cue::empty(CueKind::Sound)
        };
        let wait = Cue {
            duration_ms: Some(100),
            ..Cue::empty(CueKind::Wait)
        };
        let ready = |commands: &[Command]| {
            commands.iter().find_map(|c| match c {
                Command::SetPreload { preload } => Some(
                    preload
                        .sounds
                        .iter()
                        .map(|(_, s)| s.uri.clone())
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
        };
        let commands = sw
            .apply(Action::AddCueList {
                name: "Sound".into(),
                cues: vec![sound("/fx/door.wav"), wait, sound("/fx/rain.wav")],
                source: None,
            })
            .unwrap();
        assert_eq!(
            ready(&commands),
            Some(vec!["file:///fx/door.wav".to_string()]),
            "the standby cue"
        );
        assert_eq!(sw.preload_commands(), None, "unchanged: not sent again");

        let t = tokio::time::Instant::now();
        sw.cue_go(None, t, None).unwrap();
        let next = sw.preload_commands().map(|c| ready(&[c]));
        assert_eq!(next, Some(Some(vec![])), "a wait is standby: nothing held");
        sw.cue_go(None, t + Duration::from_secs(1), None).unwrap();
        let next = sw.preload_commands().map(|c| ready(&[c]));
        assert_eq!(next, Some(Some(vec!["file:///fx/rain.wav".to_string()])));
        assert!(
            ready(&sw.restore_commands()).is_some(),
            "a restore holds it ready again"
        );
    }

    #[test]
    fn sound_cues_are_checked_when_saved() {
        let mut sw = two_inputs();
        let sound = |file: &str| Cue {
            sound: Some(lmix_types::SoundCue {
                file: file.into(),
                ..Default::default()
            }),
            ..Cue::empty(CueKind::Sound)
        };
        let add = |sw: &mut Switcher, cues| {
            sw.apply(Action::AddCueList {
                name: "Sound".into(),
                cues,
                source: None,
            })
        };
        assert!(add(&mut sw, vec![sound("/fx/rain.flac")]).is_ok());
        assert!(add(&mut sw, vec![sound("rain.flac")]).is_err(), "relative");
        assert!(
            add(&mut sw, vec![Cue::empty(CueKind::Sound)]).is_err(),
            "no sound"
        );
        assert!(
            add(&mut sw, vec![Cue::empty(CueKind::Fade)]).is_err(),
            "no fade"
        );
    }

    #[test]
    fn cue_lists_go_standby_and_edit() {
        let mut sw = two_inputs();
        let list = InputRef::Text("Opening".into());
        let t = tokio::time::Instant::now();
        sw.apply(Action::SetGoGuard { ms: 0 }).unwrap();
        sw.apply(Action::AddCueList {
            name: "Opening".into(),
            cues: vec![
                cue("A", Action::CutDirect { input: named("A") }),
                cue("B", Action::CutDirect { input: named("B") }),
                cue("Black", Action::FadeToBlack { duration_ms: 500 }),
            ],
            source: None,
        })
        .unwrap();
        assert_eq!(sw.state().cue_lists[0].standby, 1);
        assert!(sw.state().cue_lists[0].active, "the first list is active");

        // Without a list: the active one.
        let out = sw.cue_go(None, t, None).unwrap();
        assert_eq!(ran(&out), [Action::CutDirect { input: named("A") }]);
        sw.cue_go(Some(&list), t, None).unwrap();
        assert_eq!(sw.state().cue_lists[0].standby, 3);

        // Back moves standby only; nothing runs.
        sw.apply(Action::CueStandby {
            list: Some(list.clone()),
            to: lmix_types::StandbyMove::Previous,
        })
        .unwrap();
        assert_eq!(sw.state().cue_lists[0].standby, 2);

        // Editing keeps the place, at most one past the last cue.
        sw.apply(Action::CueStandby {
            list: None,
            to: lmix_types::StandbyMove::Index(3),
        })
        .unwrap();
        sw.apply(Action::SetCueList {
            list: list.clone(),
            name: "Opening".into(),
            cues: vec![
                cue("A", Action::CutDirect { input: named("A") }),
                cue("B", Action::CutDirect { input: named("B") }),
            ],
            source: None,
        })
        .unwrap();
        assert_eq!(sw.state().cue_lists[0].standby, 3);
        assert!(matches!(
            sw.cue_go(None, t, None),
            Err(ActionError::BadCueList { .. })
        ));
    }

    #[test]
    fn cues_by_standby_and_lists_by_active() {
        let mut sw = two_inputs();
        let t = tokio::time::Instant::now();
        for name in ["Main", "Side"] {
            sw.apply(Action::AddCueList {
                name: name.into(),
                cues: vec![
                    cue("A", Action::CutDirect { input: named("A") }),
                    cue("B", Action::CutDirect { input: named("B") }),
                ],
                source: None,
            })
            .unwrap();
        }
        sw.apply(Action::SetActiveCueList {
            list: InputRef::Text("Side".into()),
        })
        .unwrap();
        sw.apply(Action::CueStandby {
            list: Some(InputRef::Text("active".into())),
            to: lmix_types::StandbyMove::Next,
        })
        .unwrap();
        assert_eq!(sw.state().cue_lists[1].standby, 2, "active = Side");

        let out = sw.cue_start(None, "standby", t).unwrap();
        assert_eq!(ran(&out), [Action::CutDirect { input: named("B") }]);
        assert_eq!(sw.state().cue_lists[1].standby, 2, "standby stays");
        assert!(sw.cue_panic(t, Some(20_000)).is_err(), "panic time 0–10 s");
    }

    #[test]
    fn cue_lists_are_checked() {
        let mut sw = two_inputs();
        let add = |sw: &mut Switcher, cues: Vec<Cue>| {
            sw.apply(Action::AddCueList {
                name: "L".into(),
                cues,
                source: None,
            })
        };
        let refused =
            |r: Result<Vec<Command>, ActionError>| matches!(r, Err(ActionError::BadCueList { .. }));
        let mut wait = Cue::empty(CueKind::Wait);
        wait.duration_ms = Some(1005);
        let mut numbered = cue("A", Action::Cut);
        numbered.number = "1".into();
        let mut deep = cue("leaf", Action::Cut);
        for _ in 0..4 {
            deep = Cue {
                children: vec![deep],
                ..Cue::empty(CueKind::Group)
            };
        }
        let mut stray = Cue::empty(CueKind::Note);
        stray.duration_ms = Some(100);

        assert!(refused(add(
            &mut sw,
            vec![cue("Loop", Action::CueGo { list: None })]
        )));
        assert!(refused(add(&mut sw, vec![wait])), "waits in steps of 10 ms");
        assert!(
            refused(add(&mut sw, vec![Cue::empty(CueKind::Wait)])),
            "a wait needs a duration"
        );
        assert!(
            refused(add(&mut sw, vec![Cue::empty(CueKind::Start)])),
            "a start needs a target"
        );
        assert!(
            refused(add(&mut sw, vec![deep])),
            "at most three group levels"
        );
        assert!(refused(add(&mut sw, vec![stray])), "fields of another kind");
        add(&mut sw, vec![numbered.clone()]).unwrap();
        let mut again = numbered;
        again.id = lmix_types::CueId::new();
        assert!(
            refused(add(&mut sw, vec![again])),
            "numbers are unique in the show"
        );
        assert!(refused(sw.apply(Action::SetGoGuard { ms: 9000 })));
    }

    #[test]
    fn clip_end_triggers_need_a_clip_and_fire_at_the_end() {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::File {
                path: "/clips/intro.mp4".into(),
            },
            name: Some("Intro".into()),
        })
        .unwrap();
        let at_end = |before_ms| Trigger {
            on: TriggerOn::ClipEnd,
            delay_ms: 0,
            before_ms,
            at_ms: None,
            actions: vec![Action::Cut],
        };
        assert!(matches!(
            sw.apply(Action::SetTriggers {
                input: named("A"),
                triggers: vec![at_end(None)],
            }),
            Err(ActionError::BadTriggers { .. })
        ));
        assert!(matches!(
            sw.apply(Action::SetTriggers {
                input: named("Intro"),
                triggers: vec![Trigger {
                    on: TriggerOn::Program,
                    ..at_end(Some(500))
                }],
            }),
            Err(ActionError::BadTriggers { .. })
        ));
        sw.apply(Action::SetTriggers {
            input: named("Intro"),
            triggers: vec![at_end(None), at_end(Some(3_000))],
        })
        .unwrap();

        let slot = sw.state().inputs[2].slot;
        let ended = crate::clips::ClipChange {
            ended: true,
            ..Default::default()
        };
        let counting = crate::clips::ClipChange {
            left_ms: Some((3_100, 2_900)),
            played_ms: Some((6_900, 7_100)),
            ..Default::default()
        };
        assert_eq!(sw.clip_triggers(slot, &ended).len(), 1);
        assert_eq!(sw.clip_triggers(slot, &counting).len(), 1);
        // At 7 s of playing time.
        sw.apply(Action::SetTriggers {
            input: named("Intro"),
            triggers: vec![Trigger {
                on: TriggerOn::PlayTime,
                at_ms: Some(7_000),
                ..at_end(None)
            }],
        })
        .unwrap();
        assert_eq!(sw.clip_triggers(slot, &counting).len(), 1);
        assert!(sw.clip_triggers(slot, &ended).is_empty());
        for wrong in [
            Trigger {
                on: TriggerOn::PlayTime,
                ..at_end(None)
            },
            Trigger {
                at_ms: Some(1),
                ..at_end(None)
            },
        ] {
            assert!(sw
                .apply(Action::SetTriggers {
                    input: named("Intro"),
                    triggers: vec![wrong],
                })
                .is_err());
        }
        assert!(sw
            .clip_triggers(sw.state().inputs[0].slot, &ended)
            .is_empty());
    }

    #[test]
    fn triggers_fire_for_what_went_on_or_off_air() {
        let mut sw = two_inputs();
        sw.apply(Action::SetTriggers {
            input: named("B"),
            triggers: vec![
                Trigger {
                    on: TriggerOn::Program,
                    delay_ms: 1000,
                    before_ms: None,
                    at_ms: None,
                    actions: vec![Action::FadeToBlack { duration_ms: 500 }],
                },
                Trigger {
                    on: TriggerOn::ProgramOff,
                    delay_ms: 0,
                    before_ms: None,
                    at_ms: None,
                    actions: vec![Action::Cut],
                },
            ],
        })
        .unwrap();

        let before = sw.on_air();
        sw.apply(Action::CutDirect { input: named("B") }).unwrap();
        assert_eq!(
            sw.fired_triggers(&before),
            [(1000, vec![Action::FadeToBlack { duration_ms: 500 }])]
        );

        let before = sw.on_air();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        assert_eq!(sw.fired_triggers(&before), [(0, vec![Action::Cut])]);

        assert!(matches!(
            sw.apply(Action::SetTriggers {
                input: named("A"),
                triggers: vec![Trigger {
                    on: TriggerOn::Preview,
                    delay_ms: 60_000,
                    before_ms: None,
                    at_ms: None,
                    actions: vec![Action::Cut],
                }],
            }),
            Err(ActionError::BadTriggers { .. })
        ));
    }

    #[test]
    fn whip_inputs_carry_their_ingest_links() {
        let mut sw = Switcher::new(SLOTS);
        sw.set_ingest_base("http://studio:8080/".into());
        sw.apply(Action::AddInput {
            source: SourceSpec::Whip {
                endpoint: "guest".into(),
            },
            name: None,
        })
        .unwrap();

        let ingest = sw.state().inputs[0].ingest.clone().unwrap();
        assert_eq!(ingest.whip_url, "http://studio:8080/whip/guest");
        assert_eq!(ingest.browser_url, "http://studio:8080/player/whip-ingest");
    }

    #[test]
    fn outputs_start_stop_and_restart_after_engine_loss() {
        let mut sw = Switcher::new(SLOTS);
        let srt = OutputSpec::Srt {
            uri: "srt://cdn:9000".into(),
            video_kbps: 6000,
            encoding: Default::default(),
        };
        sw.apply(Action::AddOutput {
            spec: srt.clone(),
            source: OutputSource::Program,
            name: Some("Stream".into()),
            aux: None,
        })
        .unwrap();
        let id = sw.state().outputs[0].id;

        let cmds = sw
            .apply(Action::StartOutput {
                output: OutputRef::Text("Stream".into()),
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::StartOutput {
                id,
                spec: srt.clone(),
                feed: Feed::Program,
                aux: None,
            }]
        );
        sw.set_output_status(id, OutputStatus::Running { since_ms: 1 });

        // Engine restart: running outputs come back, stopped ones do not.
        assert!(sw.restore_commands().contains(&Command::StartOutput {
            id,
            spec: srt,
            feed: Feed::Program,
            aux: None,
        }));

        let cmds = sw
            .apply(Action::RemoveOutput {
                output: OutputRef::Number(1),
            })
            .unwrap();
        assert_eq!(cmds, [Command::StopOutput { id }]);
        assert!(sw.state().outputs.is_empty());
        let err = sw
            .apply(Action::StopOutput {
                output: OutputRef::Number(1),
            })
            .unwrap_err();
        assert!(matches!(err, ActionError::UnknownOutput { .. }));
    }

    #[test]
    fn the_sample_rate_rebuilds_the_mixer_only() {
        let mut sw = two_inputs();
        let cmds = sw.apply(Action::SetSampleRate { hz: 44_100 }).unwrap();
        assert!(matches!(
            cmds[0],
            Command::SetFormat {
                sample_rate: 44_100,
                ..
            }
        ));
        assert!(
            !cmds.iter().any(|c| matches!(c, Command::SetSlot { .. })),
            "the sources keep running (the channel strips resample)"
        );
        assert_eq!(sw.state().sample_rate, 44_100);
        assert!(sw
            .apply(Action::SetSampleRate { hz: 44_100 })
            .unwrap()
            .is_empty());
        assert!(matches!(
            sw.apply(Action::SetSampleRate { hz: 96_000 }),
            Err(ActionError::BadSampleRate { hz: 96_000 })
        ));
    }

    #[test]
    fn a_multiview_layout_rebuilds_the_mixer_only() {
        let mut sw = two_inputs();
        let layout = lmix_types::MultiviewLayout {
            program: lmix_types::ProgramSide::Left,
            meters: lmix_types::MultiviewMeters::Hidden,
        };

        let cmds = sw.apply(Action::SetMultiview { layout }).unwrap();

        assert_eq!(
            cmds[0],
            Command::SetFormat {
                format: Default::default(),
                multiview: layout,
                sample_rate: 48_000
            }
        );
        assert!(
            !cmds.iter().any(|c| matches!(c, Command::SetSlot { .. })),
            "the sources keep running"
        );
        assert_eq!(sw.state().multiview, layout);
        assert!(sw
            .apply(Action::SetMultiview { layout })
            .unwrap()
            .is_empty());
    }

    #[test]
    fn rtmp_needs_an_rtmp_address_and_h264() {
        let mut sw = Switcher::new(SLOTS);
        let rtmp = |url: &str, codec| OutputSpec::Rtmp {
            url: url.into(),
            key: "key".into(),
            video_kbps: 4500,
            encoding: VideoEncoding {
                codec,
                ..Default::default()
            },
        };
        let add = |spec| Action::AddOutput {
            spec,
            source: OutputSource::Program,
            name: None,
            aux: None,
        };
        for bad in [
            rtmp("srt://cdn:9000", lmix_types::VideoCodec::H264),
            rtmp("rtmp://cdn/live", lmix_types::VideoCodec::H265),
        ] {
            assert!(matches!(
                sw.apply(add(bad)),
                Err(ActionError::BadEncoding { .. })
            ));
        }
        sw.apply(add(rtmp("rtmps://cdn/live", lmix_types::VideoCodec::H264)))
            .unwrap();
        assert_eq!(sw.state().outputs[0].name, "RTMP rtmps://cdn/live");
        // A stream like SRT: started with the others (STR.1).
        let cmds = sw
            .apply(Action::StartOutputs {
                group: OutputGroup::Streams,
            })
            .unwrap();
        assert_eq!(cmds.len(), 1);
    }

    #[test]
    fn a_long_keyframe_interval_is_refused() {
        let mut sw = Switcher::new(SLOTS);
        let spec = |keyframe_ms| OutputSpec::Record {
            folder: "show".into(),
            container: Default::default(),
            video_kbps: 8000,
            encoding: VideoEncoding {
                keyframe_ms,
                ..Default::default()
            },
        };
        let add = |spec| Action::AddOutput {
            spec,
            source: OutputSource::Program,
            name: None,
            aux: None,
        };
        assert!(matches!(
            sw.apply(add(spec(20_000))),
            Err(ActionError::BadEncoding { .. })
        ));
        assert!(sw.apply(add(spec(10_000))).is_ok());
        let json = r#"{"action":"add_output","spec":{"kind":"srt","uri":"srt://cdn:9000","encoding":{"codec":"h265","rate":"cbr"}}}"#;
        let Action::AddOutput { spec, .. } = serde_json::from_str::<Action>(json).unwrap() else {
            panic!("not add_output");
        };
        assert_eq!(
            spec.encoding(),
            Some(&VideoEncoding {
                codec: lmix_types::VideoCodec::H265,
                rate: lmix_types::RateMode::Cbr,
                keyframe_ms: lmix_types::output::DEFAULT_KEYFRAME_MS,
            })
        );
    }

    #[test]
    fn output_groups_start_and_stop_together() {
        let mut sw = Switcher::new(SLOTS);
        let srt = |port: u16| OutputSpec::Srt {
            uri: format!("srt://cdn:{port}"),
            video_kbps: 6000,
            encoding: Default::default(),
        };
        let record = OutputSpec::Record {
            folder: "show".into(),
            container: Default::default(),
            video_kbps: 8000,
            encoding: Default::default(),
        };
        for spec in [srt(9000), record, srt(9001)] {
            sw.apply(Action::AddOutput {
                spec,
                source: OutputSource::Program,
                name: None,
                aux: None,
            })
            .unwrap();
        }
        let ids: Vec<OutputId> = sw.state().outputs.iter().map(|o| o.id).collect();
        sw.set_output_status(ids[2], OutputStatus::Running { since_ms: 1 });
        let started = |cmds: Vec<Command>| -> Vec<OutputId> {
            cmds.iter()
                .map(|c| match c {
                    Command::StartOutput { id, .. } | Command::StopOutput { id } => *id,
                    other => panic!("{other:?}"),
                })
                .collect()
        };

        let streams = Action::StartOutputs {
            group: OutputGroup::Streams,
        };
        assert_eq!(
            started(sw.apply(streams.clone()).unwrap()),
            [ids[0]],
            "not the running one"
        );
        sw.set_output_status(ids[0], OutputStatus::Running { since_ms: 2 });
        assert!(sw.apply(streams).unwrap().is_empty());

        let stop_all = Action::StopOutputs {
            group: OutputGroup::All,
        };
        assert_eq!(started(sw.apply(stop_all).unwrap()), [ids[0], ids[2]]);
        let recordings = Action::StartOutputs {
            group: OutputGroup::Recordings,
        };
        assert_eq!(started(sw.apply(recordings).unwrap()), [ids[1]]);
        assert_eq!(
            serde_json::from_str::<Action>(r#"{"action":"stop_outputs","group":"streams"}"#)
                .unwrap(),
            Action::StopOutputs {
                group: OutputGroup::Streams
            }
        );
    }

    #[test]
    fn iso_output_follows_its_input() {
        let mut sw = two_inputs();
        let b = sw.state().inputs[1].id;
        let rec = OutputSpec::Record {
            folder: "iso".into(),
            container: lmix_types::Container::Mkv,
            video_kbps: 8000,
            encoding: Default::default(),
        };
        sw.apply(Action::AddOutput {
            spec: rec,
            source: OutputSource::Input(b),
            name: None,
            aux: None,
        })
        .unwrap();
        let output = sw.state().outputs[0].clone();
        assert_eq!(output.name, "Record iso (B)");

        let cmds = sw
            .apply(Action::StartOutput {
                output: OutputRef::Number(1),
            })
            .unwrap();
        assert!(matches!(
            cmds[0],
            Command::StartOutput {
                feed: Feed::Slot {
                    slot: 1,
                    audio: true
                },
                ..
            }
        ));
        sw.set_output_status(output.id, OutputStatus::Running { since_ms: 1 });

        let cmds = sw.apply(Action::RemoveInput { input: named("B") }).unwrap();
        assert!(cmds.contains(&Command::StopOutput { id: output.id }));
        assert!(matches!(
            sw.state().outputs[0].status,
            OutputStatus::Failed { .. }
        ));
        let err = sw
            .apply(Action::StartOutput {
                output: OutputRef::Number(1),
            })
            .unwrap_err();
        assert!(matches!(err, ActionError::UnknownInput { .. }));
    }

    /// A page renders at the program size of its slot flow, so a new
    /// format builds it again; other sources keep running.
    #[test]
    fn a_new_format_renders_pages_again() {
        let mut sw = Switcher::new(SLOTS);
        sw.apply(Action::AddInput {
            source: SourceSpec::Web {
                url: "https://example.com".into(),
                css: String::new(),
            },
            name: None,
        })
        .unwrap();
        sw.apply(Action::AddInput {
            source: SourceSpec::Pattern {
                pattern: TestPattern::Ball,
            },
            name: None,
        })
        .unwrap();
        let portrait = lmix_types::MasterFormat {
            width: 1080,
            height: 1920,
            ..lmix_types::MasterFormat::default()
        };
        let cmds = sw
            .apply(Action::SetMasterFormat { format: portrait })
            .unwrap();
        let rebuilt: Vec<_> = cmds
            .iter()
            .filter_map(|c| match c {
                Command::SetSlot {
                    source: Some(source),
                    ..
                } => Some(source.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(
            rebuilt,
            vec![SourceSpec::Web {
                url: "https://example.com".into(),
                css: String::new(),
            }]
        );
        // After the new mixer: SetFormat comes first.
        assert!(matches!(cmds[0], Command::SetFormat { .. }));
    }

    /// A page's JavaScript and freezing go to its slot; an input without a
    /// page refuses both.
    #[test]
    fn pages_take_scripts_and_freeze() {
        let mut sw = Switcher::new(SLOTS);
        sw.apply(Action::AddInput {
            source: SourceSpec::Web {
                url: "https://example.com/score".into(),
                css: String::new(),
            },
            name: Some("Score".into()),
        })
        .unwrap();
        sw.apply(Action::AddInput {
            source: SourceSpec::Pattern {
                pattern: TestPattern::Ball,
            },
            name: Some("Ball".into()),
        })
        .unwrap();
        let score = InputRef::Text("Score".into());
        let slot = sw.show.inputs[0].slot;

        assert_eq!(
            sw.apply(Action::PageCall {
                input: score.clone(),
                script: "goal('home')".into(),
            })
            .unwrap(),
            vec![Command::PageScript {
                slot,
                script: "goal('home')".into()
            }]
        );
        assert_eq!(
            sw.apply(Action::SetPageRunning {
                input: score,
                running: PageRunning::Frozen,
            })
            .unwrap(),
            vec![Command::PageRunning {
                slot,
                running: PageRunning::Frozen
            }]
        );
        assert!(matches!(
            sw.apply(Action::PageCall {
                input: InputRef::Text("Ball".into()),
                script: "x()".into(),
            }),
            Err(ActionError::NoPage { .. })
        ));
    }

    #[test]
    fn pages_need_a_web_address_and_graphics_a_channel() {
        let mut sw = Switcher::new(SLOTS);
        let add = |source: SourceSpec| Action::AddInput { source, name: None };
        let web = |url: &str| SourceSpec::Web {
            url: url.into(),
            css: String::new(),
        };
        let graphics = |g: &str| SourceSpec::Graphics { graphics: g.into() };

        for url in [
            "file:///etc/passwd",
            "example.com",
            "javascript:alert(1)",
            "",
        ] {
            assert!(
                matches!(sw.apply(add(web(url))), Err(ActionError::BadWebUrl { .. })),
                "{url}"
            );
        }
        assert!(sw.apply(add(web("https://example.com/score"))).is_ok());

        assert!(matches!(
            sw.apply(add(graphics("1"))),
            Err(ActionError::UnknownGraphics { .. })
        ));
        sw.apply(Action::AddGraphics {
            name: Some("Lower thirds".into()),
        })
        .unwrap();
        // By number or by name, as the graphics page takes it.
        assert!(sw.apply(add(graphics("1"))).is_ok());
        assert!(sw.apply(add(graphics("Lower thirds"))).is_ok());
        assert!(matches!(
            sw.apply(add(graphics("2"))),
            Err(ActionError::UnknownGraphics { .. })
        ));
    }

    #[test]
    fn bad_colors_are_refused() {
        let mut sw = Switcher::new(SLOTS);
        let add = |color: &str| Action::AddInput {
            source: SourceSpec::Color {
                color: color.into(),
            },
            name: None,
        };

        assert!(matches!(
            sw.apply(add("red")),
            Err(ActionError::BadColor { .. })
        ));
        assert!(sw.apply(add("#204080")).is_ok());
    }

    #[test]
    fn transport_only_for_media_players() {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::File {
                path: "/clip.mp4".into(),
            },
            name: Some("Clip".into()),
        })
        .unwrap();

        let cmds = sw
            .apply(Action::TransportInput {
                input: named("Clip"),
                transport: Transport::Pause,
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::Transport {
                slot: 2,
                transport: Transport::Pause
            }]
        );

        let err = sw
            .apply(Action::TransportInput {
                input: named("A"),
                transport: Transport::Play,
            })
            .unwrap_err();
        assert!(matches!(err, ActionError::NoTransport { .. }));

        // Jumping in the clip, and to an item of a playlist.
        assert_eq!(
            sw.apply(Action::SeekInput {
                input: named("Clip"),
                position_ms: 12_500,
            })
            .unwrap(),
            [Command::Seek {
                slot: 2,
                position_ms: 12_500
            }]
        );
        sw.apply(Action::AddInput {
            source: SourceSpec::Playlist {
                paths: vec!["/a.mp4".into(), "/b.mp4".into()],
            },
            name: Some("List".into()),
        })
        .unwrap();
        assert_eq!(
            sw.apply(Action::PlayItem {
                input: named("List"),
                item: 2,
            })
            .unwrap(),
            [Command::PlayItem { slot: 3, index: 1 }]
        );
        for item in [0, 3] {
            assert!(matches!(
                sw.apply(Action::PlayItem {
                    input: named("List"),
                    item,
                }),
                Err(ActionError::BadPosition { .. })
            ));
        }
        assert!(matches!(
            sw.apply(Action::PlayItem {
                input: named("Clip"),
                item: 1,
            }),
            Err(ActionError::NoTransport { .. })
        ));
    }

    #[test]
    fn listen_is_kept_per_input_and_cleared_together() {
        let mut sw = two_inputs();

        let cmds = sw
            .apply(Action::SetListen {
                input: named("A"),
                listen: Listen::Pfl,
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::SetListen {
                slot: 0,
                listen: Listen::Pfl
            }]
        );
        sw.apply(Action::SetListen {
            input: named("B"),
            listen: Listen::Afl,
        })
        .unwrap();
        assert_eq!(sw.state().inputs[1].listen, Listen::Afl);

        let cmds = sw.apply(Action::ClearListen).unwrap();
        assert_eq!(cmds.len(), 2);
        assert!(sw.state().inputs.iter().all(|i| i.listen == Listen::Off));
    }

    #[test]
    fn removing_a_soloed_input_ends_its_solo() {
        let mut sw = two_inputs();
        sw.apply(Action::SetListen {
            input: named("B"),
            listen: Listen::Pfl,
        })
        .unwrap();

        let cmds = sw.apply(Action::RemoveInput { input: named("B") }).unwrap();

        assert!(cmds.contains(&Command::SetListen {
            slot: 1,
            listen: Listen::Off
        }));
    }

    #[test]
    fn devices_get_no_audio_commands() {
        let mut sw = Switcher::new(SLOTS);

        let cmds = sw
            .apply(Action::AddInput {
                source: SourceSpec::Device { id: "cam".into() },
                name: None,
            })
            .unwrap();

        assert!(audio_of(&cmds).is_empty());
        assert!(!sw.state().inputs[0].has_audio);
    }

    #[test]
    fn add_takes_lowest_free_slot() {
        let mut sw = two_inputs();
        sw.apply(Action::RemoveInput { input: named("A") }).unwrap();

        let cmds = sw
            .apply(Action::AddInput {
                source: pattern(TestPattern::Ball),
                name: None,
            })
            .unwrap();

        assert_eq!(
            cmds,
            vec![
                Command::SetSlot {
                    slot: 0,
                    source: Some(pattern(TestPattern::Ball))
                },
                Command::SetSignalLoss {
                    slot: 0,
                    mode: SignalLoss::Hold
                },
                Command::SetStrip {
                    slot: 0,
                    strip: ChannelStrip::default()
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 0,
                    gain: 0.0
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 1,
                    gain: 0.0
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 2,
                    gain: 0.0
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 3,
                    gain: 0.0
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 4,
                    gain: 0.0
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 5,
                    gain: 0.0
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 6,
                    gain: 0.0
                },
                Command::SetAudio {
                    slot: 0,
                    gain: 1.0,
                    muted: false,
                    ramp_ms: AUDIO_RAMP_MS
                },
            ]
        );
    }

    #[test]
    fn add_fails_when_slots_full() {
        let mut sw = Switcher::new(1);
        add(&mut sw, "A");

        let err = sw
            .apply(Action::AddInput {
                source: pattern(TestPattern::Bars),
                name: None,
            })
            .unwrap_err();

        assert_eq!(err, ActionError::SlotsFull { slots: 1 });
        assert_eq!(sw.state().inputs.len(), 1);
    }

    #[test]
    fn refs_resolve_by_number_name_id_and_role() {
        let mut sw = two_inputs();
        let b = sw.state().inputs[1].id;
        sw.apply(Action::PreviewInput {
            input: InputRef::Number(2),
        })
        .unwrap();

        assert_eq!(sw.find(&named("B")), Ok(1));
        assert_eq!(sw.find(&InputRef::Text(b.to_string())), Ok(1));
        assert_eq!(sw.find(&named(REF_PREVIEW)), Ok(1));
        assert!(sw.find(&named(REF_PROGRAM)).is_err());
        assert!(sw.find(&InputRef::Number(0)).is_err());
        assert!(sw.find(&InputRef::Number(3)).is_err());
    }

    #[test]
    fn cut_swaps_preview_and_program() {
        let mut sw = two_inputs();
        let (a, b) = (sw.state().inputs[0].id, sw.state().inputs[1].id);
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();

        let cmds = sw.apply(Action::Cut).unwrap();

        assert_eq!(
            cmds,
            vec![
                Command::Program {
                    slot: Some(1),
                    effect: Effect::Cut,
                    duration_ms: 0
                },
                Command::Preview { slot: 0 },
            ]
        );
        assert_eq!(sw.state().program, Some(b));
        assert_eq!(sw.state().preview, Some(a));
        assert_eq!(sw.tally(), vec![2, 1]);
    }

    #[test]
    fn transition_uses_button_settings() {
        let mut sw = two_inputs();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        sw.apply(Action::SetTransition {
            slot: TransitionSlot(3),
            effect: Effect::SlideLeft,
            duration_ms: 750,
            stinger: None,
        })
        .unwrap();

        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(3),
            })
            .unwrap();

        assert_eq!(
            cmds,
            vec![Command::Program {
                slot: Some(1),
                effect: Effect::SlideLeft,
                duration_ms: 750
            }]
        );
    }

    #[test]
    fn take_without_preview_fails() {
        let mut sw = two_inputs();
        assert_eq!(sw.apply(Action::Cut), Err(ActionError::NoPreview));
    }

    #[test]
    fn bad_transition_slot_fails() {
        let mut sw = two_inputs();
        for slot in [0, TRANSITION_SLOTS + 1] {
            let err = sw
                .apply(Action::Transition {
                    slot: TransitionSlot(slot),
                })
                .unwrap_err();
            assert!(matches!(err, ActionError::BadTransitionSlot { .. }));
        }
    }

    #[test]
    fn removing_program_input_clears_program() {
        let mut sw = two_inputs();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();

        let cmds = sw.apply(Action::RemoveInput { input: named("A") }).unwrap();

        assert_eq!(sw.state().program, None);
        assert!(matches!(cmds[0], Command::Program { slot: None, .. }));
        assert_eq!(sw.state().inputs[0].number, 1);
    }

    #[test]
    fn move_reorders_numbers() {
        let mut sw = two_inputs();

        sw.apply(Action::MoveInput {
            input: named("B"),
            number: 1,
        })
        .unwrap();

        let names: Vec<_> = sw.state().inputs.iter().map(|i| i.name.clone()).collect();
        assert_eq!(names, ["B", "A"]);
        assert!(sw
            .apply(Action::MoveInput {
                input: named("A"),
                number: 3
            })
            .is_err());
    }

    #[test]
    fn replace_rebuilds_slots() {
        let mut sw = two_inputs();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        let show = sw.show().clone();
        let mut fresh = Switcher::new(SLOTS);

        let cmds = fresh.replace(show, Some("x.lmix".into())).unwrap();

        // The format (a new mixer if it differs), then picture (slots,
        // program, preview), then audio and the rest.
        assert!(matches!(cmds[0], Command::SetFormat { .. }));
        assert!(matches!(cmds[1], Command::SetSlot { slot: 0, .. }));
        assert!(matches!(cmds[2], Command::SetSlot { slot: 1, .. }));
        assert!(matches!(cmds[3], Command::Program { slot: None, .. }));
        assert_eq!(cmds[4], Command::Preview { slot: 1 });
        assert!(matches!(cmds[5], Command::SetAudio { slot: 0, .. }));
        assert_eq!(fresh.state().path.as_deref(), Some("x.lmix"));
    }

    #[test]
    fn program_input_cannot_be_previewed() {
        let mut sw = two_inputs();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();

        let err = sw
            .apply(Action::PreviewInput { input: named("A") })
            .unwrap_err();

        assert!(matches!(err, ActionError::OnProgram { .. }));
    }

    #[test]
    fn cut_direct_of_preview_swaps() {
        let mut sw = two_inputs();
        let (a, b) = (sw.state().inputs[0].id, sw.state().inputs[1].id);
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();

        let cmds = sw.apply(Action::CutDirect { input: named("B") }).unwrap();

        assert_eq!((sw.state().program, sw.state().preview), (Some(b), Some(a)));
        assert_eq!(cmds[1], Command::Preview { slot: 0 });
    }

    #[test]
    fn failed_action_changes_nothing() {
        let mut sw = two_inputs();
        let before = sw.state();

        let _ = sw.apply(Action::SetInputColor {
            input: named("missing"),
            color: Some(InputColor::Red),
        });
        let _ = sw.apply(Action::RenameInput {
            input: named("missing"),
            name: "X".into(),
        });

        assert_eq!(sw.state(), before);
    }
}
