//! lmix: one window with pages (Mix, Show, ...) over lmix-core
//! (docs/PLAN-SEITEN.md).

mod app;
mod bridge;
mod config;
mod forms;
mod portal;
mod qt_translations;

fn main() {
    app::run();
}
