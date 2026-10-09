//! Text lines over TCP (API.5): small enough for a microcontroller.
//!
//! Every line ends with `\n` (`\r\n` is fine). The server greets with
//! `LMIX 1`. Client lines:
//!
//! | Line | Answer |
//! |---|---|
//! | `{"action": …}` | `OK` or `ER <error> <message>` |
//! | `TALLY` | `TALLY <digits>`: per input 0 off, 1 program, 2 preview |
//! | `STATE` | `STATE <json>` |
//! | `SUB <topic>` | `OK`, then a line per change; topics `TALLY`, `STATE`, `LEVELS`, `OUTPUTS`, `ALL` |
//! | `UNSUB <topic>` | `OK` |
//! | `QUIT` | closes |
//!
//! Subscribed changes arrive as `TALLY <digits>`, `STATE <json>`,
//! `LEVELS <json>` and `OUTPUTS <json>` lines. A slow client misses
//! intermediate changes but never a final one: after falling behind it
//! gets the current tally and state again.

use std::net::SocketAddr;
use std::sync::Arc;

use lmix_types::{Action, Event};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;

use crate::service::Service;

/// Protocol version in the greeting.
const VERSION: u32 = 1;

/// Longest line a client may send.
const MAX_LINE: u64 = 64 * 1024;

/// Topics a client can subscribe to, as bits.
#[derive(Clone, Copy, Default)]
struct Topics(u8);

impl Topics {
    const TALLY: u8 = 1;
    const STATE: u8 = 2;
    const LEVELS: u8 = 4;
    const OUTPUTS: u8 = 8;
    const ALL: u8 = Self::TALLY | Self::STATE | Self::LEVELS | Self::OUTPUTS;

    fn parse(name: &str) -> Option<u8> {
        match name.to_ascii_uppercase().as_str() {
            "TALLY" => Some(Self::TALLY),
            "STATE" => Some(Self::STATE),
            "LEVELS" => Some(Self::LEVELS),
            "OUTPUTS" => Some(Self::OUTPUTS),
            "ALL" => Some(Self::ALL),
            _ => None,
        }
    }

    fn wants(self, event: &Event) -> bool {
        let bit = match event {
            Event::Tally { .. } => Self::TALLY,
            Event::State { .. } => Self::STATE,
            Event::Levels { .. } => Self::LEVELS,
            Event::Outputs { .. } => Self::OUTPUTS,
        };
        self.0 & bit != 0
    }
}

/// Accept clients until the process ends.
pub async fn serve(listen: SocketAddr, service: Arc<Service>) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen).await?;
    tracing::info!(%listen, "text protocol up");
    loop {
        let (stream, peer) = listener.accept().await?;
        let service = service.clone();
        tokio::spawn(async move {
            if let Err(error) = client(stream, service).await {
                tracing::debug!(%peer, %error, "text client gone");
            }
        });
    }
}

async fn client(stream: TcpStream, service: Arc<Service>) -> std::io::Result<()> {
    let (read, mut write) = stream.into_split();
    let mut events = service.subscribe();
    let mut topics = Topics::default();
    write
        .write_all(format!("LMIX {VERSION}\n").as_bytes())
        .await?;

    // Lines come through a channel: reading a line is not cancel safe in
    // `select!`, receiving is.
    let (tx, mut lines) = mpsc::channel::<Line>(LINES_QUEUED);
    tokio::spawn(read_lines(read, tx));

    loop {
        tokio::select! {
            line = lines.recv() => {
                let line = match line {
                    Some(Line::Text(line)) => line,
                    Some(Line::TooLong) => {
                        write.write_all(b"ER line_too_long\n").await?;
                        return Ok(());
                    }
                    None => return Ok(()),
                };
                match answer(line.trim(), &service, &mut topics).await {
                    Some(reply) => write.write_all(reply.as_bytes()).await?,
                    None => return Ok(()),
                }
            }
            event = events.recv() => {
                let text = match event {
                    Ok(event) if topics.wants(&event) => event_line(&event),
                    Ok(_) => continue,
                    // Fell behind: send what is true now.
                    Err(RecvError::Lagged(_)) => catch_up(&service, topics).await,
                    Err(RecvError::Closed) => return Ok(()),
                };
                write.write_all(text.as_bytes()).await?;
            }
        }
    }
}

/// Lines read ahead of their answers.
const LINES_QUEUED: usize = 16;

enum Line {
    Text(String),
    TooLong,
}

/// Feeds the client's lines into `tx` until it closes or a line is too long.
async fn read_lines(read: OwnedReadHalf, tx: mpsc::Sender<Line>) {
    let mut read = BufReader::new(read);
    loop {
        let mut line = String::new();
        let line = match (&mut read).take(MAX_LINE).read_line(&mut line).await {
            Ok(0) | Err(_) => return,
            Ok(_) if !line.ends_with('\n') && line.len() as u64 >= MAX_LINE => Line::TooLong,
            Ok(_) => Line::Text(line),
        };
        let stop = matches!(line, Line::TooLong);
        if tx.send(line).await.is_err() || stop {
            return;
        }
    }
}

/// The answer to one client line, newline included; `None` closes.
async fn answer(line: &str, service: &Arc<Service>, topics: &mut Topics) -> Option<String> {
    if line.starts_with('{') {
        let action = match serde_json::from_str::<Action>(line) {
            Ok(action) => action,
            Err(error) => return Some(format!("ER bad_action {}\n", one_line(&error.to_string()))),
        };
        return Some(match service.handle(action).await {
            Ok(_) => "OK\n".to_string(),
            Err(error) => {
                let code = serde_json::to_value(&error)
                    .ok()
                    .and_then(|v| v["error"].as_str().map(str::to_string))
                    .unwrap_or_else(|| "refused".into());
                format!("ER {code} {}\n", one_line(&error.to_string()))
            }
        });
    }

    let mut words = line.split_whitespace();
    let verb = words.next().unwrap_or("").to_ascii_uppercase();
    let topic = words.next().map(Topics::parse);
    Some(match (verb.as_str(), topic) {
        ("", _) => return Some(String::new()),
        ("QUIT", _) => return None,
        ("TALLY", None) => tally_line(&service.tally().await),
        ("STATE", None) => event_line(&Event::State {
            state: service.state().await,
        }),
        ("SUB", Some(Some(bit))) => {
            topics.0 |= bit;
            "OK\n".into()
        }
        ("UNSUB", Some(Some(bit))) => {
            topics.0 &= !bit;
            "OK\n".into()
        }
        ("SUB" | "UNSUB", _) => "ER bad_topic TALLY, STATE, LEVELS, OUTPUTS or ALL\n".into(),
        _ => "ER unknown_command JSON action, TALLY, STATE, SUB, UNSUB or QUIT\n".into(),
    })
}

/// Current tally and state for the topics, after missed events.
async fn catch_up(service: &Service, topics: Topics) -> String {
    let mut text = String::new();
    if topics.0 & Topics::TALLY != 0 {
        text += &tally_line(&service.tally().await);
    }
    if topics.0 & Topics::STATE != 0 {
        text += &event_line(&Event::State {
            state: service.state().await,
        });
    }
    text
}

fn tally_line(tally: &[u8]) -> String {
    let digits: String = tally.iter().map(|t| char::from(b'0' + t.min(&9))).collect();
    format!("TALLY {digits}\n")
}

fn event_line(event: &Event) -> String {
    match event {
        Event::Tally { tally } => tally_line(tally),
        Event::State { state } => format!("STATE {}\n", json(state)),
        Event::Levels { levels } => format!("LEVELS {}\n", json(levels)),
        Event::Outputs { stats } => format!("OUTPUTS {}\n", json(stats)),
    }
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("event serializes")
}

/// Messages keep to their line.
fn one_line(text: &str) -> String {
    text.replace(['\r', '\n'], " ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::null::NullEngine;
    use tokio::io::AsyncBufReadExt;

    async fn connect() -> (
        tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
        tokio::net::tcp::OwnedWriteHalf,
    ) {
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            client(stream, service).await.unwrap();
        });
        let (read, write) = TcpStream::connect(addr).await.unwrap().into_split();
        (BufReader::new(read).lines(), write)
    }

    #[tokio::test]
    async fn actions_tally_and_subscriptions() {
        let (mut lines, mut write) = connect().await;
        let mut next = async || lines.next_line().await.unwrap().unwrap();
        assert_eq!(next().await, "LMIX 1");

        write.write_all(b"SUB tally\r\n").await.unwrap();
        assert_eq!(next().await, "OK");
        write
            .write_all(b"{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"bars\"},\"name\":\"Cam\"}\n")
            .await
            .unwrap();
        // The tally event and the answer, in either order.
        let mut got = [next().await, next().await];
        got.sort();
        assert_eq!(got, ["OK", "TALLY 0"]);

        write
            .write_all(b"{\"action\":\"cut_direct\",\"input\":\"Cam\"}\n")
            .await
            .unwrap();
        let mut got = [next().await, next().await];
        got.sort();
        assert_eq!(got, ["OK", "TALLY 1"]);

        write.write_all(b"TALLY\n").await.unwrap();
        assert_eq!(next().await, "TALLY 1");

        write
            .write_all(b"{\"action\":\"preview_input\",\"input\":9}\n")
            .await
            .unwrap();
        assert_eq!(next().await, "ER unknown_input no input matches 9");
        write.write_all(b"FROB\n").await.unwrap();
        assert!(next().await.starts_with("ER unknown_command"));
        write.write_all(b"STATE\n").await.unwrap();
        assert!(next().await.starts_with("STATE {"));

        write.write_all(b"QUIT\n").await.unwrap();
        assert_eq!(lines.next_line().await.unwrap(), None);
    }
}
