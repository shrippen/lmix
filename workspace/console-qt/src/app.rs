//! lmix in Qt Quick / Kirigami (GUI-SHELL.md): one window with pages
//! (Mix, Show, ...; docs/PLAN-SEITEN.md). A client of lmix-core, with live
//! monitors drawn by GStreamer straight into QML items; several can run at
//! the same time against the same lmix-core.

use clap::{CommandFactory, FromArgMatches, Parser, ValueEnum};
use cxx_qt_lib::{QGuiApplication, QQmlApplicationEngine, QString, QUrl};
use gstreamer as gst;
use std::path::{Path, PathBuf};

use crate::{atlas_cell, config, gl_format, qt_translations};

/// Qt application name: settings in `~/.config/lmix/lmix.conf`; also the
/// binary, desktop file and Wayland app id.
const NAME: &str = "lmix";
/// Settings of the two programs before the pages (2026-10-18 to 10-23).
const MIXER_SETTINGS: &str = "mixer.conf";
const SHOW_SETTINGS: &str = "show.conf";
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
    /// (Kante at first). Not `--style`: Qt reads the same arguments and
    /// would look for a Quick Controls style of that name.
    #[arg(long, value_enum)]
    theme: Option<Style>,

    /// How the window opens (UI.12).
    #[arg(long, value_enum, default_value_t = Window::Normal)]
    window: Window,

    /// Mix page: simple (switching essentials) or advanced for this run;
    /// without it the one chosen on the page (UI.7, UI.12).
    #[arg(long, value_enum)]
    mode: Option<Mode>,

    /// The page to open on; without it the one last open.
    #[arg(long, value_enum)]
    page: Option<Page>,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Page {
    Mix,
    Show,
    Audio,
    Outputs,
    Setup,
}

impl Page {
    fn name(self) -> &'static str {
        match self {
            Page::Mix => "mix",
            Page::Show => "show",
            Page::Audio => "audio",
            Page::Outputs => "outputs",
            Page::Setup => "setup",
        }
    }
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

/// Takes over the settings of the programs before the pages once:
/// `mixer.conf` and `show.conf` merged into `lmix.conf`, or, before those,
/// the one Console's `console.conf`. Through `lmix.conf.tmp` and a rename: a
/// copy cut short leaves no half file that would count as the new one.
/// True when it wrote one.
fn adopt_old_settings(dir: &Path) -> std::io::Result<bool> {
    let own = dir.join(format!("{NAME}.conf"));
    if own.exists() {
        return Ok(false);
    }
    let read = |name: &str| -> std::io::Result<Option<String>> {
        let path = dir.join(name);
        if path.is_file() {
            std::fs::read_to_string(path).map(Some)
        } else {
            Ok(None)
        }
    };
    let text = match (read(MIXER_SETTINGS)?, read(SHOW_SETTINGS)?) {
        (None, None) => match read(OLD_SETTINGS)? {
            Some(old) => without_global_off(&old),
            None => return Ok(false),
        },
        (mixer, show) => merge_settings(mixer.as_deref().unwrap_or(""), show.as_deref()),
    };
    let tmp = dir.join(format!("{NAME}.conf.tmp"));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &own)?;
    Ok(true)
}

/// An INI file as sections in order, each with its lines (the header line
/// first; lines before any header under "").
fn sections(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = vec![(String::new(), Vec::new())];
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            out.push((trimmed.to_string(), vec![line.to_string()]));
        } else if !trimmed.is_empty() {
            out.last_mut().unwrap().1.push(line.to_string());
        }
    }
    out.retain(|(_, lines)| !lines.is_empty());
    out
}

fn key_of(line: &str) -> Option<&str> {
    line.split_once('=').map(|(key, _)| key.trim())
}

/// The mixer's settings with the show's added: a key both have stays the
/// mixer's, except whether shortcut keys work at the desktop (`[keys]
/// global`), which is the show's (on by default there, off in the mixer;
/// one app keeps them on). A `global=true` of the mixer alone is kept.
fn merge_settings(mixer: &str, show: Option<&str>) -> String {
    let show_sections = show.map(sections).unwrap_or_default();
    let show_has_global = show_sections
        .iter()
        .any(|(name, lines)| name == "[keys]" && lines.iter().any(|l| key_of(l) == Some("global")));
    let mut merged = sections(mixer);
    for (name, lines) in merged.iter_mut() {
        if name == "[keys]" {
            lines.retain(|l| {
                key_of(l) != Some("global")
                    || (!show_has_global && l.replace(' ', "").ends_with("=true"))
            });
        }
    }
    for (name, lines) in show_sections {
        match merged.iter_mut().find(|(n, _)| *n == name) {
            Some((_, have)) => {
                for line in lines.into_iter().skip(usize::from(!name.is_empty())) {
                    let present =
                        key_of(&line).is_some_and(|k| have.iter().any(|h| key_of(h) == Some(k)));
                    if !present {
                        have.push(line);
                    }
                }
            }
            None => merged.push((name, lines)),
        }
    }
    let mut out = String::new();
    for (_, lines) in merged {
        for line in lines {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

/// The old console wrote its default `global=false` under `[keys]` into
/// every file; kept, it would hide the default (on). Only a key the user
/// switched on is carried over.
fn without_global_off(text: &str) -> String {
    let mut section = "";
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed;
        } else if section == "[keys]" && trimmed.replace(' ', "") == "global=false" {
            continue;
        }
        out.push_str(line);
    }
    out
}

/// Starts lmix; returns only by ending the process.
pub fn run() -> ! {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .init();
    let matches = Args::command()
        .name(NAME)
        .about("lmix: mix, show, audio, outputs and setup in one window (Qt)")
        .get_matches();
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
    let page = args.page.map_or("", Page::name);
    config::init(
        &args.core,
        args.theme.map_or(-1, Style::kind),
        window,
        mode,
        page,
    );

    if let Some(dir) = settings_dir() {
        match adopt_old_settings(&dir) {
            Ok(true) => tracing::info!(
                "settings taken over into {}",
                dir.join(format!("{NAME}.conf")).display()
            ),
            Ok(false) => {}
            Err(e) => tracing::warn!("old settings not taken over: {e}"),
        }
    }

    // qml6glsink draws with the scene graph's OpenGL context, desktop GL
    // so that the engine's GPU frames can be copied into it.
    std::env::set_var("QSG_RHI_BACKEND", "opengl");
    gl_format::ffi::use_desktop_gl();
    atlas_cell::ffi::register_atlas_cell();
    // Draw on the scene graph's own thread: Qt picks its basic loop here
    // (Wayland, NVIDIA), which renders every frame of every live picture
    // on the GUI thread (24 % of it with 31 live tiles, 3 % threaded).
    if std::env::var_os("QSG_RENDER_LOOP").is_none() {
        std::env::set_var("QSG_RENDER_LOOP", "threaded");
    }
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
        qt_app.as_mut().set_application_name(&QString::from(NAME));
        QGuiApplication::set_desktop_file_name(&QString::from(NAME));
        qt_translations::ffi::install_qt_translations();
    }
    #[cfg(feature = "qml-profile")]
    if let Some(port) = std::env::var("LMIX_QML_PROFILE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
    {
        crate::qml_profile::ffi::start_qml_profiler(port);
    }
    let mut engine = QQmlApplicationEngine::new();
    if let Some(engine) = engine.as_mut() {
        engine.load(&QUrl::from(&format!("{QML_DIR}Main.qml")));
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

    fn own(dir: &Path) -> String {
        std::fs::read_to_string(dir.join("lmix.conf")).unwrap()
    }

    #[test]
    fn mixer_and_show_settings_merge_once() {
        let dir = folder("merge");
        std::fs::write(
            dir.join(MIXER_SETTINGS),
            "[appearance]\nstyle=2\n[keys]\nglobal=false\n[view]\ntileSize=2\nrows=3\n",
        )
        .unwrap();
        std::fs::write(
            dir.join(SHOW_SETTINGS),
            "[appearance]\nstyle=0\n[view]\ndarkRoom=true\nrows=1\n",
        )
        .unwrap();

        assert!(adopt_old_settings(&dir).unwrap());
        // The mixer's style and rows win, the show's dark room is added,
        // the mixer's default `global=false` is gone (on by default now).
        assert_eq!(
            own(&dir),
            "[appearance]\nstyle=2\n[keys]\n[view]\ntileSize=2\nrows=3\ndarkRoom=true\n"
        );

        // lmix.conf wins from then on.
        std::fs::write(dir.join("lmix.conf"), "[appearance]\nstyle=1\n").unwrap();
        assert!(!adopt_old_settings(&dir).unwrap());
        assert_eq!(own(&dir), "[appearance]\nstyle=1\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn global_keys_come_from_the_show() {
        assert_eq!(
            merge_settings("[keys]\nglobal=true\n", Some("[keys]\nglobal=false\n")),
            "[keys]\nglobal=false\n"
        );
        // Switched on in the mixer alone: stays on.
        assert_eq!(
            merge_settings("[keys]\nglobal=true\n", None),
            "[keys]\nglobal=true\n"
        );
        assert_eq!(
            merge_settings("[keys]\nglobal = true\n", Some("[view]\nx=1\n")),
            "[keys]\nglobal = true\n[view]\nx=1\n"
        );
    }

    #[test]
    fn a_copy_cut_short_is_redone() {
        let dir = folder("cut");
        std::fs::write(dir.join(SHOW_SETTINGS), "[appearance]\nstyle=2\n").unwrap();
        // What an earlier start left when it stopped mid-copy.
        std::fs::write(dir.join("lmix.conf.tmp"), "[appear").unwrap();

        assert!(adopt_old_settings(&dir).unwrap());
        assert_eq!(own(&dir), "[appearance]\nstyle=2\n");
        assert!(!dir.join("lmix.conf.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_old_console_settings_without_their_global_off() {
        let dir = folder("console");
        std::fs::write(
            dir.join(OLD_SETTINGS),
            "[appearance]\nstyle=2\n[keys]\nglobal=false\n[view]\nglobal=false\n",
        )
        .unwrap();

        assert!(adopt_old_settings(&dir).unwrap());
        assert_eq!(
            own(&dir),
            "[appearance]\nstyle=2\n[keys]\n[view]\nglobal=false\n"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn nothing_to_adopt_without_old_settings() {
        let dir = folder("none");
        assert!(!adopt_old_settings(&dir).unwrap());
        assert!(!dir.join("lmix.conf").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
