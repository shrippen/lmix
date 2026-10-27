//! The action catalog (API.1): every operation of lmix is one [`Action`].
//!
//! JSON, tagged by `action`:
//!
//! ```json
//! {"action": "preview_input", "input": "Camera 1"}
//! {"action": "transition", "slot": 1}
//! ```
//!
//! The machine-readable catalog is the JSON schema of [`Action`]
//! (`GET /api/actions`).

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::automation::Trigger;
use crate::bus::BusRef;
use crate::cue::{Cue, StandbyMove, StopMode};
use crate::graphics::{
    BindingSpec, DataSourceSpec, Easing, LayerTransform, TitleData, TitleTrigger,
};
use crate::input::{
    AtEnd, AutoPlay, ChannelStrip, InputColor, InputPress, InputRef, Listen, Look, PageRunning,
    SignalLoss, SourceSpec, Transport,
};
use crate::layout::{BoxRect, BoxSpec};
use crate::output::{OutputGroup, OutputRef, OutputSource, OutputSpec};
use crate::shortcut::ShortcutSpec;
use crate::target::{HttpMethod, TargetSpec};

/// Clip and cut point of a stinger as an action names them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct StingerSpec {
    pub input: InputRef,
    /// Cut point, ms into the clip; at most the button's duration.
    pub cut_ms: u32,
}

/// One of the four transition buttons (SWI.3), 1-based.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct TransitionSlot(pub u8);

/// Dynamic inputs and values, numbered 1–4 (AUT.5).
pub const DYNAMIC_SLOTS: u8 = 4;

/// Number of transition buttons.
pub const TRANSITION_SLOTS: u8 = 4;
/// Overlay channels above program (OVL.1), numbered 1–8.
pub const OVERLAY_CHANNELS: u8 = 8;
/// Default fade of an overlay in or out.
pub const DEFAULT_OVERLAY_MS: u32 = 500;
/// Longest time an overlay stays on before it goes off by itself (OVL.2).
pub const AUTO_OFF_MAX_MS: u32 = 3_600_000;

/// Transition effects the engine can render (SWI.4, subset so far).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Cut,
    Fade,
    DipToBlack,
    SlideLeft,
    SlideRight,
    PushLeft,
    PushRight,
    WipeLeft,
    WipeRight,
    /// A clip over program, cut underneath (lmix-core, not the engine).
    Stinger,
}

impl Effect {
    pub const ALL: [Effect; 10] = [
        Effect::Cut,
        Effect::Fade,
        Effect::DipToBlack,
        Effect::SlideLeft,
        Effect::SlideRight,
        Effect::PushLeft,
        Effect::PushRight,
        Effect::WipeLeft,
        Effect::WipeRight,
        Effect::Stinger,
    ];

    /// Short button label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Cut => "Cut",
            Self::Fade => "Fade",
            Self::DipToBlack => "Dip",
            Self::SlideLeft => "Slide ◀",
            Self::SlideRight => "Slide ▶",
            Self::PushLeft => "Push ◀",
            Self::PushRight => "Push ▶",
            Self::WipeLeft => "Wipe ◀",
            Self::WipeRight => "Wipe ▶",
            Self::Stinger => "Stinger",
        }
    }
}

/// Groups of actions (API.1), like vMix's function groups, for catalogs
/// and controller software: `GET /api/actions` marks each action with
/// `x-group`. A test keeps every action in exactly one group.
pub const ACTION_GROUPS: &[(&str, &[&str])] = &[
    (
        "switching",
        &[
            "preview_input",
            "cut",
            "cut_direct",
            "transition",
            "set_transition",
            "quick_play",
            "set_quick_play",
            "click_input",
            "go_input",
            "fade_to_black",
            "set_fade_to_black",
            "t_bar",
            "set_midi_tbar",
        ],
    ),
    (
        "input",
        &[
            "add_input",
            "remove_input",
            "restore_input",
            "replace_source",
            "rename_input",
            "set_input_color",
            "set_look",
            "page_call",
            "set_page_running",
            "set_signal_loss",
            "set_auto_play",
            "set_at_end",
            "set_input_press",
            "transport_input",
            "seek_input",
            "play_item",
            "move_input",
        ],
    ),
    (
        "overlay",
        &[
            "overlay_in",
            "overlay_out",
            "overlay_out_all",
            "overlay_last",
            "overlay_preview",
            "set_overlay",
        ],
    ),
    (
        "layout",
        &[
            "add_layout",
            "set_layout",
            "show_box",
            "remove_layout",
            "preview_layout",
            "save_layout_template",
            "load_layout_template",
            "remove_layout_template",
            "export_layout_templates",
            "import_layout_templates",
        ],
    ),
    (
        "audio",
        &[
            "set_audio",
            "set_audio_follow",
            "set_volume",
            "set_channel_strip",
            "set_send",
            "set_phones_send",
            "add_bus",
            "remove_bus",
            "rename_bus",
            "set_bus",
            "set_bus_tap",
            "set_bus_listen",
            "set_master",
            "set_phones",
            "set_phones_tap",
            "set_default_delay",
            "fade_volume",
            "set_listen",
            "clear_listen",
        ],
    ),
    (
        "output",
        &[
            "add_output",
            "remove_output",
            "set_output_bus",
            "start_output",
            "stop_output",
            "start_outputs",
            "stop_outputs",
            "set_master_format",
            "set_multiview",
            "set_sample_rate",
        ],
    ),
    (
        "title",
        &[
            "add_graphics",
            "remove_graphics",
            "rename_graphics",
            "title_load",
            "title_play",
            "title_next",
            "title_stop",
            "title_clear",
            "title_update",
            "title_add",
            "title_transform",
            "title_call",
            "title_bind",
            "set_title_triggers",
            "title_report",
            "save_dataset",
            "remove_dataset",
            "apply_dataset",
            "dataset_next",
            "dataset_previous",
            "import_datasets",
            "export_datasets",
        ],
    ),
    (
        "data",
        &[
            "add_data_source",
            "remove_data_source",
            "data_row",
            "data_next",
            "data_previous",
            "set_data_auto",
        ],
    ),
    (
        "automation",
        &[
            "set_triggers",
            "add_cue_list",
            "set_cue_list",
            "update_cue",
            "remove_cue_list",
            "cue_go",
            "cue_start",
            "cue_stop",
            "cue_standby",
            "cue_reset",
            "set_active_cue_list",
            "set_go_guard",
            "set_go_rearm",
            "cue_panic",
            "cue_hard_stop",
            "set_panic_time",
            "set_emergency_list",
            "set_script_folder",
            "add_shortcut",
            "set_shortcut",
            "remove_shortcut",
            "run_shortcut",
            "set_dynamic_input",
            "set_dynamic_value",
        ],
    ),
    (
        "zones",
        &[
            "set_zone",
            "remove_zone",
            "set_zone_patch",
            "remove_zone_patch",
        ],
    ),
    (
        "sound",
        &["play_sound", "fade_sound", "stop_sound", "set_zone_level"],
    ),
    (
        "stages",
        &[
            "set_stage",
            "remove_stage",
            "set_stage_patch",
            "remove_stage_patch",
            "show_picture",
            "fade_picture",
            "stop_picture",
            "set_stage_level",
        ],
    ),
    (
        "targets",
        &[
            "set_target",
            "remove_target",
            "test_target",
            "check_targets",
            "http_request",
            "ha_service",
            "set_feed",
            "remove_feed",
        ],
    ),
    (
        "ptz",
        &[
            "set_ptz",
            "ptz_move",
            "ptz_zoom",
            "ptz_focus",
            "ptz_home",
            "ptz_preset",
        ],
    ),
    (
        "show",
        &[
            "export_settings",
            "import_settings",
            "reset_settings",
            "new_show",
            "save_show",
            "open_show",
            "append_show",
            "relocate_media",
            "export_package",
            "import_package",
        ],
    ),
    ("safety", &["set_lock", "set_cards"]),
];

/// The group of an action by its name (`cut`, `title_load`).
pub fn action_group(name: &str) -> Option<&'static str> {
    ACTION_GROUPS
        .iter()
        .find(|(_, names)| names.contains(&name))
        .map(|(group, _)| *group)
}

/// Longest level fade (AUD.14).
pub const FADE_MAX_MS: u32 = 60_000;

/// What a level fade moves (AUD.14): `{"input": "Cam 1"}`,
/// `{"bus": "Stream"}`, `"master"` or `"phones"`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum VolumeTarget {
    Input(InputRef),
    Bus(BusRef),
    Master,
    Phones,
}

/// Every operation, whatever triggered it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Action {
    // ---- Inputs (INP) ----
    /// Add an input; it gets the next number and a free mixer slot.
    AddInput {
        source: SourceSpec,
        #[serde(default)]
        name: Option<String>,
    },
    /// Close an input.
    RemoveInput {
        input: InputRef,
    },
    /// Bring back the input closed last, with its settings (AUT.10).
    RestoreInput,
    /// Change what an input shows, keeping its settings (INP.4).
    ReplaceSource {
        input: InputRef,
        source: SourceSpec,
    },
    /// Rename an input.
    RenameInput {
        input: InputRef,
        name: String,
    },
    /// Set or clear the color tag of an input (INP.7).
    SetInputColor {
        input: InputRef,
        #[serde(default)]
        color: Option<InputColor>,
    },
    /// Set an input's colour correction (needs the GL mixer).
    SetLook {
        input: InputRef,
        look: Look,
    },
    /// Run JavaScript in a web or graphics input's page, such as one of its
    /// functions (GFX.6).
    PageCall {
        input: InputRef,
        script: String,
    },
    /// Freeze a web or graphics input's page or let it run again (GFX.2).
    SetPageRunning {
        input: InputRef,
        running: PageRunning,
    },
    /// Choose last frame or black for when the input's source stalls.
    SetSignalLoss {
        input: InputRef,
        mode: SignalLoss,
    },
    /// Whether a file, URL or playlist input plays, starts from the
    /// beginning, pauses by itself as it goes on and off air (INP.11).
    SetAutoPlay {
        input: InputRef,
        auto_play: AutoPlay,
    },
    /// Whether a file, URL or playlist input loops or stops on its last
    /// frame at the end (INP.10); changes on air without a restart.
    SetAtEnd {
        input: InputRef,
        at_end: AtEnd,
    },
    /// Choose what a click on the input (INP.8) and its GO (INP.9) do;
    /// one left out stays as it is.
    SetInputPress {
        input: InputRef,
        #[serde(default)]
        click: Option<InputPress>,
        #[serde(default)]
        go: Option<InputPress>,
    },
    /// Play, pause, restart or step a file, URL or playlist input.
    TransportInput {
        input: InputRef,
        transport: Transport,
    },
    /// Jump to `position_ms` in the item a file, URL or playlist input
    /// plays (UI.5).
    SeekInput {
        input: InputRef,
        position_ms: u64,
    },
    /// Play item `item` (1-based) of a playlist input (UI.4).
    PlayItem {
        input: InputRef,
        item: u32,
    },

    // ---- Outputs (OUT, REC, STR) ----
    /// Add an output to the show; it starts stopped.
    AddOutput {
        spec: OutputSpec,
        /// Program (default) or one input (ISO, MultiCorder).
        #[serde(default)]
        source: OutputSource,
        /// Sound from this bus instead of the source's own.
        #[serde(default)]
        bus: Option<BusRef>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Remove an output (stops it first).
    RemoveOutput {
        output: OutputRef,
    },
    /// Sound of an output from bus `bus`, or the source's own without
    /// one. A running output plays on: it takes the other sound at once
    /// (one with no sound before, an ISO of a silent input, restarts).
    SetOutputBus {
        output: OutputRef,
        #[serde(default)]
        bus: Option<BusRef>,
    },
    /// Start streaming / recording / sending.
    StartOutput {
        output: OutputRef,
    },
    /// Stop it.
    StopOutput {
        output: OutputRef,
    },
    /// Start every output of `group` that is not running (STR.1); one
    /// that cannot start does not keep the others back.
    StartOutputs {
        group: OutputGroup,
    },
    /// Stop every running output of `group`.
    StopOutputs {
        group: OutputGroup,
    },

    // ---- Audio (AUD) ----
    /// Switch an input's sound on or off.
    SetAudio {
        input: InputRef,
        on: bool,
    },
    /// Audio follows video: the input is heard only while on program.
    SetAudioFollow {
        input: InputRef,
        follow: bool,
    },
    /// Set an input's volume, 0–100 (100 = unity gain).
    SetVolume {
        input: InputRef,
        volume: u8,
    },
    /// Set an input's channel strip: trim, pan, filter, EQ, dynamics,
    /// delay.
    SetChannelStrip {
        input: InputRef,
        strip: ChannelStrip,
    },
    /// Send an input to bus `bus` at `level`, 0–100; 0 stops the send.
    SetSend {
        input: InputRef,
        bus: BusRef,
        level: u8,
    },
    /// What an input sends to the headphones (AUD.4): on or off, and its
    /// level there, 0–100.
    SetPhonesSend {
        input: InputRef,
        on: bool,
        level: u8,
    },
    /// The headphones: volume 0–100 and mute (AUD.4).
    SetPhones {
        volume: u8,
        mute: bool,
    },
    /// Whether the headphones take the sends before the channel faders;
    /// the running audio mixer changes, outputs play on.
    SetPhonesTap {
        pre_fader: bool,
    },
    /// A new bus at the end (AUD.2), named `name` or "Bus <n>"; the
    /// running audio mixer changes, outputs play on. No input sends to it
    /// yet.
    AddBus {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        pre_fader: bool,
    },
    /// Remove a bus and every send to it; refused while an output takes
    /// its sound. The running audio mixer changes, outputs play on.
    RemoveBus {
        bus: BusRef,
    },
    RenameBus {
        bus: BusRef,
        name: String,
    },
    /// Whether a bus takes its sends before the channel faders; the
    /// running audio mixer changes, outputs play on.
    SetBusTap {
        bus: BusRef,
        pre_fader: bool,
    },
    /// Listen to a bus on the headphones, after its master (like an
    /// input's solo).
    SetBusListen {
        bus: BusRef,
        on: bool,
    },
    /// Delay new inputs with sound get in their channel strip, 0–500 ms
    /// (AUD.6); inputs already there keep theirs.
    SetDefaultDelay {
        ms: u32,
    },
    /// Picture size and rate of program (OUT.1). The engine builds the
    /// mixer anew: program stops for a moment, running outputs restart.
    SetMasterFormat {
        format: crate::output::MasterFormat,
    },
    /// Layout of the multiview (OUT.10): which side program is on, level
    /// meters. The engine builds the mixer anew, as for the format.
    SetMultiview {
        layout: crate::output::MultiviewLayout,
    },
    /// Sample rate of the audio mixer (AUD.17): 48000 (default) or
    /// 44100. Inputs are resampled to it; the mixer is built anew.
    SetSampleRate {
        hz: u32,
    },
    /// Master of program sound: volume 0–100 and mute (AUD.2).
    SetMaster {
        volume: u8,
        mute: bool,
    },
    /// Move a level to `volume` over `duration_ms` (AUD.14): an input's
    /// fader, a bus, the master or the headphones; mute stays as it is. The state
    /// shows the end value at once; a new level on the same target takes
    /// over from where the fade is.
    FadeVolume {
        target: VolumeTarget,
        volume: u8,
        duration_ms: u32,
    },
    /// Master of a bus: volume 0–100 and mute.
    SetBus {
        bus: BusRef,
        volume: u8,
        mute: bool,
    },
    /// Listen to an input on the headphones, before or after its fader.
    /// While any input or bus is soloed, the headphones carry only the
    /// solos; otherwise their own mix.
    SetListen {
        input: InputRef,
        listen: Listen,
    },
    /// End every solo, of inputs and buses: the headphones hear their own
    /// mix again.
    ClearListen,

    /// Move an input to a 1-based position in the input bar (INP.3).
    MoveInput {
        input: InputRef,
        number: u32,
    },

    // ---- Layouts (picture in picture) ----
    /// Add a layout: a background input with input boxes on top. It can
    /// stand in preview and program like an input.
    AddLayout {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        background: Option<InputRef>,
        #[serde(default)]
        boxes: Vec<BoxSpec>,
    },
    /// Change a layout (by number, name or id); on air it moves there.
    SetLayout {
        layout: InputRef,
        #[serde(default)]
        background: Option<InputRef>,
        #[serde(default)]
        boxes: Vec<BoxSpec>,
    },
    /// Show, hide or flip box `number` (1-based) of a layout (LAY.2); a
    /// hidden box stays in the layout, is not drawn and not on air.
    ShowBox {
        layout: InputRef,
        number: u8,
        shown: crate::layout::BoxShown,
    },
    /// Remove a layout (not while it is on program).
    RemoveLayout {
        layout: InputRef,
    },
    /// Put a layout into preview.
    PreviewLayout {
        layout: InputRef,
    },
    /// Keep a layout as a template for every show (LAY.5), under `name`
    /// (the layout's by default); replaces a template of that name.
    SaveLayoutTemplate {
        layout: InputRef,
        #[serde(default)]
        name: Option<String>,
    },
    /// Add a layout from a template: each box takes the input of the
    /// name it had, else the one at its number.
    LoadLayoutTemplate {
        template: String,
        #[serde(default)]
        name: Option<String>,
    },
    /// Remove a template.
    RemoveLayoutTemplate {
        template: String,
    },
    /// Write templates (by name, all if empty) to a file.
    ExportLayoutTemplates {
        path: String,
        #[serde(default)]
        templates: Vec<String>,
    },
    /// Read templates from a file; ones of the same name are replaced.
    ImportLayoutTemplates {
        path: String,
    },

    // ---- Automation (AUT.7, AUT.14) ----
    /// Replace an input's triggers.
    SetTriggers {
        input: InputRef,
        triggers: Vec<Trigger>,
    },
    /// Add a cue list. `source`: where it comes from, e.g. a script
    /// file (IMP.9); clients then do not edit it.
    AddCueList {
        name: String,
        cues: Vec<Cue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    /// Replace a cue list's name and cues (by number, name or id); the
    /// next cue stays where it was, at most one past the last.
    SetCueList {
        list: InputRef,
        name: String,
        cues: Vec<Cue>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
    },
    /// Change fields of one cue (number or id) of a list, inside groups
    /// too; without `list` the active list. `changes` is a JSON merge
    /// patch of the cue (RFC 7386): only the fields it names change,
    /// `null` clears one, e.g. `{"name": "Rain", "sound": {"gain_db":
    /// -6}}`. The id stays; a list from a script is edited there. So two
    /// clients, or two quick edits, change different fields without
    /// sending the whole list from an older state. Standby and running
    /// cues stay as with `set_cue_list`.
    UpdateCue {
        #[serde(default)]
        list: Option<InputRef>,
        cue: String,
        changes: serde_json::Value,
    },
    /// Remove a cue list.
    RemoveCueList {
        list: InputRef,
    },
    /// Start the list's standby cue with its follow chain and move
    /// standby past it (RUN.1); without `list` the active list. Dropped
    /// when it comes sooner than the GO guard (RUN.6).
    CueGo {
        #[serde(default)]
        list: Option<InputRef>,
    },
    /// Start a cue (number or id) with its follow chain; standby stays,
    /// the GO guard does not apply.
    CueStart {
        #[serde(default)]
        list: Option<InputRef>,
        cue: String,
    },
    /// Stop a running cue (number or id), or without `cue` every cue of
    /// the list, or without `list` every cue (RUN.4).
    CueStop {
        #[serde(default)]
        list: Option<InputRef>,
        #[serde(default)]
        cue: Option<String>,
        #[serde(default)]
        mode: StopMode,
    },
    /// Move standby without starting anything (RUN.2).
    CueStandby {
        #[serde(default)]
        list: Option<InputRef>,
        to: StandbyMove,
    },
    /// Stop every cue at once and put every standby back to the start
    /// (RUN.11).
    CueReset,
    /// The list GO goes to without a list (RUN.7).
    SetActiveCueList {
        list: InputRef,
    },
    /// Shortest gap between two GOs, 0–5000 ms (RUN.6).
    SetGoGuard {
        ms: u32,
    },
    /// Take GO again after the guard alone, or once the button is let go
    /// as well (RUN.6).
    SetGoRearm {
        rearm: crate::cue::GoRearm,
    },
    /// Stop every cue of every list over the panic time, or over `ms`
    /// ("panic in time x", CTL.1); a second panic within it is a hard
    /// stop. Standby stays; the emergency list runs (SAF.1, SAF.4).
    CuePanic {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ms: Option<u32>,
    },
    /// Stop every cue at once; the emergency list runs (SAF.3).
    CueHardStop,
    /// Panic time of the show, 0–10 000 ms (SAF.2).
    SetPanicTime {
        ms: u32,
    },
    /// The list that runs with every panic and hard stop; `null` for none
    /// (SAF.4).
    SetEmergencyList {
        #[serde(default)]
        list: Option<InputRef>,
    },
    /// The show's script folder: lmix-core keeps `lmix-script load
    /// <folder> --watch` running for it; `null` stops it (SSH.2).
    SetScriptFolder {
        #[serde(default)]
        folder: Option<String>,
    },

    // ---- Zones (show mode SND.10) ----
    /// Add a zone to the show, or replace the one of the same name.
    SetZone {
        zone: crate::zone::Zone,
    },
    RemoveZone {
        name: String,
    },
    /// Where a zone plays on this machine (machine settings, SSH.3).
    SetZonePatch {
        patch: crate::zone::ZonePatch,
    },
    RemoveZonePatch {
        zone: String,
    },

    // ---- Sound (show mode SND) ----
    /// Play `sound` as `id` into the zones; a sound of the same id is
    /// replaced. Sound cues run this with their cue id.
    PlaySound {
        id: String,
        sound: crate::sound::SoundCue,
    },
    /// Fade sound `id` to `gain_db` over `duration_ms` (S-curve, SND.6,
    /// SND.8).
    FadeSound {
        id: String,
        gain_db: f32,
        duration_ms: u32,
        #[serde(default)]
        then: crate::sound::AfterFade,
    },
    /// Stop sound `id`, or every sound, faded out over `fade_ms`.
    StopSound {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        fade_ms: u32,
    },
    /// Level and mute of a zone, reached over `duration_ms` (SND.11).
    SetZoneLevel {
        zone: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        gain_db: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        mute: Option<bool>,
        #[serde(default)]
        duration_ms: u32,
    },

    // ---- Stages (show mode VIS) ----
    /// Add a stage to the show, or replace the one of the same name.
    SetStage {
        stage: crate::stage::Stage,
    },
    RemoveStage {
        name: String,
    },
    /// Where a stage shows on this machine (machine settings, SSH.3).
    SetStagePatch {
        patch: crate::stage::StagePatch,
    },
    RemoveStagePatch {
        stage: String,
    },
    /// Show `picture` as `id`, replacing a picture of that id. Picture
    /// cues run this with their cue id.
    ShowPicture {
        id: String,
        picture: crate::stage::PictureCue,
    },
    /// Fade picture `id` to `opacity` over `duration_ms` (S-curve, VIS.4).
    FadePicture {
        id: String,
        opacity: f32,
        duration_ms: u32,
        #[serde(default)]
        then: crate::sound::AfterFade,
    },
    /// Take picture `id`, or every picture, off its stage over `fade_ms`.
    StopPicture {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        id: Option<String>,
        #[serde(default)]
        fade_ms: u32,
    },
    /// Brightness of a stage, 0 (black) to 1, over `duration_ms` (VIS.7).
    SetStageLevel {
        stage: String,
        brightness: f32,
        #[serde(default)]
        duration_ms: u32,
    },

    // ---- Output targets (show mode EXT) ----
    /// Add a target of this machine, or replace the one of the same name.
    SetTarget {
        target: TargetSpec,
    },
    RemoveTarget {
        name: String,
    },
    /// Send a probe (HTTP: `GET` of the first address); refused with the
    /// reason when it fails (EXT.5).
    TestTarget {
        name: String,
    },
    /// Probe every target the cue lists send to, all at once; those that
    /// do not answer go into the warnings until they do (SUI.5).
    CheckTargets,
    /// An HTTP request to every address of `target`. The action does not
    /// wait for it; how it went comes as `Event::Target` (EXT.3).
    HttpRequest {
        target: String,
        #[serde(default)]
        method: HttpMethod,
        /// Appended to each address, e.g. `/api/scene/1`.
        #[serde(default)]
        path: String,
        #[serde(default)]
        headers: std::collections::BTreeMap<String, String>,
        /// A string goes as text, anything else as JSON.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        body: Option<serde_json::Value>,
    },
    /// Call a Home Assistant service, e.g. `light.turn_on`, for `entity`
    /// with `data`; the target's secret is the access token (EXT.4).
    HaService {
        target: String,
        service: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        entity: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        data: Option<serde_json::Value>,
    },

    /// Add a fixed OSC subscriber, or replace the one of the same name
    /// (CTL.4).
    SetFeed {
        feed: crate::feed::Feed,
    },
    RemoveFeed {
        name: String,
    },

    // ---- Safety ----
    /// Lock or unlock (AUT.11): while locked, closing inputs, opening or
    /// clearing the show, stopping or removing outputs and restarting one
    /// for another bus are refused.
    SetLock {
        on: bool,
    },
    /// Use the capture and playout cards (DeckLink, SRC.1/OUT.4) or let
    /// go of them, which only one program can use at a time. Off: card
    /// inputs show black with a banner, card outputs stop and wait. On:
    /// both come back. Off at every start of lmix-core, not in the show.
    SetCards {
        on: bool,
    },
    /// Bind a MIDI controller (`cc`) to the T-bar: 0–127 moves it from
    /// 0 to 1; `null` unbinds.
    SetMidiTbar {
        #[serde(default)]
        midi: Option<crate::midi::MidiBinding>,
    },

    // ---- Shortcuts (AUT.1–3) ----
    /// Add a shortcut: a key and label for a list of actions.
    AddShortcut {
        shortcut: ShortcutSpec,
    },
    /// Replace a shortcut (by number, label or id).
    SetShortcut {
        target: InputRef,
        shortcut: ShortcutSpec,
    },
    /// Remove a shortcut.
    RemoveShortcut {
        target: InputRef,
    },
    /// Run a shortcut's actions in order; the first failing one stops
    /// the rest.
    RunShortcut {
        target: InputRef,
    },

    // ---- Switching (SWI) ----
    /// Put an input into preview.
    PreviewInput {
        input: InputRef,
    },
    /// Swap preview and program instantly.
    Cut,
    /// Put an input on program instantly, skipping preview.
    CutDirect {
        input: InputRef,
    },
    /// Take preview to program with transition button `slot` (1–8).
    Transition {
        slot: TransitionSlot,
    },
    /// Take `input` (preview when left out) to program with the quick
    /// play effect and play it, if it is a clip (SWI.8). Preview stays,
    /// unless `input` was in it; on program already it only plays.
    QuickPlay {
        #[serde(default)]
        input: Option<InputRef>,
    },
    /// Set the effect and length of quick play (no stinger).
    SetQuickPlay {
        effect: Effect,
        duration_ms: u32,
    },
    /// Do what a click on `input` does (INP.8; preview unless set).
    ClickInput {
        input: InputRef,
    },
    /// Do what GO on `input` does (INP.9; quick play unless set).
    GoInput {
        input: InputRef,
    },
    /// Configure transition button `slot`.
    SetTransition {
        slot: TransitionSlot,
        effect: Effect,
        duration_ms: u32,
        /// Clip and cut point for `effect` stinger; the clip is paused
        /// until the button is pressed.
        #[serde(default)]
        stinger: Option<StingerSpec>,
    },
    /// Toggle fade to black of all sinks (SWI.9).
    FadeToBlack {
        #[serde(default = "default_ftb_ms")]
        duration_ms: u32,
    },
    /// Fade to black on or off, whatever it is now: a second press of a
    /// panic sends the same and changes nothing (SWI.9).
    SetFadeToBlack {
        on: bool,
        #[serde(default = "default_ftb_ms")]
        duration_ms: u32,
    },
    /// Move the T-bar (SWI.3): mix preview over program by `position`
    /// (0–1). Reaching 1 takes preview to program; back at 0 leaves both
    /// as they were.
    TBar {
        position: f32,
    },

    // ---- Overlays (OVL) ----
    /// Show `input` on overlay `channel` (1–8), above program and its
    /// transitions; replaces what the channel showed. Without
    /// `duration_ms` with the channel's effect, with it a fade of that
    /// length (0 cuts); the same for the other overlay actions.
    OverlayIn {
        channel: u8,
        input: InputRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u32>,
    },
    /// Take overlay `channel` off.
    OverlayOut {
        channel: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u32>,
    },
    /// Take every overlay channel off (OVL.8).
    OverlayOutAll {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u32>,
    },
    /// Show the input overlay `channel` showed last again (OVL.8); does
    /// nothing while the channel shows something.
    OverlayLast {
        channel: u8,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u32>,
    },
    /// Prepare `input` on overlay `channel` (OVL.3): it goes on with the
    /// next take of preview (cut, transition, T-bar, quick play), with
    /// the take's effect and length; `null` clears it.
    OverlayPreview {
        channel: u8,
        #[serde(default)]
        input: Option<InputRef>,
    },
    /// Settings of overlay `channel` (OVL.2): effect (cut or fade) and
    /// length, auto off after `auto_off_ms`, picture in picture in
    /// `rect` (full screen without).
    SetOverlay {
        channel: u8,
        effect: Effect,
        duration_ms: u32,
        #[serde(default)]
        auto_off_ms: Option<u32>,
        #[serde(default)]
        rect: Option<BoxRect>,
    },

    // ---- Dynamic inputs and values (AUT.5) ----
    /// Let `"dynamic<slot>"` (1–8) name `input` in later actions; null
    /// empties it. One key picks the input, another acts on it.
    SetDynamicInput {
        slot: u8,
        #[serde(default)]
        input: Option<InputRef>,
    },
    /// Set dynamic value `slot` (1–8); title texts and function arguments
    /// show it where they say `{value<slot>}`.
    SetDynamicValue {
        slot: u8,
        value: String,
    },

    // ---- PTZ (PER.2) ----
    /// Set or clear (null) the PTZ address of an input's camera.
    SetPtz {
        input: InputRef,
        #[serde(default)]
        ptz: Option<crate::ptz::PtzSpec>,
    },
    /// Pan and tilt at a speed: −1 (left, down) to 1 (right, up); 0 0
    /// stops.
    PtzMove {
        input: InputRef,
        pan: f32,
        tilt: f32,
    },
    /// Zoom at a speed: −1 (wide) to 1 (tele); 0 stops.
    PtzZoom {
        input: InputRef,
        speed: f32,
    },
    /// Focus: auto, manual, near, far, stop.
    PtzFocus {
        input: InputRef,
        focus: crate::ptz::PtzFocus,
    },
    /// Back to the camera's home position.
    PtzHome {
        input: InputRef,
    },
    /// Go to preset `preset` (0–127), or store the position there.
    PtzPreset {
        input: InputRef,
        preset: u8,
        #[serde(default)]
        store: bool,
    },

    // ---- Titles (GFX) ----
    /// Add a graphics channel: a page of title layers to render as an
    /// input or overlay.
    AddGraphics {
        #[serde(default)]
        name: Option<String>,
    },
    /// Remove a graphics channel and its layers.
    RemoveGraphics {
        graphics: InputRef,
    },
    /// Rename a graphics channel.
    RenameGraphics {
        graphics: InputRef,
        name: String,
    },
    /// Load `template` on `layer` (1–8) with `data`, replacing what the
    /// layer held; `play` plays it at once (CasparCG `CG ADD`).
    TitleLoad {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
        template: String,
        #[serde(default)]
        data: TitleData,
        #[serde(default)]
        play: bool,
    },
    /// Play the layer's title (in animation); again from the start if it
    /// played already.
    TitlePlay {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
    },
    /// Next step of a multi-step title.
    TitleNext {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
    },
    /// Stop the title: it plays its out animation.
    TitleStop {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
    },
    /// Remove the title at once; without `layer` all layers.
    TitleClear {
        graphics: InputRef,
        #[serde(default)]
        layer: Option<u32>,
    },
    /// Set fields (others keep their values); an empty value clears one.
    TitleUpdate {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
        data: TitleData,
    },
    /// Add `amount` to a number field (a score +1); an empty or
    /// non-number field counts as 0.
    TitleAdd {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
        field: String,
        #[serde(default = "one")]
        amount: f64,
    },
    /// Place the layer (GFX.18): position, size, turn, tilt, crop,
    /// opacity, colour, blend; all values together, moving there in
    /// `duration_ms` (0: at once).
    TitleTransform {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
        transform: LayerTransform,
        #[serde(default)]
        duration_ms: u32,
        #[serde(default)]
        easing: Easing,
    },
    /// Call a function of the template (CasparCG `CG INVOKE`).
    TitleCall {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
        function: String,
        #[serde(default)]
        args: Vec<String>,
    },
    /// Feed a field from a data source column; `binding` null unbinds.
    TitleBind {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
        field: String,
        #[serde(default)]
        binding: Option<BindingSpec>,
    },
    /// Replace the channel's triggers on template reports.
    SetTitleTriggers {
        graphics: InputRef,
        triggers: Vec<TitleTrigger>,
    },
    /// Sent by the graphics page: the template reported `event` (e.g.
    /// `done` after its out animation, `zero` of a countdown).
    TitleReport {
        graphics: InputRef,
        layer: u32,
        event: String,
    },
    /// Save field values under `name` (GFX.8); replaces a set of that
    /// name.
    SaveDataset {
        name: String,
        #[serde(default)]
        template: Option<String>,
        data: TitleData,
    },
    /// Remove a saved set.
    RemoveDataset {
        name: String,
    },
    /// Set a layer's fields from a saved set.
    ApplyDataset {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
        name: String,
    },

    /// Apply the next saved set to the layer (sets of its template, or
    /// of none); after the last, the first again.
    DatasetNext {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
    },
    /// Apply the set before.
    DatasetPrevious {
        graphics: InputRef,
        #[serde(default = "first_layer")]
        layer: u32,
    },
    /// Read sets from a CSV file: a column `name`, the others are fields;
    /// sets of the same name are replaced.
    ImportDatasets {
        path: String,
        #[serde(default)]
        template: Option<String>,
    },
    /// Write the sets (of `template`, or all) to a CSV file.
    ExportDatasets {
        path: String,
        #[serde(default)]
        template: Option<String>,
    },

    // ---- Data sources (DAT) ----
    /// Add a data source; lmix-core reads the file and follows changes.
    AddDataSource {
        spec: DataSourceSpec,
        #[serde(default)]
        name: Option<String>,
    },
    /// Remove a data source; fields bound to it keep their last values.
    RemoveDataSource {
        source: InputRef,
    },
    /// Select row `row` (1-based).
    DataRow {
        source: InputRef,
        row: u32,
    },
    /// Select the next row (the first after the last with `loop_rows`).
    DataNext {
        source: InputRef,
    },
    /// Select the previous row.
    DataPrevious {
        source: InputRef,
    },
    /// Move on every `interval_ms` (null stops), and whether to start
    /// again after the last row.
    SetDataAuto {
        source: InputRef,
        #[serde(default)]
        interval_ms: Option<u32>,
        #[serde(default)]
        loop_rows: bool,
    },

    // ---- Show (SHW) ----
    /// Start an empty show, in `format` if given (SHW.3), else in the
    /// default 1920 × 1080 at 30 fps.
    NewShow {
        #[serde(default)]
        format: Option<crate::output::MasterFormat>,
    },
    /// Save the show; without `path` to its current file.
    SaveShow {
        #[serde(default)]
        path: Option<String>,
    },
    /// Open a show file, replacing the current show.
    OpenShow {
        path: String,
    },
    /// Look for the missing media files of all inputs in `folder` (by
    /// file name) and use those found (SHW.4).
    RelocateMedia {
        folder: String,
    },
    /// Add inputs of another show file to this one, with their settings
    /// (SHW.3): `inputs` by number in that show, all if empty.
    AppendShow {
        path: String,
        #[serde(default)]
        inputs: Vec<u32>,
    },
    /// Write the show with its media, data files and own title templates
    /// into one package file (SHW.6).
    /// This machine's settings into a file: shortcuts for all shows,
    /// layout templates, output targets and OSC subscribers, no secrets
    /// (SHW.8).
    ExportSettings {
        path: String,
    },
    /// Take a settings file instead of the current settings; targets of
    /// the same name keep their secret.
    ImportSettings {
        path: String,
    },
    /// Back to factory settings: no shortcuts for all shows, layout
    /// templates, targets or subscribers.
    ResetSettings,
    ExportPackage {
        path: String,
        /// Also this machine's output targets and OSC subscribers, without
        /// secrets (`machine.json`, show mode SSH.5).
        #[serde(default)]
        machine: bool,
    },
    /// Unpack a package into `folder` (new or empty) and open its show.
    ImportPackage {
        path: String,
        folder: String,
        /// Also take the package's targets and OSC subscribers; ones of
        /// the same name are replaced, their secrets stay (SSH.5).
        #[serde(default)]
        machine: bool,
    },
}

/// Default FTB fade.
pub const DEFAULT_FTB_MS: u32 = 1000;

fn default_ftb_ms() -> u32 {
    DEFAULT_FTB_MS
}

fn first_layer() -> u32 {
    1
}

fn one() -> f64 {
    1.0
}

/// Why an action was refused. Clients show `message` (API.9: errors with
/// a reason).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "error", rename_all = "snake_case")]
pub enum ActionError {
    #[error("no output matches {reference}")]
    UnknownOutput { reference: String },
    #[error("encoding: {reason}")]
    BadEncoding { reason: String },
    #[error("sample rate {hz} Hz: 48000 or 44100")]
    BadSampleRate { hz: u32 },
    #[error("no input matches {reference}")]
    UnknownInput { reference: String },
    #[error("all {slots} mixer slots are in use")]
    SlotsFull { slots: usize },
    #[error("transition slot {slot} does not exist (1–{max})")]
    BadTransitionSlot { slot: u8, max: u8 },
    #[error("nothing in preview")]
    NoPreview,
    #[error("all {max} layouts are in use")]
    LayoutsFull { max: usize },
    #[error("no layout {reference}")]
    UnknownLayout { reference: String },
    #[error("layout: {reason}")]
    BadLayout { reason: String },
    #[error("no cue list {reference}")]
    UnknownCueList { reference: String },
    #[error("no target {name}")]
    UnknownTarget { name: String },
    #[error("target: {reason}")]
    BadTarget { reason: String },
    #[error("zone: {reason}")]
    BadZone { reason: String },
    #[error("sound: {reason}")]
    BadSound { reason: String },
    #[error("stage: {reason}")]
    BadStage { reason: String },
    #[error("cue list: {reason}")]
    BadCueList { reason: String },
    #[error("triggers: {reason}")]
    BadTriggers { reason: String },
    #[error("locked: unlock first")]
    Locked,
    #[error("no closed input to bring back")]
    NothingToRestore,
    #[error("no shortcut {reference}")]
    UnknownShortcut { reference: String },
    #[error("shortcut: {reason}")]
    BadShortcut { reason: String },
    #[error("MIDI: {reason}")]
    BadMidi { reason: String },
    #[error("a stinger is playing")]
    StingerPlaying,
    #[error("stinger: {reason}")]
    BadStinger { reason: String },
    #[error("quick play: {reason}")]
    BadQuickPlay { reason: String },
    #[error("{reference} is on program; it cannot be in preview at the same time")]
    OnProgram { reference: String },
    #[error("position {number} is outside 1–{max}")]
    BadPosition { number: u32, max: u32 },
    #[error("{reference} has no picture (audio input)")]
    NoVideo { reference: String },
    #[error("{reference} has no sound")]
    NoAudio { reference: String },
    #[error("{reference} has no transport (not a file, URL or playlist)")]
    NoTransport { reference: String },
    #[error("color {color} is not #rrggbb")]
    BadColor { color: String },
    #[error("web page {url} is not an http:// or https:// address")]
    BadWebUrl { url: String },
    #[error("capture mode {mode} is not a resolution like 1920x1080 or a frame rate like 30/1")]
    BadDeviceMode { mode: String },
    #[error("card: {reason}")]
    BadCard { reason: String },
    #[error("{reference} shows no page (not a web or graphics input)")]
    NoPage { reference: String },
    #[error("volume {volume} is outside 0–{max}")]
    BadVolume { volume: u8, max: u8 },
    #[error("a level fade lasts 0–{max} ms, not {duration_ms}")]
    BadFade { duration_ms: u32, max: u32 },
    #[error("program format: even sides of {min}–{max} pixels")]
    BadFormat { min: u32, max: u32 },
    #[error("overlay channel {channel} does not exist (1–{max})")]
    BadOverlay { channel: u8, max: u8 },
    #[error("overlay {channel} has no input to show again")]
    NoOverlayLast { channel: u8 },
    #[error("overlay {channel}: {reason}")]
    BadOverlaySettings { channel: u8, reason: String },
    #[error("no layout template {name}")]
    UnknownLayoutTemplate { name: String },
    #[error("T-bar position is outside 0–1")]
    BadTBar,
    #[error("fade to black is on; end it before using the T-bar")]
    FadeToBlackOn,
    #[error("a transition is still running; move the T-bar when it has landed")]
    TransitionRunning,
    #[error("no bus matches {reference}")]
    UnknownBus { reference: String },
    #[error("a show has at most {max} buses besides master and headphones")]
    TooManyBuses { max: usize },
    #[error("bus name: 1–{max} characters, not empty")]
    BadBusName { max: usize },
    #[error("there is a bus {name} already")]
    DuplicateBusName { name: String },
    #[error("output {output} takes the sound of bus {bus}: give it another first")]
    BusInUse { bus: String, output: String },
    #[error("look: {field} is out of range")]
    BadLook { field: String },
    #[error("looks need the GL mixer (lmix-core --mixer gl)")]
    NoLooks,
    #[error("channel strip: {field} is out of range")]
    BadStrip { field: String },
    #[error("dynamic slot {slot} does not exist (1–{max})")]
    BadDynamic { slot: u8, max: u8 },
    #[error("PTZ: {reason}")]
    BadPtz { reason: String },
    #[error("no graphics channel {reference}")]
    UnknownGraphics { reference: String },
    #[error("all {max} graphics channels are in use")]
    GraphicsFull { max: usize },
    #[error("no template {template}")]
    UnknownTemplate { template: String },
    #[error("title: {reason}")]
    BadTitle { reason: String },
    #[error("no data set {name}")]
    UnknownDataset { name: String },
    #[error("no data source {reference}")]
    UnknownDataSource { reference: String },
    #[error("data source: {reason}")]
    BadDataSource { reason: String },
    #[error("show has no file yet; give a path")]
    NoShowPath,
    #[error("show file: {message}")]
    ShowFile { message: String },
    #[error("engine: {message}")]
    Engine { message: String },
}
