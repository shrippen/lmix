//! lmix in Qt Quick / Kirigami (GUI-SHELL.md): the start shared by the
//! two programs, mixer and show. Each is a client of lmix-core, with live
//! monitors drawn by GStreamer straight into QML items; both can run at
//! the same time against the same lmix-core.

use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use gstreamer as gst;
use std::path::{Path, PathBuf};

use crate::{config, qt_translations};

/// One of the programs: its name and window.
pub struct App {
    /// Qt application name: settings in `~/.config/lmix/<name>.conf`.
    pub name: &'static str,
    /// Binary, desktop file and Wayland app id, e.g. `lmix-mixer`.
    pub id: &'static str,
    /// `--help` line.
    pub about: &'static str,
    /// Root QML file of the window.
    pub qml: &'static str,
}

/// Settings of the one Console before the split (2026-10-18).
const OLD_SETTINGS: &str = "console.conf";
const ORGANIZATION: &str = "lmix";
const QML_DIR: &str = "qrc:/qt/qml/org/lmix/console/qml/";

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
struct Args {
    /// lmix-core base URL.
    #[arg(long, env = "LMIX_CORE", default_value = "http://127.0.0.1:8200")]
    core: String,

    /// Style for this run; without it the one chosen in the window
    /// (Kante at first).
    #[arg(long, value_enum)]
    style: Option<Style>,

    /// How the window opens (UI.12).
    #[arg(long, value_enum, default_value_t = Window::Normal)]
    window: Window,

    /// Mixer: simple (switching essentials) or advanced for this run;
    /// without it the one chosen in the mixer (UI.7, UI.12).
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

/// Where Qt keeps the settings of organization `lmix`.
fn settings_dir() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|home| Path::new(&home).join(".config")))?;
    Some(base.join(ORGANIZATION))
}

/// Copies the old `console.conf` to `<name>.conf` once, so style and view
/// settings survive the split; later the two files go their own ways.
/// Through `<name>.conf.tmp` and a rename: a copy cut short leaves no half
/// file that would count as the program's own. True when it copied.
fn adopt_old_settings(dir: &Path, name: &str) -> std::io::Result<bool> {
    let own = dir.join(format!("{name}.conf"));
    let old = dir.join(OLD_SETTINGS);
    if own.exists() || !old.is_file() {
        return Ok(false);
    }
    let tmp = dir.join(format!("{name}.conf.tmp"));
    std::fs::copy(&old, &tmp)?;
    std::fs::rename(&tmp, &own)?;
    Ok(true)
}

/// Starts the program `app`; returns only by ending the process.
pub fn run(app: &App) -> ! {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let matches = Args::command().name(app.id).about(app.about).get_matches();
    let args = Args::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
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

    if let Some(dir) = settings_dir() {
        match adopt_old_settings(&dir, app.name) {
            Ok(true) => tracing::info!(
                "settings taken over from {}",
                dir.join(OLD_SETTINGS).display()
            ),
            Ok(false) => {}
            Err(e) => tracing::warn!("settings of {OLD_SETTINGS} not taken over: {e}"),
        }
    }

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

    let mut qt_app = QGuiApplication::new();
    if let Some(mut qt_app) = qt_app.as_mut() {
        // Where Qt keeps this program's settings, and the Wayland app id
        // for window rules.
        qt_app
            .as_mut()
            .set_organization_name(&QString::from(ORGANIZATION));
        qt_app
            .as_mut()
            .set_application_name(&QString::from(app.name));
        QGuiApplication::set_desktop_file_name(&QString::from(app.id));
        qt_translations::ffi::install_qt_translations();
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(&format!("{QML_DIR}{}", app.qml)));
    }
    let code = qt_app.as_mut().map(|app| app.exec()).unwrap_or(1);
    // Live pipelines hold GL resources of the window; quit before Qt tears
    // down the context under them.
    drop(engine);
    std::process::exit(code);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fresh, empty folder for one test.
    fn folder(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("lmix-app-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn old_settings_are_copied_once() {
        let dir = folder("copy");
        std::fs::write(dir.join(OLD_SETTINGS), "[appearance]\nstyle=2\n").unwrap();

        assert!(adopt_old_settings(&dir, "mixer").unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.join("mixer.conf")).unwrap(),
            "[appearance]\nstyle=2\n"
        );

        // The program's own file wins from then on.
        std::fs::write(dir.join("mixer.conf"), "[appearance]\nstyle=0\n").unwrap();
        assert!(!adopt_old_settings(&dir, "mixer").unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.join("mixer.conf")).unwrap(),
            "[appearance]\nstyle=0\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_copy_cut_short_is_redone() {
        let dir = folder("cut");
        std::fs::write(dir.join(OLD_SETTINGS), "[appearance]\nstyle=2\n").unwrap();
        // What an earlier start left when it stopped mid-copy.
        std::fs::write(dir.join("show.conf.tmp"), "[appear").unwrap();

        assert!(adopt_old_settings(&dir, "show").unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.join("show.conf")).unwrap(),
            "[appearance]\nstyle=2\n"
        );
        assert!(!dir.join("show.conf.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_to_adopt_without_old_settings() {
        let dir = folder("none");
        assert!(!adopt_old_settings(&dir, "show").unwrap());
        assert!(!dir.join("show.conf").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
