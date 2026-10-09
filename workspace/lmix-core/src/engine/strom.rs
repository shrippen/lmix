//! strom driver: switcher commands → strom REST calls.
//!
//! | Command | strom |
//! |---|---|
//! | `SetSlot` with source | replace slot flow (fixed id), start it |
//! | `SetSlot` empty | stop and delete slot flow |
//! | `Preview` | vision mixer: select preview |
//! | `Program` | vision mixer: target into its preview (strom takes preview, not `to_input`), transition (`cut` for hard cuts), old preview back; no slot = black input |
//! | `FadeToBlack` | vision mixer: toggle FTB |
//! | `SetAudio` | audio mixer: channel fader and mute, ramped |
//! | `SetListen` | audio mixer: channel PFL/AFL (monitor bus) |
//! | `SetSignalLoss` | vision mixer compositor pads: `max-last-buffer-repeat` |
//! | `StartOutput` / `StopOutput` | output flow (own flow per output) built and started / deleted |
//! | `Transport` | slot's media player: play, pause, seek 0, next, previous |

use async_trait::async_trait;
use lmix_types::{
    ChannelStrip, Device, DeviceKind, Effect, Listen, OutputId, OutputStats, SignalLoss, Transport,
};
use reqwest::StatusCode;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use strom_types::api::{
    FadeToBlackRequest, SelectPreviewRequest, SrtRole, SrtStats, SrtStatsResponse,
    TriggerTransitionRequest, UpdateBlockPropertiesRequest, VisionMixerState,
};
use strom_types::discovery::DeviceResponse;
use strom_types::vision_mixer::{Source, TRANSITION_FOREGROUND_ZORDER};
use strom_types::{Flow, FlowId, PipelineState, PropertyValue};
use tokio::sync::{broadcast, watch, Mutex};

/// How often and how long `start` waits for the mixer to play.
const PLAYING_POLL: Duration = Duration::from_millis(200);
const PLAYING_POLLS: u32 = 50;
/// Flow errors kept for a slow reader.
const ERROR_BUFFER: usize = 64;
/// Vision mixer compositors whose input pads follow `SignalLoss`.
const MIXER_COMPOSITORS: [&str; 2] = ["mixer", "mv_comp"];
/// `max-last-buffer-repeat` default: repeat until end of stream.
const REPEAT_FOREVER_NS: u64 = u64::MAX;
/// `SignalLoss::Black`: this long without a frame, then black.
const BLACK_AFTER: Duration = Duration::from_millis(400);

/// strom discovery categories and what lmix makes of them.
const DEVICE_CATEGORIES: [(&str, DeviceKind); 3] = [
    ("video_source", DeviceKind::Video),
    ("audio_source", DeviceKind::Audio),
    ("network_source", DeviceKind::Ndi),
];

/// strom's answer to a transition before the pipeline has a position.
const NOT_RUNNING_YET: &str = "query pipeline position";

use super::flows::{self, MixerBackend, MonitorOut, AUDIO_MIXER, MIXER_BLOCK, PGM_TAP, SLOT_TAP};
use super::meters::{self, EngineLevels, FlowError};
use super::{Engine, EngineError, LivePicture, Picture};
use crate::switcher::{Command, AUDIO_RAMP_MS};

pub struct StromEngine {
    base_url: String,
    http: reqwest::Client,
    backend: MixerBackend,
    monitor: MonitorOut,
    /// Mixer input that shows black; set by `start`.
    black: AtomicUsize,
    levels: watch::Sender<EngineLevels>,
    errors: broadcast::Sender<FlowError>,
    /// The meter reader runs once, from the first `start`.
    metering: AtomicBool,
    /// Directory of the live picture sockets (see [`Engine::live`]).
    live_dir: PathBuf,
    /// One live flow is set up at a time (two Consoles asking at once).
    live_setup: Mutex<()>,
}

impl StromEngine {
    pub fn new(base_url: String, backend: MixerBackend, monitor: MonitorOut) -> Self {
        Self {
            base_url,
            http: reqwest::Client::new(),
            backend,
            monitor,
            black: AtomicUsize::new(0),
            levels: watch::Sender::default(),
            errors: broadcast::channel(ERROR_BUFFER).0,
            metering: AtomicBool::new(false),
            live_dir: default_live_dir(),
            live_setup: Mutex::new(()),
        }
    }

    /// Live picture sockets in `dir` instead of [`default_live_dir`].
    pub fn with_live_dir(mut self, dir: PathBuf) -> Self {
        self.live_dir = dir;
        self
    }

    /// GStreamer version strom runs on (`/api/version`).
    async fn gstreamer_version(&self) -> Option<String> {
        #[derive(serde::Deserialize)]
        struct Version {
            gstreamer_version: String,
        }
        let resp = self.http.get(self.url("/version")).send().await.ok()?;
        let version: Version = check(resp).await.ok()?.json().await.ok()?;
        Some(version.gstreamer_version)
    }

    fn url(&self, path: &str) -> String {
        format!("{}/api{}", self.base_url, path)
    }

    // ---- Flows ----

    async fn get_flow(&self, id: FlowId) -> Result<Option<Flow>, EngineError> {
        let resp = self
            .http
            .get(self.url(&format!("/flows/{id}")))
            .send()
            .await
            .map_err(net)?;
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
        let resp = self
            .http
            .post(self.url("/flows"))
            .json(flow)
            .send()
            .await
            .map_err(net)?;
        check(resp).await?;
        self.post_empty(&format!("/flows/{}/start", flow.id)).await
    }

    /// Stop and delete; a missing flow is fine.
    async fn drop_flow(&self, id: FlowId) -> Result<(), EngineError> {
        let resp = self
            .http
            .delete(self.url(&format!("/flows/{id}")))
            .send()
            .await
            .map_err(net)?;
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
        Err(EngineError::Refused(format!(
            "flow {id} did not reach PLAYING"
        )))
    }

    async fn mixer_state(&self) -> Result<VisionMixerState, EngineError> {
        let path = format!("/flows/{}/blocks/{MIXER_BLOCK}/state", flows::mixer_id());
        let resp = self.http.get(self.url(&path)).send().await.map_err(net)?;
        check(resp).await?.json().await.map_err(net)
    }

    /// Transition from whatever the engine has on program to `slot`.
    async fn program(
        &self,
        slot: Option<usize>,
        effect: Effect,
        duration_ms: u32,
    ) -> Result<(), EngineError> {
        let to = slot.unwrap_or_else(|| self.black.load(Ordering::Relaxed));
        let state = self.mixer_state().await?;
        let from = state.program_input;
        if from == Some(to) {
            return Ok(());
        }

        // strom takes whatever is in its preview and ignores `to_input`:
        // put the target there first and give the old preview back after.
        let borrowed = state.preview_input.filter(|p| *p != to);
        if borrowed.is_some() || state.preview_input.is_none() {
            self.select_preview(to).await?;
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
            let warming_up =
                matches!(&result, Err(EngineError::Refused(m)) if m.contains(NOT_RUNNING_YET));
            attempt += 1;
            if !warming_up || attempt >= PLAYING_POLLS {
                result?;
                break;
            }
            tokio::time::sleep(PLAYING_POLL).await;
        }

        match borrowed {
            Some(preview) => self.select_preview(preview).await,
            None => Ok(()),
        }
    }

    async fn select_preview(&self, input: usize) -> Result<(), EngineError> {
        let body = SelectPreviewRequest {
            source: Source::Input(input),
        };
        self.mixer_call("preview", &body).await
    }

    /// Fader and mute of a slot's audio mixer channel. strom maps both to
    /// one volume element and restores the pre-mute volume on unmute, so a
    /// batch with both loses the fader: unmute before the fader, mute after.
    async fn set_channel(
        &self,
        slot: usize,
        gain: f64,
        muted: bool,
        ramp_ms: u32,
    ) -> Result<(), EngineError> {
        let channel = flows::audio_channel(slot);
        let fader = (format!("ch{channel}_fader"), PropertyValue::Float(gain));
        let mute = (format!("ch{channel}_mute"), PropertyValue::Bool(muted));
        let order = if muted { [fader, mute] } else { [mute, fader] };
        for property in order {
            self.patch_audio_mixer(vec![property], ramp_ms).await?;
        }
        Ok(())
    }

    /// How long a compositor pad repeats its last frame: forever (hold)
    /// or briefly, then the mixer's black background shows (PGM and
    /// multiview alike).
    async fn set_signal_loss(&self, slot: usize, mode: SignalLoss) -> Result<(), EngineError> {
        // A stopped source ends its stream (EOS): without
        // `repeat-after-eos` the pad then drops its last frame.
        let (repeat_ns, after_eos) = match mode {
            SignalLoss::Hold => (REPEAT_FOREVER_NS, true),
            SignalLoss::Black => (BLACK_AFTER.as_nanos() as u64, false),
        };
        let properties = [
            ("max-last-buffer-repeat", PropertyValue::UInt(repeat_ns)),
            ("repeat-after-eos", PropertyValue::Bool(after_eos)),
        ];
        for compositor in MIXER_COMPOSITORS {
            let path = format!(
                "/flows/{}/elements/{MIXER_BLOCK}:{compositor}/pads/sink_{slot}/properties",
                flows::mixer_id()
            );
            for (name, value) in &properties {
                let body = serde_json::json!({ "property_name": name, "value": value });
                let resp = self
                    .http
                    .patch(self.url(&path))
                    .json(&body)
                    .send()
                    .await
                    .map_err(net)?;
                check(resp).await?;
            }
        }
        Ok(())
    }

    /// Channel strip: the audio mixer's channel properties, then the
    /// channel delay on its own element.
    async fn set_strip(&self, slot: usize, strip: &ChannelStrip) -> Result<(), EngineError> {
        self.patch_audio_mixer(strip_properties(flows::audio_channel(slot), strip), 0)
            .await?;
        let path = format!(
            "/flows/{}/elements/{}/properties",
            flows::mixer_id(),
            flows::audio_delay(slot)
        );
        let delay_ns = i64::from(strip.delay_ms) * 1_000_000;
        let body = serde_json::json!({
            "property_name": "ts-offset",
            "value": PropertyValue::Int(delay_ns),
        });
        let resp = self
            .http
            .patch(self.url(&path))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        check(resp).await.map(|_| ())
    }

    /// T-bar: the preview input's pad on the program compositor comes in
    /// front (below the overlays) with the bar's position as opacity. The
    /// take at the end, a cut, sets every pad back (strom resets them on
    /// each take).
    async fn t_bar(&self, slot: usize, position: f32) -> Result<(), EngineError> {
        let path = format!(
            "/flows/{}/elements/{MIXER_BLOCK}:mixer/pads/sink_{slot}/properties",
            flows::mixer_id()
        );
        let properties = [
            (
                "zorder",
                PropertyValue::UInt(u64::from(TRANSITION_FOREGROUND_ZORDER)),
            ),
            ("alpha", PropertyValue::Float(f64::from(position))),
        ];
        for (name, value) in properties {
            let body = serde_json::json!({ "property_name": name, "value": value });
            let resp = self
                .http
                .patch(self.url(&path))
                .json(&body)
                .send()
                .await
                .map_err(net)?;
            check(resp).await?;
        }
        Ok(())
    }

    /// Overlay `channel` is DSK layer `channel + 1` of the vision mixer,
    /// fed by its own `inter` input: point that input at the slot, then
    /// fade the layer in; off fades it out and leaves the input as is.
    async fn overlay(
        &self,
        channel: usize,
        slot: Option<usize>,
        duration_ms: u32,
    ) -> Result<(), EngineError> {
        self.dsk_layer(flows::overlay_layer(channel), slot, duration_ms)
            .await
    }

    /// Show `slot` on DSK `layer` (0-based) or take the layer off.
    async fn dsk_layer(
        &self,
        layer: usize,
        slot: Option<usize>,
        duration_ms: u32,
    ) -> Result<(), EngineError> {
        if let Some(slot) = slot {
            let path = format!(
                "/flows/{}/elements/{}:intersrc/properties",
                flows::mixer_id(),
                flows::layer_input(layer)
            );
            let body = serde_json::json!({
                "property_name": "producer-name",
                "value": PropertyValue::String(flows::slot_channel(slot)),
            });
            let resp = self
                .http
                .patch(self.url(&path))
                .json(&body)
                .send()
                .await
                .map_err(net)?;
            check(resp).await?;
        }
        let body = serde_json::json!({
            "dsk": layer + 1,
            "enabled": slot.is_some(),
            "duration_ms": duration_ms,
        });
        self.mixer_call("dsk", &body).await
    }

    /// Stinger on: its clip from the start on the stinger layer; off:
    /// layer away, clip paused (it would loop unseen otherwise).
    async fn stinger(&self, slot: usize, on: bool) -> Result<(), EngineError> {
        if !on {
            self.dsk_layer(flows::STINGER_LAYER, None, 0).await?;
            return self.transport(slot, Transport::Pause).await;
        }
        self.transport(slot, Transport::Restart).await?;
        self.dsk_layer(flows::STINGER_LAYER, Some(slot), 0).await
    }

    /// Media player control; restart is a seek to 0 plus play.
    async fn transport(&self, slot: usize, transport: Transport) -> Result<(), EngineError> {
        let player = format!(
            "/flows/{}/blocks/{}/player",
            flows::slot_id(slot),
            flows::PLAYER
        );
        let action = match transport {
            Transport::Play => "play",
            Transport::Pause => "pause",
            Transport::Next => "next",
            Transport::Previous => "previous",
            Transport::Restart => {
                let body = serde_json::json!({ "position_ns": 0 });
                let url = self.url(&format!("{player}/seek"));
                let resp = self.http.post(url).json(&body).send().await.map_err(net)?;
                check(resp).await?;
                "play"
            }
        };
        let body = serde_json::json!({ "action": action });
        let url = self.url(&format!("{player}/control"));
        let resp = self.http.post(url).json(&body).send().await.map_err(net)?;
        check(resp).await.map(|_| ())
    }

    /// PFL and AFL of a channel (separate volume elements: one batch).
    /// strom flips the monitor bus between program and solos itself.
    async fn set_listen(&self, slot: usize, listen: Listen) -> Result<(), EngineError> {
        let channel = flows::audio_channel(slot);
        let properties = vec![
            (
                format!("ch{channel}_pfl"),
                PropertyValue::Bool(listen == Listen::Pfl),
            ),
            (
                format!("ch{channel}_afl"),
                PropertyValue::Bool(listen == Listen::Afl),
            ),
        ];
        self.patch_audio_mixer(properties, AUDIO_RAMP_MS).await
    }

    async fn patch_audio_mixer(
        &self,
        properties: Vec<(String, PropertyValue)>,
        ramp_ms: u32,
    ) -> Result<(), EngineError> {
        let body = UpdateBlockPropertiesRequest {
            properties: properties.into_iter().collect(),
            ramp_ms: Some(ramp_ms),
            ramp_ms_overrides: None,
        };
        let path = format!(
            "/flows/{}/blocks/{AUDIO_MIXER}/properties",
            flows::mixer_id()
        );
        let resp = self
            .http
            .patch(self.url(&path))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        check(resp).await.map(|_| ())
    }

    async fn post_empty(&self, path: &str) -> Result<(), EngineError> {
        let resp = self.http.post(self.url(path)).send().await.map_err(net)?;
        check(resp).await.map(|_| ())
    }
}

#[async_trait]
impl Engine for StromEngine {
    /// strom's process start time (`/api/version`).
    async fn instance(&self) -> Option<String> {
        #[derive(serde::Deserialize)]
        struct Version {
            process_started_at: String,
        }
        let resp = self.http.get(self.url("/version")).send().await.ok()?;
        let version: Version = check(resp).await.ok()?.json().await.ok()?;
        Some(version.process_started_at)
    }

    /// Keep a running mixer with the right slot count (no picture drop when
    /// lmix-core restarts); rebuild it otherwise.
    async fn start(&self, slots: usize) -> Result<(), EngineError> {
        if !self.metering.swap(true, Ordering::Relaxed) {
            let ws_url = self.base_url.replacen("http", "ws", 1) + "/api/ws";
            tokio::spawn(meters::follow(
                ws_url,
                flows::mixer_id(),
                AUDIO_MIXER,
                self.levels.clone(),
                self.errors.clone(),
            ));
        }

        let black = flows::black_input(slots);
        self.black.store(black, Ordering::Relaxed);

        let wanted = flows::mixer_flow(slots, self.backend, self.monitor);
        let current = self.get_flow(wanted.id).await?;
        let reusable = current.as_ref().is_some_and(|f| {
            f.running && f.name == flows::MIXER_NAME && slot_count(f) == Some(black + 1)
        });
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
            Command::SetSlot {
                slot,
                source: Some(source),
            } => {
                self.put_flow(&flows::slot_flow(*slot, source, self.backend))
                    .await
            }
            Command::SetSlot { slot, source: None } => self.drop_flow(flows::slot_id(*slot)).await,
            Command::Preview { slot } => self.select_preview(*slot).await,
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
            Command::SetAudio {
                slot,
                gain,
                muted,
                ramp_ms,
            } => self.set_channel(*slot, *gain, *muted, *ramp_ms).await,
            Command::SetListen { slot, listen } => self.set_listen(*slot, *listen).await,
            Command::Transport { slot, transport } => self.transport(*slot, *transport).await,
            Command::SetSignalLoss { slot, mode } => self.set_signal_loss(*slot, *mode).await,
            Command::StartOutput {
                id,
                spec,
                feed,
                aux,
            } => {
                self.put_flow(&flows::output_flow(*id, spec, *feed, *aux, self.backend))
                    .await
            }
            Command::StopOutput { id } => self.drop_flow(flows::output_flow_id(*id)).await,
            Command::Overlay {
                channel,
                slot,
                duration_ms,
            } => self.overlay(*channel, *slot, *duration_ms).await,
            Command::TBar { slot, position } => self.t_bar(*slot, *position).await,
            Command::SetStrip { slot, strip } => self.set_strip(*slot, strip).await,
            Command::Stinger { slot, on } => self.stinger(*slot, *on).await,
            // The service waits; nothing for the engine to do.
            Command::Wait { .. } => Ok(()),
            Command::SetAuxSend { slot, bus, gain } => {
                let channel = flows::audio_channel(*slot);
                let name = format!("ch{channel}_aux{}_level", bus + 1);
                self.patch_audio_mixer(vec![(name, PropertyValue::Float(*gain))], AUDIO_RAMP_MS)
                    .await
            }
            Command::SetAuxBus { bus, gain, mute } => {
                let properties = vec![
                    (format!("aux{}_fader", bus + 1), PropertyValue::Float(*gain)),
                    (format!("aux{}_mute", bus + 1), PropertyValue::Bool(*mute)),
                ];
                self.patch_audio_mixer(properties, AUDIO_RAMP_MS).await
            }
        }
    }

    async fn live(&self, picture: LivePicture) -> Result<PathBuf, EngineError> {
        let socket = self.live_dir.join(flows::live_socket_name(picture));
        let id = flows::live_flow_id(picture);
        let _setup = self.live_setup.lock().await;
        let running = self.get_flow(id).await?.is_some_and(|f| f.running);
        if running && socket.exists() {
            return Ok(socket);
        }
        std::fs::create_dir_all(&self.live_dir).map_err(|e| {
            EngineError::Refused(format!("live directory {}: {e}", self.live_dir.display()))
        })?;
        let path = socket
            .to_str()
            .ok_or_else(|| EngineError::Refused("live directory is not UTF-8".into()))?;
        if path.len() > MAX_SOCKET_PATH {
            return Err(EngineError::Refused(format!(
                "live socket path longer than {MAX_SOCKET_PATH} bytes, choose a shorter --live-dir: {path}"
            )));
        }
        let sink =
            flows::FdSink::for_gstreamer(&self.gstreamer_version().await.unwrap_or_default());
        self.put_flow(&flows::live_flow(picture, path, self.backend, sink))
            .await?;
        self.wait_playing(id).await?;
        Ok(socket)
    }

    /// strom answers 504 while a tap has no frame yet (the first request
    /// switches it on).
    async fn picture(&self, picture: Picture) -> Result<Vec<u8>, EngineError> {
        let (flow, block) = match picture {
            Picture::Program => (flows::mixer_id(), PGM_TAP),
            Picture::Slot(slot) => (flows::slot_id(slot), SLOT_TAP),
        };
        let path = format!("/flows/{flow}/blocks/{block}/thumbnail");
        let resp = self.http.get(self.url(&path)).send().await.map_err(net)?;
        if resp.status() == StatusCode::GATEWAY_TIMEOUT {
            return Err(EngineError::NotReady);
        }
        let bytes = check(resp).await?.bytes().await.map_err(net)?;
        Ok(bytes.to_vec())
    }

    fn levels(&self) -> watch::Receiver<EngineLevels> {
        self.levels.subscribe()
    }

    fn flow_errors(&self) -> broadcast::Receiver<FlowError> {
        self.errors.subscribe()
    }

    /// Buffer-age probe on the mixer's program output: with base time 0
    /// in every flow and timestamps kept over `inter`, a buffer's age there
    /// is the time since its source captured it.
    async fn measure_latency(&self, seconds: u32) -> Result<(), EngineError> {
        self.levels.send_modify(|l| l.program_ages_ms.clear());
        let body = serde_json::json!({
            "element_id": flows::PGM_OUT,
            "sample_interval": 1,
            "timeout_secs": seconds,
        });
        let path = format!("/flows/{}/probes", flows::mixer_id());
        let resp = self
            .http
            .post(self.url(&path))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        check(resp).await.map(|_| ())
    }

    async fn output_running(&self, id: OutputId) -> bool {
        matches!(self.get_flow(flows::output_flow_id(id)).await, Ok(Some(flow)) if flow.running)
    }

    /// SRT sender statistics of the output's flow, summed over its
    /// receivers (a listener may serve several).
    async fn output_stats(&self, id: OutputId) -> Option<OutputStats> {
        let path = format!("/flows/{}/srt-stats", flows::output_flow_id(id));
        let resp = self.http.get(self.url(&path)).send().await.ok()?;
        let stats: SrtStatsResponse = check(resp).await.ok()?.json().await.ok()?;
        Some(sender_stats(id, &stats.stats))
    }

    /// Cameras, microphones and NDI sources, in that order.
    async fn devices(&self) -> Result<Vec<Device>, EngineError> {
        let mut devices = Vec::new();
        for (category, kind) in DEVICE_CATEGORIES {
            let url = self.url(&format!("/discovery/devices?category={category}"));
            let resp = self.http.get(url).send().await.map_err(net)?;
            let found: Vec<DeviceResponse> = check(resp).await?.json().await.map_err(net)?;
            devices.extend(found.into_iter().map(|d| Device {
                api: d.api_label(),
                id: d.id,
                name: d.name,
                kind,
            }));
        }
        Ok(devices)
    }
}

/// strom audio mixer properties of channel `channel` (1-based) for a strip.
fn strip_properties(channel: usize, strip: &ChannelStrip) -> Vec<(String, PropertyValue)> {
    let f = |v: f32| PropertyValue::Float(f64::from(v));
    let ch = |name: &str| format!("ch{channel}_{name}");
    let mut properties = vec![
        (ch("gain"), f(strip.gain_db)),
        (ch("pan"), f(strip.pan)),
        (ch("hpf_enabled"), PropertyValue::Bool(strip.hpf.on)),
        (ch("hpf_freq"), f(strip.hpf.hz)),
        (ch("eq_enabled"), PropertyValue::Bool(strip.eq.on)),
        (ch("gate_enabled"), PropertyValue::Bool(strip.gate.on)),
        (ch("gate_threshold"), f(strip.gate.threshold_db)),
        (ch("comp_enabled"), PropertyValue::Bool(strip.compressor.on)),
        (ch("comp_threshold"), f(strip.compressor.threshold_db)),
        (ch("comp_ratio"), f(strip.compressor.ratio)),
    ];
    for (band, gain) in strip.eq.gain_db.iter().enumerate() {
        properties.push((ch(&format!("eq{}_gain", band + 1)), f(*gain)));
    }
    properties
}

/// One output's view of strom's SRT statistics.
fn sender_stats(id: OutputId, stats: &SrtStats) -> OutputStats {
    let callers: Vec<_> = stats
        .connections
        .values()
        .filter(|c| c.role == SrtRole::Sink)
        .flat_map(|c| &c.callers)
        .collect();
    let rate_mbps: f64 = callers.iter().filter_map(|c| c.send_rate_mbps).sum();
    OutputStats {
        output: id,
        connected: stats
            .connections
            .values()
            .any(|c| c.role == SrtRole::Sink && c.connected),
        kbps: (rate_mbps * 1000.0).round() as u32,
        packets_lost: callers.iter().filter_map(|c| c.packets_sent_lost).sum(),
        packets_dropped: callers.iter().filter_map(|c| c.packets_sent_dropped).sum(),
        rtt_ms: callers
            .iter()
            .filter_map(|c| c.rtt_ms)
            .reduce(f64::max)
            .map(|ms| ms.round() as u32),
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
        // lmix-core plays stingers itself and cuts underneath.
        Effect::Stinger => "cut",
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

/// Longest Unix socket path (`sun_path` holds 108 bytes with the NUL).
const MAX_SOCKET_PATH: usize = 107;

/// `$XDG_RUNTIME_DIR/lmix/live` (private to the user), else the temp dir.
/// strom and the Console run as the same user on the engine machine.
pub fn default_live_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("lmix")
        .join("live")
}

#[cfg(test)]
mod tests {
    use super::*;
    use strom_types::api::{SrtCallerStats, SrtConnectionStats};

    fn caller(rate_mbps: f64, lost: u64, rtt_ms: f64) -> SrtCallerStats {
        SrtCallerStats {
            send_rate_mbps: Some(rate_mbps),
            packets_sent_lost: Some(lost),
            packets_sent_dropped: Some(1),
            rtt_ms: Some(rtt_ms),
            ..Default::default()
        }
    }

    #[test]
    fn sender_stats_sum_receivers_and_ignore_inputs() {
        let sink = SrtConnectionStats {
            role: SrtRole::Sink,
            connected: true,
            callers: vec![caller(4.0, 2, 12.4), caller(2.5, 3, 30.6)],
            ..Default::default()
        };
        let source = SrtConnectionStats {
            role: SrtRole::Source,
            connected: true,
            callers: vec![caller(9.0, 100, 99.0)],
            ..Default::default()
        };
        let stats = SrtStats {
            connections: [("srt:srtsink".into(), sink), ("in:srtsrc".into(), source)].into(),
        };
        let id = OutputId::new();

        let out = sender_stats(id, &stats);

        assert_eq!(
            out,
            OutputStats {
                output: id,
                connected: true,
                kbps: 6500,
                packets_lost: 5,
                packets_dropped: 2,
                rtt_ms: Some(31),
            }
        );
    }

    #[test]
    fn strip_maps_to_the_mixer_channel() {
        let mut strip = ChannelStrip::default();
        strip.eq.on = true;
        strip.eq.gain_db[2] = -3.0;
        strip.pan = -0.5;

        let properties: std::collections::HashMap<_, _> =
            strip_properties(3, &strip).into_iter().collect();

        assert!(matches!(
            properties["ch3_eq_enabled"],
            PropertyValue::Bool(true)
        ));
        assert!(matches!(properties["ch3_eq3_gain"], PropertyValue::Float(g) if g == -3.0));
        assert!(matches!(properties["ch3_pan"], PropertyValue::Float(p) if p == -0.5));
        assert!(matches!(
            properties["ch3_hpf_enabled"],
            PropertyValue::Bool(false)
        ));
        assert!(properties.keys().all(|k| k.starts_with("ch3_")));
    }

    #[test]
    fn sender_stats_without_receiver() {
        let stats = SrtStats {
            connections: [("srt:srtsink".into(), SrtConnectionStats::default())].into(),
        };

        let out = sender_stats(OutputId::new(), &stats);

        assert!(!out.connected);
        assert_eq!((out.kbps, out.rtt_ms), (0, None));
    }
}
