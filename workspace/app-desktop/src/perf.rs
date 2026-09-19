//! Performance monitor: collects latency/jitter samples from every part of
//! the Console (UI frames, REST round trips, WS event pacing, take latency),
//! shows them as an overlay and optionally logs every sample to a CSV.
//!
//! ```text
//!   UI frame loop ─┐
//!   thumbnail poll ├─► Perf::record(metric, ms) ─► ring buffer ─► overlay / report
//!   WS events      │                             └► CSV (t_ms,metric,value_ms)
//!   take / state  ─┘
//! ```
//!
//! Jitter is the RFC 3550 smoothed mean deviation of consecutive samples.

use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;

const WINDOW: usize = 600;
const JITTER_GAIN: f64 = 16.0;
const P95: f64 = 0.95;
const OVERLAY_ALPHA: u8 = 200;
const OVERLAY_FONT: f32 = 11.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Metric {
    /// Interval between UI frames.
    UiFrame,
    /// Time spent inside one UI frame (layout + paint submit).
    UiCost,
    /// REST round trip of one thumbnail fetch.
    ThumbRtt,
    /// Interval between consecutive thumbnail arrivals for one input.
    ThumbGap,
    /// Interval between WebSocket events (any kind).
    WsGap,
    /// REST round trip of a take/transition request.
    TakeRtt,
    /// Take click until the confirming WS state event arrives.
    TakeToEvent,
    /// REST round trip of a mixer state query.
    StateRtt,
    /// Pipeline latency reported by the backend (`/latency`), min value.
    PipelineLatency,
}

impl Metric {
    pub const ALL: [Self; 9] = [
        Self::UiFrame,
        Self::UiCost,
        Self::ThumbRtt,
        Self::ThumbGap,
        Self::WsGap,
        Self::TakeRtt,
        Self::TakeToEvent,
        Self::StateRtt,
        Self::PipelineLatency,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::UiFrame => "ui_frame",
            Self::UiCost => "ui_cost",
            Self::ThumbRtt => "thumb_rtt",
            Self::ThumbGap => "thumb_gap",
            Self::WsGap => "ws_gap",
            Self::TakeRtt => "take_rtt",
            Self::TakeToEvent => "take_to_event",
            Self::StateRtt => "state_rtt",
            Self::PipelineLatency => "pipeline_latency",
        }
    }

    fn index(self) -> usize {
        Self::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }
}

#[derive(Default, Clone, Copy, Debug)]
pub struct Stats {
    pub count: u64,
    pub last: f64,
    pub mean: f64,
    pub p95: f64,
    pub max: f64,
    pub jitter: f64,
}

#[derive(Default)]
struct Series {
    window: VecDeque<f64>,
    count: u64,
    sum: f64,
    max: f64,
    last: f64,
    jitter: f64,
}

impl Series {
    fn push(&mut self, ms: f64) {
        if self.count > 0 {
            self.jitter += ((ms - self.last).abs() - self.jitter) / JITTER_GAIN;
        }
        if self.window.len() == WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(ms);
        self.count += 1;
        self.sum += ms;
        self.max = self.max.max(ms);
        self.last = ms;
    }

    fn stats(&self) -> Stats {
        if self.count == 0 {
            return Stats::default();
        }
        let mut sorted: Vec<f64> = self.window.iter().copied().collect();
        sorted.sort_by(|a, b| a.total_cmp(b));
        let p95_idx = ((sorted.len() as f64 * P95) as usize).min(sorted.len() - 1);
        Stats {
            count: self.count,
            last: self.last,
            mean: self.sum / self.count as f64,
            p95: sorted[p95_idx],
            max: self.max,
            jitter: self.jitter,
        }
    }
}

struct Inner {
    series: Vec<Series>,
    take_started: Option<Instant>,
    last_ws: Option<Instant>,
    last_thumb: Vec<Option<Instant>>,
}

pub struct Perf {
    start: Instant,
    inner: Mutex<Inner>,
    log: Mutex<Option<BufWriter<File>>>,
}

impl Perf {
    pub fn new(log_path: Option<&Path>) -> Self {
        let log = log_path.and_then(|p| match File::create(p) {
            Ok(f) => {
                let mut w = BufWriter::new(f);
                let _ = writeln!(w, "t_ms,metric,value_ms");
                Some(w)
            }
            Err(e) => {
                tracing::warn!("cannot open perf log {}: {e}", p.display());
                None
            }
        });
        Self {
            start: Instant::now(),
            inner: Mutex::new(Inner {
                series: Metric::ALL.iter().map(|_| Series::default()).collect(),
                take_started: None,
                last_ws: None,
                last_thumb: Vec::new(),
            }),
            log: Mutex::new(log),
        }
    }

    pub fn record(&self, metric: Metric, ms: f64) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.series[metric.index()].push(ms);
        }
        self.log_sample(metric, ms);
    }

    fn log_sample(&self, metric: Metric, ms: f64) {
        let Ok(mut log) = self.log.lock() else {
            return;
        };
        let Some(w) = log.as_mut() else {
            return;
        };
        let t_ms = self.start.elapsed().as_secs_f64() * 1000.0;
        let _ = writeln!(w, "{t_ms:.3},{},{ms:.3}", metric.name());
    }

    pub fn stats(&self, metric: Metric) -> Stats {
        self.inner
            .lock()
            .map(|i| i.series[metric.index()].stats())
            .unwrap_or_default()
    }

    pub fn flush(&self) {
        if let Ok(mut log) = self.log.lock() {
            if let Some(w) = log.as_mut() {
                let _ = w.flush();
            }
        }
    }

    /// Call for every WebSocket event; records the gap to the previous one.
    pub fn ws_event(&self) {
        let now = Instant::now();
        let gap = self.inner.lock().ok().and_then(|mut i| {
            let gap = i.last_ws.map(|t| now.duration_since(t));
            i.last_ws = Some(now);
            gap
        });
        if let Some(gap) = gap {
            self.record(Metric::WsGap, gap.as_secs_f64() * 1000.0);
        }
    }

    /// Call whenever a thumbnail for `input` arrives.
    pub fn thumb_arrived(&self, input: usize) {
        let now = Instant::now();
        let gap = self.inner.lock().ok().and_then(|mut i| {
            if i.last_thumb.len() <= input {
                i.last_thumb.resize(input + 1, None);
            }
            let gap = i.last_thumb[input].map(|t| now.duration_since(t));
            i.last_thumb[input] = Some(now);
            gap
        });
        if let Some(gap) = gap {
            self.record(Metric::ThumbGap, gap.as_secs_f64() * 1000.0);
        }
    }

    /// Mark the moment a take was requested.
    pub fn take_started(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.take_started = Some(Instant::now());
        }
    }

    /// Call when a mixer state event arrives; closes an open take marker.
    pub fn take_confirmed(&self) {
        let started = self
            .inner
            .lock()
            .ok()
            .and_then(|mut i| i.take_started.take());
        if let Some(t) = started {
            self.record(Metric::TakeToEvent, t.elapsed().as_secs_f64() * 1000.0);
        }
    }

    /// Semi-transparent stats table in the top-right corner (F3).
    pub fn overlay(&self, ctx: &egui::Context) {
        egui::Area::new(egui::Id::new("perf_overlay"))
            .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-8.0, -8.0))
            .interactable(false)
            .show(ctx, |ui| {
                egui::Frame::popup(ui.style())
                    .fill(egui::Color32::from_black_alpha(OVERLAY_ALPHA))
                    .show(ui, |ui| {
                        ui.label(
                            egui::RichText::new(self.report())
                                .monospace()
                                .size(OVERLAY_FONT),
                        );
                    });
            });
    }

    /// One text table with every metric that has samples.
    pub fn report(&self) -> String {
        let mut out = format!(
            "{:<17} {:>7} {:>9} {:>9} {:>9} {:>9} {:>9}\n",
            "metric", "count", "last", "mean", "p95", "max", "jitter"
        );
        for metric in Metric::ALL {
            let s = self.stats(metric);
            if s.count == 0 {
                continue;
            }
            out.push_str(&format!(
                "{:<17} {:>7} {:>9.1} {:>9.1} {:>9.1} {:>9.1} {:>9.1}\n",
                metric.name(),
                s.count,
                s.last,
                s.mean,
                s.p95,
                s.max,
                s.jitter
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steady_samples_have_no_jitter() {
        let perf = Perf::new(None);
        for _ in 0..50 {
            perf.record(Metric::UiFrame, 16.0);
        }
        let s = perf.stats(Metric::UiFrame);

        assert_eq!(s.count, 50);
        assert_eq!(s.jitter, 0.0);
        assert_eq!(s.max, 16.0);
    }

    #[test]
    fn spike_shows_in_max_and_jitter() {
        let perf = Perf::new(None);
        for _ in 0..20 {
            perf.record(Metric::ThumbRtt, 10.0);
        }
        perf.record(Metric::ThumbRtt, 110.0);
        let s = perf.stats(Metric::ThumbRtt);

        assert_eq!(s.max, 110.0);
        assert!(s.jitter > 0.0);
        assert!(s.p95 <= 110.0);
    }

    #[test]
    fn take_latency_needs_a_started_take() {
        let perf = Perf::new(None);
        perf.take_confirmed();
        assert_eq!(perf.stats(Metric::TakeToEvent).count, 0);

        perf.take_started();
        perf.take_confirmed();
        assert_eq!(perf.stats(Metric::TakeToEvent).count, 1);
    }
}
