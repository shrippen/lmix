//! Real-time priority for the cue timer (show mode RUN.8): a busy machine
//! must not make waits and follows late.
//!
//! ```text
//!   cue thread  ─► runs the timer at once
//!   helper      ─► cue thread SCHED_FIFO (RLIMIT_RTPRIO allows it, or root)
//!               └► else rtkit over D-Bus, as PipeWire does (no root)
//!               └► else a normal thread: warn, /metrics shows 0
//! ```
//!
//! A helper asks, so a hung system bus never holds up the timer. Both
//! ways first limit the CPU time a real-time thread may use without
//! sleeping (RLIMIT_RTTIME): rtkit grants nothing without it, and a
//! runaway real-time thread cannot freeze the machine. At the hard limit
//! the kernel kills the process, so the soft limit is lower: its SIGXCPU
//! makes the cue thread a normal thread again, and the show goes on. The
//! cues' actions run on that thread; one that computes that long (opening
//! a show with large data sources) costs the priority, not the show.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Duration;

/// Priority asked for directly. rtkit grants at most its own maximum
/// (usually 20), which is asked for there.
const PRIORITY: i32 = 20;

/// CPU time a real-time thread may use without sleeping: SIGXCPU at the
/// soft limit, the end of the process at the hard one (rtkit's default
/// maximum).
const RT_TIME_SOFT: Duration = Duration::from_millis(100);
const RT_TIME_HARD: Duration = Duration::from_millis(200);

static REALTIME: AtomicBool = AtomicBool::new(false);
static DEMOTED: AtomicBool = AtomicBool::new(false);
/// The thread with real-time priority, for the SIGXCPU handler.
static RT_THREAD: AtomicI32 = AtomicI32::new(0);

/// Whether the cue timer runs with real-time priority (for `/metrics`).
pub fn granted() -> bool {
    REALTIME.load(Ordering::Relaxed)
}

/// Whether the timer gave its priority up at the soft limit.
pub fn demoted() -> bool {
    DEMOTED.load(Ordering::Relaxed)
}

/// The cue thread ended: no real-time timer any more.
pub fn ended() {
    REALTIME.store(false, Ordering::Relaxed);
}

/// The calling thread's kernel id, for [`make_realtime`].
pub fn thread_id() -> libc::pid_t {
    // SAFETY: gettid has no preconditions.
    unsafe { libc::gettid() }
}

/// Give thread `tid` of this process real-time priority. Returns how it
/// was granted, or why not.
pub fn make_realtime(tid: libc::pid_t) -> Result<&'static str, String> {
    RT_THREAD.store(tid, Ordering::Relaxed);
    limit_rt_time()?;
    let how = match fifo(tid, PRIORITY) {
        Ok(()) => "SCHED_FIFO",
        Err(direct) => match rtkit(tid) {
            Ok(()) => "rtkit",
            Err(kit) => return Err(format!("{direct}; rtkit: {kit}")),
        },
    };
    REALTIME.store(true, Ordering::Relaxed);
    Ok(how)
}

/// SIGXCPU: the real-time thread computed too long. Back to a normal
/// thread; only async-signal-safe calls here.
extern "C" fn on_xcpu(_: libc::c_int) {
    let param = libc::sched_param { sched_priority: 0 };
    let tid = RT_THREAD.load(Ordering::Relaxed);
    // SAFETY: sched_setscheduler is async-signal-safe (a plain syscall).
    unsafe { libc::sched_setscheduler(tid, libc::SCHED_OTHER, &param) };
    REALTIME.store(false, Ordering::Relaxed);
    DEMOTED.store(true, Ordering::Relaxed);
}

/// Soft and hard RLIMIT_RTTIME (a lower hard limit already set stays),
/// and the SIGXCPU handler for the soft one.
fn limit_rt_time() -> Result<(), String> {
    let mut current = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: valid out pointer.
    unsafe { libc::getrlimit(libc::RLIMIT_RTTIME, &mut current) };
    let hard = (RT_TIME_HARD.as_micros() as libc::rlim_t).min(current.rlim_max);
    let soft = (RT_TIME_SOFT.as_micros() as libc::rlim_t).min(hard / 2);
    // SAFETY: installing a handler that only makes async-signal-safe calls.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_xcpu as *const () as usize;
        libc::sigemptyset(&mut action.sa_mask);
        if libc::sigaction(libc::SIGXCPU, &action, std::ptr::null_mut()) != 0 {
            return Err(format!("SIGXCPU: {}", std::io::Error::last_os_error()));
        }
    }
    let limit = libc::rlimit {
        rlim_cur: soft,
        rlim_max: hard,
    };
    // SAFETY: valid rlimit struct for this process.
    if unsafe { libc::setrlimit(libc::RLIMIT_RTTIME, &limit) } == 0 {
        Ok(())
    } else {
        Err(format!(
            "RLIMIT_RTTIME: {}",
            std::io::Error::last_os_error()
        ))
    }
}

fn fifo(tid: libc::pid_t, priority: i32) -> Result<(), String> {
    let param = libc::sched_param {
        sched_priority: priority,
    };
    // SAFETY: plain syscall on one of this process's threads with a valid
    // parameter struct.
    let rc = unsafe {
        libc::sched_setscheduler(tid, libc::SCHED_FIFO | libc::SCHED_RESET_ON_FORK, &param)
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!("SCHED_FIFO: {}", std::io::Error::last_os_error()))
    }
}

fn rtkit(tid: libc::pid_t) -> Result<(), String> {
    let bus = zbus::blocking::Connection::system().map_err(|e| e.to_string())?;
    let kit = zbus::blocking::Proxy::new(
        &bus,
        "org.freedesktop.RealtimeKit1",
        "/org/freedesktop/RealtimeKit1",
        "org.freedesktop.RealtimeKit1",
    )
    .map_err(|e| e.to_string())?;
    let max: i32 = kit
        .get_property("MaxRealtimePriority")
        .map_err(|e| e.to_string())?;
    let priority = max.clamp(1, PRIORITY) as u32;
    kit.call_method("MakeThreadRealtime", &(tid as u64, priority))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real-time thread that computes past the soft limit becomes a
    /// normal thread instead of ending the process. Needs the right to
    /// SCHED_FIFO (root or RLIMIT_RTPRIO); without it there is nothing to
    /// guard and the test says so.
    #[test]
    fn a_long_step_costs_the_priority_not_the_process() {
        let worker = std::thread::spawn(|| {
            if let Err(why) = make_realtime(thread_id()) {
                eprintln!("skipped, no real-time priority here: {why}");
                return None;
            }
            let start = std::time::Instant::now();
            while !demoted() && start.elapsed() < Duration::from_millis(1500) {
                std::hint::spin_loop();
            }
            // SAFETY: plain query of the calling thread.
            let policy = unsafe { libc::sched_getscheduler(0) };
            Some((demoted(), policy))
        });
        if let Some((demoted, policy)) = worker.join().unwrap() {
            assert!(demoted, "SIGXCPU came");
            assert_eq!(policy, libc::SCHED_OTHER, "a normal thread again");
            assert!(!granted());
        }
    }
}
