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
//!   and only the actions the page itself sends: `tally` none, `keys` the
//!   shortcuts marked for the web, `mixer` switching (preview, cut,
//!   transitions, FTB, layouts), `titles` title and data actions.
//!
//! OSC (UDP) carries no password: it obeys the local-network rule only.

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
        let titles = self.opened(Page::Titles)
            && (path == "/api/templates"
                || path.starts_with("/api/data/")
                || path.starts_with("/titles/"));
        pages || reads || titles
    }
}

/// Actions an open page sends (checked without the password).
async fn open_action(access: &Access, service: &Service, action: &Action) -> bool {
    let group = serde_json::to_value(action)
        .ok()
        .and_then(|v| v["action"].as_str().map(str::to_string))
        .and_then(|name| lmix_types::action_group(&name));
    let mixer = access.opened(Page::Mixer)
        && matches!(
            action,
            Action::PreviewInput { .. }
                | Action::CutDirect { .. }
                | Action::Cut
                | Action::Transition { .. }
                | Action::FadeToBlack { .. }
                | Action::PreviewLayout { .. }
        );
    let titles = access.opened(Page::Titles) && matches!(group, Some("title" | "data"));
    let keys = access.opened(Page::Keys)
        && match action {
            Action::RunShortcut { target } => {
                let state = service.state().await;
                let text = match target {
                    InputRef::Number(n) => n.to_string(),
                    InputRef::Text(t) => t.clone(),
                };
                state.shortcuts.iter().any(|s| {
                    s.web
                        && (s.id.to_string() == text
                            || matches!(target, InputRef::Number(n) if s.number == *n))
                })
            }
            _ => false,
        };
    mixer || titles || keys
}

/// The rules as middleware for the HTTP API and pages.
pub async fn guard(
    State((access, service)): State<(Arc<Access>, Arc<Service>)>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    request: Request<Body>,
    next: Next,
) -> Response {
    match access.peer(peer.ip()) {
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
            if open_action(&access, &service, &action).await {
                return next
                    .run(Request::from_parts(parts, Body::from(bytes)))
                    .await;
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
