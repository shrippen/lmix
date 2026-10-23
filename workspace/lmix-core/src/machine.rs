//! The machine's load for clients (RUN.8, SNF.4): CPU use between two
//! reads of `/proc/stat`, the load average per core, and whether cue steps
//! ran late lately. Reading the files is the caller's; this is the sum.
//! `Service::watch_machine` sends it as `Event::Machine`.

use std::time::{Duration, Instant};

use lmix_types::machine::{HIGH_CPU_PERCENT, HIGH_LOAD_PER_CORE};
use lmix_types::{MachineLoad, TimerPriority};

/// How long a late cue step (over [`crate::cues::LATE_BUDGET`]) keeps the
/// warning on.
pub const LATE_HOLD: Duration = Duration::from_secs(30);

/// Busy and total jiffies of all cores (`cpu` line of `/proc/stat`).
fn jiffies(stat: &str) -> Option<(u64, u64)> {
    let line = stat.lines().find(|l| l.starts_with("cpu "))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    // user nice system idle iowait irq softirq steal (guest is in user).
    let total: u64 = values.iter().take(8).sum();
    let idle = values.get(3)? + values.get(4).copied().unwrap_or(0);
    Some((total - idle, total))
}

/// Cores counted in `/proc/stat` (`cpu0`, `cpu1`, …).
fn cores(stat: &str) -> u32 {
    let n = stat
        .lines()
        .filter(|l| l.starts_with("cpu") && l.as_bytes().get(3).is_some_and(u8::is_ascii_digit))
        .count();
    u32::try_from(n.max(1)).unwrap_or(1)
}

/// Turns reads of the system files into [`MachineLoad`].
#[derive(Default)]
pub struct Sampler {
    last: Option<(u64, u64)>,
    over_budget: Option<u64>,
    late_until: Option<Instant>,
}

impl Sampler {
    /// One report from `/proc/stat`, `/proc/loadavg`, the count of cue
    /// steps over budget so far and how the timer runs.
    pub fn sample(
        &mut self,
        stat: &str,
        loadavg: &str,
        over_budget: u64,
        cue_timer: TimerPriority,
        now: Instant,
    ) -> MachineLoad {
        let jiffies = jiffies(stat);
        let cpu_percent = match (self.last, jiffies) {
            (Some((busy0, total0)), Some((busy1, total1))) if total1 > total0 => {
                let busy = busy1.saturating_sub(busy0) as f64;
                (busy * 100.0 / (total1 - total0) as f64).round().min(100.0) as u8
            }
            _ => 0,
        };
        self.last = jiffies.or(self.last);

        let cores = cores(stat);
        let load: f32 = loadavg
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.0);
        let load_per_core = load / cores as f32;

        if self.over_budget.is_some_and(|before| over_budget > before) {
            self.late_until = Some(now + LATE_HOLD);
        }
        self.over_budget = Some(over_budget);
        let cues_late = self.late_until.is_some_and(|until| now < until);

        MachineLoad {
            cpu_percent,
            load_per_core,
            cores,
            cue_timer,
            cues_late,
            high: cues_late
                || cpu_percent >= HIGH_CPU_PERCENT
                || load_per_core >= HIGH_LOAD_PER_CORE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stat(busy: u64, idle: u64) -> String {
        // user nice system idle iowait irq softirq steal guest guest_nice
        format!(
            "cpu  {busy} 0 0 {idle} 0 0 0 0 0 0\ncpu0 1 0 0 1 0 0 0 0 0 0\ncpu1 1 0 0 1 0 0 0 0 0 0\nintr 5\n"
        )
    }

    #[test]
    fn cpu_use_is_between_two_reads() {
        let mut sampler = Sampler::default();
        let now = Instant::now();
        let first = sampler.sample(
            &stat(100, 100),
            "0.50 0.4 0.3 1/200 9",
            0,
            TimerPriority::Normal,
            now,
        );
        assert_eq!(first.cpu_percent, 0, "nothing to compare with yet");
        assert_eq!(first.cores, 2);
        assert!((first.load_per_core - 0.25).abs() < 1e-6);
        assert!(!first.high);

        let second = sampler.sample(
            &stat(190, 110),
            "0.50 0.4 0.3 1/200 9",
            0,
            TimerPriority::Normal,
            now,
        );
        assert_eq!(second.cpu_percent, 90);
        assert!(second.high, "90 % is loaded");
    }

    #[test]
    fn a_full_run_queue_is_loaded() {
        let load = Sampler::default().sample(
            &stat(1, 1),
            "2.10 1 1 3/200 9",
            0,
            TimerPriority::Normal,
            Instant::now(),
        );
        assert!(load.load_per_core > 1.0);
        assert!(load.high);
    }

    #[test]
    fn late_cue_steps_warn_for_a_while() {
        let mut sampler = Sampler::default();
        let t0 = Instant::now();
        let idle = "0.1 0.1 0.1 1/200 9";
        assert!(
            !sampler
                .sample(&stat(1, 100), idle, 3, TimerPriority::Normal, t0)
                .cues_late
        );
        assert!(
            !sampler
                .sample(&stat(2, 200), idle, 3, TimerPriority::Normal, t0)
                .high,
            "no new late step"
        );

        let late = sampler.sample(&stat(3, 300), idle, 4, TimerPriority::Normal, t0);
        assert!(late.cues_late && late.high);
        let still = sampler.sample(
            &stat(4, 400),
            idle,
            4,
            TimerPriority::Normal,
            t0 + LATE_HOLD - Duration::from_secs(1),
        );
        assert!(still.cues_late);
        let over = sampler.sample(
            &stat(5, 500),
            idle,
            4,
            TimerPriority::Normal,
            t0 + LATE_HOLD,
        );
        assert!(!over.cues_late);
    }
}
