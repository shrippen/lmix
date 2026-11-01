//! How the engine keeps up with an encoded output (STR.7), from strom's
//! block statistics of the output flow (`/flows/{id}/rtp-stats`):
//!
//! ```text
//!   pgm (Inter Input) ──► enc (Video Encoder) ──► stream / recorder
//!    └ buffer, dropped      └ encode time
//! ```
//!
//! The buffer is counted in frames, not by strom's queued time: that one
//! comes from the frames' timestamps, which a queue in a flow on the
//! mixer's running time does not always turn into a delay (todo.md, VM
//! test). Frames times the program rate is what waits.

use lmix_types::{Framerate, OutputId, OutputLoad};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use strom_types::stats::{BlockStats, InterInputStats, VideoEncoderStats};

/// A reading older than this counts no more: the output stopped in
/// between, or the poll was held up.
const STALE: Duration = Duration::from_secs(4);

const NS_PER_MS: u64 = 1_000_000;

/// The last encoder reading of each output.
#[derive(Default)]
pub struct EncoderReadings {
    last: HashMap<OutputId, (VideoEncoderStats, Instant)>,
}

impl EncoderReadings {
    /// Encode time per frame (ms) and busy share (%) of `id` since its
    /// last reading; `None` for the first reading, after a stale one,
    /// when the counters went back (a new start) or no frame came.
    pub fn load(
        &mut self,
        id: OutputId,
        now: VideoEncoderStats,
        at: Instant,
    ) -> (Option<f32>, Option<u32>) {
        self.last
            .retain(|_, (_, when)| at.saturating_duration_since(*when) < STALE);
        let Some((before, when)) = self.last.insert(id, (now, at)) else {
            return (None, None);
        };
        let frames = now.frames.checked_sub(before.frames);
        let busy_ns = now.encode_time_ns.checked_sub(before.encode_time_ns);
        let wall_ns = at.saturating_duration_since(when).as_nanos() as u64;
        let (Some(frames @ 1..), Some(busy_ns)) = (frames, busy_ns) else {
            return (None, None);
        };
        if wall_ns == 0 {
            return (None, None);
        }
        let per_frame_ms = busy_ns as f64 / frames as f64 / NS_PER_MS as f64;
        let percent = (busy_ns as f64 * 100.0 / wall_ns as f64).round() as u32;
        (Some(per_frame_ms as f32), Some(percent))
    }
}

/// The load of an output flow from its `blocks`' statistics: the buffer of
/// block `picture`, the encode time of block `encoder` (through
/// `readings`). `None` without the buffer's statistics (an older strom).
pub fn output_load(
    blocks: &[BlockStats],
    picture: &str,
    encoder: &str,
    id: OutputId,
    framerate: Framerate,
    readings: &mut EncoderReadings,
) -> Option<OutputLoad> {
    let block = |name: &str| {
        blocks
            .iter()
            .find(|b| b.block_instance_id == name)
            .map(|b| b.stats.as_slice())
    };
    let queue = InterInputStats::from_statistics(block(picture)?)?;
    let (encode_ms, encoder_percent) = block(encoder)
        .and_then(VideoEncoderStats::from_statistics)
        .map(|e| readings.load(id, e, Instant::now()))
        .unwrap_or_default();
    let max_ms = queue.max_time_ns / NS_PER_MS;
    let queued_ms = queue.queued_buffers * framerate.frame_ns() / NS_PER_MS;
    Some(OutputLoad {
        buffer_ms: queued_ms.min(max_ms) as u32,
        buffer_max_ms: max_ms as u32,
        frames_dropped: queue.dropped_buffers,
        encode_ms,
        encoder_percent,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn encoder(frames: u64, ms: u64) -> VideoEncoderStats {
        VideoEncoderStats {
            frames,
            encode_time_ns: ms * NS_PER_MS,
        }
    }

    #[test]
    fn the_encoder_load_is_the_time_since_the_last_reading() {
        let mut readings = EncoderReadings::default();
        let id = OutputId::new();
        let t0 = Instant::now();
        assert_eq!(readings.load(id, encoder(0, 0), t0), (None, None));
        // 25 frames of 8 ms in a second: 200 ms busy, 20 %.
        let t1 = t0 + Duration::from_secs(1);
        assert_eq!(
            readings.load(id, encoder(25, 200), t1),
            (Some(8.0), Some(20))
        );
        // A new start counts from zero: no load from that reading.
        let t2 = t1 + Duration::from_secs(1);
        assert_eq!(readings.load(id, encoder(3, 30), t2), (None, None));
        // No frame in a second: no time per frame to give.
        let t3 = t2 + Duration::from_secs(1);
        assert_eq!(readings.load(id, encoder(3, 30), t3), (None, None));
    }

    #[test]
    fn a_stale_reading_starts_anew() {
        let mut readings = EncoderReadings::default();
        let id = OutputId::new();
        let t0 = Instant::now();
        readings.load(id, encoder(0, 0), t0);
        let later = t0 + STALE + Duration::from_secs(1);
        assert_eq!(readings.load(id, encoder(250, 2000), later), (None, None));
    }

    fn block(id: &str, stats: Vec<strom_types::stats::Statistic>) -> BlockStats {
        BlockStats {
            block_instance_id: id.into(),
            block_definition_id: String::new(),
            block_name: String::new(),
            stats,
            collected_at: 0,
        }
    }

    /// Three frames at 25 fps wait (120 ms of a 500 ms buffer), seven
    /// were dropped; the encoder's first reading gives no load yet.
    #[test]
    fn the_buffer_is_counted_in_frames() {
        let queue = InterInputStats {
            queued_buffers: 3,
            // A queued time that is no delay (running time jumps).
            queued_time_ns: 14_000 * NS_PER_MS,
            max_time_ns: 500 * NS_PER_MS,
            dropped_buffers: 7,
        };
        let stats = [
            block("pgm", queue.to_statistics()),
            block("enc", encoder(10, 80).to_statistics()),
        ];
        let mut readings = EncoderReadings::default();
        let load = output_load(
            &stats,
            "pgm",
            "enc",
            OutputId::new(),
            Framerate::Fps25,
            &mut readings,
        );
        assert_eq!(
            load,
            Some(OutputLoad {
                buffer_ms: 120,
                buffer_max_ms: 500,
                frames_dropped: 7,
                encode_ms: None,
                encoder_percent: None,
            })
        );
    }

    #[test]
    fn a_full_buffer_reads_its_size_and_none_without_its_statistics() {
        let queue = InterInputStats {
            queued_buffers: 40,
            queued_time_ns: 0,
            max_time_ns: 500 * NS_PER_MS,
            dropped_buffers: 0,
        };
        let mut readings = EncoderReadings::default();
        let stats = [block("pgm", queue.to_statistics())];
        let load = output_load(
            &stats,
            "pgm",
            "enc",
            OutputId::new(),
            Framerate::Fps25,
            &mut readings,
        );
        assert_eq!(load.map(|l| l.buffer_ms), Some(500));
        let none = [block("enc", encoder(1, 1).to_statistics())];
        let load = output_load(
            &none,
            "pgm",
            "enc",
            OutputId::new(),
            Framerate::Fps25,
            &mut readings,
        );
        assert_eq!(load, None);
    }
}
