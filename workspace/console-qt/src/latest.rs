//! The newest of a stream of values for the GUI thread. A state arrives
//! for every operator move; queued one by one, a busy window worked
//! through them long after the drag (29.10.2026). Now a value waiting for
//! the GUI thread is replaced by a newer one instead of queued behind it.

use std::sync::{Arc, Mutex};

pub struct Latest<T>(Arc<Mutex<Option<T>>>);

impl<T> Clone for Latest<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> Latest<T> {
    pub fn new() -> Self {
        Self(Arc::new(Mutex::new(None)))
    }

    /// Keeps `value` for the next `take`; true when nothing was waiting,
    /// so the caller has to wake the GUI thread for it.
    pub fn put(&self, value: T) -> bool {
        self.0.lock().expect("latest lock").replace(value).is_none()
    }

    /// The newest value since the last take.
    pub fn take(&self) -> Option<T> {
        self.0.lock().expect("latest lock").take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_burst_wakes_once_and_hands_over_the_newest() {
        let latest = Latest::new();
        let wakes = (1..=100).filter(|v| latest.put(*v)).count();
        assert_eq!(wakes, 1);
        assert_eq!(latest.take(), Some(100));
        assert_eq!(latest.take(), None);
    }

    #[test]
    fn after_a_take_the_next_value_wakes_again() {
        let latest = Latest::new();
        assert!(latest.put("a"));
        assert_eq!(latest.take(), Some("a"));
        assert!(latest.put("b"));
        assert!(!latest.put("c"));
        assert_eq!(latest.take(), Some("c"));
    }
}
