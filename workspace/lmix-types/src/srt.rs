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

const MASK: &str = "…";

/// Endings of a name (property or query key) whose value is a secret, matched
/// on the name in lower case without `-`, `_` and other separators:
/// `passphrase`, `auth_token`, `stream-key`, `streamKey`, `X-Amz-Signature`.
/// `pbkeylen` or `key-int-max` do not end in one, so they stay readable.
const SECRET_ENDINGS: &[&str] = &[
    "key",
    "sig",
    "auth",
    "pass",
    "pwd",
    "passwd",
    "password",
    "passphrase",
    "token",
    "secret",
    "signature",
    "credential",
    "credentials",
    "authorization",
    "cookie",
    "streamid",
    "hash",
    "hmac",
    "md5",
];

/// Values that name an authorization scheme, behind which the secret follows
/// as one more word: `Authorization: Bearer abc`.
const AUTH_SCHEMES: &[&str] = &["bearer", "basic", "digest"];

/// Parameter names (as [`plain_name`] reads them) that may show behind a
/// secret value, in a query (`passphrase=…&latency=120`) or behind a blank
/// in quotes (`"passphrase=… latency=120"`): those of SRT, RIST and
/// GStreamer elements. Any other name there may be part of the secret
/// (`passphrase=a&b=c`) and is masked with the rest.
const KNOWN_PARAMS: &[&str] = &[
    "mode",
    "latency",
    "rcvlatency",
    "peerlatency",
    "pbkeylen",
    "localaddress",
    "localport",
    "adapter",
    "conntimeo",
    "connecttimeout",
    "maxbw",
    "inputbw",
    "oheadbw",
    "payloadsize",
    "transtype",
    "tlpktdrop",
    "nakreport",
    "linger",
    "timeout",
    "polltimeout",
    "waitforconnection",
    "autoreconnect",
    "keeplistening",
    "ipttl",
    "iptos",
    "mss",
    "sndbuf",
    "rcvbuf",
    "lossmaxttl",
    "minversion",
    "enforcedencryption",
    "kmrefreshrate",
    "kmpreannounce",
    "messageapi",
    "packetfilter",
    "groupconnect",
    "cname",
    "bandwidth",
    "buffer",
    "profile",
    "aestype",
    "expires",
    "name",
    "uri",
    "location",
    "sync",
    "async",
    "caps",
];

/// Words that may show as an HTTP or WebSocket path segment, alone or as
/// the parts of a file name (`segment_001.ts`). Anything else in such a path
/// may be the key: Cloudflare Stream publishes to `/<key>/webRTC/publish`.
const PATH_WORDS: &[&str] = &[
    "api",
    "v",
    "whip",
    "whep",
    "webrtc",
    "rtc",
    "publish",
    "play",
    "live",
    "stream",
    "streams",
    "endpoint",
    "resource",
    "resources",
    "session",
    "sessions",
    "ingest",
    "offer",
    "answer",
    "ice",
    "hls",
    "dash",
    "index",
    "playlist",
    "manifest",
    "master",
    "media",
    "video",
    "audio",
    "segment",
    "segments",
    "chunk",
    "chunks",
    "init",
    "app",
    "static",
    "assets",
    "status",
    "health",
    "metrics",
];

/// File extensions of a path word that names a file (`index.m3u8`).
const FILE_EXTENSIONS: &[&str] = &[
    "m3u8", "mpd", "ts", "m4s", "mp4", "m4a", "m4v", "aac", "vtt", "webm", "mkv", "mov", "json",
    "html", "js", "css", "png", "jpg", "ico", "txt", "xml",
];

/// Whether a property or query key names a secret.
pub fn is_secret_name(name: &str) -> bool {
    let plain = plain_name(name);
    SECRET_ENDINGS.iter().any(|end| plain.ends_with(end))
}

/// `name` in lower case, `%xx` decoded, letters and digits only: `Auth_Token`,
/// `auth-token`, `authToken` and `auth%5Ftoken` all read `authtoken`.
fn plain_name(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = String::with_capacity(name.len());
    let mut i = 0;
    while i < bytes.len() {
        let mut byte = bytes[i];
        let hex = |b: u8| (b as char).to_digit(16);
        if byte == b'%' && i + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                byte = (high * 16 + low) as u8;
                i += 2;
            }
        }
        if byte.is_ascii_alphanumeric() {
            out.push(byte.to_ascii_lowercase() as char);
        }
        i += 1;
    }
    out
}

/// What of a URL's path and query may show.
#[derive(Clone, Copy)]
struct Rules {
    /// How many path segments may show.
    segments: usize,
    /// Whether a segment behind those may show when it is a path word
    /// ([`path_word`]).
    words: bool,
    /// Whether a segment that looks like a token ([`token_like`]) is masked.
    tokens: bool,
    /// Whether query pairs may show.
    query: bool,
}

impl Rules {
    /// No limit beyond the plain forms.
    const ANY: Rules = Rules {
        segments: usize::MAX,
        words: true,
        tokens: false,
        query: true,
    };
    /// Plain parts; a token in the path is masked.
    const PLAIN: Rules = Rules {
        tokens: true,
        ..Rules::ANY
    };
    /// The application; the stream key and query are masked.
    const RTMP: Rules = Rules {
        segments: 1,
        words: false,
        tokens: false,
        query: false,
    };
    /// Path words only; any other segment and the query are masked.
    const HTTP: Rules = Rules {
        segments: 0,
        words: true,
        tokens: false,
        query: false,
    };

    /// The rules of a scheme: RTMP for `rtmp*`, path words only for HTTP and
    /// WebSocket (a key in the path is common there), all of a local `file:`
    /// path, else plain.
    fn of(scheme: &str) -> Rules {
        let scheme = scheme.to_ascii_lowercase();
        if scheme.starts_with("rtmp") {
            return Rules::RTMP;
        }
        match scheme.as_str() {
            "http" | "https" | "ws" | "wss" => Rules::HTTP,
            "file" => Rules::ANY,
            _ => Rules::PLAIN,
        }
    }

    /// Whether path segment `n` may show.
    fn shows(self, n: usize, segment: &str) -> bool {
        let plain = segment.bytes().all(|b| plain_byte(b) || b == b'%');
        if segment.is_empty() {
            return true;
        }
        if !plain || (self.tokens && token_like(segment)) {
            return false;
        }
        n < self.segments || (self.words && path_word(segment))
    }
}

/// Whether a path segment is a word of [`PATH_WORDS`], or a file name
/// whose parts (split at `_` and `-`) are such words, each with digits
/// behind it, or numbers: `whip`, `v1`, `index.m3u8`,
/// `chunk-stream0-00001.m4s`. A number alone is no word, and a segment
/// with more than 10 digits in all is none (`live-1234567890-9876543210`
/// may be a numeric key).
fn path_word(segment: &str) -> bool {
    let (stem, ext) = match segment.rsplit_once('.') {
        Some((stem, ext)) => (stem, Some(ext)),
        None => (segment, None),
    };
    if ext.is_some_and(|ext| !FILE_EXTENSIONS.iter().any(|e| ext.eq_ignore_ascii_case(e))) {
        return false;
    }
    if stem.bytes().filter(u8::is_ascii_digit).count() > 10 {
        return false;
    }
    let mut word = false;
    for part in stem.split(['_', '-']) {
        let letters = part.trim_end_matches(|c: char| c.is_ascii_digit());
        if letters.is_empty() && !part.is_empty() {
            continue;
        }
        if !PATH_WORDS.iter().any(|w| letters.eq_ignore_ascii_case(w)) {
            return false;
        }
        word = true;
    }
    word
}

/// Whether a path segment looks like a token: a run of 16 or more letters,
/// digits, `-`, `_` or `%` (between `.` and `~`) with a digit or a capital
/// letter in it. `ch01`, `Streaming`, `my-camera-front-door` and
/// `My%20Video.mp4` do not.
fn token_like(segment: &str) -> bool {
    segment.split(['.', '~']).any(|run| {
        run.len() >= 16
            && run
                .bytes()
                .any(|b| b.is_ascii_digit() || b.is_ascii_uppercase())
    })
}

/// Whether `name` is a URL scheme: a letter, then letters, digits, `+-.`.
fn is_scheme(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_alphabetic()) && name.chars().all(is_scheme_char)
}

fn is_scheme_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')
}

/// A URL as shown, and where masking began.
struct Masked {
    shown: String,
    /// Byte offset in the URL of the first masked part, and in `shown`.
    from: Option<(usize, usize)>,
    /// Whether the URL ends in a masked part.
    tail: bool,
    /// Whether the authority was masked without an `@` in the URL: a
    /// password with a raw blank may go on to an `@` in a later word.
    open_authority: bool,
}

impl Masked {
    fn show(&mut self, part: &str) {
        self.shown.push_str(part);
        if !part.is_empty() {
            self.tail = false;
        }
    }

    fn mask(&mut self, at: usize) {
        self.from.get_or_insert((at, self.shown.len()));
        self.shown.push_str(MASK);
        self.tail = true;
    }
}

/// Mask `raw`, a URL whose scheme ends at `sep` (the `://`), with `rules`,
/// as strom's `redact::url` does.
///
/// Shown are only the scheme, a plain `host[:port]` or `[v6][:port]`, path
/// segments of plain characters (letters, digits, `.-_~%`) and query pairs
/// whose key and value are plain characters (letters, digits, `.-_~`). The
/// value of a secret key is masked, and behind it only pairs whose key is a
/// known parameter or a whole secret name (`KNOWN_PARAMS`) show. The first
/// part that is not shown is masked together with all after it. Userinfo
/// runs to the last `@` before the query and is masked; an `@` in the query
/// (a password with a raw `?`, or a value) masks everything after the
/// scheme. A path segment that looks like a token (16 or more letters,
/// digits, `-_%` with a digit or a capital letter in them) is masked with
/// the rest, except in a `file:` URL. `rtmp*` schemes: the application
/// only, the stream key and query masked;
/// `http`, `https`, `ws`, `wss`: only the origin and path segments that are
/// known words (`PATH_WORDS`, also as a file name: `index.m3u8`,
/// `segment_001.ts`, with at most 10 digits), the query masked. Text that
/// does not start with `scheme://` is masked whole.
///
/// ```text
/// srt://h:9000?mode=caller&passphrase=abc -> srt://h:9000?mode=caller&passphrase=…
/// https://u:p@h/a.mp4?token=abc           -> https://…@h/…
/// https://h/whip/0123abcd/x               -> https://h/whip/…
/// srt://u:p?w=x@h:9000                    -> srt://…
/// ```
fn mask_url(raw: &str, sep: usize, rules: Rules) -> Masked {
    let body = sep + 3;
    let mut m = Masked {
        shown: raw[..body].to_string(),
        from: None,
        tail: false,
        open_authority: false,
    };
    let rest = &raw[body..];
    let query = rest.find('?');
    let before = &rest[..query.unwrap_or(rest.len())];
    if query.is_some_and(|q| rest[q..].contains('@')) {
        m.mask(body);
        return m;
    }

    // An `@` behind a `/` is in the path when a plain host without a port
    // comes first (`rtmp://h/live/a@b`), and ends a password with a raw `/`
    // when it does not (`rtmp://user:p/ss@h`). With a port it may be either
    // (`srt://u:12/34@h`): all is masked.
    let first = &before[..before.find('/').unwrap_or(before.len())];
    let at_sign = before.rfind('@').filter(|&at| {
        at < first.len() || !plain_host(first) || (first.contains(':') && !first.starts_with('['))
    });
    if at_sign.is_some_and(|at| at > first.len() && plain_host(first)) {
        m.mask(body);
        return m;
    }
    let host_at = match at_sign {
        Some(at) => {
            m.mask(body);
            m.shown.push('@');
            at + 1
        }
        None => 0,
    };
    let after = &before[host_at..];
    let slash = after.find('/').unwrap_or(after.len());
    if !plain_host(&after[..slash]) {
        m.mask(body + host_at);
        m.open_authority = host_at == 0;
        return m;
    }
    m.show(&after[..slash]);

    let mut at = body + host_at + slash;
    if let Some(path) = after[slash..].strip_prefix('/') {
        for (n, segment) in path.split('/').enumerate() {
            m.shown.push('/');
            at += 1;
            if !rules.shows(n, segment) {
                m.mask(at);
                return m;
            }
            m.show(segment);
            at += segment.len();
        }
    }

    let Some(q) = query else {
        return m;
    };
    m.shown.push('?');
    let mut at = body + q + 1;
    if !rules.query {
        m.mask(at);
        return m;
    }
    // Behind a secret value only known names show, up to the end: the value
    // may hold a raw `&` (`passphrase=a&b=c`).
    let mut after_secret = false;
    for (n, pair) in rest[q + 1..].split('&').enumerate() {
        if n > 0 {
            m.shown.push('&');
        }
        match pair.split_once('=') {
            Some((key, value))
                if !key.is_empty()
                    && key.bytes().all(plain_byte)
                    && (!after_secret || known_name(key)) =>
            {
                m.show(key);
                m.shown.push('=');
                let value_at = at + key.len() + 1;
                if !value.bytes().all(plain_byte) {
                    m.mask(value_at);
                    return m;
                }
                match is_secret_name(key) {
                    true => {
                        after_secret = true;
                        m.mask(value_at);
                    }
                    false => m.show(value),
                }
            }
            _ if pair.is_empty() => {}
            _ => {
                m.mask(at);
                return m;
            }
        }
        at += pair.len() + 1;
    }
    m
}

/// Whether `name` may show behind a secret value: a known parameter, or a
/// secret ending as a whole name (`passphrase`, `token`), whose value is
/// masked in turn. A longer name only ends in one and may be part of the
/// secret (`a btoken=`).
fn known_name(name: &str) -> bool {
    let plain = plain_name(name);
    KNOWN_PARAMS.contains(&plain.as_str()) || SECRET_ENDINGS.contains(&plain.as_str())
}

/// A byte of a plain path segment, query key or value.
fn plain_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_' | b'~')
}

/// Whether `authority` is `host[:port]` or `[v6%zone][:port]`, port digits
/// only. Anything else may be (part of) a credential.
fn plain_host(authority: &str) -> bool {
    let (host, port) = match authority.strip_prefix('[') {
        Some(v6) => {
            let Some((addr, after)) = v6.split_once(']') else {
                return false;
            };
            let (addr, zone) = addr.split_once('%').unwrap_or((addr, ""));
            let plain = !addr.is_empty()
                && addr
                    .chars()
                    .all(|c| c.is_ascii_hexdigit() || matches!(c, ':' | '.'))
                && zone.bytes().all(plain_byte);
            if !plain {
                return false;
            }
            match after.strip_prefix(':') {
                Some(port) => ("", port),
                None if after.is_empty() => ("", ""),
                None => return false,
            }
        }
        None => authority.split_once(':').unwrap_or((authority, "")),
    };
    host.chars()
        .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
        && port.bytes().all(|b| b.is_ascii_digit())
}

/// Mask every URL and every secret `name=value` or `name: value` in free text:
/// an engine error, a log line, a gst-launch string, a `{:?}` dump. A port of
/// strom's `redact::text` (`workspace/strom/backend/src/redact.rs`), same
/// rules and the same test vectors; only the mask differs (`…`).
///
/// A word is a run of non-blanks; quotes do not end it.
/// - A word with `://` is a URL behind a plain prefix (`uri=`, `String("`,
///   `<`): the URL is masked as strom's `redact::url` does (HTTP shows
///   only its origin and path words), and a trailing `:`, `,`, `)`, quote
///   or the like stays outside it (`srt://h:9000: refused` keeps its
///   host). A URL inside an open quote (`uri="srt://…`, `"uri=srt://…`) is
///   the whole quoted run, blanks included, up to the partner of that
///   quote, when the URL in its word ends masked or an `@` follows. A URL
///   whose authority is masked with no `@` in its word masks on through the
///   last `@` in its line (`srt://u:a b@h`). Any other prefix masks the
///   whole word.
/// - At each `=` or `:` of a word (quotes around the name allowed), a name
///   that ends in a secret ending ([`is_secret_name`]) masks the value: up to
///   the end of the word; when the separator ends the word, the next word
///   behind `=` (`passphrase= abc`) and the rest of the line behind `:`
///   (`auth-token: a b`, a header) up to a blank followed by a known pair
///   or a `!` (`auth-token: a b latency=1`); one more word behind `Bearer`,
///   `Basic`, `Digest`. A value that starts with a quote is the quoted run
///   through its partner (`{"passphrase":"a b","x":1}`).
/// - `user:pass@host` without `//` is masked whole.
/// - A quote that opens in a masked part (`passphrase="two words"`,
///   `passphrase=(string)"a b"`, `\"` in a `{:?}` dump) masks through its
///   partner with as many backslashes before it, or to the end of the text,
///   and on to the end of the partner's word unless the value ends there
///   (`ends_value`: `'a b'\''c'`).
/// - Quotes are followed from the start of the text: a `"`, `'` or `` ` ``
///   at the start of a word or behind a non-letter opens, the same quote with
///   as many backslashes closes, except at the start of a word or value
///   (behind a blank or `([{=:,;`) with a word behind it: in
///   `the '90s … 'passphrase=a b'` the second opens. Whenever masking begins
///   inside an open quote (`"passphrase=a b"`, `'uri=srt://h?passphrase=a
///   b'`, a quote opened in an earlier word), the mask runs through the
///   partner of the innermost one, or to the end of the text; on past it
///   when a letter follows (an apostrophe in the value), to the end of its
///   word when the value does not end there (`'a'\''b'`) or an `@` is in
///   that word. A quote opened in the value that is still open at the
///   partner masks to its own partner or the end of the text, so a shell
///   quote in a quoted value hides the rest
///   (`'passphrase="my"'\''s secret'`). The partner itself and what follows
///   show. Inside the quote
///   the mask ends early at a blank followed by a known pair
///   (`"passphrase=a b latency=120"`, see `pair_start`). A mask that ends
///   inside its URL, with shown parts behind it
///   (`"srt://h?passphrase=…&latency=1 x"`), does not run to the partner.
///
/// Not covered, because nothing tells them from prose: a secret behind a
/// name that has no secret ending or with no `=` or `:` between them; an
/// unquoted value with a raw blank (`passphrase=a b`, `rtmp://h/live/a b`),
/// except a password before the last `@` of its line, unless the word
/// before the blank holds an `@` (`srt://u:a@b c@h`) or ends in digits only
/// (`srt://u:12 34@h` reads as a host and port); a path secret in a URL of
/// another scheme than HTTP and RTMP that is shorter than 16 characters or
/// of small letters only (`rtsp://h/abcdef`); a secret value that holds `&`
/// followed by a known name (`passphrase=a&latency=1`), or in quotes a
/// blank followed by a known pair or a `!` (`"passphrase=a latency=1"`).
/// In strom a property value is no prose: `redact::value` masks a URL in
/// it whole.
///
/// A bug in here must neither abort the caller nor let a secret through: a
/// panic masks the whole text.
pub fn redact_text(s: &str) -> String {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| mask_text(s)))
        .unwrap_or_else(|_| MASK.to_string())
}

/// [`redact_text`] without the guard against a panic.
fn mask_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut at = 0;
    let mut word = Word {
        end: 0,
        at_sign: None,
        sep: None,
        colon: None,
    };
    let mut value_next = Next::No;
    let mut quotes = Quotes::default();
    while let Some(c) = s[at..].chars().next() {
        if c.is_whitespace() {
            out.push(c);
            at += c.len_utf8();
            continue;
        }
        if at >= word.end {
            let end = s[at..]
                .find(char::is_whitespace)
                .map_or(s.len(), |i| at + i);
            let at_sign = s[at..end].rfind('@').map(|i| at + i);
            word = Word {
                end,
                at_sign,
                sep: s[at..end].find("://").map(|i| at + i),
                colon: s[at..end].find(':').map(|i| at + i),
            };
        }
        let step = match value_next {
            Next::No => mask_word(s, at, &mut word, &mut quotes, &mut out),
            next => {
                let w = &s[at..word.end];
                let shown_at = out.len();
                out.push_str(MASK);
                // A header value runs to the end of its line, or to a known
                // pair; a quoted one is the quoted run.
                let end = match next == Next::Line && !starts_quoted(w) {
                    true => {
                        let line = quotes.line_end(s, at);
                        quotes.value_end(s, at, line)
                    }
                    false => word.end,
                };
                let scheme = AUTH_SCHEMES.iter().any(|a| w.eq_ignore_ascii_case(a));
                Step {
                    next: value_masked_end(s, at, end),
                    value_next: match scheme && next == Next::Word {
                        true => Next::Word,
                        false => Next::No,
                    },
                    mask: Some((at, shown_at)),
                    bounded: false,
                }
            }
        };
        (at, value_next) = (step.next, step.value_next);
        // Masking began inside an open quote: the secret runs to its partner.
        let Some((from, shown_at)) = step.mask.filter(|_| !step.bounded) else {
            continue;
        };
        let Some(stop) = quotes.masked_to(s, from) else {
            continue;
        };
        out.truncate(shown_at);
        out.push_str(MASK);
        (at, value_next) = match stop {
            // The partner and what follows show.
            Stop::Partner(close, escapes) => {
                out.push_str(&s[close - escapes..=close]);
                (close + 1, Next::No)
            }
            // The value ended at a blank inside the quote.
            Stop::Before(blank) => (blank.max(step.next), Next::No),
            // Never short of what `mask_word` masked.
            Stop::At(stop) => (stop.max(step.next), Next::No),
        };
    }

    out
}

/// Whether the word after a secret name is its value.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Next {
    No,
    /// `passphrase= abc`, `Bearer abc`: the next word.
    Word,
    /// `auth-token: a b`: the rest of the line.
    Line,
}

/// What [`mask_word`] did: where to go on, whether the next word is a
/// secret value, where masking began (in `s` and in the output), and
/// whether the mask ended inside a URL with shown parts behind it.
struct Step {
    next: usize,
    value_next: Next,
    mask: Option<(usize, usize)>,
    bounded: bool,
}

impl Step {
    fn new(next: usize, value_next: Next, mask: Option<(usize, usize)>) -> Self {
        Step {
            next,
            value_next,
            mask,
            bounded: false,
        }
    }
}

/// Where a mask that began inside an open quote ends ([`Quotes::masked_to`]).
enum Stop {
    /// Right behind the partner, at this offset with as many backslashes.
    Partner(usize, usize),
    /// At this blank inside the quote, which stays open.
    Before(usize),
    /// At this offset.
    At(usize),
}

/// The quotes open in the text read so far, innermost last: a `"`, `'` or
/// `` ` `` at the start of a word or behind a non-letter opens (an
/// apostrophe in `can't` does not), the same quote with as many backslashes
/// before it closes it and any opened inside it, unless it may not close
/// ([`can_close`]).
#[derive(Default)]
struct Quotes {
    open: Vec<(usize, usize)>,
    /// How far the text is read.
    read: usize,
    /// Built on the first lookup.
    index: Option<Index>,
}

/// Where the quotes that may close, blanks, line ends and `@` of the text
/// stand, so that a lookup is a binary search and not a scan: a quote open
/// over many words is asked for its partner and the word behind it once
/// per URL in them.
struct Index {
    /// Each quote as (quote, backslashes right before it, offset), sorted.
    quotes: Vec<(u8, usize, usize)>,
    blanks: Vec<usize>,
    lines: Vec<usize>,
    at_signs: Vec<usize>,
}

impl Index {
    fn new(s: &str) -> Self {
        let mut index = Index {
            quotes: Vec::new(),
            blanks: Vec::new(),
            lines: Vec::new(),
            at_signs: Vec::new(),
        };
        let mut escapes = 0;
        // The last character that is not a backslash.
        let mut last = None;
        for (i, c) in s.char_indices() {
            match c {
                '"' | '\'' | '`' if closes(last, s[i + 1..].chars().next()) => {
                    index.quotes.push((c as u8, escapes, i))
                }
                '@' => index.at_signs.push(i),
                '\n' => {
                    index.lines.push(i);
                    index.blanks.push(i);
                }
                c if c.is_whitespace() => index.blanks.push(i),
                _ => {}
            }
            escapes = if c == '\\' { escapes + 1 } else { 0 };
            if c != '\\' {
                last = Some(c);
            }
        }
        index.quotes.sort_unstable();
        index
    }

    /// The first offset in `sorted` at or behind `from`.
    fn first(sorted: &[usize], from: usize) -> Option<usize> {
        sorted.get(sorted.partition_point(|&i| i < from)).copied()
    }
}

impl Quotes {
    /// How deep a closing quote looks for its opening one.
    const DEPTH: usize = 16;

    /// Read on to `to`.
    fn read_to(&mut self, s: &str, to: usize) {
        let b = s.as_bytes();
        for i in self.read..to {
            if !matches!(b[i], b'"' | b'\'' | b'`') {
                continue;
            }
            let escapes = backslashes_before(b, 0, i);
            let depth = self.open.len();
            let matching = match can_close(s, i) {
                true => self
                    .open
                    .iter()
                    .rev()
                    .take(Self::DEPTH)
                    .position(|&(o, e)| b[o] == b[i] && e == escapes),
                false => None,
            };
            match matching {
                Some(k) => self.open.truncate(depth - 1 - k),
                None => {
                    let after_letter = s[..i]
                        .chars()
                        .next_back()
                        .is_some_and(char::is_alphanumeric);
                    if !after_letter {
                        self.open.push((i, escapes));
                    }
                }
            }
        }
        self.read = self.read.max(to);
    }

    fn index(&mut self, s: &str) -> &Index {
        self.index.get_or_insert_with(|| Index::new(s))
    }

    /// The partner of the open quote at `open`, as [`partner`] finds it: the
    /// backslashes before a later quote never reach back past `open`.
    fn partner(&mut self, s: &str, open: usize, escapes: usize) -> Option<usize> {
        let key = (s.as_bytes()[open], escapes, open + 1);
        let quotes = &self.index(s).quotes;
        let &(quote, e, close) = quotes.get(quotes.partition_point(|&q| q < key))?;
        (quote == key.0 && e == escapes).then_some(close)
    }

    /// The first `@` at or behind `from`.
    fn at_sign_from(&mut self, s: &str, from: usize) -> Option<usize> {
        Index::first(&self.index(s).at_signs, from)
    }

    /// The last `@` in `from..to`.
    fn last_at_sign(&mut self, s: &str, from: usize, to: usize) -> Option<usize> {
        let at_signs = &self.index(s).at_signs;
        let k = at_signs.partition_point(|&i| i < to);
        at_signs[..k].last().copied().filter(|&a| a >= from)
    }

    /// The end of the line that `at` stands in.
    fn line_end(&mut self, s: &str, at: usize) -> usize {
        Index::first(&self.index(s).lines, at).unwrap_or(s.len())
    }

    /// The end of the word that `close` stands in, and whether an `@`
    /// follows in it.
    fn word_from(&mut self, s: &str, close: usize) -> (usize, bool) {
        let end = Index::first(&self.index(s).blanks, close).unwrap_or(s.len());
        (end, self.at_sign_from(s, close).is_some_and(|a| a < end))
    }

    /// Where a secret value that may hold blanks, from `from`, ends before
    /// `end`: at the first blank followed by a known pair ([`pair_start`]),
    /// or `end`.
    fn value_end(&mut self, s: &str, from: usize, end: usize) -> usize {
        let blanks = &self.index(s).blanks;
        let mut k = blanks.partition_point(|&i| i < from);
        while let Some(&blank) = blanks.get(k) {
            if blank >= end {
                break;
            }
            let start = blank + s[blank..].chars().next().map_or(1, char::len_utf8);
            let stop = blanks.get(k + 1).map_or(s.len(), |&b| b).min(end);
            if start < stop && pair_start(&s[start..stop]) {
                return blank;
            }
            k += 1;
        }
        end
    }

    /// The innermost quote open at `at`, and its backslashes. A quote that
    /// closed in a part skipped as masked is dropped here.
    fn open_at(&mut self, s: &str, at: usize) -> Option<(usize, usize)> {
        self.read_to(s, at);
        while let Some(&(open, escapes)) = self.open.last() {
            match self.partner(s, open, escapes) {
                Some(close) if close < at => {
                    self.open.pop();
                }
                _ => return Some((open, escapes)),
            }
        }
        None
    }

    /// Where a mask that begins at `from` ends when a quote is open there:
    /// at a blank before a known pair inside the quote ([`Quotes::value_end`]),
    /// right behind its partner, or the end of `s`. Past the partner the mask
    /// goes on when a quote opened in the value runs past it (`masked_end`),
    /// to the next such quote when a letter follows (`'it's a'`: the partner
    /// was an apostrophe), and to the end of the partner's word when the
    /// value does not end there ([`ends_value`]: `'a'\''b'`, `"a"'b'`) or an
    /// `@` follows in that word (a password with a quote).
    fn masked_to(&mut self, s: &str, from: usize) -> Option<Stop> {
        let (open, escapes) = self.open_at(s, from)?;
        let b = s.as_bytes();
        let Some(close) = self.partner(s, open, escapes) else {
            self.open.clear();
            self.read = b.len();
            return Some(Stop::At(b.len()));
        };
        let blank = self.value_end(s, from, close);
        if blank < close && masked_end(s, from, from, blank) <= blank {
            self.read = self.read.max(blank);
            return Some(Stop::Before(blank));
        }
        self.open.pop();
        let inner = masked_end(s, from, from, close);
        let (close_end, at_after) = self.word_from(s, close);
        let after = s[close + 1..].chars().next();
        // What follows the partner, read as it would be behind a value
        // that ends at the partner; none when the value ends there.
        let behind = if after.is_some_and(char::is_alphanumeric) {
            // An apostrophe: the value goes on to the next one.
            Some(
                masked_end(s, close, close - escapes, close + 1).max(masked_end(
                    s,
                    close + 1,
                    close + 1,
                    close_end,
                )),
            )
        } else if !ends_value(s, close + 1) || at_after {
            Some(masked_end(s, close + 1, close + 1, close_end))
        } else {
            None
        };
        // A quote opened in the value that runs past the partner does not
        // excuse what follows it: `'passphrase="my"'\''s secret'`.
        let stop = match (inner > close, behind) {
            (true, behind) => Stop::At(inner.max(behind.unwrap_or(0))),
            (false, Some(behind)) => Stop::At(behind),
            (false, None) => Stop::Partner(close, escapes),
        };
        self.read = self.read.max(match stop {
            Stop::At(at) => at,
            _ => close + 1,
        });
        Some(stop)
    }
}

/// The word of [`redact_text`] being masked: where it ends, its last `@`,
/// and its first `://` and `:` from where it is read, looked up again only
/// once passed, so that a long word masked in many places is read once.
struct Word {
    end: usize,
    at_sign: Option<usize>,
    sep: Option<usize>,
    colon: Option<usize>,
}

impl Word {
    /// The first `pat` at or behind `at` in the word, from `cached`.
    fn next(
        s: &str,
        at: usize,
        end: usize,
        cached: &mut Option<usize>,
        pat: &str,
    ) -> Option<usize> {
        if cached.is_some_and(|i| i < at) {
            *cached = s[at..end].find(pat).map(|i| at + i);
        }
        *cached
    }
}

/// Mask the word, or the rest of it, that starts at `at` into `out`. Returns
/// where to go on, and whether the next word is a secret value.
fn mask_word(s: &str, at: usize, word: &mut Word, quotes: &mut Quotes, out: &mut String) -> Step {
    let end = word.end;
    let w = &s[at..end];
    let url = Word::next(s, at, end, &mut word.sep, "://").map(|sep| {
        let sep = sep - at;
        let start = w[..sep]
            .char_indices()
            .rev()
            .find(|&(_, c)| !is_scheme_char(c))
            .map_or(0, |(i, c)| i + c.len_utf8());
        (start, sep)
    });

    let names = url.map_or(w, |(start, _)| &w[..start]);
    if let Some(value) = secret_value(names) {
        out.push_str(&w[..value]);
        if value == w.len() {
            let next = match w.ends_with(':') {
                true => Next::Line,
                false => Next::Word,
            };
            return Step::new(end, next, None);
        }
        let mask = Some((at + value, out.len()));
        out.push_str(MASK);
        // `Authorization:Bearer abc`: the secret is the next word.
        if AUTH_SCHEMES
            .iter()
            .any(|a| w[value..].eq_ignore_ascii_case(a))
        {
            return Step::new(end, Next::Word, mask);
        }
        return Step::new(value_masked_end(s, at + value, end), Next::No, mask);
    }

    let Some((start, sep)) = url else {
        // `user:pass@host`, a URL without `//`.
        let colon = Word::next(s, at, end, &mut word.colon, ":");
        if word
            .at_sign
            .is_some_and(|a| a > at && colon.is_some_and(|c| c < a))
        {
            let mask = Some((at, out.len()));
            out.push_str(MASK);
            return Step::new(masked_end(s, at, at, end), Next::No, mask);
        }
        out.push_str(w);
        return Step::new(end, Next::No, None);
    };
    let prefix = &w[..start];
    let scheme = &w[start..sep];
    if !is_scheme(scheme) || !prefix.chars().all(is_prefix_char) {
        let mask = Some((at, out.len()));
        out.push_str(MASK);
        return Step::new(masked_end(s, at, at, end), Next::No, mask);
    }
    out.push_str(prefix);
    let url_at = at + start;
    let sep = sep - start;
    let rules = Rules::of(scheme);

    // A URL in quotes (`uri="srt://…"`, `"uri=srt://…"`, a quote open since
    // an earlier word) is the quoted run up to the partner of the quote
    // (`"srt://h?a=1","b":2`), or to the end of the partner's word when an
    // `@` follows it there (a password with a quote). Behind a blank the run
    // counts when the URL in this word ends masked
    // (`"srt://h?passphrase=a b"`), up to a blank before a known pair, or an
    // `@` follows before the partner (`"srt://u:1 2@h"`); otherwise a clean
    // URL ends with its word, so a quoted message that starts with one stays
    // readable. A quote without a partner masks all to the end.
    let mut stop = end;
    let mut run = false;
    if let Some((open, escapes)) = quotes.open_at(s, url_at) {
        let Some(close) = quotes.partner(s, open, escapes) else {
            out.push_str(&s[url_at..url_at + sep + 3]);
            let mask = Some((url_at + sep + 3, out.len()));
            out.push_str(MASK);
            return Step::new(s.len(), Next::No, mask);
        };
        let (close_end, at_after) = match close < end {
            true => (end, word.at_sign.is_some_and(|a| a > close)),
            false => quotes.word_from(s, close),
        };
        let at_inside = quotes.at_sign_from(s, end).is_some_and(|a| a < close);
        let tail = || {
            mask_url(
                &s[url_at..url_at + url_len(&s[url_at..end], sep)],
                sep,
                rules,
            )
            .tail
        };
        if at_after {
            stop = close_end;
        } else if close < end || at_inside {
            stop = close - escapes;
        } else if tail() {
            stop = quotes.value_end(s, end, close - escapes);
        }
        // Never before the end of the scheme (a stale quote must not cut
        // the URL backwards).
        stop = stop.max(url_at + sep + 3);
        run = stop > end;
    }

    let part = &s[url_at..stop];
    let url_end = url_len(part, sep);
    let m = mask_url(&part[..url_end], sep, rules);
    let shown_at = out.len();
    out.push_str(&m.shown);
    let Some((from, from_shown)) = m.from else {
        out.push_str(&part[url_end..]);
        return Step::new(stop, Next::No, None);
    };
    let mask = Some((url_at + from, shown_at + from_shown));
    let url_stop = url_at + url_end;
    let mut next = masked_end(s, url_at + from, url_at, url_stop);
    // A password with a raw blank runs to the last `@` in the line, unless
    // the quoted run holds it.
    if m.open_authority && (!run || next > stop) {
        let line = quotes.line_end(s, url_stop);
        if let Some(a) = quotes.last_at_sign(s, url_stop, line).filter(|&a| a > next) {
            next = masked_end(s, next, url_at, a);
        }
    }
    if next > url_stop {
        return Step::new(next, Next::No, mask);
    }
    // A masked end may go on behind the punctuation: drop it.
    if !m.tail {
        out.push_str(&part[url_end..]);
    }
    Step {
        next: stop,
        value_next: Next::No,
        mask,
        bounded: !m.tail,
    }
}

/// The length of the URL at the start of `part` without the punctuation
/// behind it ([`is_trailing`]); `sep` is where its `://` starts.
fn url_len(part: &str, sep: usize) -> usize {
    let bracket = part.contains('[');
    let mut len = part.len();
    while len > sep + 3 && is_trailing(part.as_bytes()[len - 1], bracket) {
        len -= 1;
    }
    len
}

/// Characters that may stand before a URL in a word: `uri=`, `String("`,
/// `<`, `(location:`, `\"`.
fn is_prefix_char(c: char) -> bool {
    c.is_ascii_alphanumeric()
        || matches!(
            c,
            '_' | '.' | '-' | '=' | ':' | ',' | '(' | '[' | '{' | '<' | '"' | '\'' | '`' | '\\'
        )
}

/// Punctuation behind a URL that is not part of it: `srt://h:9000:`,
/// `(srt://h)`, `"srt://h",`. A `]` only when the URL has no `[`.
fn is_trailing(b: u8, bracket: bool) -> bool {
    matches!(
        b,
        b':' | b',' | b';' | b'.' | b'!' | b'?' | b')' | b'}' | b'>' | b'"' | b'\'' | b'`' | b'\\'
    ) || (b == b']' && !bracket)
}

/// Where the value of the first secret `name=` or `name:` in `w` starts. The
/// name is the run of name characters before the separator, also in quotes:
/// `"passphrase":`, `\"token\"=`.
fn secret_value(w: &str) -> Option<usize> {
    for (i, c) in w.char_indices() {
        if c != '=' && c != ':' {
            continue;
        }
        let mut name_end = i;
        if let Some(unquoted) = w[..i].strip_suffix(['"', '\'']) {
            name_end = unquoted.trim_end_matches('\\').len();
        }
        let name_start = w[..name_end]
            .char_indices()
            .rev()
            .find(|&(_, c)| !is_name_char(c))
            .map_or(0, |(j, c)| j + c.len_utf8());
        if is_secret_name(&w[name_start..name_end]) {
            return Some(i + 1);
        }
    }
    None
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '%')
}

/// Whether a word starts a new pair behind a secret value that may hold
/// blanks: `name=` or `name:` with a known name or a secret one
/// ([`known_name`]), or a gst-launch link `!`.
fn pair_start(word: &str) -> bool {
    if word == "!" {
        return true;
    }
    let Some(sep) = word.find(['=', ':']) else {
        return false;
    };
    let name = &word[..sep];
    !name.is_empty() && name.chars().all(is_name_char) && known_name(name)
}

/// Whether `w` starts with a quote, behind backslashes.
fn starts_quoted(w: &str) -> bool {
    w.trim_start_matches('\\').starts_with(['"', '\'', '`'])
}

/// The end of a masked secret value from `from` to at least `end`, as
/// [`masked_end`]; a value that starts with a quote is the quoted run
/// through its partner, unless the value goes on there ([`ends_value`]).
fn value_masked_end(s: &str, from: usize, end: usize) -> usize {
    let w = &s[from..];
    match starts_quoted(w) {
        true => {
            let quote = from + w.len() - w.trim_start_matches('\\').len();
            masked_end(s, from, from, quote + 1)
        }
        false => masked_end(s, from, from, end),
    }
}

/// Whether a quoted value ends at `i`, right behind its partner: at the end
/// of the text or a blank, or at `,;)]}` followed by one of these, a quote
/// or a backslash (`"a"}`, `"a", b`, `"a","b"`; not `"a",b`).
fn ends_value(s: &str, i: usize) -> bool {
    let mut chars = s[i..].chars();
    match chars.next() {
        None => true,
        Some(c) if c.is_whitespace() => true,
        Some(c) if ",;)]}".contains(c) => chars
            .next()
            .is_none_or(|d| d.is_whitespace() || ",;)]}\"'`\\".contains(d)),
        _ => false,
    }
}

/// The end of a masked run from `from` to at least `end`: a quote that opens
/// in it masks through its partner, wherever that is, or to the end of `s`,
/// and on to the end of the partner's word unless the value ends there
/// ([`ends_value`]). Backslashes before a quote are counted back to `run_at`.
fn masked_end(s: &str, from: usize, run_at: usize, end: usize) -> usize {
    let b = s.as_bytes();
    let mut end = end;
    let mut i = from;
    // The end of the word that `i` stands in, once looked up.
    let mut word_end = 0;
    while i < end {
        if matches!(b[i], b'"' | b'\'' | b'`') {
            let escapes = backslashes_before(b, run_at, i);
            match partner(s, i, escapes, b.len()) {
                Some(close) => {
                    i = close + 1;
                    end = end.max(i);
                    if !ends_value(s, i) {
                        if i > word_end {
                            word_end = s[i..].find(char::is_whitespace).map_or(s.len(), |k| i + k);
                        }
                        end = end.max(word_end);
                    }
                }
                None => return b.len(),
            }
            continue;
        }
        i += 1;
    }
    end
}

/// How many backslashes stand right before `at`, back to `from`.
fn backslashes_before(b: &[u8], from: usize, at: usize) -> usize {
    b[from..at]
        .iter()
        .rev()
        .take_while(|&&c| c == b'\\')
        .count()
}

/// Whether a quote may close one, with `last` the character before it and
/// its backslashes and `next` the one behind it. A quote that starts a word
/// or a value (at the start of the text, behind a blank or `([{=:,;`) and has
/// a word behind it opens one: in `the '90s … 'passphrase=a b'` and
/// `the '90s uri='srt://…'` the second quote opens.
fn closes(last: Option<char>, next: Option<char>) -> bool {
    let starts_word = last.is_none_or(|c| c.is_whitespace() || "([{=:,;".contains(c));
    let word_follows = next.is_some_and(|c| !c.is_whitespace() && !",;:.!?)]}>\"'`\\".contains(c));
    !(starts_word && word_follows)
}

/// Whether the quote at `i` may close one ([`closes`]).
fn can_close(s: &str, i: usize) -> bool {
    let run = backslashes_before(s.as_bytes(), 0, i);
    closes(s[..i - run].chars().next_back(), s[i + 1..].chars().next())
}

/// The partner of the quote at `open` that has `escapes` backslashes before
/// it: the next same quote with exactly as many (an inner quote is escaped
/// once more) that may close ([`can_close`]), before `end`.
fn partner(s: &str, open: usize, escapes: usize, end: usize) -> Option<usize> {
    let b = s.as_bytes();
    let quote = b[open];
    (open + 1..end).find(|&i| {
        b[i] == quote && backslashes_before(b, open + 1, i) == escapes && can_close(s, i)
    })
}

/// `url` without its secrets, for names and short notes: a `user:password@`
/// before the host (everything up to the last `@` before the query) and the
/// value of each secret parameter ([`is_secret_name`]; whatever characters
/// it has, up to the next `&`) become `…`; an `@` in the query (a password
/// with a raw `?`) masks all after the scheme. Unlike [`redact_text`] it
/// keeps the rest as written, so a name keeps its path; engine errors and
/// log lines go through [`redact_text`].
pub fn redact_url(url: &str) -> String {
    let (base, query) = match url.split_once('?') {
        Some((base, query)) => (base, Some(query)),
        None => (url, None),
    };
    let base = match base.split_once("://") {
        Some((scheme, _)) if query.is_some_and(|q| q.contains('@')) => {
            return format!("{scheme}://{MASK}");
        }
        Some((scheme, rest)) => match rest.rfind('@') {
            Some(at) => format!("{scheme}://{MASK}{}", &rest[at..]),
            None => base.to_string(),
        },
        None => base.to_string(),
    };
    let Some(query) = query else {
        return base;
    };
    let query: Vec<String> = query
        .split('&')
        .map(|pair| {
            let key = pair.split_once('=').map_or(pair, |(k, _)| k);
            if is_secret_name(key) {
                format!("{key}={MASK}")
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
                "srt://:1?PASSPHRASE=…&streamid=…",
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
            // Userinfo runs to the last `@` before the query.
            ("http://host/a@b", "http://…@b"),
            ("srt://u:ab/c\"SEKRET@h", "srt://…@h"),
            ("srt://u:12'SEKRET@h:9000", "srt://…@h:9000"),
            // An `@` in the query: all after the scheme goes.
            ("srt://u:p@ss?w=SEKRET@h", "srt://…"),
            ("srt://h?passphrase=a@b", "srt://…"),
            // Secret endings, not exact names.
            (
                "https://h/x?auth_token=t&x=1",
                "https://h/x?auth_token=…&x=1",
            ),
            (
                "https://h/x?X-Amz-Signature=s",
                "https://h/x?X-Amz-Signature=…",
            ),
            ("no secret", "no secret"),
        ] {
            assert_eq!(redact_url(url), shown, "{url}");
        }
    }

    // Shared test vectors: keep identical in strom `backend/src/redact.rs` and
    // lmix `lmix-types/src/srt.rs`. Every input of every review round, each
    // also checked as a `{:?}` dump and a dump of a dump.

    /// Inputs and the secrets in them that must not survive.
    const LEAKS: &[(&str, &[&str])] = &[
        // An `@` in the query.
        ("https://h/a?foo@bar&token=SEKRET", &["SEKRET"]),
        ("https://h/a?mail@x.de=1&token=SEKRET", &["SEKRET"]),
        ("https://h?my@token=SEKRET", &["SEKRET"]),
        (
            "https://user:p@ss@h?x@y&passphrase=SEKRET",
            &["SEKRET", "p@ss"],
        ),
        ("srt://h?passphrase=SEKRET&x=@", &["SEKRET"]),
        ("srt://h:9000?streamid=#!::u=a@b,r=SEKRET", &["SEKRET"]),
        (
            "srt://192.0.2.1:9000?streamid=#!::u=a@b,r=x&passphrase=SEKRET",
            &["SEKRET"],
        ),
        (
            "srt://192.0.2.1:9000?passphrase=top@SEKRET&latency=120",
            &["SEKRET"],
        ),
        ("srt://[::1]?passphrase=p@SEKRET", &["SEKRET"]),
        ("srt://ü?ä@ö&token=SEKRET", &["SEKRET"]),
        // Userinfo with `@`, `/`, `?`, `[`, quotes, backslashes.
        ("rtmp://u:SEKRET@h/app/KEYX?x@y", &["SEKRET", "KEYX"]),
        ("rtmp://h?a@b/KEYX", &["KEYX"]),
        ("http://h/@/x?token=SEKRET", &["SEKRET"]),
        (
            "srt://user:pa?ss=SEKA@h:9000?passphrase=SEKB",
            &["SEKA", "SEKB"],
        ),
        ("srt://user:SEKRET?R@h:9000", &["SEKRET"]),
        ("srt://u:SEKA?ss=x@h:9000", &["SEKA"]),
        ("srt://u:SEKA/b@h:9000", &["SEKA"]),
        ("srt://user:1234?x=SEKRET@h:9000", &["SEKRET"]),
        ("srt://user:?x=SEKRET@h:9000", &["SEKRET"]),
        ("srt://:12?x=SEKRET@h", &["SEKRET"]),
        ("srt://user:12/w?x=SEKRET@h", &["SEKRET"]),
        ("srt://u:[x]?a=SEKRET@h", &["SEKRET"]),
        ("srt://u:[SEKRET]@h", &["SEKRET"]),
        ("srt://us(SEKRET)er:9000", &["SEKRET"]),
        ("srt://u:p@ss?w=SEKRET@h", &["SEKRET"]),
        ("srt://u:SEKRET@[::1]:9000?x@y", &["SEKRET"]),
        ("srt://ü:SEKA@hö?passphrase=SEKB", &["SEKA", "SEKB"]),
        ("https://u%3ASEKRET@h/x", &["SEKRET"]),
        ("http://u:SEKRET@[fe80::1%eth0]:8080/x", &["SEKRET"]),
        ("turn://user:SEKRET@turn.example.com:3478", &["SEKRET"]),
        ("rtmp://user:p/ss@host/live/KEYX", &["p/ss", "KEYX"]),
        ("rtmp://user:1234?x=SEKRET@h/app/KEYX", &["SEKRET", "KEYX"]),
        (
            "rtmps://alice:SEKRET@example.com/live/KEYX",
            &["SEKRET", "KEYX"],
        ),
        ("srt://user:'SEKRET@h:9000?mode=caller", &["SEKRET"]),
        ("srt://u:12'SEKRET@h:9000", &["SEKRET"]),
        ("uri=srt://u:1'SEKRET@h", &["SEKRET"]),
        ("srt://:'SEKRET@h:9000", &["SEKRET"]),
        ("srt://h:9000'SEKRET@x", &["SEKRET"]),
        ("https://user:\"SEKRET@h/x", &["SEKRET"]),
        ("srt://user:\"SEKRET@h", &["SEKRET"]),
        ("srt://\\SEKRET@h", &["SEKRET"]),
        ("srt://u:`SEKRET@h", &["SEKRET"]),
        ("srt://u:<SEKRET@h", &["SEKRET"]),
        ("srt://u:ab/c\"SEKRET@h", &["SEKRET"]),
        ("String(\"srt://u:12\\\"SEKRET@h:9000\")", &["SEKRET"]),
        ("\"srt://u:pa\"SEKRET@h\"", &["SEKRET"]),
        ("\"srt://user\"SEKRET@h\"", &["SEKRET"]),
        ("uri=\"srt://u:'SEKA SEKB'@h\"", &["SEKA", "SEKB"]),
        ("us@er://x", &["us@"]),
        // Without `//`.
        ("rtmp:/user:SEKRET@host/live/KEYX", &["SEKRET", "KEYX"]),
        ("turn:user:SEKRET@turn.example.com", &["SEKRET"]),
        // Query values and keys.
        ("srt://h:9000?mode=caller&passphrase=SEKRET", &["SEKRET"]),
        ("srt://[::1]:9000?passphrase=SEKRET", &["SEKRET"]),
        ("srt://h:9000?a=1?passphrase=SEKRET", &["SEKRET"]),
        ("https://h/a#x?token=SEKRET", &["SEKRET"]),
        ("https://h/x?token=SEKA%40SEKB", &["SEKA", "SEKB"]),
        ("https://h/x?auth_token=SEKRET", &["SEKRET"]),
        ("https://h/x?X-Amz-Signature=SEKRET&x=1", &["SEKRET"]),
        ("srt://u:p@h:9000?x=1'Y&passphrase=SEKRET", &["SEKRET"]),
        ("uri=srt://h?cb=https://u:SEKRET@x/y", &["SEKRET"]),
        ("https://h/?next=srt://h?passphrase=SEKRET", &["SEKRET"]),
        ("uri=srt://h?passphrase=ab'SEKRET x", &["SEKRET"]),
        ("uri=srt://h?passphrase=\"SEKA SEKB\"", &["SEKA", "SEKB"]),
        (
            "{\"uri\":\"srt://h:1?passphrase=SEKRET\",\"latency\":120}",
            &["SEKRET"],
        ),
        (
            "{\"uri\":\"srt://h?passphrase=ab\\\"SEKRET\",\"x\":1}",
            &["SEKRET"],
        ),
        ("<srt://h:1?streamid=SEKRET>", &["SEKRET"]),
        (
            "Could not open srt://192.0.2.1:9000?passphrase=SEKRET: refused",
            &["SEKRET"],
        ),
        (
            "srtsink uri=srt://h:1?streamid=SEKA passphrase=SEKB latency=120",
            &["SEKA", "SEKB"],
        ),
        // RTMP stream keys.
        ("rtmp://h/live/KEYX", &["KEYX"]),
        ("rtmp://h/live/KEYX: Connection refused", &["KEYX"]),
        ("rtmp://u:SEKA@h/app/KEYX?x=SEKB", &["SEKA", "SEKB", "KEYX"]),
        ("rtmp://ü/ä/KEYX?ö@ü", &["KEYX"]),
        ("rtmp?://h/app/KEYX", &["KEYX"]),
        ("rtmp:/h/KEYX?r=http://x", &["KEYX"]),
        ("String(\"rtmp://h/live/KEYX\")", &["KEYX"]),
        // A URL in quotes is the quoted run, blanks and all.
        ("srtsink uri=\"srt://h:9000?passphrase=SEKA SEKB\" sync=false", &["SEKA", "SEKB"]),
        ("uri='srt://h:9000?mode=caller&passphrase=SEKA SEKB'", &["SEKA", "SEKB"]),
        ("uri=\"srt://u:SEKA SEKB@h:9000\"", &["SEKA", "SEKB"]),
        ("\"srt://u:12 SEKRET@h:9000\"", &["SEKRET"]),
        ("location=\"rtmp://h/live/SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("whip-endpoint=\"https://h/whip/ep?token=SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("\"srt://h:9000?passphrase=SEKA&x=1 SEKB\"", &["SEKA", "SEKB"]),
        ("String(\"srt://h:9000?passphrase=SEKA SEKB\")", &["SEKA", "SEKB"]),
        (
            "{\"srt_uri\": String(\"srt://h:9000?mode=caller&passphrase=SEKA SEKB\")}",
            &["SEKA", "SEKB"],
        ),
        ("x \"srt://h:9000?passphrase=SEKA SEKB", &["SEKA", "SEKB"]),
        (
            "gst-launch-1.0 srtsink \"uri=srt://h:9000?mode=caller&passphrase=SEKA SEKB\"",
            &["SEKA", "SEKB"],
        ),
        ("\"uri=srt://h:9000?passphrase=SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("'location=rtmp://h/live/SEKA SEKB' x", &["SEKA", "SEKB"]),
        ("uri=`srt://h?passphrase=SEKA SEKB`", &["SEKA", "SEKB"]),
        ("uri='srt://h?passphrase=SEKA's SEKB'", &["SEKA", "SEKB"]),
        // A quote around the whole argument holds a secret value too.
        (
            "gst-launch-1.0 srtsrc \"passphrase=SEKA SEKB\" uri=srt://h:9000 ! fakesink",
            &["SEKA", "SEKB"],
        ),
        ("srtsrc 'passphrase=SEKA SEKB' latency=120", &["SEKA", "SEKB"]),
        ("srtsink \"streamid=#!::r=live,passphrase=SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("\"name=out uri=srt://h:9000?passphrase=SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("\"name=out passphrase=SEKA SEKB\" x", &["SEKA", "SEKB"]),
        ("srtsrc 'passphrase=SEKA's SEKB' x", &["SEKA", "SEKB"]),
        ("\"srt://u:pa\"-SEKRET@h\"", &["SEKRET"]),
        // A key in the path of an HTTP endpoint (Cloudflare Stream WHIP).
        (
            "whipclientsink0: Unexpected status 401 for https://customer-abc.cloudflarestream.com/SEKRETKEY0123/webRTC/publish",
            &["SEKRETKEY0123"],
        ),
        ("error sending request for url (https://h/SEKRETKEY/webRTC/publish)", &["SEKRETKEY"]),
        ("wss://h/SEKRETKEY", &["SEKRETKEY"]),
        // Pairs: names, several in a word, a secret name before `://`.
        ("auth_token=SEKRET", &["SEKRET"]),
        ("auth-token=SEKRET", &["SEKRET"]),
        ("stream_key=SEKRET", &["SEKRET"]),
        ("streamKey=SEKRET", &["SEKRET"]),
        ("X-Amz-Signature=SEKRET", &["SEKRET"]),
        ("pass%70hrase=SEKRET", &["SEKRET"]),
        ("pass%77ord=SEKRET", &["SEKRET"]),
        ("(passphrase=SEKRET)", &["SEKRET"]),
        ("latency=120&passphrase=SEKRET", &["SEKRET"]),
        ("mode=caller;passphrase=SEKRET", &["SEKRET"]),
        ("opts=latency=1,streamid=SEKRET", &["SEKRET"]),
        ("a=b,passphrase=SEKRET", &["SEKRET"]),
        ("caps=application/x-srt,passphrase=SEKRET", &["SEKRET"]),
        ("passphrase=SEKRET://x", &["SEKRET"]),
        ("token=ab:SEKRET//x://y", &["SEKRET"]),
        ("{\"details\":\"passphrase=SEKRET\"}", &["SEKRET"]),
        // Quotes and backslashes in values.
        ("passphrase=ab\\SEKRET x", &["SEKRET"]),
        ("passphrase=\\SEKRET", &["SEKRET"]),
        ("passphrase=ab'SEKRET x", &["SEKRET"]),
        ("e: \"passphrase=ab\\\"SEKRET\" x\"", &["SEKRET"]),
        ("passphrase='SEKA\\'SEKB' x", &["SEKA", "SEKB"]),
        ("token=\"SEKA\\\"SEKB\" x", &["SEKA", "SEKB"]),
        ("passphrase=\\\\\"SEKA SEKB\\\\\"", &["SEKA", "SEKB"]),
        (
            "srtsink passphrase=\"two SEKA words\" latency=120",
            &["SEKA"],
        ),
        (
            "{\"e\":\"passphrase=\\\"a\\\\\\\"SEKA\\\" SEKB\"}",
            &["SEKA"],
        ),
        ("passphrase=\"SEKRET", &["SEKRET"]),
        ("x passphrase=\"ü SEKRET ö\" y", &["SEKRET"]),
        ("PassPhrase='SEKA SEKB'", &["SEKA", "SEKB"]),
        ("name=\"v\" passphrase=\"SEKRET\" after \"q\"", &["SEKRET"]),
        ("a passphrase='SEKRET' b 'c' d", &["SEKRET"]),
        ("auth-token='SEKA SEKB'", &["SEKA", "SEKB"]),
        ("passphrase=(string)\"SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("passphrase=ab\"SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("é?passphrase=\"SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("passphrase=\"abc\"SEKRET", &["SEKRET"]),
        ("passphrase= SEKRET", &["SEKRET"]),
        // `name: value` and quoted names.
        ("\"passphrase\"=\"SEKRET\"", &["SEKRET"]),
        ("'passphrase'='SEKRET'", &["SEKRET"]),
        ("{\"passphrase\":\"SEKRET\",\"x\":1}", &["SEKRET"]),
        ("\"passphrase\": \"SEKA SEKB\"", &["SEKA", "SEKB"]),
        ("Foo { passphrase: \"SEKRET\" }", &["SEKRET"]),
        ("Authorization: Bearer SEKRET", &["SEKRET"]),
        ("Authorization:Bearer SEKRET", &["SEKRET"]),
        ("Authorization=Bearer SEKRET", &["SEKRET"]),
        ("srt://h?md5=SEKRET&expires=1", &["SEKRET"]),
        ("srt://h?wowzatokenhash=SEKRET", &["SEKRET"]),
        ("hdnts=exp=1~hmac=SEKRET", &["SEKRET"]),
        // Nothing to leak, nothing may panic.
        ("token=\"", &[]),
        ("token=\\\"", &[]),
        ("a=\"ü", &[]),
        ("ü=\"ä", &[]),
        ("é://x", &[]),
        ("ü@é:ö", &[]),
        ("\"'\\", &[]),
        ("?a@b", &[]),
        ("?@", &[]),
        ("@", &[]),
        ("@?", &[]),
        ("?", &[]),
        ("", &[]),
        ("://", &[]),
        ("?://@", &[]),
        ("h/live/KEYX?cb=http://x", &[]),
        // A mask must not end before the end of the word `mask_word` masked.
        ("srtsrc 'passphrase=SEKA'\\''SEKB' latency=1", &["SEKA", "SEKB"]),
        ("srtsrc 'passphrase=SEKA'\"'\"'SEKB'", &["SEKA", "SEKB"]),
        ("srtsrc \"passphrase=SEKA\"'SEKB'", &["SEKA", "SEKB"]),
        ("'passphrase=SEKA'SEKB'SEKC'", &["SEKA", "SEKB", "SEKC"]),
        // A shell-quoted value with a blank and an apostrophe.
        ("passphrase='SEKA SEKB'\\''SEKC' x", &["SEKA", "SEKB", "SEKC"]),
        (
            "'passphrase=SEKA'\"'\"'SEKB SEKC'\"'\"'SEKD' x",
            &["SEKA", "SEKB", "SEKC", "SEKD"],
        ),
        // A stray quote before a quoted secret.
        ("the '90s and 'passphrase=SEKA SEKB' x", &["SEKA", "SEKB"]),
        ("location=/a/\"x.ts and \"passphrase=SEKA SEKB\" x", &["SEKA", "SEKB"]),
        ("the '90s uri='srt://h?passphrase=SEKA SEKB'", &["SEKA", "SEKB"]),
        ("\"open {\"details\":\"passphrase=SEKA SEKB\"} x", &["SEKA", "SEKB"]),
        ("location=/a/\"x.ts {\"details\":\"token=SEKA SEKB\"}", &["SEKA", "SEKB"]),
        // A raw blank in a password.
        ("srt://u:SEKA SEKB@h:9000", &["SEKA", "SEKB"]),
        ("Could not open srt://u:SEKA SEKB SEKC@h:9000: refused", &["SEKA", "SEKB", "SEKC"]),
        ("srt://u:SEKA's SEKB's SEKC@h", &["SEKA", "SEKB", "SEKC"]),
        ("\"open srt://u:SEKA's SEKB\"SEKC's SEKD@h:9000) done", &["SEKA", "SEKB", "SEKC", "SEKD"]),
        // A raw `&` in a secret value: behind it only known names show.
        ("srt://h?passphrase=SEKA&SEKB=SEKC", &["SEKA", "SEKB", "SEKC"]),
        ("srt://h?passphrase=SEKA&latency=1&SEKB=SEKC", &["SEKA", "SEKB", "SEKC"]),
        ("srt://h?passphrase=SEKA&SEKBtoken=SEKC", &["SEKA", "SEKB", "SEKC"]),
        // A token in the path of another scheme, an `@` in a path.
        ("rtsp://h:554/SEKRET0123456789/stream", &["SEKRET"]),
        ("srt://h:9000/SEKRETabcdefghij1", &["SEKRET"]),
        ("rtsp://u:p@h/live/Abcdefghijklmnop: 404", &["Abcdefghijklmnop"]),
        ("rtmp://h/live/SEKA@SEKB", &["SEKA", "SEKB"]),
        ("https://h/whip/SEKA@SEKB/x", &["SEKA", "SEKB"]),
        // HTTP path words: anything else masks the rest.
        ("https://h/whip/SEKRET/whip", &["SEKRET"]),
        ("https://h/live/SEKRET.m3u8", &["SEKRET"]),
        ("https://h/index.SEKRET", &["SEKRET"]),
        ("https://h/segment_SEKRET.ts", &["SEKRET"]),
        ("https://h/stream-SEKRET/whip", &["SEKRET"]),
        ("https://h/whip/12345678901", &["12345678901"]),
        ("https://h/v12345678901", &["12345678901"]),
        (
            "https://h/live/live-1234567890-9876543210-5555555555",
            &["9876543210", "5555555555"],
        ),
        ("https://h/live/chunk-12345-678901.m4s", &["678901"]),
        ("wss://h/whip/SEKRET", &["SEKRET"]),
        ("https://h/whip?token=SEKRET", &["SEKRET"]),
        // A shell quote in a quoted value: the rest of the text is masked.
        ("srtsrc 'passphrase=\"SEKA\"'\\''s SEKB' x", &["SEKA", "SEKB"]),
        (
            "srtsrc 'passphrase=SEKA\"SEKB\"SEKC'\\'', SEKD' x",
            &["SEKA", "SEKB", "SEKC", "SEKD"],
        ),
        // A header value runs to the end of its line.
        ("auth-token: SEKA SEKB", &["SEKA", "SEKB"]),
        ("x-auth-token: SEKA SEKB, SEKC", &["SEKA", "SEKB", "SEKC"]),
        ("Authorization: Bearer SEKA SEKB", &["SEKA", "SEKB"]),
        // Behind a blank in quotes only a known pair ends a value.
        ("\"passphrase=SEKA SEKB latencyx=1\"", &["SEKA", "SEKB"]),
        ("\"passphrase=SEKA SEKBtoken=x\"", &["SEKA", "SEKB"]),
        ("\"srt://h?passphrase=SEKA SEKB&latency=1\"", &["SEKA", "SEKB"]),
        // A quoted value ends at its partner only where a value ends.
        ("passphrase=\"SEKA\",SEKB x", &["SEKA", "SEKB"]),
        ("{\"passphrase\":\"SEKA\"SEKB}", &["SEKA", "SEKB"]),
        ("'passphrase=SEKA',SEKB x", &["SEKA", "SEKB"]),
        ("\"passphrase=SEKA\"}SEKB", &["SEKA", "SEKB"]),
        ("Foo { passphrase: \"SEKA\"SEKB, x: 1 }", &["SEKA", "SEKB"]),
    ];

    /// Ordinary lines that must come back unchanged.
    const UNCHANGED: &[&str] = &[
        "flow started id=abc name=\"Main\" latency=120",
        "srt://192.0.2.1:9000?mode=caller&latency=120",
        "srt://:9000?mode=listener&latency=200&pbkeylen=16",
        "key-int-max=60 bitrate=4000 pbkeylen=16",
        "Could not connect to srt://h:9000: Connection refused",
        "<srt://h:9000>",
        "pad src (caps=\"video/x-raw, width=1920\")",
        "Element { name: \"srtsink0\", uri: \"srt://h:9000?mode=caller\" }",
        "{\"uri\":\"srt://h:9000?mode=caller\",\"latency\":120}",
        "srt://kamera-büro.local:9000?latency=200",
        "http://[fe80::1%eth0]:8080",
        "Failed to open srt://h:9000?mode=caller&latency=120: Connection refused",
        "rtmp://a.rtmp.youtube.com/live2: Connection refused",
        "srtsink uri=\"srt://h:9000?mode=caller\" sync=false",
        "srtsink \"uri=srt://h:9000?mode=caller\" sync=false",
        "name=\"a\" uri=srt://h:9000 sync=false",
        "can't open srt://h:9000: Connection refused",
        "https://example.com/ refused",
        "rtmps://example.com/live",
        "Loading file: file:///home/user/My%20Video.mp4",
        "mail from user@example.com at 12:30:00 user=alice",
        "state changed to PLAYING",
        // HTTP path words and file names, other paths without tokens.
        "GET https://h:8443/live/index.m3u8: 404 Not Found",
        "https://h/hls/stream0/segment_001.ts not found",
        "https://h/hls/segment_1730000000.ts not found",
        "Element { name: \"whep\", uri: \"https://h/whep/resource\" }",
        "rtsp://h:554/Streaming/Channels/101",
        "rtsp://h/cam/realmonitor?channel=1&subtype=0",
        "file:///home/user/2026-10-31T12-30-00_recording.mp4",
        // A quote that ends behind a blank still closes.
        "{\"a\":\"b \",\"uri\":\"srt://h:9000\"}",
    ];

    /// `s`, `{:?}` of it and `{:?}` of that.
    fn dumps(s: &str) -> [String; 3] {
        let once = format!("{:?}", s);
        let twice = format!("{:?}", once);
        [s.to_string(), once, twice]
    }

    /// Inputs that would be slow on a quadratic path, and odd ones.
    fn stress_inputs() -> Vec<String> {
        let n = 1 << 16;
        let mut inputs: Vec<String> = [
            "a",
            "\"",
            "'",
            "\\",
            "\\\"",
            "=",
            ":",
            "@",
            "a=",
            "x:",
            "passphrase=\"",
            "passphrase=a ",
            "srt://",
            "a://",
            "\"srt://h\"",
            "@?",
            "srt://h?a=1&",
            "ü",
            "😀=",
            "passphrase=😀\"",
        ]
        .iter()
        .map(|unit| unit.repeat(n / unit.len().max(1)))
        .collect();
        inputs.push(format!("passphrase=\"{}", "\\\"".repeat(n)));
        inputs.push(format!("srt://h?{}", "a=1&".repeat(n)));
        inputs
    }

    /// Inputs that once panicked: a quote that closed in a skipped part,
    /// a backtick, more nested quotes than are tracked.
    fn panic_inputs() -> Vec<String> {
        let mut inputs: Vec<String> = [
            "'til now: srtsrc \"passphrase=it's\" ! srtsink uri='srt://h:9000'",
            "the '90s srtsrc \"passphrase=it's x\" ! srtsink 'uri=srt://h:9000'",
            "a `\"passphrase=x` b\" srt://h:9000",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        for quote in ["\"", "'", "`"] {
            let nested: String = (0..20)
                .map(|i| format!(" {}{}a", "\\".repeat(i % 4), quote))
                .collect();
            inputs.push(format!("{} passphrase=x srt://h:9000?a=1 b", nested));
            inputs.push(format!("passphrase={} srt://h:9000 x", nested));
        }
        inputs
    }

    /// `n` pseudo-random gst-like lines of quotes, backslashes, pairs and
    /// URLs, the same on every run.
    fn fuzz_inputs(n: usize) -> impl Iterator<Item = String> {
        const TOKENS: &[&str] = &[
            "\"",
            "'",
            "`",
            "\\",
            " ",
            " ",
            "a",
            "it's",
            "=",
            ":",
            "@",
            "?",
            "&",
            ",",
            ")",
            "!",
            "srt://h:9000",
            "srt://u:p@h",
            "https://h/p?x=1",
            "rtmp://h/live/k",
            "passphrase=",
            "uri=",
            "token:",
            "Bearer ",
            "?passphrase=",
            "ü",
            "\n",
        ];
        let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        (0..n).map(move |_| {
            let len = 1 + next() % 24;
            (0..len)
                .map(|_| TOKENS[(next() % TOKENS.len() as u64) as usize])
                .collect()
        })
    }

    #[test]
    fn shared_vectors_leak_nothing() {
        for (raw, secrets) in LEAKS {
            for input in dumps(raw) {
                let shown = redact_text(&input);
                for secret in *secrets {
                    assert!(!shown.contains(secret), "{input:?} -> {shown:?}");
                }
            }
        }
    }

    #[test]
    fn shared_vectors_keep_ordinary_lines() {
        for line in UNCHANGED {
            for input in dumps(line) {
                assert_eq!(redact_text(&input), input);
            }
        }
    }

    #[test]
    fn long_and_odd_inputs_do_not_panic() {
        for input in stress_inputs() {
            let _ = redact_text(&input);
            let _ = redact_url(&input);
        }
    }

    /// The masking itself must not panic, not only `redact_text` behind its
    /// guard.
    #[test]
    fn fuzzed_inputs_do_not_panic() {
        for input in panic_inputs().into_iter().chain(fuzz_inputs(100_000)) {
            let run = std::panic::catch_unwind(|| {
                let _ = mask_text(&input);
                let _ = redact_url(&input);
            });
            assert!(run.is_ok(), "panicked on {:?}", input);
        }
    }

    /// A quote open over the whole text with URLs in it, quoted and not, or
    /// many quoted URLs in one word: a lookup must not scan the rest of the
    /// text again for each URL. Quadratic, the first took 30 s in a debug
    /// build, the others longer; now each takes well under a second.
    #[test]
    fn long_quoted_runs_take_linear_time() {
        let n = 1 << 18;
        let inputs = [
            format!("\"{}\"", " 'srt://h:9000' srt://h:9000".repeat(n / 28)),
            format!(
                "{:?}",
                format!("\"{}\"", " \"srt://a\" srt://b".repeat(n / 20))
            ),
            "\"srt://h?passphrase=a\",".repeat(n / 23),
            format!("\"{} \"{}", " srt://h".repeat(n / 16), "x".repeat(n / 2)),
        ];
        for input in inputs {
            let start = std::time::Instant::now();
            let _ = mask_text(&input);
            let took = start.elapsed();
            assert!(took.as_secs() < 5, "{:?} for {:?}…", took, &input[..40]);
        }
    }

    #[test]
    fn redact_text_masks_urls_and_pairs() {
        for (text, shown) in [
            (
                "400 Bad Request: {\"error\":\"uri=srt://h:1?passphrase=abcdefghij&latency=120 refused\"}",
                // The mask ends inside the URL: the rest shows.
                "400 Bad Request: {\"error\":\"uri=srt://h:1?passphrase=…&latency=120 refused\"}",
            ),
            (
                "srtsink passphrase=abcdefghij latency=120",
                "srtsink passphrase=… latency=120",
            ),
            (
                "rtsp://user:pw@cam/stream\nnext line",
                "rtsp://…@cam/stream\nnext line",
            ),
            (
                "{\"details\":\"passphrase=abcdefghij\"}",
                // The value is inside the open quote: up to its partner,
                // which shows with what follows.
                "{\"details\":\"passphrase=…\"}",
            ),
            ("<srt://h:1?streamid=feed>", "<srt://h:1?streamid=…"),
            (
                "srtsink passphrase=\"two secret words\" latency=120",
                "srtsink passphrase=… latency=120",
            ),
            ("token='a b' x", "token=… x"),
            (
                "{\"e\":\"passphrase=\\\"a b\\\" x\"}",
                "{\"e\":\"passphrase=…\"}",
            ),
            ("rtmp://h/live/key: refused", "rtmp://h/live/… refused"),
            // A header value runs to the end of its line.
            ("Authorization: Bearer abc next", "Authorization: …"),
            ("no secret  here", "no secret  here"),
        ] {
            assert_eq!(redact_text(text), shown, "{text}");
        }
    }

    /// What follows a masked value shows when it is clearly not part of it.
    #[test]
    fn text_behind_a_mask_stays_readable() {
        for (raw, shown) in [
            // The partner of the quote the value is in, and what follows.
            (
                "{\"details\":\"passphrase=abc\"}",
                "{\"details\":\"passphrase=…\"}",
            ),
            (
                "{\"passphrase\":\"abc\",\"latency\":120}",
                "{\"passphrase\":…,\"latency\":120}",
            ),
            (
                "Foo { passphrase: \"abc\", latency: 120 }",
                "Foo { passphrase: …, latency: 120 }",
            ),
            // A URL mask that ends inside the URL.
            (
                "{\"error\":\"Failed to open srt://h:9000?passphrase=abc&latency=120: Connection refused\"}",
                "{\"error\":\"Failed to open srt://h:9000?passphrase=…&latency=120: Connection refused\"}",
            ),
            (
                "\"srt://u:pw@h:9000 refused\"",
                "\"srt://…@h:9000 refused\"",
            ),
            // A known pair behind a blank in quotes, a `{:?}` dump of a line.
            (
                "\"srtsink passphrase=abc latency=120 ! fakesink\"",
                "\"srtsink passphrase=… latency=120 ! fakesink\"",
            ),
            (
                "'uri=srt://h:9000?passphrase=a b mode=caller'",
                "'uri=srt://h:9000?passphrase=… mode=caller'",
            ),
            ("passphrase: abc latency: 120", "passphrase: … latency: 120"),
            // HTTP path words and file names.
            (
                "whipclientsink0: Unexpected status 401 for https://h/whip/0123abcd/x",
                "whipclientsink0: Unexpected status 401 for https://h/whip/…",
            ),
            (
                "https://h/v1/whip/publish/index.m3u8?token=abc",
                "https://h/v1/whip/publish/index.m3u8?…",
            ),
        ] {
            assert_eq!(redact_text(raw), shown, "{raw}");
        }
    }

    /// The leftovers of the review rounds: each now masked.
    #[test]
    fn former_gaps_are_masked() {
        for (raw, shown) in [
            ("passphrase='a b'\\''c' x", "passphrase=…"),
            (
                "the '90s and 'passphrase=a b' x",
                "the '90s and 'passphrase=…' x",
            ),
            (
                "location=/a/\"x.ts and \"passphrase=a b\" x",
                "location=/a/\"x.ts and \"passphrase=…\" x",
            ),
            ("srt://u:a b@h:9000 refused", "srt://…@h:9000 refused"),
            (
                "srt://h:9000?passphrase=a&b=c",
                "srt://h:9000?passphrase=…&…",
            ),
            (
                "srt://h:9000?passphrase=a&latency=120",
                "srt://h:9000?passphrase=…&latency=120",
            ),
            ("rtsp://h:554/0123456789abcdef0/live", "rtsp://h:554/…"),
            ("auth-token: a b", "auth-token: …"),
            (
                "srtsrc 'passphrase=\"my\"'\\''s secret' x",
                "srtsrc 'passphrase=…",
            ),
            (
                "https://h/live/live-1234567890-9876543210-5555555555",
                "https://h/live/…",
            ),
            ("auth-token: a b latency=1 c", "auth-token: … latency=1 c"),
        ] {
            assert_eq!(redact_text(raw), shown, "{raw}");
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
