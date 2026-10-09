//! Console as a client of lmix-core (`--core`, ROADMAP 2b.2, persona
//! finding B22): input bar, PREVIEW/PROGRAM monitors, transition buttons.
//!
//! ```text
//!   ┌ toolbar: lmix-core ● · show file · New / Open / Save / Recent ┐
//!   │  PREVIEW monitor   │ CUT 1 2 3 4 FTB │ PROGRAM monitor │
//!   │  input bar: tiles (click = preview, double click = take, menu) │
//!   └ add input: kind · source · name · Add ┘
//! ```
//!
//! Every click becomes an [`Action`]; what is shown comes only from the
//! state lmix-core broadcasts, so every client sees the same thing.

use std::collections::HashMap;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use lmix_types::{
    Action, Container, Device, DeviceKind, DisplayMode, Effect, InputColor, InputId, InputRef,
    InputStatus, Latency, Levels, Listen, OutputId, OutputRef, OutputSource, OutputSpec,
    OutputStats, OutputStatus, ShowState, SignalLoss, SourceSpec, TestPattern, TransitionSlot,
    Transport,
};

use crate::core_client::{CoreClient, CoreMessage, FilePurpose, PictureTarget};

const PREVIEW_COLOR: egui::Color32 = egui::Color32::from_rgb(40, 200, 80);
const PROGRAM_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 40, 40);
const IDLE_COLOR: egui::Color32 = egui::Color32::from_gray(70);
const EMPTY_PICTURE: egui::Color32 = egui::Color32::from_gray(20);
const TILE_WIDTH: f32 = 160.0;
const TILE_STROKE: f32 = 3.0;
const STATUS_DOT: f32 = 10.0;
const COLOR_BAND: f32 = 4.0;
const VIDEO_KBPS_RANGE: std::ops::RangeInclusive<u32> = 1000..=50000;
const TRANSPORT_BUTTONS: [(Transport, &str, &str); 5] = [
    (Transport::Restart, "|◀", "Restart"),
    (Transport::Previous, "◀◀", "Previous item"),
    (Transport::Play, "▶", "Play"),
    (Transport::Pause, "||", "Pause"),
    (Transport::Next, "▶▶", "Next item"),
];
/// Color picker start: a calm blue.
const DEFAULT_COLOR: [u8; 3] = [0x20, 0x60, 0xc0];
const METER_FLOOR_DB: f32 = -60.0;
const METER_HOT_DB: f32 = -6.0;
const METER_HEIGHT: f32 = 6.0;
const METER_COLOR: egui::Color32 = egui::Color32::from_rgb(60, 190, 90);
const METER_HOT_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 60, 50);
const AUDIO_OFF_COLOR: egui::Color32 = egui::Color32::from_rgb(200, 50, 50);
const AFV_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 160, 40);
/// Solo buttons, like the yellow PFL/AFL keys on a desk.
const SOLO_COLOR: egui::Color32 = egui::Color32::from_rgb(230, 200, 40);
const SHOW_FILTER: (&str, &[&str]) = ("lmix show", &["lmix"]);
const REMOTE_HINT: &str =
    "Paths are opened by lmix-core: on another machine, they must exist there.";
const TRANSITION_HINT: &str = "Click: take preview. Right click: effect and length.";
const BUTTON_SIZE: [f32; 2] = [80.0, 36.0];
const DURATION_PRESETS_MS: [u32; 4] = [250, 500, 1000, 2000];
const PATTERNS: [TestPattern; 5] = [
    TestPattern::Bars,
    TestPattern::Ball,
    TestPattern::Checkers,
    TestPattern::Snow,
    TestPattern::Black,
];
/// Transition button a double click takes with.
const TAKE_SLOT: TransitionSlot = TransitionSlot(1);
const HOTKEYS: [egui::Key; 9] = [
    egui::Key::Num1,
    egui::Key::Num2,
    egui::Key::Num3,
    egui::Key::Num4,
    egui::Key::Num5,
    egui::Key::Num6,
    egui::Key::Num7,
    egui::Key::Num8,
    egui::Key::Num9,
];

/// Bus an input is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tally {
    Off,
    Preview,
    Program,
}

impl Tally {
    fn color(self) -> egui::Color32 {
        match self {
            Self::Off => IDLE_COLOR,
            Self::Preview => PREVIEW_COLOR,
            Self::Program => PROGRAM_COLOR,
        }
    }
}

/// Tile band of a color tag.
fn tag_color(color: InputColor) -> egui::Color32 {
    match color {
        InputColor::Red => egui::Color32::from_rgb(230, 70, 70),
        InputColor::Orange => egui::Color32::from_rgb(240, 150, 50),
        InputColor::Yellow => egui::Color32::from_rgb(235, 210, 60),
        InputColor::Green => egui::Color32::from_rgb(80, 190, 90),
        InputColor::Cyan => egui::Color32::from_rgb(50, 180, 170),
        InputColor::Blue => egui::Color32::from_rgb(70, 130, 230),
        InputColor::Purple => egui::Color32::from_rgb(150, 90, 220),
    }
}

/// Short type badge of a tile.
fn source_badge(source: &SourceSpec) -> &'static str {
    match source {
        SourceSpec::Pattern { .. } => "PAT",
        SourceSpec::File { .. } => "FILE",
        SourceSpec::Url { .. } => "URL",
        SourceSpec::Srt { .. } => "SRT",
        SourceSpec::Device { .. } => "CAM",
        SourceSpec::Color { .. } => "COL",
        SourceSpec::Image { .. } => "IMG",
        SourceSpec::Playlist { .. } => "LIST",
        SourceSpec::Ndi { .. } => "NDI",
        SourceSpec::Whip { .. } => "WHIP",
        SourceSpec::AudioDevice { .. } => "MIC",
    }
}

/// Program wins when an input is on both (it never is, the core refuses).
pub fn tally(state: &ShowState, id: InputId) -> Tally {
    if state.program == Some(id) {
        return Tally::Program;
    }
    if state.preview == Some(id) {
        return Tally::Preview;
    }
    Tally::Off
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SourceKind {
    Pattern,
    Color,
    Image,
    File,
    Playlist,
    Url,
    Srt,
    Ndi,
    Whip,
    Device,
    AudioDevice,
}

impl SourceKind {
    const ALL: [SourceKind; 11] = [
        Self::Pattern,
        Self::Color,
        Self::Image,
        Self::File,
        Self::Playlist,
        Self::Url,
        Self::Srt,
        Self::Ndi,
        Self::Whip,
        Self::Device,
        Self::AudioDevice,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Pattern => "Test pattern",
            Self::File => "File",
            Self::Url => "URL",
            Self::Srt => "SRT",
            Self::Device => "Camera / capture",
            Self::Color => "Color",
            Self::Image => "Image",
            Self::Playlist => "Playlist",
            Self::Ndi => "NDI",
            Self::Whip => "WHIP (browser, OBS)",
            Self::AudioDevice => "Audio (mic, line)",
        }
    }

    /// Kind of device offered in the picker, if this source uses one.
    fn device_kind(self) -> Option<DeviceKind> {
        match self {
            Self::Device => Some(DeviceKind::Video),
            Self::AudioDevice => Some(DeviceKind::Audio),
            Self::Ndi => Some(DeviceKind::Ndi),
            _ => None,
        }
    }

    /// Picked from files with the "…" button.
    fn picks_files(self) -> bool {
        matches!(self, Self::File | Self::Image | Self::Playlist)
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Pattern => "",
            Self::File => "/path/to/clip.mp4",
            Self::Url => "https://…",
            Self::Srt => "srt://host:port",
            Self::Device | Self::AudioDevice => "device id (none found)",
            Self::Color => "",
            Self::Image => "/path/to/logo.png",
            Self::Playlist => "/clips/a.mp4; /clips/b.mp4",
            Self::Ndi => "HOST (Source name)",
            Self::Whip => "endpoint name, e.g. stage",
        }
    }
}

/// The "add input" row.
struct AddForm {
    kind: SourceKind,
    pattern: TestPattern,
    color: [u8; 3],
    text: String,
    name: String,
}

impl AddForm {
    /// The source to add; `None` while the text field is empty.
    fn source(&self) -> Option<SourceSpec> {
        let text = self.text.trim().to_string();
        let needs_text = !matches!(self.kind, SourceKind::Pattern | SourceKind::Color);
        if needs_text && text.is_empty() {
            return None;
        }
        Some(match self.kind {
            SourceKind::Pattern => SourceSpec::Pattern {
                pattern: self.pattern,
            },
            SourceKind::File => SourceSpec::File { path: text },
            SourceKind::Url => SourceSpec::Url { url: text },
            SourceKind::Srt => SourceSpec::Srt { uri: text },
            SourceKind::Device => SourceSpec::Device { id: text },
            SourceKind::AudioDevice => SourceSpec::AudioDevice { id: text },
            SourceKind::Color => {
                let [r, g, b] = self.color;
                SourceSpec::Color {
                    color: format!("#{r:02x}{g:02x}{b:02x}"),
                }
            }
            SourceKind::Image => SourceSpec::Image { path: text },
            SourceKind::Playlist => SourceSpec::Playlist {
                paths: text
                    .split(';')
                    .map(str::trim)
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect(),
            },
            SourceKind::Ndi => SourceSpec::Ndi { name: text },
            SourceKind::Whip => SourceSpec::Whip { endpoint: text },
        })
    }
}

pub struct CoreApp {
    client: CoreClient,
    tx: mpsc::Sender<CoreMessage>,
    rx: mpsc::Receiver<CoreMessage>,
    connected: bool,
    state: Option<ShowState>,
    error: Option<String>,
    pictures: HashMap<PictureTarget, egui::TextureHandle>,
    /// What the picture poller fetches; follows the input bar.
    wanted: Arc<Mutex<Vec<PictureTarget>>>,
    add: AddForm,
    show_path: String,
    renaming: Option<(InputId, String)>,
    devices: Vec<Device>,
    recent: Vec<String>,
    levels: Levels,
    /// Latest latency measurement and whether one runs.
    latency: Option<Latency>,
    measuring_latency: bool,
    /// Link health per running network output.
    output_stats: HashMap<OutputId, OutputStats>,
    /// Volume slider value per input while it is dragged.
    volume_drag: HashMap<InputId, u8>,
    new_output: OutputForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OutputKind {
    Srt,
    Record,
    Ndi,
    Window,
    Fullscreen,
    Drm,
}

impl OutputKind {
    const ALL: [OutputKind; 6] = [
        Self::Srt,
        Self::Record,
        Self::Ndi,
        Self::Window,
        Self::Fullscreen,
        Self::Drm,
    ];

    /// Whether the form needs the text field.
    fn needs_text(self) -> bool {
        !matches!(self, Self::Window | Self::Fullscreen)
    }

    fn label(self) -> &'static str {
        match self {
            Self::Srt => "SRT stream",
            Self::Record => "Recording",
            Self::Ndi => "NDI",
            Self::Window => "Screen: window",
            Self::Fullscreen => "Screen: fullscreen (Wayland)",
            Self::Drm => "Screen: connector (no desktop)",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Srt => "srt://host:9000",
            Self::Record => "folder in the engine's media directory",
            Self::Ndi => "NDI name, e.g. lmix PGM",
            Self::Window | Self::Fullscreen => "",
            Self::Drm => "DRM connector id",
        }
    }
}

/// The "+ Output" form.
struct OutputForm {
    kind: OutputKind,
    /// Program, or one input (ISO, MultiCorder).
    source: OutputSource,
    text: String,
    container: Container,
    video_kbps: u32,
}

impl OutputForm {
    /// The output to add; `None` while the text field is empty.
    fn spec(&self) -> Option<OutputSpec> {
        let text = self.text.trim().to_string();
        if self.kind.needs_text() && text.is_empty() {
            return None;
        }
        Some(match self.kind {
            OutputKind::Srt => OutputSpec::Srt {
                uri: text,
                video_kbps: self.video_kbps,
            },
            OutputKind::Record => OutputSpec::Record {
                folder: text,
                container: self.container,
                video_kbps: self.video_kbps,
            },
            OutputKind::Ndi => OutputSpec::Ndi { name: text },
            OutputKind::Window => OutputSpec::Display {
                mode: DisplayMode::Window,
            },
            OutputKind::Fullscreen => OutputSpec::Display {
                mode: DisplayMode::Fullscreen,
            },
            OutputKind::Drm => OutputSpec::Display {
                mode: DisplayMode::Drm {
                    connector: text.parse().ok()?,
                },
            },
        })
    }
}

impl CoreApp {
    pub fn new(url: String, picture_ms: u64, ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let client = CoreClient::new(url);
        let wanted = Arc::new(Mutex::new(vec![PictureTarget::Program]));
        client.follow(tx.clone(), ctx.clone());
        poll_pictures(
            client.clone(),
            wanted.clone(),
            tx.clone(),
            ctx.clone(),
            picture_ms,
        );

        let app = Self {
            client,
            tx,
            rx,
            connected: false,
            state: None,
            error: None,
            pictures: HashMap::new(),
            wanted,
            add: AddForm {
                kind: SourceKind::Pattern,
                pattern: TestPattern::Bars,
                color: DEFAULT_COLOR,
                text: String::new(),
                name: String::new(),
            },
            show_path: String::new(),
            renaming: None,
            devices: Vec::new(),
            recent: Vec::new(),
            levels: Levels::default(),
            latency: None,
            measuring_latency: false,
            output_stats: HashMap::new(),
            volume_drag: HashMap::new(),
            new_output: OutputForm {
                kind: OutputKind::Record,
                source: OutputSource::Program,
                text: String::new(),
                container: Container::default(),
                video_kbps: lmix_types::output::DEFAULT_VIDEO_KBPS,
            },
        };
        app.refresh_lists();
        app
    }

    /// Devices and recent shows: at start, after file actions, on request.
    fn refresh_lists(&self) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(CoreMessage::Devices(client.devices().await));
            let _ = tx.send(CoreMessage::Recent(client.recent().await));
        });
    }

    fn measure_latency(&mut self) {
        self.measuring_latency = true;
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(CoreMessage::Latency(client.measure_latency().await));
        });
    }

    /// Native file dialog (xdg portal: KDE's own on Plasma).
    fn pick_file(&self, purpose: FilePurpose) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let dialog = rfd::AsyncFileDialog::new();
            let file = match purpose {
                FilePurpose::OpenShow => {
                    dialog
                        .add_filter(SHOW_FILTER.0, SHOW_FILTER.1)
                        .pick_file()
                        .await
                }
                FilePurpose::SaveShow => {
                    dialog
                        .add_filter(SHOW_FILTER.0, SHOW_FILTER.1)
                        .save_file()
                        .await
                }
                FilePurpose::AddFile => dialog.pick_file().await,
                FilePurpose::AddPlaylist => {
                    for file in dialog.pick_files().await.unwrap_or_default() {
                        let path = file.path().display().to_string();
                        let _ = tx.send(CoreMessage::Picked(purpose, path));
                    }
                    return;
                }
            };
            let Some(file) = file else {
                return;
            };
            let _ = tx.send(CoreMessage::Picked(
                purpose,
                file.path().display().to_string(),
            ));
        });
    }

    fn send(&self, action: Action) {
        let client = self.client.clone();
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(CoreMessage::ActionDone(client.act(&action).await));
        });
    }

    fn drain(&mut self, ctx: &egui::Context) {
        while let Ok(message) = self.rx.try_recv() {
            match message {
                CoreMessage::Connected(connected) => self.connected = connected,
                CoreMessage::State(state) => self.set_state(state),
                CoreMessage::Levels(levels) => self.levels = levels,
                CoreMessage::Outputs(stats) => {
                    self.output_stats = stats.into_iter().map(|s| (s.output, s)).collect();
                }
                CoreMessage::ActionDone(error) => self.error = error,
                CoreMessage::Picture(target, image) => self.set_picture(ctx, target, image),
                CoreMessage::Devices(devices) => self.devices = devices,
                CoreMessage::Recent(recent) => self.recent = recent,
                CoreMessage::Latency(latency) => {
                    self.measuring_latency = false;
                    if latency.is_none() {
                        self.error = Some("latency measurement failed".into());
                    }
                    self.latency = latency;
                }
                CoreMessage::Picked(purpose, path) => self.picked(purpose, path),
            }
        }
    }

    fn picked(&mut self, purpose: FilePurpose, path: String) {
        match purpose {
            FilePurpose::OpenShow => self.send(Action::OpenShow { path }),
            FilePurpose::SaveShow => self.send(Action::SaveShow { path: Some(path) }),
            FilePurpose::AddFile => self.add.text = path,
            FilePurpose::AddPlaylist => {
                if !self.add.text.trim().is_empty() {
                    self.add.text.push_str("; ");
                }
                self.add.text.push_str(&path);
            }
        }
    }

    fn set_state(&mut self, state: ShowState) {
        // A new show path means a file was opened or saved: the recent
        // list changed.
        let old_path = self.state.as_ref().and_then(|s| s.path.clone());
        if state.path.is_some() && state.path != old_path {
            self.refresh_lists();
        }

        let mut wanted = vec![PictureTarget::Program];
        wanted.extend(state.inputs.iter().map(|i| PictureTarget::Input(i.id)));
        self.pictures.retain(|target, _| wanted.contains(target));
        *self.wanted.lock().expect("picture list lock") = wanted;

        if let Some(path) = &state.path {
            self.show_path = path.clone();
        }
        self.state = Some(state);
    }

    fn set_picture(&mut self, ctx: &egui::Context, target: PictureTarget, image: egui::ColorImage) {
        if let Some(texture) = self.pictures.get_mut(&target) {
            texture.set(image, egui::TextureOptions::LINEAR);
            return;
        }
        let texture = ctx.load_texture(format!("{target:?}"), image, egui::TextureOptions::LINEAR);
        self.pictures.insert(target, texture);
    }

    // ---- Panels ----

    fn toolbar(&mut self, ui: &mut egui::Ui, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            let (dot, text) = if self.connected {
                (PREVIEW_COLOR, "connected")
            } else {
                (PROGRAM_COLOR, "not connected")
            };
            status_dot(ui, dot);
            ui.label(format!("lmix-core {} · {text}", self.client.base_url()));
            ui.separator();

            if ui.button("New").clicked() {
                actions.push(Action::NewShow);
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.show_path)
                    .hint_text("show.lmix")
                    .desired_width(260.0),
            );
            let path = self.show_path.trim().to_string();
            if ui
                .add_enabled(!path.is_empty(), egui::Button::new("Open"))
                .on_hover_text(REMOTE_HINT)
                .clicked()
            {
                actions.push(Action::OpenShow { path: path.clone() });
            }
            if ui.button("Save").clicked() {
                let path = (!path.is_empty()).then_some(path);
                actions.push(Action::SaveShow { path });
            }
            if ui.button("Open…").on_hover_text(REMOTE_HINT).clicked() {
                self.pick_file(FilePurpose::OpenShow);
            }
            if ui.button("Save as…").clicked() {
                self.pick_file(FilePurpose::SaveShow);
            }
            ui.add_enabled_ui(!self.recent.is_empty(), |ui| {
                ui.menu_button("Recent", |ui| {
                    for recent in &self.recent {
                        if ui.button(recent).clicked() {
                            actions.push(Action::OpenShow {
                                path: recent.clone(),
                            });
                            ui.close();
                        }
                    }
                });
            });
            ui.separator();
            self.latency_button(ui);
        });

        let Some(error) = self.error.clone() else {
            return;
        };
        ui.horizontal(|ui| {
            ui.colored_label(PROGRAM_COLOR, error);
            if ui.small_button("✕").clicked() {
                self.error = None;
            }
        });
    }

    /// Measure capture-to-program latency (NFA.3) and show the result.
    fn latency_button(&mut self, ui: &mut egui::Ui) {
        let label = match self.measuring_latency {
            true => "Measuring…",
            false => "Latency",
        };
        let hint = format!(
            "Measure source-to-program latency for {} s",
            crate::core_client::LATENCY_SECONDS
        );
        if ui
            .add_enabled(!self.measuring_latency, egui::Button::new(label))
            .on_hover_text(hint)
            .clicked()
        {
            self.measure_latency();
        }
        if let Some(latency) = &self.latency {
            ui.label(latency_text(latency));
        }
    }

    fn monitors(&self, ui: &mut egui::Ui, state: &ShowState, actions: &mut Vec<Action>) {
        let preview = state
            .preview
            .and_then(|id| self.pictures.get(&PictureTarget::Input(id)));
        let program = self.pictures.get(&PictureTarget::Program);

        ui.columns(3, |columns| {
            monitor(&mut columns[0], "PREVIEW", PREVIEW_COLOR, preview, None);
            let soloed = state.inputs.iter().any(|i| i.listen != Listen::Off);
            if soloed {
                let clear = egui::Button::new(
                    egui::RichText::new("Clear solo").color(egui::Color32::BLACK),
                )
                .fill(SOLO_COLOR);
                if columns[0]
                    .add(clear)
                    .on_hover_text("End all PFL/AFL: monitor follows program")
                    .clicked()
                {
                    actions.push(Action::ClearListen);
                }
            }
            transition_bar(&mut columns[1], state, actions);
            monitor(
                &mut columns[2],
                "PROGRAM",
                PROGRAM_COLOR,
                program,
                Some(&self.levels),
            );
        });
    }

    fn input_bar(&mut self, ui: &mut egui::Ui, state: &ShowState, actions: &mut Vec<Action>) {
        if state.inputs.is_empty() {
            ui.weak("No inputs yet: add one below.");
            return;
        }
        egui::ScrollArea::horizontal().show(ui, |ui| {
            ui.horizontal(|ui| {
                for input in &state.inputs {
                    let picture = self.pictures.get(&PictureTarget::Input(input.id));
                    let tally = tally(state, input.id);
                    let reference = InputRef::Text(input.id.to_string());
                    let level = self.level_of(input.id);
                    let response = tile(
                        ui,
                        input,
                        tally,
                        picture,
                        level,
                        &mut self.volume_drag,
                        actions,
                    );

                    // Audio inputs have no picture: no preview, no take.
                    let has_video = input.source.has_video();
                    if response.double_clicked() && tally == Tally::Preview {
                        actions.push(Action::Transition { slot: TAKE_SLOT });
                    } else if response.clicked() && tally == Tally::Off && has_video {
                        actions.push(Action::PreviewInput {
                            input: reference.clone(),
                        });
                    }
                    let replacement = self.add.source();
                    response.context_menu(|ui| {
                        if ui.button("Cut to program").clicked() {
                            actions.push(Action::CutDirect {
                                input: reference.clone(),
                            });
                            ui.close();
                        }
                        // Swap what the input shows for the source set up in
                        // the "add input" row; name and position stay.
                        let replace =
                            egui::Button::new("Replace source with \"Add input\" selection");
                        if ui.add_enabled(replacement.is_some(), replace).clicked() {
                            actions.push(Action::ReplaceSource {
                                input: reference.clone(),
                                source: replacement.clone().expect("enabled only with a source"),
                            });
                            ui.close();
                        }
                        if let Some(ingest) = &input.ingest {
                            ingest_menu(ui, ingest);
                        }
                        ui.menu_button("On signal loss", |ui| {
                            for (mode, label) in [
                                (SignalLoss::Hold, "Hold last frame"),
                                (SignalLoss::Black, "Black"),
                            ] {
                                if ui
                                    .selectable_label(input.signal_loss == mode, label)
                                    .clicked()
                                {
                                    actions.push(Action::SetSignalLoss {
                                        input: reference.clone(),
                                        mode,
                                    });
                                    ui.close();
                                }
                            }
                        });
                        ui.menu_button("Color", |ui| {
                            for color in InputColor::ALL {
                                let label = egui::RichText::new(format!("■ {color:?}"))
                                    .color(tag_color(color));
                                if ui
                                    .selectable_label(input.color == Some(color), label)
                                    .clicked()
                                {
                                    actions.push(Action::SetInputColor {
                                        input: reference.clone(),
                                        color: Some(color),
                                    });
                                    ui.close();
                                }
                            }
                            if ui.button("None").clicked() {
                                actions.push(Action::SetInputColor {
                                    input: reference.clone(),
                                    color: None,
                                });
                                ui.close();
                            }
                        });
                        if ui.button("Rename…").clicked() {
                            self.renaming = Some((input.id, input.name.clone()));
                            ui.close();
                        }
                        if ui.button("Remove").clicked() {
                            actions.push(Action::RemoveInput {
                                input: reference.clone(),
                            });
                            ui.close();
                        }
                    });
                }
            });
        });
    }

    fn level_of(&self, id: InputId) -> Option<f32> {
        self.levels
            .inputs
            .iter()
            .find(|l| l.input == id)
            .map(|l| l.peak_db)
    }

    /// Outputs like vMix's bottom bar: click starts or stops, right click
    /// removes; "+ Output" adds one.
    fn output_bar(&mut self, ui: &mut egui::Ui, state: &ShowState, actions: &mut Vec<Action>) {
        // Running times tick even when nothing else changes.
        if state
            .outputs
            .iter()
            .any(|o| matches!(o.status, OutputStatus::Running { .. }))
        {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
        }
        ui.horizontal_wrapped(|ui| {
            ui.label("Outputs");
            for output in &state.outputs {
                let reference = OutputRef::Text(output.id.to_string());
                let (fill, text, hint) = output_face(output, self.output_stats.get(&output.id));
                let button =
                    egui::Button::new(egui::RichText::new(text).color(egui::Color32::WHITE))
                        .fill(fill);
                let response = ui.add(button).on_hover_text(hint);
                if response.clicked() {
                    let running = matches!(output.status, OutputStatus::Running { .. });
                    actions.push(match running {
                        true => Action::StopOutput {
                            output: reference.clone(),
                        },
                        false => Action::StartOutput {
                            output: reference.clone(),
                        },
                    });
                }
                response.context_menu(|ui| {
                    if ui.button("Remove").clicked() {
                        actions.push(Action::RemoveOutput {
                            output: reference.clone(),
                        });
                        ui.close();
                    }
                });
            }
            ui.menu_button("+ Output", |ui| self.output_form(ui, state, actions));
        });
    }

    fn output_form(&mut self, ui: &mut egui::Ui, state: &ShowState, actions: &mut Vec<Action>) {
        let form = &mut self.new_output;
        let source_name = |source: OutputSource| match source {
            OutputSource::Program => "Program".to_string(),
            OutputSource::Input(id) => state
                .inputs
                .iter()
                .find(|i| i.id == id)
                .map_or("?".into(), |i| format!("{} {}", i.number, i.name)),
        };
        egui::ComboBox::from_id_salt("output_source")
            .selected_text(source_name(form.source))
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut form.source, OutputSource::Program, "Program");
                for input in state.inputs.iter().filter(|i| i.source.has_video()) {
                    let source = OutputSource::Input(input.id);
                    ui.selectable_value(&mut form.source, source, source_name(source));
                }
            });
        egui::ComboBox::from_id_salt("output_kind")
            .selected_text(form.kind.label())
            .show_ui(ui, |ui| {
                for kind in OutputKind::ALL {
                    ui.selectable_value(&mut form.kind, kind, kind.label());
                }
            });
        if form.kind.needs_text() {
            ui.add(
                egui::TextEdit::singleline(&mut form.text)
                    .hint_text(form.kind.hint())
                    .desired_width(280.0),
            );
        }
        if matches!(form.kind, OutputKind::Srt | OutputKind::Record) {
            ui.add(egui::Slider::new(&mut form.video_kbps, VIDEO_KBPS_RANGE).text("video kbit/s"));
        }
        if form.kind == OutputKind::Record {
            egui::ComboBox::from_id_salt("output_container")
                .selected_text(format!("{:?}", form.container))
                .show_ui(ui, |ui| {
                    for container in Container::ALL {
                        ui.selectable_value(
                            &mut form.container,
                            container,
                            format!("{container:?}"),
                        );
                    }
                });
        }
        let spec = form.spec();
        if ui
            .add_enabled(spec.is_some(), egui::Button::new("Add"))
            .clicked()
        {
            actions.push(Action::AddOutput {
                spec: spec.expect("enabled only with a spec"),
                source: form.source,
                name: None,
            });
            form.text.clear();
            ui.close();
        }
    }

    fn add_row(&mut self, ui: &mut egui::Ui, state: &ShowState, actions: &mut Vec<Action>) {
        ui.horizontal(|ui| {
            ui.label("Add input");
            egui::ComboBox::from_id_salt("add_kind")
                .selected_text(self.add.kind.label())
                .show_ui(ui, |ui| {
                    for kind in SourceKind::ALL {
                        ui.selectable_value(&mut self.add.kind, kind, kind.label());
                    }
                });
            if self.add.kind == SourceKind::Pattern {
                egui::ComboBox::from_id_salt("add_pattern")
                    .selected_text(format!("{:?}", self.add.pattern))
                    .show_ui(ui, |ui| {
                        for pattern in PATTERNS {
                            ui.selectable_value(
                                &mut self.add.pattern,
                                pattern,
                                format!("{pattern:?}"),
                            );
                        }
                    });
            } else if self.add.kind == SourceKind::Color {
                ui.color_edit_button_srgb(&mut self.add.color);
            } else if let Some(kind) = self
                .add
                .kind
                .device_kind()
                .filter(|k| self.devices.iter().any(|d| d.kind == *k))
            {
                let offered: Vec<&Device> =
                    self.devices.iter().filter(|d| d.kind == kind).collect();
                let selected = offered
                    .iter()
                    .find(|d| device_value(d) == self.add.text)
                    .map_or("choose a device".to_string(), |d| device_label(d));
                egui::ComboBox::from_id_salt("add_device")
                    .selected_text(selected)
                    .width(320.0)
                    .show_ui(ui, |ui| {
                        for device in offered {
                            ui.selectable_value(
                                &mut self.add.text,
                                device_value(device),
                                device_label(device),
                            );
                        }
                    });
            } else {
                let hint = self.add.kind.hint();
                ui.add(
                    egui::TextEdit::singleline(&mut self.add.text)
                        .hint_text(hint)
                        .desired_width(320.0),
                );
            }
            if self.add.kind.picks_files() && ui.button("…").on_hover_text(REMOTE_HINT).clicked()
            {
                let purpose = if self.add.kind == SourceKind::Playlist {
                    FilePurpose::AddPlaylist
                } else {
                    FilePurpose::AddFile
                };
                self.pick_file(purpose);
            }
            if self.add.kind.device_kind().is_some()
                && ui
                    .button("↻")
                    .on_hover_text("Search devices again")
                    .clicked()
            {
                self.refresh_lists();
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.add.name)
                    .hint_text("name (optional)")
                    .desired_width(160.0),
            );

            let free = state.inputs.len() < state.slots;
            let source = self.add.source();
            if ui
                .add_enabled(free && source.is_some(), egui::Button::new("Add"))
                .clicked()
            {
                let name = self.add.name.trim().to_string();
                actions.push(Action::AddInput {
                    source: source.expect("button enabled only with a source"),
                    name: (!name.is_empty()).then_some(name),
                });
                self.add.text.clear();
                self.add.name.clear();
            }
            ui.weak(format!("{} / {} slots", state.inputs.len(), state.slots));
        });
    }

    fn rename_window(&mut self, ctx: &egui::Context, actions: &mut Vec<Action>) {
        let Some((id, name)) = &mut self.renaming else {
            return;
        };
        let mut done = false;
        egui::Window::new("Rename input")
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                let edit = ui.text_edit_singleline(name);
                let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                ui.horizontal(|ui| {
                    if ui.button("OK").clicked() || enter {
                        actions.push(Action::RenameInput {
                            input: InputRef::Text(id.to_string()),
                            name: name.trim().to_string(),
                        });
                        done = true;
                    }
                    if ui.button("Cancel").clicked() {
                        done = true;
                    }
                });
            });
        if done {
            self.renaming = None;
        }
    }

    /// Keys 1–9 put that input into preview, unless a text field has focus.
    fn hotkeys(&self, ctx: &egui::Context, state: &ShowState, actions: &mut Vec<Action>) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        for (index, key) in HOTKEYS.iter().enumerate() {
            let Some(input) = state.inputs.get(index) else {
                break;
            };
            if ctx.input(|i| i.key_pressed(*key)) && tally(state, input.id) == Tally::Off {
                actions.push(Action::PreviewInput {
                    input: InputRef::Number(input.number),
                });
            }
        }
    }
}

impl eframe::App for CoreApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain(&ctx);
        let mut actions = Vec::new();

        egui::Panel::top("toolbar").show_inside(ui, |ui| self.toolbar(ui, &mut actions));

        let Some(state) = self.state.clone() else {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.spinner();
                ui.label("Waiting for lmix-core …");
            });
            return;
        };

        self.hotkeys(&ctx, &state, &mut actions);
        egui::Panel::bottom("add").show_inside(ui, |ui| {
            self.output_bar(ui, &state, &mut actions);
            ui.separator();
            self.add_row(ui, &state, &mut actions);
        });
        egui::CentralPanel::default().show_inside(ui, |ui| {
            self.monitors(ui, &state, &mut actions);
            ui.separator();
            self.input_bar(ui, &state, &mut actions);
        });
        self.rename_window(&ctx, &mut actions);

        for action in actions {
            self.send(action);
        }
    }
}

// ---- Widgets ----

/// Monitor: label bar in the tally color over a 16:9 picture; program
/// and monitor-bus meters under it when `levels` is given (program).
fn monitor(
    ui: &mut egui::Ui,
    label: &str,
    color: egui::Color32,
    picture: Option<&egui::TextureHandle>,
    levels: Option<&Levels>,
) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(TILE_STROKE, color))
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).strong().color(color));
            let width = ui.available_width();
            picture_area(ui, egui::vec2(width, width * 9.0 / 16.0), picture);
            let Some(levels) = levels else {
                return;
            };
            for (name, level) in [
                ("PGM", levels.program_peak_db),
                ("MON", levels.monitor_peak_db),
            ] {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).small().weak());
                    meter(ui, level.unwrap_or(METER_FLOOR_DB));
                });
            }
        });
}

/// CUT, the four transition buttons (right click: effect and length) and
/// FTB, stacked between the monitors.
fn transition_bar(ui: &mut egui::Ui, state: &ShowState, actions: &mut Vec<Action>) {
    let can_take = state.preview.is_some();
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        let cut = egui::Button::new(egui::RichText::new("CUT").strong());
        if ui
            .add_enabled(can_take, cut.min_size(BUTTON_SIZE.into()))
            .clicked()
        {
            actions.push(Action::Cut);
        }

        for transition in &state.transitions {
            let text = format!(
                "{} {}\n{} ms",
                transition.slot.0,
                transition.effect.label(),
                transition.duration_ms
            );
            let button = egui::Button::new(text).min_size(BUTTON_SIZE.into());
            let response = ui
                .add_enabled(can_take, button)
                .on_hover_text(TRANSITION_HINT);
            if response.clicked() {
                actions.push(Action::Transition {
                    slot: transition.slot,
                });
            }
            response.context_menu(|ui| {
                for effect in Effect::ALL {
                    if ui
                        .selectable_label(effect == transition.effect, effect.label())
                        .clicked()
                    {
                        actions.push(Action::SetTransition {
                            slot: transition.slot,
                            effect,
                            duration_ms: transition.duration_ms,
                        });
                    }
                }
                ui.separator();
                for ms in DURATION_PRESETS_MS {
                    if ui
                        .selectable_label(ms == transition.duration_ms, format!("{ms} ms"))
                        .clicked()
                    {
                        actions.push(Action::SetTransition {
                            slot: transition.slot,
                            effect: transition.effect,
                            duration_ms: ms,
                        });
                    }
                }
            });
        }

        let mut ftb = egui::Button::new("FTB").min_size(BUTTON_SIZE.into());
        if state.fade_to_black {
            ftb = ftb.fill(PROGRAM_COLOR);
        }
        if ui.add(ftb).clicked() {
            actions.push(Action::FadeToBlack {
                duration_ms: lmix_types::action::DEFAULT_FTB_MS,
            });
        }
    });
}

/// Input tile: picture, number and name, frame in the tally color.
#[allow(clippy::too_many_arguments)]
fn tile(
    ui: &mut egui::Ui,
    input: &lmix_types::Input,
    tally: Tally,
    picture: Option<&egui::TextureHandle>,
    level: Option<f32>,
    volume_drag: &mut HashMap<InputId, u8>,
    actions: &mut Vec<Action>,
) -> egui::Response {
    let frame = egui::Frame::new()
        .stroke(egui::Stroke::new(TILE_STROKE, tally.color()))
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(TILE_WIDTH);
                let band = input.color.map_or(EMPTY_PICTURE, tag_color);
                let (rect, _) = ui
                    .allocate_exact_size(egui::vec2(TILE_WIDTH, COLOR_BAND), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, band);
                let size = egui::vec2(TILE_WIDTH, TILE_WIDTH * 9.0 / 16.0);
                if input.source.has_video() {
                    picture_area(ui, size, picture);
                } else {
                    audio_only_area(ui, size);
                }
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(input.number.to_string()).strong());
                    ui.label(
                        egui::RichText::new(source_badge(&input.source))
                            .small()
                            .weak(),
                    );
                    ui.label(&input.name);
                    if input.status != InputStatus::Online {
                        ui.weak(format!("{:?}", input.status).to_lowercase());
                    }
                });
                if input.source.has_transport() {
                    transport_row(ui, input, actions);
                }
                if input.has_audio {
                    audio_row(ui, input, level, actions);
                    volume_control(ui, input, volume_drag, actions);
                }
            });
        });
    ui.interact(
        frame.response.rect,
        ui.id().with(input.id),
        egui::Sense::click(),
    )
}

/// WHIP links to hand to the sender (B30): copy with one click.
fn ingest_menu(ui: &mut egui::Ui, ingest: &lmix_types::Ingest) {
    ui.menu_button("Send to this input", |ui| {
        for (label, url) in [
            ("WHIP (OBS, encoder)", &ingest.whip_url),
            ("Browser page (guest)", &ingest.browser_url),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                ui.monospace(url);
                if ui.small_button("Copy").clicked() {
                    ui.ctx().copy_text(url.clone());
                    ui.close();
                }
            });
        }
    });
}

/// Button color, text and hover text of an output; network outputs add
/// their link health while running.
fn output_face(
    output: &lmix_types::Output,
    stats: Option<&OutputStats>,
) -> (egui::Color32, String, String) {
    match &output.status {
        OutputStatus::Stopped => (
            IDLE_COLOR,
            output.name.clone(),
            "Stopped: click to start".into(),
        ),
        OutputStatus::Running { since_ms } => {
            let mut text = format!("{} {}", output.name, running_time(*since_ms));
            let mut hint = "Running: click to stop".to_string();
            if let Some(stats) = stats {
                text.push_str(&format!(" {}", link_rate(stats)));
                hint.push_str(&format!("\n{}", link_detail(stats)));
            }
            (PROGRAM_COLOR, text, hint)
        }
        OutputStatus::Failed { message } => {
            let text = format!("! {}", output.name);
            (
                AFV_COLOR,
                text,
                format!("Failed: {message}\nClick to start again"),
            )
        }
    }
}

/// Latency result in the toolbar.
fn latency_text(latency: &Latency) -> String {
    match (latency.median_ms, latency.p95_ms) {
        (Some(median), Some(p95)) => format!("PGM {median} ms (p95 {p95} ms)"),
        _ => "PGM latency –".into(),
    }
}

/// Short link state on the button: rate, or that nobody receives.
fn link_rate(stats: &OutputStats) -> String {
    if !stats.connected {
        return "no receiver".into();
    }
    format!("{:.1} Mb/s", f64::from(stats.kbps) / 1000.0)
}

/// Link details for the hover text.
fn link_detail(stats: &OutputStats) -> String {
    let rtt = stats
        .rtt_ms
        .map_or("RTT –".to_string(), |ms| format!("RTT {ms} ms"));
    format!(
        "{} · lost {} · dropped {} · {rtt}",
        link_rate(stats),
        stats.packets_lost,
        stats.packets_dropped
    )
}

/// `h:mm:ss` since `since_ms` (Unix ms).
fn running_time(since_ms: u64) -> String {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    let secs = now_ms.saturating_sub(since_ms) / 1000;
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// Picture area of an audio input: no picture, a label instead.
fn audio_only_area(ui: &mut egui::Ui, size: egui::Vec2) {
    let (_, rect) = ui.allocate_space(size);
    ui.painter().rect_filled(rect, 0.0, EMPTY_PICTURE);
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "AUDIO",
        egui::FontId::proportional(14.0),
        egui::Color32::GRAY,
    );
}

/// Restart, previous, play, pause, next for files, URLs and playlists.
fn transport_row(ui: &mut egui::Ui, input: &lmix_types::Input, actions: &mut Vec<Action>) {
    ui.horizontal(|ui| {
        for (transport, label, hint) in TRANSPORT_BUTTONS {
            if ui.small_button(label).on_hover_text(hint).clicked() {
                actions.push(Action::TransportInput {
                    input: InputRef::Text(input.id.to_string()),
                    transport,
                });
            }
        }
    });
}

/// Volume 0–100 on the tile (B28); sent when the drag ends, not on every
/// step, so the core and other clients see one change.
fn volume_control(
    ui: &mut egui::Ui,
    input: &lmix_types::Input,
    volume_drag: &mut HashMap<InputId, u8>,
    actions: &mut Vec<Action>,
) {
    let value = volume_drag.entry(input.id).or_insert(input.audio.volume);
    let slider = egui::Slider::new(value, 0..=lmix_types::input::VOLUME_MAX).show_value(true);
    let response = ui
        .add_sized([TILE_WIDTH, METER_HEIGHT * 3.0], slider)
        .on_hover_text("Volume");
    if response.drag_stopped() || (response.changed() && !response.dragged()) {
        actions.push(Action::SetVolume {
            input: InputRef::Text(input.id.to_string()),
            volume: *value,
        });
    }
    if !response.dragged() && !response.has_focus() {
        volume_drag.remove(&input.id);
    }
}

/// "M" (sound on/off), "AFV" (audio follows video), "PFL"/"AFL" (solo on
/// the monitor bus) and the input level.
fn audio_row(
    ui: &mut egui::Ui,
    input: &lmix_types::Input,
    level: Option<f32>,
    actions: &mut Vec<Action>,
) {
    let reference = || InputRef::Text(input.id.to_string());
    let audio = input.audio;
    ui.horizontal(|ui| {
        let mute = egui::Button::new(egui::RichText::new("M").small())
            .selected(!audio.on)
            .fill(if audio.on {
                egui::Color32::TRANSPARENT
            } else {
                AUDIO_OFF_COLOR
            });
        if ui.add(mute).on_hover_text("Sound on/off").clicked() {
            actions.push(Action::SetAudio {
                input: reference(),
                on: !audio.on,
            });
        }
        let follow =
            egui::Button::new(egui::RichText::new("AFV").small()).fill(if audio.follow_video {
                AFV_COLOR
            } else {
                egui::Color32::TRANSPARENT
            });
        if ui
            .add(follow)
            .on_hover_text("Audio follows video: heard only on program")
            .clicked()
        {
            actions.push(Action::SetAudioFollow {
                input: reference(),
                follow: !audio.follow_video,
            });
        }
        for (listen, label, hint) in [
            (
                Listen::Pfl,
                "PFL",
                "Pre-fader listen on the monitor bus (even when muted)",
            ),
            (
                Listen::Afl,
                "AFL",
                "After-fader listen on the monitor bus (as on program)",
            ),
        ] {
            let active = input.listen == listen;
            let text = egui::RichText::new(label).small();
            let text = if active {
                text.color(egui::Color32::BLACK)
            } else {
                text
            };
            let button = egui::Button::new(text).fill(if active {
                SOLO_COLOR
            } else {
                egui::Color32::TRANSPARENT
            });
            if ui.add(button).on_hover_text(hint).clicked() {
                actions.push(Action::SetListen {
                    input: reference(),
                    listen: if active { Listen::Off } else { listen },
                });
            }
        }
        meter(ui, level.unwrap_or(METER_FLOOR_DB));
    });
}

/// Peak meter filling the rest of the row.
fn meter(ui: &mut egui::Ui, peak_db: f32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, METER_HEIGHT), egui::Sense::hover());
    ui.painter().rect_filled(rect, 0.0, EMPTY_PICTURE);
    let fill = ((peak_db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0);
    let color = if peak_db > METER_HOT_DB {
        METER_HOT_COLOR
    } else {
        METER_COLOR
    };
    let bar = egui::Rect::from_min_size(rect.min, egui::vec2(width * fill, METER_HEIGHT));
    ui.painter().rect_filled(bar, 0.0, color);
}

/// What the picker puts into the source: the name for NDI, else the id.
fn device_value(device: &Device) -> String {
    match device.kind {
        DeviceKind::Ndi => device.name.clone(),
        DeviceKind::Video | DeviceKind::Audio => device.id.clone(),
    }
}

fn device_label(device: &Device) -> String {
    format!("{} ({})", device.name, device.api)
}

/// Filled circle; the default font has no "●".
fn status_dot(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(STATUS_DOT, STATUS_DOT), egui::Sense::hover());
    ui.painter()
        .circle_filled(rect.center(), STATUS_DOT / 2.0, color);
}

fn picture_area(ui: &mut egui::Ui, size: egui::Vec2, picture: Option<&egui::TextureHandle>) {
    if let Some(texture) = picture {
        ui.image((texture.id(), size));
        return;
    }
    let (_, rect) = ui.allocate_space(size);
    ui.painter().rect_filled(rect, 0.0, EMPTY_PICTURE);
}

/// Fetch every wanted picture each `interval_ms`.
fn poll_pictures(
    client: CoreClient,
    wanted: Arc<Mutex<Vec<PictureTarget>>>,
    tx: mpsc::Sender<CoreMessage>,
    ctx: egui::Context,
    interval_ms: u64,
) {
    tokio::spawn(async move {
        loop {
            let targets = wanted.lock().expect("picture list lock").clone();
            for target in targets {
                let Some(jpeg) = client.picture(target).await else {
                    continue;
                };
                let Ok(image) = image::load_from_memory(&jpeg) else {
                    continue;
                };
                let rgba = image.to_rgba8();
                let size = [rgba.width() as usize, rgba.height() as usize];
                let image = egui::ColorImage::from_rgba_unmultiplied(size, &rgba);
                let _ = tx.send(CoreMessage::Picture(target, image));
            }
            ctx.request_repaint();
            tokio::time::sleep(Duration::from_millis(interval_ms)).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::state::API_VERSION;

    fn state(preview: Option<InputId>, program: Option<InputId>) -> ShowState {
        ShowState {
            api_version: API_VERSION,
            path: None,
            inputs: Vec::new(),
            preview,
            program,
            transitions: Vec::new(),
            fade_to_black: false,
            slots: 4,
            outputs: Vec::new(),
        }
    }

    #[test]
    fn latency_in_the_toolbar() {
        let ages: Vec<u64> = (100..=120).collect();

        assert_eq!(latency_text(&Latency::of(&ages)), "PGM 110 ms (p95 119 ms)");
        assert_eq!(latency_text(&Latency::of(&[])), "PGM latency –");
    }

    #[test]
    fn link_health_on_the_output_button() {
        let mut stats = OutputStats {
            output: OutputId::new(),
            connected: true,
            kbps: 5960,
            packets_lost: 3,
            packets_dropped: 0,
            rtt_ms: Some(18),
        };

        assert_eq!(link_rate(&stats), "6.0 Mb/s");
        assert_eq!(
            link_detail(&stats),
            "6.0 Mb/s · lost 3 · dropped 0 · RTT 18 ms"
        );

        stats.connected = false;
        assert_eq!(link_rate(&stats), "no receiver");
    }

    #[test]
    fn tally_follows_the_buses() {
        let (a, b, c) = (InputId::new(), InputId::new(), InputId::new());
        let s = state(Some(a), Some(b));

        assert_eq!(tally(&s, a), Tally::Preview);
        assert_eq!(tally(&s, b), Tally::Program);
        assert_eq!(tally(&s, c), Tally::Off);
    }

    #[test]
    fn running_time_counts_hours_minutes_seconds() {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        assert_eq!(running_time(now_ms - 3_725_000), "1:02:05");
    }

    #[test]
    fn screen_outputs_need_no_text_except_a_connector() {
        let mut form = OutputForm {
            kind: OutputKind::Window,
            source: OutputSource::Program,
            text: String::new(),
            container: Container::Mkv,
            video_kbps: 6000,
        };
        assert_eq!(
            form.spec(),
            Some(OutputSpec::Display {
                mode: DisplayMode::Window
            })
        );

        form.kind = OutputKind::Drm;
        assert_eq!(form.spec(), None);
        form.text = "42".into();
        let spec = Some(OutputSpec::Display {
            mode: DisplayMode::Drm { connector: 42 },
        });
        assert_eq!(form.spec(), spec);
    }

    #[test]
    fn output_form_needs_a_target() {
        let mut form = OutputForm {
            kind: OutputKind::Srt,
            source: OutputSource::Program,
            text: " ".into(),
            container: Container::Mkv,
            video_kbps: 6000,
        };
        assert_eq!(form.spec(), None);

        form.text = "srt://cdn:9000".into();
        assert!(matches!(
            form.spec(),
            Some(OutputSpec::Srt {
                video_kbps: 6000,
                ..
            })
        ));
    }

    #[test]
    fn add_form_builds_color_and_playlist() {
        let mut form = AddForm {
            kind: SourceKind::Color,
            pattern: TestPattern::Bars,
            color: [0xff, 0x80, 0x00],
            text: String::new(),
            name: String::new(),
        };
        assert_eq!(
            form.source(),
            Some(SourceSpec::Color {
                color: "#ff8000".into()
            })
        );

        form.kind = SourceKind::Playlist;
        form.text = "/a.mp4; ;/b.mp4 ".into();
        let paths = vec!["/a.mp4".to_string(), "/b.mp4".to_string()];
        assert_eq!(form.source(), Some(SourceSpec::Playlist { paths }));
    }

    #[test]
    fn add_form_needs_text_except_for_patterns() {
        let mut form = AddForm {
            kind: SourceKind::File,
            pattern: TestPattern::Bars,
            color: DEFAULT_COLOR,
            text: "  ".into(),
            name: String::new(),
        };
        assert_eq!(form.source(), None);

        form.text = " /a.mp4 ".into();
        assert_eq!(
            form.source(),
            Some(SourceSpec::File {
                path: "/a.mp4".into()
            })
        );

        form.kind = SourceKind::Pattern;
        assert_eq!(
            form.source(),
            Some(SourceSpec::Pattern {
                pattern: TestPattern::Bars
            })
        );
    }
}
