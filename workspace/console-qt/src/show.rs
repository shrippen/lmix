//! lmix Show: runs prepared cues with GO (LASTENHEFT-SHOW.md).

mod app;
mod bridge;
mod config;
mod forms;
mod portal;
mod qt_translations;

fn main() {
    app::run(&app::App {
        name: "show",
        id: "lmix-show",
        about: "lmix Show: run prepared cues with GO (Qt)",
        qml: "ShowMain.qml",
    });
}
