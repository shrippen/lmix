//! HTTP and WebSocket API (API.1–6).
//!
//! | Route | Purpose |
//! |---|---|
//! | `GET /api/state` | full show state |
//! | `POST /api/actions` | run one [`Action`] (JSON) |
//! | `GET /api/actions` | action catalog as JSON schema |
//! | `GET /api/tally` | tally per input: 0 off, 1 program, 2 preview |
//! | `GET /api/events` | WebSocket: state first, then every event |
//! | `GET /health` | liveness |

use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use lmix_types::{Action, ActionError, Event};
use tokio::sync::broadcast::error::RecvError;

use crate::service::Service;

pub fn router(service: Arc<Service>) -> Router {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/api/state", get(state))
        .route("/api/tally", get(tally))
        .route("/api/actions", get(catalog).post(action))
        .route("/api/events", get(events))
        .with_state(service)
}

async fn state(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.state().await)
}

async fn tally(State(service): State<Arc<Service>>) -> impl IntoResponse {
    Json(service.tally().await)
}

async fn catalog() -> impl IntoResponse {
    Json(schemars::schema_for!(Action))
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

async fn events(State(service): State<Arc<Service>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| stream(socket, service))
}

/// Current state first, so a client never starts blind; then live events.
async fn stream(mut socket: WebSocket, service: Arc<Service>) {
    let mut rx = service.subscribe();
    let first = Event::State {
        state: service.state().await,
    };
    if send(&mut socket, &first).await.is_err() {
        return;
    }

    loop {
        let event = match rx.recv().await {
            Ok(event) => event,
            // Fell behind: resend the full state instead of the lost diffs.
            Err(RecvError::Lagged(_)) => Event::State {
                state: service.state().await,
            },
            Err(RecvError::Closed) => return,
        };
        if send(&mut socket, &event).await.is_err() {
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
    async fn catalog_lists_actions() {
        let req = Request::get("/api/actions").body(Body::empty()).unwrap();
        let resp = app().oneshot(req).await.unwrap();
        let bytes = resp.into_body().collect().await.unwrap().to_bytes();
        let text = String::from_utf8(bytes.to_vec()).unwrap();

        for name in ["add_input", "cut", "transition", "fade_to_black", "open_show"] {
            assert!(text.contains(&format!("\"{name}\"")), "missing {name}");
        }
    }
}
