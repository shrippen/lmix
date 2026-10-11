//! lmix-core: the control process between GUI/APIs and the strom engine.
//!
//! ```text
//!   GUI ─┐
//!   API ─┼─► lmix-core ──REST──► strom (headless engine)
//!   …  ─┘
//! ```
//!
//! Runs without any GUI (OPS.1); a GUI crash never touches it (NFA.6).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use lmix_types::Action;

mod api;
mod clips;
mod datasource;
mod engine;
mod metrics;
mod midi;
mod osc;
mod recent;
mod service;
mod show;
mod switcher;
mod tcp;
mod titles;
mod tsl;
mod web;

use engine::flows::{MixerBackend, MonitorOut};
use engine::null::NullEngine;
use engine::strom::StromEngine;
use engine::Engine;
use service::Service;

/// Mixer slots by default; strom's vision mixer takes up to 32.
const DEFAULT_SLOTS: usize = 16;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum EngineKind {
    /// strom over REST.
    Strom,
    /// No media engine; state and API only (tests, demos).
    None,
}

#[derive(Parser, Debug)]
#[command(about = "lmix control core")]
struct Args {
    /// Address of lmix's own API.
    #[arg(long, env = "LMIX_LISTEN", default_value = "127.0.0.1:8200")]
    listen: SocketAddr,

    /// strom engine base URL.
    #[arg(long, env = "LMIX_STROM", default_value = "http://127.0.0.1:8099")]
    strom: String,

    #[arg(long, value_enum, default_value_t = EngineKind::Strom)]
    engine: EngineKind,

    /// Vision mixer compositor; `cpu` only for tests without GPU.
    #[arg(long, value_enum, default_value_t = MixerBackend::Cuda)]
    mixer: MixerBackend,

    /// Engine address as remote senders reach it (WHIP links); default:
    /// the `--strom` address.
    #[arg(long, env = "LMIX_PUBLIC_URL")]
    public_url: Option<String>,

    /// Monitor bus (solo listening): `auto` plays it on the engine
    /// machine's default audio output.
    #[arg(long, env = "LMIX_MONITOR_OUT", value_enum, default_value_t = MonitorOut::None)]
    monitor_out: MonitorOut,

    /// Directory for the live picture sockets of Consoles on this machine;
    /// default `$XDG_RUNTIME_DIR/lmix/live`.
    #[arg(long, env = "LMIX_LIVE_DIR")]
    live_dir: Option<PathBuf>,

    /// Mixer slots (inputs fed at the same time).
    #[arg(long, default_value_t = DEFAULT_SLOTS)]
    slots: usize,

    /// Address of the text protocol over TCP (API.5); off without it.
    #[arg(long, env = "LMIX_TCP")]
    tcp: Option<SocketAddr>,

    /// MIDI device files to read (`/dev/snd/midiC1D0`); repeat for more.
    #[arg(long, env = "LMIX_MIDI", value_delimiter = ',')]
    midi: Vec<PathBuf>,

    /// UDP address for OSC (AUT.13); off without it.
    #[arg(long, env = "LMIX_OSC")]
    osc: Option<SocketAddr>,

    /// Show file to open at start; without it the last autosave is resumed.
    #[arg(long, env = "LMIX_SHOW")]
    show: Option<String>,

    /// Autosave file, written after every change.
    #[arg(long, env = "LMIX_AUTOSAVE", default_value_os_t = default_autosave())]
    autosave: PathBuf,

    /// Send tally to TSL UMD receivers (multiviewers, tally systems) at
    /// these UDP addresses (PER.1); repeat or separate with commas.
    #[arg(long, env = "LMIX_TSL", value_delimiter = ',')]
    tsl: Vec<SocketAddr>,

    /// TSL UMD version: 5 (5.0) or 3 (3.1).
    #[arg(long, env = "LMIX_TSL_VERSION", value_enum, default_value_t = tsl::Version::V50)]
    tsl_version: tsl::Version,

    /// Folder of own title templates, besides the built-in ones.
    #[arg(long, env = "LMIX_TEMPLATES", default_value_os_t = default_templates())]
    templates: PathBuf,
}

/// `$XDG_DATA_HOME/lmix/templates`, else `~/.local/share/…`.
fn default_templates() -> PathBuf {
    data_home().join("lmix").join("templates")
}

/// `$XDG_DATA_HOME/lmix/autosave.lmix`, else `~/.local/share/…`.
fn default_autosave() -> PathBuf {
    data_home().join("lmix").join("autosave.lmix")
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    metrics::started();
    let engine: Arc<dyn Engine> = match args.engine {
        EngineKind::Strom => {
            let engine = StromEngine::new(args.strom.clone(), args.mixer, args.monitor_out);
            Arc::new(match args.live_dir.clone() {
                Some(dir) => engine.with_live_dir(dir),
                None => engine,
            })
        }
        EngineKind::None => Arc::new(NullEngine::default()),
    };

    let service = Service::new(args.slots, engine.clone(), Some(args.autosave));
    service.await_engine();
    let public_url = args
        .public_url
        .clone()
        .unwrap_or_else(|| args.strom.clone());
    service.set_ingest_base(public_url).await;
    // Before the show: its titles name templates from the folder.
    service.use_templates(args.templates.clone()).await;
    let opened = match args.show {
        Some(path) => service
            .handle(Action::OpenShow { path })
            .await
            .map(|_| true),
        None => service.resume().await,
    };
    match opened {
        Ok(true) => tracing::info!("show loaded"),
        Ok(false) => tracing::info!("starting with an empty show"),
        Err(e) => tracing::error!(error = %e, "loading show failed"),
    }

    // The engine may come up after us: keep trying, then feed it the show.
    tokio::spawn(connect(engine, service.clone(), args.slots));
    tokio::spawn(service.clone().forward_levels());
    tokio::spawn(service.clone().watch_failures());
    tokio::spawn(service.clone().forward_output_stats());
    tokio::spawn(service.clone().watch_players());
    tokio::spawn(service.clone().watch_titles());
    if !args.midi.is_empty() {
        tokio::spawn(midi::serve(args.midi.clone(), service.clone()));
    }
    if let Some(listen) = args.osc {
        let service = service.clone();
        tokio::spawn(async move {
            if let Err(error) = osc::serve(listen, service).await {
                tracing::error!(%error, "OSC stopped");
            }
        });
    }
    if !args.tsl.is_empty() {
        let service = service.clone();
        let (targets, version) = (args.tsl.clone(), args.tsl_version);
        tokio::spawn(async move {
            if let Err(error) = tsl::serve(targets, version, service).await {
                tracing::error!(%error, "TSL tally stopped");
            }
        });
    }
    if let Some(listen) = args.tcp {
        let service = service.clone();
        tokio::spawn(async move {
            if let Err(error) = tcp::serve(listen, service).await {
                tracing::error!(%error, "text protocol stopped");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("bind {}", args.listen))?;
    tracing::info!(listen = %args.listen, "lmix-core API up");

    axum::serve(listener, api::router(service))
        .with_graceful_shutdown(stopped())
        .await?;
    Ok(())
}

/// Ctrl-C, or SIGTERM from systemd (OPS.1). The engine keeps running.
async fn stopped() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = signal(SignalKind::terminate()).expect("SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
    tracing::info!("stopping");
}

/// Wait between engine start attempts.
const ENGINE_RETRY: Duration = Duration::from_secs(2);

async fn connect(engine: Arc<dyn Engine>, service: Arc<Service>, slots: usize) {
    loop {
        match engine.start(slots).await {
            Ok(()) => break,
            Err(e) => tracing::warn!(error = %e, "engine not ready, retrying"),
        }
        tokio::time::sleep(ENGINE_RETRY).await;
    }

    tracing::info!("engine ready");
    if let Err(e) = service.restore().await {
        tracing::error!(error = %e, "restoring show to engine failed");
    }
    // After restore: it restarts outputs lmix-core knew as running (engine
    // restarted); outputs that kept running while lmix-core was gone are
    // only adopted, never restarted (no gap in a recording).
    service.adopt_running_outputs().await;
    service.watch_engine(slots).await;
}
