//! Process-wide settings and services, set once in `main` before the QML
//! engine creates the `Core` object.

use std::sync::OnceLock;

struct Config {
    core_url: String,
    style: i32,
}

static CONFIG: OnceLock<Config> = OnceLock::new();
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
static HTTP: OnceLock<reqwest::Client> = OnceLock::new();

pub fn init(core_url: &str, style: i32) {
    let _ = CONFIG.set(Config {
        core_url: core_url.trim_end_matches('/').to_string(),
        style,
    });
}

fn config() -> &'static Config {
    CONFIG.get().expect("config::init before use")
}

pub fn core_url() -> &'static str {
    &config().core_url
}

pub fn style() -> i32 {
    config().style
}

/// `ws://…/api/events` of `core_url`.
pub fn events_url() -> String {
    events_url_of(core_url())
}

fn events_url_of(base: &str) -> String {
    base.replacen("http://", "ws://", 1)
        .replacen("https://", "wss://", 1)
        + "/api/events"
}

/// Runtime for the network tasks; the Qt event loop owns the main thread.
pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("tokio runtime")
    })
}

pub fn http() -> &'static reqwest::Client {
    HTTP.get_or_init(reqwest::Client::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_url_follows_the_scheme() {
        assert_eq!(events_url_of("http://h:8200"), "ws://h:8200/api/events");
        assert_eq!(events_url_of("https://h"), "wss://h/api/events");
    }
}
