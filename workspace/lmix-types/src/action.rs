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

use crate::automation::{Cue, Trigger};
use crate::graphics::{
    BindingSpec, DataSourceSpec, Easing, LayerTransform, TitleData, TitleTrigger,
};
use crate::input::{
    ChannelStrip, InputColor, InputRef, Listen, Look, SignalLoss, SourceSpec, Transport, AUX_BUSES,
};
use crate::layout::BoxSpec;
use crate::output::{OutputRef, OutputSource, OutputSpec};
use crate::shortcut::ShortcutSpec;

/// 0-based index of aux bus `bus` (1-based, as in actions).
pub fn aux_index(bus: u8) -> Result<usize, ActionError> {
    let max = AUX_BUSES as u8;
    if bus == 0 || bus > max {
        return Err(ActionError::BadAuxBus { bus, max });
    }
    Ok(usize::from(bus - 1))
}

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
/// Overlay channels above program (OVL.1), numbered 1–4.
pub const OVERLAY_CHANNELS: u8 = 4;
/// Default fade of an overlay in or out.
pub const DEFAULT_OVERLAY_MS: u32 = 500;

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
    RemoveInput { input: InputRef },
    /// Bring back the input closed last, with its settings (AUT.10).
    RestoreInput,
    /// Change what an input shows, keeping its settings (INP.4).
    ReplaceSource { input: InputRef, source: SourceSpec },
    /// Rename an input.
    RenameInput { input: InputRef, name: String },
    /// Set or clear the color tag of an input (INP.3).
    SetInputColor {
        input: InputRef,
        #[serde(default)]
        color: Option<InputColor>,
    },
    /// Set an input's colour correction (needs the GL mixer).
    SetLook { input: InputRef, look: Look },
    /// Choose last frame or black for when the input's source stalls.
    SetSignalLoss { input: InputRef, mode: SignalLoss },
    /// Play, pause, restart or step a file, URL or playlist input.
    TransportInput {
        input: InputRef,
        transport: Transport,
    },

    // ---- Outputs (OUT, REC, STR) ----
    /// Add an output to the show; it starts stopped.
    AddOutput {
        spec: OutputSpec,
        /// Program (default) or one input (ISO, MultiCorder).
        #[serde(default)]
        source: OutputSource,
        /// Sound from aux bus 1–2 instead of the source's own.
        #[serde(default)]
        aux: Option<u8>,
        #[serde(default)]
        name: Option<String>,
    },
    /// Remove an output (stops it first).
    RemoveOutput { output: OutputRef },
    /// Start streaming / recording / sending.
    StartOutput { output: OutputRef },
    /// Stop it.
    StopOutput { output: OutputRef },

    // ---- Audio (AUD) ----
    /// Switch an input's sound on or off.
    SetAudio { input: InputRef, on: bool },
    /// Audio follows video: the input is heard only while on program.
    SetAudioFollow { input: InputRef, follow: bool },
    /// Set an input's volume, 0–100 (100 = unity gain).
    SetVolume { input: InputRef, volume: u8 },
    /// Set an input's channel strip: trim, pan, filter, EQ, dynamics,
    /// delay.
    SetChannelStrip {
        input: InputRef,
        strip: ChannelStrip,
    },
    /// Send an input to aux bus `bus` (1–2) at `level`, 0–100; 0 stops
    /// the send.
    SetAuxSend { input: InputRef, bus: u8, level: u8 },
    /// Master of aux bus `bus` (1–2): volume 0–100 and mute.
    SetAuxBus { bus: u8, volume: u8, mute: bool },
    /// Listen to an input on the monitor bus, before or after its fader.
    /// While any input is soloed, the monitor bus carries only the solos;
    /// otherwise it follows program.
    SetListen { input: InputRef, listen: Listen },
    /// End every solo: the monitor bus follows program again.
    ClearListen,

    /// Move an input to a 1-based position in the input bar (INP.3).
    MoveInput { input: InputRef, number: u32 },

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
    /// Remove a layout (not while it is on program).
    RemoveLayout { layout: InputRef },
    /// Put a layout into preview.
    PreviewLayout { layout: InputRef },

    // ---- Automation (AUT.7, AUT.14) ----
    /// Replace an input's triggers.
    SetTriggers {
        input: InputRef,
        triggers: Vec<Trigger>,
    },
    /// Add a cue list.
    AddCueList { name: String, cues: Vec<Cue> },
    /// Replace a cue list's name and cues (by number, name or id); the
    /// next cue stays where it was, at most one past the last.
    SetCueList {
        list: InputRef,
        name: String,
        cues: Vec<Cue>,
    },
    /// Remove a cue list.
    RemoveCueList { list: InputRef },
    /// Run the list's next cue and move on.
    CueGo { list: InputRef },
    /// Run the cue before the last one run and step back.
    CueBack { list: InputRef },
    /// Make cue `cue` (1-based) the next one, without running it.
    CueJump { list: InputRef, cue: u32 },

    // ---- Safety ----
    /// Lock or unlock (AUT.11): while locked, closing inputs, opening or
    /// clearing the show and stopping or removing outputs are refused.
    SetLock { on: bool },
    /// Bind a MIDI controller (`cc`) to the T-bar: 0–127 moves it from
    /// 0 to 1; `null` unbinds.
    SetMidiTbar {
        #[serde(default)]
        midi: Option<crate::midi::MidiBinding>,
    },

    // ---- Shortcuts (AUT.1–3) ----
    /// Add a shortcut: a key and label for a list of actions.
    AddShortcut { shortcut: ShortcutSpec },
    /// Replace a shortcut (by number, label or id).
    SetShortcut {
        target: InputRef,
        shortcut: ShortcutSpec,
    },
    /// Remove a shortcut.
    RemoveShortcut { target: InputRef },
    /// Run a shortcut's actions in order; the first failing one stops
    /// the rest.
    RunShortcut { target: InputRef },

    // ---- Switching (SWI) ----
    /// Put an input into preview.
    PreviewInput { input: InputRef },
    /// Swap preview and program instantly.
    Cut,
    /// Put an input on program instantly, skipping preview.
    CutDirect { input: InputRef },
    /// Take preview to program with transition button `slot` (1–4).
    Transition { slot: TransitionSlot },
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
    /// Move the T-bar (SWI.3): mix preview over program by `position`
    /// (0–1). Reaching 1 takes preview to program; back at 0 leaves both
    /// as they were.
    TBar { position: f32 },

    // ---- Overlays (OVL) ----
    /// Show `input` on overlay `channel` (1–4), above program and its
    /// transitions; replaces what the channel showed.
    OverlayIn {
        channel: u8,
        input: InputRef,
        #[serde(default = "default_overlay_ms")]
        duration_ms: u32,
    },
    /// Take overlay `channel` off.
    OverlayOut {
        channel: u8,
        #[serde(default = "default_overlay_ms")]
        duration_ms: u32,
    },

    // ---- Dynamic inputs and values (AUT.5) ----
    /// Let `"dynamic<slot>"` (1–4) name `input` in later actions; null
    /// empties it. One key picks the input, another acts on it.
    SetDynamicInput {
        slot: u8,
        #[serde(default)]
        input: Option<InputRef>,
    },
    /// Set dynamic value `slot` (1–4); title texts and function arguments
    /// show it where they say `{value<slot>}`.
    SetDynamicValue { slot: u8, value: String },

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
    PtzZoom { input: InputRef, speed: f32 },
    /// Focus: auto, manual, near, far, stop.
    PtzFocus {
        input: InputRef,
        focus: crate::ptz::PtzFocus,
    },
    /// Back to the camera's home position.
    PtzHome { input: InputRef },
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
    RemoveGraphics { graphics: InputRef },
    /// Rename a graphics channel.
    RenameGraphics { graphics: InputRef, name: String },
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
    RemoveDataset { name: String },
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
    RemoveDataSource { source: InputRef },
    /// Select row `row` (1-based).
    DataRow { source: InputRef, row: u32 },
    /// Select the next row (the first after the last with `loop_rows`).
    DataNext { source: InputRef },
    /// Select the previous row.
    DataPrevious { source: InputRef },
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
    /// Start an empty show.
    NewShow,
    /// Save the show; without `path` to its current file.
    SaveShow {
        #[serde(default)]
        path: Option<String>,
    },
    /// Open a show file, replacing the current show.
    OpenShow { path: String },
    /// Write the show with its media, data files and own title templates
    /// into one package file (SHW.6).
    ExportPackage { path: String },
    /// Unpack a package into `folder` (new or empty) and open its show.
    ImportPackage { path: String, folder: String },
}

/// Default FTB fade.
pub const DEFAULT_FTB_MS: u32 = 1000;

fn default_ftb_ms() -> u32 {
    DEFAULT_FTB_MS
}

fn default_overlay_ms() -> u32 {
    DEFAULT_OVERLAY_MS
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
    #[error("volume {volume} is outside 0–{max}")]
    BadVolume { volume: u8, max: u8 },
    #[error("overlay channel {channel} does not exist (1–{max})")]
    BadOverlay { channel: u8, max: u8 },
    #[error("T-bar position is outside 0–1")]
    BadTBar,
    #[error("aux bus {bus} does not exist (1–{max})")]
    BadAuxBus { bus: u8, max: u8 },
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
