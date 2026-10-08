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
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use clap::{Parser, ValueEnum};
use lmix_types::Action;

mod api;
mod engine;
mod service;
mod show;
mod switcher;

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

    /// Mixer slots (inputs fed at the same time).
    #[arg(long, default_value_t = DEFAULT_SLOTS)]
    slots: usize,

    /// Show file to open at start.
    #[arg(long)]
    show: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();

    let args = Args::parse();
    let engine: Arc<dyn Engine> = match args.engine {
        EngineKind::Strom => Arc::new(StromEngine::new(args.strom.clone())),
        EngineKind::None => Arc::new(NullEngine::default()),
    };

    let service = Service::new(args.slots, engine.clone());
    if let Some(path) = args.show {
        if let Err(e) = service.handle(Action::OpenShow { path }).await {
            tracing::error!(error = %e, "opening show failed");
        }
    }

    // The engine may come up after us: keep trying, then feed it the show.
    tokio::spawn(connect(engine, service.clone(), args.slots));

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("bind {}", args.listen))?;
    tracing::info!(listen = %args.listen, "lmix-core API up");

    axum::serve(listener, api::router(service))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
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
}
