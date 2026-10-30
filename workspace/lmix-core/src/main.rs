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

mod access;
mod api;
mod checks;
mod clips;
mod cues;
mod datasource;
mod engine;
mod feedback;
mod feeds;
mod layouts;
mod machine;
mod metrics;
mod midi;
mod osc;
mod osc_subs;
mod package;
mod patch;
mod playback;
mod ptz;
mod realtime;
mod recent;
mod script;
mod service;
mod settings;
mod show;
mod stages;
mod switcher;
mod tally_http;
mod targets;
mod tcp;
mod titles;
mod tsl;
mod web;
mod zones;

use engine::flows::{MixerBackend, MonitorOut};
use engine::null::NullEngine;
use engine::strom::StromEngine;
use engine::Engine;
use service::Service;

/// Mixer slots by default; at most `flows::MAX_SLOTS`.
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

    /// Mixer slots (inputs fed at the same time), 1 to 31: 32 made a
    /// mixer the engine refused, over and over (29.10.2026).
    #[arg(long, default_value_t = DEFAULT_SLOTS,
          value_parser = clap::builder::RangedU64ValueParser::<usize>::new().range(1..=engine::flows::MAX_SLOTS as u64))]
    slots: usize,

    /// Address of the text protocol over TCP (API.5); off without it.
    #[arg(long, env = "LMIX_TCP")]
    tcp: Option<SocketAddr>,

    /// MIDI device files to read (`/dev/snd/midiC1D0`); repeat for more.
    #[arg(long, env = "LMIX_MIDI", value_delimiter = ',')]
    midi: Vec<PathBuf>,

    /// Light the shortcuts' MIDI notes and controllers on the `--midi`
    /// devices as their inputs stand (AUT.6).
    #[arg(long, env = "LMIX_MIDI_FEEDBACK")]
    midi_feedback: bool,

    /// Lamp values for program, preview and off (colours on some
    /// controllers, e.g. APC mini 3,1,0).
    #[arg(long, env = "LMIX_MIDI_LAMPS", default_value = "127,64,0")]
    midi_lamps: feedback::Lamps,

    /// UDP address for OSC (AUT.13); off without it.
    #[arg(long, env = "LMIX_OSC")]
    osc: Option<SocketAddr>,

    /// Shortcuts for all shows (AUT.3), kept apart from show files.
    #[arg(long, env = "LMIX_SHORTCUTS", default_value_os_t = config_file("shortcuts.json"))]
    shortcuts: PathBuf,

    /// How often running cues go to subscribers and OSC subscribers while
    /// cues run (CTL.2), 1–20 Hz.
    #[arg(long, env = "LMIX_CUE_EVENTS_HZ", default_value_t = 4, value_parser = clap::value_parser!(u32).range(1..=20))]
    cue_events_hz: u32,

    /// The lmix-script tool, run for a show's script folder (show mode
    /// SSH.2); default: next to lmix-core, else from PATH.
    #[arg(long, env = "LMIX_SCRIPT_TOOL")]
    script_tool: Option<PathBuf>,

    /// Output targets of this machine with their secrets (show mode
    /// EXT.1), kept apart from show files; written readable only by you.
    #[arg(long, env = "LMIX_TARGETS", default_value_os_t = config_file("targets.json"))]
    targets: PathBuf,

    /// Layout templates (LAY.5), kept apart from show files.
    #[arg(long, env = "LMIX_LAYOUT_TEMPLATES", default_value_os_t = config_file("layout-templates.json"))]
    layout_templates: PathBuf,

    /// Password for access from the network (API.7); programs on this
    /// machine need none. HTTP Basic or Bearer; text protocol `AUTH`.
    #[arg(long, env = "LMIX_PASSWORD", hide_env_values = true)]
    password: Option<String>,

    /// Access keys with rights, a JSON list of `{name, key, right}` with
    /// right `view`, `control` or `edit` (show mode CTL.5); like the
    /// password, for HTTP and the text protocol.
    #[arg(long, env = "LMIX_ACCESS_KEYS")]
    access_keys: Option<PathBuf>,

    /// Panic and hard stop from the local network without a key (a
    /// button by the stage), also over OSC (show mode CTL.5, SAF.5).
    #[arg(long, env = "LMIX_OPEN_PANIC")]
    open_panic: bool,

    /// Addresses or networks with a right without a key, e.g.
    /// `192.168.1.20=control`, `192.168.1.0/24=view` (show mode CTL.5);
    /// only matters with `--password` or `--access-keys`. HTTP, text
    /// protocol and OSC.
    #[arg(long, value_delimiter = ',', env = "LMIX_TRUST")]
    trust: Vec<access::Trust>,

    /// Browser pages usable from the network without the password, with
    /// only what they need (`tally`, `keys`, `mixer`, `titles`).
    #[arg(long, value_enum, value_delimiter = ',', env = "LMIX_OPEN_PAGES")]
    open_page: Vec<access::Page>,

    /// Answer addresses outside the local network too (behind a reverse
    /// proxy or VPN); by default only private and link-local ones.
    #[arg(long, env = "LMIX_ALLOW_INTERNET")]
    allow_internet: bool,

    /// Key for the Google Sheets API (data sources `google_sheet`,
    /// DAT.1); kept out of show files.
    #[arg(long, env = "LMIX_GOOGLE_API_KEY", hide_env_values = true)]
    google_api_key: Option<String>,

    /// How long before its time a frame goes to a DeckLink card, in ms
    /// (the sink's processing deadline).
    #[arg(long, env = "LMIX_CARD_LEAD_MS", default_value_t = engine::flows::CARD_LEAD_MS)]
    card_lead_ms: u64,

    /// Frames a DeckLink card queues before playback; each one is a
    /// frame of delay.
    #[arg(long, env = "LMIX_CARD_FRAMES", default_value_t = engine::flows::CARD_FRAMES)]
    card_frames: u64,

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

    /// The same over TCP to these addresses (TSL 5.0 framed with DLE/STX).
    #[arg(long, env = "LMIX_TSL_TCP", value_delimiter = ',')]
    tsl_tcp: Vec<SocketAddr>,

    /// Call these addresses with the tally on every change (tally lights
    /// over HTTP, PER.1): POST JSON, or GET with `{program}`, `{preview}`,
    /// `{tally}` filled in; repeat for more (in the environment variable:
    /// separated by spaces).
    #[arg(long, env = "LMIX_TALLY_HTTP", value_delimiter = ' ')]
    tally_http: Vec<String>,

    /// TSL UMD version: 5 (5.0) or 3 (3.1).
    #[arg(long, env = "LMIX_TSL_VERSION", value_enum, default_value_t = tsl::Version::V50)]
    tsl_version: tsl::Version,

    /// Outputs to start once the engine is up (number or name; repeat or
    /// separate with commas), e.g. the stream and the recording (UI.12).
    /// One already running is left as it is.
    #[arg(long, env = "LMIX_START_OUTPUTS", value_delimiter = ',')]
    start_outputs: Vec<String>,

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

/// `$XDG_CONFIG_HOME/lmix/<name>`, else `~/.config/…`.
fn config_file(name: &str) -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("lmix")
        .join(name)
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
    if let Some(key) = args.google_api_key.clone().filter(|k| !k.trim().is_empty()) {
        datasource::set_google_api_key(key.trim().to_string());
    }
    engine::flows::set_card_lead_ms(args.card_lead_ms);
    engine::flows::set_card_frames(args.card_frames);
    metrics::started();
    let engine: Arc<dyn Engine> = match args.engine {
        EngineKind::Strom => {
            let engine = StromEngine::new(args.strom.clone(), args.mixer, args.monitor_out)
                .with_pages(pages_url(args.listen));
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
    service.use_shortcuts_file(args.shortcuts.clone()).await;
    service.use_targets_file(args.targets.clone()).await;
    service
        .use_layout_templates_file(args.layout_templates.clone())
        .await;
    let opened = match args.show {
        Some(path) => service
            .handle(Action::OpenShow { path })
            .await
            .map(|_| true),
        None => service.resume().await,
    };
    let tool = args
        .script_tool
        .clone()
        .unwrap_or_else(script::default_tool);
    service
        .use_script_tool(tool, format!("http://{}", args.listen))
        .await;
    match opened {
        Ok(true) => tracing::info!("show loaded"),
        Ok(false) => tracing::info!("starting with an empty show"),
        Err(e) => tracing::error!(error = %e, "loading show failed"),
    }

    // The engine may come up after us: keep trying, then feed it the show.
    tokio::spawn(connect(
        engine,
        service.clone(),
        args.slots,
        args.start_outputs.clone(),
    ));
    tokio::spawn(service.clone().forward_levels());
    tokio::spawn(service.clone().watch_failures());
    tokio::spawn(service.clone().forward_output_stats());
    tokio::spawn(service.clone().watch_machine());
    tokio::spawn(service.clone().watch_players());
    tokio::spawn(service.clone().watch_stingers());
    tokio::spawn(service.clone().watch_playback());
    tokio::spawn(service.clone().watch_media());
    tokio::spawn(service.clone().watch_titles());
    spawn_cue_timer(service.clone());
    tokio::spawn(service.clone().watch_feeds(args.cue_events_hz));
    if !args.midi.is_empty() {
        tokio::spawn(midi::serve(args.midi.clone(), service.clone()));
        if args.midi_feedback {
            tokio::spawn(feedback::serve(
                args.midi.clone(),
                args.midi_lamps,
                service.clone(),
            ));
        }
    }
    if let Some(listen) = args.osc {
        let service = service.clone();
        let internet = args.allow_internet;
        // No password over OSC: with one set, only this machine may send
        // (panic too from the network with --open-panic).
        let local_only =
            args.password.as_deref().is_some_and(|p| !p.is_empty()) || args.access_keys.is_some();
        let open_panic = args.open_panic;
        let trusted = args.trust.clone();
        if local_only {
            tracing::info!(
                "OSC takes messages from this machine and trusted addresses only (it has no password)"
            );
        }
        tokio::spawn(async move {
            if let Err(error) =
                osc::serve(listen, service, internet, local_only, open_panic, trusted).await
            {
                tracing::error!(%error, "OSC stopped");
            }
        });
    }
    if !args.tsl.is_empty() || !args.tsl_tcp.is_empty() {
        let service = service.clone();
        let (targets, tcp, version) = (args.tsl.clone(), args.tsl_tcp.clone(), args.tsl_version);
        tokio::spawn(async move {
            if let Err(error) = tsl::serve(targets, tcp, version, service).await {
                tracing::error!(%error, "TSL tally stopped");
            }
        });
    }
    if !args.tally_http.is_empty() {
        tokio::spawn(tally_http::serve(args.tally_http.clone(), service.clone()));
    }
    let keys = match &args.access_keys {
        Some(path) => access::read_keys(path)?,
        None => Vec::new(),
    };
    let rules = Arc::new(access::Access {
        password: args.password.clone().filter(|p| !p.is_empty()),
        open_pages: args.open_page.clone(),
        internet: args.allow_internet,
        keys,
        open_panic: args.open_panic,
        trusted: args.trust.clone(),
        ..access::Access::default()
    });
    if let Some(listen) = args.tcp {
        let service = service.clone();
        let rules = rules.clone();
        tokio::spawn(async move {
            if let Err(error) = tcp::serve(listen, service, rules).await {
                tracing::error!(%error, "text protocol stopped");
            }
        });
    }

    let listener = tokio::net::TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("bind {}", args.listen))?;
    tracing::info!(listen = %args.listen, "lmix-core API up");

    let app = api::router(service.clone()).layer(axum::middleware::from_fn_with_state(
        (rules, service),
        access::guard,
    ));
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
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

async fn connect(engine: Arc<dyn Engine>, service: Arc<Service>, slots: usize, start: Vec<String>) {
    loop {
        match engine.start(slots).await {
            Ok(()) => break,
            Err(e) => tracing::warn!(error = %e, "engine not ready, retrying"),
        }
        tokio::time::sleep(ENGINE_RETRY).await;
    }

    tracing::info!("engine ready");
    service.adopt_cards().await;
    if let Err(e) = service.restore().await {
        tracing::error!(error = %e, "restoring show to engine failed");
    }
    // After restore: it restarts outputs lmix-core knew as running (engine
    // restarted); outputs that kept running while lmix-core was gone are
    // only adopted, never restarted (no gap in a recording).
    service.adopt_running_outputs().await;
    start_outputs(&service, &start).await;
    service.watch_engine(slots).await;
}

/// Start the outputs `--start-outputs` names, unless they run already.
async fn start_outputs(service: &Arc<Service>, names: &[String]) {
    for name in names.iter().map(|n| n.trim()).filter(|n| !n.is_empty()) {
        let name = &name.to_string();
        let output = match name.parse::<u32>() {
            Ok(number) => lmix_types::OutputRef::Number(number),
            Err(_) => lmix_types::OutputRef::Text(name.clone()),
        };
        let state = service.state().await;
        let running = state.outputs.iter().enumerate().any(|(i, o)| {
            let named = match &output {
                lmix_types::OutputRef::Number(n) => *n as usize == i + 1,
                lmix_types::OutputRef::Text(t) => o.name == *t || o.id.to_string() == *t,
            };
            named && matches!(o.status, lmix_types::OutputStatus::Running { .. })
        });
        if running {
            tracing::info!(output = %name, "already running");
            continue;
        }
        match service.handle(Action::StartOutput { output }).await {
            Ok(_) => tracing::info!(output = %name, "started"),
            Err(error) => tracing::error!(output = %name, %error, "could not start"),
        }
    }
}

/// lmix-core's address as the engine on this machine reaches it: a
/// wildcard bind (`0.0.0.0`, `::`) is reached over loopback.
fn pages_url(listen: SocketAddr) -> String {
    let ip = match listen.ip() {
        ip if ip.is_unspecified() && ip.is_ipv4() => std::net::IpAddr::from([127, 0, 0, 1]),
        ip if ip.is_unspecified() => std::net::IpAddr::from(std::net::Ipv6Addr::LOCALHOST),
        ip => ip,
    };
    format!("http://{}", SocketAddr::new(ip, listen.port()))
}

/// The cue timer on its own thread and runtime with real-time priority
/// (RUN.8): on a busy machine, the shared runtime's workers wait for a
/// free core, and with them every wait and follow. The cues' actions run
/// there too, in order with their steps (a panic never overtakes them).
fn spawn_cue_timer(service: std::sync::Arc<service::Service>) {
    let (tid_tx, tid_rx) = std::sync::mpsc::channel();
    std::thread::Builder::new()
        .name("lmix-cues".into())
        .spawn(move || {
            /// Says so when the timer stops, a panic included.
            struct Ended;
            impl Drop for Ended {
                fn drop(&mut self) {
                    realtime::ended();
                    tracing::error!("cue timer stopped: waits and follows no longer run");
                }
            }
            let _ended = Ended;
            let _ = tid_tx.send(realtime::thread_id());
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("cue timer runtime builds")
                .block_on(service.watch_cues());
        })
        .expect("cue timer thread starts");
    // Asked from the side: a hung system bus must not hold up the timer.
    std::thread::spawn(move || {
        let Ok(tid) = tid_rx.recv() else { return };
        match realtime::make_realtime(tid) {
            Ok(how) => tracing::info!(how, "cue timer runs with real-time priority"),
            Err(why) => {
                tracing::warn!(
                    %why,
                    "cue timer without real-time priority: steps may be late on a busy machine (RUN.8)"
                );
                return;
            }
        }
        let mut reported = false;
        while !realtime::demoted() {
            if let (Some(errno), false) = (realtime::demotion_failed(), reported) {
                reported = true;
                tracing::error!(
                    errno,
                    "cue timer could not give up real-time priority at RLIMIT_RTTIME's soft limit: the hard limit may end lmix-core (RUN.8)"
                );
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        tracing::warn!(
            "cue timer gave up real-time priority for the rest of the run: a cue step computed past RLIMIT_RTTIME's soft limit without a pause (RUN.8)"
        );
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slots_stay_within_the_vision_mixer() {
        let slots = |n: &str| Args::try_parse_from(["lmix-core", "--slots", n]).map(|a| a.slots);
        assert_eq!(slots("31").unwrap(), engine::flows::MAX_SLOTS);
        assert!(slots("32").is_err());
        assert!(slots("0").is_err());
    }
}
