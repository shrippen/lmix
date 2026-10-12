//! Tally to devices on the network over HTTP (PER.1): tally lights and
//! boxes (ESP32, Shelly, home automation) that take a request. On every
//! change of the tally, and every 30 s again (a device that restarted
//! catches up), lmix-core calls each address:
//!
//! - an address without `{…}` gets a POST with JSON:
//!   `{"tally":[1,2,0],"program":[1],"preview":[2]}`: per input in bar
//!   order 0 off, 1 program (also overlays and boxes of a layout on
//!   program), 2 preview; then the input numbers on program and preview;
//! - an address with placeholders gets a GET with them filled in:
//!   `{program}` and `{preview}` (numbers, comma separated, empty if
//!   none), `{tally}` (the list as digits, `120`).
//!
//!   lmix-core --tally-http http://192.168.1.60/tally
//!   lmix-core --tally-http 'http://192.168.1.61/set?red={program}&green={preview}'

use std::sync::Arc;
use std::time::Duration;

use lmix_types::Event;
use tokio::sync::broadcast::error::RecvError;

use crate::service::Service;

/// Sent again this often without a change.
const REFRESH: Duration = Duration::from_secs(30);
/// A device has this long to answer; a slow one does not hold up the rest
/// for long.
const TIMEOUT: Duration = Duration::from_secs(1);

/// Input numbers (1-based) with tally `value`, comma separated.
fn numbers(tally: &[u8], value: u8) -> Vec<usize> {
    tally
        .iter()
        .enumerate()
        .filter(|(_, t)| **t == value)
        .map(|(i, _)| i + 1)
        .collect()
}

fn joined(numbers: &[usize]) -> String {
    numbers
        .iter()
        .map(usize::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

/// The request for `url`: GET with the placeholders filled in, or the
/// JSON body for a POST.
fn request(url: &str, tally: &[u8]) -> (String, Option<serde_json::Value>) {
    let (program, preview) = (numbers(tally, 1), numbers(tally, 2));
    if url.contains('{') {
        let digits: String = tally.iter().map(|t| char::from(b'0' + t.min(&9))).collect();
        let filled = url
            .replace("{program}", &joined(&program))
            .replace("{preview}", &joined(&preview))
            .replace("{tally}", &digits);
        (filled, None)
    } else {
        let body = serde_json::json!({ "tally": tally, "program": program, "preview": preview });
        (url.to_string(), Some(body))
    }
}

/// Call `urls` with the tally until the process ends.
pub async fn serve(urls: Vec<String>, service: Arc<Service>) {
    let http = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .expect("HTTP client");
    tracing::info!(targets = urls.len(), "HTTP tally up");
    let mut events = service.subscribe();
    let mut last: Vec<u8> = service.tally().await;
    let mut tick = tokio::time::interval(REFRESH);
    // Devices that failed last time, logged once until they answer again.
    let mut failing = vec![false; urls.len()];
    loop {
        let changed = tokio::select! {
            event = events.recv() => match event {
                Ok(Event::Tally { tally }) if tally != last => {
                    last = tally;
                    true
                }
                Ok(_) => false,
                Err(RecvError::Lagged(_)) => {
                    last = service.tally().await;
                    true
                }
                Err(RecvError::Closed) => return,
            },
            _ = tick.tick() => true,
        };
        if !changed {
            continue;
        }
        // A burst of changes (a transition) ends on its last tally.
        while let Ok(event) = events.try_recv() {
            if let Event::Tally { tally } = event {
                last = tally;
            }
        }
        let calls = urls.iter().map(|url| {
            let (target, body) = request(url, &last);
            let call = match body {
                Some(body) => http.post(&target).json(&body),
                None => http.get(&target),
            };
            async move {
                call.send()
                    .await
                    .and_then(reqwest::Response::error_for_status)
            }
        });
        for ((result, url), failed) in futures_util::future::join_all(calls)
            .await
            .into_iter()
            .zip(&urls)
            .zip(&mut failing)
        {
            match result {
                Ok(_) => *failed = false,
                Err(error) if !*failed => {
                    tracing::warn!(%url, error = %error.without_url(), "HTTP tally not delivered");
                    *failed = true;
                }
                Err(_) => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn get_with_placeholders_or_post_json() {
        let tally = [1, 2, 0, 1];
        assert_eq!(
            request(
                "http://t/set?red={program}&green={preview}&all={tally}",
                &tally
            ),
            ("http://t/set?red=1,4&green=2&all=1201".to_string(), None)
        );
        let (url, body) = request("http://t/tally", &tally);
        assert_eq!(url, "http://t/tally");
        assert_eq!(
            body.unwrap(),
            serde_json::json!({ "tally": [1, 2, 0, 1], "program": [1, 4], "preview": [2] })
        );
        assert_eq!(request("http://t/?p={program}", &[0, 0]).0, "http://t/?p=");
    }

    #[tokio::test]
    async fn devices_get_each_change() {
        use crate::engine::null::NullEngine;
        use axum::{routing::post, Json, Router};
        use lmix_types::{Action, InputRef, SourceSpec, TestPattern};

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<serde_json::Value>();
        let app = Router::new().route(
            "/tally",
            post(move |Json(body): Json<serde_json::Value>| {
                let tx = tx.clone();
                async move {
                    let _ = tx.send(body);
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await });

        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        service
            .handle(Action::AddInput {
                source: SourceSpec::Pattern {
                    pattern: TestPattern::Bars,
                },
                name: Some("A".into()),
            })
            .await
            .unwrap();
        tokio::spawn(serve(
            vec![format!("http://{address}/tally")],
            service.clone(),
        ));
        // The first call (the interval fires at once): nothing on air.
        let first = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(first["tally"], serde_json::json!([0]));

        service
            .handle(Action::CutDirect {
                input: InputRef::Number(1),
            })
            .await
            .unwrap();
        let next = tokio::time::timeout(Duration::from_secs(3), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next["program"], serde_json::json!([1]));
    }
}
