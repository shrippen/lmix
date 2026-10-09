//! Typed REST client against the strom backend.
//!
//! Deliberately thin: only the endpoints the Console actually calls, not a
//! general-purpose strom client (see docs/STROM-ANATOMY.md for the full API).

use std::collections::HashMap;
use strom_types::api::{
    BlockPropertiesResponse, DskToggleRequest, DskToggleResponse,
    FadeToBlackRequest, FadeToBlackResponse, FlowListResponse, FlowResponse, LatencyResponse, SelectPreviewRequest,
    SelectPreviewResponse, TransitionResponse, TriggerTransitionRequest,
    UpdateBlockPropertiesRequest, VisionMixerState,
};
use strom_types::discovery::DeviceResponse;
use strom_types::vision_mixer::Source;
use strom_types::{Flow, FlowId, PropertyValue};

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

    /// WebSocket event stream URL of the same server.
    pub fn ws_url(&self) -> String {
        self.base_url
            .replacen("http://", "ws://", 1)
            .replacen("https://", "wss://", 1)
            + "/api/ws"
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

    pub async fn get_block_properties(
        &self,
        flow_id: FlowId,
        block_id: &str,
    ) -> ApiResult<HashMap<String, PropertyValue>> {
        let url = format!(
            "{}/api/flows/{}/blocks/{}/properties",
            self.base_url, flow_id, block_id
        );
        let resp = self.http.get(url).send().await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json::<BlockPropertiesResponse>().await?.properties)
    }

    pub async fn patch_block_properties(
        &self,
        flow_id: FlowId,
        block_id: &str,
        properties: HashMap<String, PropertyValue>,
    ) -> ApiResult<HashMap<String, PropertyValue>> {
        self.patch_block_properties_ramped(flow_id, block_id, properties, None)
            .await
    }

    /// Like `patch_block_properties`; audio volume/mute changes fade over
    /// `ramp_ms` (all properties of the batch together).
    pub async fn patch_block_properties_ramped(
        &self,
        flow_id: FlowId,
        block_id: &str,
        properties: HashMap<String, PropertyValue>,
        ramp_ms: Option<u32>,
    ) -> ApiResult<HashMap<String, PropertyValue>> {
        let url = format!(
            "{}/api/flows/{}/blocks/{}/properties",
            self.base_url, flow_id, block_id
        );
        let resp = self
            .http
            .patch(url)
            .json(&UpdateBlockPropertiesRequest {
                properties,
                ramp_ms,
                ramp_ms_overrides: None,
            })
            .send()
            .await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json::<BlockPropertiesResponse>().await?.properties)
    }

    /// Idempotent on the backend: starting an already-running flow just
    /// returns its current state, no error - safe to call unconditionally
    /// whenever the Console opens a flow.
    pub async fn start_flow(&self, flow_id: FlowId) -> ApiResult<()> {
        let url = format!("{}/api/flows/{}/start", self.base_url, flow_id);
        let resp = self.http.post(url).send().await?;
        Self::check_status(resp).await?;
        Ok(())
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

    /// Create a flow under its own client-generated id.
    pub async fn create_flow(&self, flow: &Flow) -> ApiResult<Flow> {
        let url = format!("{}/api/flows", self.base_url);
        let resp = self.http.post(url).json(flow).send().await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json::<FlowResponse>().await?.flow)
    }

    /// Video capture devices known to the backend's discovery.
    pub async fn list_video_devices(&self) -> ApiResult<Vec<DeviceResponse>> {
        let url = format!("{}/api/discovery/devices?category=video_source", self.base_url);
        let resp = self.http.get(url).send().await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_latency(&self, flow_id: FlowId) -> ApiResult<LatencyResponse> {
        let url = format!("{}/api/flows/{}/latency", self.base_url, flow_id);
        let resp = self.http.get(url).send().await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json().await?)
    }

    /// Toggle Fade to Black; the response carries the new state.
    pub async fn toggle_ftb(
        &self,
        flow_id: FlowId,
        block_id: &str,
        duration_ms: u64,
    ) -> ApiResult<FadeToBlackResponse> {
        let url = format!("{}/api/flows/{}/blocks/{}/ftb", self.base_url, flow_id, block_id);
        let resp = self
            .http
            .post(url)
            .json(&FadeToBlackRequest { duration_ms })
            .send()
            .await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json().await?)
    }

    /// Switch DSK layer `dsk` (1-based) on or off.
    pub async fn set_dsk(
        &self,
        flow_id: FlowId,
        block_id: &str,
        dsk: usize,
        enabled: bool,
    ) -> ApiResult<DskToggleResponse> {
        let url = format!("{}/api/flows/{}/blocks/{}/dsk", self.base_url, flow_id, block_id);
        let resp = self
            .http
            .post(url)
            .json(&DskToggleRequest { dsk, enabled })
            .send()
            .await?;
        let resp = Self::check_status(resp).await?;
        Ok(resp.json().await?)
    }

}
