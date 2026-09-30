//! lmix Console - vMix-style switcher UI, driven entirely over strom's
//! REST/WS API. Runs as its own OS process, separate from the strom server
//! (see claude.md, "Feststehende Entscheidung 4").

mod api;
mod gpu_profile;
mod perf;
mod selftest;
mod source_ui;
mod sources;
mod state;
mod ws;

use api::ApiClient;
use perf::{Metric, Perf};
use source_ui::SourceBuilder;
use clap::Parser;
use state::{
    find_switchable_flows, find_thumbnail_block, meter_channel, AppMessage, AudioMixerState, AutoTransition,
    EngineLoad, MixerState, Selection, SwitchableFlow,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use gpu_profile::{BackendChoice, MixerBackend};
use strom_types::{Flow, StromEvent};

/// lmix Console
#[derive(Parser, Debug)]
struct Args {
    /// Base URL of the strom backend to connect to.
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,

    /// Show the performance overlay from the start (toggle with F3).
    #[arg(long)]
    debug_perf: bool,

    /// Write every latency/jitter sample to this CSV file.
    #[arg(long)]
    perf_log: Option<std::path::PathBuf>,

    /// Run the headless multiview/switching self-test instead of the GUI.
    #[arg(long)]
    selftest: bool,

    /// Self-test: number of generated test-pattern inputs.
    #[arg(long, default_value_t = 8)]
    selftest_patterns: usize,

    /// Self-test: duration of the load phase in seconds.
    #[arg(long, default_value_t = 20)]
    selftest_secs: u64,

    /// Self-test: add a file input (repeatable).
    #[arg(long)]
    selftest_file: Vec<String>,

    /// Self-test: add a URL input (repeatable).
    #[arg(long)]
    selftest_url: Vec<String>,

    /// Self-test: add an SRT (MPEG-TS) input by URI (repeatable).
    #[arg(long)]
    selftest_srt: Vec<String>,

    /// Self-test: add every discovered video capture device.
    #[arg(long)]
    selftest_devices: bool,

    /// Self-test: encode PGM and multiview on hardware and check fps/latency.
    #[arg(long)]
    selftest_encode: bool,

    /// Self-test: stop a file input's player in the middle of a fade (needs
    /// a --selftest-file input at index >= 1).
    #[arg(long)]
    selftest_dropout: bool,

    /// Create and open the 8-pattern preset flow right away (for testing).
    #[arg(long)]
    auto_open: bool,

    /// Thumbnail poll interval per input, in milliseconds.
    #[arg(long, default_value_t = DEFAULT_THUMB_MS)]
    thumb_ms: u64,

    /// Vision mixer backend for new flows (auto = fastest for the engine's GPU).
    #[arg(long, value_enum, default_value_t = BackendChoice::Auto)]
    mixer_backend: BackendChoice,
}

/// A background task tags every message it sends with the session it was
/// started for; the UI drops messages from a session it has since moved
/// away from (e.g. the user picked a different flow while a poll was in
/// flight) instead of applying stale data.
type Session = u64;

const METER_FLOOR_DB: f64 = -60.0;
const METER_HOT_DB: f64 = -6.0;
const METER_WIDTH: f32 = 40.0;
const IDLE_REPAINT_MS: u64 = 250;
const PERF_REPAINT_MS: u64 = 16;
const DEFAULT_THUMB_MS: u64 = 250;
const LATENCY_POLL_MS: u64 = 2000;
const GPU_INFO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const DEFAULT_AUTO_MS: u64 = 300;
const FTB_ACTIVE_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 40, 40);
const AUTO_MS_RANGE: std::ops::RangeInclusive<u64> = 100..=5000;

struct ConsoleApp {
    api: ApiClient,
    tx: mpsc::Sender<(Session, AppMessage)>,
    rx: mpsc::Receiver<(Session, AppMessage)>,
    session: Arc<AtomicU64>,

    flows: Vec<Flow>,
    loading: bool,
    error: Option<String>,

    selection: Option<Selection>,
    mixer: MixerState,
    audio: Option<AudioMixerState>,
    textures: HashMap<usize, egui::TextureHandle>,
    auto_transition: AutoTransition,
    auto_duration_ms: u64,

    perf: Arc<Perf>,
    show_perf: bool,
    thumb_ms: u64,
    last_frame: Option<std::time::Instant>,
    builder: SourceBuilder,
    devices: Vec<strom_types::discovery::DeviceResponse>,
    backend_choice: BackendChoice,
    /// Engine CPU/GPU load, from `SystemStats` WS events.
    engine: Option<EngineLoad>,
}

impl ConsoleApp {
    fn new(args: Args) -> Self {
        let (tx, rx) = mpsc::channel();
        let auto_open = args.auto_open;
        let mut app = Self {
            api: ApiClient::new(args.server),
            tx,
            rx,
            session: Arc::new(AtomicU64::new(0)),
            flows: Vec::new(),
            loading: false,
            error: None,
            selection: None,
            mixer: MixerState::default(),
            audio: None,
            textures: HashMap::new(),
            auto_transition: AutoTransition::Fade,
            auto_duration_ms: DEFAULT_AUTO_MS,
            perf: Arc::new(Perf::new(args.perf_log.as_deref())),
            show_perf: args.debug_perf,
            thumb_ms: args.thumb_ms,
            last_frame: None,
            builder: SourceBuilder::new(),
            devices: Vec::new(),
            backend_choice: args.mixer_backend,
            engine: None,
        };
        app.refresh_flows();
        app.refresh_devices();
        if auto_open {
            let request = app.builder.preset_request();
            app.create_flow(request);
        }
        app
    }

    fn refresh_flows(&self) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        let session = self.session.load(Ordering::SeqCst);
        tokio::spawn(async move {
            let msg = match api.list_flows().await {
                Ok(flows) => AppMessage::FlowsLoaded(flows),
                Err(e) => AppMessage::Error(format!("Failed to load flows: {e}")),
            };
            let _ = tx.send((session, msg));
        });
    }

    fn refresh_devices(&self) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        let session = self.session.load(Ordering::SeqCst);
        tokio::spawn(async move {
            match api.list_video_devices().await {
                Ok(devices) => {
                    let _ = tx.send((session, AppMessage::DevicesLoaded(devices)));
                }
                Err(e) => tracing::warn!("device discovery failed: {e}"),
            }
        });
    }

    /// Build a flow from the user's source list, create it on the backend
    /// and report it back so the Console can open it.
    fn create_flow(&self, request: source_ui::CreateRequest) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        let session = self.session.load(Ordering::SeqCst);
        let choice = self.backend_choice;
        tokio::spawn(async move {
            let backend = match resolve_backend(&api, choice).await {
                Ok(backend) => backend,
                Err(e) => {
                    let _ = tx.send((session, AppMessage::Error(e)));
                    return;
                }
            };
            let flow = match sources::build_flow(&request.name, &request.sources, backend) {
                Ok(flow) => flow,
                Err(e) => {
                    let _ = tx.send((session, AppMessage::Error(e.to_string())));
                    return;
                }
            };
            let msg = match api.create_flow(&flow).await {
                Ok(created) => AppMessage::FlowCreated(created),
                Err(e) => AppMessage::Error(format!("Failed to create flow: {e}")),
            };
            let _ = tx.send((session, msg));
        });
    }

    /// Enter the Console for one flow: bump the session (invalidating any
    /// in-flight work from a previous selection), fetch the initial Vision
    /// Mixer state, connect the WS event stream, and start polling
    /// thumbnails for every input that has one.
    fn select_flow(&mut self, flow: &SwitchableFlow, ctx: &egui::Context) {
        let session = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        self.mixer = MixerState::default();
        self.audio = None;
        self.textures.clear();
        self.error = None;

        self.selection = Some(Selection {
            flow_id: flow.flow_id,
            flow_name: flow.name.clone(),
            vision_mixer_block_id: flow.vision_mixer_block_id.clone(),
            audio_mixer_block_id: flow.audio_mixer_block_id.clone(),
            mixer_backend: flow.mixer_backend.clone(),
        });

        let vm_block = flow.vision_mixer_block_id.clone();
        let audio_block = flow.audio_mixer_block_id.clone();
        let flow_id = flow.flow_id;
        let perf = self.perf.clone();
        let thumb_ms = self.thumb_ms;

        // Start the flow, then fetch its initial Vision Mixer state (WS only
        // carries *changes* from here on). `start_flow` is idempotent on the
        // backend (already-running is not an error), so the Console can be
        // pointed at a stopped flow and just start it, instead of leaving
        // the operator stuck on "Flow is not running - start it to switch".
        // Sequenced (not two independent spawns): the state fetch must not
        // race a start still in flight, or it observes the pre-start 404.
        {
            let api = self.api.clone();
            let tx = self.tx.clone();
            let vm_block = vm_block.clone();
            tokio::spawn(async move {
                if let Err(e) = api.start_flow(flow_id).await {
                    let _ = tx.send((session, AppMessage::Error(format!("Failed to start flow: {e}"))));
                    return;
                }
                match api.get_vision_mixer_state(flow_id, &vm_block).await {
                    Ok(Some(state)) => {
                        let _ = tx.send((session, AppMessage::VisionMixerStateLoaded(state)));
                    }
                    Ok(None) => {
                        let _ = tx.send((
                            session,
                            AppMessage::Error(
                                "Flow started but has no Vision Mixer state yet".to_string(),
                            ),
                        ));
                    }
                    Err(e) => {
                        let _ = tx.send((session, AppMessage::Error(format!("{e}"))));
                    }
                }
            });
        }

        // Fetch the full flow (fresh, not the cached list snapshot) to walk
        // its link graph for thumbnail block assignment, then start pollers.
        {
            let api = self.api.clone();
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            let session_counter = self.session.clone();
            tokio::spawn(async move {
                let full_flow = match api.get_flow(flow_id).await {
                    Ok(f) => f,
                    Err(e) => {
                        let _ = tx.send((session, AppMessage::Error(format!("{e}"))));
                        return;
                    }
                };
                if !full_flow.blocks.iter().any(|b| b.id == vm_block) {
                    return;
                }
                // Try a generous upper bound on input count; unused indices
                // simply find no thumbnail block and are skipped.
                for idx in 0..16 {
                    if let Some(thumb_block) =
                        find_thumbnail_block(&full_flow, &vm_block, idx)
                    {
                        spawn_thumbnail_poller(
                            perf.clone(),
                            thumb_ms,
                            api.clone(),
                            tx.clone(),
                            ctx.clone(),
                            session_counter.clone(),
                            session,
                            flow_id,
                            idx,
                            thumb_block,
                        );
                    }
                }

                if let Some(audio_block) = audio_block {
                    match api.get_block_properties(flow_id, &audio_block).await {
                        Ok(properties) => {
                            // num_channels itself is construction-time (not
                            // "live"), so GET .../properties omits it - derive
                            // the channel count from how many ch{N}_fader
                            // keys are actually present instead.
                            let num_channels = properties
                                .keys()
                                .filter_map(|k| {
                                    k.strip_prefix("ch")?.strip_suffix("_fader")?.parse::<usize>().ok()
                                })
                                .max()
                                .unwrap_or(0);
                            let _ = tx.send((
                                session,
                                AppMessage::AudioMixerLoaded {
                                    num_channels,
                                    properties,
                                },
                            ));
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load audio mixer properties: {e}");
                        }
                    }
                }
            });
        }

        spawn_latency_poller(self.perf.clone(), self.api.clone(), self.session.clone(), session, flow_id);

        // WS event stream for live PVW/PGM state.
        ws::connect(self.api.ws_url(), self.tx.clone(), ctx.clone(), session, self.perf.clone());
    }

    /// Put an input on PVW (click a tile, or a number hotkey).
    fn select_preview(&self, input: usize) {
        let Some(selection) = self.selection.clone() else {
            return;
        };
        let session = self.session.load(Ordering::SeqCst);
        let api = self.api.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            match api
                .select_preview(
                    selection.flow_id,
                    &selection.vision_mixer_block_id,
                    strom_types::vision_mixer::Source::Input(input),
                )
                .await
            {
                Ok(resp) => {
                    let _ = tx.send((
                        session,
                        AppMessage::MixerUpdate {
                            preview_input: resp.preview_input,
                            program_input: resp.program_input,
                        },
                    ));
                }
                Err(e) => {
                    let _ = tx.send((session, AppMessage::Error(format!("Preview select failed: {e}"))));
                }
            }
        });
    }

    fn is_selected_mixer(&self, block_id: &str) -> bool {
        self.selection
            .as_ref()
            .is_some_and(|s| s.vision_mixer_block_id == block_id)
    }

    /// FTB (lit while black) and one toggle per DSK layer, like vMix's
    /// row under the transition buttons.
    fn draw_ftb_dsk(&mut self, ui: &mut egui::Ui) {
        let ftb = egui::Button::new(egui::RichText::new("FTB").strong())
            .fill(if self.mixer.ftb_active { FTB_ACTIVE_COLOR } else { ui.visuals().widgets.inactive.bg_fill });
        if ui.add_sized([90.0, 28.0], ftb).clicked() {
            self.toggle_ftb();
        }

        let layers = self.mixer.dsk_enabled.clone();
        for (i, enabled) in layers.into_iter().enumerate() {
            let dsk = i + 1;
            if ui.selectable_label(enabled, format!("DSK {dsk}")).clicked() {
                self.set_dsk(dsk, !enabled);
            }
        }
    }

    /// Toggle Fade to Black with the AUTO duration.
    fn toggle_ftb(&self) {
        let Some(selection) = self.selection.clone() else {
            return;
        };
        let (api, tx) = (self.api.clone(), self.tx.clone());
        let session = self.session.load(Ordering::SeqCst);
        let duration_ms = self.auto_duration_ms;
        tokio::spawn(async move {
            let msg = match api
                .toggle_ftb(selection.flow_id, &selection.vision_mixer_block_id, duration_ms)
                .await
            {
                Ok(resp) => AppMessage::FtbUpdate(resp.active),
                Err(e) => AppMessage::Error(format!("FTB failed: {e}")),
            };
            let _ = tx.send((session, msg));
        });
    }

    /// Switch DSK layer `dsk` (1-based) to `enabled`.
    fn set_dsk(&self, dsk: usize, enabled: bool) {
        let Some(selection) = self.selection.clone() else {
            return;
        };
        let (api, tx) = (self.api.clone(), self.tx.clone());
        let session = self.session.load(Ordering::SeqCst);
        tokio::spawn(async move {
            let msg = match api
                .set_dsk(selection.flow_id, &selection.vision_mixer_block_id, dsk, enabled)
                .await
            {
                Ok(resp) => AppMessage::DskUpdate { dsk: resp.dsk, enabled: resp.enabled },
                Err(e) => AppMessage::Error(format!("DSK {dsk} failed: {e}")),
            };
            let _ = tx.send((session, msg));
        });
    }

    /// Take the current PVW to PGM (CUT, or a timed transition).
    fn take(&self, transition_type: &'static str, duration_ms: u64) {
        let Some(selection) = self.selection.clone() else {
            return;
        };
        let Some(from_input) = self.mixer.program_input else {
            return;
        };
        let Some(to_input) = self.mixer.preview_input else {
            return;
        };
        let session = self.session.load(Ordering::SeqCst);
        let api = self.api.clone();
        let tx = self.tx.clone();
        let perf = self.perf.clone();
        perf.take_started();
        tokio::spawn(async move {
            let started = std::time::Instant::now();
            let result = api
                .trigger_transition(
                    selection.flow_id,
                    &selection.vision_mixer_block_id,
                    from_input,
                    to_input,
                    transition_type,
                    duration_ms,
                )
                .await;
            perf.record(Metric::TakeRtt, started.elapsed().as_secs_f64() * 1000.0);
            match result {
                // After a take, PGM/PVW swap (old PGM becomes the new PVW) -
                // mirrors backend/src/state.rs's own post-take bookkeeping.
                Ok(_resp) => {
                    let _ = tx.send((
                        session,
                        AppMessage::MixerUpdate {
                            preview_input: Some(from_input),
                            program_input: Some(to_input),
                        },
                    ));
                }
                Err(e) => {
                    let _ = tx.send((session, AppMessage::Error(format!("Transition failed: {e}"))));
                }
            }
        });
    }

    /// Patch one audio mixer channel property. Applied to local state
    /// immediately (audio has no WS event stream in our scope, unlike the
    /// Vision Mixer) - the PATCH response isn't awaited for UI feedback,
    /// just logged if it's rejected.
    fn patch_audio_property(&mut self, name: String, value: strom_types::PropertyValue) {
        let Some(selection) = self.selection.clone() else {
            return;
        };
        let Some(audio_block) = selection.audio_mixer_block_id.clone() else {
            return;
        };
        let api = self.api.clone();
        let tx = self.tx.clone();
        let session = self.session.load(Ordering::SeqCst);
        let name_for_task = name.clone();
        tokio::spawn(async move {
            let mut properties = HashMap::new();
            properties.insert(name_for_task.clone(), value);
            if let Err(e) = api
                .patch_block_properties(selection.flow_id, &audio_block, properties)
                .await
            {
                let _ = tx.send((
                    session,
                    AppMessage::Error(format!("Failed to set {name_for_task}: {e}")),
                ));
            }
        });
    }

    fn drain_messages(&mut self, ctx: &egui::Context) {
        let current_session = self.session.load(Ordering::SeqCst);
        while let Ok((session, msg)) = self.rx.try_recv() {
            match msg {
                AppMessage::FlowsLoaded(flows) => {
                    let switchable = find_switchable_flows(&flows).len();
                    tracing::info!(
                        "Loaded {} flows ({} with a Vision Mixer block)",
                        flows.len(),
                        switchable
                    );
                    self.flows = flows;
                    self.loading = false;
                    self.error = None;
                }
                AppMessage::DevicesLoaded(devices) => {
                    self.devices = devices;
                }
                AppMessage::FlowCreated(flow) => {
                    self.refresh_flows();
                    if let Some(switchable) = find_switchable_flows(&[flow]).pop() {
                        self.select_flow(&switchable, ctx);
                    }
                }
                AppMessage::Error(e) => {
                    if session == current_session {
                        self.error = Some(e);
                    }
                    self.loading = false;
                }
                AppMessage::VisionMixerStateLoaded(state) if session == current_session => {
                    self.mixer = state.into();
                }
                AppMessage::Event(StromEvent::VisionMixerStateChanged {
                    block_id,
                    preview_input,
                    program_input,
                    ..
                }) if session == current_session => {
                    self.perf.take_confirmed();
                    if self
                        .selection
                        .as_ref()
                        .is_some_and(|s| s.vision_mixer_block_id == block_id)
                    {
                        self.mixer.preview_input = preview_input;
                        self.mixer.program_input = program_input;
                    }
                }
                AppMessage::Event(StromEvent::MeterData {
                    flow_id,
                    element_id,
                    peak,
                    ..
                }) if session == current_session => {
                    let Some(selection) = self.selection.as_ref() else {
                        continue;
                    };
                    let Some(block_id) = selection.audio_mixer_block_id.as_deref() else {
                        continue;
                    };
                    let Some(audio) = self.audio.as_mut() else {
                        continue;
                    };
                    if flow_id != selection.flow_id {
                        continue;
                    }
                    let Some(ch) = meter_channel(&element_id, block_id) else {
                        continue;
                    };
                    let loudest = peak.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                    audio.peaks_db.insert(ch, loudest);
                }
                AppMessage::Event(StromEvent::SystemStats(stats)) => {
                    self.engine = EngineLoad::from_stats(&stats);
                }
                AppMessage::MixerUpdate {
                    preview_input,
                    program_input,
                } if session == current_session => {
                    self.mixer.preview_input = preview_input;
                    self.mixer.program_input = program_input;
                }
                AppMessage::FtbUpdate(active) if session == current_session => {
                    self.mixer.ftb_active = active;
                }
                AppMessage::DskUpdate { dsk, enabled } if session == current_session => {
                    self.mixer.set_dsk(dsk, enabled);
                }
                AppMessage::Event(StromEvent::VisionMixerFtbChanged { block_id, active, .. })
                    if session == current_session && self.is_selected_mixer(&block_id) =>
                {
                    self.mixer.ftb_active = active;
                }
                AppMessage::Event(StromEvent::VisionMixerDskChanged { block_id, dsk, enabled, .. })
                    if session == current_session && self.is_selected_mixer(&block_id) =>
                {
                    self.mixer.set_dsk(dsk, enabled);
                }
                AppMessage::AudioMixerLoaded {
                    num_channels,
                    properties,
                } if session == current_session => {
                    self.audio = Some(AudioMixerState::from_properties(num_channels, &properties));
                }
                AppMessage::ThumbnailDecoded(idx, image) if session == current_session => {
                    let handle = ctx.load_texture(
                        format!("thumb-{idx}"),
                        image,
                        egui::TextureOptions::LINEAR,
                    );
                    self.textures.insert(idx, handle);
                }
                // Anything else (stale VisionMixerStateLoaded/Event/ThumbnailDecoded
                // from a session we've since left, or an unhandled event kind) is
                // dropped rather than applied.
                _ => {}
            }
        }
    }

    /// vMix-style Console: big PREVIEW/PROGRAM monitors on top with the
    /// transition bar between them, the input list (all sources, click to
    /// preview) below, and the audio mixer strip at the bottom.
    fn render_console(&mut self, ui: &mut egui::Ui, selection: &Selection) {
        ui.horizontal(|ui| {
            ui.heading(&selection.flow_name);
            if ui.button("<- Back to flow list").clicked() {
                self.selection = None;
            }
        });
        ui.horizontal(|ui| {
            ui.label(format!("Vision Mixer block: {}", selection.vision_mixer_block_id));
            let backend = selection.mixer_backend.as_deref().unwrap_or("auto");
            ui.label(format!("· backend: {backend}"));
            if let Some(load) = &self.engine {
                ui.label(format!("· {}", load.summary()));
            }
        });

        if self.mixer.num_inputs == 0 {
            ui.weak("Waiting for Vision Mixer state...");
            return;
        }

        // Hotkeys 1-8 select PVW, matching backend/static/vision-mixer.html's
        // own scheme. Guarded so typing into a text field never triggers a
        // switch.
        if !ui.ctx().egui_wants_keyboard_input() {
            const HOTKEYS: [egui::Key; 8] = [
                egui::Key::Num1,
                egui::Key::Num2,
                egui::Key::Num3,
                egui::Key::Num4,
                egui::Key::Num5,
                egui::Key::Num6,
                egui::Key::Num7,
                egui::Key::Num8,
            ];
            for (idx, key) in HOTKEYS.iter().enumerate() {
                if idx < self.mixer.num_inputs && ui.input(|i| i.key_pressed(*key)) {
                    self.select_preview(idx);
                }
            }
        }

        ui.separator();

        // Two big monitors, PREVIEW (green) left / PROGRAM (red) right -
        // vMix's own left-to-right convention. Each shows the same texture
        // as its input's tile below, just enlarged.
        const PREVIEW_COLOR: egui::Color32 = egui::Color32::from_rgb(40, 200, 80);
        const PROGRAM_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 40, 40);
        let preview_tex = self.mixer.preview_input.and_then(|idx| self.textures.get(&idx));
        let program_tex = self.mixer.program_input.and_then(|idx| self.textures.get(&idx));
        ui.columns(2, |columns| {
            render_monitor(&mut columns[0], "PREVIEW", PREVIEW_COLOR, preview_tex);
            render_monitor(&mut columns[1], "PROGRAM", PROGRAM_COLOR, program_tex);
        });

        // Transition bar, centered between the monitors above and the input
        // list below - vMix's Cut/Auto row.
        let can_take = self.mixer.program_input.is_some() && self.mixer.preview_input.is_some();
        ui.vertical_centered(|ui| {
            ui.add_space(6.0);
            ui.add_enabled_ui(can_take, |ui| {
                ui.horizontal(|ui| {
                    if ui
                        .add_sized([90.0, 36.0], egui::Button::new(
                            egui::RichText::new("CUT").strong(),
                        ))
                        .clicked()
                    {
                        self.take("cut", 0);
                    }
                    if ui
                        .add_sized([120.0, 36.0], egui::Button::new("AUTO"))
                        .clicked()
                    {
                        self.take(self.auto_transition.wire_name(), self.auto_duration_ms);
                    }
                    egui::ComboBox::from_id_salt("auto_transition")
                        .selected_text(self.auto_transition.label())
                        .show_ui(ui, |ui| {
                            for t in AutoTransition::ALL {
                                ui.selectable_value(&mut self.auto_transition, t, t.label());
                            }
                        });
                    ui.add(
                        egui::DragValue::new(&mut self.auto_duration_ms)
                            .range(AUTO_MS_RANGE)
                            .suffix(" ms"),
                    );
                });
            });
            ui.horizontal(|ui| {
                self.draw_ftb_dsk(ui);
            });
            ui.add_space(6.0);
        });

        ui.separator();

        // Input list: every source, click (or hotkey) to preview. Tally
        // border mirrors the monitor above it, just smaller.
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for idx in 0..self.mixer.num_inputs {
                    let is_pgm = self.mixer.program_input == Some(idx);
                    let is_pvw = self.mixer.preview_input == Some(idx);

                    let border_color = if is_pgm {
                        PROGRAM_COLOR
                    } else if is_pvw {
                        PREVIEW_COLOR
                    } else {
                        ui.visuals().widgets.inactive.bg_fill
                    };

                    let tile = egui::Frame::new()
                        .stroke(egui::Stroke::new(3.0_f32, border_color))
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.set_width(150.0);
                            ui.vertical(|ui| {
                                if let Some(tex) = self.textures.get(&idx) {
                                    let size = egui::vec2(142.0, 80.0);
                                    ui.image((tex.id(), size));
                                } else {
                                    let (_, rect) = ui.allocate_space(egui::vec2(142.0, 80.0));
                                    ui.painter().rect_filled(
                                        rect,
                                        0.0,
                                        egui::Color32::from_gray(30),
                                    );
                                }
                                let label = match (is_pgm, is_pvw) {
                                    (true, _) => format!("{} - PGM", idx + 1),
                                    (_, true) => format!("{} - PVW", idx + 1),
                                    _ => format!("{}", idx + 1),
                                };
                                ui.label(label);
                            });
                        });

                    // Click a tile (unless it's already PGM - nothing to
                    // preview there) to put it on PVW.
                    if !is_pgm
                        && tile
                            .response
                            .interact(egui::Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                    {
                        self.select_preview(idx);
                    }
                }
            });
        });

        if self.audio.is_some() {
            ui.separator();
            self.render_audio_strip(ui);
        }
    }

    /// Simplified audio strip: one fader + mute per channel on the
    /// `builtin.mixer` block. Not the full X32-style panel
    /// `frontend/src/mixer/` has - just what M1 asks for.
    fn render_audio_strip(&mut self, ui: &mut egui::Ui) {
        let audio = self.audio.as_mut().expect("checked by caller");
        let mut changes: Vec<(String, strom_types::PropertyValue)> = Vec::new();

        egui::Frame::new()
            .fill(egui::Color32::from_gray(18))
            .inner_margin(8.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("AUDIO MIXER").strong());
                ui.horizontal(|ui| {
                    for ch in 1..=audio.num_channels {
                        ui.vertical(|ui| {
                            ui.label(format!("Ch {ch}"));

                            let fader = audio.faders.entry(ch).or_insert(1.0);
                            if ui
                                .add(egui::Slider::new(fader, 0.0..=2.0).vertical())
                                .changed()
                            {
                                changes.push((
                                    format!("ch{ch}_fader"),
                                    strom_types::PropertyValue::Float(*fader),
                                ));
                            }

                            let peak_db = audio.peaks_db.get(&ch).copied().unwrap_or(METER_FLOOR_DB);
                            render_meter(ui, peak_db);

                            let muted = audio.mutes.entry(ch).or_insert(false);
                            if ui.selectable_label(*muted, "MUTE").clicked() {
                                *muted = !*muted;
                                changes.push((
                                    format!("ch{ch}_mute"),
                                    strom_types::PropertyValue::Bool(*muted),
                                ));
                            }
                        });
                        ui.add_space(10.0);
                    }
                });
            });

        for (name, value) in changes {
            self.patch_audio_property(name, value);
        }
    }
}

/// Horizontal peak bar, `METER_FLOOR_DB`..0 dB; green, red above
/// `METER_HOT_DB`.
fn render_meter(ui: &mut egui::Ui, peak_db: f64) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(METER_WIDTH, 8.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(30));

    let level = ((peak_db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0) as f32;
    let color = if peak_db > METER_HOT_DB {
        egui::Color32::RED
    } else {
        egui::Color32::GREEN
    };
    let fill = egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * level, rect.height()));
    ui.painter().rect_filled(fill, 0.0, color);
}

/// One big vMix-style monitor: a colored label bar (tally color) over a
/// 16:9 video area, filling the column's available width. `texture` is
/// `None` when nothing is on that bus yet (fresh flow, or PGM/PVW unset).
fn render_monitor(
    ui: &mut egui::Ui,
    label: &str,
    tally_color: egui::Color32,
    texture: Option<&egui::TextureHandle>,
) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(3.0_f32, tally_color))
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(label).strong().color(tally_color));
                });
                let width = ui.available_width();
                let size = egui::vec2(width, width * 9.0 / 16.0);
                if let Some(tex) = texture {
                    ui.image((tex.id(), size));
                } else {
                    let (_, rect) = ui.allocate_space(size);
                    ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(20));
                }
            });
        });
}

/// Pipeline latency as reported by the backend, sampled every couple of
/// seconds while the flow is open.
fn spawn_latency_poller(
    perf: Arc<Perf>,
    api: ApiClient,
    session_counter: Arc<AtomicU64>,
    session: Session,
    flow_id: strom_types::FlowId,
) {
    tokio::spawn(async move {
        while session_counter.load(Ordering::SeqCst) == session {
            if let Ok(latency) = api.get_latency(flow_id).await {
                perf.record(Metric::PipelineLatency, latency.min_latency_ns as f64 / 1e6);
            }
            tokio::time::sleep(std::time::Duration::from_millis(LATENCY_POLL_MS)).await;
        }
    });
}

#[allow(clippy::too_many_arguments)]
fn spawn_thumbnail_poller(
    perf: Arc<Perf>,
    thumb_ms: u64,
    api: ApiClient,
    tx: mpsc::Sender<(Session, AppMessage)>,
    ctx: egui::Context,
    session_counter: Arc<AtomicU64>,
    session: Session,
    flow_id: strom_types::FlowId,
    input_index: usize,
    thumbnail_block_id: String,
) {
    tokio::spawn(async move {
        loop {
            if session_counter.load(Ordering::SeqCst) != session {
                return;
            }
            let started = std::time::Instant::now();
            let result = api.get_thumbnail(flow_id, &thumbnail_block_id).await;
            if result.is_ok() {
                perf.record(Metric::ThumbRtt, started.elapsed().as_secs_f64() * 1000.0);
                perf.thumb_arrived(input_index);
            }
            match result {
                Ok(bytes) => {
                    if let Ok(img) = image::load_from_memory(&bytes) {
                        let rgba = img.to_rgba8();
                        let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                        let color_image =
                            egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba);
                        let _ = tx.send((
                            session,
                            AppMessage::ThumbnailDecoded(input_index, color_image),
                        ));
                        ctx.request_repaint();
                    }
                }
                Err(api::ApiError::Server { status: 504, .. }) => {
                    // Expected: first request just activated the capture
                    // branch, no frame yet - retry on the next tick.
                }
                Err(e) => {
                    tracing::trace!("thumbnail poll for input {input_index} failed: {e}");
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(thumb_ms)).await;
        }
    });
}

impl eframe::App for ConsoleApp {
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.perf.flush();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let frame_start = std::time::Instant::now();
        if let Some(prev) = self.last_frame.replace(frame_start) {
            self.perf
                .record(Metric::UiFrame, frame_start.duration_since(prev).as_secs_f64() * 1000.0);
        }
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.key_pressed(egui::Key::F3)) {
            self.show_perf = !self.show_perf;
        }
        self.drain_messages(&ctx);
        // With the overlay on, repaint at ~60 Hz so ui_frame shows real frame
        // pacing instead of the idle repaint interval.
        let repaint_ms = if self.show_perf { PERF_REPAINT_MS } else { IDLE_REPAINT_MS };
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(repaint_ms));

        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(format!("Server: {}", self.api.base_url()));
                if ui.button("Refresh").clicked() {
                    self.loading = true;
                    self.refresh_flows();
                }
                if self.loading {
                    ui.spinner();
                }
            });
        });

        if let Some(err) = &self.error {
            egui::Panel::top("error").show_inside(ui, |ui| {
                ui.colored_label(egui::Color32::RED, err);
            });
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if let Some(selection) = self.selection.clone() {
                self.render_console(ui, &selection);
                return;
            }

            if let Some(request) = self.builder.show(ui, &self.devices) {
                self.create_flow(request);
            }
            ui.separator();

            ui.heading("Select a flow to switch");
            let switchable: Vec<SwitchableFlow> = find_switchable_flows(&self.flows);
            if switchable.is_empty() {
                ui.weak("No flow with a Vision Mixer block yet - create one above.");
            }
            let ctx = ui.ctx().clone();
            for flow in switchable {
                ui.horizontal(|ui| {
                    let running = if flow.running { "running" } else { "stopped" };
                    if ui.button(format!("{} ({running})", flow.name)).clicked() {
                        self.select_flow(&flow, &ctx);
                    }
                });
            }
        });

        if self.show_perf {
            self.perf.overlay(&ctx);
        }
        self.perf.record(
            Metric::UiCost,
            frame_start.elapsed().as_secs_f64() * 1000.0,
        );
    }
}

/// Mixer backend for a new flow, from the engine's GPU (see `gpu_profile`).
async fn resolve_backend(api: &ApiClient, choice: BackendChoice) -> Result<MixerBackend, String> {
    if choice != BackendChoice::Auto {
        return MixerBackend::resolve(choice, None);
    }
    let stats = ws::first_system_stats(&api.ws_url(), GPU_INFO_TIMEOUT).await;
    let gl = stats.and_then(|s| s.gl_renderer);
    let backend = MixerBackend::resolve(choice, gl.as_ref())?;
    tracing::info!("mixer backend {:?} for GPU {:?}", backend, gl.map(|g| g.renderer));
    Ok(backend)
}

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let args = Args::parse();

    // Background tasks (flow polling, WS, thumbnails) need an active
    // reactor - see the identical fix in
    // workspace/strom/frontend/src/main.rs (M0).
    let runtime = tokio::runtime::Runtime::new().expect("failed to create Tokio runtime");
    let _runtime_guard = runtime.enter();

    if args.selftest {
        let opts = selftest::Options {
            server: args.server.clone(),
            patterns: args.selftest_patterns,
            files: args.selftest_file.clone(),
            urls: args.selftest_url.clone(),
            srts: args.selftest_srt.clone(),
            devices: args.selftest_devices,
            seconds: args.selftest_secs,
            thumb_ms: args.thumb_ms,
            perf_log: args.perf_log.clone(),
            backend: args.mixer_backend,
            dropout: if args.selftest_dropout {
                selftest::Dropout::DuringFade
            } else {
                selftest::Dropout::None
            },
            outputs: if args.selftest_encode {
                selftest::OutputLoad::Encode
            } else {
                selftest::OutputLoad::None
            },
        };
        let passed = runtime.block_on(selftest::run(opts));
        std::process::exit(if passed { 0 } else { 1 });
    }

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 860.0])
            .with_title("lmix Console"),
        ..Default::default()
    };

    eframe::run_native(
        "lmix Console",
        native_options,
        Box::new(move |_cc| Ok(Box::new(ConsoleApp::new(args)))),
    )
}
