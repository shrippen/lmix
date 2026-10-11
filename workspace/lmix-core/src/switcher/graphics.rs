//! Titles and data sources in the show (GFX, DAT): graphics channels with
//! title layers, saved data sets, and data sources feeding fields.
//!
//! Nothing here talks to the engine: the graphics page follows the state
//! (`/api/events`) and calls the templates. The service reads data source
//! files and the templates folder and hands the results in
//! ([`Switcher::set_rows`], [`Switcher::set_templates`]).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use lmix_types::graphics::{
    cell, formatted, DATASETS, DATA_AUTO_MIN_MS, DATA_ROWS, DATA_SOURCES, GRAPHICS, GRAPHICS_LAYERS,
};
use lmix_types::{
    Action, ActionError, BindingSpec, DataSource, Dataset, FieldBinding, Graphics, InputId,
    InputRef, TitleCall, TitleData, TitleLayer, TitlePhase,
};

use super::{Command, Switcher};
use crate::show::{ShowDataSource, ShowGraphics, ShowTitle};

/// Longest function name a title call may carry.
const CALL_NAME_MAX: usize = 64;

/// A template as the switcher needs it: where the page loads it, and its
/// revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateRef {
    pub url: String,
    pub revision: u64,
}

/// Rows read from a data source's file, or why it could not be read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rows {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub error: Option<String>,
}

/// Runtime state of titles and data sources (not saved).
#[derive(Debug, Default)]
pub struct TitleRuntime {
    /// Templates by id (built in and from the folder).
    templates: HashMap<String, TemplateRef>,
    /// Rows per data source.
    rows: HashMap<InputId, Rows>,
    /// Last function call per graphics channel and layer.
    calls: HashMap<(InputId, u32), TitleCall>,
    call_seq: u64,
    /// Plays per graphics channel and layer.
    plays: HashMap<(InputId, u32), u64>,
    /// When each auto-advancing data source moves on next.
    due: HashMap<InputId, Instant>,
}

impl TitleRuntime {
    /// After a new or opened show: nothing of the old one is kept.
    pub fn reset(&mut self) {
        self.rows.clear();
        self.calls.clear();
        self.plays.clear();
        self.due.clear();
    }
}

fn reference(r: &InputRef) -> String {
    match r {
        InputRef::Number(n) => n.to_string(),
        InputRef::Text(t) => format!("\"{t}\""),
    }
}

/// Position of the item `r` names: 1-based number, id, then name.
fn position<T>(
    items: &[T],
    r: &InputRef,
    id: impl Fn(&T) -> InputId,
    name: impl Fn(&T) -> &str,
) -> Option<usize> {
    match r {
        InputRef::Number(n) => (*n as usize).checked_sub(1).filter(|i| *i < items.len()),
        InputRef::Text(text) => items
            .iter()
            .position(|item| id(item).to_string() == *text)
            .or_else(|| items.iter().position(|item| name(item) == text)),
    }
}

fn check_layer(layer: u32) -> Result<(), ActionError> {
    if layer == 0 || layer > GRAPHICS_LAYERS {
        return Err(ActionError::BadTitle {
            reason: format!("layer {layer} is outside 1–{GRAPHICS_LAYERS}"),
        });
    }
    Ok(())
}

fn no_title(layer: u32) -> ActionError {
    ActionError::BadTitle {
        reason: format!("nothing loaded on layer {layer}"),
    }
}

/// Fields set by an update: empty values remove the field.
fn merge(into: &mut TitleData, data: TitleData) {
    for (field, value) in data {
        if value.is_empty() {
            into.remove(&field);
        } else {
            into.insert(field, value);
        }
    }
}

impl Switcher {
    /// Templates the folder and the built-in set offer, with revisions.
    pub fn set_templates(&mut self, templates: HashMap<String, TemplateRef>) {
        self.titles.templates = templates;
    }

    /// Rows of a data source after its file was read (again); keeps the
    /// selected row where it can.
    pub fn set_rows(&mut self, source: InputId, mut rows: Rows) {
        rows.rows.truncate(DATA_ROWS);
        if let Some(entry) = self.show.data_sources.iter_mut().find(|s| s.id == source) {
            let count = rows.rows.len() as u32;
            entry.row = match (entry.row, count) {
                (_, 0) => 0,
                (0, _) => 1,
                (row, count) => row.min(count),
            };
        }
        self.titles.rows.insert(source, rows);
    }

    /// Rows of a data source as last read.
    pub fn rows(&self, source: InputId) -> Option<&Rows> {
        self.titles.rows.get(&source)
    }

    /// Data sources whose rows were never read (just added or opened).
    pub fn unread_sources(&self) -> Vec<(InputId, lmix_types::DataSourceSpec)> {
        self.show
            .data_sources
            .iter()
            .filter(|s| !self.titles.rows.contains_key(&s.id))
            .map(|s| (s.id, s.spec.clone()))
            .collect()
    }

    /// Data sources with their file paths (to follow changes).
    pub fn source_files(&self) -> Vec<(InputId, lmix_types::DataSourceSpec)> {
        self.show
            .data_sources
            .iter()
            .map(|s| (s.id, s.spec.clone()))
            .collect()
    }

    /// Auto-advancing data sources due at `now`; their next turn is set.
    pub fn due_sources(&mut self, now: Instant) -> Vec<InputId> {
        let mut due = Vec::new();
        for source in &self.show.data_sources {
            let Some(ms) = source.auto_ms else {
                self.titles.due.remove(&source.id);
                continue;
            };
            let every = Duration::from_millis(u64::from(ms));
            let next = *self.titles.due.entry(source.id).or_insert(now + every);
            if next <= now {
                self.titles.due.insert(source.id, now + every);
                due.push(source.id);
            }
        }
        due
    }

    pub(super) fn apply_title(&mut self, action: Action) -> Result<Vec<Command>, ActionError> {
        match action {
            Action::AddGraphics { name } => {
                if self.show.graphics.len() >= GRAPHICS {
                    return Err(ActionError::GraphicsFull { max: GRAPHICS });
                }
                let number = self.show.graphics.len() + 1;
                self.show.graphics.push(ShowGraphics {
                    id: InputId::new(),
                    name: name.unwrap_or_else(|| format!("Graphics {number}")),
                    layers: Vec::new(),
                    triggers: Vec::new(),
                });
            }
            Action::RemoveGraphics { graphics } => {
                self.check_unlocked()?;
                let index = self.find_graphics(&graphics)?;
                let removed = self.show.graphics.remove(index);
                self.titles.calls.retain(|(id, _), _| *id != removed.id);
            }
            Action::RenameGraphics { graphics, name } => {
                let index = self.find_graphics(&graphics)?;
                self.show.graphics[index].name = name;
            }
            Action::TitleLoad {
                graphics,
                layer,
                template,
                data,
                play,
            } => {
                check_layer(layer)?;
                let index = self.find_graphics(&graphics)?;
                if !self.titles.templates.contains_key(&template) {
                    return Err(ActionError::UnknownTemplate { template });
                }
                let channel = &mut self.show.graphics[index];
                let title = ShowTitle {
                    layer,
                    template,
                    data,
                    bindings: Vec::new(),
                    phase: if play {
                        TitlePhase::Playing
                    } else {
                        TitlePhase::Loaded
                    },
                    step: 0,
                };
                match channel.layers.iter().position(|t| t.layer == layer) {
                    Some(at) => channel.layers[at] = title,
                    None => {
                        channel.layers.push(title);
                        channel.layers.sort_by_key(|t| t.layer);
                    }
                }
                let id = channel.id;
                self.titles.calls.remove(&(id, layer));
                if play {
                    *self.titles.plays.entry((id, layer)).or_default() += 1;
                }
            }
            Action::TitlePlay { graphics, layer } => {
                let title = self.title_mut(&graphics, layer)?;
                title.phase = TitlePhase::Playing;
                title.step = 0;
                let id = self.show.graphics[self.find_graphics(&graphics)?].id;
                *self.titles.plays.entry((id, layer)).or_default() += 1;
            }
            Action::TitleNext { graphics, layer } => {
                let title = self.title_mut(&graphics, layer)?;
                if title.phase != TitlePhase::Playing {
                    return Err(ActionError::BadTitle {
                        reason: format!("layer {layer} is not playing"),
                    });
                }
                title.step += 1;
            }
            Action::TitleStop { graphics, layer } => {
                self.title_mut(&graphics, layer)?.phase = TitlePhase::Stopped;
            }
            Action::TitleClear { graphics, layer } => {
                let index = self.find_graphics(&graphics)?;
                let channel = &mut self.show.graphics[index];
                match layer {
                    Some(layer) => {
                        check_layer(layer)?;
                        let at = channel
                            .layers
                            .iter()
                            .position(|t| t.layer == layer)
                            .ok_or_else(|| no_title(layer))?;
                        channel.layers.remove(at);
                        self.titles.calls.remove(&(channel.id, layer));
                    }
                    None => {
                        channel.layers.clear();
                        let id = channel.id;
                        self.titles.calls.retain(|(g, _), _| *g != id);
                    }
                }
            }
            Action::TitleUpdate {
                graphics,
                layer,
                data,
            } => merge(&mut self.title_mut(&graphics, layer)?.data, data),
            Action::TitleAdd {
                graphics,
                layer,
                field,
                amount,
            } => {
                if !amount.is_finite() {
                    return Err(ActionError::BadTitle {
                        reason: "the amount is no number".into(),
                    });
                }
                let title = self.title_mut(&graphics, layer)?;
                let now = title
                    .data
                    .get(&field)
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .unwrap_or(0.0);
                title.data.insert(field, number_text(now + amount));
            }
            Action::TitleCall {
                graphics,
                layer,
                function,
                args,
            } => {
                let valid = !function.is_empty()
                    && function.len() <= CALL_NAME_MAX
                    && function
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
                    && !function.starts_with(|c: char| c.is_ascii_digit());
                if !valid {
                    return Err(ActionError::BadTitle {
                        reason: format!("\"{function}\" is no function name"),
                    });
                }
                let id = self.show.graphics[self.find_graphics(&graphics)?].id;
                self.title_mut(&graphics, layer)?;
                self.titles.call_seq += 1;
                let seq = self.titles.call_seq;
                self.titles.calls.insert(
                    (id, layer),
                    TitleCall {
                        seq,
                        function,
                        args,
                    },
                );
            }
            Action::TitleBind {
                graphics,
                layer,
                field,
                binding,
            } => {
                let binding = match binding {
                    Some(BindingSpec {
                        source,
                        column,
                        format,
                    }) => {
                        let at = self.find_source(&source)?;
                        Some(FieldBinding {
                            field: field.clone(),
                            source: self.show.data_sources[at].id,
                            column,
                            format,
                        })
                    }
                    None => None,
                };
                let title = self.title_mut(&graphics, layer)?;
                title.bindings.retain(|b| b.field != field);
                title.bindings.extend(binding);
            }
            // The service passes the report on and runs its triggers.
            Action::TitleReport {
                graphics, layer, ..
            } => {
                self.title_mut(&graphics, layer)?;
            }
            Action::SetTitleTriggers { graphics, triggers } => {
                super::check_triggers(triggers.iter().map(|t| (t.delay_ms, t.actions.as_slice())))?;
                if let Some(layer) = triggers.iter().find_map(|t| t.layer) {
                    check_layer(layer)?;
                }
                let index = self.find_graphics(&graphics)?;
                self.show.graphics[index].triggers = triggers;
            }
            Action::SaveDataset {
                name,
                template,
                data,
            } => {
                if name.trim().is_empty() {
                    return Err(ActionError::BadTitle {
                        reason: "a data set needs a name".into(),
                    });
                }
                let set = Dataset {
                    name,
                    template,
                    data,
                };
                let sets = &mut self.show.datasets;
                match sets.iter().position(|d| d.name == set.name) {
                    Some(at) => sets[at] = set,
                    None if sets.len() >= DATASETS => {
                        return Err(ActionError::BadTitle {
                            reason: format!("at most {DATASETS} data sets"),
                        })
                    }
                    None => sets.push(set),
                }
            }
            Action::RemoveDataset { name } => {
                let at = self.find_dataset(&name)?;
                self.show.datasets.remove(at);
            }
            Action::ApplyDataset {
                graphics,
                layer,
                name,
            } => {
                let data = self.show.datasets[self.find_dataset(&name)?].data.clone();
                self.title_mut(&graphics, layer)?.data = data;
            }
            Action::AddDataSource { spec, name } => {
                if self.show.data_sources.len() >= DATA_SOURCES {
                    return Err(ActionError::BadDataSource {
                        reason: format!("at most {DATA_SOURCES} data sources"),
                    });
                }
                if spec.path().trim().is_empty() {
                    return Err(ActionError::BadDataSource {
                        reason: "no file given".into(),
                    });
                }
                let name = name.unwrap_or_else(|| file_name(spec.path()));
                self.show.data_sources.push(ShowDataSource {
                    id: InputId::new(),
                    name,
                    spec,
                    row: 1,
                    auto_ms: None,
                    loop_rows: false,
                });
            }
            Action::RemoveDataSource { source } => {
                let at = self.find_source(&source)?;
                // Bound fields keep the value they showed last.
                let values: Vec<_> = self
                    .show
                    .graphics
                    .iter()
                    .map(|g| {
                        g.layers
                            .iter()
                            .map(|t| self.values_of(t))
                            .collect::<Vec<_>>()
                    })
                    .collect();
                let removed = self.show.data_sources.remove(at);
                for (channel, values) in self.show.graphics.iter_mut().zip(values) {
                    for (title, values) in channel.layers.iter_mut().zip(values) {
                        for binding in title.bindings.iter().filter(|b| b.source == removed.id) {
                            if let Some(value) = values.get(&binding.field) {
                                title.data.insert(binding.field.clone(), value.clone());
                            }
                        }
                        title.bindings.retain(|b| b.source != removed.id);
                    }
                }
                self.titles.rows.remove(&removed.id);
                self.titles.due.remove(&removed.id);
            }
            Action::DataRow { source, row } => {
                let at = self.find_source(&source)?;
                let count = self.row_count(at);
                if row == 0 || row > count {
                    return Err(ActionError::BadDataSource {
                        reason: format!("row {row} is outside 1–{count}"),
                    });
                }
                self.show.data_sources[at].row = row;
            }
            Action::DataNext { source } => {
                let at = self.find_source(&source)?;
                let count = self.row_count(at);
                let entry = &mut self.show.data_sources[at];
                if entry.row < count {
                    entry.row += 1;
                } else if entry.loop_rows && count > 0 {
                    entry.row = 1;
                } else {
                    return Err(ActionError::BadDataSource {
                        reason: "already at the last row".into(),
                    });
                }
            }
            Action::DataPrevious { source } => {
                let at = self.find_source(&source)?;
                let entry = &mut self.show.data_sources[at];
                if entry.row <= 1 {
                    return Err(ActionError::BadDataSource {
                        reason: "already at the first row".into(),
                    });
                }
                entry.row -= 1;
            }
            Action::SetDataAuto {
                source,
                interval_ms,
                loop_rows,
            } => {
                if interval_ms.is_some_and(|ms| ms < DATA_AUTO_MIN_MS) {
                    return Err(ActionError::BadDataSource {
                        reason: format!("interval below {DATA_AUTO_MIN_MS} ms"),
                    });
                }
                let at = self.find_source(&source)?;
                let entry = &mut self.show.data_sources[at];
                entry.auto_ms = interval_ms;
                entry.loop_rows = loop_rows;
                self.titles.due.remove(&entry.id);
            }
            _ => unreachable!("not a title action"),
        }
        Ok(Vec::new())
    }

    /// Triggers a template's report fires: (delay, actions).
    pub fn title_triggers(
        &self,
        graphics: InputId,
        layer: u32,
        report: &str,
    ) -> Vec<(u32, Vec<Action>)> {
        self.show
            .graphics
            .iter()
            .filter(|g| g.id == graphics)
            .flat_map(|g| &g.triggers)
            .filter(|t| t.fires_on(layer, report))
            .map(|t| (t.delay_ms, t.actions.clone()))
            .collect()
    }

    /// Id of the graphics channel `graphics` names.
    pub fn graphics_id(&self, graphics: &InputRef) -> Option<InputId> {
        let at = self.find_graphics(graphics).ok()?;
        Some(self.show.graphics[at].id)
    }

    fn find_graphics(&self, graphics: &InputRef) -> Result<usize, ActionError> {
        position(&self.show.graphics, graphics, |g| g.id, |g| &g.name).ok_or_else(|| {
            ActionError::UnknownGraphics {
                reference: reference(graphics),
            }
        })
    }

    fn find_source(&self, source: &InputRef) -> Result<usize, ActionError> {
        position(&self.show.data_sources, source, |s| s.id, |s| &s.name).ok_or_else(|| {
            ActionError::UnknownDataSource {
                reference: reference(source),
            }
        })
    }

    fn find_dataset(&self, name: &str) -> Result<usize, ActionError> {
        self.show
            .datasets
            .iter()
            .position(|d| d.name == name)
            .ok_or_else(|| ActionError::UnknownDataset { name: name.into() })
    }

    fn title_mut(
        &mut self,
        graphics: &InputRef,
        layer: u32,
    ) -> Result<&mut ShowTitle, ActionError> {
        check_layer(layer)?;
        let index = self.find_graphics(graphics)?;
        self.show.graphics[index]
            .layers
            .iter_mut()
            .find(|t| t.layer == layer)
            .ok_or_else(|| no_title(layer))
    }

    fn row_count(&self, at: usize) -> u32 {
        let id = self.show.data_sources[at].id;
        self.titles.rows.get(&id).map_or(0, |r| r.rows.len() as u32)
    }

    /// What the template gets: set fields, bound fields from the selected
    /// rows on top.
    fn values_of(&self, title: &ShowTitle) -> TitleData {
        let mut values = title.data.clone();
        for binding in &title.bindings {
            let Some(source) = self
                .show
                .data_sources
                .iter()
                .find(|s| s.id == binding.source)
            else {
                continue;
            };
            let Some(rows) = self.titles.rows.get(&source.id) else {
                continue;
            };
            let row = (source.row as usize)
                .checked_sub(1)
                .and_then(|i| rows.rows.get(i));
            if let Some(value) = row.and_then(|row| cell(&rows.columns, row, &binding.column)) {
                values.insert(
                    binding.field.clone(),
                    formatted(binding.format.as_deref(), value),
                );
            }
        }
        values
    }

    pub(super) fn graphics_state(&self) -> Vec<Graphics> {
        self.show
            .graphics
            .iter()
            .enumerate()
            .map(|(i, g)| Graphics {
                id: g.id,
                number: i as u32 + 1,
                name: g.name.clone(),
                page: format!("/graphics/{}", i + 1),
                triggers: g.triggers.clone(),
                layers: g
                    .layers
                    .iter()
                    .map(|t| {
                        let template = self.titles.templates.get(&t.template);
                        TitleLayer {
                            layer: t.layer,
                            template: t.template.clone(),
                            url: template.map(|t| t.url.clone()).unwrap_or_default(),
                            data: t.data.clone(),
                            bindings: t.bindings.clone(),
                            values: self.values_of(t),
                            phase: t.phase,
                            played: self
                                .titles
                                .plays
                                .get(&(g.id, t.layer))
                                .copied()
                                .unwrap_or(0),
                            step: t.step,
                            revision: template.map_or(0, |t| t.revision),
                            call: self.titles.calls.get(&(g.id, t.layer)).cloned(),
                        }
                    })
                    .collect(),
            })
            .collect()
    }

    pub(super) fn data_sources_state(&self) -> Vec<DataSource> {
        self.show
            .data_sources
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let rows = self.titles.rows.get(&s.id);
                DataSource {
                    id: s.id,
                    number: i as u32 + 1,
                    name: s.name.clone(),
                    spec: s.spec.clone(),
                    columns: rows.map(|r| r.columns.clone()).unwrap_or_default(),
                    rows: rows.map(|r| r.rows.clone()).unwrap_or_default(),
                    row: s.row,
                    auto_ms: s.auto_ms,
                    loop_rows: s.loop_rows,
                    error: rows.and_then(|r| r.error.clone()),
                }
            })
            .collect()
    }
}

/// A number as a field shows it: whole numbers without a point.
fn number_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// File name without folder and extension.
fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::DataSourceSpec;

    fn switcher() -> Switcher {
        let mut sw = Switcher::new(2);
        sw.set_templates(HashMap::from([(
            "lmix/name-role".to_string(),
            TemplateRef {
                url: "/titles/lmix/name-role/index.html".into(),
                revision: 7,
            },
        )]));
        sw.apply(Action::AddGraphics { name: None }).unwrap();
        sw
    }

    fn data(pairs: &[(&str, &str)]) -> TitleData {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn load(sw: &mut Switcher, layer: u32) {
        sw.apply(Action::TitleLoad {
            graphics: InputRef::Number(1),
            layer,
            template: "lmix/name-role".into(),
            data: data(&[("name", "A")]),
            play: false,
        })
        .unwrap();
    }

    fn layer(sw: &Switcher, layer: u32) -> TitleLayer {
        sw.state().graphics[0]
            .layers
            .iter()
            .find(|t| t.layer == layer)
            .cloned()
            .unwrap()
    }

    #[test]
    fn load_play_next_stop_clear() {
        let mut sw = switcher();
        load(&mut sw, 1);

        let title = layer(&sw, 1);
        assert_eq!(title.phase, TitlePhase::Loaded);
        assert_eq!(title.url, "/titles/lmix/name-role/index.html");
        assert_eq!(title.revision, 7);
        assert_eq!(sw.state().graphics[0].page, "/graphics/1");

        let g = || InputRef::Text("Graphics 1".into());
        assert!(matches!(
            sw.apply(Action::TitleNext {
                graphics: g(),
                layer: 1
            }),
            Err(ActionError::BadTitle { .. })
        ));
        sw.apply(Action::TitlePlay {
            graphics: g(),
            layer: 1,
        })
        .unwrap();
        sw.apply(Action::TitleNext {
            graphics: g(),
            layer: 1,
        })
        .unwrap();
        assert_eq!(layer(&sw, 1).phase, TitlePhase::Playing);
        assert_eq!(layer(&sw, 1).step, 1);

        // Playing again starts over.
        sw.apply(Action::TitlePlay {
            graphics: g(),
            layer: 1,
        })
        .unwrap();
        assert_eq!((layer(&sw, 1).step, layer(&sw, 1).played), (0, 2));

        sw.apply(Action::TitleStop {
            graphics: g(),
            layer: 1,
        })
        .unwrap();
        assert_eq!(layer(&sw, 1).phase, TitlePhase::Stopped);

        sw.apply(Action::TitleClear {
            graphics: g(),
            layer: Some(1),
        })
        .unwrap();
        assert!(sw.state().graphics[0].layers.is_empty());
        assert!(sw
            .apply(Action::TitlePlay {
                graphics: g(),
                layer: 1
            })
            .is_err());
    }

    #[test]
    fn layers_stay_sorted_and_replace() {
        let mut sw = switcher();
        load(&mut sw, 3);
        load(&mut sw, 1);
        load(&mut sw, 3);

        let layers: Vec<u32> = sw.state().graphics[0]
            .layers
            .iter()
            .map(|t| t.layer)
            .collect();
        assert_eq!(layers, [1, 3]);
        assert!(matches!(
            sw.apply(Action::TitleLoad {
                graphics: InputRef::Number(1),
                layer: 9,
                template: "lmix/name-role".into(),
                data: TitleData::new(),
                play: false,
            }),
            Err(ActionError::BadTitle { .. })
        ));
    }

    #[test]
    fn unknown_template_and_graphics_are_refused() {
        let mut sw = switcher();

        assert!(matches!(
            sw.apply(Action::TitleLoad {
                graphics: InputRef::Number(1),
                layer: 1,
                template: "nope".into(),
                data: TitleData::new(),
                play: true,
            }),
            Err(ActionError::UnknownTemplate { .. })
        ));
        assert!(matches!(
            sw.apply(Action::TitlePlay {
                graphics: InputRef::Number(2),
                layer: 1
            }),
            Err(ActionError::UnknownGraphics { .. })
        ));
    }

    #[test]
    fn update_merges_and_empty_clears() {
        let mut sw = switcher();
        load(&mut sw, 1);

        sw.apply(Action::TitleUpdate {
            graphics: InputRef::Number(1),
            layer: 1,
            data: data(&[("role", "R"), ("name", "")]),
        })
        .unwrap();

        assert_eq!(layer(&sw, 1).data, data(&[("role", "R")]));
    }

    #[test]
    fn add_counts_a_number_field() {
        let mut sw = switcher();
        load(&mut sw, 1);
        let add = |sw: &mut Switcher, field: &str, amount: f64| {
            sw.apply(Action::TitleAdd {
                graphics: InputRef::Number(1),
                layer: 1,
                field: field.into(),
                amount,
            })
        };

        add(&mut sw, "home_score", 1.0).unwrap();
        add(&mut sw, "home_score", 2.0).unwrap();
        add(&mut sw, "name", 1.5).unwrap(); // "A" is no number: from 0
        assert_eq!(layer(&sw, 1).data["home_score"], "3");
        assert_eq!(layer(&sw, 1).data["name"], "1.5");
        add(&mut sw, "home_score", -4.0).unwrap();
        assert_eq!(layer(&sw, 1).data["home_score"], "-1");
        assert!(add(&mut sw, "home_score", f64::NAN).is_err());
    }

    #[test]
    fn calls_count_up_and_check_names() {
        let mut sw = switcher();
        load(&mut sw, 1);
        let call = |sw: &mut Switcher, function: &str| {
            sw.apply(Action::TitleCall {
                graphics: InputRef::Number(1),
                layer: 1,
                function: function.into(),
                args: vec!["5".into()],
            })
        };

        call(&mut sw, "start").unwrap();
        call(&mut sw, "clock.adjust").unwrap();
        let last = layer(&sw, 1).call.unwrap();
        assert_eq!((last.seq, last.function.as_str()), (2, "clock.adjust"));

        for bad in ["", "alert(1)", "1go", "a b"] {
            assert!(call(&mut sw, bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn datasets_save_apply_remove() {
        let mut sw = switcher();
        load(&mut sw, 1);
        let save = |sw: &mut Switcher, value: &str| {
            sw.apply(Action::SaveDataset {
                name: "Guest".into(),
                template: None,
                data: data(&[("name", value)]),
            })
            .unwrap();
        };
        save(&mut sw, "B");
        save(&mut sw, "C");
        assert_eq!(sw.state().datasets.len(), 1);

        sw.apply(Action::ApplyDataset {
            graphics: InputRef::Number(1),
            layer: 1,
            name: "Guest".into(),
        })
        .unwrap();
        assert_eq!(layer(&sw, 1).data, data(&[("name", "C")]));

        sw.apply(Action::RemoveDataset {
            name: "Guest".into(),
        })
        .unwrap();
        assert!(matches!(
            sw.apply(Action::RemoveDataset {
                name: "Guest".into()
            }),
            Err(ActionError::UnknownDataset { .. })
        ));
    }

    fn with_source(sw: &mut Switcher) -> InputId {
        sw.apply(Action::AddDataSource {
            spec: DataSourceSpec::Csv {
                path: "/x/guests.csv".into(),
                header: true,
            },
            name: None,
        })
        .unwrap();
        let id = sw.state().data_sources[0].id;
        assert_eq!(sw.unread_sources().len(), 1);
        sw.set_rows(
            id,
            Rows {
                columns: vec!["name".into(), "role".into()],
                rows: vec![
                    vec!["A".into(), "Host".into()],
                    vec!["B".into(), "Guest".into()],
                ],
                error: None,
            },
        );
        assert!(sw.unread_sources().is_empty());
        id
    }

    #[test]
    fn bound_fields_follow_the_row() {
        let mut sw = switcher();
        load(&mut sw, 1);
        with_source(&mut sw);
        assert_eq!(sw.state().data_sources[0].name, "guests");

        sw.apply(Action::TitleBind {
            graphics: InputRef::Number(1),
            layer: 1,
            field: "role".into(),
            binding: Some(BindingSpec {
                source: InputRef::Text("guests".into()),
                column: "role".into(),
                format: Some("– {0}".into()),
            }),
        })
        .unwrap();
        assert_eq!(layer(&sw, 1).values["role"], "– Host");

        sw.apply(Action::DataNext {
            source: InputRef::Number(1),
        })
        .unwrap();
        assert_eq!(layer(&sw, 1).values["role"], "– Guest");
        assert!(sw
            .apply(Action::DataNext {
                source: InputRef::Number(1)
            })
            .is_err());

        // Removing the source keeps what the field showed.
        sw.apply(Action::RemoveDataSource {
            source: InputRef::Number(1),
        })
        .unwrap();
        let title = layer(&sw, 1);
        assert!(title.bindings.is_empty());
        assert_eq!(title.values["role"], "– Guest");
    }

    #[test]
    fn rows_loop_and_auto_advance() {
        let mut sw = switcher();
        let id = with_source(&mut sw);
        sw.apply(Action::SetDataAuto {
            source: InputRef::Number(1),
            interval_ms: Some(1000),
            loop_rows: true,
        })
        .unwrap();

        let start = Instant::now();
        assert!(sw.due_sources(start).is_empty());
        assert!(sw
            .due_sources(start + Duration::from_millis(999))
            .is_empty());
        assert_eq!(sw.due_sources(start + Duration::from_millis(1000)), [id]);

        sw.apply(Action::DataRow {
            source: InputRef::Number(1),
            row: 2,
        })
        .unwrap();
        sw.apply(Action::DataNext {
            source: InputRef::Number(1),
        })
        .unwrap();
        assert_eq!(sw.state().data_sources[0].row, 1);

        assert!(sw
            .apply(Action::SetDataAuto {
                source: InputRef::Number(1),
                interval_ms: Some(10),
                loop_rows: false,
            })
            .is_err());
        assert!(sw
            .apply(Action::DataRow {
                source: InputRef::Number(1),
                row: 3,
            })
            .is_err());
    }

    #[test]
    fn fewer_rows_after_reload_move_the_selection() {
        let mut sw = switcher();
        let id = with_source(&mut sw);
        sw.apply(Action::DataRow {
            source: InputRef::Number(1),
            row: 2,
        })
        .unwrap();

        sw.set_rows(
            id,
            Rows {
                columns: vec!["name".into()],
                rows: vec![vec!["A".into()]],
                error: None,
            },
        );
        assert_eq!(sw.state().data_sources[0].row, 1);

        sw.set_rows(
            id,
            Rows {
                error: Some("gone".into()),
                ..Rows::default()
            },
        );
        let source = &sw.state().data_sources[0];
        assert_eq!((source.row, source.error.as_deref()), (0, Some("gone")));
    }

    #[test]
    fn title_triggers_fire_on_their_report() {
        let mut sw = switcher();
        load(&mut sw, 3);
        let id = sw.state().graphics[0].id;
        let trigger = |report: &str, actions: Vec<Action>| lmix_types::TitleTrigger {
            report: Some(report.into()),
            layer: None,
            delay_ms: 500,
            actions,
        };
        sw.apply(Action::SetTitleTriggers {
            graphics: InputRef::Number(1),
            triggers: vec![trigger("zero", vec![Action::Cut])],
        })
        .unwrap();

        assert_eq!(sw.title_triggers(id, 3, "zero"), [(500, vec![Action::Cut])]);
        assert!(sw.title_triggers(id, 3, "done").is_empty());
        assert_eq!(sw.state().graphics[0].triggers.len(), 1);

        // The same checks as an input's triggers.
        for bad in [
            vec![trigger("zero", Vec::new())],
            vec![trigger(
                "zero",
                vec![Action::CueGo {
                    list: InputRef::Number(1),
                }],
            )],
        ] {
            assert!(matches!(
                sw.apply(Action::SetTitleTriggers {
                    graphics: InputRef::Number(1),
                    triggers: bad
                }),
                Err(ActionError::BadTriggers { .. })
            ));
        }
    }

    #[test]
    fn graphics_full_and_locked_removal() {
        let mut sw = switcher();
        for _ in 1..GRAPHICS {
            sw.apply(Action::AddGraphics { name: None }).unwrap();
        }
        assert!(matches!(
            sw.apply(Action::AddGraphics { name: None }),
            Err(ActionError::GraphicsFull { .. })
        ));

        sw.apply(Action::SetLock { on: true }).unwrap();
        assert!(matches!(
            sw.apply(Action::RemoveGraphics {
                graphics: InputRef::Number(1)
            }),
            Err(ActionError::Locked)
        ));
    }
}
