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

use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

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

    /// Whether `given` is the password (or none is needed).
    pub fn password_ok(&self, given: &str) -> bool {
        self.password
            .as_deref()
            .is_none_or(|p| same(p.as_bytes(), given.as_bytes()))
    }

    /// The password in a request's `Authorization` header, if right.
    fn authorized(&self, request: &Request<Body>) -> bool {
        let Some(password) = &self.password else {
            return true;
        };
        let Some(value) = request
            .headers()
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
        else {
            return false;
        };
        let given = if let Some(token) = value.strip_prefix("Bearer ") {
            token.to_string()
        } else if let Some(basic) = value.strip_prefix("Basic ") {
            let Ok(decoded) = base64::engine::general_purpose::STANDARD.decode(basic.trim()) else {
                return false;
            };
            let text = String::from_utf8_lossy(&decoded).into_owned();
            // "user:password"; the user name does not matter.
            text.split_once(':')
                .map(|(_, p)| p.to_string())
                .unwrap_or_default()
        } else {
            return false;
        };
        same(password.as_bytes(), given.as_bytes())
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
    if access.authorized(&request) {
        return next.run(request).await;
    }
    let path = request.uri().path().to_string();
    if request.method() == Method::GET && access.open_read(&path) {
        return next.run(request).await;
    }
    if request.method() == Method::POST && path == "/api/actions" && !access.open_pages.is_empty() {
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
        assert!(access.authorized(&with("Bearer geheim")));
        assert!(access.authorized(&with(&basic("anyone:geheim"))));
        assert!(!access.authorized(&with(&basic("geheim:wrong"))));
        assert!(!access.authorized(&with("Bearer geheim2")));
        assert!(!access.authorized(&Request::get("/").body(Body::empty()).unwrap()));
        assert!(access.password_ok("geheim") && !access.password_ok("gehei"));
        assert!(Access::default().password_ok("anything"));
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
}
