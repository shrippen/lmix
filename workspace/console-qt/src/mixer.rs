//! lmix Mixer: switches inputs live (PGM/PVW, LASTENHEFT.md).

mod app;
mod bridge;
mod config;
mod forms;
mod portal;
mod qt_translations;

fn main() {
    app::run(&app::App {
        name: "mixer",
        id: "lmix-mixer",
        about: "lmix Mixer: switch inputs live (Qt)",
        qml: "MixerMain.qml",
    });
}
