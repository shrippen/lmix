//! Console as a client of lmix-core (`--core`, ROADMAP 2b.2, persona
//! finding B22): input bar, PREVIEW/PROGRAM monitors, transition buttons.
//!
//! ```text
//!   ┌ toolbar: lmix-core ● · show file · New / Open / Save ┐
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

use lmix_types::{Action, Effect, InputId, InputRef, InputStatus, ShowState, SourceSpec, TestPattern, TransitionSlot};

use crate::core_client::{CoreClient, CoreMessage, PictureTarget};

const PREVIEW_COLOR: egui::Color32 = egui::Color32::from_rgb(40, 200, 80);
const PROGRAM_COLOR: egui::Color32 = egui::Color32::from_rgb(220, 40, 40);
const IDLE_COLOR: egui::Color32 = egui::Color32::from_gray(70);
const EMPTY_PICTURE: egui::Color32 = egui::Color32::from_gray(20);
const TILE_WIDTH: f32 = 160.0;
const TILE_STROKE: f32 = 3.0;
const STATUS_DOT: f32 = 10.0;
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
    File,
    Url,
    Srt,
    Device,
}

impl SourceKind {
    const ALL: [SourceKind; 5] = [Self::Pattern, Self::File, Self::Url, Self::Srt, Self::Device];

    fn label(self) -> &'static str {
        match self {
            Self::Pattern => "Test pattern",
            Self::File => "File",
            Self::Url => "URL",
            Self::Srt => "SRT",
            Self::Device => "Device",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Pattern => "",
            Self::File => "/path/to/clip.mp4",
            Self::Url => "https://…",
            Self::Srt => "srt://host:port",
            Self::Device => "device id",
        }
    }
}

/// The "add input" row.
struct AddForm {
    kind: SourceKind,
    pattern: TestPattern,
    text: String,
    name: String,
}

impl AddForm {
    /// The source to add; `None` while the text field is empty.
    fn source(&self) -> Option<SourceSpec> {
        let text = self.text.trim().to_string();
        if self.kind != SourceKind::Pattern && text.is_empty() {
            return None;
        }
        Some(match self.kind {
            SourceKind::Pattern => SourceSpec::Pattern { pattern: self.pattern },
            SourceKind::File => SourceSpec::File { path: text },
            SourceKind::Url => SourceSpec::Url { url: text },
            SourceKind::Srt => SourceSpec::Srt { uri: text },
            SourceKind::Device => SourceSpec::Device { id: text },
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
}

impl CoreApp {
    pub fn new(url: String, picture_ms: u64, ctx: &egui::Context) -> Self {
        let (tx, rx) = mpsc::channel();
        let client = CoreClient::new(url);
        let wanted = Arc::new(Mutex::new(vec![PictureTarget::Program]));
        client.follow(tx.clone(), ctx.clone());
        poll_pictures(client.clone(), wanted.clone(), tx.clone(), ctx.clone(), picture_ms);

        Self {
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
                text: String::new(),
                name: String::new(),
            },
            show_path: String::new(),
            renaming: None,
        }
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
                CoreMessage::ActionDone(error) => self.error = error,
                CoreMessage::Picture(target, image) => self.set_picture(ctx, target, image),
            }
        }
    }

    fn set_state(&mut self, state: ShowState) {
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
            ui.add(egui::TextEdit::singleline(&mut self.show_path).hint_text("show.lmix").desired_width(260.0));
            let path = self.show_path.trim().to_string();
            if ui.add_enabled(!path.is_empty(), egui::Button::new("Open")).clicked() {
                actions.push(Action::OpenShow { path: path.clone() });
            }
            if ui.button("Save").clicked() {
                let path = (!path.is_empty()).then_some(path);
                actions.push(Action::SaveShow { path });
            }
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

    fn monitors(&self, ui: &mut egui::Ui, state: &ShowState, actions: &mut Vec<Action>) {
        let preview = state.preview.and_then(|id| self.pictures.get(&PictureTarget::Input(id)));
        let program = self.pictures.get(&PictureTarget::Program);

        ui.columns(3, |columns| {
            monitor(&mut columns[0], "PREVIEW", PREVIEW_COLOR, preview);
            transition_bar(&mut columns[1], state, actions);
            monitor(&mut columns[2], "PROGRAM", PROGRAM_COLOR, program);
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
                    let response = tile(ui, input, tally, picture);
                    let reference = InputRef::Text(input.id.to_string());

                    if response.double_clicked() && tally == Tally::Preview {
                        actions.push(Action::Transition { slot: TAKE_SLOT });
                    } else if response.clicked() && tally == Tally::Off {
                        actions.push(Action::PreviewInput { input: reference.clone() });
                    }
                    let replacement = self.add.source();
                    response.context_menu(|ui| {
                        if ui.button("Cut to program").clicked() {
                            actions.push(Action::CutDirect { input: reference.clone() });
                            ui.close();
                        }
                        // Swap what the input shows for the source set up in
                        // the "add input" row; name and position stay.
                        let replace = egui::Button::new("Replace source with \"Add input\" selection");
                        if ui.add_enabled(replacement.is_some(), replace).clicked() {
                            actions.push(Action::ReplaceSource {
                                input: reference.clone(),
                                source: replacement.clone().expect("enabled only with a source"),
                            });
                            ui.close();
                        }
                        if ui.button("Rename…").clicked() {
                            self.renaming = Some((input.id, input.name.clone()));
                            ui.close();
                        }
                        if ui.button("Remove").clicked() {
                            actions.push(Action::RemoveInput { input: reference.clone() });
                            ui.close();
                        }
                    });
                }
            });
        });
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
                            ui.selectable_value(&mut self.add.pattern, pattern, format!("{pattern:?}"));
                        }
                    });
            } else {
                let hint = self.add.kind.hint();
                ui.add(egui::TextEdit::singleline(&mut self.add.text).hint_text(hint).desired_width(320.0));
            }
            ui.add(egui::TextEdit::singleline(&mut self.add.name).hint_text("name (optional)").desired_width(160.0));

            let free = state.inputs.len() < state.slots;
            let source = self.add.source();
            if ui.add_enabled(free && source.is_some(), egui::Button::new("Add")).clicked() {
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
        egui::Window::new("Rename input").collapsible(false).resizable(false).show(ctx, |ui| {
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
        egui::Panel::bottom("add").show_inside(ui, |ui| self.add_row(ui, &state, &mut actions));
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

/// Monitor: label bar in the tally color over a 16:9 picture.
fn monitor(ui: &mut egui::Ui, label: &str, color: egui::Color32, picture: Option<&egui::TextureHandle>) {
    egui::Frame::new()
        .stroke(egui::Stroke::new(TILE_STROKE, color))
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.label(egui::RichText::new(label).strong().color(color));
            let width = ui.available_width();
            picture_area(ui, egui::vec2(width, width * 9.0 / 16.0), picture);
        });
}

/// CUT, the four transition buttons (right click: effect and length) and
/// FTB, stacked between the monitors.
fn transition_bar(ui: &mut egui::Ui, state: &ShowState, actions: &mut Vec<Action>) {
    let can_take = state.preview.is_some();
    ui.vertical_centered(|ui| {
        ui.add_space(24.0);
        let cut = egui::Button::new(egui::RichText::new("CUT").strong());
        if ui.add_enabled(can_take, cut.min_size(BUTTON_SIZE.into())).clicked() {
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
            let response = ui.add_enabled(can_take, button).on_hover_text(TRANSITION_HINT);
            if response.clicked() {
                actions.push(Action::Transition { slot: transition.slot });
            }
            response.context_menu(|ui| {
                for effect in Effect::ALL {
                    if ui.selectable_label(effect == transition.effect, effect.label()).clicked() {
                        actions.push(Action::SetTransition {
                            slot: transition.slot,
                            effect,
                            duration_ms: transition.duration_ms,
                        });
                    }
                }
                ui.separator();
                for ms in DURATION_PRESETS_MS {
                    if ui.selectable_label(ms == transition.duration_ms, format!("{ms} ms")).clicked() {
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
fn tile(
    ui: &mut egui::Ui,
    input: &lmix_types::Input,
    tally: Tally,
    picture: Option<&egui::TextureHandle>,
) -> egui::Response {
    let frame = egui::Frame::new()
        .stroke(egui::Stroke::new(TILE_STROKE, tally.color()))
        .inner_margin(4.0)
        .show(ui, |ui| {
            ui.vertical(|ui| {
                ui.set_width(TILE_WIDTH);
                picture_area(ui, egui::vec2(TILE_WIDTH, TILE_WIDTH * 9.0 / 16.0), picture);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(input.number.to_string()).strong());
                    ui.label(&input.name);
                    if input.status != InputStatus::Online {
                        ui.weak(format!("{:?}", input.status).to_lowercase());
                    }
                });
            });
        });
    ui.interact(frame.response.rect, ui.id().with(input.id), egui::Sense::click())
}

/// Filled circle; the default font has no "●".
fn status_dot(ui: &mut egui::Ui, color: egui::Color32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(STATUS_DOT, STATUS_DOT), egui::Sense::hover());
    ui.painter().circle_filled(rect.center(), STATUS_DOT / 2.0, color);
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
        }
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
    fn add_form_needs_text_except_for_patterns() {
        let mut form = AddForm {
            kind: SourceKind::File,
            pattern: TestPattern::Bars,
            text: "  ".into(),
            name: String::new(),
        };
        assert_eq!(form.source(), None);

        form.text = " /a.mp4 ".into();
        assert_eq!(form.source(), Some(SourceSpec::File { path: "/a.mp4".into() }));

        form.kind = SourceKind::Pattern;
        assert_eq!(form.source(), Some(SourceSpec::Pattern { pattern: TestPattern::Bars }));
    }
}
