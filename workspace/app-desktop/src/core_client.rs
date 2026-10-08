//! Client of lmix-core: actions over HTTP, show state over WebSocket,
//! pictures as JPEG. The Console in `--core` mode talks to nothing else
//! (LASTENHEFT E.4: the GUI is one client among many).

use futures_util::stream::StreamExt;
use lmix_types::{Action, Event, InputId, ShowState};
use std::sync::mpsc::Sender;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

/// Wait between reconnect attempts.
const RECONNECT: Duration = Duration::from_secs(1);

/// What the background tasks report to the UI.
pub enum CoreMessage {
    Connected(bool),
    State(ShowState),
    /// Answer to an action: `None` when it ran, the reason otherwise.
    ActionDone(Option<String>),
    Picture(PictureTarget, egui::ColorImage),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PictureTarget {
    Program,
    Input(InputId),
}

#[derive(Clone)]
pub struct CoreClient {
    base_url: String,
    http: reqwest::Client,
}

impl CoreClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            http: reqwest::Client::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    fn events_url(&self) -> String {
        self.base_url
            .replacen("http://", "ws://", 1)
            .replacen("https://", "wss://", 1)
            + "/api/events"
    }

    /// Run an action; the reason when lmix-core refused it.
    pub async fn act(&self, action: &Action) -> Option<String> {
        let resp = match self.http.post(format!("{}/api/actions", self.base_url)).json(action).send().await {
            Ok(resp) => resp,
            Err(e) => return Some(e.to_string()),
        };
        if resp.status().is_success() {
            return None;
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Some(refusal(status.as_u16(), &body))
    }

    /// Latest picture; `None` while lmix-core has none yet.
    pub async fn picture(&self, target: PictureTarget) -> Option<Vec<u8>> {
        let path = match target {
            PictureTarget::Program => "program".to_string(),
            PictureTarget::Input(id) => format!("inputs/{id}"),
        };
        let resp = self.http.get(format!("{}/api/pictures/{path}", self.base_url)).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        resp.bytes().await.ok().map(|b| b.to_vec())
    }

    /// Follow the event stream forever, reconnecting when it drops.
    pub fn follow(&self, tx: Sender<CoreMessage>, ctx: egui::Context) {
        let url = self.events_url();
        tokio::spawn(async move {
            loop {
                if let Ok((mut stream, _)) = connect_async(&url).await {
                    let _ = tx.send(CoreMessage::Connected(true));
                    while let Some(Ok(message)) = stream.next().await {
                        let Message::Text(text) = message else {
                            continue;
                        };
                        if let Ok(Event::State { state }) = serde_json::from_str(&text) {
                            let _ = tx.send(CoreMessage::State(state));
                            ctx.request_repaint();
                        }
                    }
                }
                let _ = tx.send(CoreMessage::Connected(false));
                ctx.request_repaint();
                tokio::time::sleep(RECONNECT).await;
            }
        });
    }
}

/// Readable reason from an error answer (`{"message": …}`).
fn refusal(status: u16, body: &str) -> String {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_string));
    match message {
        Some(message) => message,
        None => format!("lmix-core {status}: {body}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_prefers_the_message() {
        let body = r#"{"error":{"error":"no_preview"},"message":"nothing in preview"}"#;

        assert_eq!(refusal(409, body), "nothing in preview");
        assert_eq!(refusal(502, "down"), "lmix-core 502: down");
    }

    #[test]
    fn events_url_follows_the_scheme() {
        assert_eq!(CoreClient::new("http://h:8200/".into()).events_url(), "ws://h:8200/api/events");
        assert_eq!(CoreClient::new("https://h".into()).events_url(), "wss://h/api/events");
    }
}
