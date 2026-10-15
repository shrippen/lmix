//! OSC over UDP (AUT.13): actions in, tally out.
//!
//! | Address | Arguments | Action |
//! |---|---|---|
//! | `/lmix/action` | JSON string | any action |
//! | `/lmix/cut` | – | `cut` |
//! | `/lmix/preview` | input (int or string) | `preview_input` |
//! | `/lmix/program` | input | `cut_direct` |
//! | `/lmix/transition` | slot (int) | `transition` |
//! | `/lmix/ftb` | – | `fade_to_black` |
//! | `/lmix/tbar` | position (float 0–1) | `t_bar` |
//! | `/lmix/overlay/{n}/in` | input | `overlay_in` |
//! | `/lmix/overlay/{n}/out` | – | `overlay_out` |
//! | `/lmix/cue/go`, `/lmix/cue/back` | list | `cue_go`, `cue_back` |
//! | `/lmix/subscribe` | – | the sender gets tally from now on |
//! | `/lmix/unsubscribe` | – | stops it |
//!
//! Refused actions answer the sender with `/lmix/error s:reason`.
//! Subscribers get `/lmix/tally s:digits` (0 off, 1 program, 2 preview per
//! input) on every change and right after subscribing. Bundles are
//! unpacked; their time tags are ignored (run now).

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use lmix_types::{Action, Event, InputRef};
use tokio::net::UdpSocket;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::Mutex;

use crate::service::Service;

/// Largest datagram read.
const MAX_PACKET: usize = 64 * 1024;

/// Subscribers kept at most; the oldest is dropped beyond that.
const MAX_SUBSCRIBERS: usize = 32;

/// One OSC argument.
#[derive(Clone, Debug, PartialEq)]
pub enum Arg {
    Int(i32),
    Float(f32),
    Str(String),
}

#[derive(Debug, PartialEq)]
pub struct Message {
    pub address: String,
    pub args: Vec<Arg>,
}

/// Messages of a packet (a message or a bundle, nested bundles too).
pub fn decode(packet: &[u8]) -> Vec<Message> {
    let mut out = Vec::new();
    decode_into(packet, &mut out, 0);
    out
}

/// Bundles nest at most this deep.
const MAX_DEPTH: usize = 8;

fn decode_into(packet: &[u8], out: &mut Vec<Message>, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    if packet.starts_with(b"#bundle\0") {
        // Tag (8) and time tag (8), then size-prefixed elements.
        let mut at = 16;
        while at + 4 <= packet.len() {
            let size = u32::from_be_bytes(packet[at..at + 4].try_into().unwrap()) as usize;
            at += 4;
            let Some(element) = packet.get(at..at + size) else {
                return;
            };
            decode_into(element, out, depth + 1);
            at += size;
        }
        return;
    }
    if let Some(message) = decode_message(packet) {
        out.push(message);
    }
}

fn decode_message(packet: &[u8]) -> Option<Message> {
    let mut at = 0;
    let address = read_string(packet, &mut at)?;
    if !address.starts_with('/') {
        return None;
    }
    let mut args = Vec::new();
    if at >= packet.len() {
        return Some(Message { address, args });
    }
    let tags = read_string(packet, &mut at)?;
    for tag in tags.strip_prefix(',')?.chars() {
        let arg = match tag {
            'i' => Arg::Int(i32::from_be_bytes(read4(packet, &mut at)?)),
            'f' => Arg::Float(f32::from_be_bytes(read4(packet, &mut at)?)),
            's' | 'S' => Arg::Str(read_string(packet, &mut at)?),
            'T' => Arg::Int(1),
            'F' => Arg::Int(0),
            'N' | 'I' => continue,
            // Unknown sizes: stop here, keep what was read.
            _ => break,
        };
        args.push(arg);
    }
    Some(Message { address, args })
}

fn read4(packet: &[u8], at: &mut usize) -> Option<[u8; 4]> {
    let bytes = packet.get(*at..*at + 4)?.try_into().ok()?;
    *at += 4;
    Some(bytes)
}

/// A NUL-terminated string padded to four bytes.
fn read_string(packet: &[u8], at: &mut usize) -> Option<String> {
    let rest = packet.get(*at..)?;
    let end = rest.iter().position(|b| *b == 0)?;
    let text = std::str::from_utf8(&rest[..end]).ok()?.to_string();
    *at += (end + 4) & !3;
    Some(text)
}

/// A message with string arguments only.
pub fn encode(address: &str, args: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    write_string(&mut out, address);
    write_string(&mut out, &format!(",{}", "s".repeat(args.len())));
    for arg in args {
        write_string(&mut out, arg);
    }
    out
}

fn write_string(out: &mut Vec<u8>, text: &str) {
    out.extend_from_slice(text.as_bytes());
    let padded = (text.len() + 4) & !3;
    out.resize(out.len() + padded - text.len(), 0);
}

/// What a message asks for.
#[derive(Debug, PartialEq)]
pub enum Request {
    Action(Action),
    Subscribe,
    Unsubscribe,
}

/// The request of a message, or why it makes none.
pub fn request(message: &Message) -> Result<Request, String> {
    let address = message
        .address
        .strip_prefix("/lmix")
        .unwrap_or(&message.address);
    let parts: Vec<&str> = address.trim_start_matches('/').split('/').collect();
    let arg = |i: usize| message.args.get(i);
    let input = |i: usize| -> Result<InputRef, String> {
        match arg(i) {
            Some(Arg::Int(n)) if *n > 0 => Ok(InputRef::Number(*n as u32)),
            Some(Arg::Float(n)) if *n >= 1.0 => Ok(InputRef::Number(*n as u32)),
            Some(Arg::Str(s)) => Ok(InputRef::Text(s.clone())),
            _ => Err(format!(
                "{} needs an input (number or name)",
                message.address
            )),
        }
    };
    let json = |value: serde_json::Value| {
        serde_json::from_value::<Action>(value).map_err(|e| e.to_string())
    };
    let action = match parts.as_slice() {
        ["subscribe"] => return Ok(Request::Subscribe),
        ["unsubscribe"] => return Ok(Request::Unsubscribe),
        ["action"] => match arg(0) {
            Some(Arg::Str(text)) => serde_json::from_str(text).map_err(|e| e.to_string())?,
            _ => return Err("/lmix/action needs a JSON string".into()),
        },
        ["cut"] => Action::Cut,
        ["ftb"] => json(serde_json::json!({ "action": "fade_to_black" }))?,
        ["preview"] => Action::PreviewInput { input: input(0)? },
        ["program"] => Action::CutDirect { input: input(0)? },
        ["transition"] => match arg(0) {
            Some(Arg::Int(slot)) => {
                json(serde_json::json!({ "action": "transition", "slot": slot }))?
            }
            Some(Arg::Float(slot)) => {
                json(serde_json::json!({ "action": "transition", "slot": *slot as i64 }))?
            }
            _ => return Err("/lmix/transition needs a slot number".into()),
        },
        ["tbar"] => match arg(0) {
            Some(Arg::Float(position)) => Action::TBar {
                position: *position,
            },
            Some(Arg::Int(position)) => Action::TBar {
                position: *position as f32,
            },
            _ => return Err("/lmix/tbar needs a position 0–1".into()),
        },
        ["overlay", channel, "in"] => {
            let channel = channel.parse::<u8>().map_err(|_| "overlay channel 1–4")?;
            let input = match input(0)? {
                InputRef::Number(n) => serde_json::json!(n),
                InputRef::Text(s) => serde_json::json!(s),
            };
            json(serde_json::json!({ "action": "overlay_in", "channel": channel, "input": input }))?
        }
        ["overlay", channel, "out"] => {
            let channel = channel.parse::<u8>().map_err(|_| "overlay channel 1–4")?;
            json(serde_json::json!({ "action": "overlay_out", "channel": channel }))?
        }
        ["cue", step @ ("go" | "back")] => {
            let list = input(0)?;
            if *step == "go" {
                Action::CueGo { list }
            } else {
                Action::CueBack { list }
            }
        }
        _ => return Err(format!("unknown address {}", message.address)),
    };
    Ok(Request::Action(action))
}

/// Receive messages and send tally until the process ends.
pub async fn serve(listen: SocketAddr, service: Arc<Service>) -> anyhow::Result<()> {
    let socket = Arc::new(UdpSocket::bind(listen).await?);
    tracing::info!(%listen, "OSC up");
    let subscribers = Arc::new(Mutex::new(Vec::<SocketAddr>::new()));
    tokio::spawn(send_tally(
        socket.clone(),
        subscribers.clone(),
        service.clone(),
    ));

    let mut buf = vec![0u8; MAX_PACKET];
    loop {
        let (len, peer) = socket.recv_from(&mut buf).await?;
        for message in decode(&buf[..len]) {
            match request(&message) {
                Ok(Request::Action(action)) => {
                    if let Err(error) = service.handle(action).await {
                        let reply = encode("/lmix/error", &[&error.to_string()]);
                        let _ = socket.send_to(&reply, peer).await;
                    }
                }
                Ok(Request::Subscribe) => {
                    let mut subs = subscribers.lock().await;
                    if !subs.contains(&peer) {
                        if subs.len() >= MAX_SUBSCRIBERS {
                            subs.remove(0);
                        }
                        subs.push(peer);
                    }
                    drop(subs);
                    let tally = tally_message(&service.tally().await);
                    let _ = socket.send_to(&tally, peer).await;
                }
                Ok(Request::Unsubscribe) => subscribers.lock().await.retain(|s| *s != peer),
                Err(reason) => {
                    let _ = socket
                        .send_to(&encode("/lmix/error", &[&reason]), peer)
                        .await;
                }
            }
        }
    }
}

fn tally_message(tally: &[u8]) -> Vec<u8> {
    let digits: String = tally.iter().map(|t| char::from(b'0' + t.min(&9))).collect();
    encode("/lmix/tally", &[&digits])
}

async fn send_tally(
    socket: Arc<UdpSocket>,
    subscribers: Arc<Mutex<Vec<SocketAddr>>>,
    service: Arc<Service>,
) {
    let mut events = service.subscribe();
    let mut last: Option<Vec<u8>> = None;
    loop {
        let tally = match events.recv().await {
            Ok(Event::Tally { tally }) => tally,
            Ok(_) => continue,
            Err(RecvError::Lagged(_)) => service.tally().await,
            Err(RecvError::Closed) => return,
        };
        if last.as_ref() == Some(&tally) {
            continue;
        }
        let message = tally_message(&tally);
        last = Some(tally);
        let subs: HashSet<SocketAddr> = subscribers.lock().await.iter().copied().collect();
        for peer in subs {
            let _ = socket.send_to(&message, peer).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn message(address: &str, args: &[Arg]) -> Vec<u8> {
        let mut out = Vec::new();
        write_string(&mut out, address);
        let tags: String = args
            .iter()
            .map(|a| match a {
                Arg::Int(_) => 'i',
                Arg::Float(_) => 'f',
                Arg::Str(_) => 's',
            })
            .collect();
        write_string(&mut out, &format!(",{tags}"));
        for arg in args {
            match arg {
                Arg::Int(i) => out.extend_from_slice(&i.to_be_bytes()),
                Arg::Float(f) => out.extend_from_slice(&f.to_be_bytes()),
                Arg::Str(s) => write_string(&mut out, s),
            }
        }
        out
    }

    #[test]
    fn decodes_messages_and_bundles() {
        let one = message("/lmix/preview", &[Arg::Int(2)]);
        let two = message("/lmix/tbar", &[Arg::Float(0.5)]);
        let mut bundle = b"#bundle\0".to_vec();
        bundle.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
        for m in [&one, &two] {
            bundle.extend_from_slice(&(m.len() as u32).to_be_bytes());
            bundle.extend_from_slice(m);
        }
        assert_eq!(
            decode(&bundle),
            [
                Message {
                    address: "/lmix/preview".into(),
                    args: vec![Arg::Int(2)]
                },
                Message {
                    address: "/lmix/tbar".into(),
                    args: vec![Arg::Float(0.5)]
                },
            ]
        );
        // Truncated packets give nothing rather than garbage.
        assert!(decode(&one[..one.len() - 2]).is_empty());
    }

    #[test]
    fn maps_addresses_to_actions() {
        let req = |address: &str, args: Vec<Arg>| {
            request(&Message {
                address: address.into(),
                args,
            })
        };
        assert_eq!(req("/lmix/cut", vec![]), Ok(Request::Action(Action::Cut)));
        assert_eq!(
            req("/lmix/program", vec![Arg::Str("Cam".into())]),
            Ok(Request::Action(Action::CutDirect {
                input: InputRef::Text("Cam".into())
            }))
        );
        assert!(matches!(
            req("/lmix/overlay/2/in", vec![Arg::Int(3)]),
            Ok(Request::Action(Action::OverlayIn {
                channel: 2,
                input: InputRef::Number(3),
                ..
            }))
        ));
        assert!(matches!(
            req(
                "/lmix/action",
                vec![Arg::Str(r#"{"action":"fade_to_black"}"#.into())]
            ),
            Ok(Request::Action(Action::FadeToBlack { .. }))
        ));
        assert_eq!(req("/lmix/subscribe", vec![]), Ok(Request::Subscribe));
        assert!(req("/lmix/preview", vec![]).is_err());
        assert!(req("/lmix/frob", vec![]).is_err());
    }

    #[test]
    fn encodes_padded_strings() {
        let packet = encode("/lmix/tally", &["012"]);
        assert_eq!(packet.len() % 4, 0);
        assert_eq!(
            decode(&packet),
            [Message {
                address: "/lmix/tally".into(),
                args: vec![Arg::Str("012".into())]
            }]
        );
    }
}
