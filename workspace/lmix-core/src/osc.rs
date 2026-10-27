//! OSC over UDP (AUT.13): actions in, tally out.
//!
//! | Address | Arguments | Action |
//! |---|---|---|
//! | `/lmix/action` | JSON string | any action |
//! | `/lmix/cut` | – | `cut` |
//! | `/lmix/preview` | input (int or string) | `preview_input` |
//! | `/lmix/program` | input | `cut_direct` |
//! | `/lmix/transition` | slot (int) | `transition` |
//! | `/lmix/quickplay` | input, or none for preview | `quick_play` |
//! | `/lmix/go` | input | `go_input` |
//! | `/lmix/ftb` | – | `fade_to_black` |
//! | `/lmix/tbar` | position (float 0–1) | `t_bar` |
//! | `/lmix/overlay/{n}/in` | input | `overlay_in` |
//! | `/lmix/overlay/{n}/out` | – | `overlay_out` |
//! | `/lmix/overlay/{n}/last` | – | `overlay_last` |
//! | `/lmix/overlay/{n}/preview` | input, or none to clear | `overlay_preview` |
//! | `/lmix/overlay/out` | – | `overlay_out_all` |
//! | `/lmix/cue/go` | list, or none for the active one | `cue_go` |
//! | `/lmix/cue/back` | list, or none | `cue_standby` previous |
//! | `/lmix/cue/stop` | list, or none for every cue | `cue_stop` |
//! | `/lmix/cue/start` | list, cue (number or id) | `cue_start` |
//! | `/lmix/cue/toggle` | list, cue (number or id) | `cue_toggle`: a pad (RUN.12) |
//! | `/lmix/cue/release` | none | the GO button is let go (RUN.6) |
//! | `/lmix/panic` | time in ms (int) or s (float), or none | `cue_panic` (twice: hard stop) |
//! | `/lmix/stop/hard` | – | `cue_hard_stop` |
//! | `/lmix/subscribe` | none, or topic and fields (`osc_subs`, CTL.3) | the sender gets tally and state, or that topic, from now on |
//! | `/lmix/unsubscribe` | none, or a topic | stops it, or that topic |
//! | `/lmix/ping` | – | answers `/lmix/pong`; keeps a subscription |
//! | `/lmix/state` | – | answers `/lmix/state s:json` (the full state) |
//!
//! Refused actions answer the sender with `/lmix/error s:reason`.
//! A subscription lasts 60 s after the subscriber's last message (CTL.3):
//! send `/lmix/subscribe` or `/lmix/ping` within that. For buttons without
//! keepalive there are fixed subscribers (`set_feed`, CTL.4).
//! Subscribers get, right after subscribing and then on every change:
//!
//! | Address | Arguments |
//! |---|---|
//! | `/lmix/tally` | digits: per input 0 off, 1 program, 2 preview |
//! | `/lmix/program`, `/lmix/preview` | name, number (0: a layout, or none) |
//! | `/lmix/tbar` | position (float 0–1) |
//! | `/lmix/ftb` | 1 while faded to black |
//! | `/lmix/cue/standby` | list name, number of the standby cue (1-based), its name |
//!
//! Bundles are unpacked; their time tags are ignored (run now).

use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;

use lmix_types::{Action, Event, InputRef, ShowState};
use tokio::net::UdpSocket;
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::Mutex;

use crate::access::Right;
use crate::osc_subs::{Subscription, Topic, Wish};
use crate::service::Service;

/// Largest datagram read.
const MAX_PACKET: usize = 64 * 1024;

/// Subscribers kept at most; the oldest is dropped beyond that.
const MAX_SUBSCRIBERS: usize = 32;

/// A subscription ends this long after the subscriber's last message.
const SUBSCRIPTION_TTL: std::time::Duration = std::time::Duration::from_secs(60);

/// A subscriber: when it was last heard, and what it hears (CTL.3).
#[derive(Clone, Debug)]
struct Subscriber {
    peer: SocketAddr,
    heard: tokio::time::Instant,
    wants: Subscription,
}

type Subscribers = Arc<Mutex<Vec<Subscriber>>>;

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
    let args: Vec<Arg> = args.iter().map(|a| Arg::Str((*a).to_string())).collect();
    encode_args(address, &args)
}

/// A message with any arguments.
pub fn encode_args(address: &str, args: &[Arg]) -> Vec<u8> {
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
            Arg::Str(text) => write_string(&mut out, text),
        }
    }
    out
}

/// What subscriber `wants` hears about the state, keyed so that only
/// changes are sent.
fn state_messages(state: &ShowState, wants: &Subscription) -> Vec<(String, Vec<u8>)> {
    let mut out = if wants.wants(Topic::Mixer) {
        mixer_messages(state)
    } else {
        Vec::new()
    };
    out.extend(crate::osc_subs::standby(state, wants));
    out
}

/// Program, preview, T-bar and FTB.
fn mixer_messages(state: &ShowState) -> Vec<(String, Vec<u8>)> {
    let source = |id: Option<lmix_types::InputId>| -> [Arg; 2] {
        let input = id.and_then(|id| state.inputs.iter().find(|i| i.id == id));
        let layout = id.and_then(|id| state.layouts.iter().find(|l| l.id == id));
        match (input, layout) {
            (Some(i), _) => [Arg::Str(i.name.clone()), Arg::Int(i.number as i32)],
            (None, Some(l)) => [Arg::Str(l.name.clone()), Arg::Int(0)],
            _ => [Arg::Str(String::new()), Arg::Int(0)],
        }
    };
    vec![
        (
            "program".to_string(),
            encode_args("/lmix/program", &source(state.program)),
        ),
        (
            "preview".to_string(),
            encode_args("/lmix/preview", &source(state.preview)),
        ),
        (
            "tbar".to_string(),
            encode_args("/lmix/tbar", &[Arg::Float(state.t_bar)]),
        ),
        (
            "ftb".to_string(),
            encode_args("/lmix/ftb", &[Arg::Int(i32::from(state.fade_to_black))]),
        ),
    ]
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
    /// The sender's GO button is let go (RUN.6).
    Release,
    Subscribe(Wish),
    Ping,
    Unsubscribe(Option<Topic>),
    State,
}

/// String arguments of `message`; others are skipped.
fn texts(message: &Message) -> Vec<String> {
    message
        .args
        .iter()
        .filter_map(|a| match a {
            Arg::Str(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
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
        ["subscribe"] => return crate::osc_subs::wish(&texts(message)).map(Request::Subscribe),
        ["ping"] => return Ok(Request::Ping),
        ["unsubscribe"] => {
            return crate::osc_subs::unwish(&texts(message)).map(Request::Unsubscribe)
        }
        ["state"] => return Ok(Request::State),
        ["action"] => match arg(0) {
            Some(Arg::Str(text)) => serde_json::from_str(text).map_err(|e| e.to_string())?,
            _ => return Err("/lmix/action needs a JSON string".into()),
        },
        ["cut"] => Action::Cut,
        ["ftb"] => json(serde_json::json!({ "action": "fade_to_black" }))?,
        ["preview"] => Action::PreviewInput { input: input(0)? },
        ["program"] => Action::CutDirect { input: input(0)? },
        ["quickplay"] => Action::QuickPlay {
            input: arg(0).map(|_| input(0)).transpose()?,
        },
        ["go"] => Action::GoInput { input: input(0)? },
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
            let channel = channel.parse::<u8>().map_err(|_| "overlay channel 1–8")?;
            let input = match input(0)? {
                InputRef::Number(n) => serde_json::json!(n),
                InputRef::Text(s) => serde_json::json!(s),
            };
            json(serde_json::json!({ "action": "overlay_in", "channel": channel, "input": input }))?
        }
        ["overlay", channel, "out"] => {
            let channel = channel.parse::<u8>().map_err(|_| "overlay channel 1–8")?;
            json(serde_json::json!({ "action": "overlay_out", "channel": channel }))?
        }
        ["overlay", channel, "last"] => {
            let channel = channel.parse::<u8>().map_err(|_| "overlay channel 1–8")?;
            json(serde_json::json!({ "action": "overlay_last", "channel": channel }))?
        }
        ["overlay", channel, "preview"] => {
            let channel = channel.parse::<u8>().map_err(|_| "overlay channel 1–8")?;
            Action::OverlayPreview {
                channel,
                input: arg(0).map(|_| input(0)).transpose()?,
            }
        }
        ["overlay", "out"] => json(serde_json::json!({ "action": "overlay_out_all" }))?,
        // An optional time: panic over that many ms (CTL.1).
        ["panic"] => Action::CuePanic {
            ms: match arg(0) {
                Some(Arg::Int(ms)) => {
                    Some(u32::try_from(*ms).map_err(|_| "panic time 0–10000 ms")?)
                }
                Some(Arg::Float(s)) => Some((s * 1000.0).round() as u32),
                _ => None,
            },
        },
        ["stop", "hard"] => Action::CueHardStop,
        ["cue", "release"] => return Ok(Request::Release),
        // A pad's button: a list and a cue (RUN.12).
        ["cue", step @ ("start" | "toggle")] => {
            let list = Some(input(0)?);
            let cue = match arg(1) {
                Some(Arg::Str(cue)) => cue.clone(),
                Some(Arg::Int(number)) => number.to_string(),
                _ => return Err("a list and a cue (number or id)".into()),
            };
            match *step {
                "start" => Action::CueStart { list, cue },
                _ => Action::CueToggle { list, cue },
            }
        }
        ["cue", step @ ("go" | "back" | "stop")] => {
            let list = arg(0).map(|_| input(0)).transpose()?;
            match *step {
                "go" => Action::CueGo { list },
                "back" => Action::CueStandby {
                    list,
                    to: lmix_types::StandbyMove::Previous,
                },
                _ => Action::CueStop {
                    list,
                    cue: None,
                    mode: lmix_types::StopMode::Fade,
                },
            }
        }
        _ => return Err(format!("unknown address {}", message.address)),
    };
    Ok(Request::Action(action))
}

/// A sender's GO button (RUN.6): Companion sends all its buttons from one
/// socket, so any GO button there counts as the same.
fn hold(peer: SocketAddr) -> crate::cues::Hold {
    crate::cues::Hold::of(&format!("osc {peer}"))
}

/// Receive messages and send tally until the process ends.
/// OSC carries no password: senders outside the local network are
/// ignored unless `internet`, and with a password (`local_only`) all but
/// this machine (API.7) and the trusted addresses, with their right;
/// `open_panic` lets panic and hard stop through from the network all
/// the same (CTL.5).
pub async fn serve(
    listen: SocketAddr,
    service: Arc<Service>,
    internet: bool,
    local_only: bool,
    open_panic: bool,
    trusted: Vec<crate::access::Trust>,
) -> anyhow::Result<()> {
    let access = crate::access::Access {
        internet,
        trusted,
        ..Default::default()
    };
    let socket = Arc::new(UdpSocket::bind(listen).await?);
    tracing::info!(%listen, "OSC up");
    let subscribers: Subscribers = Arc::new(Mutex::new(Vec::new()));
    tokio::spawn(send_updates(
        socket.clone(),
        subscribers.clone(),
        service.clone(),
    ));

    let mut buf = vec![0u8; MAX_PACKET];
    loop {
        let (len, peer) = socket.recv_from(&mut buf).await?;
        // None: only an open panic.
        let granted = match access.peer(peer.ip()) {
            crate::access::Peer::Refused => continue,
            crate::access::Peer::Network if local_only => access.trusted(peer.ip()),
            crate::access::Peer::Network | crate::access::Peer::Local => Some(Right::Edit),
        };
        if granted.is_none() && !open_panic {
            continue;
        }
        // Any message keeps the sender's subscription (CTL.3).
        heard(&subscribers, peer).await;
        for message in decode(&buf[..len]) {
            let request = request(&message);
            let panic = matches!(
                request,
                Ok(Request::Action(
                    Action::CuePanic { .. } | Action::CueHardStop
                ))
            );
            let allowed = match (&request, granted) {
                (Ok(Request::Action(action)), Some(right)) => {
                    crate::access::right_of(action) <= right
                }
                (_, granted) => granted.is_some(),
            };
            if !(allowed || panic && open_panic) {
                continue;
            }
            match request {
                // GO waits for this sender's release (RUN.6).
                Ok(Request::Action(action @ Action::CueGo { .. })) => {
                    if let Err(error) = service.go_held(action, hold(peer)).await {
                        let reply = encode("/lmix/error", &[&error.to_string()]);
                        let _ = socket.send_to(&reply, peer).await;
                    }
                }
                Ok(Request::Release) => service.release(hold(peer)).await,
                Ok(Request::Action(action)) => {
                    if let Err(error) = service.handle(action).await {
                        let reply = encode("/lmix/error", &[&error.to_string()]);
                        let _ = socket.send_to(&reply, peer).await;
                    }
                }
                Ok(Request::Subscribe(wish)) => {
                    let wants = {
                        let mut subs = subscribers.lock().await;
                        let i = match subs.iter().position(|s| s.peer == peer) {
                            Some(i) => i,
                            None => {
                                if subs.len() >= MAX_SUBSCRIBERS {
                                    subs.remove(0);
                                }
                                subs.push(Subscriber {
                                    peer,
                                    heard: tokio::time::Instant::now(),
                                    wants: Subscription::default(),
                                });
                                subs.len() - 1
                            }
                        };
                        subs[i].wants.add(wish);
                        subs[i].wants.clone()
                    };
                    // The current values at once.
                    if wants.wants(Topic::Mixer) {
                        let tally = tally_message(&service.tally().await);
                        let _ = socket.send_to(&tally, peer).await;
                    }
                    for (_, message) in state_messages(&service.state().await, &wants) {
                        let _ = socket.send_to(&message, peer).await;
                    }
                }
                Ok(Request::State) => {
                    let json = serde_json::to_string(&service.state().await).unwrap_or_default();
                    let _ = socket.send_to(&encode("/lmix/state", &[&json]), peer).await;
                }
                Ok(Request::Unsubscribe(topic)) => subscribers
                    .lock()
                    .await
                    .retain_mut(|s| s.peer != peer || s.wants.remove(topic)),
                Ok(Request::Ping) => {
                    let _ = socket.send_to(&encode("/lmix/pong", &[]), peer).await;
                }
                Err(reason) => {
                    let _ = socket
                        .send_to(&encode("/lmix/error", &[&reason]), peer)
                        .await;
                }
            }
        }
    }
}

/// `peer` sent something: its subscription, if any, starts over.
async fn heard(subscribers: &Subscribers, peer: SocketAddr) {
    if let Some(entry) = subscribers.lock().await.iter_mut().find(|s| s.peer == peer) {
        entry.heard = tokio::time::Instant::now();
    }
}

/// Subscribers heard within the TTL; the others are dropped.
async fn live(subscribers: &Subscribers) -> Vec<(SocketAddr, Subscription)> {
    let mut subs = subscribers.lock().await;
    subs.retain(|s| s.heard.elapsed() < SUBSCRIPTION_TTL);
    subs.iter().map(|s| (s.peer, s.wants.clone())).collect()
}

fn tally_message(tally: &[u8]) -> Vec<u8> {
    let digits: String = tally.iter().map(|t| char::from(b'0' + t.min(&9))).collect();
    encode("/lmix/tally", &[&digits])
}

/// Send what each subscriber wants as it changes (CTL.2, CTL.3).
async fn send_updates(socket: Arc<UdpSocket>, subscribers: Subscribers, service: Arc<Service>) {
    let mut events = service.subscribe();
    // Last message sent per subscriber and key: only changes go out.
    let mut sent: std::collections::HashMap<(SocketAddr, String), Vec<u8>> =
        std::collections::HashMap::new();
    // Lists with a running cue at the last report: they get one empty
    // message when it ends.
    let mut ran: HashSet<lmix_types::InputId> = HashSet::new();
    let mut state = service.state().await;
    loop {
        let event = match events.recv().await {
            Ok(event) => event,
            // Fell behind: everything anew.
            Err(RecvError::Lagged(_)) => {
                sent.clear();
                Event::State {
                    state: service.state().await,
                }
            }
            Err(RecvError::Closed) => return,
        };
        let subs = live(&subscribers).await;
        sent.retain(|(peer, _), _| subs.iter().any(|(p, _)| p == peer));
        // Keyed messages are sent when they changed, the others always.
        let mut keyed: Vec<(SocketAddr, String, Vec<u8>)> = Vec::new();
        let mut plain: Vec<(SocketAddr, Vec<u8>)> = Vec::new();
        match &event {
            Event::Tally { tally } => {
                let message = tally_message(tally);
                for (peer, _) in subs.iter().filter(|(_, w)| w.wants(Topic::Mixer)) {
                    keyed.push((*peer, "tally".into(), message.clone()));
                }
            }
            Event::State { state: new } => {
                state = new.clone();
                for (peer, wants) in &subs {
                    for (key, message) in state_messages(&state, wants) {
                        keyed.push((*peer, key, message));
                    }
                }
            }
            Event::Running { cues } => {
                let now: HashSet<_> = cues.iter().map(|c| c.list).collect();
                let ended: Vec<_> = ran.difference(&now).copied().collect();
                ran = now;
                for (peer, wants) in &subs {
                    for message in crate::osc_subs::running(&state.cue_lists, cues, &ended, wants) {
                        plain.push((*peer, message));
                    }
                }
            }
            Event::Cue { list, cue, what } => {
                for (peer, wants) in &subs {
                    let message = crate::osc_subs::cue(&state.cue_lists, *list, *cue, what, wants);
                    plain.extend(message.map(|m| (*peer, m)));
                }
            }
            Event::Panic { mode } => {
                for (peer, wants) in &subs {
                    plain.extend(crate::osc_subs::panic(*mode, wants).map(|m| (*peer, m)));
                }
            }
            Event::Target { target, result } => {
                for (peer, wants) in &subs {
                    let message = crate::osc_subs::target(target, result, wants);
                    plain.extend(message.map(|m| (*peer, m)));
                }
            }
            _ => continue,
        }
        for (peer, key, message) in keyed {
            if sent.get(&(peer, key.clone())) != Some(&message) {
                sent.insert((peer, key), message.clone());
                plain.push((peer, message));
            }
        }
        for (peer, message) in plain {
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
            req("/lmix/panic", vec![]),
            Ok(Request::Action(Action::CuePanic { ms: None }))
        );
        assert_eq!(
            req("/lmix/stop/hard", vec![]),
            Ok(Request::Action(Action::CueHardStop))
        );
        assert_eq!(
            req("/lmix/panic", vec![Arg::Int(3000)]),
            Ok(Request::Action(Action::CuePanic { ms: Some(3000) }))
        );
        assert_eq!(
            req("/lmix/cue/go", vec![]),
            Ok(Request::Action(Action::CueGo { list: None }))
        );
        assert_eq!(req("/lmix/cue/release", vec![]), Ok(Request::Release));
        assert_eq!(
            req(
                "/lmix/cue/toggle",
                vec![Arg::Str("Pads".into()), Arg::Int(3)]
            ),
            Ok(Request::Action(Action::CueToggle {
                list: Some(InputRef::Text("Pads".into())),
                cue: "3".into()
            }))
        );
        assert_eq!(
            req(
                "/lmix/cue/start",
                vec![Arg::Int(2), Arg::Str("Thunder".into())]
            ),
            Ok(Request::Action(Action::CueStart {
                list: Some(InputRef::Number(2)),
                cue: "Thunder".into()
            }))
        );
        assert!(req("/lmix/cue/toggle", vec![Arg::Int(2)]).is_err());
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
        assert_eq!(
            req("/lmix/quickplay", vec![]),
            Ok(Request::Action(Action::QuickPlay { input: None }))
        );
        assert_eq!(
            req("/lmix/quickplay", vec![Arg::Int(2)]),
            Ok(Request::Action(Action::QuickPlay {
                input: Some(InputRef::Number(2))
            }))
        );
        assert_eq!(
            req("/lmix/go", vec![Arg::Str("Clip".into())]),
            Ok(Request::Action(Action::GoInput {
                input: InputRef::Text("Clip".into())
            }))
        );
        assert!(matches!(
            req("/lmix/overlay/3/last", vec![]),
            Ok(Request::Action(Action::OverlayLast { channel: 3, .. }))
        ));
        assert_eq!(
            req("/lmix/overlay/1/preview", vec![Arg::Int(4)]),
            Ok(Request::Action(Action::OverlayPreview {
                channel: 1,
                input: Some(InputRef::Number(4))
            }))
        );
        assert_eq!(
            req("/lmix/overlay/1/preview", vec![]),
            Ok(Request::Action(Action::OverlayPreview {
                channel: 1,
                input: None
            }))
        );
        assert!(matches!(
            req("/lmix/overlay/out", vec![]),
            Ok(Request::Action(Action::OverlayOutAll { .. }))
        ));
        assert!(matches!(
            req(
                "/lmix/action",
                vec![Arg::Str(r#"{"action":"fade_to_black"}"#.into())]
            ),
            Ok(Request::Action(Action::FadeToBlack { .. }))
        ));
        assert_eq!(req("/lmix/subscribe", vec![]), Ok(Request::Subscribe(None)));
        assert_eq!(
            req(
                "/lmix/subscribe",
                vec![Arg::Str("running".into()), Arg::Str("remaining".into())]
            ),
            Ok(Request::Subscribe(Some((
                Topic::Running,
                vec![crate::osc_subs::Field::Remaining]
            ))))
        );
        assert_eq!(
            req("/lmix/unsubscribe", vec![Arg::Str("cue".into())]),
            Ok(Request::Unsubscribe(Some(Topic::Cue)))
        );
        assert!(req("/lmix/subscribe", vec![Arg::Str("weather".into())]).is_err());
        assert_eq!(req("/lmix/ping", vec![]), Ok(Request::Ping));
        assert!(req("/lmix/preview", vec![]).is_err());
        assert!(req("/lmix/frob", vec![]).is_err());
    }

    #[test]
    fn state_goes_out_as_messages() {
        let mut sw = crate::switcher::Switcher::new(2);
        for name in ["Cam", "Slides"] {
            sw.apply(
                serde_json::from_value(serde_json::json!({
                    "action": "add_input", "name": name,
                    "source": {"kind": "pattern", "pattern": "bars"}
                }))
                .unwrap(),
            )
            .unwrap();
        }
        sw.apply(Action::CutDirect {
            input: InputRef::Text("Slides".into()),
        })
        .unwrap();
        let mut old = Subscription::default();
        old.add(None);
        let messages: Vec<Message> = state_messages(&sw.state(), &old)
            .iter()
            .flat_map(|(_, packet)| decode(packet))
            .collect();
        assert!(messages.contains(&Message {
            address: "/lmix/program".into(),
            args: vec![Arg::Str("Slides".into()), Arg::Int(2)],
        }));
        assert!(messages.contains(&Message {
            address: "/lmix/preview".into(),
            args: vec![Arg::Str(String::new()), Arg::Int(0)],
        }));
        assert!(messages.contains(&Message {
            address: "/lmix/tbar".into(),
            args: vec![Arg::Float(0.0)],
        }));
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

    #[tokio::test(start_paused = true)]
    async fn a_subscription_ends_without_a_word() {
        let subscribers: Subscribers = Arc::new(Mutex::new(Vec::new()));
        let a: SocketAddr = "192.168.1.5:9000".parse().unwrap();
        let b: SocketAddr = "192.168.1.6:9000".parse().unwrap();
        let now = tokio::time::Instant::now();
        let sub = |peer| Subscriber {
            peer,
            heard: now,
            wants: Subscription::default(),
        };
        subscribers.lock().await.extend([sub(a), sub(b)]);

        tokio::time::advance(std::time::Duration::from_secs(40)).await;
        heard(&subscribers, a).await;
        tokio::time::advance(std::time::Duration::from_secs(30)).await;

        let peers: Vec<SocketAddr> = live(&subscribers)
            .await
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        assert_eq!(peers, [a], "b said nothing for 70 s");
    }
}
