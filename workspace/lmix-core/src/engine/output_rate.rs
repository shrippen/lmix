//! Rate of an output from strom's byte counters (`/flows/{id}/output-bytes`):
//! the bytes over the last few seconds of readings.
//!
//! Over a window, not between two readings: a recording grows in steps
//! (fragmented MP4 writes one fragment a second), and read once a second
//! it showed 0 and then double the rate in turns.

use lmix_types::OutputId;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// The span the rate is taken over. Readings older than this count no
/// more; an output with no newer one starts anew (it stopped in between,
/// or the poll was held up).
const WINDOW: Duration = Duration::from_secs(4);

/// The recent readings of each output, oldest first.
#[derive(Default)]
pub struct OutputRates {
    readings: HashMap<OutputId, VecDeque<(u64, Instant)>>,
}

impl OutputRates {
    /// kbit/s of `id` over its readings of the last `WINDOW`, `bytes` at
    /// `now` included; `None` for the first reading, after a gap of a
    /// whole window, and when the counter went back (a new start, a
    /// reconnect).
    pub fn rate(&mut self, id: OutputId, bytes: u64, now: Instant) -> Option<u32> {
        let fresh = |at: &Instant| now.saturating_duration_since(*at) < WINDOW;
        self.readings
            .retain(|_, r| r.back().is_some_and(|(_, at)| fresh(at)));
        let readings = self.readings.entry(id).or_default();
        if readings.back().is_some_and(|(before, _)| bytes < *before) {
            readings.clear();
        }
        while readings.front().is_some_and(|(_, at)| !fresh(at)) {
            readings.pop_front();
        }
        let first = readings.front().copied();
        readings.push_back((bytes, now));
        let (before, at) = first?;
        let ms = now.saturating_duration_since(at).as_millis();
        if ms == 0 {
            return None;
        }
        // bytes · 8 / ms = kbit/s.
        Some(((u128::from(bytes - before) * 8 + ms / 2) / ms).min(u128::from(u32::MAX)) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(t0: Instant, ms: u64) -> Instant {
        t0 + Duration::from_millis(ms)
    }

    #[test]
    fn the_rate_is_the_bytes_over_the_recent_readings() {
        let mut rates = OutputRates::default();
        let id = OutputId::new();
        let t0 = Instant::now();
        assert_eq!(rates.rate(id, 1_000, t0), None, "no rate from one reading");
        // 750 000 bytes in a second: 6000 kbit/s.
        assert_eq!(rates.rate(id, 751_000, at(t0, 1000)), Some(6000));
        // 1 000 000 bytes in 1.5 s since the first reading: 5333 kbit/s.
        assert_eq!(rates.rate(id, 1_001_000, at(t0, 1500)), Some(5333));
        // Past the window the first two readings count no more: 875 000
        // bytes in the 3.5 s since the third.
        assert_eq!(rates.rate(id, 1_876_000, at(t0, 5000)), Some(2000));
    }

    /// A recording that writes 1 000 000 bytes every two seconds, read
    /// each second: 4000 kbit/s on average, and every reading near it,
    /// not 0 and 8000 in turns as between two readings.
    #[test]
    fn a_recording_written_in_steps_reads_steady() {
        let mut rates = OutputRates::default();
        let id = OutputId::new();
        let t0 = Instant::now();
        let mut shown = Vec::new();
        for second in 0..10u64 {
            let bytes = 500_000 * (second / 2 * 2);
            shown.extend(rates.rate(id, bytes, at(t0, second * 1000)));
        }
        assert!(
            shown.iter().skip(3).all(|k| (2500..=5500).contains(k)),
            "{shown:?}"
        );
    }

    #[test]
    fn a_new_start_or_a_gap_gives_no_rate() {
        let mut rates = OutputRates::default();
        let id = OutputId::new();
        let t0 = Instant::now();
        rates.rate(id, 5_000_000, t0);
        assert_eq!(
            rates.rate(id, 10_000, at(t0, 1000)),
            None,
            "a counter that went back started anew"
        );
        assert_eq!(
            rates.rate(id, 20_000, at(t0, 5000)),
            None,
            "four seconds since the last reading: a gap"
        );
        assert_eq!(rates.rate(id, 145_000, at(t0, 6000)), Some(1000));
    }

    #[test]
    fn outputs_count_apart_and_old_ones_are_forgotten() {
        let mut rates = OutputRates::default();
        let (a, b) = (OutputId::new(), OutputId::new());
        let t0 = Instant::now();
        rates.rate(a, 0, t0);
        rates.rate(b, 0, t0);
        assert_eq!(rates.rate(a, 125_000, at(t0, 1000)), Some(1000));
        assert_eq!(rates.rate(b, 250_000, at(t0, 1000)), Some(2000));
        rates.rate(a, 250_000, at(t0, 10_000));
        assert_eq!(rates.readings.len(), 1, "b's readings are gone");
        assert_eq!(rates.readings[&a].len(), 1, "a's old readings are gone");
    }
}
