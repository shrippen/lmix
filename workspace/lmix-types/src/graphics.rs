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
}

/// How an action binds a field.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct BindingSpec {
    pub source: InputRef,
    pub column: String,
    #[serde(default)]
    pub format: Option<String>,
}

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
    /// `next` calls since the last play.
    pub step: u32,
    /// Changes when the template's files change; the page reloads it and
    /// gives it the same data (GFX.14).
    pub revision: u64,
    /// Last function call, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<TitleCall>,
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

/// Where a data source reads from (DAT.1, so far CSV and JSON files).
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
    },
    /// A JSON array of objects; keys become columns.
    Json { path: String },
}

fn yes() -> bool {
    true
}

impl DataSourceSpec {
    pub fn path(&self) -> &str {
        match self {
            Self::Csv { path, .. } | Self::Json { path } => path,
        }
    }
}

/// A data source as clients see it (DAT.5).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DataSource {
    pub id: InputId,
    pub number: u32,
    pub name: String,
    pub spec: DataSourceSpec,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// Selected row, 1-based; 0 while there are no rows.
    pub row: u32,
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
                header: true
            }
        );
    }
}
