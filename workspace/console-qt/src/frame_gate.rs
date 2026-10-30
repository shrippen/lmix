//! Hands the frames of all live pictures to Qt together. The engine
//! sends program and the tile picture in step, but a millisecond apart:
//! the first starts a frame of the window at once, the second, arriving
//! while that frame waits for the display, a second one. Held at the gate
//! until the others are there too (at most `WAIT`), they are drawn in one
//! frame: the window draws at the engine's rate, not twice that.
//!
//! The first frame of a batch waits, the last one never does: a picture
//! is only held for the ones behind it, at most `WAIT`.

use std::sync::{Condvar, Mutex};
use std::time::{Duration, Instant};

/// How long the first frame of a batch waits for the others: well under
/// a frame, so a picture that runs out of step (or stalls) costs little.
pub const WAIT: Duration = Duration::from_millis(4);

#[derive(Default)]
struct State {
    /// Live pictures that pass the gate.
    members: usize,
    /// Frames of the batch now collecting; 0 while none is.
    arrived: usize,
    deadline: Option<Instant>,
    /// Bumped when a batch is let through.
    batch: u64,
}

pub struct FrameGate {
    state: Mutex<State>,
    through: Condvar,
    wait: Duration,
}

/// A live picture's place at the gate; leaves it when dropped.
pub struct Member(&'static FrameGate);

impl Drop for Member {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.members -= 1;
        // The batch may have waited for this one only.
        if state.arrived > 0 && state.arrived >= state.members {
            self.0.open(&mut state);
        }
    }
}

impl Member {
    /// Wait until this frame's batch goes through (see the module).
    pub fn pass(&self) {
        self.0.pass();
    }
}

impl FrameGate {
    pub const fn new(wait: Duration) -> Self {
        Self {
            state: Mutex::new(State {
                members: 0,
                arrived: 0,
                deadline: None,
                batch: 0,
            }),
            through: Condvar::new(),
            wait,
        }
    }

    pub fn join(&'static self) -> Member {
        self.lock().members += 1;
        Member(self)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn open(&self, state: &mut State) {
        state.arrived = 0;
        state.deadline = None;
        state.batch += 1;
        self.through.notify_all();
    }

    fn pass(&self) {
        let mut state = self.lock();
        if state.members <= 1 {
            return;
        }
        let deadline = *state
            .deadline
            .get_or_insert_with(|| Instant::now() + self.wait);
        state.arrived += 1;
        if state.arrived >= state.members {
            self.open(&mut state);
            return;
        }
        let batch = state.batch;
        while state.batch == batch {
            let now = Instant::now();
            if now >= deadline {
                self.open(&mut state);
                return;
            }
            state = self
                .through
                .wait_timeout(state, deadline - now)
                .unwrap_or_else(|e| e.into_inner())
                .0;
        }
    }
}

/// The gate of this process's live pictures.
pub static GATE: FrameGate = FrameGate::new(WAIT);

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(wait: Duration) -> &'static FrameGate {
        Box::leak(Box::new(FrameGate::new(wait)))
    }

    #[test]
    fn a_picture_alone_never_waits() {
        let member = gate(Duration::from_secs(5)).join();
        let start = Instant::now();
        member.pass();
        assert!(start.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn the_first_frame_waits_for_the_second() {
        let gate = gate(Duration::from_secs(5));
        let (a, b) = (gate.join(), gate.join());
        let first = std::thread::spawn(move || {
            a.pass();
            Instant::now()
        });
        std::thread::sleep(Duration::from_millis(50));
        let second = Instant::now();
        b.pass();
        let through = first.join().unwrap();
        // Held until the second came, not the 5 s.
        assert!(through >= second);
        assert!(through - second < Duration::from_secs(1));
    }

    #[test]
    fn a_missing_picture_holds_the_others_only_briefly() {
        let gate = gate(Duration::from_millis(20));
        let (a, _stalled) = (gate.join(), gate.join());
        let start = Instant::now();
        a.pass();
        let waited = start.elapsed();
        assert!(waited >= Duration::from_millis(20));
        assert!(waited < Duration::from_secs(1));
        // The next batch starts afresh.
        let start = Instant::now();
        a.pass();
        assert!(start.elapsed() >= Duration::from_millis(20));
    }

    #[test]
    fn a_picture_that_leaves_lets_the_batch_through() {
        let gate = gate(Duration::from_secs(5));
        let (a, b) = (gate.join(), gate.join());
        let first = std::thread::spawn(move || {
            let start = Instant::now();
            a.pass();
            start.elapsed()
        });
        std::thread::sleep(Duration::from_millis(50));
        drop(b);
        assert!(first.join().unwrap() < Duration::from_secs(1));
    }
}
