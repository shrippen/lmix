//! Headless self-test: builds a multi-source flow, runs the same polling and
//! switching load the Console produces, and checks latency/jitter budgets.
//!
//! ```text
//!   create flow ─► start ─► wait first frame on every input
//!        │
//!        ├─ per input: poll thumbnail (RTT, gap, "did the picture change")
//!        ├─ every SWITCH_EVERY: preview + take (RTT, take -> WS event)
//!        ├─ every second: backend CPU/RSS from /proc
//!        └─ WS listener: event pacing
//!   report ─► verdict (exit code) ─► stop + delete flow
//! ```
//!
//! The mixer's multiview output has no REST tap (it is WHEP only), so the
//! test exercises it indirectly: it is part of the same pipeline, and a
//! stalled or failed multiview branch shows up as pipeline error / stall
//! (thumbnail gaps, take latency, flow leaving Playing).

use crate::afv::{mute_property, Afv};
use crate::api::ApiClient;
use crate::gpu_profile::BackendChoice;
use crate::perf::{Metric, Perf};
use crate::sources::{
    add_encoders, build_flow, SourceSpec, TestPattern, AUDIO_MIXER_ID, ENCODED_OUTPUTS, PGM_TAP_ID,
};
use crate::state::{find_switchable_flows, find_thumbnail_block, AppMessage};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use strom_types::vision_mixer::Source;
use strom_types::StromEvent;

const READY_TIMEOUT: Duration = Duration::from_secs(45);
const READY_POLL: Duration = Duration::from_millis(300);
const SWITCH_EVERY: Duration = Duration::from_millis(1500);
const LATENCY_EVERY: Duration = Duration::from_secs(2);
const CPU_EVERY: Duration = Duration::from_secs(1);
const SETTLE: Duration = Duration::from_millis(500);
const FTB_MS: u64 = 300;
/// A thumbnail tap refreshes once a second (strom default).
const THUMB_REFRESH: Duration = Duration::from_millis(1200);
/// How long an idle thumbnail tap may take to deliver its first frame.
const TAP_WAKEUP: Duration = Duration::from_secs(3);
/// Black after JPEG: limited range and ringing stay far below this.
const MAX_BLACK_LUMA: f64 = 20.0;
/// Dropout scenario: fade length, when the source stops, time left at the end.
const DROPOUT_FADE_MS: u64 = 1000;
const DROPOUT_AFTER: Duration = Duration::from_millis(300);
const DROPOUT_PHASE: Duration = Duration::from_secs(8);
/// No 0.5 s probe window may fall below this: a stall, not jitter.
const MIN_WINDOW_FPS: f64 = 20.0;
const TRANSITIONS: [&str; 4] = ["cut", "fade", "dip_to_black", "wipe_left"];
const BACKEND_COMM: &str = "strom";

/// Budgets the run is judged against.
const MAX_THUMB_RTT_P95_MS: f64 = 250.0;
const MAX_TAKE_EVENT_P95_MS: f64 = 500.0;
const MAX_UI_STALL_MS: f64 = 1000.0;
const MAX_PIPELINE_LATENCY_MS: f64 = 1000.0;
const MAX_STARTUP_S: f64 = 15.0;
/// Output frame rate floor: 97 % of the mixer's 30 fps.
const MIN_OUTPUT_FPS: f64 = 29.0;
/// PGM buffer age after the encoder: 5 frames at 30 fps (M1b budget).
const MAX_PGM_AGE_MS: f64 = 167.0;
/// Output picture check: poll interval, minimum picture changes over the
/// run and minimum luma standard deviation of the busiest frame (black or
/// a flat colour is ~0; the mostly black "ball" pattern ~6, the ball clip
/// ~40).
const OUTPUT_PICTURE_EVERY: Duration = Duration::from_millis(500);
const MIN_OUTPUT_CHANGES: u64 = 3;
const MIN_OUTPUT_LUMA_STD: f64 = 3.0;
/// Probes outlive the load phase by this much, then the backend drops them.
const PROBE_GRACE_S: u64 = 10;

/// Moving patterns first, so inputs 0 and 1 must show changing pictures.
const MOTION_INPUTS: usize = 2;
const PATTERN_ORDER: [TestPattern; 16] = [
    TestPattern::Ball,
    TestPattern::Snow,
    TestPattern::Smpte100,
    TestPattern::ColorChart,
    TestPattern::Black,
    TestPattern::White,
    TestPattern::Ramp,
    TestPattern::Checkers,
    TestPattern::Smpte75,
    TestPattern::Red,
    TestPattern::Green,
    TestPattern::Blue,
    TestPattern::ZonePlate,
    TestPattern::Circular,
    TestPattern::Pinwheel,
    TestPattern::Blink,
];

pub struct Options {
    pub server: String,
    pub patterns: usize,
    pub files: Vec<String>,
    pub urls: Vec<String>,
    pub srts: Vec<String>,
    pub devices: bool,
    pub seconds: u64,
    pub thumb_ms: u64,
    pub perf_log: Option<std::path::PathBuf>,
    pub backend: BackendChoice,
    pub outputs: OutputLoad,
    pub dropout: Dropout,
}

/// Source failure scenario run at the end of the load phase.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Dropout {
    None,
    /// A file input's player stops while a fade to it is running.
    DuringFade,
}

/// What hangs off the mixer outputs during the test.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum OutputLoad {
    /// Nothing: outputs unlinked.
    None,
    /// PGM and multiview through hardware encoders, probed for fps and age.
    Encode,
}

/// GPU load samples from the engine's `SystemStats` events (first GPU).
#[derive(Default)]
struct GpuLoad {
    gpu: Vec<f32>,
    decoder: Vec<f32>,
    encoder: Vec<f32>,
    vram_max: u64,
}

impl GpuLoad {
    fn record(&mut self, stats: &strom_types::SystemStats) {
        let Some(g) = stats.gpu_stats.first() else {
            return;
        };
        self.gpu.push(g.utilization);
        self.decoder.extend(g.decoder_utilization);
        self.encoder.extend(g.encoder_utilization);
        self.vram_max = self.vram_max.max(g.used_memory);
    }

    fn report(&self) {
        if self.gpu.is_empty() {
            println!("gpu: no stats from engine");
            return;
        }
        let mean_max = |v: &[f32]| {
            if v.is_empty() {
                return "n/a".to_string();
            }
            let mean = v.iter().sum::<f32>() / v.len() as f32;
            let max = v.iter().copied().fold(0.0, f32::max);
            format!("mean {mean:.0} %  max {max:.0} %")
        };
        println!(
            "gpu: {}; decoder: {}; encoder: {}; vram max {:.1} GB",
            mean_max(&self.gpu),
            mean_max(&self.decoder),
            mean_max(&self.encoder),
            self.vram_max as f64 / 1e9
        );
    }
}

/// What the thumbnail tap on one mixer output showed during the run.
#[derive(Default)]
struct OutputPicture {
    polls: u64,
    changes: u64,
    last_hash: Option<u64>,
    max_luma_std: f64,
}

impl OutputPicture {
    fn record(&mut self, jpeg: &[u8]) {
        self.polls += 1;
        let hash = hash_bytes(jpeg);
        self.changes += u64::from(self.last_hash.is_some_and(|h| h != hash));
        self.last_hash = Some(hash);
        if let Some(std) = luma_std(jpeg) {
            self.max_luma_std = self.max_luma_std.max(std);
        }
    }

    fn ok(&self) -> bool {
        self.changes >= MIN_OUTPUT_CHANGES && self.max_luma_std >= MIN_OUTPUT_LUMA_STD
    }
}

/// Mean luma of a JPEG, `None` if it does not decode.
fn luma_mean(jpeg: &[u8]) -> Option<f64> {
    let img = image::load_from_memory(jpeg).ok()?.to_luma8();
    let n = img.pixels().len() as f64;
    Some(img.pixels().map(|p| p.0[0] as f64).sum::<f64>() / n)
}

/// Standard deviation of the luma of a JPEG, `None` if it does not decode.
fn luma_std(jpeg: &[u8]) -> Option<f64> {
    let img = image::load_from_memory(jpeg).ok()?.to_luma8();
    let n = img.pixels().len() as f64;
    let mean = img.pixels().map(|p| p.0[0] as f64).sum::<f64>() / n;
    let var = img.pixels().map(|p| (p.0[0] as f64 - mean).powi(2)).sum::<f64>() / n;
    Some(var.sqrt())
}

/// Poll each mixer output's thumbnail tap until `stop`.
fn watch_output_pictures(
    api: &ApiClient,
    flow_id: strom_types::FlowId,
    stop: &Arc<AtomicU64>,
) -> Vec<(&'static str, tokio::task::JoinHandle<OutputPicture>)> {
    ENCODED_OUTPUTS
        .iter()
        .map(|out| {
            let (api, stop) = (api.clone(), stop.clone());
            let task = tokio::spawn(async move {
                let mut picture = OutputPicture::default();
                while stop.load(Ordering::SeqCst) == 0 {
                    if let Ok(jpeg) = api.get_thumbnail(flow_id, out.tap).await {
                        picture.record(&jpeg);
                    }
                    tokio::time::sleep(OUTPUT_PICTURE_EVERY).await;
                }
                picture
            });
            (out.mixer_pad, task)
        })
        .collect()
}

/// Where a probe sits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ProbePoint {
    /// A source, at its thumbnail tap in front of the mixer.
    Input,
    /// A mixer output, before or after its encoder.
    Output,
}

/// fps and buffer age at one probed point, from `BufferAgeProbe` events.
struct OutputProbe {
    point: ProbePoint,
    label: String,
    ages_ms: Vec<f64>,
    first: Option<(u64, Instant)>,
    last: Option<(u64, Instant)>,
    /// Slowest frame rate between two consecutive probe events.
    min_window_fps: Option<f64>,
}

impl OutputProbe {
    fn new(point: ProbePoint, label: String) -> Self {
        Self { point, label, ages_ms: Vec::new(), first: None, last: None, min_window_fps: None }
    }

    fn record(&mut self, age_ms: u64, sample: u64) {
        let now = Instant::now();
        self.record_window(sample, now);
        self.ages_ms.push(age_ms as f64);
        self.first.get_or_insert((sample, now));
        self.last = Some((sample, now));
    }

    fn record_window(&mut self, sample: u64, now: Instant) {
        let Some((s0, t0)) = self.last else {
            return;
        };
        let secs = now.duration_since(t0).as_secs_f64();
        if secs > 0.0 {
            let fps = (sample - s0) as f64 / secs;
            self.min_window_fps = Some(self.min_window_fps.map_or(fps, |m| m.min(fps)));
        }
    }

    fn fps(&self) -> f64 {
        let (Some((s0, t0)), Some((s1, t1))) = (self.first, self.last) else {
            return 0.0;
        };
        let secs = t1.duration_since(t0).as_secs_f64();
        if secs <= 0.0 {
            return 0.0;
        }
        (s1 - s0) as f64 / secs
    }

    /// (p50, p95, max) of the sampled ages.
    fn ages(&self) -> (f64, f64, f64) {
        let mut v = self.ages_ms.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        let at = |q: f64| v.get(((v.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(f64::NAN);
        (at(0.5), at(0.95), v.last().copied().unwrap_or(f64::NAN))
    }
}

#[derive(Default)]
struct InputProbe {
    polls: u64,
    changes: u64,
    last_hash: Option<u64>,
    first_frame: Option<Duration>,
}

fn hash_bytes(bytes: &[u8]) -> u64 {
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

/// Sum of utime+stime ticks and RSS pages of the backend process.
fn backend_proc_sample() -> Option<(u64, u64)> {
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let path = entry.path();
        let comm = std::fs::read_to_string(path.join("comm")).unwrap_or_default();
        if comm.trim() != BACKEND_COMM {
            continue;
        }
        let stat = std::fs::read_to_string(path.join("stat")).ok()?;
        // Fields after the ")" that closes comm: state is index 0, utime 11, stime 12, rss 21.
        let rest = stat.rsplit_once(')')?.1;
        let f: Vec<&str> = rest.split_whitespace().collect();
        let ticks = f.get(11)?.parse::<u64>().ok()? + f.get(12)?.parse::<u64>().ok()?;
        let rss_pages = f.get(21)?.parse::<u64>().ok()?;
        return Some((ticks, rss_pages));
    }
    None
}

const TICKS_PER_SEC: f64 = 100.0;
const PAGE_KB: f64 = 4.0;

fn sources_for(opts: &Options, devices: &[strom_types::discovery::DeviceResponse]) -> Vec<SourceSpec> {
    let mut sources: Vec<SourceSpec> = (0..opts.patterns)
        .map(|i| SourceSpec::Pattern(PATTERN_ORDER[i % PATTERN_ORDER.len()]))
        .collect();
    sources.extend(opts.files.iter().cloned().map(SourceSpec::File));
    sources.extend(opts.urls.iter().cloned().map(SourceSpec::Url));
    sources.extend(opts.srts.iter().cloned().map(SourceSpec::Srt));
    if opts.devices {
        sources.extend(devices.iter().map(|d| SourceSpec::Device {
            id: d.id.clone(),
            name: d.name.clone(),
        }));
    }
    sources
}

/// Runs the test; returns true when every budget held.
pub async fn run(opts: Options) -> bool {
    let api = ApiClient::new(opts.server.clone());
    let perf = Arc::new(Perf::new(opts.perf_log.as_deref()));

    let devices = api.list_video_devices().await.unwrap_or_default();
    let sources = sources_for(&opts, &devices);
    let name = format!("selftest-{}", std::process::id());
    let backend = match crate::resolve_backend(&api, opts.backend).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("FAIL: {e}");
            return false;
        }
    };
    let mut flow = match build_flow(&name, &sources, backend) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot build flow: {e}");
            return false;
        }
    };

    if opts.outputs == OutputLoad::Encode {
        add_encoders(&mut flow);
    }

    println!("selftest: {} inputs, {} s, mixer backend {:?}", sources.len(), opts.seconds, backend);
    for (i, s) in sources.iter().enumerate() {
        println!("  input {i}: {}", s.label());
    }

    let created = match api.create_flow(&flow).await {
        Ok(f) => f,
        Err(e) => {
            eprintln!("create flow failed: {e}");
            return false;
        }
    };
    let flow_id = created.id;

    // First file input after input 0: its player is what the dropout stops.
    let dropout_input = (opts.dropout == Dropout::DuringFade)
        .then(|| {
            sources
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, s)| matches!(s, SourceSpec::File(_) | SourceSpec::Url(_)))
                .map(|(i, _)| i)
        })
        .flatten();
    if opts.dropout == Dropout::DuringFade && dropout_input.is_none() {
        eprintln!("FAIL: dropout needs a file input at index >= 1");
        return false;
    }

    let ok = run_flow(&opts, &api, &perf, &created, sources.len(), dropout_input).await;

    let _ = api.stop_flow(flow_id).await;
    let _ = api.delete_flow(flow_id).await;
    perf.flush();
    ok
}

async fn run_flow(
    opts: &Options,
    api: &ApiClient,
    perf: &Arc<Perf>,
    flow: &strom_types::Flow,
    inputs: usize,
    dropout_input: Option<usize>,
) -> bool {
    let flow_id = flow.id;
    let Some(target) = find_switchable_flows(std::slice::from_ref(flow)).pop() else {
        eprintln!("flow has no vision mixer");
        return false;
    };
    let vm = target.vision_mixer_block_id;

    let started = Instant::now();
    if let Err(e) = api.start_flow(flow_id).await {
        eprintln!("start failed: {e}");
        return false;
    }

    // WS listener: pacing + take confirmation.
    let (tx, rx) = mpsc::channel::<(u64, AppMessage)>();
    crate::ws::connect(api.ws_url(), tx, egui::Context::default(), 0, perf.clone());

    let probes: Arc<Mutex<Vec<InputProbe>>> =
        Arc::new(Mutex::new((0..inputs).map(|_| InputProbe::default()).collect()));
    let stop = Arc::new(AtomicU64::new(0));

    let mut pollers = Vec::new();
    let mut thumbs = Vec::new();
    for idx in 0..inputs {
        let Some(thumb) = find_thumbnail_block(flow, &vm, idx) else {
            eprintln!("input {idx} has no thumbnail tap");
            return false;
        };
        thumbs.push(thumb.clone());
        let (api, perf, probes, stop) = (api.clone(), perf.clone(), probes.clone(), stop.clone());
        let thumb_ms = opts.thumb_ms;
        pollers.push(tokio::spawn(async move {
            while stop.load(Ordering::SeqCst) == 0 {
                let t0 = Instant::now();
                let result = api.get_thumbnail(flow_id, &thumb).await;
                if let Ok(bytes) = result {
                    perf.record(Metric::ThumbRtt, t0.elapsed().as_secs_f64() * 1000.0);
                    perf.thumb_arrived(idx);
                    let hash = hash_bytes(&bytes);
                    if let Ok(mut p) = probes.lock() {
                        let probe = &mut p[idx];
                        probe.polls += 1;
                        probe.changes += u64::from(probe.last_hash.is_some_and(|h| h != hash));
                        probe.last_hash = Some(hash);
                        probe.first_frame.get_or_insert(started.elapsed());
                    }
                }
                tokio::time::sleep(Duration::from_millis(thumb_ms)).await;
            }
        }));
    }

    // Wait until every input delivered a frame.
    let ready_deadline = Instant::now() + READY_TIMEOUT;
    loop {
        let ready = probes
            .lock()
            .map(|p| p.iter().all(|x| x.first_frame.is_some()))
            .unwrap_or(false);
        if ready {
            break;
        }
        if Instant::now() > ready_deadline {
            eprintln!("FAIL: not every input produced a frame within {READY_TIMEOUT:?}");
            stop.store(1, Ordering::SeqCst);
            return false;
        }
        tokio::time::sleep(READY_POLL).await;
    }
    println!("all inputs live after {:.1} s", started.elapsed().as_secs_f64());

    let mut outputs = start_output_probes(api, flow_id, opts, &thumbs).await;
    let pictures = if opts.outputs == OutputLoad::Encode {
        watch_output_pictures(api, flow_id, &stop)
    } else {
        Vec::new()
    };
    let mut gpu_load = GpuLoad::default();

    // Switcher task: preview + take at a fixed cadence until `stop`.
    // Own stop flag: the dropout scenario halts switching before the end.
    let switch_stop = Arc::new(AtomicU64::new(0));
    let switcher = {
        let (api, perf, vm, stop) = (api.clone(), perf.clone(), vm.clone(), switch_stop.clone());
        tokio::spawn(async move {
            let (mut switches, mut failures, mut target) = (0u32, 0u32, 1usize);
            while stop.load(Ordering::SeqCst) == 0 {
                switches += 1;
                let transition = TRANSITIONS[switches as usize % TRANSITIONS.len()];
                if !switch_once(&api, &perf, flow_id, &vm, inputs, &mut target, transition).await {
                    failures += 1;
                }
                tokio::time::sleep(SWITCH_EVERY).await;
            }
            (switches, failures)
        })
    };

    // Main load loop.
    let mut cpu_samples: Vec<f64> = Vec::new();
    let mut rss_kb_max = 0.0_f64;
    let mut last_proc = backend_proc_sample().map(|s| (s, Instant::now()));
    let mut next_latency = Instant::now();
    let mut next_cpu = Instant::now() + CPU_EVERY;
    let end = Instant::now() + Duration::from_secs(opts.seconds);

    let mut dropout = None;
    // PGM/PVW as a WS-only client (the Console) sees them.
    let mut ws_state = None;
    // AFV on every channel, applied in PGM order by one task.
    let has_audio = flow.blocks.iter().any(|b| b.id == AUDIO_MIXER_ID);
    let (afv_tx, afv_task) = if has_audio {
        let (tx, task) = start_afv(api.clone(), flow_id, inputs);
        (Some(tx), Some(task))
    } else {
        (None, None)
    };
    let mut loop_prev = Instant::now();
    while Instant::now() < end {
        let now = Instant::now();
        if let Some(input) = dropout_input.filter(|_| dropout.is_none() && now + DROPOUT_PHASE >= end) {
            switch_stop.store(1, Ordering::SeqCst);
            let (api, vm) = (api.clone(), vm.clone());
            dropout = Some(tokio::spawn(async move { dropout_during_fade(&api, flow_id, &vm, input).await }));
        }
        // Scheduler stall: this loop should tick at ~10 ms.
        perf.record(Metric::UiFrame, now.duration_since(loop_prev).as_secs_f64() * 1000.0);
        loop_prev = now;

        while let Ok((_, msg)) = rx.try_recv() {
            match msg {
                AppMessage::Event(StromEvent::VisionMixerStateChanged {
                    program_input,
                    preview_input,
                    ..
                }) => {
                    perf.take_confirmed();
                    ws_state = Some((program_input, preview_input));
                    if let Some(tx) = &afv_tx {
                        let _ = tx.send(program_input);
                    }
                }
                AppMessage::Event(StromEvent::SystemStats(stats)) => gpu_load.record(&stats),
                AppMessage::Event(StromEvent::BufferAgeProbe { probe_id, age_ms, sample_number, .. }) => {
                    if let Some(p) = outputs.get_mut(&probe_id) {
                        p.record(age_ms, sample_number);
                    }
                }
                _ => {}
            }
        }

        if now >= next_latency {
            next_latency = now + LATENCY_EVERY;
            if let Ok(l) = api.get_latency(flow_id).await {
                perf.record(Metric::PipelineLatency, l.min_latency_ns as f64 / 1e6);
            }
        }
        if now >= next_cpu {
            next_cpu = now + CPU_EVERY;
            if let (Some(prev), Some(cur)) = (last_proc, backend_proc_sample()) {
                let secs = prev.1.elapsed().as_secs_f64();
                cpu_samples.push((cur.0 - prev.0 .0) as f64 / TICKS_PER_SEC / secs * 100.0);
                rss_kb_max = rss_kb_max.max(cur.1 as f64 * PAGE_KB);
            }
            last_proc = backend_proc_sample().map(|s| (s, Instant::now()));
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    stop.store(1, Ordering::SeqCst);
    switch_stop.store(1, Ordering::SeqCst);
    for p in pollers {
        let _ = p.await;
    }
    let (switches, switch_failures) = switcher.await.unwrap_or((0, 1));
    let mut pictures_ok = true;
    for (pad, task) in pictures {
        let picture = task.await.unwrap_or_default();
        let ok = picture.ok();
        pictures_ok &= ok;
        println!(
            "{} {pad} picture: {} changes in {} polls, max luma std {:.1} (>= {MIN_OUTPUT_CHANGES}, >= {MIN_OUTPUT_LUMA_STD:.0})",
            if ok { "PASS" } else { "FAIL" },
            picture.changes,
            picture.polls,
            picture.max_luma_std
        );
    }

    let dropout_ok = match dropout {
        Some(task) => {
            let ok = task.await.unwrap_or(false);
            println!("{} source stopped during a fade: take completed, cut back, source resumed", if ok { "PASS" } else { "FAIL" });
            ok
        }
        None => true,
    };

    let engine_state = api
        .get_vision_mixer_state(flow_id, &vm)
        .await
        .ok()
        .flatten()
        .map(|s| (s.program_input, s.preview_input));
    let mirror_ok = ws_state.is_some() && ws_state == engine_state;
    println!(
        "{} WS mirror {:?} matches engine {:?} (PGM, PVW)",
        if mirror_ok { "PASS" } else { "FAIL" },
        ws_state,
        engine_state
    );

    let afv_ok = match afv_task {
        Some(task) => {
            // A take right at the end may have its WS event still in flight.
            if let Some(tx) = afv_tx {
                let _ = tx.send(engine_state.and_then(|s| s.0));
            }
            let afv = task.await.unwrap_or_default();
            let ok = check_afv(api, flow_id, &afv, engine_state.and_then(|s| s.0)).await;
            println!("{} AFV: engine mutes follow PGM on every channel", if ok { "PASS" } else { "FAIL" });
            ok
        }
        None => true,
    };

    let ftb_ok = ftb_round_trip(api, flow_id, &vm).await;

    let still_playing = api
        .get_flow(flow_id)
        .await
        .map(|f| f.running)
        .unwrap_or(false);

    gpu_load.report();
    println!("{} FTB on and off round trip", if ftb_ok { "PASS" } else { "FAIL" });
    let ok = check_outputs(opts, &outputs, dropout_input)
        && pictures_ok
        && ftb_ok
        && dropout_ok
        && mirror_ok
        && afv_ok;
    verdict(perf, &probes, inputs, switches, switch_failures, still_playing, &cpu_samples, rss_kb_max) && ok
}

/// Probe each encoder's input and its sink; keyed by probe id.
async fn start_output_probes(
    api: &ApiClient,
    flow_id: strom_types::FlowId,
    opts: &Options,
    thumbs: &[String],
) -> std::collections::HashMap<String, OutputProbe> {
    let mut probes = std::collections::HashMap::new();
    if opts.outputs == OutputLoad::None {
        return probes;
    }

    let timeout = (opts.seconds + PROBE_GRACE_S) as u32;
    for out in &ENCODED_OUTPUTS {
        for (element, stage) in [(out.encoder, "enc_in"), (out.sink, "encoded")] {
            match api.activate_probe(flow_id, element, timeout).await {
                Ok(r) => {
                    let label = format!("{} {stage}", out.mixer_pad);
                    probes.insert(r.probe_id, OutputProbe::new(ProbePoint::Output, label));
                }
                Err(e) => eprintln!("probe on {element} failed: {e}"),
            }
        }
    }

    // Source frame rate: the mixer keeps its output rate by repeating
    // frames, so a starved decoder only shows up here.
    for (i, thumb) in thumbs.iter().enumerate() {
        match api.activate_probe(flow_id, thumb, timeout).await {
            Ok(r) => {
                probes.insert(r.probe_id, OutputProbe::new(ProbePoint::Input, format!("input {i}")));
            }
            Err(e) => eprintln!("probe on {thumb} failed: {e}"),
        }
    }
    probes
}

/// Print the output table; every probed point must hold the frame rate,
/// and PGM after encode must stay inside the latency budget.
/// `stopped` is the input the dropout scenario stops on purpose; its rate
/// is not judged.
fn check_outputs(
    opts: &Options,
    outputs: &std::collections::HashMap<String, OutputProbe>,
    stopped: Option<usize>,
) -> bool {
    if opts.outputs == OutputLoad::None {
        return true;
    }

    let mut rows: Vec<&OutputProbe> = outputs.values().filter(|p| p.point == ProbePoint::Output).collect();
    rows.sort_by(|a, b| a.label.cmp(&b.label));
    println!("\noutput                   fps  min-win   age p50   p95   max (ms)");
    let mut ok = rows.len() == 2 * ENCODED_OUTPUTS.len();
    for p in rows {
        let (p50, p95, max) = p.ages();
        let min_window = p.min_window_fps.unwrap_or(0.0);
        println!("{:<22} {:>5.1}  {:>7.1}  {:>7.0} {:>5.0} {:>5.0}", p.label, p.fps(), min_window, p50, p95, max);
        ok &= p.fps() >= MIN_OUTPUT_FPS && min_window >= MIN_WINDOW_FPS;
        if p.label == "pgm_out encoded" {
            ok &= p95 <= MAX_PGM_AGE_MS;
        }
    }
    println!(
        "{} outputs >= {MIN_OUTPUT_FPS:.0} fps (no window < {MIN_WINDOW_FPS:.0}), PGM encoded age p95 <= {MAX_PGM_AGE_MS:.0} ms",
        if ok { "PASS" } else { "FAIL" }
    );

    let stopped_label = stopped.map(|i| format!("input {i}"));
    let mut inputs: Vec<&OutputProbe> = outputs
        .values()
        .filter(|p| p.point == ProbePoint::Input && Some(&p.label) != stopped_label.as_ref())
        .collect();
    inputs.sort_by(|a, b| a.fps().total_cmp(&b.fps()));
    for p in inputs.iter().filter(|p| p.fps() < MIN_OUTPUT_FPS) {
        let samples = p.last.map_or(0, |l| l.0) - p.first.map_or(0, |f| f.0);
        println!("  slow {:<9} {:>5.1} fps ({samples} frames, {} events)", p.label, p.fps(), p.ages_ms.len());
    }
    let slowest = inputs.iter().min_by(|a, b| a.fps().total_cmp(&b.fps()));
    let inputs_ok = slowest.is_some_and(|p| p.fps() >= MIN_OUTPUT_FPS);
    if let Some(p) = slowest {
        let mean = inputs.iter().map(|p| p.fps()).sum::<f64>() / inputs.len() as f64;
        println!(
            "{} {} inputs: mean {mean:.1} fps, slowest {} {:.1} fps >= {MIN_OUTPUT_FPS:.0}",
            if inputs_ok { "PASS" } else { "FAIL" },
            inputs.len(),
            p.label,
            p.fps()
        );
    }
    ok && inputs_ok
}

/// Fade from the current PGM to `input`, stop `input`'s player mid-fade:
/// the fade must still complete, a cut back must work, and the player
/// must resume. Output continuity is checked by the probe windows.
async fn dropout_during_fade(api: &ApiClient, flow_id: strom_types::FlowId, vm: &str, input: usize) -> bool {
    // Let the switcher's last take finish.
    tokio::time::sleep(SWITCH_EVERY + SETTLE).await;
    let player = crate::sources::media_player_id(input);
    let pgm_is = |want: usize| async move {
        matches!(api.get_vision_mixer_state(flow_id, vm).await, Ok(Some(s)) if s.program_input == Some(want))
    };

    let Ok(Some(state)) = api.get_vision_mixer_state(flow_id, vm).await else {
        return false;
    };
    // `input` is >= 1, so input 0 is always a valid other source.
    let pgm = match state.program_input {
        Some(p) if p != input => p,
        _ => 0,
    };
    // A take always puts the current PVW on PGM: select before each take.
    if state.program_input != Some(pgm) && !take_to(api, flow_id, vm, state.program_input, pgm).await {
        return false;
    }
    if api.select_preview(flow_id, vm, Source::Input(input)).await.is_err() {
        return false;
    }

    if api.trigger_transition(flow_id, vm, pgm, input, "fade", DROPOUT_FADE_MS).await.is_err() {
        eprintln!("dropout: fade request failed");
        return false;
    }
    tokio::time::sleep(DROPOUT_AFTER).await;
    if let Err(e) = api.player_control(flow_id, &player, "stop").await {
        eprintln!("dropout: stopping {player} failed: {e}");
        return false;
    }
    tokio::time::sleep(Duration::from_millis(DROPOUT_FADE_MS) + SETTLE).await;
    if !pgm_is(input).await {
        eprintln!("dropout: fade to the stopped source did not complete");
        return false;
    }

    if !take_to(api, flow_id, vm, Some(input), pgm).await {
        eprintln!("dropout: cut back failed");
        return false;
    }
    tokio::time::sleep(SETTLE).await;
    if !pgm_is(pgm).await {
        eprintln!("dropout: cut back did not land");
        return false;
    }

    if let Err(e) = api.player_control(flow_id, &player, "play").await {
        eprintln!("dropout: restarting {player} failed: {e}");
        return false;
    }
    true
}

/// AFV on channels 1..=inputs; applies the mutes for each PGM received,
/// in order, and returns the AFV set once the sender is dropped.
fn start_afv(
    api: ApiClient,
    flow_id: strom_types::FlowId,
    inputs: usize,
) -> (tokio::sync::mpsc::UnboundedSender<Option<usize>>, tokio::task::JoinHandle<Afv>) {
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Option<usize>>();
    let task = tokio::spawn(async move {
        let mut afv = Afv::default();
        for ch in 1..=inputs {
            afv.set(crate::sources::audio_channel(ch - 1), true);
        }
        while let Some(pgm) = rx.recv().await {
            let properties = afv
                .mutes(pgm)
                .into_iter()
                .map(|(ch, muted)| (mute_property(ch), strom_types::PropertyValue::Bool(muted)))
                .collect();
            let ramp = Some(crate::afv::ANTI_CLICK_MS);
            if let Err(e) = api.patch_block_properties_ramped(flow_id, AUDIO_MIXER_ID, properties, ramp).await {
                eprintln!("AFV patch failed: {e}");
            }
        }
        afv
    });
    (tx, task)
}

/// The engine's mixer mutes must match AFV for the final PGM.
async fn check_afv(api: &ApiClient, flow_id: strom_types::FlowId, afv: &Afv, pgm: Option<usize>) -> bool {
    let Ok(properties) = api.get_block_properties(flow_id, AUDIO_MIXER_ID).await else {
        return false;
    };
    afv.mutes(pgm).into_iter().all(|(ch, muted)| {
        matches!(properties.get(&mute_property(ch)), Some(strom_types::PropertyValue::Bool(m)) if *m == muted)
    })
}

/// Cut `to` onto PGM: select it on PVW, then take.
async fn take_to(api: &ApiClient, flow_id: strom_types::FlowId, vm: &str, from: Option<usize>, to: usize) -> bool {
    if api.select_preview(flow_id, vm, Source::Input(to)).await.is_err() {
        return false;
    }
    let from = from.unwrap_or(to);
    api.trigger_transition(flow_id, vm, from, to, "cut", 0).await.is_ok()
}

/// Fade to black and back; the mixer state must follow each toggle.
/// While black, the PGM tap must show black too.
async fn ftb_round_trip(api: &ApiClient, flow_id: strom_types::FlowId, vm: &str) -> bool {
    for expected in [true, false] {
        let Ok(resp) = api.toggle_ftb(flow_id, vm, FTB_MS).await else {
            return false;
        };
        tokio::time::sleep(Duration::from_millis(FTB_MS) + SETTLE).await;
        let state = api.get_vision_mixer_state(flow_id, vm).await;
        let active = matches!(state, Ok(Some(s)) if s.ftb_active == expected);
        if resp.active != expected || !active {
            return false;
        }
        if expected && !pgm_is_black(api, flow_id).await {
            eprintln!("FTB: state is black but the PGM picture is not");
            return false;
        }
    }
    true
}

/// Mean PGM luma below `MAX_BLACK_LUMA`, after the tap caught a new frame.
/// The first request may only activate the tap (504): retry for a while.
async fn pgm_is_black(api: &ApiClient, flow_id: strom_types::FlowId) -> bool {
    tokio::time::sleep(THUMB_REFRESH).await;
    let deadline = Instant::now() + TAP_WAKEUP;
    loop {
        if let Ok(jpeg) = api.get_thumbnail(flow_id, PGM_TAP_ID).await {
            return luma_mean(&jpeg).is_some_and(|m| m < MAX_BLACK_LUMA);
        }
        if Instant::now() > deadline {
            return false;
        }
        tokio::time::sleep(READY_POLL).await;
    }
}

/// PVW <- next input, then take; returns whether the mixer ended up with
/// that input on PGM.
async fn switch_once(
    api: &ApiClient,
    perf: &Arc<Perf>,
    flow_id: strom_types::FlowId,
    vm: &str,
    inputs: usize,
    target: &mut usize,
    transition: &str,
) -> bool {
    let Ok(state) = api.get_vision_mixer_state(flow_id, vm).await else {
        return false;
    };
    let Some(pgm) = state.and_then(|s| s.program_input) else {
        return false;
    };
    let next = if *target == pgm { (pgm + 1) % inputs } else { *target };
    *target = (next + 1) % inputs;

    if api.select_preview(flow_id, vm, Source::Input(next)).await.is_err() {
        return false;
    }

    perf.take_started();
    let t0 = Instant::now();
    let duration_ms = if transition == "cut" { 0 } else { 300 };
    let taken = api
        .trigger_transition(flow_id, vm, pgm, next, transition, duration_ms)
        .await;
    perf.record(Metric::TakeRtt, t0.elapsed().as_secs_f64() * 1000.0);
    if taken.is_err() {
        return false;
    }

    // Let the transition finish, then the reported PGM must be `next`.
    tokio::time::sleep(Duration::from_millis(duration_ms) + SETTLE).await;
    let t1 = Instant::now();
    let after = api.get_vision_mixer_state(flow_id, vm).await;
    perf.record(Metric::StateRtt, t1.elapsed().as_secs_f64() * 1000.0);
    matches!(after, Ok(Some(s)) if s.program_input == Some(next))
}

#[allow(clippy::too_many_arguments)]
fn verdict(
    perf: &Perf,
    probes: &Mutex<Vec<InputProbe>>,
    inputs: usize,
    switches: u32,
    switch_failures: u32,
    still_playing: bool,
    cpu: &[f64],
    rss_kb_max: f64,
) -> bool {
    println!("\n{}", perf.report());

    let mut ok = true;
    let mut check = |pass: bool, what: String| {
        println!("{} {what}", if pass { "PASS" } else { "FAIL" });
        ok &= pass;
    };

    let probes = probes.lock().map(|p| p.iter().map(|x| (x.polls, x.changes, x.first_frame)).collect::<Vec<_>>()).unwrap_or_default();
    println!("\ninput   polls  changed  first_frame");
    for (i, (polls, changes, first)) in probes.iter().enumerate() {
        println!(
            "{:>5} {:>7} {:>8}  {:>8.1} s",
            i,
            polls,
            changes,
            first.map_or(f64::NAN, |d| d.as_secs_f64())
        );
    }
    println!();

    if !cpu.is_empty() {
        let mean = cpu.iter().sum::<f64>() / cpu.len() as f64;
        let max = cpu.iter().copied().fold(0.0, f64::max);
        println!("backend cpu: mean {mean:.0} %  max {max:.0} % (100 % = one core), rss max {:.0} MB", rss_kb_max / 1024.0);
    }

    check(still_playing, "flow still running at the end".into());
    check(
        switch_failures == 0,
        format!("{switches} switches, {switch_failures} failed"),
    );
    let motion_ok = probes.iter().take(MOTION_INPUTS.min(inputs)).all(|(_, c, _)| *c > 0);
    check(motion_ok, "motion inputs delivered changing pictures".into());

    let thumb = perf.stats(Metric::ThumbRtt);
    check(
        thumb.p95 <= MAX_THUMB_RTT_P95_MS,
        format!("thumb_rtt p95 {:.0} ms <= {MAX_THUMB_RTT_P95_MS:.0} ms", thumb.p95),
    );
    let take = perf.stats(Metric::TakeToEvent);
    check(
        take.count > 0 && take.p95 <= MAX_TAKE_EVENT_P95_MS,
        format!("take_to_event p95 {:.0} ms <= {MAX_TAKE_EVENT_P95_MS:.0} ms ({} samples)", take.p95, take.count),
    );
    let stall = perf.stats(Metric::UiFrame);
    check(
        stall.max <= MAX_UI_STALL_MS,
        format!("client loop stall max {:.0} ms <= {MAX_UI_STALL_MS:.0} ms", stall.max),
    );
    let latency = perf.stats(Metric::PipelineLatency);
    check(
        latency.max <= MAX_PIPELINE_LATENCY_MS,
        format!("pipeline latency max {:.0} ms <= {MAX_PIPELINE_LATENCY_MS:.0} ms", latency.max),
    );
    let startup = probes
        .iter()
        .filter_map(|(_, _, f)| *f)
        .max()
        .map_or(f64::NAN, |d| d.as_secs_f64());
    check(
        startup <= MAX_STARTUP_S,
        format!("slowest input live after {startup:.1} s <= {MAX_STARTUP_S:.0} s"),
    );
    ok
}
