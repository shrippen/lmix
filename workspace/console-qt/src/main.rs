//! lmix Console in Qt Quick / Kirigami (GUI-SHELL.md, step 1): a client of
//! lmix-core like the egui Console, with live monitors drawn by GStreamer
//! straight into QML items.

use clap::{Parser, ValueEnum};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use gstreamer as gst;

mod bridge;
mod config;
mod forms;
mod portal;
mod qt_translations;

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

    /// Style for this run; without it the one chosen in the Console
    /// (Kante at first).
    #[arg(long, value_enum)]
    style: Option<Style>,

    /// How the window opens (UI.12).
    #[arg(long, value_enum, default_value_t = Window::Normal)]
    window: Window,

    /// Simple (switching essentials) or advanced for this run; without it
    /// the one chosen in the Console (UI.7, UI.12).
    #[arg(long, value_enum)]
    mode: Option<Mode>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Mode {
    Simple,
    Advanced,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Window {
    Normal,
    Maximized,
    Fullscreen,
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let args = Args::parse();
    let window = match args.window {
        Window::Normal => "normal",
        Window::Maximized => "maximized",
        Window::Fullscreen => "fullscreen",
    };
    let mode = match args.mode {
        None => "",
        Some(Mode::Simple) => "simple",
        Some(Mode::Advanced) => "advanced",
    };
    config::init(&args.core, args.style.map_or(-1, Style::kind), window, mode);

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
    if let Some(mut app) = app.as_mut() {
        // Where Qt keeps the Console's settings (style), and the Wayland
        // app id for window rules.
        app.as_mut().set_organization_name(&QString::from("lmix"));
        app.as_mut().set_application_name(&QString::from("console"));
        QGuiApplication::set_desktop_file_name(&QString::from("lmix-console"));
        qt_translations::ffi::install_qt_translations();
    }
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
