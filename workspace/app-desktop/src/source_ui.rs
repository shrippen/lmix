//! "New flow from sources" panel: pick test patterns, devices, files and
//! URLs, then hand the list to the Console which turns it into a flow
//! (see `sources::build_flow`).

use crate::sources::{SourceSpec, TestPattern, MAX_SOURCES, MIN_SOURCES};
use strom_types::discovery::DeviceResponse;

const DEFAULT_FLOW_NAME: &str = "Console test";
const DEFAULT_PATTERN_COUNT: usize = 8;

pub struct CreateRequest {
    pub name: String,
    pub sources: Vec<SourceSpec>,
}

pub struct SourceBuilder {
    name: String,
    sources: Vec<SourceSpec>,
    pattern: TestPattern,
    device: Option<usize>,
    file: String,
    url: String,
    srt: String,
}

impl SourceBuilder {
    pub fn new() -> Self {
        Self {
            name: DEFAULT_FLOW_NAME.to_string(),
            sources: Vec::new(),
            pattern: TestPattern::Smpte100,
            device: None,
            file: String::new(),
            url: String::new(),
            srt: String::new(),
        }
    }

    /// The default 8-pattern set as a ready-to-create request.
    pub fn preset_request(&mut self) -> CreateRequest {
        self.add_defaults();
        CreateRequest {
            name: self.name.clone(),
            sources: self.sources.clone(),
        }
    }

    fn add_defaults(&mut self) {
        self.sources = TestPattern::ALL
            .iter()
            .take(DEFAULT_PATTERN_COUNT)
            .map(|p| SourceSpec::Pattern(*p))
            .collect();
    }

    fn add_pattern_row(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("pattern")
                .selected_text(self.pattern.label())
                .show_ui(ui, |ui| {
                    for p in TestPattern::ALL {
                        ui.selectable_value(&mut self.pattern, p, p.label());
                    }
                });
            if ui.button("Add test pattern").clicked() {
                self.sources.push(SourceSpec::Pattern(self.pattern));
            }
        });
    }

    fn add_device_row(&mut self, ui: &mut egui::Ui, devices: &[DeviceResponse]) {
        ui.horizontal(|ui| {
            let selected = self
                .device
                .and_then(|i| devices.get(i))
                .map_or("(no device)", |d| d.name.as_str());
            egui::ComboBox::from_id_salt("device")
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for (i, d) in devices.iter().enumerate() {
                        ui.selectable_value(&mut self.device, Some(i), &d.name);
                    }
                });
            let chosen = self.device.and_then(|i| devices.get(i));
            if ui
                .add_enabled(chosen.is_some(), egui::Button::new("Add device"))
                .clicked()
            {
                if let Some(d) = chosen {
                    self.sources.push(SourceSpec::Device {
                        id: d.id.clone(),
                        name: d.name.clone(),
                    });
                }
            }
        });
    }

    fn add_text_row(ui: &mut egui::Ui, text: &mut String, hint: &str, button: &str) -> bool {
        let mut add = false;
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(420.0));
            add = ui
                .add_enabled(!text.trim().is_empty(), egui::Button::new(button))
                .clicked();
        });
        add
    }

    /// Draw the panel; returns a request when the user presses Create.
    pub fn show(&mut self, ui: &mut egui::Ui, devices: &[DeviceResponse]) -> Option<CreateRequest> {
        ui.heading("New flow from sources");
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.text_edit_singleline(&mut self.name);
            if ui.button("Preset: 8 test patterns").clicked() {
                self.add_defaults();
            }
        });

        self.add_pattern_row(ui);
        self.add_device_row(ui, devices);
        if Self::add_text_row(ui, &mut self.file, "/path/to/video.mp4", "Add file") {
            self.sources.push(SourceSpec::File(self.file.trim().to_string()));
            self.file.clear();
        }
        if Self::add_text_row(ui, &mut self.url, "https://... or rtsp://...", "Add URL") {
            self.sources.push(SourceSpec::Url(self.url.trim().to_string()));
            self.url.clear();
        }
        if Self::add_text_row(ui, &mut self.srt, "srt://:9000?mode=listener", "Add SRT") {
            self.sources.push(SourceSpec::Srt(self.srt.trim().to_string()));
            self.srt.clear();
        }

        let mut remove = None;
        for (i, s) in self.sources.iter().enumerate() {
            ui.horizontal(|ui| {
                ui.label(format!("{}  {}", i + 1, s.label()));
                if ui.small_button("x").clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            self.sources.remove(i);
        }

        let count = self.sources.len();
        let valid = (MIN_SOURCES..=MAX_SOURCES).contains(&count);
        let mut request = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(valid, egui::Button::new("Create + open"))
                .clicked()
            {
                request = Some(CreateRequest {
                    name: self.name.clone(),
                    sources: self.sources.clone(),
                });
            }
            ui.weak(format!("{count} sources ({MIN_SOURCES}-{MAX_SOURCES})"));
        });
        request
    }
}
