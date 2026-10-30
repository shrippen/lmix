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
//! | `Stinger` | vision mixer stinger API: clip ensured in the library, then `stinger/take` with index and file; a cut if it cannot run |
//! | `StingerLibrary` | vision mixer stinger API: clips removed, added, cut points set |

use async_trait::async_trait;
use lmix_types::{
    BoxRect, BusId, ChannelStrip, Device, DeviceKind, Effect, Listen, LiveTransport, Look,
    OutputId, OutputStats, PageRunning, SignalLoss, SourceSpec, Transport,
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
use strom_types::devtools::HtmlPageCommand;
use strom_types::discovery::DeviceResponse;
use strom_types::mixer::{AddMixerAuxBusRequest, MixerAuxBuses};
use strom_types::stinger::{
    StingerAddClipRequest, StingerBeneath, StingerClip, StingerClipSettings, StingerLayout,
    StingerState, StingerTakeRequest, StingerTakeResponse,
};
use strom_types::vision_mixer::{NormRect, Source, Zone, ZoneBorder};
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
/// The mixer's compositor of the tile picture (strom's `tiles_out`).
const TILES_COMPOSITOR: &str = "tiles_comp";
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
use crate::switcher::{Command, EngineBox, LibraryClip, MixerBuses, AUDIO_RAMP_MS};

mod playback;
use playback::Hold;

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
    /// A Console asked for the tile picture; until then its pads stay
    /// dark (see [`StromEngine::show_tiles`]).
    tiles_watched: AtomicBool,
    /// One `show_tiles` at a time: a rebuild's 0 and a Console's 1 would
    /// otherwise interleave pad by pad.
    tiles_setup: Mutex<()>,
    /// Mute of each audio mixer channel and bus as last sent (see
    /// [`level_writes`]); forgotten at every start.
    mutes: std::sync::Mutex<std::collections::HashMap<String, bool>>,
    /// Program format the mixer is built for (OUT.1).
    format: std::sync::Mutex<lmix_types::MasterFormat>,
    /// Multiview layout of the mixer (OUT.10), with `format`.
    multiview: std::sync::Mutex<lmix_types::MultiviewLayout>,
    /// Sample rate of the audio mixer (AUD.17), with `format`.
    sample_rate: std::sync::Mutex<u32>,
    /// The buses the audio mixer has (AUD.2), with strom's numbers; `None`
    /// for a reused mixer that does not tell (built before), which the
    /// restore then builds anew. Shared with the meters, which name a bus
    /// by its number.
    buses: Arc<std::sync::Mutex<Option<flows::BuiltBuses>>>,
    /// Mixer inputs of the last `start`.
    slots: AtomicUsize,
    /// Ends of the stinger takes lmix starts (strom's events).
    stingers: Arc<meters::Stingers>,
    playback: playback::Playing,
    /// Whether recordings may be live MKV; set by `start` from strom's
    /// GStreamer version.
    live_mkv: std::sync::Mutex<flows::LiveMkv>,
}

/// Longest wait for one engine call. Commands run under the switcher
/// lock, so a call that never returns would freeze lmix-core; the slowest
/// real ones (a recording drained with EOS, a mixer rebuilt) take seconds.
const ENGINE_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);
/// How long the failure watcher waits for strom to tell whether a flow
/// runs.
const FLOW_CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// How often, and how many times, a new web page is asked to take its CSS.
const PAGE_READY_POLL: std::time::Duration = std::time::Duration::from_millis(500);
const PAGE_READY_TRIES: u32 = 40;

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
            tiles_watched: AtomicBool::new(false),
            tiles_setup: Mutex::new(()),
            mutes: Default::default(),
            format: Default::default(),
            multiview: Default::default(),
            sample_rate: std::sync::Mutex::new(lmix_types::output::default_sample_rate()),
            buses: Default::default(),
            slots: AtomicUsize::new(0),
            stingers: Arc::new(meters::Stingers::new(MIXER_BLOCK)),
            playback: Default::default(),
            live_mkv: std::sync::Mutex::new(flows::LiveMkv::No),
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
        let buses = self
            .buses
            .lock()
            .expect("buses lock")
            .clone()
            .unwrap_or_default();
        flows::mixer_flow(
            self.slots.load(Ordering::Relaxed),
            self.backend,
            self.monitor,
            format,
            multiview,
            sample_rate,
            &buses,
        )
    }

    /// strom starts a fresh mixer on input 0; the restore that follows
    /// puts the show's program (or black) there. Transitions need a
    /// playing pipeline, so wait for it. Its mutes are unknown again.
    async fn build_mixer(&self, flow: &Flow) -> Result<(), EngineError> {
        self.mutes.lock().expect("mutes lock").clear();
        // What it is built with, so the show's SetFormat after a cold start
        // does not build it a second time.
        *self.buses.lock().expect("buses lock") = flows::buses_of(flow);
        self.put_flow(flow).await?;
        self.wait_playing(flow.id).await?;
        self.show_tiles().await;
        Ok(())
    }

    /// The tile picture's pads at alpha 1 once a Console asked for it, else
    /// at 0: the compositor skips a pad at 0 without scaling it, and the
    /// tile picture costs the CPU mixer as much again as the rest (strom
    /// 480 % of a core against 290 % with 7 slots, 30.10.2026). A failure
    /// only costs that load or the Console its tiles, so it is logged.
    async fn show_tiles(&self) {
        let _setup = self.tiles_setup.lock().await;
        let alpha = if self.tiles_watched.load(Ordering::Relaxed) {
            1.0
        } else {
            0.0
        };
        let pads = self.black.load(Ordering::Relaxed) + 1;
        for pad in 0..pads {
            let path = format!(
                "/flows/{}/elements/{MIXER_BLOCK}:{TILES_COMPOSITOR}/pads/sink_{pad}/properties",
                flows::mixer_id()
            );
            let body = serde_json::json!({ "property_name": "alpha", "value": alpha });
            let sent = async {
                let resp = self
                    .http
                    .patch(self.url(&path))
                    .json(&body)
                    .send()
                    .await
                    .map_err(net)?;
                check(resp).await.map(|_| ())
            };
            if let Err(e) = sent.await {
                tracing::warn!("tile picture pad {pad} to alpha {alpha}: {e}");
            }
        }
    }

    /// A Console shows the tile picture: from now on its pads are lit.
    /// Each request lights them again, so a failed PATCH is not final.
    async fn watch_tiles(&self) {
        self.tiles_watched.store(true, Ordering::Relaxed);
        self.show_tiles().await;
    }

    /// Program in `format` (OUT.1): a new mixer only when it differs.
    /// Other buses alone change the running mixer (`patch_buses`).
    async fn set_format(
        &self,
        format: lmix_types::MasterFormat,
        multiview: lmix_types::MultiviewLayout,
        sample_rate: u32,
        buses: &MixerBuses,
    ) -> Result<(), EngineError> {
        // `Some(true)`: the mixer is as wanted, `Some(false)`: only its
        // buses differ, `None`: it is built anew.
        let same = {
            let mut current = self.format.lock().expect("format lock");
            let mut current_view = self.multiview.lock().expect("multiview lock");
            let mut current_rate = self.sample_rate.lock().expect("rate lock");
            let mut current_buses = self.buses.lock().expect("buses lock");
            let picture =
                *current == format && *current_view == multiview && *current_rate == sample_rate;
            match current_buses.as_ref() {
                Some(built) if picture => Some(built.are(buses)),
                _ => {
                    *current = format;
                    *current_view = multiview;
                    *current_rate = sample_rate;
                    *current_buses = Some(flows::BuiltBuses::numbered(buses));
                    None
                }
            }
        };
        match same {
            Some(true) => Ok(()),
            Some(false) => self.patch_buses(buses).await,
            None => {
                let flow = self.mixer_flow();
                let built = self.build_mixer(&flow).await;
                if built.is_err() {
                    // Unknown buses build it anew: the same wish again
                    // tries again instead of taking the failed one as done.
                    *self.buses.lock().expect("buses lock") = None;
                }
                built
            }
        }
    }

    /// Bring the running audio mixer's buses to `wanted`: strom removes
    /// and adds buses and moves taps while the mixer plays (AUD.2). Each
    /// step is recorded as it succeeds, so after a failure the next
    /// `SetFormat` does the rest.
    async fn patch_buses(&self, wanted: &MixerBuses) -> Result<(), EngineError> {
        let current = || {
            self.buses
                .lock()
                .expect("buses lock")
                .clone()
                .ok_or_else(|| EngineError::Refused("the audio mixer is being built".into()))
        };
        let record = |built: flows::BuiltBuses| {
            *self.buses.lock().expect("buses lock") = Some(built);
        };
        let slots = self.slots.load(Ordering::Relaxed);
        let wants = |id: BusId| {
            wanted
                .buses
                .iter()
                .find(|(w, _)| *w == id)
                .map(|(_, pre)| *pre)
        };
        // Commands count buses along the show from now on, also when a step
        // below fails.
        let mut built = current()?;
        built.order = wanted.buses.iter().map(|(id, _)| *id).collect();
        record(built);

        for bus in current()?.buses {
            if wants(bus.id).is_none() {
                self.remove_aux_bus(bus.aux).await?;
                let mut built = current()?;
                built.buses.retain(|b| b.id != bus.id);
                record(built);
            }
        }
        if current()?.phones_pre_fader != wanted.phones_pre_fader {
            let taps = flows::tap_properties(slots, flows::PHONES_AUX, wanted.phones_pre_fader);
            self.patch_taps(taps).await?;
            let mut built = current()?;
            built.phones_pre_fader = wanted.phones_pre_fader;
            record(built);
        }
        for bus in current()?.buses {
            match wants(bus.id) {
                Some(pre) if pre != bus.pre_fader => {
                    let taps = flows::tap_properties(slots, bus.aux, pre);
                    self.patch_taps(taps).await?;
                    let mut built = current()?;
                    for b in built.buses.iter_mut().filter(|b| b.id == bus.id) {
                        b.pre_fader = pre;
                    }
                    record(built);
                }
                _ => {}
            }
        }
        for (id, pre_fader) in &wanted.buses {
            if current()?.buses.iter().any(|b| b.id == *id) {
                continue;
            }
            let aux = self.add_aux_bus(*id, *pre_fader).await?;
            let mut built = current()?;
            built.buses.push(flows::BuiltBus {
                id: *id,
                pre_fader: *pre_fader,
                aux,
            });
            record(built);
        }
        // The show's order: engine bus numbers count along it.
        let mut built = current()?;
        built
            .buses
            .sort_by_key(|b| wanted.buses.iter().position(|(id, _)| *id == b.id));
        record(built);
        Ok(())
    }

    /// Add bus `id` to the running audio mixer, publishing on its channel.
    /// Returns strom's number for it.
    async fn add_aux_bus(&self, id: BusId, pre_fader: bool) -> Result<usize, EngineError> {
        match self.post_aux_bus(id, pre_fader).await {
            Ok(aux) => Ok(aux),
            // strom may have added it and failed after (or the answer was
            // lost): then the bus is there, on its channel, and a second add
            // would be refused for good.
            Err(e) => {
                let channel = flows::bus_channel(id);
                let found = match self.list_aux_buses().await {
                    Ok(buses) => buses
                        .dynamic
                        .iter()
                        .find(|b| b.channel == channel)
                        .map(|b| b.aux),
                    Err(_) => None,
                };
                let aux = found.ok_or(e)?;
                // Added with the tap of that first try, maybe not this one.
                let slots = self.slots.load(Ordering::Relaxed);
                self.patch_taps(flows::tap_properties(slots, aux, pre_fader))
                    .await?;
                Ok(aux)
            }
        }
    }

    async fn post_aux_bus(&self, id: BusId, pre_fader: bool) -> Result<usize, EngineError> {
        let channel = flows::bus_channel(id);
        let body = AddMixerAuxBusRequest {
            aux: None,
            channel: channel.clone(),
            pre_fader: Some(pre_fader),
        };
        let resp = self
            .http
            .post(self.url(&self.aux_buses_path()))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        let buses: MixerAuxBuses = check(resp).await?.json().await.map_err(net)?;
        buses
            .dynamic
            .iter()
            .find(|b| b.channel == channel)
            .map(|b| b.aux)
            .ok_or_else(|| EngineError::Refused(format!("strom did not list bus {channel}")))
    }

    async fn remove_aux_bus(&self, aux: usize) -> Result<(), EngineError> {
        let path = format!("{}/{aux}", self.aux_buses_path());
        let removed = match self.http.delete(self.url(&path)).send().await {
            Ok(resp) => check(resp).await.map(|_| ()),
            Err(e) => Err(net(e)),
        };
        // strom may have removed it and failed after: a bus it no longer
        // lists is gone.
        match removed {
            Err(e) => match self.list_aux_buses().await {
                Ok(buses) if !buses.dynamic.iter().any(|b| b.aux == aux) => Ok(()),
                _ => Err(e),
            },
            ok => ok,
        }
    }

    async fn list_aux_buses(&self) -> Result<MixerAuxBuses, EngineError> {
        let resp = self
            .http
            .get(self.url(&self.aux_buses_path()))
            .send()
            .await
            .map_err(net)?;
        check(resp).await?.json().await.map_err(net)
    }

    fn aux_buses_path(&self) -> String {
        format!(
            "/flows/{}/blocks/{AUDIO_MIXER}/aux_buses",
            flows::mixer_id()
        )
    }

    /// strom's aux number of engine bus `bus`.
    fn aux(&self, bus: usize) -> Result<usize, EngineError> {
        self.buses
            .lock()
            .expect("buses lock")
            .as_ref()
            .and_then(|b| b.aux(bus))
            .ok_or_else(|| EngineError::Refused(format!("the audio mixer has no bus {bus}")))
    }

    /// Sends and listening of the buses in one PATCH. A bus the mixer
    /// lacks is left out and named afterwards; the others are set.
    async fn set_bus_mix(
        &self,
        sends: &[(usize, usize, f64)],
        listen: &[(usize, bool)],
    ) -> Result<(), EngineError> {
        let mut properties = Vec::new();
        let mut missing = std::collections::BTreeSet::new();
        for &(slot, bus, gain) in sends {
            match self.aux(bus) {
                Ok(aux) => properties.push((
                    format!("ch{}_aux{aux}_level", flows::audio_channel(slot)),
                    PropertyValue::Float(gain),
                )),
                Err(_) => {
                    missing.insert(bus);
                }
            }
        }
        for &(bus, on) in listen {
            match self.aux(bus) {
                Ok(aux) => properties.push((format!("aux{aux}_afl"), PropertyValue::Bool(on))),
                Err(_) => {
                    missing.insert(bus);
                }
            }
        }
        if !properties.is_empty() {
            self.patch_audio_mixer(properties, AUDIO_RAMP_MS).await?;
        }
        match missing.first() {
            None => Ok(()),
            Some(_) => Err(EngineError::Refused(format!(
                "the audio mixer has no bus {}",
                missing
                    .iter()
                    .map(|b| b.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ))),
        }
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
    /// channel delay on its own block.
    async fn set_strip(&self, slot: usize, strip: &ChannelStrip) -> Result<(), EngineError> {
        self.patch_audio_mixer(strip_properties(flows::audio_channel(slot), strip), 0)
            .await?;
        let delay = vec![(
            "offset_ms".to_string(),
            PropertyValue::Float(f64::from(strip.delay_ms)),
        )];
        self.patch_block(flows::mixer_id(), &flows::audio_delay(slot), delay)
            .await
    }

    /// T-bar: strom holds the take from program to preview at `position`
    /// with `effect` (its `tbar`); the take at the end, a cut, sets every
    /// pad back.
    async fn t_bar(&self, position: f32, effect: Effect) -> Result<(), EngineError> {
        let body = serde_json::json!({
            "position": f64::from(position),
            "transition_type": wire_name(effect),
        });
        self.mixer_call("tbar", &body).await
    }

    /// Overlay `channel` is DSK layer `channel` of the vision mixer,
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

    // ---- Stinger (strom's stinger transitions, DECISIONS 18.10., STG) ----

    fn stinger_url(&self, rest: &str) -> String {
        self.url(&format!(
            "/flows/{}/blocks/{MIXER_BLOCK}/stinger{rest}",
            flows::mixer_id()
        ))
    }

    async fn stinger_state(&self) -> Result<StingerState, EngineError> {
        let resp = self
            .http
            .get(self.stinger_url(""))
            .send()
            .await
            .map_err(net)?;
        check(resp).await?.json().await.map_err(net)
    }

    /// Stinger from program to `to` with `clip`; strom switches program
    /// at its cut point. The clip is put in the library first when it is
    /// missing there or has another cut point (a sync that failed heals
    /// here). A stinger that cannot run costs the graphic, not the switch:
    /// program cuts, as the switcher already has it, and the error says so.
    async fn stinger(&self, to: usize, clip: &LibraryClip) -> Result<(), EngineError> {
        self.stingers.starting();
        let taken = match self.stinger_ensure(clip).await {
            Ok(index) => self.stinger_take(to, index, &clip.file).await,
            Err(e) => Err(e),
        };
        let Err(e) = taken else {
            return Ok(());
        };
        self.take_source(Source::Input(to), Effect::Cut, 0).await?;
        Err(EngineError::Refused(format!(
            "stinger {}: {e}; cut instead",
            clip.file
        )))
    }

    /// Library index of `clip`, added or its cut point set when needed.
    async fn stinger_ensure(&self, clip: &LibraryClip) -> Result<usize, EngineError> {
        let settings = clip_settings(clip);
        let library = self.stinger_state().await?;
        let have = match library.clips.iter().find(|c| c.file == clip.file) {
            None => self.stinger_add(&clip.file, settings).await?,
            Some(have) if have.settings != settings => {
                self.stinger_settings(have, settings).await?
            }
            Some(have) => have.clone(),
        };
        Ok(have.index)
    }

    /// strom's take (PGM to PVW), with `to` in its preview first. The
    /// file guards the index: a library changed meanwhile is refused.
    async fn stinger_take(&self, to: usize, index: usize, file: &str) -> Result<(), EngineError> {
        let state = self.mixer_state().await?;
        if state.preview_pip.is_some() || state.preview_input != Some(to) {
            self.select_preview(to).await?;
        }
        let body = StingerTakeRequest {
            index: Some(index),
            file: Some(file.into()),
        };
        let resp = self
            .http
            .post(self.stinger_url("/take"))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        let take: StingerTakeResponse = check(resp).await?.json().await.map_err(net)?;
        self.stingers.started(take.take_id);
        Ok(())
    }

    /// Make the stinger library these clips: others go (and duplicates),
    /// missing ones are added, cut points set. Tries every clip; the
    /// first error is returned.
    async fn stinger_library(&self, clips: &[LibraryClip]) -> Result<(), EngineError> {
        let mut first_error = None;
        let mut note = |result: Result<(), EngineError>| {
            if let Err(e) = result {
                first_error.get_or_insert(e);
            }
        };

        // From the end, so the indices before stay valid.
        let library = self.stinger_state().await?;
        let mut kept: Vec<&str> = Vec::new();
        let mut gone = Vec::new();
        for clip in &library.clips {
            let wanted = clips.iter().any(|c| c.file == clip.file);
            if wanted && !kept.contains(&clip.file.as_str()) {
                kept.push(&clip.file);
                continue;
            }
            gone.push(clip);
        }
        for clip in gone.iter().rev() {
            note(self.stinger_remove(clip).await);
        }

        // Added clips go to the end: the indices of the kept stay.
        let library = self.stinger_state().await?;
        for clip in clips {
            let settings = clip_settings(clip);
            match library.clips.iter().find(|c| c.file == clip.file) {
                None => note(self.stinger_add(&clip.file, settings).await.map(drop)),
                Some(have) if have.settings != settings => {
                    note(self.stinger_settings(have, settings).await.map(drop))
                }
                Some(_) => {}
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    async fn stinger_remove(&self, clip: &StingerClip) -> Result<(), EngineError> {
        let resp = self
            .http
            .delete(self.stinger_url(&format!("/clips/{}", clip.index)))
            .query(&[("file", &clip.file)])
            .send()
            .await
            .map_err(net)?;
        check(resp).await.map(|_| ())
    }

    async fn stinger_add(
        &self,
        file: &str,
        settings: StingerClipSettings,
    ) -> Result<StingerClip, EngineError> {
        let body = StingerAddClipRequest {
            file: file.into(),
            settings: Some(settings),
        };
        let resp = self
            .http
            .post(self.stinger_url("/clips"))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        check(resp).await?.json().await.map_err(net)
    }

    async fn stinger_settings(
        &self,
        clip: &StingerClip,
        settings: StingerClipSettings,
    ) -> Result<StingerClip, EngineError> {
        let resp = self
            .http
            .put(self.stinger_url(&format!("/clips/{}", clip.index)))
            .query(&[("file", &clip.file)])
            .json(&settings)
            .send()
            .await
            .map_err(net)?;
        check(resp).await?.json().await.map_err(net)
    }

    /// strom's page endpoint of the web or graphics page in `slot`.
    fn page_url(&self, slot: usize) -> String {
        self.url(&format!(
            "/flows/{}/blocks/{}/devtools/page",
            flows::slot_id(slot),
            flows::WEB
        ))
    }

    /// Send `command` to the page in `slot` (strom `devtools/page`).
    async fn page_command(
        &self,
        slot: usize,
        command: &HtmlPageCommand,
    ) -> Result<(), EngineError> {
        let resp = self
            .http
            .post(self.page_url(slot))
            .json(command)
            .send()
            .await
            .map_err(net)?;
        check(resp).await.map(|_| ())
    }

    /// Set a web page's CSS once its page answers: a page that has just
    /// been built is still starting, and its CSS lasts until it loads
    /// again, which only a new slot flow does.
    fn css_when_ready(&self, slot: usize, css: String) {
        let http = self.http.clone();
        let url = self.page_url(slot);
        tokio::spawn(async move {
            let command = HtmlPageCommand::SetCss { css };
            for _ in 0..PAGE_READY_TRIES {
                let sent = http.post(&url).json(&command).send().await;
                if sent.is_ok_and(|resp| resp.status().is_success()) {
                    return;
                }
                tokio::time::sleep(PAGE_READY_POLL).await;
            }
            tracing::warn!(slot, "web page did not take its CSS");
        });
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

    /// Move sends' taps (`tap_properties`). strom answers a tap it could
    /// not move with 200 and keeps the old one; that is a failure here, so
    /// the bus is not recorded with a tap it does not have.
    async fn patch_taps(&self, taps: Vec<(String, PropertyValue)>) -> Result<(), EngineError> {
        self.patch_block(flows::mixer_id(), AUDIO_MIXER, taps).await
    }

    /// Set live properties of `block` in `flow`; one strom rejects (it
    /// answers 200 and keeps the old value) is a failure.
    async fn patch_block(
        &self,
        flow: FlowId,
        block: &str,
        properties: Vec<(String, PropertyValue)>,
    ) -> Result<(), EngineError> {
        let body = UpdateBlockPropertiesRequest {
            properties: properties.into_iter().collect(),
            ramp_ms: Some(AUDIO_RAMP_MS),
            ramp_ms_overrides: None,
        };
        let path = format!("/flows/{flow}/blocks/{block}/properties");
        let resp = self
            .http
            .patch(self.url(&path))
            .json(&body)
            .send()
            .await
            .map_err(net)?;
        let answer: serde_json::Value = check(resp).await?.json().await.unwrap_or_default();
        match answer.get("rejected").and_then(|r| r.as_object()) {
            Some(rejected) if !rejected.is_empty() => Err(EngineError::Refused(format!(
                "strom kept the old values: {}",
                serde_json::Value::Object(rejected.clone())
            ))),
            _ => Ok(()),
        }
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
                self.stingers.clone(),
                self.buses.clone(),
            ));
        }

        let version = self.gstreamer_version().await.unwrap_or_else(|| {
            tracing::warn!(
                "strom did not tell its GStreamer version: MKV recordings use the older recorder"
            );
            String::new()
        });
        *self.live_mkv.lock().expect("live mkv lock") = flows::LiveMkv::for_gstreamer(&version);

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
            *self.buses.lock().expect("buses lock") = current.as_ref().and_then(flows::buses_of);
            // A Console of the lmix-core before this one may still read
            // the tiles: their live flow runs on, and it asks no more.
            // Unknown counts as read: lit costs load at most, dark the picture.
            let read = match self.get_flow(flows::live_flow_id(LivePicture::Tiles)).await {
                Ok(flow) => flow.is_some_and(|f| f.running),
                Err(e) => {
                    tracing::warn!("tiles flow at takeover: {e}; keeping the tiles lit");
                    true
                }
            };
            if read {
                self.tiles_watched.store(true, Ordering::Relaxed);
            }
            self.show_tiles().await;
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
                self.put_flow(&flows::slot_flow(*slot, source, &env))
                    .await?;
                if let SourceSpec::Web { css, .. } = source {
                    if !css.is_empty() {
                        self.css_when_ready(*slot, css.clone());
                    }
                }
                Ok(())
            }
            Command::SetSlot { slot, source: None } => self.drop_flow(flows::slot_id(*slot)).await,
            Command::CardOff { slot } => self.put_flow(&flows::card_off_flow(*slot)).await,
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
                bus,
            } => {
                let framerate = self.format.lock().expect("format lock").framerate;
                let live_mkv = *self.live_mkv.lock().expect("live mkv lock");
                let flow =
                    flows::output_flow(*id, spec, *feed, *bus, self.backend, framerate, live_mkv);
                self.put_flow(&flow).await
            }
            Command::StopOutput { id } => self.drop_flow(flows::output_flow_id(*id)).await,
            Command::SetOutputSound { id, feed, bus } => {
                // The switcher sends it only for an output with sound before
                // and after; one without would have no input to move.
                let channel = flows::feed_sound(*feed, *bus).ok_or_else(|| {
                    EngineError::Refused("the output has no sound to change".into())
                })?;
                self.patch_block(
                    flows::output_flow_id(*id),
                    flows::OUTPUT_SOUND,
                    vec![("channel".into(), PropertyValue::String(channel))],
                )
                .await
            }
            Command::Overlay {
                channel,
                slot,
                duration_ms,
            } => self.overlay(*channel, *slot, *duration_ms).await,
            Command::OverlayPlace { channel, rect } => self.overlay_place(*channel, *rect).await,
            Command::TBar { position, effect } => self.t_bar(*position, *effect).await,
            Command::SetStrip { slot, strip } => self.set_strip(*slot, strip).await,
            Command::Stinger { to, clip } => self.stinger(*to, clip).await,
            Command::StingerLibrary { clips } => self.stinger_library(clips).await,
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
            Command::PageScript { slot, script } => {
                let command = HtmlPageCommand::Evaluate {
                    expression: script.clone(),
                };
                self.page_command(*slot, &command).await
            }
            Command::PageRunning { slot, running } => {
                let command = match running {
                    PageRunning::Running => HtmlPageCommand::Resume,
                    PageRunning::Frozen => HtmlPageCommand::Freeze,
                };
                self.page_command(*slot, &command).await
            }
            // The service waits; nothing for the engine to do.
            Command::Wait { .. } => Ok(()),
            Command::SetPlayback { playback } => self.set_playback(playback).await,
            Command::SetPreload { preload } => self.set_preload(preload).await,
            Command::ShowPicture { id, picture } => self.show_picture(id, picture).await,
            Command::FadePicture {
                id,
                opacity,
                duration_ms,
                then,
            } => self.fade_picture(id, *opacity, *duration_ms, *then).await,
            Command::StopPicture { id, fade_ms } => {
                self.stop_picture(id.as_deref(), *fade_ms).await
            }
            Command::SetStageLevel {
                stage,
                brightness,
                duration_ms,
            } => self.stage_level(*stage, *brightness, *duration_ms).await,
            Command::PlaySound { id, sound } => self.play_sound(id, sound).await,
            Command::FadeSound {
                id,
                gain_db,
                duration_ms,
                then,
            } => self.fade_sound(id, *gain_db, *duration_ms, *then).await,
            Command::StopSound { id, fade_ms } => self.stop_sound(id.as_deref(), *fade_ms).await,
            Command::PauseSound { id } => self.hold("sounds", id, Hold::Pause).await,
            Command::ResumeSound { id } => self.hold("sounds", id, Hold::Resume).await,
            Command::PausePicture { id } => self.hold("pictures", id, Hold::Pause).await,
            Command::ResumePicture { id } => self.hold("pictures", id, Hold::Resume).await,
            Command::SetZoneLevel {
                zone,
                gain_db,
                mute,
                duration_ms,
            } => self.zone_level(*zone, *gain_db, *mute, *duration_ms).await,
            Command::SetAuxSend { slot, bus, gain } => {
                let channel = flows::audio_channel(*slot);
                let name = format!("ch{channel}_aux{}_level", self.aux(*bus)?);
                self.patch_audio_mixer(vec![(name, PropertyValue::Float(*gain))], AUDIO_RAMP_MS)
                    .await
            }
            Command::SetBusMix { sends, listen } => self.set_bus_mix(sends, listen).await,
            Command::SetAuxBus {
                bus,
                gain,
                mute,
                ramp_ms,
            } => {
                let aux = self.aux(*bus)?;
                let level = Level {
                    fader: format!("aux{aux}_fader"),
                    mute: format!("aux{aux}_mute"),
                    gain: *gain,
                    muted: *mute,
                };
                self.set_level(level, *ramp_ms).await
            }
            Command::SetFormat {
                format,
                multiview,
                sample_rate,
                buses,
            } => {
                self.set_format(*format, *multiview, *sample_rate, buses)
                    .await
            }
            Command::SetMonitor { gain, ramp_ms } => {
                let fader = ("monitor_fader".to_string(), PropertyValue::Float(*gain));
                self.patch_audio_mixer(vec![fader], *ramp_ms).await
            }
            Command::SetBusListen { bus, on } => {
                let afl = (
                    format!("aux{}_afl", self.aux(*bus)?),
                    PropertyValue::Bool(*on),
                );
                self.patch_audio_mixer(vec![afl], AUDIO_RAMP_MS).await
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
        if picture == LivePicture::Tiles {
            self.watch_tiles().await;
        }
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

    fn live_transport(&self, picture: LivePicture) -> LiveTransport {
        picture.transport(self.backend)
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

    fn playback_ended(&self) -> broadcast::Receiver<String> {
        self.playback.subscribe()
    }

    fn players(&self) -> broadcast::Receiver<crate::clips::PlayerProgress> {
        self.players.subscribe()
    }

    fn stingers(&self) -> broadcast::Receiver<meters::StingerEnd> {
        self.stingers.subscribe()
    }

    fn stinger_generation(&self) -> u64 {
        self.stingers.generation()
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

    /// Asked once at the start; no answer counts as no cards.
    async fn holds_cards(&self) -> bool {
        let Ok(resp) = self.http.get(self.url("/flows")).send().await else {
            return false;
        };
        let Ok(resp) = check(resp).await else {
            return false;
        };
        resp.json::<strom_types::api::FlowListResponse>()
            .await
            .is_ok_and(|list| list.flows.iter().any(flows::uses_card))
    }

    /// No for a flow strom has stopped or does not have; yes when strom
    /// does not answer within `FLOW_CHECK_TIMEOUT` (the failure watcher
    /// waits for it), as for an engine that cannot tell.
    async fn flow_running(&self, flow: FlowId) -> bool {
        match tokio::time::timeout(FLOW_CHECK_TIMEOUT, self.get_flow(flow)).await {
            // A flow that is still starting (READY, PAUSED) runs too.
            Ok(Ok(Some(flow))) => {
                flow.running
                    || matches!(
                        flow.gst_state,
                        Some(PipelineState::Ready | PipelineState::Paused | PipelineState::Playing)
                    )
            }
            Ok(Ok(None)) => false,
            Ok(Err(_)) | Err(_) => true,
        }
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
            devices.extend(found.into_iter().filter_map(|d| device_of(d, kind)));
        }
        sort_cards(&mut devices);
        Ok(devices)
    }
}

/// strom's api label of DeckLink cards.
const DECKLINK_API: &str = "DeckLink";
/// What strom appends to the name of a card's capture side.
const CAPTURE_SUFFIX: &str = " (Video Capture)";

/// lmix's view of a device strom found in `kind`'s category. A DeckLink
/// connector is a card by its device number (both ways, SRC.1/OUT.4);
/// `None` when strom gives no number for it.
fn device_of(found: DeviceResponse, kind: DeviceKind) -> Option<Device> {
    let api = found.api_label();
    if api != DECKLINK_API || kind != DeviceKind::Video {
        return Some(Device {
            api,
            id: found.id,
            name: found.name,
            kind,
        });
    }
    let number = found.properties.get("device-number")?;
    let name = found
        .name
        .strip_suffix(CAPTURE_SUFFIX)
        .unwrap_or(&found.name);
    Some(Device {
        api,
        id: number.clone(),
        name: name.to_string(),
        kind: DeviceKind::Card,
    })
}

/// Card connectors by number, in the places cards take among `devices`:
/// strom lists them in no order (`0, 7, 2, …`).
fn sort_cards(devices: &mut [Device]) {
    let places: Vec<usize> = (0..devices.len())
        .filter(|&i| devices[i].kind == DeviceKind::Card)
        .collect();
    let mut cards: Vec<Device> = places.iter().map(|&i| devices[i].clone()).collect();
    cards.sort_by_key(|c| c.id.parse::<u32>().unwrap_or(u32::MAX));
    for (place, card) in places.into_iter().zip(cards) {
        devices[place] = card;
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
        Effect::Stinger => "stinger",
    }
}

/// strom's settings of a library clip: the button's cut point, the rest
/// as strom detects it (layout from the clip, a cut beneath).
fn clip_settings(clip: &LibraryClip) -> StingerClipSettings {
    StingerClipSettings {
        layout: StingerLayout::Auto,
        cut_point_ms: Some(u64::from(clip.cut_ms)),
        beneath: StingerBeneath::Cut,
        ..Default::default()
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

    fn discovered(name: &str, provider: &str, number: Option<&str>) -> DeviceResponse {
        let properties: std::collections::HashMap<String, String> = number
            .map(|n| [("device-number".to_string(), n.to_string())].into())
            .unwrap_or_default();
        serde_json::from_value(serde_json::json!({
            "id": "dev-1", "name": name, "device_class": "Video/Source/Hardware",
            "category": "videosource", "provider": provider, "properties": properties,
            "first_seen_secs_ago": 0, "last_seen_secs_ago": 0
        }))
        .unwrap()
    }

    /// A DeckLink connector is a card device by its number; a camera
    /// stays a video device by its id (SRC.1).
    #[test]
    fn decklink_connectors_are_card_devices() {
        let card = device_of(
            discovered(
                "DeckLink Quad (3) (Video Capture)",
                "decklinkdeviceprovider",
                Some("2"),
            ),
            DeviceKind::Video,
        )
        .unwrap();
        assert_eq!(card.kind, DeviceKind::Card);
        assert_eq!(card.id, "2");
        assert_eq!(card.name, "DeckLink Quad (3)");
        assert_eq!(card.api, "DeckLink");

        let camera = device_of(
            discovered("Facecam", "v4l2deviceprovider", None),
            DeviceKind::Video,
        )
        .unwrap();
        assert_eq!(camera.kind, DeviceKind::Video);
        assert_eq!(camera.id, "dev-1");

        // A card without its number cannot be addressed.
        assert_eq!(
            device_of(
                discovered("DeckLink", "decklinkdeviceprovider", None),
                DeviceKind::Video
            ),
            None
        );
    }

    /// Cards come by number; a camera keeps its place among them.
    #[test]
    fn cards_come_by_number() {
        let device = |id: &str, kind| Device {
            api: String::new(),
            id: id.into(),
            name: id.into(),
            kind,
        };
        let mut devices = vec![
            device("7", DeviceKind::Card),
            device("cam", DeviceKind::Video),
            device("0", DeviceKind::Card),
            device("10", DeviceKind::Card),
            device("2", DeviceKind::Card),
        ];
        sort_cards(&mut devices);
        let ids: Vec<&str> = devices.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, ["0", "cam", "2", "7", "10"]);
    }

    /// Bodies of page commands the stand-in strom received.
    static PAGE_BODIES: std::sync::Mutex<Vec<serde_json::Value>> =
        std::sync::Mutex::new(Vec::new());

    /// A strom stand-in that answers every request with `{}` and records
    /// method, path and time.
    async fn recording_strom() -> (
        String,
        std::sync::Arc<std::sync::Mutex<Vec<(String, String, std::time::Instant)>>>,
    ) {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = calls.clone();
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let seen = seen.clone();
            async move {
                let path = request.uri().path().to_string();
                seen.lock().unwrap().push((
                    request.method().to_string(),
                    path.clone(),
                    std::time::Instant::now(),
                ));
                if path.ends_with("/devtools/page") {
                    let body = axum::body::to_bytes(request.into_body(), 1 << 20)
                        .await
                        .unwrap();
                    if let Ok(value) = serde_json::from_slice(&body) {
                        PAGE_BODIES.lock().unwrap().push(value);
                    }
                }
                axum::Json(serde_json::json!({}))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), calls)
    }

    /// Whether a flow runs: strom's answer, a missing flow is stopped, and
    /// no answer counts as running (the failure watcher keeps its window).
    #[tokio::test]
    async fn a_flow_runs_unless_strom_says_otherwise() {
        let (stopped, missing, broken) = (FlowId::new_v4(), FlowId::new_v4(), FlowId::new_v4());
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| async move {
            let path = request.uri().path().to_string();
            if path.ends_with(&missing.to_string()) {
                return (StatusCode::NOT_FOUND, axum::Json(serde_json::json!({})));
            }
            if path.ends_with(&broken.to_string()) {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    axum::Json(serde_json::json!({})),
                );
            }
            let mut flow = Flow::new("stopped");
            flow.id = stopped;
            flow.running = false;
            (
                StatusCode::OK,
                axum::Json(serde_json::json!({ "flow": flow })),
            )
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let engine = StromEngine::new(
            format!("http://{addr}"),
            MixerBackend::Cpu,
            MonitorOut::None,
        );

        assert!(!engine.flow_running(stopped).await);
        assert!(!engine.flow_running(missing).await);
        assert!(engine.flow_running(broken).await);
    }

    /// A new playback flow forgets the zones' last levels: they are by
    /// index, and after a zone is removed the next one would show the
    /// removed zone's level (review of #171).
    #[tokio::test]
    async fn a_new_playback_forgets_the_zone_levels() {
        let app = axum::Router::new()
            .fallback(|| async { (StatusCode::OK, axum::Json(serde_json::json!({}))) });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let engine = StromEngine::new(
            format!("http://{addr}"),
            MixerBackend::Cpu,
            MonitorOut::None,
        );
        engine
            .levels
            .send_modify(|l| _ = l.zones.insert(0, (-9.0, std::time::Instant::now())));

        let playback = crate::switcher::Playback {
            zones: Vec::new(),
            stages: Vec::new(),
            framerate: lmix_types::Framerate::Fps25,
        };
        engine.set_playback(&playback).await.unwrap();

        assert!(engine.levels.borrow().zones.is_empty());
    }

    /// A new format whose mixer strom does not build is tried again when
    /// it is asked for again; it is not taken as done.
    /// The tile picture costs the CPU mixer as much again as the rest
    /// (strom 480 % of a core against 290 % with 7 slots, 30.10.2026), so
    /// it stays dark until a Console asks for it: a new mixer has its tile
    /// pads at alpha 0, which the compositor skips without scaling.
    #[tokio::test]
    async fn the_tiles_stay_dark_until_watched() {
        let patches = std::sync::Arc::new(std::sync::Mutex::new(
            Vec::<(String, serde_json::Value)>::new(),
        ));
        let seen = patches.clone();
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let seen = seen.clone();
            async move {
                let (parts, body) = request.into_parts();
                let path = parts.uri.path().to_string();
                if parts.method == "PATCH" {
                    let body = axum::body::to_bytes(body, usize::MAX).await.unwrap();
                    let body = serde_json::from_slice(&body).unwrap_or_default();
                    seen.lock().unwrap().push((path, body));
                    return (StatusCode::OK, axum::Json(serde_json::json!({})));
                }
                if parts.method == "GET" && path == format!("/api/flows/{}", flows::mixer_id()) {
                    let mut flow = Flow::new("mixer");
                    flow.id = flows::mixer_id();
                    flow.running = true;
                    flow.gst_state = Some(PipelineState::Playing);
                    return (
                        StatusCode::OK,
                        axum::Json(serde_json::json!({ "flow": flow })),
                    );
                }
                (StatusCode::OK, axum::Json(serde_json::json!({})))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let engine = StromEngine::new(
            format!("http://{addr}"),
            MixerBackend::Cpu,
            MonitorOut::None,
        );
        engine.slots.store(2, Ordering::Relaxed);
        engine.black.store(flows::black_input(2), Ordering::Relaxed);
        let tiles = |alpha: f64| {
            let pads = flows::black_input(2) + 1;
            (0..pads)
                .map(|i| {
                    (
                        format!(
                            "/api/flows/{}/elements/{MIXER_BLOCK}:tiles_comp/pads/sink_{i}/properties",
                            flows::mixer_id()
                        ),
                        serde_json::json!({ "property_name": "alpha", "value": alpha }),
                    )
                })
                .collect::<Vec<_>>()
        };

        engine.build_mixer(&engine.mixer_flow()).await.unwrap();
        assert_eq!(*patches.lock().unwrap(), tiles(0.0), "a new mixer");

        patches.lock().unwrap().clear();
        engine.watch_tiles().await;
        assert_eq!(*patches.lock().unwrap(), tiles(1.0), "a Console asks");

        // Every request lights them again: a failed PATCH is not final.
        patches.lock().unwrap().clear();
        engine.watch_tiles().await;
        assert_eq!(*patches.lock().unwrap(), tiles(1.0), "a Console asks again");

        patches.lock().unwrap().clear();
        engine.build_mixer(&engine.mixer_flow()).await.unwrap();
        assert_eq!(
            *patches.lock().unwrap(),
            tiles(1.0),
            "rebuilt while watched"
        );
    }

    /// An lmix-core that takes over a running mixer whose tiles a Console
    /// still reads (their live flow runs) keeps them lit: that Console
    /// does not ask again (review of #183).
    #[tokio::test]
    async fn a_takeover_keeps_watched_tiles_lit() {
        let probe = StromEngine::new(String::new(), MixerBackend::Cpu, MonitorOut::None);
        probe.slots.store(2, Ordering::Relaxed);
        let mut mixer = probe.mixer_flow();
        mixer.running = true;
        let mut tiles = Flow::new("tiles");
        tiles.id = flows::live_flow_id(LivePicture::Tiles);
        tiles.running = true;
        let flows_by_id = std::sync::Arc::new(std::collections::HashMap::from([
            (format!("/api/flows/{}", mixer.id), mixer),
            (format!("/api/flows/{}", tiles.id), tiles),
        ]));
        let alphas = std::sync::Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
        let seen = alphas.clone();
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let (seen, flows_by_id) = (seen.clone(), flows_by_id.clone());
            async move {
                let (parts, body) = request.into_parts();
                let path = parts.uri.path().to_string();
                if parts.method == "PATCH" && path.contains(":tiles_comp/") {
                    let body: serde_json::Value = serde_json::from_slice(
                        &axum::body::to_bytes(body, usize::MAX).await.unwrap(),
                    )
                    .unwrap();
                    seen.lock().unwrap().push(body["value"].clone());
                } else if parts.method == "GET" {
                    if let Some(flow) = flows_by_id.get(&path) {
                        return (
                            StatusCode::OK,
                            axum::Json(serde_json::json!({ "flow": flow })),
                        );
                    }
                }
                (StatusCode::OK, axum::Json(serde_json::json!({})))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let engine = StromEngine::new(
            format!("http://{addr}"),
            MixerBackend::Cpu,
            MonitorOut::None,
        );

        engine.start(2).await.unwrap();

        let alphas = alphas.lock().unwrap().clone();
        assert!(!alphas.is_empty(), "the tile pads are set");
        assert!(alphas.iter().all(|a| a == 1.0), "{alphas:?}");
    }

    /// Whether a Console still reads the tiles is unknown when strom does
    /// not answer for their flow: lit costs load at most, dark the picture.
    #[tokio::test]
    async fn a_takeover_keeps_the_tiles_lit_when_strom_cannot_say() {
        let probe = StromEngine::new(String::new(), MixerBackend::Cpu, MonitorOut::None);
        probe.slots.store(2, Ordering::Relaxed);
        let mut mixer = probe.mixer_flow();
        mixer.running = true;
        let mut tiles = Flow::new("tiles");
        tiles.id = flows::live_flow_id(LivePicture::Tiles);
        tiles.running = true;
        let flows_by_id = std::sync::Arc::new(std::collections::HashMap::from([
            (format!("/api/flows/{}", mixer.id), mixer),
            (format!("/api/flows/{}", tiles.id), tiles),
        ]));
        let alphas = std::sync::Arc::new(std::sync::Mutex::new(Vec::<serde_json::Value>::new()));
        let seen = alphas.clone();
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let (seen, flows_by_id) = (seen.clone(), flows_by_id.clone());
            async move {
                let (parts, body) = request.into_parts();
                let path = parts.uri.path().to_string();
                if parts.method == "PATCH" && path.contains(":tiles_comp/") {
                    let body: serde_json::Value = serde_json::from_slice(
                        &axum::body::to_bytes(body, usize::MAX).await.unwrap(),
                    )
                    .unwrap();
                    seen.lock().unwrap().push(body["value"].clone());
                } else if parts.method == "GET" {
                    if path.ends_with(&flows::live_flow_id(LivePicture::Tiles).to_string()) {
                        return (
                            StatusCode::INTERNAL_SERVER_ERROR,
                            axum::Json(serde_json::json!({})),
                        );
                    }
                    if let Some(flow) = flows_by_id.get(&path) {
                        return (
                            StatusCode::OK,
                            axum::Json(serde_json::json!({ "flow": flow })),
                        );
                    }
                }
                (StatusCode::OK, axum::Json(serde_json::json!({})))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let engine = StromEngine::new(
            format!("http://{addr}"),
            MixerBackend::Cpu,
            MonitorOut::None,
        );

        engine.start(2).await.unwrap();

        let alphas = alphas.lock().unwrap().clone();
        assert!(!alphas.is_empty(), "the tile pads are set");
        assert!(alphas.iter().all(|a| a == 1.0), "{alphas:?}");
    }

    /// A new format whose mixer strom does not build is tried again when
    /// it is asked for again; it is not taken as done.
    #[tokio::test]
    async fn a_failed_new_mixer_is_built_again() {
        let posts = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = posts.clone();
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let seen = seen.clone();
            async move {
                if request.method() == "POST" && request.uri().path() == "/api/flows" {
                    seen.fetch_add(1, Ordering::SeqCst);
                    return (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        axum::Json(serde_json::json!({})),
                    );
                }
                (StatusCode::OK, axum::Json(serde_json::json!({})))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let engine = StromEngine::new(
            format!("http://{addr}"),
            MixerBackend::Cpu,
            MonitorOut::None,
        );
        let command = Command::SetFormat {
            format: *engine.format.lock().unwrap(),
            multiview: *engine.multiview.lock().unwrap(),
            sample_rate: *engine.sample_rate.lock().unwrap(),
            buses: MixerBuses::default(),
        };
        assert!(engine.run(&command).await.is_err());
        assert!(engine.run(&command).await.is_err());
        assert_eq!(posts.load(Ordering::SeqCst), 2);
    }

    /// A request the stinger stand-in strom received.
    #[derive(Debug)]
    struct Call {
        method: String,
        path: String,
        query: String,
        body: serde_json::Value,
    }

    /// The stinger stand-in's library and take state.
    #[derive(Default)]
    struct FakeStinger {
        /// Files with their cut points, in library order.
        clips: Vec<(String, u64)>,
        /// Files strom cannot add (not on disk).
        absent: Vec<String>,
        calls: Vec<Call>,
    }

    type Fake = std::sync::Arc<std::sync::Mutex<FakeStinger>>;

    fn fake_clip(index: usize, (file, cut_ms): &(String, u64)) -> serde_json::Value {
        serde_json::to_value(StingerClip {
            index,
            file: file.clone(),
            settings: StingerClipSettings {
                cut_point_ms: Some(*cut_ms),
                ..Default::default()
            },
            info: None,
            variant: None,
            downgraded_from: None,
            cut_point_ms: Some(*cut_ms),
            missing: false,
            analysis_error: None,
        })
        .unwrap()
    }

    /// One request to the stand-in: what strom's stinger API does with
    /// the library, as far as lmix sees it. PGM 0, PVW 1.
    fn fake_answer(
        fake: &mut FakeStinger,
        call: &Call,
    ) -> (axum::http::StatusCode, serde_json::Value) {
        use axum::http::StatusCode;
        let ok = |v| (StatusCode::OK, v);
        let file = call
            .query
            .strip_prefix("file=")
            .map(|f| f.replace("%2F", "/"));
        let index = call
            .path
            .rsplit('/')
            .next()
            .and_then(|i| i.parse::<usize>().ok());
        let at = |fake: &FakeStinger, i: usize| fake.clips.get(i).map(|c| c.0.clone());
        match (call.method.as_str(), call.path.rsplit('/').next()) {
            ("GET", Some("stinger")) => {
                let clips = fake
                    .clips
                    .iter()
                    .enumerate()
                    .map(|(i, c)| fake_clip(i, c))
                    .collect::<Vec<_>>();
                ok(serde_json::json!({
                    "source_block_id": flows::STINGER_PLAYER, "problem": null,
                    "matte_supported": false, "preroll_ms": 80, "clips": clips,
                    "cued_index": null, "ready": true, "last_cue_ms": null,
                    "running": false, "last_take": null
                }))
            }
            ("GET", Some("state")) => ok(serde_json::json!({
                "program_input": 0, "preview_input": 1, "program_pip": null,
                "preview_pip": null, "ftb_active": false, "dsk_enabled": [],
                "overlay_alpha": 1.0, "pips": []
            })),
            ("DELETE", _) => match index {
                Some(i) if at(fake, i) == file => {
                    fake.clips.remove(i);
                    ok(serde_json::json!({}))
                }
                _ => (StatusCode::CONFLICT, serde_json::json!({})),
            },
            ("PUT", _) => match index {
                Some(i) if at(fake, i) == file => {
                    fake.clips[i].1 = call.body["cut_point_ms"].as_u64().unwrap();
                    ok(fake_clip(i, &fake.clips[i]))
                }
                _ => (StatusCode::CONFLICT, serde_json::json!({})),
            },
            ("POST", Some("clips")) => {
                let file = call.body["file"].as_str().unwrap().to_string();
                if fake.absent.contains(&file) {
                    return (StatusCode::BAD_REQUEST, serde_json::json!({}));
                }
                let cut = call.body["settings"]["cut_point_ms"].as_u64().unwrap();
                fake.clips.push((file, cut));
                let i = fake.clips.len() - 1;
                ok(fake_clip(i, &fake.clips[i]))
            }
            ("POST", Some("take")) => {
                let i = call.body["index"].as_u64().unwrap() as usize;
                if at(fake, i).as_deref() != call.body["file"].as_str() {
                    return (StatusCode::CONFLICT, serde_json::json!({}));
                }
                ok(serde_json::json!({
                    "take_id": 1, "index": i, "file": fake.clips[i].0,
                    "variant": "classic", "downgraded_from": null,
                    "take_to_air_ms": 80.0, "duration_ms": 0
                }))
            }
            _ => ok(serde_json::json!({})),
        }
    }

    /// A strom stand-in whose mixer has `clips` (file, cut point) as its
    /// stinger library; it applies library edits and takes.
    async fn stinger_strom(clips: &[(&str, u64)]) -> (String, Fake) {
        let fake: Fake = Default::default();
        fake.lock().unwrap().clips = clips.iter().map(|(f, c)| ((*f).into(), *c)).collect();
        let shared = fake.clone();
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let shared = shared.clone();
            async move {
                let method = request.method().to_string();
                let path = request.uri().path().to_string();
                let query = request.uri().query().unwrap_or_default().to_string();
                let body = axum::body::to_bytes(request.into_body(), 1 << 20)
                    .await
                    .unwrap();
                let body = serde_json::from_slice(&body).unwrap_or_default();
                let call = Call {
                    method,
                    path,
                    query,
                    body,
                };
                let mut fake = shared.lock().unwrap();
                let (status, answer) = fake_answer(&mut fake, &call);
                fake.calls.push(call);
                (status, axum::Json(answer))
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), fake)
    }

    fn sting(file: &str, cut_ms: u32) -> Command {
        Command::Stinger {
            to: 1,
            clip: LibraryClip {
                file: file.into(),
                cut_ms,
            },
        }
    }

    fn takes(fake: &Fake) -> Vec<serde_json::Value> {
        let fake = fake.lock().unwrap();
        fake.calls
            .iter()
            .filter(|c| c.path.ends_with("/stinger/take"))
            .map(|c| c.body.clone())
            .collect()
    }

    /// strom plays the stinger and switches program at its cut point: the
    /// engine names the clip by its place in the library and its file.
    #[tokio::test]
    async fn a_stinger_takes_its_library_clip() {
        let (url, fake) = stinger_strom(&[("/a.webm", 300), ("/sting.webm", 400)]).await;
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);

        engine.run(&sting("/sting.webm", 400)).await.unwrap();

        let takes = takes(&fake);
        assert_eq!(takes.len(), 1);
        assert_eq!(takes[0]["index"], 1);
        assert_eq!(takes[0]["file"], "/sting.webm");
    }

    /// A library sync that failed (strom busy, the mixer restarting)
    /// heals at the take: the clip is added, or its cut point set.
    #[tokio::test]
    async fn a_stinger_puts_its_clip_in_the_library_first() {
        let (url, fake) = stinger_strom(&[("/a.webm", 300)]).await;
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);

        engine.run(&sting("/sting.webm", 400)).await.unwrap();
        engine.run(&sting("/a.webm", 500)).await.unwrap();

        assert_eq!(
            fake.lock().unwrap().clips,
            [("/a.webm".into(), 500), ("/sting.webm".into(), 400)]
        );
        let takes = takes(&fake);
        assert_eq!(
            (takes[0]["index"].clone(), takes[1]["index"].clone()),
            (1.into(), 0.into())
        );
    }

    /// After a stinger the engine does not wait for strom: the switcher
    /// holds takes until strom reports the end, so a take or fade goes
    /// out at once, without asking whether the stinger still runs.
    #[tokio::test]
    async fn after_a_stinger_takes_go_out_at_once() {
        let (url, fake) = stinger_strom(&[("/sting.webm", 400)]).await;
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);

        engine.run(&sting("/sting.webm", 400)).await.unwrap();
        let asked = fake.lock().unwrap().calls.len();
        engine
            .run(&Command::FadeToBlack { duration_ms: 0 })
            .await
            .unwrap();
        engine
            .run(&Command::Program {
                slot: Some(1),
                effect: Effect::Cut,
                duration_ms: 0,
            })
            .await
            .unwrap();
        engine.run(&sting("/sting.webm", 400)).await.unwrap();

        let fake = fake.lock().unwrap();
        let after: Vec<_> = fake.calls[asked..]
            .iter()
            .map(|c| (c.method.as_str(), c.path.rsplit('/').next().unwrap()))
            .collect();
        assert_eq!(after[0], ("POST", "ftb"), "{after:?}");
        let state_reads = after.iter().filter(|c| **c == ("GET", "stinger")).count();
        assert_eq!(
            state_reads, 1,
            "only the library lookup of the second stinger"
        );
    }

    /// A clip strom cannot have (its file is missing) costs the graphic:
    /// program still cuts, and the engine says why.
    #[tokio::test]
    async fn a_stinger_without_its_clip_cuts() {
        let (url, fake) = stinger_strom(&[]).await;
        fake.lock().unwrap().absent.push("/sting.webm".into());
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);

        let result = engine.run(&sting("/sting.webm", 400)).await;

        assert!(matches!(result, Err(EngineError::Refused(m)) if m.contains("cut instead")));
        let fake = fake.lock().unwrap();
        let cut = fake
            .calls
            .iter()
            .find(|c| c.path.ends_with("/transition"))
            .expect("a cut");
        assert_eq!(cut.body["transition_type"], "cut");
    }

    /// The library becomes what the switcher names: strangers and
    /// duplicates go (from the end, so each index still holds its file),
    /// cut points change, new clips come.
    #[tokio::test]
    async fn the_stinger_library_follows_the_buttons() {
        let (url, fake) = stinger_strom(&[
            ("/old.webm", 100),
            ("/sting.webm", 400),
            ("/sting.webm", 400),
            ("/older.webm", 100),
        ])
        .await;
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);
        let clip = |file: &str, cut_ms| LibraryClip {
            file: file.into(),
            cut_ms,
        };

        engine
            .run(&Command::StingerLibrary {
                clips: vec![clip("/sting.webm", 500), clip("/new.webm", 300)],
            })
            .await
            .unwrap();

        let fake = fake.lock().unwrap();
        assert_eq!(
            fake.clips,
            [("/sting.webm".into(), 500), ("/new.webm".into(), 300)]
        );
        let post = fake.calls.iter().find(|c| c.method == "POST").unwrap();
        assert_eq!(post.body["settings"]["layout"], "auto");
        assert_eq!(post.body["settings"]["beneath"], "cut");
    }

    /// A web page's CSS goes to its page once the slot runs, and its
    /// scripts and freezing as page commands (strom `devtools/page`).
    #[tokio::test]
    async fn page_commands_reach_the_page() {
        let (url, calls) = recording_strom().await;
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);
        let page = lmix_types::SourceSpec::Web {
            url: "https://example.com".into(),
            css: "body { background: transparent }".into(),
        };
        engine
            .run(&Command::SetSlot {
                slot: 2,
                source: Some(page),
            })
            .await
            .unwrap();
        engine
            .run(&Command::PageScript {
                slot: 2,
                script: "next()".into(),
            })
            .await
            .unwrap();
        engine
            .run(&Command::PageRunning {
                slot: 2,
                running: lmix_types::PageRunning::Frozen,
            })
            .await
            .unwrap();

        let page_path = format!("/flows/{}/blocks/web/devtools/page", flows::slot_id(2));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            let to_page = calls
                .lock()
                .unwrap()
                .iter()
                .any(|(_, path, _)| path.ends_with(&page_path));
            let bodies = PAGE_BODIES.lock().unwrap().clone();
            assert!(to_page || bodies.is_empty());
            let actions: Vec<String> = bodies
                .iter()
                .filter_map(|b| b["action"].as_str().map(str::to_string))
                .collect();
            if actions.contains(&"set_css".to_string()) {
                assert!(actions.contains(&"evaluate".to_string()), "{actions:?}");
                assert!(actions.contains(&"freeze".to_string()), "{actions:?}");
                let css = bodies.iter().find(|b| b["action"] == "set_css").unwrap();
                assert_eq!(css["css"], "body { background: transparent }");
                let script = bodies.iter().find(|b| b["action"] == "evaluate").unwrap();
                assert_eq!(script["expression"], "next()");
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "no set_css: {actions:?}"
            );
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    /// A strom stand-in for the audio mixer's buses: adds them under the
    /// lowest free number from 2, removes them, and records every request
    /// as method, path and body.
    /// How the bus stand-in answers the first add or remove.
    #[derive(Clone, Copy, PartialEq)]
    enum FirstChange {
        Done,
        /// Done, but answered with an error (the answer lost, or a failure
        /// after the change).
        DoneButFailed,
        /// Refused, nothing changed.
        Refused,
    }

    /// strom's bus API for a running mixer with buses `known` (aux,
    /// channel): lowest free number on add, 404 for a bus it does not have.
    async fn bus_strom(
        known: Vec<(usize, String)>,
        first: FirstChange,
    ) -> (
        String,
        std::sync::Arc<std::sync::Mutex<Vec<(String, String, serde_json::Value)>>>,
    ) {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let buses = std::sync::Arc::new(std::sync::Mutex::new(known));
        let first = std::sync::Arc::new(std::sync::Mutex::new(Some(first)));
        let seen = calls.clone();
        let app = axum::Router::new().fallback(move |request: axum::extract::Request| {
            let (seen, buses, first) = (seen.clone(), buses.clone(), first.clone());
            async move {
                let method = request.method().to_string();
                let path = request.uri().path().to_string();
                let bytes = axum::body::to_bytes(request.into_body(), 1 << 20)
                    .await
                    .unwrap();
                let body: serde_json::Value =
                    serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
                seen.lock()
                    .unwrap()
                    .push((method.clone(), path.clone(), body.clone()));
                let mut list = buses.lock().unwrap();
                let change = (path.ends_with("/aux_buses") && method == "POST")
                    || path.contains("/aux_buses/");
                let how = if change {
                    first.lock().unwrap().take().unwrap_or(FirstChange::Done)
                } else {
                    FirstChange::Done
                };
                let error = |status: StatusCode| (status, axum::Json(serde_json::json!({})));
                if how == FirstChange::Refused {
                    return error(StatusCode::INTERNAL_SERVER_ERROR);
                }
                if path.ends_with("/aux_buses") && method == "POST" {
                    let aux = (2..).find(|n| !list.iter().any(|(a, _)| a == n)).unwrap();
                    list.push((aux, body["channel"].as_str().unwrap().to_string()));
                } else if let Some((_, aux)) = path.rsplit_once("/aux_buses/") {
                    let aux: usize = aux.parse().unwrap();
                    if !list.iter().any(|(a, _)| *a == aux) {
                        return error(StatusCode::NOT_FOUND);
                    }
                    list.retain(|(a, _)| *a != aux);
                }
                if how == FirstChange::DoneButFailed {
                    return error(StatusCode::INTERNAL_SERVER_ERROR);
                }
                let dynamic: Vec<_> = list
                    .iter()
                    .map(|(aux, channel)| serde_json::json!({ "aux": aux, "channel": channel }))
                    .collect();
                (
                    StatusCode::OK,
                    axum::Json(serde_json::json!({
                        "capacity": 17, "num_static": 1, "dynamic": dynamic
                    })),
                )
            }
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (format!("http://{addr}"), calls)
    }

    /// A running output moves its sound input to the bus's channel, or
    /// back to program's; nothing is built.
    #[tokio::test]
    async fn an_output_takes_another_sound_on_air() {
        let (url, calls) = bus_strom(Vec::new(), FirstChange::Done).await;
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);
        let (id, bus) = (OutputId::new(), BusId::new());
        for (bus, channel) in [
            (Some(bus), flows::bus_channel(bus)),
            (
                None,
                flows::feed_sound(crate::switcher::Feed::Program, None).unwrap(),
            ),
        ] {
            engine
                .run(&Command::SetOutputSound {
                    id,
                    feed: crate::switcher::Feed::Program,
                    bus,
                })
                .await
                .unwrap();
            let (method, path, body) = calls.lock().unwrap().last().unwrap().clone();
            assert_eq!(method, "PATCH");
            assert_eq!(
                path,
                format!(
                    "/api/flows/{}/blocks/{}/properties",
                    flows::output_flow_id(id),
                    flows::OUTPUT_SOUND
                )
            );
            assert_eq!(
                body["properties"],
                serde_json::json!({ "channel": channel })
            );
        }
        assert_eq!(calls.lock().unwrap().len(), 2, "no flow built or stopped");
        // An ISO of a silent input has no sound input to move.
        assert!(engine
            .run(&Command::SetOutputSound {
                id,
                feed: crate::switcher::Feed::Slot {
                    slot: 0,
                    audio: false
                },
                bus: None,
            })
            .await
            .is_err());
    }

    /// Buses that come, go and change their tap change the running mixer:
    /// no new flow, each bus keeps its number, and sends find them by it.
    #[tokio::test]
    async fn buses_change_the_running_mixer() {
        let (a, b, c) = (BusId::new(), BusId::new(), BusId::new());
        // Strom's numbers as the mixer was built: A aux 2, B aux 3.
        let known = vec![(2, flows::bus_channel(a)), (3, flows::bus_channel(b))];
        let (url, calls) = bus_strom(known, FirstChange::Done).await;
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);
        engine.slots.store(1, Ordering::Relaxed);
        let before = MixerBuses {
            phones_pre_fader: false,
            buses: vec![(a, true), (b, false)],
        };
        *engine.buses.lock().unwrap() = Some(flows::BuiltBuses::numbered(&before));
        // A goes before anything is added.
        let format = |buses: MixerBuses| Command::SetFormat {
            format: *engine.format.lock().unwrap(),
            multiview: *engine.multiview.lock().unwrap(),
            sample_rate: *engine.sample_rate.lock().unwrap(),
            buses,
        };
        let after = MixerBuses {
            phones_pre_fader: true,
            buses: vec![(b, true), (c, false)],
        };
        engine.run(&format(after.clone())).await.unwrap();

        let calls_now = calls.lock().unwrap().clone();
        let mixer = format!("/api/flows/{}/blocks/{AUDIO_MIXER}", flows::mixer_id());
        let methods: Vec<(&str, String)> = calls_now
            .iter()
            .map(|(m, p, _)| (m.as_str(), p.trim_start_matches(&mixer).to_string()))
            .collect();
        assert_eq!(
            methods,
            [
                ("DELETE", "/aux_buses/2".to_string()),
                ("PATCH", "/properties".to_string()),
                ("PATCH", "/properties".to_string()),
                ("POST", "/aux_buses".to_string()),
            ],
            "no new mixer flow"
        );
        // The headphones and B before the faders, for the slot and the
        // stinger alike.
        assert_eq!(
            calls_now[1].2["properties"],
            serde_json::json!({ "ch1_aux1_pre": true, "ch2_aux1_pre": true })
        );
        assert_eq!(
            calls_now[2].2["properties"],
            serde_json::json!({ "ch1_aux3_pre": true, "ch2_aux3_pre": true })
        );
        assert_eq!(calls_now[3].2["channel"], flows::bus_channel(c));
        assert_eq!(calls_now[3].2["pre_fader"], false);
        let built = engine.buses.lock().unwrap().clone().unwrap();
        assert!(built.are(&after));
        // C took the free aux 2; as engine bus 2 its send goes there.
        assert_eq!(built.aux(2), Some(2));
        engine
            .run(&Command::SetAuxSend {
                slot: 0,
                bus: 2,
                gain: 1.0,
            })
            .await
            .unwrap();
        let last = calls.lock().unwrap().last().unwrap().2.clone();
        assert_eq!(
            last["properties"],
            serde_json::json!({ "ch1_aux2_level": 1.0 })
        );
        // Every send and listening in one write; a bus the mixer lacks is
        // named, the rest is set.
        let count = calls.lock().unwrap().len();
        let mix = engine
            .run(&Command::SetBusMix {
                sends: vec![(0, 1, 0.5), (0, 2, 0.25), (0, 5, 1.0)],
                listen: vec![(2, true)],
            })
            .await;
        assert!(matches!(mix, Err(EngineError::Refused(ref m)) if m.ends_with("bus 5")));
        let calls_now = calls.lock().unwrap().clone();
        assert_eq!(calls_now.len(), count + 1);
        assert_eq!(
            calls_now[count].2["properties"],
            serde_json::json!({
                "ch1_aux3_level": 0.5, "ch1_aux2_level": 0.25, "aux2_afl": true
            })
        );
        // The same buses again change nothing.
        let count = calls.lock().unwrap().len();
        engine.run(&format(after)).await.unwrap();
        assert_eq!(calls.lock().unwrap().len(), count);
    }

    /// A bus change strom carried out but answered with an error is taken
    /// as done: the patch reaches the show's buses, it does not stick on
    /// the refusal a second try would get.
    #[tokio::test]
    async fn a_bus_change_done_but_failed_counts_as_done() {
        let (a, b, c) = (BusId::new(), BusId::new(), BusId::new());
        let known = vec![(2, flows::bus_channel(a)), (3, flows::bus_channel(b))];
        for first in [FirstChange::DoneButFailed] {
            let (url, _) = bus_strom(known.clone(), first).await;
            let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);
            engine.slots.store(1, Ordering::Relaxed);
            *engine.buses.lock().unwrap() = Some(flows::BuiltBuses::numbered(&MixerBuses {
                phones_pre_fader: false,
                buses: vec![(a, false), (b, false)],
            }));
            // Removing A answers with an error, though strom removed it.
            let only_b = MixerBuses {
                phones_pre_fader: false,
                buses: vec![(b, false)],
            };
            engine.patch_buses(&only_b).await.expect("remove A");
            assert!(engine.buses.lock().unwrap().clone().unwrap().are(&only_b));
            // So does adding C, though strom added it.
            let (url, _) = bus_strom(vec![(3, flows::bus_channel(b))], first).await;
            let engine2 = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);
            engine2.slots.store(1, Ordering::Relaxed);
            *engine2.buses.lock().unwrap() = engine.buses.lock().unwrap().clone();
            let with_c = MixerBuses {
                phones_pre_fader: false,
                buses: vec![(b, false), (c, false)],
            };
            engine2.patch_buses(&with_c).await.expect("add C");
            let built = engine2.buses.lock().unwrap().clone().unwrap();
            assert!(built.are(&with_c));
            assert_eq!(built.aux(2), Some(2), "C on the number strom gave it");
        }
    }

    /// When strom refuses to remove a bus, the show's buses keep their
    /// commands: engine bus 1 is still the show's first bus, not the one
    /// left over.
    #[tokio::test]
    async fn a_bus_strom_kept_takes_no_command_of_another() {
        let (a, b) = (BusId::new(), BusId::new());
        let known = vec![(2, flows::bus_channel(a)), (3, flows::bus_channel(b))];
        let (url, calls) = bus_strom(known, FirstChange::Refused).await;
        let engine = StromEngine::new(url, MixerBackend::Cpu, MonitorOut::None);
        engine.slots.store(1, Ordering::Relaxed);
        *engine.buses.lock().unwrap() = Some(flows::BuiltBuses::numbered(&MixerBuses {
            phones_pre_fader: false,
            buses: vec![(a, false), (b, false)],
        }));
        let only_b = MixerBuses {
            phones_pre_fader: false,
            buses: vec![(b, false)],
        };
        assert!(engine.patch_buses(&only_b).await.is_err());
        engine
            .run(&Command::SetAuxSend {
                slot: 0,
                bus: 1,
                gain: 1.0,
            })
            .await
            .unwrap();
        let last = calls.lock().unwrap().last().unwrap().2.clone();
        assert_eq!(
            last["properties"],
            serde_json::json!({ "ch1_aux3_level": 1.0 }),
            "the send went to A's aux"
        );
        // The second try removes A.
        engine.patch_buses(&only_b).await.expect("remove A again");
        assert!(engine.buses.lock().unwrap().clone().unwrap().are(&only_b));
    }

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
