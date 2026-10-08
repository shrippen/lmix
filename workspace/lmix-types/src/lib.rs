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
pub mod input;
pub mod output;
pub mod state;

pub use action::{Action, ActionError, Effect, TransitionSlot};
pub use input::{
    Device, DeviceKind, Ingest, Input, InputAudio, InputColor, InputId, InputRef, InputStatus,
    Listen, SignalLoss, SourceSpec, TestPattern, Transport,
};
pub use output::{
    Container, DisplayMode, Output, OutputId, OutputRef, OutputSource, OutputSpec, OutputStatus,
};
pub use state::{Event, InputLevel, Latency, Levels, ShowState, Transition};
