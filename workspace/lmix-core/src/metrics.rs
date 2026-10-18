//! `GET /metrics` in the Prometheus text format (OPS.3): health of
//! inputs and outputs, tally, audio levels, latency, memory; from the
//! engine: CPU, memory and GPUs of its machine, its own memory and QoS
//! per flow (late buffers).

use std::fmt::Write;
use std::sync::OnceLock;
use std::time::Instant;

use lmix_types::{InputStatus, Latency, Levels, OutputStats, OutputStatus, ShowState};

use crate::engine::flows;
use crate::engine::meters::EngineHealth;

/// Everything one scrape reports.
pub struct Snapshot {
    pub state: ShowState,
    pub tally: Vec<u8>,
    pub levels: Levels,
    pub outputs: Vec<OutputStats>,
    pub latency: Latency,
    pub uptime_secs: u64,
    pub engine: EngineHealth,
    pub cues: crate::cues::Lateness,
}

static STARTED: OnceLock<Instant> = OnceLock::new();

/// When lmix-core started; call once early so uptime counts from there.
pub fn started() -> Instant {
    *STARTED.get_or_init(Instant::now)
}

/// Bytes of a memory page in `/proc/self/statm`.
const PAGE_BYTES: u64 = 4096;

/// Resident memory of this process; `None` off Linux.
pub fn resident_bytes() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(pages * PAGE_BYTES)
}

/// One metric family: HELP, TYPE, then its samples.
struct Family<'a> {
    out: &'a mut String,
}

impl<'a> Family<'a> {
    fn new(out: &'a mut String, name: &str, kind: &str, help: &str) -> Self {
        let _ = writeln!(out, "# HELP {name} {help}\n# TYPE {name} {kind}");
        Family { out }
    }

    fn sample(&mut self, name: &str, labels: &[(&str, &str)], value: impl std::fmt::Display) {
        let _ = write!(self.out, "{name}");
        if !labels.is_empty() {
            let pairs: Vec<String> = labels
                .iter()
                .map(|(k, v)| format!("{k}=\"{}\"", escape(v)))
                .collect();
            let _ = write!(self.out, "{{{}}}", pairs.join(","));
        }
        let _ = writeln!(self.out, " {value}");
    }
}

fn escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

/// The scrape text.
pub fn render(snap: &Snapshot, resident: Option<u64>) -> String {
    let mut out = String::new();
    let state = &snap.state;

    Family::new(
        &mut out,
        "lmix_uptime_seconds",
        "gauge",
        "Seconds since lmix-core started.",
    )
    .sample("lmix_uptime_seconds", &[], snap.uptime_secs);
    if let Some(bytes) = resident {
        Family::new(
            &mut out,
            "lmix_resident_memory_bytes",
            "gauge",
            "Resident memory of lmix-core.",
        )
        .sample("lmix_resident_memory_bytes", &[], bytes);
    }

    // Cue timing (show mode RUN.8, SNF.4): due time to execution.
    Family::new(
        &mut out,
        "lmix_cue_steps_total",
        "counter",
        "Cue steps run (actions, ends, follows).",
    )
    .sample("lmix_cue_steps_total", &[], snap.cues.steps);
    Family::new(
        &mut out,
        "lmix_cue_late_max_seconds",
        "gauge",
        "Latest a cue step ran after its due time since start.",
    )
    .sample(
        "lmix_cue_late_max_seconds",
        &[],
        snap.cues.max.as_secs_f64(),
    );
    Family::new(
        &mut out,
        "lmix_cue_late_over_budget_total",
        "counter",
        "Cue steps more than 10 ms late (RUN.8 allows 10 ms).",
    )
    .sample(
        "lmix_cue_late_over_budget_total",
        &[],
        snap.cues.over_budget,
    );
    Family::new(
        &mut out,
        "lmix_cue_timer_realtime",
        "gauge",
        "1 while the cue timer runs with real-time priority (RUN.8).",
    )
    .sample(
        "lmix_cue_timer_realtime",
        &[],
        u64::from(crate::realtime::granted()),
    );

    let mut f = Family::new(
        &mut out,
        "lmix_input_online",
        "gauge",
        "1 while the input delivers, 0 while starting or offline.",
    );
    for input in &state.inputs {
        let number = input.number.to_string();
        let online = u8::from(input.status == InputStatus::Online);
        f.sample(
            "lmix_input_online",
            &[("input", &number), ("name", &input.name)],
            online,
        );
    }
    let mut f = Family::new(
        &mut out,
        "lmix_input_tally",
        "gauge",
        "0 off, 1 program, 2 preview.",
    );
    for (input, tally) in state.inputs.iter().zip(&snap.tally) {
        let number = input.number.to_string();
        f.sample(
            "lmix_input_tally",
            &[("input", &number), ("name", &input.name)],
            tally,
        );
    }
    let mut f = Family::new(
        &mut out,
        "lmix_input_peak_dbfs",
        "gauge",
        "Audio peak of the input, dBFS.",
    );
    for level in &snap.levels.inputs {
        let Some(input) = state.inputs.iter().find(|i| i.id == level.input) else {
            continue;
        };
        let number = input.number.to_string();
        f.sample(
            "lmix_input_peak_dbfs",
            &[("input", &number), ("name", &input.name)],
            level.peak_db,
        );
    }
    if let Some(peak) = snap.levels.program_peak_db {
        Family::new(
            &mut out,
            "lmix_program_peak_dbfs",
            "gauge",
            "Audio peak of program, dBFS.",
        )
        .sample("lmix_program_peak_dbfs", &[], peak);
    }

    let mut f = Family::new(
        &mut out,
        "lmix_output_running",
        "gauge",
        "1 running, 0 stopped, -1 failed.",
    );
    for output in &state.outputs {
        let number = output.number.to_string();
        let running = match output.status {
            OutputStatus::Running { .. } => 1,
            OutputStatus::Stopped => 0,
            OutputStatus::Failed { .. } => -1,
        };
        f.sample(
            "lmix_output_running",
            &[("output", &number), ("name", &output.name)],
            running,
        );
    }
    let outputs_with_stats: Vec<_> = snap
        .outputs
        .iter()
        .filter_map(|s| {
            let output = state.outputs.iter().find(|o| o.id == s.output)?;
            Some((output.number.to_string(), output.name.as_str(), s))
        })
        .collect();
    let mut f = Family::new(
        &mut out,
        "lmix_output_kbps",
        "gauge",
        "Send rate of a network output.",
    );
    for (number, name, s) in &outputs_with_stats {
        f.sample(
            "lmix_output_kbps",
            &[("output", number), ("name", name)],
            s.kbps,
        );
    }
    let mut f = Family::new(
        &mut out,
        "lmix_output_packets_lost_total",
        "counter",
        "Packets the receiver reported lost.",
    );
    for (number, name, s) in &outputs_with_stats {
        f.sample(
            "lmix_output_packets_lost_total",
            &[("output", number), ("name", name)],
            s.packets_lost,
        );
    }
    let mut f = Family::new(
        &mut out,
        "lmix_output_packets_dropped_total",
        "counter",
        "Packets dropped before sending (link too slow).",
    );
    for (number, name, s) in &outputs_with_stats {
        f.sample(
            "lmix_output_packets_dropped_total",
            &[("output", number), ("name", name)],
            s.packets_dropped,
        );
    }

    let latency = &snap.latency;
    if let (Some(median), Some(p95)) = (latency.median_ms, latency.p95_ms) {
        let mut f = Family::new(
            &mut out,
            "lmix_latency_ms",
            "gauge",
            "Capture-to-program latency of the last measurement.",
        );
        f.sample("lmix_latency_ms", &[("quantile", "0.5")], median);
        f.sample("lmix_latency_ms", &[("quantile", "0.95")], p95);
    }
    render_engine(&mut out, state, &snap.engine);
    out
}

/// What the engine reports about its machine and flows.
fn render_engine(out: &mut String, state: &ShowState, engine: &EngineHealth) {
    if let Some(system) = &engine.system {
        Family::new(
            out,
            "lmix_engine_cpu_percent",
            "gauge",
            "CPU use of the engine machine.",
        )
        .sample("lmix_engine_cpu_percent", &[], system.cpu_usage);
        Family::new(
            out,
            "lmix_engine_memory_used_bytes",
            "gauge",
            "Used memory of the engine machine.",
        )
        .sample("lmix_engine_memory_used_bytes", &[], system.used_memory);
        Family::new(
            out,
            "lmix_engine_memory_total_bytes",
            "gauge",
            "Memory of the engine machine.",
        )
        .sample("lmix_engine_memory_total_bytes", &[], system.total_memory);
        if let Some(bytes) = system.process_memory {
            Family::new(
                out,
                "lmix_engine_resident_memory_bytes",
                "gauge",
                "Resident memory of the engine process.",
            )
            .sample("lmix_engine_resident_memory_bytes", &[], bytes);
        }
        let gpus: Vec<_> = system
            .gpu_stats
            .iter()
            .map(|g| (g.index.to_string(), g))
            .collect();
        let mut f = Family::new(out, "lmix_gpu_utilization_percent", "gauge", "GPU use.");
        for (index, g) in &gpus {
            f.sample(
                "lmix_gpu_utilization_percent",
                &[("gpu", index), ("name", &g.name)],
                g.utilization,
            );
        }
        let mut f = Family::new(
            out,
            "lmix_gpu_memory_used_bytes",
            "gauge",
            "GPU memory in use.",
        );
        for (index, g) in &gpus {
            f.sample(
                "lmix_gpu_memory_used_bytes",
                &[("gpu", index), ("name", &g.name)],
                g.used_memory,
            );
        }
        let mut f = Family::new(
            out,
            "lmix_gpu_encoder_percent",
            "gauge",
            "Use of the GPU's video encoder.",
        );
        for (index, g) in &gpus {
            if let Some(v) = g.encoder_utilization {
                f.sample(
                    "lmix_gpu_encoder_percent",
                    &[("gpu", index), ("name", &g.name)],
                    v,
                );
            }
        }
        let mut f = Family::new(
            out,
            "lmix_gpu_decoder_percent",
            "gauge",
            "Use of the GPU's video decoder.",
        );
        for (index, g) in &gpus {
            if let Some(v) = g.decoder_utilization {
                f.sample(
                    "lmix_gpu_decoder_percent",
                    &[("gpu", index), ("name", &g.name)],
                    v,
                );
            }
        }
    }

    // Flows by what they carry: the mixer, an input's slot, an output.
    let mut flows_qos: Vec<(Vec<(&str, String)>, _)> = engine
        .qos
        .iter()
        .filter_map(|(flow, qos)| Some((flow_labels(state, flow)?, *qos)))
        .collect();
    flows_qos.sort_by(|a, b| a.0.cmp(&b.0));
    let samples = |out: &mut String,
                   name: &str,
                   kind: &str,
                   help: &str,
                   value: &dyn Fn(&crate::engine::meters::FlowQos) -> String| {
        let mut f = Family::new(out, name, kind, help);
        for (labels, qos) in &flows_qos {
            let labels: Vec<(&str, &str)> = labels.iter().map(|(k, v)| (*k, v.as_str())).collect();
            f.sample(name, &labels, value(qos));
        }
    };
    samples(
        out,
        "lmix_flow_qos_events_total",
        "counter",
        "Late buffers an element dropped or hurried (QoS events).",
        &|q| q.events.to_string(),
    );
    samples(
        out,
        "lmix_flow_falling_behind_total",
        "counter",
        "QoS reports in which the flow was falling behind.",
        &|q| q.behind.to_string(),
    );
    samples(
        out,
        "lmix_flow_qos_proportion",
        "gauge",
        "Processing proportion of the latest QoS report (below 1: too slow).",
        &|q| q.proportion.to_string(),
    );
}

/// Labels of an engine flow; `None` for flows lmix does not know.
fn flow_labels(
    state: &ShowState,
    flow: &strom_types::FlowId,
) -> Option<Vec<(&'static str, String)>> {
    if *flow == flows::mixer_id() {
        return Some(vec![("flow", "mixer".into())]);
    }
    if let Some(input) = state
        .inputs
        .iter()
        .find(|i| flows::slot_id(i.slot) == *flow)
    {
        return Some(vec![
            ("flow", "input".into()),
            ("input", input.number.to_string()),
            ("name", input.name.clone()),
        ]);
    }
    let output = state
        .outputs
        .iter()
        .find(|o| flows::output_flow_id(o.id) == *flow)?;
    Some(vec![
        ("flow", "output".into()),
        ("output", output.number.to_string()),
        ("name", output.name.clone()),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::{Input, InputId, InputLevel};

    #[test]
    fn renders_inputs_tally_levels_and_escapes_names() {
        let mut state: ShowState = serde_json::from_value(serde_json::json!({
            "api_version": 1, "inputs": [], "transitions": [], "fade_to_black": false,
            "slots": 2, "outputs": [], "overlays": [null, null, null, null, null, null, null, null], "t_bar": 0.0
        }))
        .unwrap();
        let input: Input = serde_json::from_value(serde_json::json!({
            "id": InputId::new(), "number": 1, "name": "Cam \"A\"",
            "source": {"kind": "pattern", "pattern": "bars"}, "slot": 0, "status": "online"
        }))
        .unwrap();
        let levels = Levels {
            inputs: vec![InputLevel {
                input: input.id,
                peak_db: -12.5,
                channels_db: vec![-12.5, -20.0],
            }],
            program_peak_db: Some(-6.0),
            monitor_peak_db: None,
            aux_peak_db: Default::default(),
        };
        let slot = input.slot;
        state.inputs.push(input);
        let mut engine = EngineHealth {
            system: Some(
                serde_json::from_value(serde_json::json!({
                    "cpu_usage": 12.5, "num_cores": 8, "total_memory": 1000, "used_memory": 400,
                    "process_memory": 300,
                    "gpu_stats": [{"index": 0, "name": "RTX", "utilization": 40.0,
                        "memory_utilization": 10.0, "total_memory": 100, "used_memory": 10,
                        "temperature": null, "power_usage": null, "encoder_utilization": 7.0}],
                    "gl_renderer": null, "timestamp": 0
                }))
                .unwrap(),
            ),
            ..Default::default()
        };
        let qos = |events, behind| crate::engine::meters::FlowQos {
            events,
            behind,
            proportion: 0.9,
        };
        engine.qos.insert(flows::mixer_id(), qos(5, 1));
        engine.qos.insert(flows::slot_id(slot), qos(2, 0));
        // A flow lmix does not know stays out.
        engine.qos.insert(flows::slot_id(slot + 5), qos(9, 9));
        let text = render(
            &Snapshot {
                state,
                tally: vec![1],
                levels,
                outputs: Vec::new(),
                latency: Latency::of(&[]),
                uptime_secs: 7,
                engine,
                cues: crate::cues::Lateness {
                    steps: 12,
                    max: std::time::Duration::from_millis(3),
                    over_budget: 1,
                },
            },
            Some(4096),
        );

        for line in [
            "lmix_uptime_seconds 7",
            "lmix_cue_steps_total 12",
            "lmix_cue_late_max_seconds 0.003",
            "lmix_cue_late_over_budget_total 1",
            "lmix_resident_memory_bytes 4096",
            r#"lmix_input_online{input="1",name="Cam \"A\""} 1"#,
            r#"lmix_input_tally{input="1",name="Cam \"A\""} 1"#,
            r#"lmix_input_peak_dbfs{input="1",name="Cam \"A\""} -12.5"#,
            "lmix_program_peak_dbfs -6",
            "# TYPE lmix_output_running gauge",
            "lmix_engine_cpu_percent 12.5",
            "lmix_engine_resident_memory_bytes 300",
            r#"lmix_gpu_encoder_percent{gpu="0",name="RTX"} 7"#,
            r#"lmix_flow_qos_events_total{flow="mixer"} 5"#,
            r#"lmix_flow_qos_events_total{flow="input",input="1",name="Cam \"A\""} 2"#,
            r#"lmix_flow_falling_behind_total{flow="mixer"} 1"#,
        ] {
            assert!(text.lines().any(|l| l == line), "missing {line} in\n{text}");
        }
        // 0 or 1: the real-time test may hold the priority meanwhile.
        assert!(text
            .lines()
            .any(|l| l.starts_with("lmix_cue_timer_realtime ")));
        assert!(
            !text.contains("lmix_latency_ms{"),
            "no latency before a measurement"
        );
    }
}
