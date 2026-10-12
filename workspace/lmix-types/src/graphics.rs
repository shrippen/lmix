//! Titles (GFX) and data sources (DAT).
//!
//! A graphics channel is one HTML page lmix-core serves
//! (`/graphics/<number>`): layers of title templates stacked on a
//! transparent ground. Whatever renders it (the engine's browser, or a
//! normal browser as preview) follows the show state and calls the
//! templates' functions. Templates follow CasparCG's open convention:
//! global `play()`, `next()`, `stop()`, `update(data)`.
//!
//! ```text
//!   action ─► lmix-core (state) ─► /api/events ─► graphics page ─► template
//!                  ▲                                   │
//!                  └──────── title_report ◄────────────┘
//! ```

use std::collections::BTreeMap;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::input::{InputId, InputRef};

/// Graphics channels a show can hold.
pub const GRAPHICS: usize = 8;
/// Layers per graphics channel, numbered 1–8 (bottom to top).
pub const GRAPHICS_LAYERS: u32 = 8;
/// Data sources a show can hold.
pub const DATA_SOURCES: usize = 16;
/// Rows kept of a data source; the rest is cut off.
pub const DATA_ROWS: usize = 10_000;
/// Saved data sets per show.
pub const DATASETS: usize = 500;
/// Shortest auto-advance interval of a data source.
pub const DATA_AUTO_MIN_MS: u32 = 500;

/// Field values of a title, by field id. Values are text, as CasparCG
/// passes them; templates read numbers and colours from it.
pub type TitleData = BTreeMap<String, String>;

/// Where a title layer stands in its life (GFX.4, CasparCG's CG
/// commands).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum TitlePhase {
    /// Loaded with its data, not yet played (invisible).
    #[default]
    Loaded,
    /// Played; `next` steps through a multi-step animation.
    Playing,
    /// Told to stop: the template plays its out animation and stays
    /// loaded until cleared or played again.
    Stopped,
}

/// A function call the page makes on a template (CasparCG `CG INVOKE`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TitleCall {
    /// Counts calls on this layer, so the page runs each call once.
    pub seq: u64,
    pub function: String,
    #[serde(default)]
    pub args: Vec<String>,
}

/// A title field fed from a data source column (DAT.4).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct FieldBinding {
    pub field: String,
    pub source: InputId,
    /// Column name, or its 1-based number as text.
    pub column: String,
    /// Pattern with `{0}` for the value, e.g. `"{0} km/h"`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Always this row (1-based) instead of the selected one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row: Option<u32>,
}

/// How an action binds a field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BindingSpec {
    pub source: InputRef,
    pub column: String,
    #[serde(default)]
    pub format: Option<String>,
    /// A fixed row (1-based); without it the selected one (DAT.4).
    #[serde(default)]
    pub row: Option<u32>,
}

/// How a title layer sits in the picture (GFX.18), applied by the
/// graphics page around the template. All at rest by default.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default)]
pub struct LayerTransform {
    /// Shift as fractions of the picture's width and height.
    pub x: f32,
    pub y: f32,
    /// Size, 1 = as the template draws it (0.05–10).
    pub scale: f32,
    /// Turn in the picture plane, degrees.
    pub rotation: f32,
    /// Tilt for perspective, degrees around the horizontal and the
    /// vertical axis (−89–89).
    pub tilt_x: f32,
    pub tilt_y: f32,
    /// Hidden part at each edge, fractions.
    pub crop: crate::layout::BoxCrop,
    /// 0–1.
    pub opacity: f32,
    /// 1 = unchanged (0–4 each).
    pub brightness: f32,
    pub contrast: f32,
    pub saturation: f32,
    /// How the layer mixes with the layers below it.
    pub blend: Blend,
}

impl Default for LayerTransform {
    fn default() -> Self {
        Self {
            x: 0.0,
            y: 0.0,
            scale: 1.0,
            rotation: 0.0,
            tilt_x: 0.0,
            tilt_y: 0.0,
            crop: crate::layout::BoxCrop::default(),
            opacity: 1.0,
            brightness: 1.0,
            contrast: 1.0,
            saturation: 1.0,
            blend: Blend::Normal,
        }
    }
}

impl LayerTransform {
    pub fn is_rest(&self) -> bool {
        *self == Self::default()
    }

    /// The first value out of its range, if any.
    pub fn bad_field(&self) -> Option<&'static str> {
        let tilt = -89.0..=89.0;
        let finite = [
            self.x,
            self.y,
            self.scale,
            self.rotation,
            self.tilt_x,
            self.tilt_y,
            self.opacity,
            self.brightness,
            self.contrast,
            self.saturation,
        ]
        .iter()
        .all(|v| v.is_finite());
        if !finite {
            return Some("a number");
        }
        [
            ("x", (-2.0..=2.0).contains(&self.x)),
            ("y", (-2.0..=2.0).contains(&self.y)),
            ("scale", (0.05..=10.0).contains(&self.scale)),
            ("tilt_x", tilt.contains(&self.tilt_x)),
            ("tilt_y", tilt.contains(&self.tilt_y)),
            ("crop", self.crop.is_zero() || self.crop.is_valid()),
            ("opacity", (0.0..=1.0).contains(&self.opacity)),
            ("brightness", (0.0..=4.0).contains(&self.brightness)),
            ("contrast", (0.0..=4.0).contains(&self.contrast)),
            ("saturation", (0.0..=4.0).contains(&self.saturation)),
        ]
        .into_iter()
        .find_map(|(field, ok)| (!ok).then_some(field))
    }
}

/// CSS blend modes a layer can use.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    Difference,
}

/// Easing of an animated change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Easing {
    Linear,
    EaseIn,
    EaseOut,
    #[default]
    EaseInOut,
}

/// Longest animated transform change.
pub const TRANSFORM_MS_MAX: u32 = 10_000;

/// One layer of a graphics channel.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TitleLayer {
    /// 1–8, higher layers on top.
    pub layer: u32,
    /// Template id from `GET /api/templates`.
    pub template: String,
    /// Address of the template page, as the graphics page loads it.
    pub url: String,
    /// Fields set by actions.
    pub data: TitleData,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<FieldBinding>,
    /// What the template gets: `data` with the bound fields filled in.
    pub values: TitleData,
    pub phase: TitlePhase,
    /// Counts plays since lmix-core started, so the page sees a play of a
    /// title that is playing already.
    #[serde(default)]
    pub played: u64,
    /// When the title was last played (Unix ms): a page that loads it
    /// later lets clocks and countdowns go on from there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub played_at_ms: Option<u64>,
    /// `next` calls since the last play.
    pub step: u32,
    /// Changes when the template's files change; the page reloads it and
    /// gives it the same data (GFX.14).
    pub revision: u64,
    /// Last function call, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<TitleCall>,
    /// Where the layer sits; the last change moves there in
    /// `transform_ms` with `easing`.
    #[serde(default)]
    pub transform: LayerTransform,
    #[serde(default)]
    pub transform_ms: u32,
    #[serde(default)]
    pub easing: Easing,
}

/// Actions run when a template of the channel reports something
/// (GFX.9: a countdown's `zero`; GFX.4: `done` after the out animation).
/// They fire no triggers themselves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TitleTrigger {
    /// The report, e.g. `zero`; any report without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub report: Option<String>,
    /// Only from this layer; any layer without one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layer: Option<u32>,
    /// 0–30 000 ms.
    #[serde(default)]
    pub delay_ms: u32,
    pub actions: Vec<crate::action::Action>,
}

impl TitleTrigger {
    /// Whether a report of `layer` fires it.
    pub fn fires_on(&self, layer: u32, report: &str) -> bool {
        self.report.as_deref().is_none_or(|r| r == report) && self.layer.is_none_or(|l| l == layer)
    }
}

/// A graphics channel as clients see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Graphics {
    pub id: InputId,
    /// 1-based position among the channels.
    pub number: u32,
    pub name: String,
    /// The page to render: `/graphics/<number>` on lmix-core.
    pub page: String,
    /// Loaded layers, lowest first.
    pub layers: Vec<TitleLayer>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<TitleTrigger>,
}

/// A saved set of field values (GFX.8).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Dataset {
    pub name: String,
    /// Template it was saved from, as a hint for lists.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    pub data: TitleData,
}

/// Where a data source reads from (DAT.1, DAT.2): a file, or an
/// `http(s)://` address read again every few seconds (a public Google
/// Sheet as CSV: `…/export?format=csv`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DataSourceSpec {
    /// Comma, semicolon or tab separated; the separator is guessed from
    /// the first line.
    Csv {
        path: String,
        /// First row holds the column names (DAT.3); else they are
        /// numbered.
        #[serde(default = "yes")]
        header: bool,
        /// Rows become columns (DAT.3): the first column holds the
        /// names, each further column is a row.
        #[serde(default, skip_serializing_if = "is_false")]
        transpose: bool,
    },
    /// A spreadsheet (XLSX, XLSM, XLS, ODS): one sheet, by default the
    /// first, read like CSV.
    Sheet {
        path: String,
        /// Name of the sheet; the first if none.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sheet: Option<String>,
        #[serde(default = "yes")]
        header: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        transpose: bool,
    },
    /// A Google Sheet through the Sheets API with the key lmix-core was
    /// started with (`--google-api-key`): public sheets and those shared
    /// by link, read like CSV. `path` is the sheet's address (its tab,
    /// `#gid=`, counts) or id; `range` a sheet name or A1 range, by
    /// default the linked tab or the first sheet.
    GoogleSheet {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        range: Option<String>,
        #[serde(default = "yes")]
        header: bool,
        #[serde(default, skip_serializing_if = "is_false")]
        transpose: bool,
    },
    /// A JSON array of objects; keys become columns.
    Json { path: String },
    /// Plain text in the column `text`: each line that is not empty is a
    /// row (ticker items), or with `paragraphs` each block between empty
    /// lines, line breaks kept (song lyrics, slides; GFX.21).
    Text {
        path: String,
        #[serde(default, skip_serializing_if = "is_false")]
        paragraphs: bool,
    },
    /// An RSS or Atom feed: a row per item, columns `title`, `text`,
    /// `link`, `date`, `author`.
    Feed { path: String },
    /// XML: every element at `rows` (a path like `scores/game`) is a row;
    /// its attributes and child elements are the columns.
    Xml { path: String, rows: String },
}

fn yes() -> bool {
    true
}

fn is_false(v: &bool) -> bool {
    !v
}

impl DataSourceSpec {
    pub fn path(&self) -> &str {
        match self {
            Self::Csv { path, .. }
            | Self::Sheet { path, .. }
            | Self::GoogleSheet { path, .. }
            | Self::Json { path }
            | Self::Text { path, .. }
            | Self::Feed { path }
            | Self::Xml { path, .. } => path,
        }
    }
}

/// A data source as clients see it (DAT.5). The rows themselves are not
/// in the state (it goes to every client on every change): they come from
/// `GET /api/data/<source>`, again when `revision` changes.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DataSource {
    pub id: InputId,
    pub number: u32,
    pub name: String,
    pub spec: DataSourceSpec,
    pub columns: Vec<String>,
    pub row_count: u32,
    /// Selected row, 1-based; 0 while there are no rows.
    pub row: u32,
    /// Cells of the selected row.
    #[serde(default)]
    pub current: Vec<String>,
    /// Changes whenever the rows are read again.
    #[serde(default)]
    pub revision: u64,
    /// Move to the next row this often.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_ms: Option<u32>,
    /// After the last row, start again (with `auto_ms`).
    #[serde(default)]
    pub loop_rows: bool,
    /// Why the file could not be read, if it could not.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// The rows of a data source (`GET /api/data/<source>`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DataTable {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub revision: u64,
}

/// One field of a template, from its manifest.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateField {
    pub id: String,
    /// Label in forms; the id if the manifest has none.
    pub label: String,
    #[serde(default)]
    pub kind: FieldKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<String>,
    /// Values offered for `kind` choice.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
}

/// What a field holds (GFX.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum FieldKind {
    #[default]
    Text,
    /// Several lines.
    LongText,
    Number,
    /// `#rrggbb`.
    Color,
    /// Image address relative to the template, or a URL.
    Image,
    /// `"true"` or `"false"`.
    Toggle,
    /// One of `choices`.
    Choice,
}

/// A template's texts in another language (manifest `i18n`, keyed by
/// language code such as `de`); what is missing stays as written.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TemplateTexts {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Field labels by field id.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub fields: BTreeMap<String, String>,
    /// Shown text of choice values, by value.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub choices: BTreeMap<String, String>,
}

/// How the template wants `update`'s argument (GFX.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DataFormat {
    /// A JSON object of field values, as a string.
    #[default]
    Json,
    /// CasparCG's `<templateData>` XML string.
    Xml,
}

/// A template as `GET /api/templates` lists it (GFX.19).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Template {
    pub id: String,
    pub name: String,
    /// Address of the page on lmix-core.
    pub url: String,
    /// Built into lmix (the Kante set) or from the templates folder.
    pub builtin: bool,
    #[serde(default)]
    pub fields: Vec<TemplateField>,
    #[serde(default)]
    pub data_format: DataFormat,
    /// Steps `next` goes through after play, 0 if none.
    #[serde(default)]
    pub steps: u32,
    /// Functions to call besides the convention's (`TitleCall`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<String>,
    /// Changes with the template's files.
    pub revision: u64,
    /// Address of a still of the template (its `thumbnail.png`), if it
    /// has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumbnail: Option<String>,
    /// Name, labels and choices in other languages.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub i18n: BTreeMap<String, TemplateTexts>,
}

/// Value of `column` in `row` (by name, else 1-based number).
pub fn cell<'a>(columns: &[String], row: &'a [String], column: &str) -> Option<&'a str> {
    let index = columns
        .iter()
        .position(|c| c == column)
        .or_else(|| column.parse::<usize>().ok()?.checked_sub(1))?;
    row.get(index).map(String::as_str)
}

/// `format` with `{0}` replaced by `value`; the value alone without one.
pub fn formatted(format: Option<&str>, value: &str) -> String {
    match format {
        Some(pattern) if pattern.contains("{0}") => pattern.replace("{0}", value),
        _ => value.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_by_name_or_number() {
        let columns = vec!["name".to_string(), "role".to_string()];
        let row = vec!["A".to_string(), "B".to_string()];

        assert_eq!(cell(&columns, &row, "role"), Some("B"));
        assert_eq!(cell(&columns, &row, "1"), Some("A"));
        assert_eq!(cell(&columns, &row, "3"), None);
        assert_eq!(cell(&columns, &row, "0"), None);
        assert_eq!(cell(&columns, &row, "x"), None);
    }

    #[test]
    fn title_triggers_match_report_and_layer() {
        let trigger = |report: Option<&str>, layer| TitleTrigger {
            report: report.map(str::to_string),
            layer,
            delay_ms: 0,
            actions: Vec::new(),
        };

        assert!(trigger(None, None).fires_on(3, "done"));
        assert!(trigger(Some("zero"), None).fires_on(3, "zero"));
        assert!(!trigger(Some("zero"), None).fires_on(3, "done"));
        assert!(trigger(Some("zero"), Some(3)).fires_on(3, "zero"));
        assert!(!trigger(Some("zero"), Some(2)).fires_on(3, "zero"));
    }

    #[test]
    fn transform_ranges() {
        assert_eq!(LayerTransform::default().bad_field(), None);
        let t = |f: fn(&mut LayerTransform)| {
            let mut t = LayerTransform::default();
            f(&mut t);
            t.bad_field()
        };
        assert_eq!(t(|t| t.scale = 0.0), Some("scale"));
        assert_eq!(t(|t| t.opacity = 1.5), Some("opacity"));
        assert_eq!(t(|t| t.tilt_x = 90.0), Some("tilt_x"));
        assert_eq!(t(|t| t.crop.left = 0.99), Some("crop"));
        assert_eq!(t(|t| t.x = f32::NAN), Some("a number"));
        assert_eq!(t(|t| t.rotation = 720.0), None);
    }

    #[test]
    fn format_pattern() {
        assert_eq!(formatted(Some("{0} km/h"), "12"), "12 km/h");
        assert_eq!(formatted(Some("no slot"), "12"), "12");
        assert_eq!(formatted(None, "12"), "12");
    }

    #[test]
    fn csv_header_defaults_to_on() {
        let spec: DataSourceSpec =
            serde_json::from_str(r#"{"kind":"csv","path":"a.csv"}"#).unwrap();

        assert_eq!(
            spec,
            DataSourceSpec::Csv {
                path: "a.csv".into(),
                header: true,
                transpose: false,
            }
        );
    }
}
