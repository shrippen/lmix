//! HTTP and WebSocket API (API.1–6).
//!
//! | Route | Purpose |
//! |---|---|
//! | `GET /api/state` | full show state |
//! | `POST /api/actions` | run one [`Action`] (JSON) |
//! | `GET /api/actions` | action catalog as JSON schema |
//! | `GET /api/tally` | tally per input: 0 off, 1 program, 2 preview |
//! | `POST /api/latency/measure?seconds=N` | measure capture-to-program latency for N s (default 10) |
//! | `GET /api/latency` | latest latency statistics (NFA.3) |
//! | `GET /api/audio/levels` | audio levels per input and of program (dBFS) |
//! | `GET /api/outputs/stats` | link health of running SRT outputs (rate, lost, dropped, RTT) |
//! | `GET /api/shows/recent` | recently opened or saved show files, newest first |
//! | `GET /api/devices` | capture devices for `SourceSpec::Device` |
//! | `GET /api/pictures/program` | program picture, JPEG |
//! | `GET /api/pictures/inputs/{input}` | input picture (number, name, id, `preview`, `program`) |
//! | `GET /api/templates` | title templates: built-in and from the folder (GFX.19) |
//! | `GET /api/data/{source}` | all rows of a data source (number, name or id) |
//! | `GET /titles/{path}` | template files, the title SDK, Kante's tokens and fonts |
//! | `GET /graphics/{graphics}` | the page a graphics channel renders (number, name or id) |
//! | `GET /api/events` | WebSocket: state first, then every event (`?diff=true`: state changes as JSON Patch) |
//! | `GET /health` | liveness |
//! | `GET /metrics` | health and metrics, Prometheus text format (OPS.3) |
//! | `GET /`, `/mixer`, `/tally`, `/keys`, `/titles` | browser pages (`web.rs`) |

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::header;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use lmix_types::{Action, ActionError, Event, InputRef, LiveFeed};
use tokio::sync::broadcast::error::RecvError;

use crate::engine::EngineError;
use crate::service::Service;

/// Seconds a client waits before asking again for a picture not ready yet.
const PICTURE_RETRY_SECS: &str = "1";

pub fn router(service: Arc<Service>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/metrics", get(metrics))
        .route("/api/state", get(state))
        .route("/api/tally", get(tally))
        .route("/api/actions", get(catalog).post(action))
        .route("/api/events", get(events))
        .route("/api/audio/levels", get(levels))
        .route("/api/outputs/stats", get(output_stats))
        .route("/api/latency", get(latency))
        .route("/api/latency/measure", post(measure_latency))
        .route("/api/shows/recent", get(recent))
        .route("/api/devices", get(devices))
        .route("/api/pictures/program", get(program_picture))
        .route("/api/pictures/inputs/{input}", get(input_picture))
        .route("/api/live/program", get(live_program))
        .route("/api/live/inputs/{input}", get(live_input))
        .route("/api/templates", get(templates))
        .route("/api/data/{source}", get(data_table))
        .route("/titles/{*path}", get(title_file))
        .route("/graphics/{graphics}", get(graphics_page))
        .merge(crate::web::routes())
        .with_state(service)
}

async fn state(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.state().await)
}

async fn tally(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.tally().await)
}

async fn metrics(State(service): State<Arc<Service>>) -> impl IntoResponse {
    let snapshot = crate::metrics::Snapshot {
        state: service.state().await,
        tally: service.tally().await,
        levels: service.levels().await,
        outputs: service.output_stats(),
        latency: service.latency(),
        uptime_secs: crate::metrics::started().elapsed().as_secs(),
        engine: service.engine_health(),
    };
    (
        [(header::CONTENT_TYPE, "text/plain; version=0.0.4")],
        crate::metrics::render(&snapshot, crate::metrics::resident_bytes()),
    )
}

async fn latency(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.latency())
}

#[derive(serde::Deserialize)]
struct MeasureQuery {
    #[serde(default = "default_measure_secs")]
    seconds: u32,
}

/// Long enough for a few hundred frames.
const DEFAULT_MEASURE_SECS: u32 = 10;

fn default_measure_secs() -> u32 {
    DEFAULT_MEASURE_SECS
}

async fn measure_latency(
    State(service): State<Arc<Service>>,
    Query(query): Query<MeasureQuery>,
) -> Response {
    match service.measure_latency(query.seconds).await {
        Ok(()) => StatusCode::ACCEPTED.into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

async fn output_stats(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.output_stats())
}

async fn levels(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.levels().await)
}

async fn recent(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.recent().await)
}

async fn devices(State(service): State<Arc<Service>>) -> Response {
    match service.devices().await {
        Ok(devices) => Json(devices).into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

async fn program_picture(State(service): State<Arc<Service>>) -> Response {
    picture_response(service.program_picture().await)
}

async fn input_picture(State(service): State<Arc<Service>>, Path(input): Path<String>) -> Response {
    // A path segment is text; numbers mean the input bar position.
    let input = match input.parse::<u32>() {
        Ok(number) => InputRef::Number(number),
        Err(_) => InputRef::Text(input),
    };
    match service.input_picture(&input).await {
        Ok(picture) => picture_response(picture),
        Err(error) => error_response(error),
    }
}

async fn live_program(State(service): State<Arc<Service>>) -> Response {
    live_response(service.live_program().await)
}

/// `?size=full`: the input as it arrives (preview monitor); else tile size.
#[derive(serde::Deserialize)]
struct LiveQuery {
    #[serde(default)]
    size: Option<String>,
}

async fn live_input(
    State(service): State<Arc<Service>>,
    Path(input): Path<String>,
    Query(query): Query<LiveQuery>,
) -> Response {
    let input = match input.parse::<u32>() {
        Ok(number) => InputRef::Number(number),
        Err(_) => InputRef::Text(input),
    };
    let full = query.size.as_deref() == Some("full");
    match service.live_input(&input, full).await {
        Ok(feed) => live_response(feed),
        Err(error) => error_response(error),
    }
}

/// Socket of a live picture; 404 when the engine has none (no engine).
fn live_response(feed: Result<std::path::PathBuf, EngineError>) -> Response {
    match feed {
        Ok(socket) => Json(LiveFeed {
            socket: socket.to_string_lossy().into_owned(),
        })
        .into_response(),
        Err(EngineError::NoPictures) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

/// JPEG; 503 with Retry-After while the engine has no frame yet.
fn picture_response(picture: Result<Vec<u8>, EngineError>) -> Response {
    match picture {
        Ok(jpeg) => ([(header::CONTENT_TYPE, "image/jpeg")], jpeg).into_response(),
        Err(EngineError::NotReady) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::RETRY_AFTER, PICTURE_RETRY_SECS)],
        )
            .into_response(),
        Err(EngineError::NoPictures) => StatusCode::NOT_FOUND.into_response(),
        Err(error) => (StatusCode::BAD_GATEWAY, error.to_string()).into_response(),
    }
}

async fn data_table(State(service): State<Arc<Service>>, Path(source): Path<String>) -> Response {
    let source = match source.parse::<u32>() {
        Ok(number) => InputRef::Number(number),
        Err(_) => InputRef::Text(source),
    };
    match service.data_table(&source).await {
        Ok(table) => Json(table).into_response(),
        Err(error) => error_response(error),
    }
}

async fn templates(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.templates())
}

/// Template files change while a show runs: never cached.
async fn title_file(State(service): State<Arc<Service>>, Path(path): Path<String>) -> Response {
    match service.title_file(&path) {
        Some(bytes) => (
            [
                (header::CONTENT_TYPE, crate::titles::content_type(&path)),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            bytes,
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// The page finds its channel from the address; it shows an empty
/// transparent picture until the channel exists.
async fn graphics_page(Path(_graphics): Path<String>) -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/html; charset=utf-8"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        crate::titles::host_page(),
    )
        .into_response()
}

async fn catalog() -> impl IntoResponse {
    Json(catalog_schema())
}

/// The actions' JSON schema, each marked with its group (`x-group`,
/// API.1).
fn catalog_schema() -> serde_json::Value {
    let mut schema = serde_json::to_value(schemars::schema_for!(Action)).expect("schema");
    if let Some(variants) = schema.get_mut("oneOf").and_then(|v| v.as_array_mut()) {
        for variant in variants {
            if let Some(group) = action_name(variant).and_then(lmix_types::action_group) {
                variant["x-group"] = group.into();
            }
        }
    }
    schema["x-groups"] = lmix_types::ACTION_GROUPS
        .iter()
        .map(|(group, _)| *group)
        .collect::<Vec<_>>()
        .into();
    schema
}

/// The `action` name a variant of the schema stands for.
fn action_name(variant: &serde_json::Value) -> Option<&str> {
    let tag = &variant["properties"]["action"];
    tag["const"]
        .as_str()
        .or_else(|| tag["enum"].get(0).and_then(|v| v.as_str()))
}

async fn action(State(service): State<Arc<Service>>, Json(action): Json<Action>) -> Response {
    match service.handle(action).await {
        Ok(state) => Json(state).into_response(),
        Err(error) => error_response(error),
    }
}

/// Refused actions: 4xx with the reason; engine trouble: 502.
fn error_response(error: ActionError) -> Response {
    let status = match error {
        ActionError::UnknownInput { .. } => StatusCode::NOT_FOUND,
        ActionError::Engine { .. } => StatusCode::BAD_GATEWAY,
        ActionError::SlotsFull { .. } | ActionError::NoPreview | ActionError::OnProgram { .. } => {
            StatusCode::CONFLICT
        }
        _ => StatusCode::UNPROCESSABLE_ENTITY,
    };
    let body = serde_json::json!({ "error": error, "message": error.to_string() });
    (status, Json(body)).into_response()
}

#[derive(serde::Deserialize)]
struct EventsQuery {
    /// State changes as `state_diff` events (API.6) instead of the whole
    /// state each time: `?diff`, `?diff=true`, `?diff=1`.
    #[serde(default)]
    diff: Option<String>,
}

impl EventsQuery {
    fn diff(&self) -> bool {
        matches!(self.diff.as_deref(), Some("" | "1" | "true" | "yes"))
    }
}

async fn events(
    State(service): State<Arc<Service>>,
    Query(query): Query<EventsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    let diff = query.diff();
    ws.on_upgrade(move |socket| stream(socket, service, diff))
}

/// Current state first, so a client never starts blind; then live events.
/// With `diff`, a state after the first goes out as
/// `{"event":"state_diff","ops":[…]}`: JSON Patch operations (RFC 6902)
/// on the last state sent; the whole state again when that is shorter or
/// the client fell behind.
async fn stream(mut socket: WebSocket, service: Arc<Service>, diff: bool) {
    let mut rx = service.subscribe();
    let first = Event::State {
        state: service.state().await,
    };
    // The state this client holds, for diffs.
    let mut held = diff.then(|| serde_json::to_value(&first).expect("state serializes"));
    // Length of the last whole state sent: a diff about as long goes out
    // whole instead.
    let mut whole_len = held.as_ref().map_or(0, |v| v.to_string().len());
    if send(&mut socket, &first).await.is_err() {
        return;
    }

    loop {
        let event = match rx.recv().await {
            Ok(event) => event,
            // Fell behind: resend the full state instead of the lost diffs.
            Err(RecvError::Lagged(_)) => {
                let full = Event::State {
                    state: service.state().await,
                };
                if let Some(held) = &mut held {
                    *held = serde_json::to_value(&full).expect("state serializes");
                }
                if send(&mut socket, &full).await.is_err() {
                    return;
                }
                continue;
            }
            Err(RecvError::Closed) => return,
        };
        let sent = match (&mut held, &event) {
            (Some(held), Event::State { .. }) => {
                let now = serde_json::to_value(&event).expect("state serializes");
                let ops = crate::patch::diff(&held["state"], &now["state"]);
                let text = if ops.is_empty() {
                    None
                } else {
                    let patch =
                        serde_json::json!({ "event": "state_diff", "ops": ops }).to_string();
                    // The whole state is serialized only when the diff comes
                    // near the last one's length.
                    if patch.len() < whole_len / 2 {
                        Some(patch)
                    } else {
                        let whole = now.to_string();
                        whole_len = whole.len();
                        Some(if patch.len() < whole.len() {
                            patch
                        } else {
                            whole
                        })
                    }
                };
                *held = now;
                match text {
                    Some(text) => socket.send(Message::Text(text.into())).await,
                    None => Ok(()),
                }
            }
            _ => send(&mut socket, &event).await,
        };
        if sent.is_err() {
            return;
        }
    }
}

async fn send(socket: &mut WebSocket, event: &Event) -> Result<(), axum::Error> {
    let text = serde_json::to_string(event).expect("event serializes");
    socket.send(Message::Text(text.into())).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::null::NullEngine;
    use axum::body::Body;
    use axum::http::Request;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    fn app() -> Router {
        router(Service::new(2, Arc::new(NullEngine::default()), None))
    }

    async fn post(app: &Router, body: &str) -> (StatusCode, serde_json::Value) {
        let req = Request::post("/api/actions")
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[tokio::test]
    async fn add_then_preview_by_name() {
        let app = app();

        let (status, _) = post(
            &app,
            r#"{"action":"add_input","source":{"kind":"pattern","pattern":"bars"},"name":"Cam 1"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::OK);

        let (status, state) = post(&app, r#"{"action":"preview_input","input":"Cam 1"}"#).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(state["preview"], state["inputs"][0]["id"]);
    }

    #[tokio::test]
    async fn unknown_input_is_404_with_reason() {
        let (status, body) = post(&app(), r#"{"action":"preview_input","input":7}"#).await;

        assert_eq!(status, StatusCode::NOT_FOUND);
        assert_eq!(body["error"]["error"], "unknown_input");
        assert_eq!(body["message"], "no input matches 7");
    }

    #[tokio::test]
    async fn live_without_engine_is_404() {
        let req = Request::get("/api/live/program")
            .body(Body::empty())
            .unwrap();
        let resp = app().oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn live_of_unknown_input_is_404() {
        for uri in ["/api/live/inputs/3", "/api/live/inputs/3?size=full"] {
            let req = Request::get(uri).body(Body::empty()).unwrap();
            let resp = app().oneshot(req).await.unwrap();

            assert_eq!(resp.status(), StatusCode::NOT_FOUND, "{uri}");
        }
    }

    #[tokio::test]
    async fn picture_of_unknown_input_is_404() {
        let req = Request::get("/api/pictures/inputs/3")
            .body(Body::empty())
            .unwrap();
        let resp = app().oneshot(req).await.unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    async fn get(app: &Router, uri: &str) -> (StatusCode, String, Vec<u8>) {
        let resp = app
            .clone()
            .oneshot(Request::get(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let kind = resp
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|v| v.to_str().unwrap().to_string())
            .unwrap_or_default();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        (status, kind, bytes.to_vec())
    }

    #[tokio::test]
    async fn titles_pages_and_templates() {
        let app = app();

        let (status, _, body) = get(&app, "/api/templates").await;
        assert_eq!(status, StatusCode::OK);
        let list: Vec<lmix_types::Template> = serde_json::from_slice(&body).unwrap();
        let lower = list.iter().find(|t| t.id == "lmix/lower-third").unwrap();
        assert!(lower.builtin);

        let (status, kind, _) = get(&app, &lower.url).await;
        assert_eq!(
            (status, kind.as_str()),
            (StatusCode::OK, "text/html; charset=utf-8")
        );
        let (status, kind, _) = get(&app, "/titles/kante/fonts/Rajdhani-700.ttf").await;
        assert_eq!((status, kind.as_str()), (StatusCode::OK, "font/ttf"));
        let (status, _, page) = get(&app, "/graphics/1").await;
        assert_eq!(status, StatusCode::OK);
        assert!(String::from_utf8(page).unwrap().contains("/api/events"));
        let (status, _, _) = get(&app, "/titles/nothing.html").await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn title_actions_over_http() {
        let app = app();
        post(&app, r#"{"action":"add_graphics","name":"Titles"}"#).await;

        let (status, state) = post(
            &app,
            r#"{"action":"title_load","graphics":"Titles","template":"lmix/name-role","data":{"name":"N"},"play":true}"#,
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(state["graphics"][0]["layers"][0]["phase"], "playing");
        assert_eq!(state["graphics"][0]["layers"][0]["values"]["name"], "N");

        let (status, body) = post(
            &app,
            r#"{"action":"title_load","graphics":1,"template":"nope"}"#,
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error"]["error"], "unknown_template");
        let (status, _) = post(&app, r#"{"action":"title_play","graphics":2}"#).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    #[test]
    fn every_action_is_in_one_group() {
        let schema = catalog_schema();
        let names: Vec<&str> = schema["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| action_name(v).expect("tagged variant"))
            .collect();
        let missing: Vec<_> = names
            .iter()
            .filter(|n| lmix_types::action_group(n).is_none())
            .collect();
        assert!(missing.is_empty(), "no group: {missing:?}");
        let grouped: Vec<&str> = lmix_types::ACTION_GROUPS
            .iter()
            .flat_map(|(_, n)| n.iter().copied())
            .collect();
        let unknown: Vec<_> = grouped.iter().filter(|n| !names.contains(n)).collect();
        assert!(unknown.is_empty(), "not an action: {unknown:?}");
        let mut unique = grouped.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), grouped.len(), "an action in two groups");
        assert_eq!(
            schema["oneOf"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| action_name(v) == Some("cut"))
                .unwrap()["x-group"],
            "switching"
        );
    }

    #[tokio::test]
    async fn catalog_lists_actions() {
        let req = Request::get("/api/actions").body(Body::empty()).unwrap();
        let resp = app().oneshot(req).await.unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(bytes.to_vec()).unwrap();

        for name in [
            "add_input",
            "cut",
            "transition",
            "fade_to_black",
            "open_show",
        ] {
            assert!(text.contains(&format!("\"{name}\"")), "missing {name}");
        }
    }
}
