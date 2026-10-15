//! `GET /metrics` in the Prometheus text format (OPS.3): health of
//! inputs and outputs, tally, audio levels, latency, memory.

use std::fmt::Write;
use std::sync::OnceLock;
use std::time::Instant;

use lmix_types::{InputStatus, Latency, Levels, OutputStats, OutputStatus, ShowState};

/// Everything one scrape reports.
pub struct Snapshot {
    pub state: ShowState,
    pub tally: Vec<u8>,
    pub levels: Levels,
    pub outputs: Vec<OutputStats>,
    pub latency: Latency,
    pub uptime_secs: u64,
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
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::{Input, InputId, InputLevel};

    #[test]
    fn renders_inputs_tally_levels_and_escapes_names() {
        let mut state: ShowState = serde_json::from_value(serde_json::json!({
            "api_version": 1, "inputs": [], "transitions": [], "fade_to_black": false,
            "slots": 2, "outputs": [], "overlays": [null, null, null, null], "t_bar": 0.0
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
            }],
            program_peak_db: Some(-6.0),
            monitor_peak_db: None,
            aux_peak_db: Default::default(),
        };
        state.inputs.push(input);
        let text = render(
            &Snapshot {
                state,
                tally: vec![1],
                levels,
                outputs: Vec::new(),
                latency: Latency::of(&[]),
                uptime_secs: 7,
            },
            Some(4096),
        );

        for line in [
            "lmix_uptime_seconds 7",
            "lmix_resident_memory_bytes 4096",
            r#"lmix_input_online{input="1",name="Cam \"A\""} 1"#,
            r#"lmix_input_tally{input="1",name="Cam \"A\""} 1"#,
            r#"lmix_input_peak_dbfs{input="1",name="Cam \"A\""} -12.5"#,
            "lmix_program_peak_dbfs -6",
            "# TYPE lmix_output_running gauge",
        ] {
            assert!(text.lines().any(|l| l == line), "missing {line} in\n{text}");
        }
        assert!(
            !text.contains("lmix_latency_ms{"),
            "no latency before a measurement"
        );
    }
}
