//! Rate of an output from strom's byte counters (`/flows/{id}/output-bytes`):
//! the bytes between two readings over the time between them.

use lmix_types::OutputId;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// A reading older than this is no base for a rate: the output stopped
/// in between, or the poll was held up.
const STALE: Duration = Duration::from_secs(3);

/// The last reading of each output.
#[derive(Default)]
pub struct OutputRates {
    last: HashMap<OutputId, (u64, Instant)>,
}

impl OutputRates {
    /// kbit/s since the last reading of `id`, which `bytes` at `now`
    /// replaces; `None` for the first reading, after a stale one, and when
    /// the counter went back (a new start, a reconnect).
    pub fn rate(&mut self, id: OutputId, bytes: u64, now: Instant) -> Option<u32> {
        self.last
            .retain(|_, (_, at)| now.saturating_duration_since(*at) < STALE);
        let (before, at) = self.last.insert(id, (bytes, now))?;
        let ms = now.saturating_duration_since(at).as_millis();
        if bytes < before || ms == 0 {
            return None;
        }
        // bytes · 8 / ms = kbit/s.
        Some(((u128::from(bytes - before) * 8 + ms / 2) / ms).min(u128::from(u32::MAX)) as u32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rate_is_the_bytes_between_two_readings() {
        let mut rates = OutputRates::default();
        let id = OutputId::new();
        let t0 = Instant::now();
        assert_eq!(rates.rate(id, 1_000, t0), None, "no rate from one reading");
        // 750 000 bytes in a second: 6000 kbit/s.
        assert_eq!(
            rates.rate(id, 751_000, t0 + Duration::from_secs(1)),
            Some(6000)
        );
        // Half a second, 250 000 bytes: 4000 kbit/s.
        assert_eq!(
            rates.rate(id, 1_001_000, t0 + Duration::from_millis(1500)),
            Some(4000)
        );
    }

    #[test]
    fn a_new_start_or_a_stale_reading_gives_no_rate() {
        let mut rates = OutputRates::default();
        let id = OutputId::new();
        let t0 = Instant::now();
        rates.rate(id, 5_000_000, t0);
        assert_eq!(
            rates.rate(id, 10_000, t0 + Duration::from_secs(1)),
            None,
            "a counter that went back started anew"
        );
        assert_eq!(
            rates.rate(id, 20_000, t0 + Duration::from_secs(5)),
            None,
            "four seconds since the last reading: stale"
        );
        assert_eq!(
            rates.rate(id, 145_000, t0 + Duration::from_secs(6)),
            Some(1000)
        );
    }

    #[test]
    fn outputs_count_apart_and_stale_ones_are_forgotten() {
        let mut rates = OutputRates::default();
        let (a, b) = (OutputId::new(), OutputId::new());
        let t0 = Instant::now();
        rates.rate(a, 0, t0);
        rates.rate(b, 0, t0);
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(rates.rate(a, 125_000, t1), Some(1000));
        assert_eq!(rates.rate(b, 250_000, t1), Some(2000));
        rates.rate(a, 250_000, t0 + Duration::from_secs(10));
        assert_eq!(rates.last.len(), 1, "b's old reading is gone");
    }
}
