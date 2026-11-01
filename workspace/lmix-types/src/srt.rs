//! An SRT address as its parts (SRC.12): the URI GStreamer's `srtsrc`
//! reads (`srt://host:port?mode=listener&latency=200&passphrase=…`), taken
//! apart for a form and put together again.
//!
//! ```text
//!   srt://cam.local:9000?mode=caller&latency=200&streamid=%23!%3A%3Ar%3Dcam1
//!         └─ host ─┘ └port┘ └ mode ┘ └latency ┘ └──── stream id ─────┘
//! ```
//!
//! Values in the query are percent-encoded; `srtsrc` decodes them (checked
//! with GStreamer 1.28). Parameters lmix has no field for stay as written.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Shortest and longest passphrase SRT accepts.
pub const SRT_PASSPHRASE_MIN: usize = 10;
pub const SRT_PASSPHRASE_MAX: usize = 79;
/// AES key lengths in bytes; none: the sender's.
pub const SRT_KEY_LENGTHS: [u32; 3] = [16, 24, 32];
/// The receive buffer strom gives an SRT input when none is set (its
/// `latency` default, which overrides one in the URI).
pub const SRT_DEFAULT_LATENCY_MS: u32 = 125;
/// Longest receive buffer and decoder delay lmix sets.
pub const SRT_LATENCY_MAX_MS: u32 = 10_000;
pub const SRT_DECODER_DELAY_MAX_MS: u32 = 10_000;
/// Longest stream id (SRT's limit).
pub const SRT_STREAM_ID_MAX: usize = 512;

/// Who opens the connection.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SrtMode {
    /// lmix calls the sender at host:port (SRT's default).
    #[default]
    Caller,
    /// lmix waits on port for the sender; host: the local address, none: all.
    Listener,
    /// Both sides call each other at the same time (through firewalls).
    Rendezvous,
}

impl SrtMode {
    pub const ALL: [SrtMode; 3] = [SrtMode::Caller, SrtMode::Listener, SrtMode::Rendezvous];

    fn word(self) -> &'static str {
        match self {
            Self::Caller => "caller",
            Self::Listener => "listener",
            Self::Rendezvous => "rendezvous",
        }
    }
}

/// The parts of an SRT input's URI.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct SrtAddress {
    pub mode: SrtMode,
    /// Name or address; IPv6 without brackets. Empty: all local addresses
    /// (listener only).
    pub host: String,
    pub port: u16,
    /// Receive buffer in ms; none: strom's 125.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u32>,
    /// Empty: unencrypted.
    pub passphrase: String,
    /// AES key length in bytes (16, 24, 32); none: the sender's.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_length: Option<u32>,
    /// What the sender or a server is asked for (`#!::r=…`); empty: none.
    pub stream_id: String,
    /// Other query parameters, decoded, in their order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub extra: Vec<(String, String)>,
}

impl SrtAddress {
    /// The parts of `uri`; `Err` says why it is no SRT address.
    pub fn parse(uri: &str) -> Result<Self, String> {
        let uri = uri.trim();
        let rest = uri
            .get(..6)
            .filter(|s| s.eq_ignore_ascii_case("srt://"))
            .map(|_| &uri[6..])
            .ok_or_else(|| "the address starts with srt://".to_string())?;
        let (authority, query) = rest.split_once('?').unwrap_or((rest, ""));
        let authority = authority.trim_end_matches('/');
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let (host, after) = v6
                .split_once(']')
                .ok_or_else(|| "an IPv6 address ends with ]".to_string())?;
            (host, after.strip_prefix(':').unwrap_or(""))
        } else {
            authority.rsplit_once(':').unwrap_or((authority, ""))
        };
        let port = port
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or_else(|| "the address needs a port, 1 to 65535".to_string())?;
        let mut address = SrtAddress {
            host: decode(host),
            port,
            ..Default::default()
        };
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (key, value) = (decode(key), decode(value));
            match key.as_str() {
                "mode" => {
                    address.mode = SrtMode::ALL
                        .into_iter()
                        .find(|m| m.word().eq_ignore_ascii_case(&value))
                        .ok_or_else(|| format!("mode {value}: caller, listener or rendezvous"))?
                }
                "latency" => {
                    address.latency_ms = Some(
                        value
                            .parse()
                            .map_err(|_| format!("latency {value} is no number of ms"))?,
                    )
                }
                "passphrase" => address.passphrase = value,
                "pbkeylen" => {
                    let length: u32 = value
                        .parse()
                        .map_err(|_| format!("key length {value}: 16, 24 or 32"))?;
                    address.key_length = (length != 0).then_some(length);
                }
                "streamid" => address.stream_id = value,
                _ => address.extra.push((key, value)),
            }
        }
        Ok(address)
    }

    /// The URI of these parts, values percent-encoded; parameters at their
    /// defaults (caller, no latency, no passphrase …) left out.
    pub fn uri(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            encode(&self.host)
        };
        let mut query: Vec<(String, String)> = Vec::new();
        if self.mode != SrtMode::Caller {
            query.push(("mode".into(), self.mode.word().into()));
        }
        if let Some(latency) = self.latency_ms {
            query.push(("latency".into(), latency.to_string()));
        }
        if !self.passphrase.is_empty() {
            query.push(("passphrase".into(), self.passphrase.clone()));
        }
        if let Some(length) = self.key_length {
            query.push(("pbkeylen".into(), length.to_string()));
        }
        if !self.stream_id.is_empty() {
            query.push(("streamid".into(), self.stream_id.clone()));
        }
        query.extend(self.extra.iter().cloned());
        let query: Vec<String> = query
            .iter()
            .map(|(k, v)| format!("{}={}", encode(k), encode(v)))
            .collect();
        let mut uri = format!("srt://{host}:{}", self.port);
        if !query.is_empty() {
            uri.push('?');
            uri.push_str(&query.join("&"));
        }
        uri
    }

    /// What strom cannot use, or a passphrase SRT refuses.
    pub fn check(&self) -> Result<(), String> {
        if self.host.is_empty() && self.mode != SrtMode::Listener {
            return Err(format!("a {} needs a host", self.mode.word()));
        }
        if let Some(latency) = self.latency_ms.filter(|l| *l > SRT_LATENCY_MAX_MS) {
            return Err(format!(
                "latency {latency} ms is above {SRT_LATENCY_MAX_MS}"
            ));
        }
        let length = self.passphrase.chars().count();
        if length > 0 && !(SRT_PASSPHRASE_MIN..=SRT_PASSPHRASE_MAX).contains(&length) {
            return Err(format!(
                "a passphrase has {SRT_PASSPHRASE_MIN} to {SRT_PASSPHRASE_MAX} characters"
            ));
        }
        if let Some(length) = self.key_length {
            if !SRT_KEY_LENGTHS.contains(&length) {
                return Err(format!("key length {length}: 16, 24 or 32"));
            }
            if self.passphrase.is_empty() {
                return Err("a key length needs a passphrase".into());
            }
        }
        if self.stream_id.chars().count() > SRT_STREAM_ID_MAX {
            return Err(format!(
                "a stream id has at most {SRT_STREAM_ID_MAX} characters"
            ));
        }
        Ok(())
    }

    /// `srt://host:port` without the query: for names and lists, never
    /// showing a passphrase.
    pub fn public(&self) -> String {
        let host = if self.host.contains(':') {
            format!("[{}]", self.host)
        } else {
            self.host.clone()
        };
        format!("srt://{host}:{}", self.port)
    }
}

/// `uri` without its query (and so without a passphrase), also when it
/// does not read as an SRT address.
pub fn public_uri(uri: &str) -> String {
    match SrtAddress::parse(uri) {
        Ok(address) => address.public(),
        Err(_) => uri.split('?').next().unwrap_or_default().trim().to_string(),
    }
}

/// Percent-encode all but RFC 3986's unreserved characters.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

/// Percent-decode; a `%` without two hex digits stays as it is, and so
/// does `+` (GStreamer reads it as a space only in what it writes itself).
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = (bytes[i] == b'%')
            .then(|| text.get(i + 1..i + 3))
            .flatten()
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match hex {
            Some(byte) => {
                out.push(byte);
                i += 3;
            }
            None => {
                out.push(bytes[i]);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_address_reads_into_its_parts_and_back() {
        let uri = "srt://cam.local:9000?mode=rendezvous&latency=400&passphrase=a%26b%20c%3Dd1234&pbkeylen=32&streamid=%23%21%3A%3Ar%3Dcam1%2Cm%3Drequest&poll-timeout=500";
        let address = SrtAddress::parse(uri).unwrap();
        assert_eq!(
            address,
            SrtAddress {
                mode: SrtMode::Rendezvous,
                host: "cam.local".into(),
                port: 9000,
                latency_ms: Some(400),
                passphrase: "a&b c=d1234".into(),
                key_length: Some(32),
                stream_id: "#!::r=cam1,m=request".into(),
                extra: vec![("poll-timeout".into(), "500".into())],
            }
        );
        assert_eq!(address.uri(), uri);
        assert_eq!(address.check(), Ok(()));
        assert_eq!(address.public(), "srt://cam.local:9000");
    }

    #[test]
    fn defaults_stay_out_of_the_uri() {
        let address = SrtAddress::parse("srt://:9000?mode=listener").unwrap();
        assert_eq!(address.mode, SrtMode::Listener);
        assert_eq!(address.host, "");
        assert_eq!(address.uri(), "srt://:9000?mode=listener");
        assert_eq!(address.check(), Ok(()));
        let caller = SrtAddress::parse("SRT://10.0.0.5:7001/").unwrap();
        assert_eq!(caller.mode, SrtMode::Caller);
        assert_eq!(caller.uri(), "srt://10.0.0.5:7001");
    }

    #[test]
    fn ipv6_hosts_keep_their_brackets() {
        let address = SrtAddress::parse("srt://[fd00::5]:9000?mode=caller").unwrap();
        assert_eq!(address.host, "fd00::5");
        assert_eq!(address.uri(), "srt://[fd00::5]:9000");
        assert_eq!(address.public(), "srt://[fd00::5]:9000");
    }

    #[test]
    fn what_is_no_srt_address_says_why() {
        for (uri, why) in [
            ("http://host:9000", "srt://"),
            ("srt://host", "port"),
            ("srt://host:0", "port"),
            ("srt://host:70000", "port"),
            ("srt://host:9000?mode=push", "mode push"),
            ("srt://host:9000?latency=soon", "latency soon"),
            ("srt://host:9000?pbkeylen=x", "key length"),
        ] {
            let error = SrtAddress::parse(uri).unwrap_err();
            assert!(error.contains(why), "{uri}: {error}");
        }
    }

    #[test]
    fn check_refuses_what_srt_or_strom_would() {
        let base = SrtAddress::parse("srt://host:9000").unwrap();
        let with = |f: fn(&mut SrtAddress)| {
            let mut a = base.clone();
            f(&mut a);
            a.check()
        };
        assert!(with(|a| a.host.clear())
            .unwrap_err()
            .contains("caller needs a host"));
        assert!(with(|a| a.passphrase = "short".into())
            .unwrap_err()
            .contains("10 to 79"));
        assert!(with(|a| a.passphrase = "x".repeat(80)).is_err());
        assert_eq!(with(|a| a.passphrase = "x".repeat(79)), Ok(()));
        assert!(with(|a| a.key_length = Some(32))
            .unwrap_err()
            .contains("needs a passphrase"));
        assert!(with(|a| {
            a.passphrase = "0123456789".into();
            a.key_length = Some(20)
        })
        .is_err());
        assert!(with(|a| a.latency_ms = Some(SRT_LATENCY_MAX_MS + 1)).is_err());
        assert!(with(|a| a.stream_id = "s".repeat(SRT_STREAM_ID_MAX + 1)).is_err());
    }

    #[test]
    fn a_key_length_of_zero_is_the_senders() {
        let address = SrtAddress::parse("srt://host:9000?pbkeylen=0").unwrap();
        assert_eq!(address.key_length, None);
        assert_eq!(address.uri(), "srt://host:9000");
    }

    #[test]
    fn the_public_form_never_shows_the_passphrase() {
        assert_eq!(
            public_uri("srt://host:9000?passphrase=0123456789"),
            "srt://host:9000"
        );
        assert_eq!(public_uri("srt://host?passphrase=0123456789"), "srt://host");
    }

    #[test]
    fn a_lone_percent_stays() {
        let address = SrtAddress::parse("srt://h:1?streamid=50%25+%zz").unwrap();
        assert_eq!(address.stream_id, "50%+%zz");
    }
}
