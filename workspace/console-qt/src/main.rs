//! lmix Console in Qt Quick / Kirigami (GUI-SHELL.md, step 1): a client of
//! lmix-core like the egui Console, with live monitors drawn by GStreamer
//! straight into QML items.

use clap::{Parser, ValueEnum};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QUrl};
use gstreamer as gst;

mod bridge;
mod config;

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Style {
    /// Kante, dark (Leinen when the platform theme is light).
    Kante,
    /// Kante shapes on the platform colours.
    KanteLight,
    /// The platform theme (Breeze under Plasma).
    System,
}

impl Style {
    /// `KanteStyle.Kind` value.
    fn kind(self) -> i32 {
        match self {
            Style::System => 0,
            Style::Kante => 1,
            Style::KanteLight => 2,
        }
    }
}

#[derive(Parser, Debug)]
#[command(about = "lmix Console (Qt)")]
struct Args {
    /// lmix-core base URL.
    #[arg(long, env = "LMIX_CORE", default_value = "http://127.0.0.1:8200")]
    core: String,

    #[arg(long, value_enum, default_value_t = Style::Kante)]
    style: Style,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    config::init(&args.core, args.style.kind());

    // qml6glsink draws with the scene graph's OpenGL context.
    std::env::set_var("QSG_RHI_BACKEND", "opengl");
    if let Err(e) = gst::init() {
        eprintln!("GStreamer: {e}");
        std::process::exit(1);
    }
    // Loading the qml6 plugin registers GstGLQt6VideoItem for QML.
    if gst::ElementFactory::make("qml6glsink").build().is_err() {
        eprintln!("GStreamer element qml6glsink missing (gst-plugin-qml6 / gstreamer1.0-qt6)");
        std::process::exit(1);
    }

    let mut app = QGuiApplication::new();
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from("qrc:/qt/qml/org/lmix/console/qml/Main.qml"));
    }
    let code = app.as_mut().map(|app| app.exec()).unwrap_or(1);
    // Live pipelines hold GL resources of the window; quit before Qt tears
    // down the context under them.
    drop(engine);
    std::process::exit(code);
}
