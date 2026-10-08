//! strom driver: switcher commands → strom REST calls.
//!
//! | Command | strom |
//! |---|---|
//! | `SetSlot` with source | replace slot flow (fixed id), start it |
//! | `SetSlot` empty | stop and delete slot flow |
//! | `Preview` | vision mixer: select preview |
//! | `Program` | vision mixer: transition from its current program (`cut` for hard cuts); no slot = black input |
//! | `FadeToBlack` | vision mixer: toggle FTB |

use async_trait::async_trait;
use lmix_types::Effect;
use reqwest::StatusCode;
use serde::Serialize;
use std::sync::atomic::{AtomicUsize, Ordering};
use strom_types::api::{
    FadeToBlackRequest, SelectPreviewRequest, TriggerTransitionRequest, VisionMixerState,
};
use strom_types::vision_mixer::Source;
use strom_types::{Flow, FlowId, PipelineState, PropertyValue};
use std::time::Duration;

/// How often and how long `start` waits for the mixer to play.
const PLAYING_POLL: Duration = Duration::from_millis(200);
const PLAYING_POLLS: u32 = 50;
/// strom's answer to a transition before the pipeline has a position.
const NOT_RUNNING_YET: &str = "query pipeline position";

use super::flows::{self, MixerBackend, MIXER_BLOCK};
use super::{Engine, EngineError};
use crate::switcher::Command;

pub struct StromEngine {
    base_url: String,
    http: reqwest::Client,
    backend: MixerBackend,
    /// Mixer input that shows black; set by `start`.
    black: AtomicUsize,
}

impl StromEngine {
    pub fn new(base_url: String, backend: MixerBackend) -> Self {
        Self {
            base_url,
            http: reqwest::Client::new(),
            backend,
            black: AtomicUsize::new(0),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api{}", self.base_url, path)
    }

    // ---- Flows ----

    async fn get_flow(&self, id: FlowId) -> Result<Option<Flow>, EngineError> {
        let resp = self.http.get(self.url(&format!("/flows/{id}"))).send().await.map_err(net)?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(None);
        }

        #[derive(serde::Deserialize)]
        struct FlowResponse {
            flow: Flow,
        }
        let resp = check(resp).await?;
        let body: FlowResponse = resp.json().await.map_err(net)?;
        Ok(Some(body.flow))
    }

    /// Replace a flow under its fixed id and start it.
    async fn put_flow(&self, flow: &Flow) -> Result<(), EngineError> {
        self.drop_flow(flow.id).await?;
        let resp = self.http.post(self.url("/flows")).json(flow).send().await.map_err(net)?;
        check(resp).await?;
        self.post_empty(&format!("/flows/{}/start", flow.id)).await
    }

    /// Stop and delete; a missing flow is fine.
    async fn drop_flow(&self, id: FlowId) -> Result<(), EngineError> {
        let resp = self.http.delete(self.url(&format!("/flows/{id}"))).send().await.map_err(net)?;
        if resp.status() == StatusCode::NOT_FOUND {
            return Ok(());
        }
        check(resp).await.map(|_| ())
    }

    // ---- Vision mixer ----

    async fn mixer_call<T: Serialize>(&self, action: &str, body: &T) -> Result<(), EngineError> {
        let path = format!("/flows/{}/blocks/{MIXER_BLOCK}/{action}", flows::mixer_id());
        let resp = if action == "preview" {
            self.http.put(self.url(&path)).json(body).send().await
        } else {
            self.http.post(self.url(&path)).json(body).send().await
        }
        .map_err(net)?;
        check(resp).await.map(|_| ())
    }

    async fn wait_playing(&self, id: FlowId) -> Result<(), EngineError> {
        for _ in 0..PLAYING_POLLS {
            let playing = self
                .get_flow(id)
                .await?
                .is_some_and(|f| f.gst_state == Some(PipelineState::Playing));
            if playing {
                return Ok(());
            }
            tokio::time::sleep(PLAYING_POLL).await;
        }
        Err(EngineError::Refused(format!("flow {id} did not reach PLAYING")))
    }

    async fn mixer_state(&self) -> Result<VisionMixerState, EngineError> {
        let path = format!("/flows/{}/blocks/{MIXER_BLOCK}/state", flows::mixer_id());
        let resp = self.http.get(self.url(&path)).send().await.map_err(net)?;
        check(resp).await?.json().await.map_err(net)
    }

    /// Transition from whatever the engine has on program to `slot`.
    async fn program(&self, slot: Option<usize>, effect: Effect, duration_ms: u32) -> Result<(), EngineError> {
        let to = slot.unwrap_or_else(|| self.black.load(Ordering::Relaxed));
        let from = self.mixer_state().await?.program_input;
        if from == Some(to) {
            return Ok(());
        }

        let body = TriggerTransitionRequest {
            from_input: from.unwrap_or(to),
            to_input: to,
            transition_type: wire_name(effect).into(),
            duration_ms: u64::from(duration_ms),
        };

        // Right after start strom cannot place a transition until the first
        // frames ran through ("Failed to query pipeline position").
        let mut attempt = 0;
        loop {
            let result = self.mixer_call("transition", &body).await;
            let warming_up = matches!(&result, Err(EngineError::Refused(m)) if m.contains(NOT_RUNNING_YET));
            attempt += 1;
            if !warming_up || attempt >= PLAYING_POLLS {
                return result;
            }
            tokio::time::sleep(PLAYING_POLL).await;
        }
    }

    async fn post_empty(&self, path: &str) -> Result<(), EngineError> {
        let resp = self.http.post(self.url(path)).send().await.map_err(net)?;
        check(resp).await.map(|_| ())
    }
}

#[async_trait]
impl Engine for StromEngine {
    /// Keep a running mixer with the right slot count (no picture drop when
    /// lmix-core restarts); rebuild it otherwise.
    async fn start(&self, slots: usize) -> Result<(), EngineError> {
        let black = flows::black_input(slots);
        self.black.store(black, Ordering::Relaxed);

        let wanted = flows::mixer_flow(slots, self.backend);
        let current = self.get_flow(wanted.id).await?;
        let reusable = current.as_ref().is_some_and(|f| f.running && slot_count(f) == Some(black + 1));
        if reusable {
            return Ok(());
        }

        // strom starts a fresh mixer on input 0; the restore that follows
        // every start puts the show's program (or black) there. Transitions
        // need a playing pipeline, so wait for it.
        self.put_flow(&wanted).await?;
        self.wait_playing(wanted.id).await
    }

    async fn run(&self, command: &Command) -> Result<(), EngineError> {
        match command {
            Command::SetSlot { slot, source: Some(source) } => {
                self.put_flow(&flows::slot_flow(*slot, source)).await
            }
            Command::SetSlot { slot, source: None } => self.drop_flow(flows::slot_id(*slot)).await,
            Command::Preview { slot } => {
                let body = SelectPreviewRequest {
                    source: Source::Input(*slot),
                };
                self.mixer_call("preview", &body).await
            }
            Command::Program {
                slot,
                effect,
                duration_ms,
            } => self.program(*slot, *effect, *duration_ms).await,
            Command::FadeToBlack { duration_ms } => {
                let body = FadeToBlackRequest {
                    duration_ms: u64::from(*duration_ms),
                };
                self.mixer_call("ftb", &body).await
            }
        }
    }
}

/// strom `TransitionType` name of an effect.
fn wire_name(effect: Effect) -> &'static str {
    match effect {
        Effect::Cut => "cut",
        Effect::Fade => "fade",
        Effect::DipToBlack => "dip_to_black",
        Effect::SlideLeft => "slide_left",
        Effect::SlideRight => "slide_right",
        Effect::PushLeft => "push_left",
        Effect::PushRight => "push_right",
        Effect::WipeLeft => "wipe_left",
        Effect::WipeRight => "wipe_right",
    }
}

/// `num_inputs` of the vision mixer in a mixer flow.
fn slot_count(flow: &Flow) -> Option<usize> {
    let mixer = flow.blocks.iter().find(|b| b.id == MIXER_BLOCK)?;
    match mixer.properties.get("num_inputs")? {
        PropertyValue::Int(n) => usize::try_from(*n).ok(),
        PropertyValue::UInt(n) => usize::try_from(*n).ok(),
        _ => None,
    }
}

fn net(e: reqwest::Error) -> EngineError {
    EngineError::Unreachable(e.to_string())
}

async fn check(resp: reqwest::Response) -> Result<reqwest::Response, EngineError> {
    if resp.status().is_success() {
        return Ok(resp);
    }
    let status = resp.status();
    let body = resp.text().await.unwrap_or_default();
    Err(EngineError::Refused(format!("{status}: {body}")))
}
