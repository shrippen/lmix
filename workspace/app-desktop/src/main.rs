//! lmix Console - vMix-style switcher UI, driven entirely over strom's
//! REST/WS API. Runs as its own OS process, separate from the strom server
//! (see claude.md, "Feststehende Entscheidung 4").

mod api;
mod state;
mod ws;

use api::ApiClient;
use clap::Parser;
use state::{
    find_switchable_flows, find_thumbnail_block, AppMessage, MixerState, Selection,
    SwitchableFlow,
};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use strom_types::{Flow, StromEvent};

/// lmix Console
#[derive(Parser, Debug)]
struct Args {
    /// Base URL of the strom backend to connect to.
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
}

/// A background task tags every message it sends with the session it was
/// started for; the UI drops messages from a session it has since moved
/// away from (e.g. the user picked a different flow while a poll was in
/// flight) instead of applying stale data.
type Session = u64;

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
    textures: HashMap<usize, egui::TextureHandle>,
}

impl ConsoleApp {
    fn new(server: String) -> Self {
        let (tx, rx) = mpsc::channel();
        let app = Self {
            api: ApiClient::new(server),
            tx,
            rx,
            session: Arc::new(AtomicU64::new(0)),
            flows: Vec::new(),
            loading: false,
            error: None,
            selection: None,
            mixer: MixerState::default(),
            textures: HashMap::new(),
        };
        app.refresh_flows();
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

    /// Enter the Console for one flow: bump the session (invalidating any
    /// in-flight work from a previous selection), fetch the initial Vision
    /// Mixer state, connect the WS event stream, and start polling
    /// thumbnails for every input that has one.
    fn select_flow(&mut self, flow: &SwitchableFlow, ctx: &egui::Context) {
        let session = self.session.fetch_add(1, Ordering::SeqCst) + 1;
        self.mixer = MixerState::default();
        self.textures.clear();
        self.error = None;

        self.selection = Some(Selection {
            flow_id: flow.flow_id,
            flow_name: flow.name.clone(),
            vision_mixer_block_id: flow.vision_mixer_block_id.clone(),
            audio_mixer_block_id: flow.audio_mixer_block_id.clone(),
        });

        let vm_block = flow.vision_mixer_block_id.clone();
        let flow_id = flow.flow_id;

        // Initial state fetch (WS only carries *changes* from here on).
        {
            let api = self.api.clone();
            let tx = self.tx.clone();
            let vm_block = vm_block.clone();
            tokio::spawn(async move {
                match api.get_vision_mixer_state(flow_id, &vm_block).await {
                    Ok(Some(state)) => {
                        let _ = tx.send((session, AppMessage::VisionMixerStateLoaded(state)));
                    }
                    Ok(None) => {
                        let _ = tx.send((
                            session,
                            AppMessage::Error(
                                "Flow is not running - start it to switch".to_string(),
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
            });
        }

        // WS event stream for live PVW/PGM state.
        let ws_url = self
            .api
            .base_url()
            .replacen("http://", "ws://", 1)
            .replacen("https://", "wss://", 1)
            + "/api/ws";
        ws::connect(ws_url, self.tx.clone(), ctx.clone(), session);
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
                    if self
                        .selection
                        .as_ref()
                        .is_some_and(|s| s.vision_mixer_block_id == block_id)
                    {
                        self.mixer.preview_input = preview_input;
                        self.mixer.program_input = program_input;
                    }
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

    /// PGM/PVW grid: one tile per Vision Mixer input, red border on the
    /// input currently on PGM, green on PVW. Interaction (click-to-preview,
    /// take/cut, hotkeys) lands in the next step - this renders live state
    /// only, always from `self.mixer`, never from a locally-assumed value.
    fn render_console(&mut self, ui: &mut egui::Ui, selection: &Selection) {
        ui.horizontal(|ui| {
            ui.heading(&selection.flow_name);
            if ui.button("<- Back to flow list").clicked() {
                self.selection = None;
            }
        });
        ui.label(format!("Vision Mixer block: {}", selection.vision_mixer_block_id));

        if self.mixer.num_inputs == 0 {
            ui.weak("Waiting for Vision Mixer state...");
            return;
        }

        ui.separator();
        egui::Grid::new("pgm_pvw_grid")
            .num_columns(4)
            .spacing([12.0, 12.0])
            .show(ui, |ui| {
                for idx in 0..self.mixer.num_inputs {
                    let is_pgm = self.mixer.program_input == Some(idx);
                    let is_pvw = self.mixer.preview_input == Some(idx);

                    let border_color = if is_pgm {
                        egui::Color32::from_rgb(220, 40, 40)
                    } else if is_pvw {
                        egui::Color32::from_rgb(40, 200, 80)
                    } else {
                        ui.visuals().widgets.inactive.bg_fill
                    };

                    egui::Frame::new()
                        .stroke(egui::Stroke::new(3.0_f32, border_color))
                        .inner_margin(4.0)
                        .show(ui, |ui| {
                            ui.set_width(200.0);
                            ui.vertical(|ui| {
                                if let Some(tex) = self.textures.get(&idx) {
                                    let size = egui::vec2(192.0, 108.0);
                                    ui.image((tex.id(), size));
                                } else {
                                    let (_, rect) = ui.allocate_space(egui::vec2(192.0, 108.0));
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

                    if (idx + 1) % 4 == 0 {
                        ui.end_row();
                    }
                }
            });
    }
}

#[allow(clippy::too_many_arguments)]
fn spawn_thumbnail_poller(
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
            match api.get_thumbnail(flow_id, &thumbnail_block_id).await {
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
            tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
        }
    });
}

impl eframe::App for ConsoleApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_messages(&ctx);
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(250));

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

            ui.heading("Select a flow to switch");
            let switchable: Vec<SwitchableFlow> = find_switchable_flows(&self.flows);
            if switchable.is_empty() {
                ui.weak(
                    "No flow with a Vision Mixer block found. Build one first \
                     (existing node editor, or docs/BUILD.md's curl example).",
                );
                return;
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
    }
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

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 700.0])
            .with_title("lmix Console"),
        ..Default::default()
    };

    eframe::run_native(
        "lmix Console",
        native_options,
        Box::new(move |_cc| Ok(Box::new(ConsoleApp::new(args.server)))),
    )
}
