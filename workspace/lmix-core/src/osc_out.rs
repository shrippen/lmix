//! OSC messages out to the addresses of an output target (EXT.6).
//!
//! ```text
//!   osc_send "Pult" /eos/go ──► osc_udp: one datagram per address
//!                           └─► osc_tcp: connect, SLIP frame, close
//! ```
//!
//! OSC answers nothing: over UDP a message counts as reached once it is
//! sent, over TCP once the connection took it.

use std::net::SocketAddr;
use std::time::Duration;

use lmix_types::TargetKind;
use tokio::io::AsyncWriteExt;

use crate::targets::TargetConfig;

/// SLIP (RFC 1055) as OSC 1.1 frames a packet over TCP.
const END: u8 = 0xC0;
const ESC: u8 = 0xDB;
const ESC_END: u8 = 0xDC;
const ESC_ESC: u8 = 0xDD;

/// `packet` framed for a stream: `END`, the escaped bytes, `END`.
pub fn slip(packet: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(packet.len() + 2);
    out.push(END);
    for &byte in packet {
        match byte {
            END => out.extend_from_slice(&[ESC, ESC_END]),
            ESC => out.extend_from_slice(&[ESC, ESC_ESC]),
            _ => out.push(byte),
        }
    }
    out.push(END);
    out
}

/// `packet` to one `address` of `target`, within its time limit.
pub async fn send(target: &TargetConfig, address: &str, packet: &[u8]) -> Result<(), String> {
    let limit = Duration::from_millis(u64::from(target.timeout_ms));
    let sent = async {
        let peer = resolve(address).await?;
        match target.kind {
            TargetKind::OscTcp => send_tcp(peer, packet).await,
            _ => send_udp(peer, packet).await,
        }
    };
    match tokio::time::timeout(limit, sent).await {
        Ok(result) => result.map_err(|reason| format!("{address}: {reason}")),
        Err(_) => Err(format!("{address}: no answer in time")),
    }
}

/// A probe of an OSC target (EXT.5): its first address resolves and,
/// over TCP, takes a connection. Sends nothing.
pub async fn probe(target: &TargetConfig) -> Result<(), String> {
    let Some(address) = target.addresses.first() else {
        return Err(format!("{} has no address", target.name));
    };
    let limit = Duration::from_millis(u64::from(target.timeout_ms));
    let reached = async {
        let peer = resolve(address).await?;
        if target.kind == TargetKind::OscTcp {
            tokio::net::TcpStream::connect(peer)
                .await
                .map_err(|_| "cannot connect".to_string())?;
        }
        Ok(())
    };
    match tokio::time::timeout(limit, reached).await {
        Ok(result) => result.map_err(|reason: String| format!("{address}: {reason}")),
        Err(_) => Err(format!("{address}: no answer in time")),
    }
}

/// The first socket address of `host:port`.
async fn resolve(address: &str) -> Result<SocketAddr, String> {
    tokio::net::lookup_host(address)
        .await
        .ok()
        .and_then(|mut found| found.next())
        .ok_or_else(|| "unknown host".to_string())
}

async fn send_udp(peer: SocketAddr, packet: &[u8]) -> Result<(), String> {
    let any: SocketAddr = match peer {
        SocketAddr::V4(_) => (std::net::Ipv4Addr::UNSPECIFIED, 0).into(),
        SocketAddr::V6(_) => (std::net::Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let socket = tokio::net::UdpSocket::bind(any)
        .await
        .map_err(|e| format!("no socket: {e}"))?;
    socket
        .send_to(packet, peer)
        .await
        .map_err(|e| format!("not sent: {e}"))?;
    Ok(())
}

async fn send_tcp(peer: SocketAddr, packet: &[u8]) -> Result<(), String> {
    let mut stream = tokio::net::TcpStream::connect(peer)
        .await
        .map_err(|_| "cannot connect".to_string())?;
    stream
        .write_all(&slip(packet))
        .await
        .map_err(|e| format!("not sent: {e}"))?;
    let _ = stream.shutdown().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::osc::{decode, encode_args, Arg};
    use tokio::io::AsyncReadExt;

    fn target(kind: TargetKind, addresses: &[String]) -> TargetConfig {
        TargetConfig {
            name: "Pult".into(),
            kind,
            addresses: addresses.to_vec(),
            secret: None,
            timeout_ms: 1000,
        }
    }

    #[test]
    fn slip_escapes_its_own_bytes() {
        assert_eq!(
            slip(&[1, END, 2, ESC, 3]),
            [END, 1, ESC, ESC_END, 2, ESC, ESC_ESC, 3, END]
        );
    }

    #[tokio::test]
    async fn udp_sends_one_datagram() {
        let socket = tokio::net::UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let address = socket.local_addr().unwrap().to_string();
        let target = target(TargetKind::OscUdp, std::slice::from_ref(&address));
        let packet = encode_args("/eos/go", &[Arg::Int(1), Arg::Bool(true), Arg::Nil]);
        send(&target, &address, &packet).await.unwrap();
        let mut got = [0u8; 256];
        let (n, _) = socket.recv_from(&mut got).await.unwrap();
        assert_eq!(&got[..n], packet.as_slice());
        let message = &decode(&got[..n])[0];
        assert_eq!(message.address, "/eos/go");
        // T is read back as 1; N carries nothing.
        assert_eq!(message.args, vec![Arg::Int(1), Arg::Int(1)]);
        probe(&target).await.unwrap();
    }

    #[tokio::test]
    async fn tcp_sends_a_slip_frame() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap().to_string();
        let target = target(TargetKind::OscTcp, std::slice::from_ref(&address));
        let packet = encode_args("/go", &[Arg::Str("Szene 2".into())]);
        let read = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut got = Vec::new();
            stream.read_to_end(&mut got).await.unwrap();
            got
        });
        send(&target, &address, &packet).await.unwrap();
        assert_eq!(read.await.unwrap(), slip(&packet));
    }

    #[tokio::test]
    async fn failures_name_the_address() {
        // A port nobody listens on: TCP refuses, the probe says so.
        let free = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = free.local_addr().unwrap().to_string();
        drop(free);
        let tcp = target(TargetKind::OscTcp, std::slice::from_ref(&address));
        let reason = send(&tcp, &address, b"x").await.unwrap_err();
        assert_eq!(reason, format!("{address}: cannot connect"));
        assert!(probe(&tcp).await.is_err());
        let nowhere = "nowhere.invalid:9000".to_string();
        let udp = target(TargetKind::OscUdp, std::slice::from_ref(&nowhere));
        let reason = probe(&udp).await.unwrap_err();
        assert!(reason.starts_with("nowhere.invalid:9000: "), "{reason}");
    }
}
