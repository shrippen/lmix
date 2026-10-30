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
pub mod bus;
pub mod card;
pub mod cue;
pub mod feed;
pub mod graphics;
pub mod input;
pub mod layout;
pub mod machine;
pub mod midi;
pub mod output;
pub mod ptz;
pub mod shortcut;
pub mod sound;
pub mod stage;
pub mod state;
pub mod target;
pub mod zone;

pub use action::{
    action_group, Action, ActionError, Effect, IoClass, StingerSpec, TransitionSlot, VolumeTarget,
    ACTION_GROUPS, FADE_MAX_MS,
};
pub use automation::{Trigger, TriggerOn};
pub use bus::{find_bus, Bus, BusId, BusPeak, BusRef, Fader, Phones, PhonesSend, BUSES};
pub use card::{CardChannels, CardMode, Scan, MAX_CARD_SOUND_DELAY_MS};
pub use cue::{
    Cue, CueEvent, CueId, CueKind, CueList, CueTarget, Follow, GroupMode, RunPhase, RunningCue,
    StandbyMove, StopMode, StopOthers, StopScope,
};
pub use feed::{Feed, FeedMessage, FeedOn};
pub use graphics::{
    BindingSpec, Blend, DataFormat, DataSource, DataSourceSpec, DataTable, Dataset, Easing,
    FieldBinding, FieldKind, Graphics, LayerTransform, Template, TemplateField, TemplateTexts,
    TitleCall, TitleData, TitleLayer, TitlePhase, TitleTrigger,
};
pub use input::{
    device_framerate, device_resolution, AtEnd, AutoPlay, ChannelStrip, Compressor, Device,
    DeviceKind, Eq4, Gate, Hpf, Ingest, Input, InputAudio, InputColor, InputId, InputPress,
    InputRef, InputStatus, Listen, Look, PageRunning, Playback, PlaybackState, SignalLoss,
    SourceSpec, TestPattern, Transport,
};
pub use layout::{
    BoxCrop, BoxRect, BoxShown, BoxSpec, Layout, LayoutBox, LayoutTemplate, TemplateBox,
    TemplateInput,
};
pub use machine::{MachineLoad, TimerPriority};
pub use midi::{MidiBinding, MidiKind, MidiMessage};
pub use output::{
    Container, DisplayMode, Framerate, MasterFormat, MultiviewLayout, MultiviewMeters, Output,
    OutputGroup, OutputId, OutputRef, OutputSource, OutputSpec, OutputStats, OutputStatus,
    ProgramSide, RateMode, VideoCodec, VideoEncoding,
};
pub use ptz::{PtzFocus, PtzProtocol, PtzSpec};
pub use shortcut::{Shortcut, ShortcutSpec};
pub use sound::{AfterFade, FadeCue, FadeTarget, SoundCue, SoundRepeat, SoundRoute};
pub use stage::{PictureCue, PictureEnd, PictureFit, Stage, StageOutput, StagePatch, StageState};
pub use state::{
    Event, InputLevel, Latency, Levels, LiveFeed, LiveTiles, LiveTransport, OverlayChannel,
    QuickPlay, RemovedKind, Restorable, ShowState, Stinger, Transition, ZonePeak,
};
pub use target::{HttpMethod, Target, TargetKind, TargetResult, TargetSpec};
pub use zone::{Zone, ZoneOutput, ZonePatch, ZoneState};
