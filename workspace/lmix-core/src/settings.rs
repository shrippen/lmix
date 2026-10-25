//! The settings of this machine as one file (SHW.8): shortcuts for all
//! shows, layout templates, output targets and OSC subscribers, without
//! secrets. To carry them to another machine, keep a copy, or start over.
//!
//! ```json
//! { "format": "lmix-settings", "version": 2, "shortcuts": [...],
//!   "layout_templates": [...], "targets": [...], "feeds": [...] }
//! ```

use lmix_types::{Feed, LayoutTemplate};
use serde::{Deserialize, Serialize};

use crate::show::ShowShortcut;
use crate::targets::TargetConfig;

/// Marker of a settings file.
const FORMAT: &str = "lmix-settings";
/// Its version. 2: the shortcuts' actions name buses instead of aux bus
/// numbers (show version 3).
const VERSION: u32 = 2;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct SettingsFile {
    #[serde(default)]
    pub shortcuts: Vec<ShowShortcut>,
    #[serde(default)]
    pub layout_templates: Vec<LayoutTemplate>,
    /// Without secrets.
    #[serde(default)]
    pub targets: Vec<TargetConfig>,
    #[serde(default)]
    pub feeds: Vec<Feed>,
    /// Where the zones play (SND.10).
    #[serde(default)]
    pub zone_patches: Vec<lmix_types::ZonePatch>,
    /// Where the stages show (VIS.1).
    #[serde(default)]
    pub stage_patches: Vec<lmix_types::StagePatch>,
}

#[derive(Serialize, Deserialize)]
struct Envelope {
    format: String,
    version: u32,
    #[serde(flatten)]
    settings: SettingsFile,
}

pub fn to_bytes(settings: &SettingsFile) -> Vec<u8> {
    let envelope = Envelope {
        format: FORMAT.into(),
        version: VERSION,
        settings: settings.clone(),
    };
    serde_json::to_vec_pretty(&envelope).expect("settings serialize")
}

pub fn parse(bytes: &[u8]) -> Result<SettingsFile, String> {
    let mut value: serde_json::Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let version = value.get("version").and_then(serde_json::Value::as_u64);
    if version.is_some_and(|v| v < 2) {
        if let Some(shortcuts) = value.get_mut("shortcuts") {
            crate::show::migrate_shortcut_actions(shortcuts);
        }
    }
    let envelope: Envelope = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if envelope.format != FORMAT {
        return Err(format!(
            "not lmix settings (format \"{}\")",
            envelope.format
        ));
    }
    if envelope.version > VERSION {
        return Err(format!(
            "settings version {}, this lmix reads up to {VERSION}",
            envelope.version
        ));
    }
    Ok(envelope.settings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_and_refusals() {
        let settings = SettingsFile::default();
        assert_eq!(parse(&to_bytes(&settings)).unwrap(), settings);
        assert!(parse(br#"{"format":"lmix-show","version":1}"#).is_err());
        assert!(parse(br#"{"format":"lmix-settings","version":9}"#).is_err());
    }

    #[test]
    fn version_1_shortcuts_name_their_buses() {
        // Version 1 numbered aux buses A–G; every bus action, also a fade,
        // names the bus a migrated show calls it.
        let file = br#"{"format":"lmix-settings","version":1,"shortcuts":[
            {"id":"6f1c0f7e-2d1b-4c55-9a43-0d3f2a9b8c11","label":"Sends","actions":[
                {"action":"set_aux_send","input":1,"bus":2,"level":50},
                {"action":"fade_volume","target":{"bus":3},"volume":0,"duration_ms":500},
                {"action":"add_output","spec":{"kind":"srt","uri":"srt://:9000?mode=listener"},"aux":1}
            ]}
        ]}"#;
        let settings = parse(file).expect("version 1 loads");
        let json = serde_json::to_string(&settings.shortcuts).unwrap();
        assert!(json.contains(r#""bus":"Bus B""#), "{json}");
        assert!(json.contains(r#""bus":"Bus C""#), "{json}");
        assert!(json.contains(r#""bus":"Bus A""#), "{json}");
        assert!(!json.contains(r#""aux""#), "{json}");
        // Version 2 is read as it is: a fade's bus number is a position.
        let file = br#"{"format":"lmix-settings","version":2,"shortcuts":[
            {"id":"6f1c0f7e-2d1b-4c55-9a43-0d3f2a9b8c11","label":"Fade","actions":[
                {"action":"fade_volume","target":{"bus":3},"volume":0,"duration_ms":500}
            ]}
        ]}"#;
        let json = serde_json::to_string(&parse(file).unwrap().shortcuts).unwrap();
        assert!(json.contains(r#""bus":3"#), "{json}");
    }
}
