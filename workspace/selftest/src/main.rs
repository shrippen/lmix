//! lmix-selftest: headless load and switching test straight against strom
//! (M1b acceptance, `scripts/acceptance-m1b.sh`). No GUI; split from the
//! egui Console so that one can go.

mod afv;
mod api;
mod flows;
mod gpu_profile;
mod perf;
mod selftest;
mod sources;
mod ws;

use api::ApiClient;
use clap::Parser;
use gpu_profile::{BackendChoice, MixerBackend};

/// Thumbnail poll interval per input, as the Console polls.
const DEFAULT_THUMB_MS: u64 = 250;
/// How long to wait for the engine's GPU report.
const GPU_INFO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Build a multi-input flow in strom, poll and switch it like the Console,
/// and check latency, jitter and picture budgets (exit code 1 on failure).
#[derive(Parser, Debug)]
struct Args {
    /// Base URL of the strom backend.
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,

    /// Number of generated test-pattern inputs.
    #[arg(long, default_value_t = 8)]
    patterns: usize,

    /// Duration of the load phase in seconds.
    #[arg(long, default_value_t = 20)]
    secs: u64,

    /// Add a file input (repeatable).
    #[arg(long)]
    file: Vec<String>,

    /// Add a URL input (repeatable).
    #[arg(long)]
    url: Vec<String>,

    /// Add an SRT (MPEG-TS) input by URI (repeatable).
    #[arg(long)]
    srt: Vec<String>,

    /// Add every discovered video capture device.
    #[arg(long)]
    devices: bool,

    /// Encode PGM and multiview on hardware and check fps/latency.
    #[arg(long)]
    encode: bool,

    /// Stop a file input's player in the middle of a fade (needs a --file
    /// input at index >= 1).
    #[arg(long)]
    dropout: bool,

    /// Thumbnail poll interval per input, in milliseconds.
    #[arg(long, default_value_t = DEFAULT_THUMB_MS)]
    thumb_ms: u64,

    /// Write every latency/jitter sample to this CSV file.
    #[arg(long)]
    perf_log: Option<std::path::PathBuf>,

    /// Vision mixer backend (auto = fastest for the engine's GPU).
    #[arg(long, value_enum, default_value_t = BackendChoice::Auto)]
    mixer_backend: BackendChoice,
}

/// Mixer backend for a new flow, from the engine's GPU (see `gpu_profile`).
async fn resolve_backend(api: &ApiClient, choice: BackendChoice) -> Result<MixerBackend, String> {
    if choice != BackendChoice::Auto {
        return MixerBackend::resolve(choice, None);
    }
    let stats = ws::first_system_stats(&api.ws_url(), GPU_INFO_TIMEOUT).await;
    let gl = stats.and_then(|s| s.gl_renderer);
    let backend = MixerBackend::resolve(choice, gl.as_ref())?;
    tracing::info!(
        "mixer backend {:?} for GPU {:?}",
        backend,
        gl.map(|g| g.renderer)
    );
    Ok(backend)
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();
    let opts = selftest::Options {
        server: args.server,
        patterns: args.patterns,
        files: args.file,
        urls: args.url,
        srts: args.srt,
        devices: args.devices,
        seconds: args.secs,
        thumb_ms: args.thumb_ms,
        perf_log: args.perf_log,
        backend: args.mixer_backend,
        dropout: match args.dropout {
            true => selftest::Dropout::DuringFade,
            false => selftest::Dropout::None,
        },
        outputs: match args.encode {
            true => selftest::OutputLoad::Encode,
            false => selftest::OutputLoad::None,
        },
    };
    let runtime = tokio::runtime::Runtime::new().expect("failed to create Tokio runtime");
    let passed = runtime.block_on(selftest::run(opts));
    std::process::exit(if passed { 0 } else { 1 });
}
