//! WebSocket client for the strom event stream. Native-only (this crate
//! never targets WASM), trimmed from the reconnect-loop pattern in
//! workspace/strom/frontend/src/ws.rs.

use futures_util::stream::StreamExt;
use std::sync::mpsc::Sender;
use std::sync::Arc;
use strom_types::StromEvent;
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use crate::perf::Perf;
use crate::state::AppMessage;

pub fn connect(
    url: String,
    tx: Sender<(u64, AppMessage)>,
    ctx: egui::Context,
    session: u64,
    perf: Arc<Perf>,
) {
    tokio::spawn(async move {
        connection_loop(url, tx, ctx, session, perf).await;
    });
}

async fn connection_loop(
    url: String,
    tx: Sender<(u64, AppMessage)>,
    ctx: egui::Context,
    session: u64,
    perf: Arc<Perf>,
) {
    let mut attempt: u32 = 1;

    loop {
        tracing::info!("WebSocket connection attempt {attempt} to: {url}");

        match connect_async(&url).await {
            Ok((mut ws_stream, _)) => {
                tracing::info!("WebSocket connected");
                attempt = 1;

                while let Some(msg_result) = ws_stream.next().await {
                    match msg_result {
                        Ok(Message::Text(text)) => match serde_json::from_str::<StromEvent>(&text)
                        {
                            Ok(event) => {
                                perf.ws_event();
                                let _ = tx.send((session, AppMessage::Event(event)));
                                ctx.request_repaint();
                            }
                            Err(err) => {
                                tracing::warn!("Failed to parse WS event: {err}");
                            }
                        },
                        Ok(Message::Close(_)) => {
                            tracing::info!("WebSocket closed by server");
                            break;
                        }
                        Ok(_) => {}
                        Err(e) => {
                            tracing::warn!("WebSocket error: {e:?}");
                            break;
                        }
                    }
                }
                tracing::warn!("WebSocket connection lost, reconnecting...");
            }
            Err(e) => {
                tracing::warn!("Failed to connect WebSocket: {e:?}");
            }
        }

        let delay_ms = (1000u64 * 2u64.pow(attempt.min(4) - 1)).min(10_000);
        sleep(Duration::from_millis(delay_ms)).await;
        attempt += 1;
    }
}
