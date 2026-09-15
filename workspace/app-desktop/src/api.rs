//! Typed REST client against the strom backend.
//!
//! Deliberately thin: only the endpoints the Console actually calls, not a
//! general-purpose strom client (see docs/STROM-ANATOMY.md for the full API).

use strom_types::api::{
    FlowListResponse, FlowResponse, SelectPreviewRequest, SelectPreviewResponse,
    TransitionResponse, TriggerTransitionRequest, VisionMixerState,
};
use strom_types::vision_mixer::Source;
use strom_types::{Flow, FlowId};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("server error {status}: {body}")]
    Server { status: u16, body: String },
}

pub type ApiResult<T> = Result<T, ApiError>;

#[derive(Clone)]
pub struct ApiClient {
    base_url: String,
    http: reqwest::Client,
}

impl ApiClient {
    pub fn new(base_url: String) -> Self {
        Self {
            base_url,
            http: reqwest::Client::new(),
        }
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    async fn check_status(resp: reqwest::Response) -> ApiResult<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status().as_u16();
        let body = resp.text().await.unwrap_or_default();
        Err(ApiError::Server { status, body })
    }

    pub async fn list_flows(&self) -> ApiResult<Vec<Flow>> {
        let url = format!("{}/api/flows", self.base_url);
        let resp = self.http.get(url).send().await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json::<FlowListResponse>().await?.flows)
    }

    pub async fn get_flow(&self, flow_id: FlowId) -> ApiResult<Flow> {
        let url = format!("{}/api/flows/{}", self.base_url, flow_id);
        let resp = self.http.get(url).send().await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json::<FlowResponse>().await?.flow)
    }

    pub async fn get_vision_mixer_state(
        &self,
        flow_id: FlowId,
        block_id: &str,
    ) -> ApiResult<Option<VisionMixerState>> {
        let url = format!(
            "{}/api/flows/{}/blocks/{}/state",
            self.base_url, flow_id, block_id
        );
        let resp = self.http.get(url).send().await?;
        if resp.status().as_u16() == 404 {
            // Flow not running yet - no live overlay state, not an error.
            return Ok(None);
        }
        let resp = Self::check_status(resp).await?;
        Ok(Some(resp.json::<VisionMixerState>().await?))
    }

    pub async fn select_preview(
        &self,
        flow_id: FlowId,
        block_id: &str,
        source: Source,
    ) -> ApiResult<SelectPreviewResponse> {
        let url = format!(
            "{}/api/flows/{}/blocks/{}/preview",
            self.base_url, flow_id, block_id
        );
        let resp = self
            .http
            .put(url)
            .json(&SelectPreviewRequest { source })
            .send()
            .await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn trigger_transition(
        &self,
        flow_id: FlowId,
        block_id: &str,
        from_input: usize,
        to_input: usize,
        transition_type: &str,
        duration_ms: u64,
    ) -> ApiResult<TransitionResponse> {
        let url = format!(
            "{}/api/flows/{}/blocks/{}/transition",
            self.base_url, flow_id, block_id
        );
        let resp = self
            .http
            .post(url)
            .json(&TriggerTransitionRequest {
                from_input,
                to_input,
                transition_type: transition_type.to_string(),
                duration_ms,
            })
            .send()
            .await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_thumbnail(&self, flow_id: FlowId, block_id: &str) -> ApiResult<Vec<u8>> {
        let url = format!(
            "{}/api/flows/{}/blocks/{}/thumbnail",
            self.base_url, flow_id, block_id
        );
        let resp = self.http.get(url).send().await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.bytes().await?.to_vec())
    }
}
