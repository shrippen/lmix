//! lmix: one window with pages (Mix, Show, ...) over lmix-core
//! (docs/PLAN-SEITEN.md).

mod app;
mod bridge;
mod config;
mod forms;
mod latest;
mod portal;
#[cfg(feature = "qml-profile")]
mod qml_profile;
mod qt_translations;
#[cfg(test)]
mod refusals;
mod state_patch;

fn main() {
    app::run();
}
