//! The show: what gets saved to and loaded from a `.lmix` file (SHW.1–2).
//!
//! ```json
//! { "format": "lmix-show", "version": 1, "show": { "inputs": [...], ... } }
//! ```
//!
//! Plain, versioned JSON so people and tools can read it. Media are
//! referenced by path, never embedded.

use std::path::Path;

use lmix_types::action::{OVERLAY_CHANNELS, TRANSITION_SLOTS};
use lmix_types::input::AUX_BUSES;
use lmix_types::{
    Action, AuxBus, ChannelStrip, Effect, InputAudio, InputColor, InputId, LayoutBox, Look,
    OutputId, OutputSource, OutputSpec, SignalLoss, SourceSpec, Transition, TransitionSlot,
};
use serde::{Deserialize, Serialize};

/// Marker of a show file.
const FILE_FORMAT: &str = "lmix-show";
/// Current file version; older versions get migrated on load.
const FILE_VERSION: u32 = 1;
/// Default length of a timed transition.
const DEFAULT_TRANSITION_MS: u32 = 500;

/// One input as stored in the show.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowInput {
    pub id: InputId,
    pub name: String,
    pub source: SourceSpec,
    pub slot: usize,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<InputColor>,
    #[serde(default)]
    pub audio: InputAudio,
    #[serde(default)]
    pub signal_loss: SignalLoss,
    #[serde(default)]
    pub strip: ChannelStrip,
    #[serde(default)]
    pub sends: [u8; AUX_BUSES],
    #[serde(default)]
    pub look: Look,
}

/// Everything a show file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Show {
    /// Inputs in input-bar order.
    pub inputs: Vec<ShowInput>,
    pub preview: Option<InputId>,
    pub program: Option<InputId>,
    pub transitions: Vec<Transition>,
    /// Outputs; whether they run is not saved.
    #[serde(default)]
    pub outputs: Vec<ShowOutput>,
    /// What each overlay channel shows, as program is kept too.
    #[serde(default)]
    pub overlays: [Option<InputId>; OVERLAY_CHANNELS as usize],
    #[serde(default)]
    pub aux_buses: [AuxBus; AUX_BUSES],
    #[serde(default)]
    pub layouts: Vec<ShowLayout>,
    #[serde(default)]
    pub shortcuts: Vec<ShowShortcut>,
}

/// One shortcut as saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowShortcut {
    pub id: InputId,
    pub label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
    #[serde(default)]
    pub web: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<InputColor>,
    pub actions: Vec<Action>,
}

/// One layout as saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowLayout {
    pub id: InputId,
    pub name: String,
    /// Engine composition it lives on (0-based).
    pub pip: usize,
    #[serde(default)]
    pub background: Option<InputId>,
    #[serde(default)]
    pub boxes: Vec<LayoutBox>,
}

/// One output as saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowOutput {
    pub id: OutputId,
    pub name: String,
    pub spec: OutputSpec,
    #[serde(default)]
    pub source: OutputSource,
    /// Sound from this aux bus (1–2) instead of the source's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aux: Option<u8>,
}

impl Default for Show {
    fn default() -> Self {
        Self {
            inputs: Vec::new(),
            preview: None,
            program: None,
            transitions: default_transitions(),
            outputs: Vec::new(),
            overlays: Default::default(),
            aux_buses: Default::default(),
            layouts: Vec::new(),
            shortcuts: Vec::new(),
        }
    }
}

/// Buttons 1–4: fade, cut, dip to black, wipe.
fn default_transitions() -> Vec<Transition> {
    let effects = [
        Effect::Fade,
        Effect::Cut,
        Effect::DipToBlack,
        Effect::WipeLeft,
    ];

    (1..=TRANSITION_SLOTS)
        .zip(effects)
        .map(|(slot, effect)| Transition {
            slot: TransitionSlot(slot),
            effect,
            duration_ms: DEFAULT_TRANSITION_MS,
            stinger: None,
        })
        .collect()
}

#[derive(Serialize, Deserialize)]
struct ShowFile {
    format: String,
    version: u32,
    show: Show,
}

#[derive(Debug, thiserror::Error)]
pub enum ShowFileError {
    #[error("cannot read {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("cannot write {path}: {source}")]
    Write {
        path: String,
        source: std::io::Error,
    },
    #[error("{path} is not an lmix show: {reason}")]
    Format { path: String, reason: String },
    #[error("{path} has show version {found}, this lmix reads up to {FILE_VERSION}")]
    TooNew { path: String, found: u32 },
}

/// Write atomically: temp file next to the target, then rename, so a crash
/// never leaves a half-written show (SHW.5).
pub async fn save(show: &Show, path: &Path) -> Result<(), ShowFileError> {
    let file = ShowFile {
        format: FILE_FORMAT.to_string(),
        version: FILE_VERSION,
        show: show.clone(),
    };
    let json = serde_json::to_vec_pretty(&file).expect("show serializes");
    let shown = path.display().to_string();
    let write_err = |source| ShowFileError::Write {
        path: shown.clone(),
        source,
    };

    let tmp = path.with_extension("lmix.tmp");
    tokio::fs::write(&tmp, json).await.map_err(write_err)?;
    tokio::fs::rename(&tmp, path).await.map_err(write_err)
}

pub async fn load(path: &Path) -> Result<Show, ShowFileError> {
    let shown = path.display().to_string();
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|source| ShowFileError::Read {
            path: shown.clone(),
            source,
        })?;

    let format_err = |reason: String| ShowFileError::Format {
        path: shown.clone(),
        reason,
    };
    let file: ShowFile = serde_json::from_slice(&bytes).map_err(|e| format_err(e.to_string()))?;

    if file.format != FILE_FORMAT {
        return Err(format_err(format!("format is \"{}\"", file.format)));
    }
    if file.version > FILE_VERSION {
        return Err(ShowFileError::TooNew {
            path: shown,
            found: file.version,
        });
    }

    Ok(file.show)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::TestPattern;

    fn sample() -> Show {
        let id = InputId::new();
        Show {
            inputs: vec![ShowInput {
                id,
                name: "Bars".into(),
                source: SourceSpec::Pattern {
                    pattern: TestPattern::Bars,
                },
                slot: 0,
                color: Some(InputColor::Blue),
                audio: InputAudio {
                    on: false,
                    follow_video: true,
                    volume: 40,
                },
                signal_loss: SignalLoss::Black,
                strip: ChannelStrip {
                    delay_ms: 40,
                    ..Default::default()
                },
                sends: [70, 0],
                look: Look {
                    saturation: 0.5,
                    ..Default::default()
                },
            }],
            preview: Some(id),
            program: None,
            transitions: default_transitions(),
            overlays: [None, Some(id), None, None],
            shortcuts: vec![ShowShortcut {
                id: InputId::new(),
                label: "Bars".into(),
                key: Some("F1".into()),
                web: true,
                color: None,
                actions: vec![Action::Cut],
            }],
            layouts: vec![ShowLayout {
                id: InputId::new(),
                name: "Interview".into(),
                pip: 0,
                background: Some(id),
                boxes: Vec::new(),
            }],
            aux_buses: [
                AuxBus::default(),
                AuxBus {
                    volume: 60,
                    mute: true,
                },
            ],
            outputs: vec![ShowOutput {
                id: OutputId::new(),
                name: "Stream".into(),
                spec: OutputSpec::Srt {
                    uri: "srt://cdn:9000".into(),
                    video_kbps: 6000,
                },
                source: OutputSource::Input(id),
                aux: Some(2),
            }],
        }
    }

    #[tokio::test]
    async fn keeps_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.lmix");
        let show = sample();

        save(&show, &path).await.unwrap();

        assert_eq!(load(&path).await.unwrap(), show);
    }

    #[tokio::test]
    async fn rejects_foreign_json() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.lmix");
        tokio::fs::write(&path, br#"{"format":"other","version":1,"show":{"inputs":[],"preview":null,"program":null,"transitions":[]}}"#)
            .await
            .unwrap();

        let err = load(&path).await.unwrap_err();

        assert!(matches!(err, ShowFileError::Format { .. }), "{err}");
    }

    #[tokio::test]
    async fn rejects_newer_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.lmix");
        let body = format!(
            r#"{{"format":"lmix-show","version":{},"show":{{"inputs":[],"preview":null,"program":null,"transitions":[]}}}}"#,
            FILE_VERSION + 1
        );
        tokio::fs::write(&path, body).await.unwrap();

        let err = load(&path).await.unwrap_err();

        assert!(matches!(err, ShowFileError::TooNew { .. }), "{err}");
    }

    #[test]
    fn default_has_four_buttons() {
        assert_eq!(Show::default().transitions.len(), TRANSITION_SLOTS as usize);
    }
}
