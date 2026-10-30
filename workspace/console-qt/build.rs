use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

fn files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read qml dir") {
        let path = entry.expect("dir entry").path();
        if path.is_dir() {
            files(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// A .qrc that puts Kante (scripts/sync-kante.sh) at the default import
/// path, so `import Kante` finds it in the binary.
fn kante_qrc() -> PathBuf {
    let kante = Path::new("qml/Kante")
        .canonicalize()
        .expect("qml/Kante (scripts/sync-kante.sh)");
    let mut found = Vec::new();
    files(&kante, &mut found);
    found.sort();
    let mut qrc = String::from("<RCC>\n  <qresource prefix=\"/qt/qml/Kante\">\n");
    for path in &found {
        let alias = path.strip_prefix(&kante).unwrap().display();
        writeln!(qrc, "    <file alias=\"{alias}\">{}</file>", path.display()).unwrap();
    }
    qrc.push_str("  </qresource>\n</RCC>\n");
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("kante.qrc");
    std::fs::write(&out, qrc).expect("write kante.qrc");
    println!("cargo::rerun-if-changed=qml/Kante");
    for path in &found {
        println!("cargo::rerun-if-changed={}", path.display());
    }
    out
}

/// Qt's lrelease: `$LRELEASE`, Qt's host binaries, or the PATH.
fn lrelease() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("LRELEASE") {
        return Some(path.into());
    }
    let host_bins = std::process::Command::new("qtpaths6")
        .args(["--query", "QT_HOST_BINS"])
        .output()
        .ok()
        .map(|out| PathBuf::from(String::from_utf8_lossy(&out.stdout).trim()));
    host_bins
        .map(|dir| dir.join("lrelease"))
        .into_iter()
        .chain(["lrelease", "lrelease-qt6"].map(PathBuf::from))
        .find(|tool| {
            std::process::Command::new(tool)
                .arg("-version")
                .output()
                .is_ok_and(|out| out.status.success())
        })
}

/// The translations (i18n/qml_<lang>.ts) as .qm files in a .qrc beside
/// the root QML files, where QQmlApplicationEngine looks for them (`i18n/qml_<lang>.qm`).
/// Without lrelease the Console builds in English only.
fn translations_qrc() -> Option<PathBuf> {
    println!("cargo::rerun-if-changed=i18n");
    let Some(tool) = lrelease() else {
        println!("cargo::warning=lrelease (Qt Linguist tools) not found: the Console is built in English only");
        return None;
    };
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("i18n");
    std::fs::create_dir_all(&out).expect("create i18n dir");
    let mut qrc =
        String::from("<RCC>\n  <qresource prefix=\"/qt/qml/org/lmix/console/qml/i18n\">\n");
    let mut sources: Vec<PathBuf> = std::fs::read_dir("i18n")
        .expect("i18n dir")
        .map(|entry| entry.expect("dir entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "ts"))
        .collect();
    sources.sort();
    for ts in sources {
        let qm = out.join(ts.with_extension("qm").file_name().unwrap());
        let status = std::process::Command::new(&tool)
            .args(["-silent"])
            .arg(&ts)
            .arg("-qm")
            .arg(&qm)
            .status()
            .expect("run lrelease");
        assert!(status.success(), "lrelease failed on {}", ts.display());
        println!("cargo::rerun-if-changed={}", ts.display());
        let alias = qm.file_name().unwrap().to_string_lossy().into_owned();
        writeln!(qrc, "    <file alias=\"{alias}\">{}</file>", qm.display()).unwrap();
    }
    qrc.push_str("  </qresource>\n</RCC>\n");
    let path = out.join("i18n.qrc");
    std::fs::write(&path, qrc).expect("write i18n.qrc");
    Some(path)
}

fn main() {
    let mut builder = CxxQtBuilder::new_qml_module(
        QmlModule::new("org.lmix.console")
            .qml_files([
                "qml/AppShell.qml",
                "qml/Main.qml",
                "qml/AppPage.qml",
                "qml/MixPage.qml",
                "qml/BusButtons.qml",
                "qml/ShowPage.qml",
                "qml/AudioPage.qml",
                "qml/BusPanel.qml",
                "qml/ZoneBar.qml",
                "qml/NarrowMixer.qml",
                "qml/OutputsPage.qml",
                "qml/StopOutputDialog.qml",
                "qml/FactoryResetDialog.qml",
                "qml/SetupPage.qml",
                "qml/LiveView.qml",
                "qml/TilesAtlas.qml",
                "qml/MessageToasts.qml",
                "qml/Removals.qml",
                "qml/InputMenu.qml",
                "qml/InputTile.qml",
                "qml/AddInputDialog.qml",
                "qml/AddOutputDialog.qml",
                "qml/TransitionPanel.qml",
                "qml/ChannelStripDialog.qml",
                "qml/BusDialog.qml",
                "qml/FormatDialog.qml",
                "qml/StingerDialog.qml",
                "qml/OverlayDialog.qml",
                "qml/LookDialog.qml",
                "qml/LayoutBar.qml",
                "qml/CollapsedBar.qml",
                "qml/AppendShowDialog.qml",
                "qml/LayoutDialog.qml",
                "qml/ShortcutsDialog.qml",
                "qml/ShortcutEditDialog.qml",
                "qml/ActionRows.qml",
                "qml/MidiLearnButton.qml",
                "qml/ActionListDialog.qml",
                "qml/TriggersDialog.qml",
                "qml/CueListsDialog.qml",
                "qml/ShowView.qml",
                "qml/ShowSidebar.qml",
                "qml/ShowInspector.qml",
                "qml/DurationField.qml",
                "qml/TargetsDialog.qml",
                "qml/ZonesDialog.qml",
                "qml/OutputBar.qml",
                "qml/ChoiceMenu.qml",
                "qml/KeyedModel.qml",
                "qml/MoveThrottle.qml",
                "qml/ShowState.qml",
                "qml/TitleField.qml",
                "qml/TitlesDialog.qml",
                "qml/DataSourcesDialog.qml",
                "qml/TransportBar.qml",
                "qml/PtzDialog.qml",
                "qml/RenameInputDialog.qml",
                "qml/TileSizePicker.qml",
                "qml/EffectMenu.qml",
                "qml/LookPanel.qml",
                "qml/ChannelStripPanel.qml",
                "qml/InputSettingsDialog.qml",
            ])
            .qml_file(QmlFile::from("qml/Texts.qml").singleton(true))
            .qml_file(QmlFile::from("qml/StatePatch.qml").singleton(true)),
    )
    .files([
        "src/bridge.rs",
        "src/qt_translations.rs",
        "src/gl_format.rs",
        "src/atlas_cell.rs",
        "src/qml_profile.rs",
    ])
    .cpp_file("cpp/qt_translations.cpp")
    .cpp_file("cpp/gl_format.cpp")
    .cpp_file("cpp/atlas_cell.h")
    .cpp_file("cpp/atlas_cell.cpp")
    .cpp_file("cpp/qml_profile.cpp")
    .qrc(kante_qrc());
    if let Some(qrc) = translations_qrc() {
        builder = builder.qrc(qrc);
    }
    // Qt's QML profiler service: never in a release build.
    if std::env::var_os("CARGO_FEATURE_QML_PROFILE").is_some() {
        // SAFETY: one define read by cpp/qml_profile.cpp only; no flag that
        // changes how cxx-qt's own code is built.
        builder = unsafe {
            builder.cc_builder(|cc| {
                cc.define("LMIX_QML_PROFILE", None);
            })
        };
    }
    builder.qt_module("Quick").build();
}
