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

use crate::api::ApiClient;
use crate::perf::{Metric, Perf};
use crate::sources::{build_flow, SourceSpec, TestPattern};
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
const TRANSITIONS: [&str; 4] = ["cut", "fade", "dip_to_black", "wipe_left"];
const BACKEND_COMM: &str = "strom";

/// Budgets the run is judged against.
const MAX_THUMB_RTT_P95_MS: f64 = 250.0;
const MAX_TAKE_EVENT_P95_MS: f64 = 500.0;
const MAX_UI_STALL_MS: f64 = 1000.0;
const MAX_PIPELINE_LATENCY_MS: f64 = 1000.0;
const MAX_STARTUP_S: f64 = 15.0;

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
    pub devices: bool,
    pub seconds: u64,
    pub thumb_ms: u64,
    pub perf_log: Option<std::path::PathBuf>,
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
    let flow = match build_flow(&name, &sources) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("cannot build flow: {e}");
            return false;
        }
    };

    println!("selftest: {} inputs, {} s", sources.len(), opts.seconds);
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

    let ok = run_flow(&opts, &api, &perf, &created, sources.len()).await;

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
    let ws_url = opts.server.replacen("http://", "ws://", 1) + "/api/ws";
    crate::ws::connect(ws_url, tx, egui::Context::default(), 0, perf.clone());

    let probes: Arc<Mutex<Vec<InputProbe>>> =
        Arc::new(Mutex::new((0..inputs).map(|_| InputProbe::default()).collect()));
    let stop = Arc::new(AtomicU64::new(0));

    let mut pollers = Vec::new();
    for idx in 0..inputs {
        let Some(thumb) = find_thumbnail_block(flow, &vm, idx) else {
            eprintln!("input {idx} has no thumbnail tap");
            return false;
        };
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

    // Switcher task: preview + take at a fixed cadence until `stop`.
    let switcher = {
        let (api, perf, vm, stop) = (api.clone(), perf.clone(), vm.clone(), stop.clone());
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

    let mut loop_prev = Instant::now();
    while Instant::now() < end {
        let now = Instant::now();
        // Scheduler stall: this loop should tick at ~10 ms.
        perf.record(Metric::UiFrame, now.duration_since(loop_prev).as_secs_f64() * 1000.0);
        loop_prev = now;

        while let Ok((_, msg)) = rx.try_recv() {
            if let AppMessage::Event(StromEvent::VisionMixerStateChanged { .. }) = msg {
                perf.take_confirmed();
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
    for p in pollers {
        let _ = p.await;
    }
    let (switches, switch_failures) = switcher.await.unwrap_or((0, 1));

    let still_playing = api
        .get_flow(flow_id)
        .await
        .map(|f| f.running)
        .unwrap_or(false);

    verdict(perf, &probes, inputs, switches, switch_failures, still_playing, &cpu_samples, rss_kb_max)
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
