//! lmix: one window with pages (Mix, Show, ...) over lmix-core
//! (docs/PLAN-SEITEN.md).

mod app;
mod bridge;
mod config;
mod forms;
mod latest;
mod portal;
mod qt_translations;
#[cfg(test)]
mod refusals;

fn main() {
    app::run();
}
