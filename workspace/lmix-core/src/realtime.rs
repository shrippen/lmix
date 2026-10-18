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
//! runaway real-time thread cannot freeze the machine. The kernel stops
//! the process at that limit, so the cue thread only takes due steps;
//! what they do runs on the shared runtime.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Priority asked for directly. rtkit grants at most its own maximum
/// (usually 20), which is asked for there.
const PRIORITY: i32 = 20;

/// CPU time a real-time thread may use without sleeping (rtkit's default
/// maximum). Taking due steps needs microseconds.
const RT_TIME: Duration = Duration::from_millis(200);

static REALTIME: AtomicBool = AtomicBool::new(false);

/// Whether the cue timer runs with real-time priority (for `/metrics`).
pub fn granted() -> bool {
    REALTIME.load(Ordering::Relaxed)
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

fn limit_rt_time() -> Result<(), String> {
    let rt_time = RT_TIME.as_micros() as libc::rlim_t;
    let limit = libc::rlimit {
        rlim_cur: rt_time,
        rlim_max: rt_time,
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
