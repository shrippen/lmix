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
//! | `Seek`, `PlayItem` | the player's `seek` and `goto` (then play) |

use async_trait::async_trait;
use lmix_types::{
    BoxRect, ChannelStrip, Device, DeviceKind, Effect, Listen, Look, OutputId, OutputStats,
    SignalLoss, Transport,
};
use reqwest::StatusCode;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use strom_types::api::{
    FadeToBlackRequest, SelectPreviewRequest, SrtRole, SrtStats, SrtStatsResponse,
    TriggerTransitionRequest, UpdateBlockPropertiesRequest, VisionMixerState,
};
use strom_types::discovery::DeviceResponse;
use strom_types::vision_mixer::{NormRect, Source, Zone, ZoneBorder, TRANSITION_FOREGROUND_ZORDER};
use strom_types::{Flow, FlowId, PipelineState, PropertyValue};
use tokio::sync::{broadcast, watch, Mutex};

/// How often and how long `start` waits for the mixer to play.
const PLAYING_POLL: Duration = Duration::from_millis(200);
const PLAYING_POLLS: u32 = 50;
/// Flow errors kept for a slow reader.
const ERROR_BUFFER: usize = 64;

/// Player position reports kept for a slow reader (16 players, 1 s).
const PLAYER_BUFFER: usize = 128;
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

/// Frame colour of layout boxes.
const LAYOUT_BORDER_COLOR: &str = "#FFFFFF";

use super::flows::{self, MixerBackend, MonitorOut, AUDIO_MIXER, MIXER_BLOCK, PGM_TAP, SLOT_TAP};
use super::meters::{self, EngineHealth, EngineLevels, FlowError};
use super::{Engine, EngineError, LivePicture, Picture};
use crate::switcher::{Command, EngineBox, AUDIO_RAMP_MS};

pub struct StromEngine {
    base_url: String,
    http: reqwest::Client,
    backend: MixerBackend,
    monitor: MonitorOut,
    /// Mixer input that shows black; set by `start`.
    black: AtomicUsize,
    levels: watch::Sender<EngineLevels>,
    errors: broadcast::Sender<FlowError>,
    health: Arc<std::sync::Mutex<EngineHealth>>,
    players: broadcast::Sender<crate::clips::PlayerProgress>,
    /// The meter reader runs once, from the first `start`.
    metering: AtomicBool,
    /// Directory of the live picture sockets (see [`Engine::live`]).
    live_dir: PathBuf,
    /// Where the engine reaches lmix-core's pages (graphics channels).
    pages: String,
    /// One live flow is set up at a time (two Consoles asking at once).
    live_setup: Mutex<()>,
    /// Mute of each audio mixer channel and bus as last sent (see
    /// [`level_writes`]); forgotten at every start.
    mutes: std::sync::Mutex<std::collections::HashMap<String, bool>>,
    /// Program format the mixer is built for (OUT.1).
    format: std::sync::Mutex<lmix_types::MasterFormat>,
    /// Multiview layout of the mixer (OUT.10), with `format`.
    multiview: std::sync::Mutex<lmix_types::MultiviewLayout>,
    /// Sample rate of the audio mixer (AUD.17), with `format`.
    sample_rate: std::sync::Mutex<u32>,
    /// Mixer inputs of the last `start`.
    slots: AtomicUsize,
}

/// Longest wait for one engine call. Commands run under the switcher
/// lock, so a call that never returns would freeze lmix-core; the slowest
/// real ones (a recording drained with EOS, a mixer rebuilt) take seconds.
const ENGINE_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

/// HTTP client for strom's API: a hung call fails after `timeout`.
fn engine_client(timeout: std::time::Duration) -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(timeout)
        .build()
        .expect("HTTP client builds")
}

impl StromEngine {
    pub fn new(base_url: String, backend: MixerBackend, monitor: MonitorOut) -> Self {
        Self {
            base_url,
            http: engine_client(ENGINE_CALL_TIMEOUT),
            backend,
            monitor,
            black: AtomicUsize::new(0),
            levels: watch::Sender::default(),
            errors: broadcast::channel(ERROR_BUFFER).0,
            health: Arc::default(),
            players: broadcast::channel(PLAYER_BUFFER).0,
            metering: AtomicBool::new(false),
            live_dir: default_live_dir(),
            pages: flows::DEFAULT_PAGES.into(),
            live_setup: Mutex::new(()),
            mutes: Default::default(),
            format: Default::default(),
            multiview: Default::default(),
            sample_rate: std::sync::Mutex::new(lmix_types::output::default_sample_rate()),
            slots: AtomicUsize::new(0),
        }
    }

    /// Live picture sockets in `dir` instead of [`default_live_dir`].
    pub fn with_live_dir(mut self, dir: PathBuf) -> Self {
        self.live_dir = dir;
        self
    }

    /// lmix-core's address as the engine reaches it, for graphics pages.
    pub fn with_pages(mut self, url: String) -> Self {
        self.pages = url;
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
        self.take_source(Source::Input(to), effect, duration_ms)
            .await
    }

    /// Transition from what is on program to `to` (an input or a
    /// composition). strom takes whatever is in its preview: put `to`
    /// there first and give the old preview back after.
    async fn take_source(
        &self,
        to: Source,
        effect: Effect,
        duration_ms: u32,
    ) -> Result<(), EngineError> {
        let state = self.mixer_state().await?;
        let on = |pip: Option<usize>, input: Option<usize>| match pip {
            Some(pip) => Some(Source::Pip(pip)),
            None => input.map(Source::Input),
        };
        let from = on(state.program_pip, state.program_input);
        if from == Some(to) {
            return Ok(());
        }
        let preview = on(state.preview_pip, state.preview_input);
        let borrowed = preview.filter(|p| *p != to);
        if borrowed.is_some() || preview.is_none() {
            self.select_source(to).await?;
        }

        // strom reads the inputs only when no composition is involved.
        let body = TriggerTransitionRequest {
            from_input: Some(state.program_input.unwrap_or(0)),
            to_input: Some(to.as_input().unwrap_or(0)),
            transition_type: wire_name(effect).into(),
            duration_ms: u64::from(duration_ms),
            stinger_clip: None,
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
            Some(preview) => self.select_source(preview).await,
            None => Ok(()),
        }
    }

    async fn select_preview(&self, input: usize) -> Result<(), EngineError> {
        self.select_source(Source::Input(input)).await
    }

    async fn select_source(&self, source: Source) -> Result<(), EngineError> {
        let body = SelectPreviewRequest { source };
        self.mixer_call("preview", &body).await
    }

    /// A layout as strom composition `pip`: the background, then one
    /// zone per box (bottom to top) holding just its input.
    async fn set_layout(
        &self,
        pip: usize,
        background: Option<usize>,
        boxes: &[EngineBox],
    ) -> Result<(), EngineError> {
        let zones: Vec<Zone> = boxes
            .iter()
            .map(|b| Zone {
                rect: Some(NormRect {
                    x: b.rect.x,
                    y: b.rect.y,
                    w: b.rect.w,
                    h: b.rect.h,
                }),
                capacity: Some(1),
                sources: vec![b.slot],
                border: (b.border_px > 0).then(|| ZoneBorder {
                    color: LAYOUT_BORDER_COLOR.into(),
                    width: f32::from(b.border_px),
                }),
            })
            .collect();
        // strom crops per source, wherever it shows in the composition.
        let transforms: serde_json::Map<String, serde_json::Value> = boxes
            .iter()
            .filter(|b| !b.crop.is_zero())
            .map(|b| {
                let c = b.crop;
                let crop = serde_json::json!({
                    "left": c.left, "top": c.top, "right": c.right, "bottom": c.bottom
                });
                (b.slot.to_string(), crop)
            })
            .collect();
        let body =
            serde_json::json!({ "bg": background, "zones": zones, "transforms": transforms });
        let path = format!(
            "/flows/{}/blocks/{MIXER_BLOCK}/pip/{pip}",
            flows::mixer_id()
        );
        let resp = self
            .http
            .put(self.url(&path))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        check(resp).await.map(|_| ())
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
        let level = Level {
            fader: format!("ch{channel}_fader"),
            mute: format!("ch{channel}_mute"),
            gain,
            muted,
        };
        self.set_level(level, ramp_ms).await
    }

    /// The mixer flow for the last `start`, the current format and
    /// multiview.
    fn mixer_flow(&self) -> Flow {
        let format = *self.format.lock().expect("format lock");
        let multiview = *self.multiview.lock().expect("multiview lock");
        let sample_rate = *self.sample_rate.lock().expect("rate lock");
        flows::mixer_flow(
            self.slots.load(Ordering::Relaxed),
            self.backend,
            self.monitor,
            format,
            multiview,
            sample_rate,
        )
    }

    /// strom starts a fresh mixer on input 0; the restore that follows
    /// puts the show's program (or black) there. Transitions need a
    /// playing pipeline, so wait for it. Its mutes are unknown again.
    async fn build_mixer(&self, flow: &Flow) -> Result<(), EngineError> {
        self.mutes.lock().expect("mutes lock").clear();
        self.put_flow(flow).await?;
        self.wait_playing(flow.id).await
    }

    /// Program in `format` (OUT.1): a new mixer only when it differs.
    async fn set_format(
        &self,
        format: lmix_types::MasterFormat,
        multiview: lmix_types::MultiviewLayout,
        sample_rate: u32,
    ) -> Result<(), EngineError> {
        {
            let mut current = self.format.lock().expect("format lock");
            let mut current_view = self.multiview.lock().expect("multiview lock");
            let mut current_rate = self.sample_rate.lock().expect("rate lock");
            if *current == format && *current_view == multiview && *current_rate == sample_rate {
                return Ok(());
            }
            *current = format;
            *current_view = multiview;
            *current_rate = sample_rate;
        }
        let flow = self.mixer_flow();
        self.build_mixer(&flow).await
    }

    /// A fader and its mute on the audio mixer, one write at a time.
    async fn set_level(&self, level: Level, ramp_ms: u32) -> Result<(), EngineError> {
        let known = self
            .mutes
            .lock()
            .expect("mutes lock")
            .get(&level.mute)
            .copied();
        for property in level_writes(&level, known) {
            self.patch_audio_mixer(vec![property], ramp_ms).await?;
        }
        self.mutes
            .lock()
            .expect("mutes lock")
            .insert(level.mute, level.muted);
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

    /// Overlay `channel` in `rect` of the program picture (OVL.2), or the
    /// whole picture: the geometry of its DSK pad. strom sets it only when
    /// it builds the mixer and keeps it through takes; the pad keeps the
    /// input's aspect inside it.
    async fn overlay_place(
        &self,
        channel: usize,
        rect: Option<BoxRect>,
    ) -> Result<(), EngineError> {
        let format = *self.format.lock().expect("format lock");
        let (w, h) = (f64::from(format.width), f64::from(format.height));
        let rect = rect.unwrap_or(BoxRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        });
        let px = |v: f32, size: f64| (f64::from(v) * size).round() as i64;
        let path = format!(
            "/flows/{}/elements/{MIXER_BLOCK}:mixer/pads/{}/properties",
            flows::mixer_id(),
            flows::dsk_pad(
                self.black.load(Ordering::Relaxed),
                flows::overlay_layer(channel)
            )
        );
        let properties = [
            ("xpos", px(rect.x, w)),
            ("ypos", px(rect.y, h)),
            ("width", px(rect.w, w).max(1)),
            ("height", px(rect.h, h).max(1)),
        ];
        for (name, value) in properties {
            let body =
                serde_json::json!({ "property_name": name, "value": PropertyValue::Int(value) });
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

    /// Colour correction of `slot` as strom's per-input shader look; a
    /// neutral look takes the shader away.
    async fn set_look(&self, slot: usize, look: &Look) -> Result<(), EngineError> {
        let effect = if look.is_neutral() {
            serde_json::json!({ "type": "none" })
        } else {
            serde_json::json!({
                "type": "color_correct",
                "brightness": look.brightness,
                "contrast": look.contrast,
                "saturation": look.saturation,
                "hue": look.hue,
                "gamma": look.gamma,
                "temperature": look.temperature,
                "tint": look.tint,
            })
        };
        let body = serde_json::json!({ "target": { "input": slot }, "effect": effect });
        self.mixer_call("effect", &body).await
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

    /// A call on the slot's media player (`seek`, `goto`).
    async fn player_call(
        &self,
        slot: usize,
        what: &str,
        body: serde_json::Value,
    ) -> Result<(), EngineError> {
        let path = format!(
            "/flows/{}/blocks/{}/player/{what}",
            flows::slot_id(slot),
            flows::PLAYER
        );
        let resp = self
            .http
            .post(self.url(&path))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        check(resp).await.map(|_| ())
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
    fn looks_available(&self) -> bool {
        self.backend == MixerBackend::Gl
    }

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
        // A reused or new mixer: its mutes are unknown, the restore sends
        // them all.
        self.mutes.lock().expect("mutes lock").clear();
        if !self.metering.swap(true, Ordering::Relaxed) {
            let ws_url = self.base_url.replacen("http", "ws", 1) + "/api/ws";
            tokio::spawn(meters::follow(
                ws_url,
                flows::mixer_id(),
                AUDIO_MIXER,
                self.levels.clone(),
                self.errors.clone(),
                self.health.clone(),
                self.players.clone(),
            ));
        }

        let black = flows::black_input(slots);
        self.black.store(black, Ordering::Relaxed);
        self.slots.store(slots, Ordering::Relaxed);

        let current = self.get_flow(flows::mixer_id()).await?;
        let reusable = current.as_ref().is_some_and(|f| {
            f.running && f.name == flows::MIXER_NAME && slot_count(f) == Some(black + 1)
        });
        // Keep a running mixer in whatever format it has: the restore that
        // follows brings the show's (OUT.1), rebuilding only if it differs.
        if let (true, Some(format)) = (reusable, current.as_ref().and_then(flows::format_of)) {
            *self.format.lock().expect("format lock") = format;
            if let Some(multiview) = current.as_ref().map(flows::multiview_of) {
                *self.multiview.lock().expect("multiview lock") = multiview;
            }
            if let Some(rate) = current.as_ref().and_then(flows::sample_rate_of) {
                *self.sample_rate.lock().expect("rate lock") = rate;
            }
            return Ok(());
        }
        let wanted = self.mixer_flow();
        self.build_mixer(&wanted).await
    }

    async fn run(&self, command: &Command) -> Result<(), EngineError> {
        match command {
            Command::SetSlot {
                slot,
                source: Some(source),
            } => {
                let env = flows::SlotEnv {
                    backend: self.backend,
                    format: *self.format.lock().expect("format lock"),
                    pages: self.pages.clone(),
                };
                self.put_flow(&flows::slot_flow(*slot, source, &env)).await
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
            Command::SetAtEnd {
                slot,
                items,
                at_end,
            } => {
                let body = serde_json::json!({
                    "files": items,
                    "loop_playlist": at_end.is_loop(),
                });
                self.player_call(*slot, "playlist", body).await
            }
            Command::Seek { slot, position_ms } => {
                self.player_call(
                    *slot,
                    "seek",
                    serde_json::json!({ "position_ns": position_ms.saturating_mul(1_000_000) }),
                )
                .await
            }
            Command::PlayItem { slot, index } => {
                self.player_call(*slot, "goto", serde_json::json!({ "index": index }))
                    .await?;
                self.transport(*slot, Transport::Play).await
            }
            Command::SetSignalLoss { slot, mode } => self.set_signal_loss(*slot, *mode).await,
            Command::StartOutput {
                id,
                spec,
                feed,
                aux,
            } => {
                let framerate = self.format.lock().expect("format lock").framerate;
                let flow = flows::output_flow(*id, spec, *feed, *aux, self.backend, framerate);
                self.put_flow(&flow).await
            }
            Command::StopOutput { id } => self.drop_flow(flows::output_flow_id(*id)).await,
            Command::Overlay {
                channel,
                slot,
                duration_ms,
            } => self.overlay(*channel, *slot, *duration_ms).await,
            Command::OverlayPlace { channel, rect } => self.overlay_place(*channel, *rect).await,
            Command::TBar { slot, position } => self.t_bar(*slot, *position).await,
            Command::SetStrip { slot, strip } => self.set_strip(*slot, strip).await,
            Command::Stinger { slot, on } => self.stinger(*slot, *on).await,
            Command::SetLayout {
                pip,
                background,
                boxes,
            } => self.set_layout(*pip, *background, boxes).await,
            Command::PreviewLayout { pip } => self.select_source(Source::Pip(*pip)).await,
            Command::ProgramLayout {
                pip,
                effect,
                duration_ms,
            } => {
                self.take_source(Source::Pip(*pip), *effect, *duration_ms)
                    .await
            }
            Command::SetLook { slot, look } => self.set_look(*slot, look).await,
            // The service waits; nothing for the engine to do.
            Command::Wait { .. } => Ok(()),
            Command::SetAuxSend { slot, bus, gain } => {
                let channel = flows::audio_channel(*slot);
                let name = format!("ch{channel}_aux{}_level", bus + 1);
                self.patch_audio_mixer(vec![(name, PropertyValue::Float(*gain))], AUDIO_RAMP_MS)
                    .await
            }
            Command::SetAuxBus {
                bus,
                gain,
                mute,
                ramp_ms,
            } => {
                let level = Level {
                    fader: format!("aux{}_fader", bus + 1),
                    mute: format!("aux{}_mute", bus + 1),
                    gain: *gain,
                    muted: *mute,
                };
                self.set_level(level, *ramp_ms).await
            }
            Command::SetFormat {
                format,
                multiview,
                sample_rate,
            } => self.set_format(*format, *multiview, *sample_rate).await,
            Command::SetMonitor { gain } => {
                let fader = ("monitor_fader".to_string(), PropertyValue::Float(*gain));
                self.patch_audio_mixer(vec![fader], AUDIO_RAMP_MS).await
            }
            Command::SetMaster {
                gain,
                mute,
                ramp_ms,
            } => {
                let level = Level {
                    fader: "main_fader".into(),
                    mute: "main_mute".into(),
                    gain: *gain,
                    muted: *mute,
                };
                self.set_level(level, *ramp_ms).await
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

    fn players(&self) -> broadcast::Receiver<crate::clips::PlayerProgress> {
        self.players.subscribe()
    }

    fn health(&self) -> EngineHealth {
        self.health.lock().expect("health lock").clone()
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

/// A fader of the audio mixer with its mute: names and wanted values.
struct Level {
    fader: String,
    mute: String,
    gain: f64,
    muted: bool,
}

/// What to write for `level`, in order. strom takes every `mute=false`
/// as an unmute: it fades in from silence to the level before the mute,
/// even on a channel that was not muted, and a fader ramp that follows
/// starts from that silence (AUD.14: a fade from 100 to 0 stayed silent).
/// So the mute goes only when it differs from `known` (the last sent;
/// `None` after a start); unmute before the fader, mute after it.
fn level_writes(level: &Level, known: Option<bool>) -> Vec<(String, PropertyValue)> {
    let fader = (level.fader.clone(), PropertyValue::Float(level.gain));
    if known == Some(level.muted) {
        return vec![fader];
    }
    let mute = (level.mute.clone(), PropertyValue::Bool(level.muted));
    if level.muted {
        vec![fader, mute]
    } else {
        vec![mute, fader]
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    /// An engine that takes a request and never answers fails the call
    /// instead of holding lmix-core (and its switcher lock) for good.
    #[tokio::test]
    async fn an_engine_call_does_not_wait_forever() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let mut held = Vec::new();
            while let Ok((stream, _)) = listener.accept().await {
                held.push(stream);
            }
        });
        let client = engine_client(std::time::Duration::from_millis(300));
        let started = std::time::Instant::now();
        let result = client.get(format!("http://{addr}/api/flows")).send().await;
        assert!(result.is_err());
        assert!(started.elapsed() < std::time::Duration::from_secs(5));
    }
    use strom_types::api::{SrtCallerStats, SrtConnectionStats};

    #[test]
    fn mutes_go_only_when_they_change() {
        let level = |muted| Level {
            fader: "main_fader".into(),
            mute: "main_mute".into(),
            gain: 0.5,
            muted,
        };
        let names = |writes: Vec<(String, PropertyValue)>| -> Vec<String> {
            writes.into_iter().map(|(name, _)| name).collect()
        };

        assert_eq!(
            names(level_writes(&level(false), Some(false))),
            ["main_fader"]
        );
        assert_eq!(
            names(level_writes(&level(true), Some(true))),
            ["main_fader"]
        );
        assert_eq!(
            names(level_writes(&level(false), Some(true))),
            ["main_mute", "main_fader"],
            "unmute first"
        );
        assert_eq!(
            names(level_writes(&level(true), Some(false))),
            ["main_fader", "main_mute"],
            "mute last"
        );
        assert_eq!(
            names(level_writes(&level(false), None)),
            ["main_mute", "main_fader"],
            "after a start: everything"
        );
    }

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
