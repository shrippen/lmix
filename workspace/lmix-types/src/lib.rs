//! API types shared by `lmix-core` and its clients (GUI, remote control,
//! third-party tools).
//!
//! ```text
//!   client ──Action──► lmix-core ──State/Event──► every client
//! ```
//!
//! Every way of operating lmix (GUI, keys, API, triggers, scripts) sends an
//! [`Action`]; `lmix-core` answers with the resulting [`ShowState`].
//! See `docs/LASTENHEFT.md` §2.3 and API.1.

pub mod action;
pub mod automation;
pub mod cue;
pub mod feed;
pub mod graphics;
pub mod input;
pub mod layout;
pub mod midi;
pub mod output;
pub mod ptz;
pub mod shortcut;
pub mod state;
pub mod target;

pub use action::{
    action_group, Action, ActionError, Effect, StingerSpec, TransitionSlot, ACTION_GROUPS,
};
pub use automation::{Trigger, TriggerOn};
pub use cue::{
    Cue, CueEvent, CueId, CueKind, CueList, CueTarget, Follow, GroupMode, RunPhase, RunningCue,
    StandbyMove, StopMode, StopOthers, StopScope,
};
pub use graphics::{
    BindingSpec, Blend, DataFormat, DataSource, DataSourceSpec, DataTable, Dataset, Easing,
    FieldBinding, FieldKind, Graphics, LayerTransform, Template, TemplateField, TemplateTexts,
    TitleCall, TitleData, TitleLayer, TitlePhase, TitleTrigger,
};
pub use input::{
    AutoPlay, AuxBus, ChannelStrip, Compressor, Device, DeviceKind, Eq4, Gate, Hpf, Ingest, Input,
    InputAudio, InputColor, InputId, InputPress, InputRef, InputStatus, Listen, Look, SignalLoss,
    SourceSpec, TestPattern, Transport,
};
pub use layout::{
    BoxCrop, BoxRect, BoxSpec, Layout, LayoutBox, LayoutTemplate, TemplateBox, TemplateInput,
};
pub use midi::{MidiBinding, MidiKind, MidiMessage};
pub use output::{
    Container, DisplayMode, Output, OutputId, OutputRef, OutputSource, OutputSpec, OutputStats,
    OutputStatus,
};
pub use ptz::{PtzFocus, PtzProtocol, PtzSpec};
pub use shortcut::{Shortcut, ShortcutSpec};
pub use state::{
    Event, InputLevel, Latency, Levels, LiveFeed, OverlayChannel, QuickPlay, ShowState, Stinger,
    Transition,
};
pub use feed::{Feed, FeedMessage, FeedOn};
pub use target::{HttpMethod, Target, TargetKind, TargetResult, TargetSpec};
