//! How busy the machine running lmix-core is, and how its cue timer runs
//! (show mode RUN.8, SNF.4). On a loaded machine cue steps run late
//! (DECISIONS 2026-10-17), and without real-time priority more so; the
//! Console warns before and during a show.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// CPU use, in percent, from which the machine counts as loaded.
pub const HIGH_CPU_PERCENT: u8 = 85;

/// Runnable processes per core (1-minute load average) from which the
/// machine counts as loaded.
pub const HIGH_LOAD_PER_CORE: f32 = 1.0;

/// How the cue timer thread runs (DECISIONS 2026-10-18, RUN.8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TimerPriority {
    /// Real-time (`SCHED_FIFO`), set directly or granted by rtkit.
    Realtime,
    /// Like every other thread: neither way granted real time.
    #[default]
    Normal,
    /// Gave real time up at the soft limit of RLIMIT_RTTIME, for the rest
    /// of the run.
    Demoted,
    /// The timer thread ended: waits and follows no longer run.
    Stopped,
}

/// The machine's load, about every two seconds (`Event::Machine`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct MachineLoad {
    /// CPU use of all cores since the last report, 0–100.
    pub cpu_percent: u8,
    /// 1-minute load average divided by the number of cores.
    pub load_per_core: f32,
    pub cores: u32,
    pub cue_timer: TimerPriority,
    /// A cue step ran more than its budget late in the last 30 s.
    pub cues_late: bool,
    /// Loaded, or cues ran late: cue times may not hold (RUN.8).
    pub high: bool,
}
