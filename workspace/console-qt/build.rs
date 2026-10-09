use cxx_qt_build::{CxxQtBuilder, QmlModule};
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

fn main() {
    CxxQtBuilder::new_qml_module(QmlModule::new("org.lmix.console").qml_files([
        "qml/Main.qml",
        "qml/LiveView.qml",
        "qml/InputTile.qml",
        "qml/SourceFields.qml",
        "qml/AddInputDialog.qml",
        "qml/AddOutputDialog.qml",
        "qml/LevelMeter.qml",
        "qml/TransitionPanel.qml",
        "qml/OutputBar.qml",
        "qml/ChoiceMenu.qml",
        "qml/KeyedModel.qml",
    ]))
    .files(["src/bridge.rs"])
    .qrc(kante_qrc())
    .qt_module("Quick")
    .build();
}
