//! The settings of this machine as one file (SHW.8): shortcuts for all
//! shows, layout templates, output targets and OSC subscribers, without
//! secrets. To carry them to another machine, keep a copy, or start over.
//!
//! ```json
//! { "format": "lmix-settings", "version": 1, "shortcuts": [...],
//!   "layout_templates": [...], "targets": [...], "feeds": [...] }
//! ```

use lmix_types::{Feed, LayoutTemplate};
use serde::{Deserialize, Serialize};

use crate::show::ShowShortcut;
use crate::targets::TargetConfig;

/// Marker of a settings file.
const FORMAT: &str = "lmix-settings";
/// Its version.
const VERSION: u32 = 1;

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
    let envelope: Envelope = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
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
}
