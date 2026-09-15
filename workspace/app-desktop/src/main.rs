//! lmix Console - vMix-style switcher UI, driven entirely over strom's
//! REST/WS API. Runs as its own OS process, separate from the strom server
//! (see claude.md, "Feststehende Entscheidung 4").

mod api;
mod state;

use api::ApiClient;
use clap::Parser;
use state::{find_switchable_flows, AppMessage, Selection, SwitchableFlow};
use std::sync::mpsc;
use strom_types::Flow;

/// lmix Console
#[derive(Parser, Debug)]
struct Args {
    /// Base URL of the strom backend to connect to.
    #[arg(long, default_value = "http://127.0.0.1:8080")]
    server: String,
}

struct ConsoleApp {
    api: ApiClient,
    tx: mpsc::Sender<AppMessage>,
    rx: mpsc::Receiver<AppMessage>,

    flows: Vec<Flow>,
    loading: bool,
    error: Option<String>,
    selection: Option<Selection>,
}

impl ConsoleApp {
    fn new(server: String) -> Self {
        let (tx, rx) = mpsc::channel();
        let app = Self {
            api: ApiClient::new(server),
            tx,
            rx,
            flows: Vec::new(),
            loading: false,
            error: None,
            selection: None,
        };
        app.refresh_flows();
        app
    }

    fn refresh_flows(&self) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let msg = match api.list_flows().await {
                Ok(flows) => AppMessage::FlowsLoaded(flows),
                Err(e) => AppMessage::Error(format!("Failed to load flows: {e}")),
            };
            let _ = tx.send(msg);
        });
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
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
                    self.error = Some(e);
                    self.loading = false;
                }
            }
        }
    }
}

impl eframe::App for ConsoleApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.drain_messages();
        // Background tasks deliver results asynchronously; keep repainting
        // so the UI picks them up promptly instead of only on user input.
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
            if let Some(selection) = &self.selection.clone() {
                ui.heading(format!("Console: {}", selection.flow_name));
                ui.label(format!("Vision Mixer block: {}", selection.vision_mixer_block_id));
                if let Some(audio) = &selection.audio_mixer_block_id {
                    ui.label(format!("Audio Mixer block: {audio}"));
                }
                if ui.button("<- Back to flow list").clicked() {
                    self.selection = None;
                }
                ui.separator();
                ui.weak("PGM/PVW grid, take/cut and hotkeys land in the next step.");
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
            for flow in switchable {
                ui.horizontal(|ui| {
                    let running = if flow.running { "running" } else { "stopped" };
                    if ui
                        .button(format!("{} ({running})", flow.name))
                        .clicked()
                    {
                        self.selection = Some(Selection {
                            flow_id: flow.flow_id,
                            flow_name: flow.name.clone(),
                            vision_mixer_block_id: flow.vision_mixer_block_id.clone(),
                            audio_mixer_block_id: flow.audio_mixer_block_id.clone(),
                        });
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

    // Background tasks (flow polling, later WS) need an active reactor - see
    // the identical fix in workspace/strom/frontend/src/main.rs (M0).
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
