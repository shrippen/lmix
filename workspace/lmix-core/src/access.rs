//! Who may use lmix-core (API.7). Programs on the same machine always may
//! (loopback: the Console, scripts). From the network:
//!
//! - only the local network by default (private and link-local addresses);
//!   `--allow-internet` lifts that (behind a reverse proxy, a VPN);
//! - with `--password` every request needs it: HTTP Basic (any user name;
//!   browsers ask once) or `Authorization: Bearer <password>`; the text
//!   protocol takes `AUTH <password>` as its first line;
//! - `--open-page` makes a browser page usable without the password: the
//!   page, its files and what it reads (state, events, tally, pictures),
//!   and only the actions the page itself sends, listed one by one: `tally`
//!   none, `keys` the shortcuts marked for the web, `mixer` preview, cut,
//!   click and GO of an input, quick play,
//!   transition, FTB, layout preview, `titles` play, next, stop, update and
//!   data sets of titles (never files, data sources or addresses);
//! - a browser may not send requests across sites: a request whose
//!   `Origin` is not the address it was sent to is refused, and on this
//!   machine only `localhost` or an IP address counts as the address (no
//!   DNS rebinding);
//! - a request forwarded by a proxy (`Forwarded`, `X-Forwarded-For`,
//!   `X-Real-IP`) counts as coming from the network even when the proxy is
//!   on this machine.
//!
//! OSC (UDP) carries no password: with one set, it takes messages from
//! this machine only.
//!
//! Access keys (show mode CTL.5, `--access-keys`): more keys, each with a
//! right: `view` (read state and pictures), `control` (run the show: GO,
//! cuts, levels, titles, panic) or `edit` (also change what is prepared:
//! inputs, cue lists, targets, files). The password counts as an `edit`
//! key. `--open-panic` lets panic and hard stop through from the local
//! network without a key (a button by the stage, OSC included). A wrong
//! key makes the next answer to that address wait longer (up to 5 s) and
//! is logged.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::extract::{ConnectInfo, State};
use axum::http::{header, Method, Request, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use clap::ValueEnum;
use lmix_types::{Action, InputRef};

use crate::service::Service;

/// Pages that can be opened without the password.
#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Page {
    Tally,
    Keys,
    Mixer,
    Titles,
}

impl Page {
    fn path(self) -> &'static str {
        match self {
            Page::Tally => "/tally",
            Page::Keys => "/keys",
            Page::Mixer => "/mixer",
            Page::Titles => "/titles",
        }
    }
}

/// The access rules lmix-core was started with.
#[derive(Clone, Debug, Default)]
pub struct Access {
    pub password: Option<String>,
    pub open_pages: Vec<Page>,
    pub internet: bool,
    /// More keys with their rights (CTL.5).
    pub keys: Vec<AccessKey>,
    /// Panic and hard stop from the local network without a key.
    pub open_panic: bool,
    /// Wrong keys per address, for the growing wait.
    pub failures: Failures,
}

/// Wrong keys per address; only [`Access::punish`] and
/// [`Access::forgive`] touch them.
#[derive(Clone, Debug, Default)]
pub struct Failures(Arc<Mutex<HashMap<IpAddr, u32>>>);

/// What a key lets its holder do (CTL.5).
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, serde::Deserialize, serde::Serialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Right {
    /// Read: state, events, tally, pictures.
    View,
    /// Run the show.
    Control,
    /// Also change what is prepared.
    Edit,
}

/// An access key from `--access-keys`.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
pub struct AccessKey {
    /// For the log, e.g. "Companion".
    pub name: String,
    pub key: String,
    pub right: Right,
}

/// Shortest key: a guessable one would be no lock.
const KEY_MIN_CHARS: usize = 12;

/// The keys of `--access-keys`: a JSON list of `{name, key, right}`.
pub fn read_keys(path: &std::path::Path) -> anyhow::Result<Vec<AccessKey>> {
    use anyhow::Context;
    let bytes = std::fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let keys: Vec<AccessKey> =
        serde_json::from_slice(&bytes).with_context(|| format!("parse {}", path.display()))?;
    for key in &keys {
        anyhow::ensure!(
            key.key.chars().count() >= KEY_MIN_CHARS,
            "{}: key {} is shorter than {KEY_MIN_CHARS} characters",
            path.display(),
            key.name
        );
    }
    let mut seen = std::collections::HashSet::new();
    anyhow::ensure!(
        keys.iter().all(|k| seen.insert(k.key.as_str())),
        "{}: a key is there twice",
        path.display()
    );
    Ok(keys)
}

/// The right an action needs: running the show is `control`, changing
/// what is prepared (or files and the machine's settings) is `edit`. No
/// wildcard: a new action does not build until it has its right.
pub fn right_of(action: &Action) -> Right {
    match action {
        Action::AddCueList { .. }
        | Action::AddDataSource { .. }
        | Action::AddGraphics { .. }
        | Action::AddInput { .. }
        | Action::AddLayout { .. }
        | Action::AddOutput { .. }
        | Action::AddShortcut { .. }
        | Action::AppendShow { .. }
        | Action::ExportDatasets { .. }
        | Action::ExportLayoutTemplates { .. }
        | Action::ExportPackage { .. }
        | Action::ImportDatasets { .. }
        | Action::ImportLayoutTemplates { .. }
        | Action::ImportPackage { .. }
        | Action::LoadLayoutTemplate { .. }
        | Action::MoveInput { .. }
        | Action::NewShow
        | Action::OpenShow { .. }
        | Action::RelocateMedia { .. }
        | Action::RemoveCueList { .. }
        | Action::RemoveDataSource { .. }
        | Action::RemoveDataset { .. }
        | Action::RemoveFeed { .. }
        | Action::RemoveGraphics { .. }
        | Action::RemoveInput { .. }
        | Action::RemoveLayout { .. }
        | Action::RemoveLayoutTemplate { .. }
        | Action::RemoveOutput { .. }
        | Action::RemoveShortcut { .. }
        | Action::RemoveTarget { .. }
        | Action::RenameGraphics { .. }
        | Action::RenameInput { .. }
        | Action::ReplaceSource { .. }
        | Action::RestoreInput
        | Action::SaveLayoutTemplate { .. }
        | Action::SaveShow { .. }
        | Action::SetAutoPlay { .. }
        | Action::SetCueList { .. }
        | Action::SetDataAuto { .. }
        | Action::SetEmergencyList { .. }
        | Action::SetScriptFolder { .. }
        | Action::ExportSettings { .. }
        | Action::ImportSettings { .. }
        | Action::ResetSettings
        | Action::SetZone { .. }
        | Action::RemoveZone { .. }
        | Action::SetZonePatch { .. }
        | Action::RemoveZonePatch { .. }
        | Action::SetFeed { .. }
        | Action::SetGoGuard { .. }
        | Action::SetGoRearm { .. }
        | Action::SetInputColor { .. }
        | Action::SetInputPress { .. }
        | Action::SetLayout { .. }
        | Action::SetLock { .. }
        | Action::SetLook { .. }
        | Action::SetMidiTbar { .. }
        | Action::SetOverlay { .. }
        | Action::SetPanicTime { .. }
        | Action::SetPtz { .. }
        | Action::SetQuickPlay { .. }
        | Action::SetShortcut { .. }
        | Action::SetSignalLoss { .. }
        | Action::SetTarget { .. }
        | Action::SetTitleTriggers { .. }
        | Action::SetTransition { .. }
        | Action::SetTriggers { .. }
        | Action::TitleBind { .. } => Right::Edit,
        Action::TransportInput { .. }
        | Action::SeekInput { .. }
        | Action::PlayItem { .. }
        | Action::StartOutput { .. }
        | Action::StopOutput { .. }
        | Action::SetAudio { .. }
        | Action::SetAudioFollow { .. }
        | Action::SetVolume { .. }
        | Action::SetChannelStrip { .. }
        | Action::SetAuxSend { .. }
        | Action::SetAuxBus { .. }
        | Action::SetListen { .. }
        | Action::ClearListen
        | Action::PreviewLayout { .. }
        | Action::CueGo { .. }
        | Action::CueStart { .. }
        | Action::CueStop { .. }
        | Action::CueStandby { .. }
        | Action::CueReset
        | Action::SetActiveCueList { .. }
        | Action::CuePanic { .. }
        | Action::CueHardStop
        | Action::TestTarget { .. }
        | Action::HttpRequest { .. }
        | Action::HaService { .. }
        | Action::RunShortcut { .. }
        | Action::PreviewInput { .. }
        | Action::Cut
        | Action::CutDirect { .. }
        | Action::Transition { .. }
        | Action::QuickPlay { .. }
        | Action::ClickInput { .. }
        | Action::GoInput { .. }
        | Action::FadeToBlack { .. }
        | Action::TBar { .. }
        | Action::OverlayIn { .. }
        | Action::OverlayOut { .. }
        | Action::OverlayOutAll { .. }
        | Action::OverlayLast { .. }
        | Action::OverlayPreview { .. }
        | Action::SetDynamicInput { .. }
        | Action::SetDynamicValue { .. }
        | Action::PtzMove { .. }
        | Action::PtzZoom { .. }
        | Action::PtzFocus { .. }
        | Action::PtzHome { .. }
        | Action::PtzPreset { .. }
        | Action::TitleLoad { .. }
        | Action::TitlePlay { .. }
        | Action::TitleNext { .. }
        | Action::TitleStop { .. }
        | Action::TitleClear { .. }
        | Action::TitleUpdate { .. }
        | Action::TitleAdd { .. }
        | Action::TitleTransform { .. }
        | Action::TitleCall { .. }
        | Action::TitleReport { .. }
        | Action::SaveDataset { .. }
        | Action::ApplyDataset { .. }
        | Action::DatasetNext { .. }
        | Action::DatasetPrevious { .. }
        | Action::DataRow { .. }
        | Action::DataNext { .. }
        | Action::DataPrevious { .. } => Right::Control,
    }
}

/// What a request's `Authorization` says.
#[derive(Debug, PartialEq, Eq)]
enum Auth {
    Right(Right),
    /// None given.
    Missing,
    /// One given that is no key.
    Wrong,
}

/// Wait after the `n`-th wrong key from one address.
fn penalty(n: u32) -> Duration {
    Duration::from_millis(100u64.saturating_mul(1 << n.saturating_sub(1).min(6)))
        .min(Duration::from_secs(5))
}

/// Loopback, private, link-local, unique-local, or the shared range
/// 100.64.0.0/10 that VPNs like Tailscale hand out.
fn local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let [a, b, ..] = v4.octets();
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || (a == 100 && (64..128).contains(&b))
        }
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => local(IpAddr::V4(v4)),
            None => {
                v6.is_loopback()
                    || (v6.segments()[0] & 0xfe00) == 0xfc00
                    || (v6.segments()[0] & 0xffc0) == 0xfe80
            }
        },
    }
}

fn loopback(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V6(v6) => v6
            .to_ipv4_mapped()
            .map_or(v6.is_loopback(), |v4| v4.is_loopback()),
        IpAddr::V4(v4) => v4.is_loopback(),
    }
}

/// Equal without telling how much of it matched by the time taken.
fn same(a: &[u8], b: &[u8]) -> bool {
    let mut diff = a.len() ^ b.len();
    for (i, x) in a.iter().enumerate() {
        diff |= usize::from(x ^ b.get(i).copied().unwrap_or(0));
    }
    diff == 0
}

/// What the rules say about a peer before any password.
#[derive(Debug, PartialEq, Eq)]
pub enum Peer {
    /// The same machine: no password.
    Local,
    /// The network: the password if there is one.
    Network,
    /// Outside the local network without `--allow-internet`.
    Refused,
}

impl Access {
    pub fn peer(&self, address: IpAddr) -> Peer {
        if loopback(address) {
            Peer::Local
        } else if self.internet || local(address) {
            Peer::Network
        } else {
            Peer::Refused
        }
    }

    /// Whether the network needs a key at all.
    pub fn needs_key(&self) -> bool {
        self.password.is_some() || !self.keys.is_empty()
    }

    /// The right `given` grants; without password and keys the local
    /// network may do everything (as before CTL.5).
    pub fn right_of_key(&self, given: &str) -> Option<Right> {
        if !self.needs_key() {
            return Some(Right::Edit);
        }
        let password = self
            .password
            .as_deref()
            .is_some_and(|p| same(p.as_bytes(), given.as_bytes()));
        let key = self
            .keys
            .iter()
            .filter(|k| same(k.key.as_bytes(), given.as_bytes()))
            .map(|k| k.right)
            .max();
        if password {
            Some(Right::Edit)
        } else {
            key
        }
    }

    /// Whether an open page or `--open-panic` lets `action` through.
    pub fn open_for(&self, action: &Action) -> bool {
        self.open_panic && matches!(action, Action::CuePanic { .. } | Action::CueHardStop)
    }

    /// A wrong key from `ip`: wait longer each time, and log it.
    pub async fn punish(&self, ip: IpAddr, what: &str) {
        let n = {
            let mut failures = self.failures.0.lock().expect("failures lock");
            let n = failures.entry(ip).or_default();
            *n = n.saturating_add(1);
            *n
        };
        tracing::warn!(%ip, failures = n, "{what}: wrong key, refused");
        tokio::time::sleep(penalty(n)).await;
    }

    /// A right key from `ip`: the wait starts over.
    pub fn forgive(&self, ip: IpAddr) {
        self.failures.0.lock().expect("failures lock").remove(&ip);
    }

    /// The key in a request's `Authorization` header.
    fn authorized(&self, request: &Request<Body>) -> Auth {
        if !self.needs_key() {
            return Auth::Right(Right::Edit);
        }
        let Some(value) = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        else {
            return Auth::Missing;
        };
        let given = if let Some(token) = value.strip_prefix("Bearer ") {
            token.to_string()
        } else if let Some(basic) = value.strip_prefix("Basic ") {
            let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(basic.trim()) else {
                return Auth::Wrong;
            };
            let text = String::from_utf8_lossy(&decoded).into_owned();
            // "user:password"; the user name does not matter.
            text.split_once(':')
                .map(|(_, p)| p.to_string())
                .unwrap_or_default()
        } else {
            return Auth::Wrong;
        };
        self.right_of_key(&given).map_or(Auth::Wrong, Auth::Right)
    }

    fn opened(&self, page: Page) -> bool {
        self.open_pages.contains(&page)
    }

    /// Reading without the password: a page, its files and what pages read.
    fn open_read(&self, path: &str) -> bool {
        if self.open_pages.is_empty() {
            return false;
        }
        let pages = self.open_pages.iter().any(|p| p.path() == path);
        let reads = matches!(
            path,
            "/" | "/health" | "/api/state" | "/api/events" | "/api/tally"
        ) || path.starts_with("/web/")
            || path.starts_with("/api/pictures/");
        let titles = self.opened(Page::Titles) && path == "/api/templates";
        pages || reads || titles
    }
}

/// Actions an open page sends (checked without the password).
/// The action as it may run, or None. A shortcut run from the keys page
/// is pinned to its id (the list may change before it runs).
async fn open_action(access: &Access, service: &Service, action: &Action) -> Option<Action> {
    if access.open_for(action) {
        return Some(action.clone());
    }
    let mixer = access.opened(Page::Mixer)
        && matches!(
            action,
            Action::PreviewInput { .. }
                | Action::CutDirect { .. }
                | Action::ClickInput { .. }
                | Action::GoInput { .. }
                | Action::QuickPlay { .. }
                | Action::Cut
                | Action::Transition { .. }
                | Action::FadeToBlack { .. }
                | Action::PreviewLayout { .. }
        );
    // What titles.html sends; data sets live in the show, no files.
    let titles = access.opened(Page::Titles)
        && matches!(
            action,
            Action::TitlePlay { .. }
                | Action::TitleNext { .. }
                | Action::TitleStop { .. }
                | Action::TitleUpdate { .. }
                | Action::SaveDataset { .. }
                | Action::ApplyDataset { .. }
        );
    if mixer || titles {
        return Some(action.clone());
    }
    if let (true, Action::RunShortcut { target }) = (access.opened(Page::Keys), action) {
        let state = service.state().await;
        let text = match target {
            InputRef::Number(n) => n.to_string(),
            InputRef::Text(t) => t.clone(),
        };
        return state
            .shortcuts
            .iter()
            .find(|s| {
                s.web
                    && (s.id.to_string() == text
                        || matches!(target, InputRef::Number(n) if s.number == *n))
            })
            .map(|s| Action::RunShortcut {
                target: InputRef::Text(s.id.to_string()),
            });
    }
    None
}

/// Host part of an authority (`host:port`, `[v6]:port`).
fn host_of(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or_default();
    }
    authority.split(':').next().unwrap_or_default()
}

/// `localhost` or an IP address: what a request on this machine is sent
/// to (a name that resolves here may be DNS rebinding).
fn local_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost") || host.parse::<IpAddr>().is_ok()
}

/// Refuses requests across sites: an `Origin` other than the `Host`, and
/// on this machine a `Host` that is a name.
fn cross_site(request: &Request<Body>, local_peer: bool) -> bool {
    let header = |name| request.headers().get(name).and_then(|v| v.to_str().ok());
    let host = header(header::HOST).unwrap_or_default();
    if local_peer && !host.is_empty() && !local_host(host_of(host)) {
        return true;
    }
    match header(header::ORIGIN) {
        // `null` (files, sandboxes) or another site.
        Some(origin) => origin
            .split_once("://")
            .is_none_or(|(_, authority)| !authority.eq_ignore_ascii_case(host)),
        None => false,
    }
}

/// Sent on by a proxy: the peer address is the proxy's, not the client's.
fn forwarded(request: &Request<Body>) -> bool {
    ["forwarded", "x-forwarded-for", "x-real-ip"]
        .iter()
        .any(|h| request.headers().contains_key(*h))
}

/// The rules as middleware for the HTTP API and pages.
pub async fn guard(
    State((access, service)): State<(Arc<Access>, Arc<Service>)>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    let mut seen = access.peer(peer.ip());
    if seen == Peer::Local && forwarded(&request) {
        // A proxy on this machine: the client is elsewhere.
        seen = Peer::Network;
    }
    if cross_site(&request, seen == Peer::Local) {
        return (StatusCode::FORBIDDEN, "requests across sites are refused").into_response();
    }
    match seen {
        Peer::Local => return next.run(request).await,
        Peer::Refused => {
            return (
                StatusCode::FORBIDDEN,
                "lmix answers only on the local network (lmix-core --allow-internet)",
            )
                .into_response();
        }
        Peer::Network => {}
    }
    match access.authorized(&request) {
        Auth::Right(right) => {
            access.forgive(peer.ip());
            return with_right(right, request, next).await;
        }
        Auth::Wrong => {
            access.punish(peer.ip(), "HTTP").await;
            return unauthorized();
        }
        Auth::Missing => {}
    }
    let path = request.uri().path().to_string();
    if request.method() == Method::GET && access.open_read(&path) {
        return next.run(request).await;
    }
    let open = !access.open_pages.is_empty() || access.open_panic;
    if request.method() == Method::POST && path == "/api/actions" && open {
        // Read the action to see whether an open page may send it.
        let (parts, body) = request.into_parts();
        let Ok(bytes) = axum::body::to_bytes(body, 64 * 1024).await else {
            return StatusCode::PAYLOAD_TOO_LARGE.into_response();
        };
        if let Ok(action) = serde_json::from_slice::<Action>(&bytes) {
            if let Some(allowed) = open_action(&access, &service, &action).await {
                let body = serde_json::to_vec(&allowed).expect("action serializes");
                let mut parts = parts;
                parts.headers.remove(header::CONTENT_LENGTH);
                return next.run(Request::from_parts(parts, Body::from(body))).await;
            }
        }
        return unauthorized();
    }
    unauthorized()
}

/// A request with a key of `right`: reading needs `view`, an action its
/// own right (`right_of`), anything else sent `control`.
async fn with_right(right: Right, request: Request<Body>, next: Next) -> Response {
    if right == Right::Edit || request.method() == Method::GET {
        return next.run(request).await;
    }
    let forbidden = |what: &str| {
        (
            StatusCode::FORBIDDEN,
            format!("this key may not {what} (it may {right:?})"),
        )
            .into_response()
    };
    if request.uri().path() != "/api/actions" {
        return match right {
            Right::Control => next.run(request).await,
            _ => forbidden("send this"),
        };
    }
    let (parts, body) = request.into_parts();
    // The API's own limit for a JSON body (axum's default).
    let Ok(bytes) = axum::body::to_bytes(body, 2 * 1024 * 1024).await else {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    };
    // Unreadable actions go on: the API refuses them with its reason.
    if let Ok(action) = serde_json::from_slice::<Action>(&bytes) {
        let needs = right_of(&action);
        if needs > right {
            return forbidden(&format!("run this action (it needs {needs:?})"));
        }
    }
    next.run(Request::from_parts(parts, Body::from(bytes)))
        .await
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        [(
            header::WWW_AUTHENTICATE,
            "Basic realm=\"lmix\", charset=\"UTF-8\"",
        )],
        "password needed",
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peers() {
        let access = Access::default();
        let ip = |s: &str| s.parse::<IpAddr>().unwrap();
        assert_eq!(access.peer(ip("127.0.0.1")), Peer::Local);
        assert_eq!(access.peer(ip("::1")), Peer::Local);
        assert_eq!(access.peer(ip("::ffff:127.0.0.1")), Peer::Local);
        for lan in [
            "192.168.1.20",
            "100.101.5.6",
            "10.0.0.5",
            "172.20.1.1",
            "169.254.3.3",
            "fd00::1",
            "fe80::1",
            "::ffff:192.168.0.9",
        ] {
            assert_eq!(access.peer(ip(lan)), Peer::Network, "{lan}");
        }
        for far in ["8.8.8.8", "2001:db8::1", "172.32.0.1", "100.128.0.1"] {
            assert_eq!(access.peer(ip(far)), Peer::Refused, "{far}");
        }
        let open = Access {
            internet: true,
            ..Access::default()
        };
        assert_eq!(open.peer(ip("8.8.8.8")), Peer::Network);
    }

    #[test]
    fn passwords() {
        let access = Access {
            password: Some("geheim".into()),
            ..Access::default()
        };
        let with = |value: &str| {
            Request::get("/api/state")
                .header(header::AUTHORIZATION, value)
                .body(Body::empty())
                .unwrap()
        };
        let basic = |s: &str| {
            format!(
                "Basic {}",
                base64::engine::general_purpose::STANDARD.encode(s)
            )
        };
        let edit = Auth::Right(Right::Edit);
        assert_eq!(access.authorized(&with("Bearer geheim")), edit);
        assert_eq!(access.authorized(&with(&basic("anyone:geheim"))), edit);
        assert_eq!(
            access.authorized(&with(&basic("geheim:wrong"))),
            Auth::Wrong
        );
        assert_eq!(access.authorized(&with("Bearer geheim2")), Auth::Wrong);
        assert_eq!(
            access.authorized(&Request::get("/").body(Body::empty()).unwrap()),
            Auth::Missing
        );
        assert_eq!(access.right_of_key("geheim"), Some(Right::Edit));
        assert_eq!(access.right_of_key("gehei"), None);
        assert_eq!(
            Access::default().right_of_key("anything"),
            Some(Right::Edit)
        );
    }

    async fn call(
        access: Access,
        from: &str,
        request: Request<Body>,
    ) -> (StatusCode, Option<lmix_types::ShowState>) {
        use axum::extract::connect_info::MockConnectInfo;
        use tower::ServiceExt;
        let service = Service::new(
            2,
            Arc::new(crate::engine::null::NullEngine::default()),
            None,
        );
        service
            .handle(Action::AddShortcut {
                shortcut: lmix_types::ShortcutSpec {
                    label: "Web".into(),
                    key: None,
                    web: true,
                    color: None,
                    midi: None,
                    actions: vec![Action::FadeToBlack { duration_ms: 0 }],
                    all_shows: false,
                },
            })
            .await
            .unwrap();
        let app = crate::api::router(service.clone())
            .layer(axum::middleware::from_fn_with_state(
                (Arc::new(access), service.clone()),
                guard,
            ))
            .layer(MockConnectInfo(SocketAddr::new(
                from.parse().unwrap(),
                50_000,
            )));
        let status = app.oneshot(request).await.unwrap().status();
        (status, Some(service.state().await))
    }

    fn post(body: &str, auth: Option<&str>) -> Request<Body> {
        let mut request =
            Request::post("/api/actions").header(header::CONTENT_TYPE, "application/json");
        if let Some(auth) = auth {
            request = request.header(header::AUTHORIZATION, auth);
        }
        request.body(Body::from(body.to_string())).unwrap()
    }

    #[tokio::test]
    async fn the_guard_lets_through_what_the_rules_say() {
        let rules = || Access {
            password: Some("p".into()),
            open_pages: vec![Page::Keys],
            internet: false,
            ..Access::default()
        };
        let cut = r#"{"action":"fade_to_black","duration_ms":0}"#;
        // This machine: no password.
        assert_eq!(
            call(rules(), "127.0.0.1", post(cut, None)).await.0,
            StatusCode::OK
        );
        // The network: the password, or a web shortcut of the open page.
        assert_eq!(
            call(rules(), "192.168.1.9", post(cut, None)).await.0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            call(rules(), "192.168.1.9", post(cut, Some("Bearer p")))
                .await
                .0,
            StatusCode::OK
        );
        assert_eq!(
            call(
                rules(),
                "192.168.1.9",
                post(r#"{"action":"run_shortcut","target":1}"#, None)
            )
            .await
            .0,
            StatusCode::OK
        );
        let get = |path: &str| Request::get(path).body(Body::empty()).unwrap();
        assert_eq!(
            call(rules(), "192.168.1.9", get("/keys")).await.0,
            StatusCode::OK
        );
        assert_eq!(
            call(rules(), "192.168.1.9", get("/mixer")).await.0,
            StatusCode::UNAUTHORIZED
        );
        // Outside the local network: not at all.
        assert_eq!(
            call(rules(), "8.8.8.8", post(cut, Some("Bearer p")))
                .await
                .0,
            StatusCode::FORBIDDEN
        );
        // No password: the local network as before.
        assert_eq!(
            call(Access::default(), "10.1.1.1", post(cut, None)).await.0,
            StatusCode::OK
        );
    }

    #[tokio::test]
    async fn browsers_and_proxies_cannot_get_around_it() {
        let rules = || Access {
            password: Some("p".into()),
            open_pages: vec![Page::Titles],
            internet: false,
            ..Access::default()
        };
        let ftb = r#"{"action":"fade_to_black","duration_ms":0}"#;
        let with = |headers: &[(&str, &str)], body: &str| {
            let mut request =
                Request::post("/api/actions").header(header::CONTENT_TYPE, "application/json");
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            request.body(Body::from(body.to_string())).unwrap()
        };
        // A proxy on this machine: the client still needs the password.
        assert_eq!(
            call(
                rules(),
                "127.0.0.1",
                with(&[("x-forwarded-for", "8.8.8.8")], ftb)
            )
            .await
            .0,
            StatusCode::UNAUTHORIZED
        );
        // Another site's page, and a name that points here (rebinding).
        assert_eq!(
            call(
                rules(),
                "127.0.0.1",
                with(
                    &[
                        ("host", "127.0.0.1:8200"),
                        ("origin", "https://evil.example")
                    ],
                    ftb
                )
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            call(
                rules(),
                "127.0.0.1",
                with(&[("host", "evil.example:8200")], ftb)
            )
            .await
            .0,
            StatusCode::FORBIDDEN
        );
        // lmix's own pages send their own origin.
        assert_eq!(
            call(
                rules(),
                "127.0.0.1",
                with(
                    &[
                        ("host", "localhost:8200"),
                        ("origin", "http://localhost:8200")
                    ],
                    ftb
                )
            )
            .await
            .0,
            StatusCode::OK
        );
        // The open titles page: title actions, never files.
        let export = r#"{"action":"export_datasets","path":"/tmp/x.csv"}"#;
        assert_eq!(
            call(rules(), "192.168.1.9", with(&[], export)).await.0,
            StatusCode::UNAUTHORIZED
        );
        let source = r#"{"action":"add_data_source","spec":{"kind":"text","path":"/etc/passwd"}}"#;
        assert_eq!(
            call(rules(), "192.168.1.9", with(&[], source)).await.0,
            StatusCode::UNAUTHORIZED
        );
        let save = r#"{"action":"save_dataset","name":"A","data":{"name":"x"}}"#;
        assert_eq!(
            call(rules(), "192.168.1.9", with(&[], save)).await.0,
            StatusCode::OK
        );
        let get = |path: &str| Request::get(path).body(Body::empty()).unwrap();
        assert_eq!(
            call(rules(), "192.168.1.9", get("/api/data/1")).await.0,
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn open_pages_read_only_what_they_need() {
        let access = Access {
            password: Some("p".into()),
            open_pages: vec![Page::Tally],
            ..Access::default()
        };
        for path in [
            "/tally",
            "/web/lmix.js",
            "/api/events",
            "/api/tally",
            "/api/pictures/inputs/1",
        ] {
            assert!(access.open_read(path), "{path}");
        }
        for path in [
            "/mixer",
            "/keys",
            "/api/data/1",
            "/titles/sdk/lmix-title.js",
            "/metrics",
            "/api/shows/inputs",
        ] {
            assert!(!access.open_read(path), "{path}");
        }
        assert!(!Access::default().open_read("/tally"));
    }

    fn keyed() -> Access {
        Access {
            keys: vec![
                AccessKey {
                    name: "Companion".into(),
                    key: "steuern-0123456".into(),
                    right: Right::Control,
                },
                AccessKey {
                    name: "Monitor".into(),
                    key: "ansehen-0123456".into(),
                    right: Right::View,
                },
            ],
            open_panic: true,
            ..Access::default()
        }
    }

    #[test]
    fn actions_need_their_right() {
        assert_eq!(right_of(&Action::Cut), Right::Control);
        assert_eq!(right_of(&Action::CuePanic { ms: None }), Right::Control);
        assert_eq!(right_of(&Action::CueGo { list: None }), Right::Control);
        assert_eq!(right_of(&Action::SetLock { on: false }), Right::Edit);
        assert_eq!(
            right_of(&Action::RemoveInput {
                input: InputRef::Number(1)
            }),
            Right::Edit
        );
        assert_eq!(
            right_of(&Action::OpenShow {
                path: "/x.lmix".into()
            }),
            Right::Edit
        );
        let keys = keyed();
        assert_eq!(keys.right_of_key("steuern-0123456"), Some(Right::Control));
        assert_eq!(keys.right_of_key("steuern-012345"), None);
        assert!(keys.needs_key());
        assert!(penalty(1) < penalty(3) && penalty(30) == Duration::from_secs(5));
    }

    #[tokio::test]
    async fn keys_give_their_rights_and_panic_is_open() {
        let cut = r#"{"action":"fade_to_black","duration_ms":0}"#;
        let add = r#"{"action":"add_input","source":{"kind":"pattern","pattern":"bars"}}"#;
        let panic = r#"{"action":"cue_panic"}"#;
        let lan = "192.168.1.9";
        let status = |access, request| async move { call(access, lan, request).await.0 };

        assert_eq!(
            status(keyed(), post(cut, Some("Bearer steuern-0123456"))).await,
            StatusCode::OK
        );
        assert_eq!(
            status(keyed(), post(add, Some("Bearer steuern-0123456"))).await,
            StatusCode::FORBIDDEN,
            "control may not edit"
        );
        assert_eq!(
            status(keyed(), post(cut, Some("Bearer ansehen-0123456"))).await,
            StatusCode::FORBIDDEN,
            "view may not act"
        );
        let state = Request::get("/api/state")
            .header(header::AUTHORIZATION, "Bearer ansehen-0123456")
            .body(Body::empty())
            .unwrap();
        assert_eq!(status(keyed(), state).await, StatusCode::OK);
        // Panic without a key; nothing else.
        assert_eq!(status(keyed(), post(panic, None)).await, StatusCode::OK);
        assert_eq!(
            status(keyed(), post(cut, None)).await,
            StatusCode::UNAUTHORIZED
        );
        let closed = Access {
            open_panic: false,
            ..keyed()
        };
        assert_eq!(
            status(closed, post(panic, None)).await,
            StatusCode::UNAUTHORIZED
        );
    }

    #[tokio::test(start_paused = true)]
    async fn wrong_keys_wait_longer() {
        let access = keyed();
        let ip: IpAddr = "192.168.1.9".parse().unwrap();
        let t = tokio::time::Instant::now();
        access.punish(ip, "test").await;
        access.punish(ip, "test").await;
        access.punish(ip, "test").await;
        assert_eq!(t.elapsed(), Duration::from_millis(100 + 200 + 400));
        access.forgive(ip);
        let t = tokio::time::Instant::now();
        access.punish(ip, "test").await;
        assert_eq!(
            t.elapsed(),
            Duration::from_millis(100),
            "a right key starts over"
        );
    }
}
