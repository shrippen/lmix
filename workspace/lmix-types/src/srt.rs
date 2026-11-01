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

/// Why an SRT address cannot be used; the Console names it in its own
/// words (`problem` with the values), the core's refusals in these.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema, thiserror::Error)]
#[serde(tag = "problem", rename_all = "snake_case")]
pub enum SrtProblem {
    #[error("the address starts with srt://")]
    NoSrt,
    #[error("an IPv6 address ends with ]")]
    OpenBracket,
    #[error("the address needs a port, 1 to 65535")]
    NoPort,
    #[error("mode {mode}: caller, listener or rendezvous")]
    BadMode { mode: String },
    #[error("latency {value} is no number of ms")]
    BadLatency { value: String },
    #[error("key length {value}: 16, 24 or 32")]
    BadKeyLength { value: String },
    #[error("a {} needs the address of the other side", .mode.word())]
    NeedsHost { mode: SrtMode },
    #[error("{host} is no host name or IP address")]
    BadHost { host: String },
    #[error("latency {ms} ms is above {max}")]
    LatencyTooHigh { ms: u32, max: u32 },
    #[error("decoder delay {ms} ms is above {max}")]
    DecoderDelayTooHigh { ms: u32, max: u32 },
    #[error("a passphrase has {min} to {max} bytes (umlauts count twice), not {bytes}")]
    PassphraseLength {
        bytes: usize,
        min: usize,
        max: usize,
    },
    #[error("a key length needs a passphrase")]
    KeyNeedsPassphrase,
    #[error("a stream id has at most {max} bytes")]
    StreamIdTooLong { max: usize },
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
    /// The parts of `uri`; `Err` says why it is no SRT address. Without a
    /// `mode`, an address without host is a listener, as `srtsrc` takes it.
    pub fn parse(uri: &str) -> Result<Self, SrtProblem> {
        let uri = uri.trim();
        let rest = uri
            .get(..6)
            .filter(|s| s.eq_ignore_ascii_case("srt://"))
            .map(|_| &uri[6..])
            .ok_or(SrtProblem::NoSrt)?;
        let (authority, query) = rest.split_once('?').unwrap_or((rest, ""));
        let authority = authority.trim_end_matches('/');
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let (host, after) = v6.split_once(']').ok_or(SrtProblem::OpenBracket)?;
            (host, after.strip_prefix(':').unwrap_or(""))
        } else {
            authority.rsplit_once(':').unwrap_or((authority, ""))
        };
        let port = port
            .parse::<u16>()
            .ok()
            .filter(|p| *p > 0)
            .ok_or(SrtProblem::NoPort)?;
        let mut address = SrtAddress {
            host: decode(host),
            port,
            ..Default::default()
        };
        let mut mode = None;
        for pair in query.split('&').filter(|p| !p.is_empty()) {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            let (key, value) = (decode(key), decode(value));
            match key.as_str() {
                "mode" => {
                    mode = Some(
                        SrtMode::ALL
                            .into_iter()
                            .find(|m| m.word().eq_ignore_ascii_case(&value))
                            .ok_or(SrtProblem::BadMode { mode: value })?,
                    )
                }
                "latency" => {
                    address.latency_ms = Some(
                        value
                            .parse()
                            .map_err(|_| SrtProblem::BadLatency { value })?,
                    )
                }
                "passphrase" => address.passphrase = value,
                "pbkeylen" => {
                    let length: u32 = value
                        .parse()
                        .map_err(|_| SrtProblem::BadKeyLength { value })?;
                    address.key_length = (length != 0).then_some(length);
                }
                "streamid" => address.stream_id = value,
                _ => address.extra.push((key, value)),
            }
        }
        address.mode = mode.unwrap_or(if address.host.is_empty() {
            SrtMode::Listener
        } else {
            SrtMode::Caller
        });
        Ok(address)
    }

    /// The host without the brackets an IPv6 address is often typed in
    /// (`[fd00::5]`).
    fn bare_host(&self) -> &str {
        self.host
            .strip_prefix('[')
            .and_then(|h| h.strip_suffix(']'))
            .unwrap_or(&self.host)
    }

    /// The host as it stands in a URI: IPv6 in brackets.
    fn uri_host(&self, encoded: bool) -> String {
        let host = self.bare_host();
        match (host.contains(':'), encoded) {
            (true, true) => format!("[{}]", host.replace('%', "%25")),
            (true, false) => format!("[{host}]"),
            (false, true) => encode(host),
            (false, false) => host.to_string(),
        }
    }

    /// The URI of these parts, values percent-encoded; parameters at their
    /// defaults (caller, no latency, no passphrase …) left out. A caller
    /// without host says so: without `mode` it would read as a listener.
    pub fn uri(&self) -> String {
        let mut query: Vec<(String, String)> = Vec::new();
        if self.mode != SrtMode::Caller || self.bare_host().is_empty() {
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
        let mut uri = format!("srt://{}:{}", self.uri_host(true), self.port);
        if !query.is_empty() {
            uri.push('?');
            uri.push_str(&query.join("&"));
        }
        uri
    }

    /// What strom cannot use, or what SRT refuses: a host that is no name
    /// or address, a passphrase outside 10–79 bytes (SRT counts bytes).
    pub fn check(&self) -> Result<(), SrtProblem> {
        if self.bare_host().is_empty() && self.mode != SrtMode::Listener {
            return Err(SrtProblem::NeedsHost { mode: self.mode });
        }
        if !self.bare_host().is_empty() && !is_host(self.bare_host()) {
            return Err(SrtProblem::BadHost {
                host: self.host.clone(),
            });
        }
        if let Some(ms) = self.latency_ms.filter(|l| *l > SRT_LATENCY_MAX_MS) {
            return Err(SrtProblem::LatencyTooHigh {
                ms,
                max: SRT_LATENCY_MAX_MS,
            });
        }
        let bytes = self.passphrase.len();
        if bytes > 0 && !(SRT_PASSPHRASE_MIN..=SRT_PASSPHRASE_MAX).contains(&bytes) {
            return Err(SrtProblem::PassphraseLength {
                bytes,
                min: SRT_PASSPHRASE_MIN,
                max: SRT_PASSPHRASE_MAX,
            });
        }
        if let Some(length) = self.key_length {
            if !SRT_KEY_LENGTHS.contains(&length) {
                return Err(SrtProblem::BadKeyLength {
                    value: length.to_string(),
                });
            }
            if self.passphrase.is_empty() {
                return Err(SrtProblem::KeyNeedsPassphrase);
            }
        }
        if self.stream_id.len() > SRT_STREAM_ID_MAX {
            return Err(SrtProblem::StreamIdTooLong {
                max: SRT_STREAM_ID_MAX,
            });
        }
        Ok(())
    }

    /// `srt://host:port` without the query: for names and lists, never
    /// showing a passphrase.
    pub fn public(&self) -> String {
        format!("srt://{}:{}", self.uri_host(false), self.port)
    }
}

/// An SRT input as the core takes it: an address that reads and that SRT
/// accepts, and a decoder delay of at most 10 s.
pub fn check_srt(uri: &str, decoder_delay_ms: Option<u32>) -> Result<SrtAddress, SrtProblem> {
    let address = SrtAddress::parse(uri)?;
    address.check()?;
    match decoder_delay_ms {
        Some(ms) if ms > SRT_DECODER_DELAY_MAX_MS => Err(SrtProblem::DecoderDelayTooHigh {
            ms,
            max: SRT_DECODER_DELAY_MAX_MS,
        }),
        _ => Ok(address),
    }
}

/// A host name (letters of any script, ASCII digits, `-`, `_`, `.`:
/// `srtsrc` resolves through GLib, which turns `bücher.example` into
/// Punycode; no `²` and the like),
/// an IPv4 address or an IPv6 address, with a zone after `%` of
/// `A–Z a–z 0–9 _ . -` (an interface name). Not `user@host`, a path, a
/// port or blanks.
fn is_host(host: &str) -> bool {
    if host.contains(':') {
        let (address, zone) = match host.split_once('%') {
            Some((address, zone)) => (address, Some(zone)),
            None => (host, None),
        };
        let zone_ok = zone.is_none_or(|z| {
            !z.is_empty()
                && z.bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        });
        return zone_ok && address.parse::<std::net::Ipv6Addr>().is_ok();
    }
    host.chars()
        .all(|c| c.is_alphabetic() || c.is_ascii_digit() || "-_.".contains(c))
        && !host.starts_with(['.', '-'])
}

/// `uri` without its query (and so without a passphrase), also when it
/// does not read as an SRT address.
pub fn public_uri(uri: &str) -> String {
    match SrtAddress::parse(uri) {
        Ok(address) => address.public(),
        Err(_) => uri.split('?').next().unwrap_or_default().trim().to_string(),
    }
}

/// Query parameters whose values are secrets (`SECRET_KEYS`, any case).
const SECRET_KEYS: [&str; 15] = [
    "passphrase",
    "password",
    "passwd",
    "pass",
    "pwd",
    "token",
    "access_token",
    "auth",
    "authorization",
    "key",
    "api_key",
    "apikey",
    "secret",
    "sig",
    "signature",
];

/// `url` without its secrets, for logs and names: the value of a
/// `passphrase`, `password`, `token`, `key` … parameter (whatever
/// characters it has, up to the next `&`) and a `user:password@` before
/// the host become `…`.
pub fn redact_url(url: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let base = match base.split_once("://") {
        Some((scheme, rest)) => {
            let authority_end = rest.find('/').unwrap_or(rest.len());
            match rest[..authority_end].rfind('@') {
                Some(at) => format!("{scheme}://…{}", &rest[at..]),
                None => base.to_string(),
            }
        }
        None => base.to_string(),
    };
    let Some(query) = query else {
        return base;
    };
    let query: Vec<String> = query
        .split('&')
        .map(|pair| {
            let key = pair.split_once('=').map_or(pair, |(k, _)| k);
            let key_text = decode(key);
            if SECRET_KEYS
                .iter()
                .any(|secret| key_text.eq_ignore_ascii_case(secret))
            {
                format!("{key}=…")
            } else {
                pair.to_string()
            }
        })
        .collect();
    format!("{base}?{}", query.join("&"))
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
            let error = SrtAddress::parse(uri).unwrap_err().to_string();
            assert!(error.contains(why), "{uri}: {error}");
        }
    }

    #[test]
    fn check_refuses_what_srt_or_strom_would() {
        let base = SrtAddress::parse("srt://host:9000").unwrap();
        let with = |f: fn(&mut SrtAddress)| {
            let mut a = base.clone();
            f(&mut a);
            a.check().map_err(|e| e.to_string())
        };
        assert!(with(|a| a.host.clear())
            .unwrap_err()
            .contains("caller needs the address"));
        assert!(with(|a| a.passphrase = "short".into())
            .unwrap_err()
            .contains("10 to 79 bytes"));
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
    fn a_host_is_a_name_or_an_address() {
        let host = |h: &str| {
            SrtAddress {
                host: h.into(),
                port: 9000,
                ..Default::default()
            }
            .check()
        };
        for good in [
            "cam.local",
            "cam-2",
            "10.0.0.5",
            "fd00::5",
            "fe80::1%eth0",
            "a_b",
        ] {
            assert_eq!(host(good), Ok(()), "{good}");
        }
        for bad in [
            "cam.local:9000",
            "srt://cam",
            "cam local",
            "user@cam",
            "cam/x",
            "cam?x",
            "cam#x",
            "fd00::zz",
            " cam",
            ".cam",
        ] {
            assert!(
                matches!(host(bad), Err(SrtProblem::BadHost { .. })),
                "{bad}"
            );
        }
        // Round trip of what the review found: none of these reads back
        // as an address that passes.
        for uri in ["srt://[cam.local:9000]:9000", "srt://cam%20local:9000"] {
            let address = SrtAddress::parse(uri).unwrap();
            assert!(address.check().is_err(), "{uri}");
        }
        // An IPv6 zone keeps its % encoded.
        let zone = SrtAddress::parse("srt://[fe80::1%25eth0]:9000").unwrap();
        assert_eq!(zone.host, "fe80::1%eth0");
        assert_eq!(zone.uri(), "srt://[fe80::1%25eth0]:9000");
    }

    #[test]
    fn a_passphrase_counts_bytes_as_srt_does() {
        let with = |p: &str| {
            SrtAddress {
                host: "cam".into(),
                port: 9000,
                passphrase: p.into(),
                ..Default::default()
            }
            .check()
        };
        // 5 umlauts are 10 bytes: enough; 42 are 84: too many.
        assert_eq!(with("äääää"), Ok(()));
        assert!(matches!(
            with("ääää"),
            Err(SrtProblem::PassphraseLength { bytes: 8, .. })
        ));
        // 75 letters and 2 umlauts: 79 bytes, one more umlaut: 81.
        assert_eq!(with(&format!("{}ää", "x".repeat(75))), Ok(()));
        assert!(matches!(
            with(&format!("{}äää", "x".repeat(75))),
            Err(SrtProblem::PassphraseLength { bytes: 81, .. })
        ));
        assert!(with(&"ä".repeat(42)).is_err());
        assert!(with("ääää")
            .unwrap_err()
            .to_string()
            .contains("umlauts count twice"));
    }

    #[test]
    fn without_mode_or_host_it_listens_as_srtsrc_does() {
        let address = SrtAddress::parse("srt://:9000").unwrap();
        assert_eq!(address.mode, SrtMode::Listener);
        assert_eq!(address.check(), Ok(()));
        assert_eq!(address.uri(), "srt://:9000?mode=listener");
        assert_eq!(
            SrtAddress::parse("srt://:9000?mode=caller")
                .unwrap()
                .check(),
            Err(SrtProblem::NeedsHost {
                mode: SrtMode::Caller
            })
        );
    }

    #[test]
    fn decoder_delay_and_address_checked_together() {
        assert!(check_srt("srt://cam:9000", Some(SRT_DECODER_DELAY_MAX_MS)).is_ok());
        assert!(matches!(
            check_srt("srt://cam:9000", Some(SRT_DECODER_DELAY_MAX_MS + 1)),
            Err(SrtProblem::DecoderDelayTooHigh { .. })
        ));
        assert_eq!(check_srt("srt://cam", None), Err(SrtProblem::NoPort));
    }

    #[test]
    fn logs_never_show_a_passphrase_or_a_password() {
        for (url, shown) in [
            (
                "srt://:1?passphrase=geheim%20x&pbkeylen=32",
                "srt://:1?passphrase=…&pbkeylen=32",
            ),
            // Raw quotes, blanks and ampersand-free tails: all of it goes.
            (
                r#"srt://:1?mode=listener&passphrase=ab"cd efgh"#,
                "srt://:1?mode=listener&passphrase=…",
            ),
            (
                "srt://:1?PASSPHRASE=x&streamid=a",
                "srt://:1?PASSPHRASE=…&streamid=a",
            ),
            ("srt://:1?pass%70hrase=x", "srt://:1?pass%70hrase=…"),
            ("rtsp://user:pw@cam/stream", "rtsp://…@cam/stream"),
            ("rtmp://live.example/app", "rtmp://live.example/app"),
            (
                "rtmp://h/app?username=u&password=p w&Token=t",
                "rtmp://h/app?username=u&password=…&Token=…",
            ),
            (
                "https://ha.local/?access_token=abc",
                "https://ha.local/?access_token=…",
            ),
            ("srt://h:1?pbkeylen=16", "srt://h:1?pbkeylen=16"),
            ("http://host/a@b", "http://host/a@b"),
            ("no secret", "no secret"),
        ] {
            assert_eq!(redact_url(url), shown, "{url}");
        }
    }

    #[test]
    fn hosts_in_other_scripts_brackets_and_zones() {
        let host = |h: &str| SrtAddress {
            host: h.into(),
            port: 9000,
            ..Default::default()
        };
        assert_eq!(host("kamera-büro.local").check(), Ok(()));
        assert!(matches!(
            host("x²").check(),
            Err(SrtProblem::BadHost { .. })
        ));
        // Brackets around nothing are no host: a caller needs one, and the
        // URI keeps the caller's mode (it would read as a listener).
        let mut empty = host("[]");
        assert!(matches!(empty.check(), Err(SrtProblem::NeedsHost { .. })));
        assert_eq!(empty.uri(), "srt://:9000?mode=caller");
        empty.mode = SrtMode::Listener;
        assert_eq!(empty.check(), Ok(()));
        assert_eq!(
            host("kamera-büro.local").uri(),
            "srt://kamera-b%C3%BCro.local:9000"
        );
        // Typed with brackets: the same as without.
        assert_eq!(host("[fd00::5]").check(), Ok(()));
        assert_eq!(host("[fd00::5]").uri(), "srt://[fd00::5]:9000");
        assert_eq!(host("[fd00::5]").public(), "srt://[fd00::5]:9000");
        assert_eq!(host("fe80::1%eth0").check(), Ok(()));
        for bad in [
            "fe80::1%eth 0",
            "fe80::1%",
            "fe80::1%e/x",
            "[cam]x",
            "[fd00::5",
        ] {
            assert!(
                matches!(host(bad).check(), Err(SrtProblem::BadHost { .. })),
                "{bad}"
            );
        }
    }

    #[test]
    fn problems_go_to_the_console_as_tagged_json() {
        let json = serde_json::to_string(&SrtProblem::PassphraseLength {
            bytes: 8,
            min: 10,
            max: 79,
        })
        .unwrap();
        assert_eq!(
            json,
            r#"{"problem":"passphrase_length","bytes":8,"min":10,"max":79}"#
        );
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
