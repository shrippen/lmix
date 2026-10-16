//! Output targets of this machine (show mode EXT.1–5).
//!
//! Kept with their secrets in a settings file of their own (`--targets`,
//! readable only by the user), never in the show. Messages go to every
//! address of a target at once (main and backup); the target counts as
//! reached when one of them answers.
//!
//! ```text
//!   cue / shortcut ──http_request "Haus"──► targets.json: Haus = http://ha:8123 + token
//!                                            └─► POST http://ha:8123/api/services/light/turn_on
//! ```

use std::collections::BTreeMap;
use std::time::Duration;

use lmix_types::feed::{Feed, FEEDS, FEED_ARGS, FEED_MESSAGES};
use lmix_types::target::{
    HttpMethod, Target, TargetKind, TargetResult, TargetSpec, TARGETS, TARGET_ADDRESSES,
    TARGET_TIMEOUT_MAX_MS, TARGET_TIMEOUT_MIN_MS,
};
use lmix_types::ActionError;
use serde::{Deserialize, Serialize};

/// A target as the settings file keeps it: with its secret.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TargetConfig {
    pub name: String,
    pub kind: TargetKind,
    pub addresses: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    pub timeout_ms: u32,
}

impl TargetConfig {
    /// As clients see it: no secret.
    pub fn public(&self) -> Target {
        Target {
            name: self.name.clone(),
            kind: self.kind,
            addresses: self.addresses.clone(),
            has_secret: self.secret.is_some(),
            timeout_ms: self.timeout_ms,
        }
    }
}

fn bad(reason: impl Into<String>) -> ActionError {
    ActionError::BadTarget {
        reason: reason.into(),
    }
}

/// Add `spec` to `targets` or replace the one of its name. A secret of
/// `null` keeps the one set, `""` removes it.
pub fn set(targets: &mut Vec<TargetConfig>, spec: TargetSpec) -> Result<(), ActionError> {
    let name = spec.name.trim();
    if name.is_empty() {
        return Err(bad("it needs a name"));
    }
    if spec.addresses.is_empty() || spec.addresses.len() > TARGET_ADDRESSES {
        return Err(bad(format!("1–{TARGET_ADDRESSES} addresses")));
    }
    for address in &spec.addresses {
        let ok = match spec.kind {
            TargetKind::Http => address.starts_with("http://") || address.starts_with("https://"),
            TargetKind::OscUdp | TargetKind::OscTcp => {
                address.rsplit_once(':').is_some_and(|(host, port)| {
                    !host.is_empty() && port.parse::<u16>().is_ok_and(|p| p > 0)
                })
            }
        };
        if !ok {
            let want = match spec.kind {
                TargetKind::Http => "an http:// or https:// URL",
                _ => "host:port",
            };
            return Err(bad(format!("{address} is not {want}")));
        }
    }
    if !(TARGET_TIMEOUT_MIN_MS..=TARGET_TIMEOUT_MAX_MS).contains(&spec.timeout_ms) {
        return Err(bad(format!(
            "the time limit is {TARGET_TIMEOUT_MIN_MS}–{TARGET_TIMEOUT_MAX_MS} ms"
        )));
    }
    let old = targets.iter().position(|t| t.name == name);
    if old.is_none() && targets.len() >= TARGETS {
        return Err(bad(format!("at most {TARGETS} targets")));
    }
    let secret = match spec.secret {
        Some(secret) if secret.is_empty() => None,
        Some(secret) => Some(secret),
        None => old.and_then(|i| targets[i].secret.clone()),
    };
    let config = TargetConfig {
        name: name.to_string(),
        kind: spec.kind,
        addresses: spec
            .addresses
            .iter()
            .map(|a| a.trim_end_matches('/').to_string())
            .collect(),
        secret,
        timeout_ms: spec.timeout_ms,
    };
    match old {
        Some(i) => targets[i] = config,
        None => targets.push(config),
    }
    Ok(())
}

/// The target `name`.
pub fn find<'a>(targets: &'a [TargetConfig], name: &str) -> Result<&'a TargetConfig, ActionError> {
    targets
        .iter()
        .find(|t| t.name == name)
        .ok_or_else(|| ActionError::UnknownTarget {
            name: name.to_string(),
        })
}

/// One HTTP request, for every address of a target.
#[derive(Clone, Debug, PartialEq)]
pub struct Request {
    pub method: HttpMethod,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Option<serde_json::Value>,
}

/// A Home Assistant service call (EXT.4): `light.turn_on` for
/// `light.room` is `POST /api/services/light/turn_on` with
/// `{"entity_id": "light.room", …data}`.
pub fn ha_request(
    service: &str,
    entity: Option<&str>,
    data: Option<&serde_json::Value>,
) -> Result<Request, ActionError> {
    let (domain, call) = service
        .split_once('.')
        .filter(|(d, c)| !d.is_empty() && !c.is_empty() && !c.contains('.'))
        .ok_or_else(|| {
            bad(format!(
                "{service} is not domain.service, e.g. light.turn_on"
            ))
        })?;
    let mut body = match data {
        None => serde_json::Map::new(),
        Some(serde_json::Value::Object(map)) => map.clone(),
        Some(_) => return Err(bad("Home Assistant data is an object")),
    };
    if let Some(entity) = entity {
        body.insert("entity_id".into(), entity.into());
    }
    Ok(Request {
        method: HttpMethod::Post,
        path: format!("/api/services/{domain}/{call}"),
        headers: BTreeMap::new(),
        body: Some(serde_json::Value::Object(body)),
    })
}

/// Requests to every address of `target` at once (EXT.1): reached when
/// one answers with success; otherwise every reason.
pub async fn send(
    http: &reqwest::Client,
    target: &TargetConfig,
    request: &Request,
) -> TargetResult {
    if target.kind != TargetKind::Http {
        return TargetResult::Failed {
            reason: format!("{} speaks OSC; OSC cues come later", target.name),
        };
    }
    let sends = target
        .addresses
        .iter()
        .map(|address| send_one(http, target, address, request));
    let results = futures_util::future::join_all(sends).await;
    if results.iter().any(Result::is_ok) {
        return TargetResult::Ok;
    }
    let reasons: Vec<String> = results.into_iter().filter_map(Result::err).collect();
    TargetResult::Failed {
        reason: reasons.join("; "),
    }
}

async fn send_one(
    http: &reqwest::Client,
    target: &TargetConfig,
    address: &str,
    request: &Request,
) -> Result<(), String> {
    let method = match request.method {
        HttpMethod::Get => reqwest::Method::GET,
        HttpMethod::Post => reqwest::Method::POST,
        HttpMethod::Put => reqwest::Method::PUT,
        HttpMethod::Patch => reqwest::Method::PATCH,
        HttpMethod::Delete => reqwest::Method::DELETE,
    };
    let url = format!("{address}{}", request.path);
    let mut builder = http
        .request(method, &url)
        .timeout(Duration::from_millis(u64::from(target.timeout_ms)));
    if let Some(secret) = &target.secret {
        builder = builder.bearer_auth(secret);
    }
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    builder = match &request.body {
        None => builder,
        Some(serde_json::Value::String(text)) => builder
            .header(reqwest::header::CONTENT_TYPE, "text/plain; charset=utf-8")
            .body(text.clone()),
        Some(json) => builder.json(json),
    };
    let response = builder
        .send()
        .await
        .map_err(|e| format!("{url}: {}", reason(&e)))?;
    let status = response.status();
    if status.is_success() || status.is_redirection() {
        return Ok(());
    }
    Err(format!("{url}: HTTP {status}"))
}

/// Short reason of a failed request (reqwest's chain is long).
fn reason(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        return "no answer in time".into();
    }
    if e.is_connect() {
        return "cannot connect".into();
    }
    e.to_string()
}

/// A probe of `target` (EXT.5): `GET` of its first address.
pub async fn probe(http: &reqwest::Client, target: &TargetConfig) -> Result<(), String> {
    let request = Request {
        method: HttpMethod::Get,
        path: String::new(),
        headers: BTreeMap::new(),
        body: None,
    };
    let first = TargetConfig {
        addresses: target.addresses.iter().take(1).cloned().collect(),
        ..target.clone()
    };
    match send(http, &first, &request).await {
        TargetResult::Ok => Ok(()),
        TargetResult::Failed { reason } => Err(reason),
    }
}

/// The settings file: targets and the fixed OSC subscribers (CTL.4)
/// that send to them.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
    #[serde(default)]
    pub feeds: Vec<Feed>,
}

pub fn parse(bytes: &[u8]) -> Result<Settings, String> {
    serde_json::from_slice(bytes).map_err(|e| e.to_string())
}

pub fn to_bytes(settings: &Settings) -> Vec<u8> {
    serde_json::to_vec_pretty(settings).expect("targets serialize")
}

/// `settings` to carry to another machine: no secrets (SSH.5, EXT.2).
pub fn without_secrets(settings: &Settings) -> Settings {
    Settings {
        targets: settings
            .targets
            .iter()
            .map(|t| TargetConfig {
                secret: None,
                ..t.clone()
            })
            .collect(),
        feeds: settings.feeds.clone(),
    }
}

/// Take `from` into `into`: targets and subscribers of the same name are
/// replaced, a target keeps its secret; within the limits.
pub fn merge(into: &mut Settings, from: Settings) -> Result<(), ActionError> {
    for target in from.targets {
        let spec = TargetSpec {
            name: target.name,
            kind: target.kind,
            addresses: target.addresses,
            secret: None,
            timeout_ms: target.timeout_ms,
        };
        set(&mut into.targets, spec)?;
    }
    for feed in from.feeds {
        set_feed(&mut into.feeds, feed)?;
    }
    Ok(())
}

/// Add `feed` to `feeds` or replace the one of its name.
pub fn set_feed(feeds: &mut Vec<Feed>, feed: Feed) -> Result<(), ActionError> {
    if feed.name.trim().is_empty() || feed.target.trim().is_empty() {
        return Err(bad("a subscriber needs a name and a target"));
    }
    if feed.messages.is_empty() || feed.messages.len() > FEED_MESSAGES {
        return Err(bad(format!("1–{FEED_MESSAGES} messages")));
    }
    for message in &feed.messages {
        if !message.address.starts_with('/') {
            return Err(bad(format!("{} is no OSC address", message.address)));
        }
        if message.args.len() > FEED_ARGS {
            return Err(bad(format!("at most {FEED_ARGS} arguments")));
        }
    }
    match feeds.iter().position(|f| f.name == feed.name) {
        Some(i) => feeds[i] = feed,
        None if feeds.len() >= FEEDS => return Err(bad(format!("at most {FEEDS} subscribers"))),
        None => feeds.push(feed),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str, addresses: &[&str], secret: Option<&str>) -> TargetSpec {
        TargetSpec {
            name: name.into(),
            kind: TargetKind::Http,
            addresses: addresses.iter().map(|a| a.to_string()).collect(),
            secret: secret.map(str::to_string),
            timeout_ms: 500,
        }
    }

    #[test]
    fn secrets_stay_unless_replaced() {
        let mut targets = Vec::new();
        set(
            &mut targets,
            spec("Haus", &["http://ha:8123/"], Some("token")),
        )
        .unwrap();
        set(&mut targets, spec("Haus", &["http://ha2:8123"], None)).unwrap();

        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].addresses, ["http://ha2:8123"]);
        assert_eq!(targets[0].secret.as_deref(), Some("token"));
        assert!(targets[0].public().has_secret);
        assert!(!serde_json::to_string(&targets[0].public())
            .unwrap()
            .contains("token"));

        set(&mut targets, spec("Haus", &["http://ha2:8123"], Some(""))).unwrap();
        assert_eq!(targets[0].secret, None);
    }

    #[test]
    fn targets_are_checked() {
        let mut targets = Vec::new();
        let refused = |r: Result<(), ActionError>| matches!(r, Err(ActionError::BadTarget { .. }));

        assert!(refused(set(&mut targets, spec("", &["http://x"], None))));
        assert!(refused(set(&mut targets, spec("A", &[], None))));
        assert!(refused(set(
            &mut targets,
            spec("A", &["ha.local:8123"], None)
        )));
        let mut osc = spec("Pult", &["10.0.0.5"], None);
        osc.kind = TargetKind::OscUdp;
        assert!(refused(set(&mut targets, osc.clone())), "host:port");
        osc.addresses = vec!["10.0.0.5:8000".into()];
        set(&mut targets, osc).unwrap();
        let mut slow = spec("Slow", &["http://x"], None);
        slow.timeout_ms = 60_000;
        assert!(refused(set(&mut targets, slow)));
    }

    #[test]
    fn machine_settings_travel_without_secrets() {
        let mut here = Settings::default();
        set(
            &mut here.targets,
            spec("Haus", &["http://ha:8123"], Some("token")),
        )
        .unwrap();
        let carried = without_secrets(&here);
        assert!(!String::from_utf8(to_bytes(&carried))
            .unwrap()
            .contains("token"));

        let mut there = Settings::default();
        set(
            &mut there.targets,
            spec("Haus", &["http://old:8123"], Some("their")),
        )
        .unwrap();
        merge(&mut there, carried).unwrap();

        assert_eq!(there.targets[0].addresses, ["http://ha:8123"]);
        assert_eq!(
            there.targets[0].secret.as_deref(),
            Some("their"),
            "a secret stays"
        );
    }

    #[test]
    fn home_assistant_calls() {
        let data = serde_json::json!({ "brightness_pct": 40 });
        let request = ha_request("light.turn_on", Some("light.room"), Some(&data)).unwrap();

        assert_eq!(request.path, "/api/services/light/turn_on");
        assert_eq!(
            request.body,
            Some(serde_json::json!({ "entity_id": "light.room", "brightness_pct": 40 }))
        );
        assert!(ha_request("turn_on", None, None).is_err());
        assert!(ha_request("light.turn_on", None, Some(&serde_json::json!([1]))).is_err());
    }

    /// One answer per connection: the status line given, the request
    /// line and headers sent back to the test.
    async fn server(status: &'static str) -> (String, tokio::sync::mpsc::Receiver<String>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let (tx, rx) = tokio::sync::mpsc::channel(8);
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                let mut buf = vec![0; 8192];
                let n = stream.read(&mut buf).await.unwrap_or(0);
                let _ = tx
                    .send(String::from_utf8_lossy(&buf[..n]).into_owned())
                    .await;
                let answer =
                    format!("HTTP/1.1 {status}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n");
                let _ = stream.write_all(answer.as_bytes()).await;
            }
        });
        (address, rx)
    }

    #[tokio::test]
    async fn a_request_goes_with_token_and_body() {
        let (address, mut seen) = server("200 OK").await;
        let mut targets = Vec::new();
        set(&mut targets, spec("Haus", &[&address], Some("geheim"))).unwrap();
        let request = ha_request("scene.turn_on", Some("scene.wald"), None).unwrap();

        let result = send(&crate::datasource::client(), &targets[0], &request).await;

        assert_eq!(result, TargetResult::Ok);
        let sent = seen.recv().await.unwrap();
        assert!(
            sent.starts_with("POST /api/services/scene/turn_on HTTP/1.1"),
            "{sent}"
        );
        assert!(
            sent.to_lowercase().contains("authorization: bearer geheim"),
            "{sent}"
        );
        assert!(sent.contains(r#"{"entity_id":"scene.wald"}"#), "{sent}");
    }

    #[tokio::test]
    async fn a_backup_address_is_enough_and_failures_say_why() {
        let (good, _seen) = server("204 No Content").await;
        let (refusing, _seen2) = server("401 Unauthorized").await;
        let http = crate::datasource::client();
        let request = Request {
            method: HttpMethod::Put,
            path: "/x".into(),
            headers: BTreeMap::new(),
            body: Some("text".into()),
        };
        let mut targets = Vec::new();
        set(
            &mut targets,
            spec("Both", &["http://127.0.0.1:9", &good], None),
        )
        .unwrap();
        set(&mut targets, spec("Bad", &[&refusing], None)).unwrap();

        assert_eq!(send(&http, &targets[0], &request).await, TargetResult::Ok);
        let TargetResult::Failed { reason } = send(&http, &targets[1], &request).await else {
            panic!("401 is no success");
        };
        assert!(reason.contains("401"), "{reason}");
        assert!(probe(&http, &targets[1]).await.is_err());
    }
}
