//! Tally to TSL UMD displays and tally systems (PER.1): multiviewers,
//! under-monitor displays and tally boxes speak TSL. lmix sends each
//! input's tally and name over UDP whenever the tally changes, and every
//! few seconds again (receivers forget a source that falls silent).
//!
//! | Version | Packet |
//! |---|---|
//! | 3.1 | 18 bytes: `0x80 + address`, control (bit 0 tally 1, bit 1 tally 2, bits 4–5 brightness), 16 ASCII characters |
//! | 5.0 | `PBC` (u16 LE, bytes after it), version 0, flags, screen (u16), then per display: index (u16), control (u16: bits 0–1 right tally, 2–3 text tally, 4–5 left tally, 6–7 brightness), length (u16), text |
//!
//! Tally 1 / red is program, tally 2 / green preview. The address (3.1) or
//! index (5.0) is the input's number in the bar.
//!
//!   lmix-core --tsl 192.168.1.50:8900               # TSL 5.0
//!   lmix-core --tsl 192.168.1.50:4003 --tsl-version 3

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use clap::ValueEnum;
use lmix_types::Event;
use tokio::net::UdpSocket;
use tokio::sync::broadcast::error::RecvError;

use crate::service::Service;

/// Sent again this often without a change.
const REFRESH: Duration = Duration::from_secs(5);
/// Display text of TSL 3.1.
const V3_TEXT: usize = 16;
/// Addresses TSL 3.1 has (0–126).
const V3_ADDRESSES: u32 = 127;
/// Full brightness (both versions: 0–3).
const BRIGHT: u8 = 3;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Version {
    #[value(name = "3")]
    V31,
    #[default]
    #[value(name = "5")]
    V50,
}

/// Tally of one input as TSL shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Display {
    /// The input's number in the bar.
    pub number: u32,
    pub name: String,
    pub program: bool,
    pub preview: bool,
}

/// TSL 3.1 packet of one display; `None` beyond address 126.
pub fn packet_v3(d: &Display) -> Option<Vec<u8>> {
    if d.number >= V3_ADDRESSES {
        return None;
    }
    let control = u8::from(d.program) | (u8::from(d.preview) << 1) | (BRIGHT << 4);
    let mut packet = vec![0x80 + d.number as u8, control];
    // Printable ASCII only, padded with spaces.
    let text: Vec<u8> = d
        .name
        .chars()
        .map(|c| {
            if c.is_ascii_graphic() || c == ' ' {
                c as u8
            } else {
                b'?'
            }
        })
        .chain(std::iter::repeat(b' '))
        .take(V3_TEXT)
        .collect();
    packet.extend(text);
    Some(packet)
}

/// TSL 5.0 packet with all displays (ASCII text, screen 0).
pub fn packet_v5(displays: &[Display]) -> Vec<u8> {
    let mut body = vec![0u8, 0u8]; // version 0, flags: ASCII, display data
    body.extend(0u16.to_le_bytes()); // screen
    for d in displays {
        // Red lamps and text for program, green for preview.
        let lamp: u16 = if d.program {
            1
        } else if d.preview {
            2
        } else {
            0
        };
        let control = lamp | (lamp << 2) | (lamp << 4) | (u16::from(BRIGHT) << 6);
        let text: Vec<u8> = d
            .name
            .chars()
            .map(|c| {
                if c.is_ascii() && !c.is_ascii_control() {
                    c as u8
                } else {
                    b'?'
                }
            })
            .take(u16::MAX as usize)
            .collect();
        body.extend((d.number as u16).to_le_bytes());
        body.extend(control.to_le_bytes());
        body.extend((text.len() as u16).to_le_bytes());
        body.extend(text);
    }
    let mut packet = (body.len() as u16).to_le_bytes().to_vec();
    packet.extend(body);
    packet
}

/// Packets for the displays in `version`.
pub fn packets(version: Version, displays: &[Display]) -> Vec<Vec<u8>> {
    match version {
        Version::V31 => displays.iter().filter_map(packet_v3).collect(),
        // One packet holds them all while it stays small (UDP).
        Version::V50 => displays.chunks(32).map(packet_v5).collect(),
    }
}

/// A TSL 5.0 packet framed for TCP: DLE/STX first, every DLE (0xFE)
/// inside doubled. TSL 3.1 goes over TCP as it is.
pub fn tcp_frame(version: Version, packet: &[u8]) -> Vec<u8> {
    match version {
        Version::V31 => packet.to_vec(),
        Version::V50 => {
            let mut framed = vec![0xFE, 0x02];
            for &byte in packet {
                framed.push(byte);
                if byte == 0xFE {
                    framed.push(0xFE);
                }
            }
            framed
        }
    }
}

/// A TCP receiver, connected when needed and again after a failure.
struct TcpTarget {
    address: SocketAddr,
    stream: Option<tokio::net::TcpStream>,
    /// After a failed connect, no new try before this: one receiver that
    /// is down must not hold up every packet.
    retry_after: Option<tokio::time::Instant>,
}

impl TcpTarget {
    async fn send(&mut self, bytes: &[u8]) {
        use tokio::io::AsyncWriteExt;
        if self.stream.is_none() {
            if self
                .retry_after
                .is_some_and(|t| tokio::time::Instant::now() < t)
            {
                return;
            }
            let connect = tokio::net::TcpStream::connect(self.address);
            match tokio::time::timeout(Duration::from_secs(1), connect).await {
                Ok(Ok(stream)) => self.stream = Some(stream),
                _ => {
                    tracing::debug!(address = %self.address, "TSL TCP receiver not reachable");
                    self.retry_after = Some(tokio::time::Instant::now() + Duration::from_secs(2));
                    return;
                }
            }
        }
        if let Some(stream) = &mut self.stream {
            if stream.write_all(bytes).await.is_err() {
                // Reconnect at the next send (every change, or 5 s).
                self.stream = None;
            }
        }
    }
}

/// Displays of the inputs with a picture, from the show state.
fn displays(state: &lmix_types::ShowState) -> Vec<Display> {
    state
        .inputs
        .iter()
        .filter(|i| i.source.has_video())
        .map(|i| Display {
            number: i.number,
            name: i.name.clone(),
            program: state.program == Some(i.id),
            preview: state.preview == Some(i.id),
        })
        .collect()
}

/// Send tally to `targets` until the process ends.
pub async fn serve(
    targets: Vec<SocketAddr>,
    tcp: Vec<SocketAddr>,
    version: Version,
    service: Arc<Service>,
) -> anyhow::Result<()> {
    let socket = UdpSocket::bind(("0.0.0.0", 0)).await?;
    let mut tcp: Vec<TcpTarget> = tcp
        .into_iter()
        .map(|address| TcpTarget {
            address,
            stream: None,
            retry_after: None,
        })
        .collect();
    tracing::info!(?targets, tcp = tcp.len(), ?version, "TSL tally up");
    let mut events = service.subscribe();
    // From the state now: the show may be on air before the first change.
    let mut last = displays(&service.state().await);
    let mut tick = tokio::time::interval(REFRESH);
    loop {
        let before = last.clone();
        let changed = tokio::select! {
            event = events.recv() => match event {
                Ok(Event::State { state }) => {
                    last = displays(&state);
                    last != before
                }
                Ok(_) => false,
                Err(RecvError::Lagged(_)) => {
                    last = displays(&service.state().await);
                    true
                }
                Err(RecvError::Closed) => return Ok(()),
            },
            _ = tick.tick() => true,
        };
        if !changed {
            continue;
        }
        // Displays whose input went away are put out once, blank, or the
        // receiver keeps their last tally.
        let gone = before
            .iter()
            .filter(|d| !last.iter().any(|n| n.number == d.number))
            .map(|d| Display {
                number: d.number,
                name: String::new(),
                program: false,
                preview: false,
            });
        let sent: Vec<Display> = last.iter().cloned().chain(gone).collect();
        for packet in packets(version, &sent) {
            for target in &targets {
                if let Err(error) = socket.send_to(&packet, target).await {
                    tracing::debug!(%target, %error, "TSL send failed");
                }
            }
            let framed = tcp_frame(version, &packet);
            for target in &mut tcp {
                target.send(&framed).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::null::NullEngine;
    use lmix_types::{Action, InputRef, SourceSpec, TestPattern};

    fn d(number: u32, name: &str, program: bool, preview: bool) -> Display {
        Display {
            number,
            name: name.into(),
            program,
            preview,
        }
    }

    #[test]
    fn v3_packet() {
        let p = packet_v3(&d(3, "Cam 3 – wide", true, false)).unwrap();

        assert_eq!(p.len(), 18);
        assert_eq!(p[0], 0x83);
        assert_eq!(p[1], 0b0011_0001, "tally 1 on, full brightness");
        assert_eq!(&p[2..], b"Cam 3 ? wide    ");
        assert_eq!(packet_v3(&d(4, "x", false, true)).unwrap()[1], 0b0011_0010);
        assert!(packet_v3(&d(127, "x", false, false)).is_none());
    }

    #[test]
    fn v5_packet() {
        let p = packet_v5(&[d(1, "PGM", true, false), d(2, "PV", false, true)]);

        // PBC counts the bytes after it.
        assert_eq!(u16::from_le_bytes([p[0], p[1]]) as usize, p.len() - 2);
        assert_eq!(&p[2..6], &[0, 0, 0, 0], "version, flags, screen");
        assert_eq!(u16::from_le_bytes([p[6], p[7]]), 1, "index");
        assert_eq!(
            u16::from_le_bytes([p[8], p[9]]),
            1 | (1 << 2) | (1 << 4) | (3 << 6),
            "red, bright"
        );
        assert_eq!(u16::from_le_bytes([p[10], p[11]]), 3);
        assert_eq!(&p[12..15], b"PGM");
        assert_eq!(
            u16::from_le_bytes([p[17], p[18]]),
            2 | (2 << 2) | (2 << 4) | (3 << 6),
            "green"
        );
        assert_eq!(&p[21..23], b"PV");
    }

    #[test]
    fn tcp_framing() {
        assert_eq!(
            tcp_frame(Version::V50, &[1, 0xFE, 2]),
            [0xFE, 0x02, 1, 0xFE, 0xFE, 2]
        );
        assert_eq!(tcp_frame(Version::V31, &[1, 0xFE]), [1, 0xFE]);
    }

    #[tokio::test]
    async fn tcp_receivers_get_framed_packets() {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        service
            .handle(Action::AddInput {
                source: SourceSpec::Pattern {
                    pattern: TestPattern::Bars,
                },
                name: Some("Bars".into()),
            })
            .await
            .unwrap();
        tokio::spawn(serve(
            Vec::new(),
            vec![address],
            Version::V50,
            service.clone(),
        ));

        // The first send (from the state at start) connects.
        let (mut stream, _) = tokio::time::timeout(Duration::from_secs(7), listener.accept())
            .await
            .unwrap()
            .unwrap();
        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(Duration::from_secs(7), stream.read(&mut buf))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(&buf[..2], &[0xFE, 0x02]);
        assert!(buf[..n].windows(4).any(|w| w == b"Bars"));
    }

    #[tokio::test]
    async fn sends_on_change() {
        let receiver = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let target = receiver.local_addr().unwrap();
        let service = Service::new(2, Arc::new(NullEngine::default()), None);
        tokio::spawn(serve(
            vec![target],
            Vec::new(),
            Version::V31,
            service.clone(),
        ));
        tokio::time::sleep(Duration::from_millis(100)).await;

        service
            .handle(Action::AddInput {
                source: SourceSpec::Pattern {
                    pattern: TestPattern::Bars,
                },
                name: Some("Bars".into()),
            })
            .await
            .unwrap();
        service
            .handle(Action::CutDirect {
                input: InputRef::Number(1),
            })
            .await
            .unwrap();

        let mut buf = [0u8; 64];
        let program = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let (n, _) = receiver.recv_from(&mut buf).await.unwrap();
                if n == 18 && buf[1] & 1 == 1 {
                    return buf[..n].to_vec();
                }
            }
        })
        .await
        .expect("a program tally");
        assert_eq!(program[0], 0x81);
        assert_eq!(&program[2..6], b"Bars");

        // Removed: display 1 goes blank and off.
        service
            .handle(Action::RemoveInput {
                input: InputRef::Number(1),
            })
            .await
            .unwrap();
        let blank = tokio::time::timeout(Duration::from_secs(3), async {
            loop {
                let (n, _) = receiver.recv_from(&mut buf).await.unwrap();
                if n == 18 && buf[0] == 0x81 && buf[1] & 0b11 == 0 && buf[2] == b' ' {
                    return true;
                }
            }
        })
        .await;
        assert!(blank.is_ok(), "a blank display 1");
    }
}
