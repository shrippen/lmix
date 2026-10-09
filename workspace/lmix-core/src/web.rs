//! Browser pages served by lmix-core (WEB.1–3): mixer, tally. They use the
//! same API as every other client (`/api/events`, `/api/actions`).

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

const HTML: &str = "text/html; charset=utf-8";

/// Page routes; merged into the API router.
pub fn routes<S: Clone + Send + Sync + 'static>() -> Router<S> {
    Router::new()
        .route("/", get(|| page(include_str!("../web/index.html"))))
        .route("/mixer", get(|| page(include_str!("../web/mixer.html"))))
        .route("/tally", get(|| page(include_str!("../web/tally.html"))))
        .route(
            "/web/lmix.css",
            get(|| asset("text/css; charset=utf-8", include_str!("../web/lmix.css"))),
        )
        .route(
            "/web/lmix.js",
            get(|| {
                asset(
                    "text/javascript; charset=utf-8",
                    include_str!("../web/lmix.js"),
                )
            }),
        )
}

async fn page(body: &'static str) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, HTML)], body)
}

async fn asset(content_type: &'static str, body: &'static str) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, content_type)], body)
}
