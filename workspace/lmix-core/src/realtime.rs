//! Real-time priority for the cue timer (show mode RUN.8): a busy machine
//! must not make waits and follows late.
//!
//! ```text
//!   cue thread ─► SCHED_FIFO itself (RLIMIT_RTPRIO allows it, or root)
//!              └► else ask rtkit over D-Bus, as PipeWire does (no root)
//!              └► else a normal thread: warn, /metrics shows 0
//! ```
//!
//! rtkit only grants threads of a process that limits their CPU time
//! (RLIMIT_RTTIME), so a runaway real-time thread cannot freeze the
//! machine; the limit is set before asking.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Priority asked for directly. rtkit grants at most its own maximum
/// (usually 20), which is asked for there.
const PRIORITY: i32 = 20;

/// CPU time a real-time thread may use without sleeping (rtkit's default
/// maximum). The cue thread sleeps between steps, so it never comes near.
const RT_TIME: Duration = Duration::from_millis(200);

static REALTIME: AtomicBool = AtomicBool::new(false);

/// Whether the cue timer runs with real-time priority (for `/metrics`).
pub fn granted() -> bool {
    REALTIME.load(Ordering::Relaxed)
}

/// Give the calling thread real-time priority. Returns how it was granted,
/// or why not.
pub fn make_current_thread_realtime() -> Result<&'static str, String> {
    let how = match fifo(PRIORITY) {
        Ok(()) => "SCHED_FIFO",
        Err(direct) => match rtkit() {
            Ok(()) => "rtkit",
            Err(kit) => return Err(format!("{direct}; rtkit: {kit}")),
        },
    };
    REALTIME.store(true, Ordering::Relaxed);
    Ok(how)
}

fn fifo(priority: i32) -> Result<(), String> {
    let param = libc::sched_param {
        sched_priority: priority,
    };
    // SAFETY: plain syscall on the calling thread (pid 0) with a valid
    // parameter struct.
    let rc = unsafe {
        libc::sched_setscheduler(0, libc::SCHED_FIFO | libc::SCHED_RESET_ON_FORK, &param)
    };
    if rc == 0 {
        Ok(())
    } else {
        Err(format!("SCHED_FIFO: {}", std::io::Error::last_os_error()))
    }
}

fn rtkit() -> Result<(), String> {
    let rt_time = RT_TIME.as_micros() as libc::rlim_t;
    let limit = libc::rlimit {
        rlim_cur: rt_time,
        rlim_max: rt_time,
    };
    // SAFETY: valid rlimit struct for this process.
    if unsafe { libc::setrlimit(libc::RLIMIT_RTTIME, &limit) } != 0 {
        return Err(format!(
            "RLIMIT_RTTIME: {}",
            std::io::Error::last_os_error()
        ));
    }
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
    // SAFETY: gettid has no preconditions.
    let thread = unsafe { libc::gettid() } as u64;
    let priority = max.clamp(1, PRIORITY) as u32;
    kit.call_method("MakeThreadRealtime", &(thread, priority))
        .map_err(|e| e.to_string())?;
    Ok(())
}
