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
//! | `SUB <topic>` | `OK`, then a line per change; topics `TALLY`, `STATE`, `LEVELS`, `OUTPUTS`, `MIDI`, `TITLE`, `POSITION`, `CUE`, `TARGET`, `MACHINE`, `ALL` |
//! | `UNSUB <topic>` | `OK` |
//! | `QUIT` | closes |
//! | `AUTH <password>` | `OK`; first line from the network when lmix-core has `--password` or `--access-keys` (API.7, CTL.5); a key's right limits the actions after it |
//!
//! Subscribed changes arrive as `TALLY <digits>`, `STATE <json>`,
//! `LEVELS <json>`, `OUTPUTS <json>`, `MIDI <json>` and `TITLE <json>` (a
//! template's report) lines. A slow client misses
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

use crate::access::Right;
use crate::service::Service;

/// Protocol version in the greeting.
const VERSION: u32 = 1;

/// How long a client from the network has to send `AUTH`.
const AUTH_WAIT: std::time::Duration = std::time::Duration::from_secs(10);

/// Longest line a client may send.
const MAX_LINE: u64 = 64 * 1024;

/// Topics a client can subscribe to, as bits.
#[derive(Clone, Copy, Default)]
struct Topics(u16);

impl Topics {
    const TALLY: u16 = 1;
    const STATE: u16 = 2;
    const LEVELS: u16 = 4;
    const OUTPUTS: u16 = 8;
    const MIDI: u16 = 16;
    const TITLE: u16 = 32;
    const POSITION: u16 = 64;
    const CUE: u16 = 128;
    const TARGET: u16 = 256;
    const MACHINE: u16 = 512;
    const ALL: u16 = Self::TALLY
        | Self::STATE
        | Self::LEVELS
        | Self::OUTPUTS
        | Self::MIDI
        | Self::TITLE
        | Self::POSITION
        | Self::CUE
        | Self::TARGET
        | Self::MACHINE;

    fn parse(name: &str) -> Option<u16> {
        match name.to_ascii_uppercase().as_str() {
            "TALLY" => Some(Self::TALLY),
            "STATE" => Some(Self::STATE),
            "LEVELS" => Some(Self::LEVELS),
            "OUTPUTS" => Some(Self::OUTPUTS),
            "MIDI" => Some(Self::MIDI),
            "TITLE" => Some(Self::TITLE),
            "POSITION" => Some(Self::POSITION),
            "CUE" => Some(Self::CUE),
            "TARGET" => Some(Self::TARGET),
            "MACHINE" => Some(Self::MACHINE),
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
            Event::Midi { .. } => Self::MIDI,
            Event::Title { .. } => Self::TITLE,
            Event::Position { .. } => Self::POSITION,
            Event::Cue { .. } => Self::CUE,
            Event::Target { .. } => Self::TARGET,
            Event::Panic { .. } => Self::CUE,
            Event::Running { .. } => Self::CUE,
            Event::Machine { .. } => Self::MACHINE,
        };
        self.0 & bit != 0
    }
}

/// Accept clients until the process ends.
pub async fn serve(
    listen: SocketAddr,
    service: Arc<Service>,
    access: Arc<crate::access::Access>,
) -> anyhow::Result<()> {
    let listener = TcpListener::bind(listen).await?;
    tracing::info!(%listen, "text protocol up");
    loop {
        let (mut stream, peer) = listener.accept().await?;
        let service = service.clone();
        let access = access.clone();
        tokio::spawn(async move {
            // A trusted address has its right at once, no `AUTH` (CTL.5).
            let (password, right) = match access.peer(peer.ip()) {
                crate::access::Peer::Local => (None, Right::Edit),
                crate::access::Peer::Network if !access.needs_key() => (None, Right::Edit),
                crate::access::Peer::Network => match access.trusted(peer.ip()) {
                    Some(right) => (None, right),
                    None => (Some(access), Right::View),
                },
                crate::access::Peer::Refused => {
                    let _ = stream
                        .write_all(b"ER forbidden lmix answers only on the local network\n")
                        .await;
                    return;
                }
            };
            if let Err(error) = client(stream, service, password, right, peer.ip()).await {
                tracing::debug!(%peer, %error, "text client gone");
            }
        });
    }
}

/// One client; `password` holds the rules while it still has to send
/// `AUTH`; `right`: what it may do until then (CTL.5).
async fn client(
    stream: TcpStream,
    service: Arc<Service>,
    mut password: Option<Arc<crate::access::Access>>,
    mut right: Right,
    ip: std::net::IpAddr,
) -> std::io::Result<()> {
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

    // The password comes first and soon; a silent connection is closed.
    let auth_deadline = tokio::time::sleep(AUTH_WAIT);
    tokio::pin!(auth_deadline);
    loop {
        tokio::select! {
            () = &mut auth_deadline, if password.is_some() => {
                write.write_all(b"ER auth password needed (AUTH <password>)\n").await?;
                return Ok(());
            }
            line = lines.recv() => {
                let line = match line {
                    Some(Line::Text(line)) => line,
                    Some(Line::TooLong) => {
                        write.write_all(b"ER line_too_long\n").await?;
                        return Ok(());
                    }
                    None => return Ok(()),
                };
                if let Some(access) = &password {
                    // Nothing before the password but an open panic; a
                    // wrong one ends it.
                    let text = line.trim_end_matches(['\r', '\n']);
                    if let Ok(action) = serde_json::from_str::<Action>(text) {
                        if access.open_for(&action) {
                            if let Some(reply) = answer(text, &service, &mut topics).await {
                                write.write_all(reply.as_bytes()).await?;
                            }
                            continue;
                        }
                    }
                    match text.strip_prefix("AUTH ").map(|given| access.right_of_key(given)) {
                        Some(Some(granted)) => {
                            access.forgive(ip);
                            right = granted;
                            password = None;
                            write.write_all(b"OK\n").await?;
                            continue;
                        }
                        Some(None) => {
                            access.punish(ip, "text protocol").await;
                            write.write_all(b"ER auth password needed (AUTH <password>)\n").await?;
                            return Ok(());
                        }
                        None => {
                            write.write_all(b"ER auth password needed (AUTH <password>)\n").await?;
                            return Ok(());
                        }
                    }
                }
                if let Ok(action) = serde_json::from_str::<Action>(line.trim()) {
                    let needs = crate::access::right_of(&action);
                    if needs > right {
                        write
                            .write_all(format!("ER forbidden this key may not run it (it needs {needs:?})\n").as_bytes())
                            .await?;
                        continue;
                    }
                }
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
        ("SUB" | "UNSUB", _) => "ER bad_topic TALLY, STATE, LEVELS, OUTPUTS, MIDI or ALL\n".into(),
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
        Event::Midi { midi } => format!("MIDI {}\n", json(midi)),
        Event::Cue { .. } => format!("CUE {}\n", json(event)),
        Event::Target { .. } => format!("TARGET {}\n", json(event)),
        Event::Panic { .. } => format!("PANIC {}\n", json(event)),
        Event::Running { cues } => format!("RUNNING {}\n", json(cues)),
        Event::Title { .. } => format!("TITLE {}\n", json(event)),
        Event::Position { .. } => format!("POSITION {}\n", json(event)),
        Event::Machine { load } => format!("MACHINE {}\n", json(load)),
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
        connect_as(None).await
    }

    /// A client that still owes the password of `access`, if any.
    async fn connect_as(
        access: Option<Arc<crate::access::Access>>,
    ) -> (
        tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
        tokio::net::tcp::OwnedWriteHalf,
    ) {
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let right = if access.is_some() {
                Right::View
            } else {
                Right::Edit
            };
            client(
                stream,
                service,
                access,
                right,
                std::net::IpAddr::from([192, 168, 1, 9]),
            )
            .await
            .unwrap();
        });
        let (read, write) = TcpStream::connect(addr).await.unwrap().into_split();
        (BufReader::new(read).lines(), write)
    }

    #[tokio::test]
    async fn the_network_says_the_password_first() {
        let access = Arc::new(crate::access::Access {
            password: Some("p".into()),
            ..Default::default()
        });
        let (mut lines, mut write) = connect_as(Some(access.clone())).await;
        assert_eq!(lines.next_line().await.unwrap().unwrap(), "LMIX 1");
        write.write_all(b"AUTH p\nTALLY\n").await.unwrap();
        assert_eq!(lines.next_line().await.unwrap().unwrap(), "OK");
        assert_eq!(lines.next_line().await.unwrap().unwrap(), "TALLY ");

        let (mut lines, mut write) = connect_as(Some(access)).await;
        lines.next_line().await.unwrap();
        write.write_all(b"TALLY\n").await.unwrap();
        assert!(lines
            .next_line()
            .await
            .unwrap()
            .unwrap()
            .starts_with("ER auth"));
        assert_eq!(lines.next_line().await.unwrap(), None, "closed");
    }

    #[tokio::test]
    async fn a_control_key_runs_the_show_but_does_not_edit() {
        let access = Arc::new(crate::access::Access {
            keys: vec![crate::access::AccessKey {
                name: "Deck".into(),
                key: "steuern-0123456".into(),
                right: crate::access::Right::Control,
            }],
            open_panic: true,
            ..Default::default()
        });
        let (mut lines, mut write) = connect_as(Some(access.clone())).await;
        let mut next = async || lines.next_line().await.unwrap().unwrap();
        assert_eq!(next().await, "LMIX 1");
        // Panic before the key: open.
        write
            .write_all(b"{\"action\":\"cue_panic\"}\n")
            .await
            .unwrap();
        assert_eq!(next().await, "OK");
        write.write_all(b"AUTH steuern-0123456\n").await.unwrap();
        assert_eq!(next().await, "OK");
        write
            .write_all(b"{\"action\":\"fade_to_black\",\"duration_ms\":0}\n")
            .await
            .unwrap();
        assert_eq!(next().await, "OK");
        write
            .write_all(b"{\"action\":\"add_input\",\"source\":{\"kind\":\"pattern\",\"pattern\":\"bars\"}}\n")
            .await
            .unwrap();
        assert!(next().await.starts_with("ER forbidden"));
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

    #[tokio::test]
    async fn a_trusted_address_skips_auth() {
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let ip = std::net::IpAddr::from([192, 168, 1, 20]);
            client(stream, service, None, Right::Control, ip)
                .await
                .unwrap();
        });
        let (read, mut write) = TcpStream::connect(addr).await.unwrap().into_split();
        let mut lines = BufReader::new(read).lines();
        assert_eq!(lines.next_line().await.unwrap().unwrap(), "LMIX 1");

        write
            .write_all(b"{\"action\":\"set_lock\",\"on\":true}\nTALLY\n")
            .await
            .unwrap();

        let refused = lines.next_line().await.unwrap().unwrap();
        assert!(refused.starts_with("ER forbidden"), "{refused}");
        assert_eq!(lines.next_line().await.unwrap().unwrap(), "TALLY ");
    }

    #[test]
    fn the_machine_topic_carries_the_load() {
        let event = Event::Machine {
            load: lmix_types::MachineLoad {
                cpu_percent: 91,
                load_per_core: 1.5,
                cores: 4,
                cue_timer: lmix_types::TimerPriority::Normal,
                cues_late: false,
                high: true,
            },
        };
        let machine = Topics(Topics::parse("machine").unwrap());
        assert!(machine.wants(&event));
        assert!(Topics(Topics::ALL).wants(&event));
        assert!(!Topics(Topics::parse("CUE").unwrap()).wants(&event));
        let line = event_line(&event);
        assert!(line.starts_with("MACHINE {"), "{line}");
        assert!(line.contains(r#""cue_timer":"normal""#), "{line}");
    }
}
