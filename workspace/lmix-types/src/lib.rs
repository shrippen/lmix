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
pub mod graphics;
pub mod input;
pub mod layout;
pub mod midi;
pub mod output;
pub mod shortcut;
pub mod state;

pub use action::{Action, ActionError, Effect, StingerSpec, TransitionSlot};
pub use automation::{Cue, CueList, Trigger, TriggerOn};
pub use graphics::{
    BindingSpec, Blend, DataFormat, DataSource, DataSourceSpec, Dataset, Easing, FieldBinding,
    FieldKind, Graphics, LayerTransform, Template, TemplateField, TemplateTexts, TitleCall,
    TitleData, TitleLayer, TitlePhase, TitleTrigger,
};
pub use input::{
    AuxBus, ChannelStrip, Compressor, Device, DeviceKind, Eq4, Gate, Hpf, Ingest, Input,
    InputAudio, InputColor, InputId, InputRef, InputStatus, Listen, Look, SignalLoss, SourceSpec,
    TestPattern, Transport,
};
pub use layout::{BoxCrop, BoxRect, BoxSpec, Layout, LayoutBox};
pub use midi::{MidiBinding, MidiKind, MidiMessage};
pub use output::{
    Container, DisplayMode, Output, OutputId, OutputRef, OutputSource, OutputSpec, OutputStats,
    OutputStatus,
};
pub use shortcut::{Shortcut, ShortcutSpec};
pub use state::{Event, InputLevel, Latency, Levels, LiveFeed, ShowState, Stinger, Transition};
