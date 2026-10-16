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
    Action, AutoPlay, AuxBus, ChannelStrip, Cue, DataSourceSpec, Dataset, Effect, FieldBinding,
    InputAudio, InputColor, InputId, InputPress, LayerTransform, LayoutBox, Look, MidiBinding,
    OutputId, OutputSource, OutputSpec, OverlayChannel, QuickPlay, SignalLoss, SourceSpec,
    TitleData, TitlePhase, TitleTrigger, Transition, TransitionSlot, Trigger,
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
    /// Play, restart, pause on and off air (INP.11).
    #[serde(default, skip_serializing_if = "AutoPlay::is_off")]
    pub auto_play: AutoPlay,
    /// What a click on it does (INP.8).
    #[serde(
        default = "InputPress::click",
        skip_serializing_if = "InputPress::is_click"
    )]
    pub click: InputPress,
    /// What its GO does (INP.9).
    #[serde(default = "InputPress::go", skip_serializing_if = "InputPress::is_go")]
    pub go: InputPress,
    #[serde(default)]
    pub strip: ChannelStrip,
    #[serde(default)]
    pub sends: [u8; AUX_BUSES],
    #[serde(default)]
    pub look: Look,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<Trigger>,
    /// The camera's PTZ address (PER.2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ptz: Option<lmix_types::PtzSpec>,
}

/// Everything a show file holds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Show {
    /// Inputs in input-bar order.
    pub inputs: Vec<ShowInput>,
    pub preview: Option<InputId>,
    pub program: Option<InputId>,
    pub transitions: Vec<Transition>,
    /// Effect and length of quick play (SWI.8).
    #[serde(default)]
    pub quick_play: QuickPlay,
    /// Outputs; whether they run is not saved.
    #[serde(default)]
    pub outputs: Vec<ShowOutput>,
    /// What each overlay channel shows, as program is kept too.
    #[serde(default)]
    pub overlays: [Option<InputId>; OVERLAY_CHANNELS as usize],
    /// Effect, length, auto off and place per channel (OVL.2).
    #[serde(default, skip_serializing_if = "all_default")]
    pub overlay_channels: [OverlayChannel; OVERLAY_CHANNELS as usize],
    /// What each channel gets with the next take (OVL.3), as preview is
    /// kept too.
    #[serde(default)]
    pub overlay_preview: [Option<InputId>; OVERLAY_CHANNELS as usize],
    #[serde(default)]
    pub aux_buses: [AuxBus; AUX_BUSES],
    #[serde(default)]
    pub layouts: Vec<ShowLayout>,
    #[serde(default)]
    pub shortcuts: Vec<ShowShortcut>,
    #[serde(default)]
    pub cue_lists: Vec<ShowCueList>,
    /// MIDI controller on the T-bar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi_tbar: Option<MidiBinding>,
    /// Graphics channels with their titles, as they stand (like program).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub graphics: Vec<ShowGraphics>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub datasets: Vec<Dataset>,
    /// Data sources; their rows are read from the files again.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub data_sources: Vec<ShowDataSource>,
}

/// One graphics channel as saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowGraphics {
    pub id: InputId,
    pub name: String,
    #[serde(default)]
    pub layers: Vec<ShowTitle>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub triggers: Vec<TitleTrigger>,
}

/// One title layer as saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowTitle {
    pub layer: u32,
    pub template: String,
    #[serde(default)]
    pub data: TitleData,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bindings: Vec<FieldBinding>,
    #[serde(default)]
    pub phase: TitlePhase,
    #[serde(default)]
    pub step: u32,
    /// Where the layer sits (kept when another template is loaded).
    #[serde(default, skip_serializing_if = "LayerTransform::is_rest")]
    pub transform: LayerTransform,
}

/// One data source as saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowDataSource {
    pub id: InputId,
    pub name: String,
    pub spec: DataSourceSpec,
    /// Selected row, 1-based.
    #[serde(default)]
    pub row: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_ms: Option<u32>,
    #[serde(default)]
    pub loop_rows: bool,
}

/// One cue list as saved; where GO stands is not saved.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowCueList {
    pub id: InputId,
    pub name: String,
    pub cues: Vec<Cue>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub midi: Option<MidiBinding>,
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
            quick_play: QuickPlay::default(),
            outputs: Vec::new(),
            overlays: Default::default(),
            overlay_channels: Default::default(),
            overlay_preview: Default::default(),
            aux_buses: Default::default(),
            layouts: Vec::new(),
            shortcuts: Vec::new(),
            cue_lists: Vec::new(),
            midi_tbar: None,
            graphics: Vec::new(),
            datasets: Vec::new(),
            data_sources: Vec::new(),
        }
    }
}

fn all_default(channels: &[OverlayChannel; OVERLAY_CHANNELS as usize]) -> bool {
    channels.iter().all(OverlayChannel::is_default)
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

/// Largest show file read.
const MAX_SHOW_BYTES: u64 = 64 * 1024 * 1024;

/// The show as a file's bytes.
pub fn to_bytes(show: &Show) -> Vec<u8> {
    let file = ShowFile {
        format: FILE_FORMAT.to_string(),
        version: FILE_VERSION,
        show: show.clone(),
    };
    serde_json::to_vec_pretty(&file).expect("show serializes")
}

/// A show from a file's bytes; `name` for errors.
pub fn from_bytes(bytes: &[u8], name: &str) -> Result<Show, ShowFileError> {
    let format_err = |reason: String| ShowFileError::Format {
        path: name.to_string(),
        reason,
    };
    let file: ShowFile = serde_json::from_slice(bytes).map_err(|e| format_err(e.to_string()))?;
    if file.format != FILE_FORMAT {
        return Err(format_err(format!("format is \"{}\"", file.format)));
    }
    if file.version > FILE_VERSION {
        return Err(ShowFileError::TooNew {
            path: name.to_string(),
            found: file.version,
        });
    }
    Ok(file.show)
}

/// Write atomically: temp file next to the target, then rename, so a crash
/// never leaves a half-written show (SHW.5).
pub async fn save(show: &Show, path: &Path) -> Result<(), ShowFileError> {
    let json = to_bytes(show);
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
    // A show is a regular file of reasonable size (not a device, a pipe
    // or something huge someone points the API at).
    let meta = tokio::fs::metadata(path)
        .await
        .map_err(|source| ShowFileError::Read {
            path: shown.clone(),
            source,
        })?;
    if !meta.is_file() || meta.len() > MAX_SHOW_BYTES {
        return Err(ShowFileError::Format {
            path: shown,
            reason: "not a show file (a regular file up to 64 MB)".into(),
        });
    }
    let bytes = tokio::fs::read(path)
        .await
        .map_err(|source| ShowFileError::Read {
            path: shown.clone(),
            source,
        })?;

    from_bytes(&bytes, &shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::TestPattern;
    use lmix_types::TriggerOn;

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
                auto_play: AutoPlay {
                    play: true,
                    restart: false,
                    pause: true,
                },
                click: InputPress::CutDirect,
                go: InputPress::None,
                strip: ChannelStrip {
                    delay_ms: 40,
                    ..Default::default()
                },
                sends: [70, 0],
                look: Look {
                    saturation: 0.5,
                    ..Default::default()
                },
                triggers: vec![Trigger {
                    on: TriggerOn::Program,
                    delay_ms: 2000,
                    before_ms: None,
                    at_ms: None,
                    actions: vec![Action::FadeToBlack { duration_ms: 500 }],
                }],
                ptz: Some(lmix_types::PtzSpec {
                    address: "10.0.0.20:52381".into(),
                    protocol: lmix_types::PtzProtocol::ViscaIp,
                }),
            }],
            preview: Some(id),
            program: None,
            transitions: default_transitions(),
            quick_play: QuickPlay {
                effect: Effect::WipeLeft,
                duration_ms: 1000,
            },
            overlays: [None, Some(id), None, None],
            overlay_channels: [
                OverlayChannel::default(),
                OverlayChannel {
                    effect: Effect::Cut,
                    duration_ms: 0,
                    auto_off_ms: Some(8000),
                    rect: Some(lmix_types::BoxRect {
                        x: 0.7,
                        y: 0.05,
                        w: 0.25,
                        h: 0.25,
                    }),
                },
                OverlayChannel::default(),
                OverlayChannel::default(),
            ],
            overlay_preview: [Some(id), None, None, None],
            cue_lists: vec![ShowCueList {
                id: InputId::new(),
                name: "Opening".into(),
                cues: vec![Cue {
                    label: "Bars".into(),
                    actions: vec![Action::Cut],
                }],
            }],
            shortcuts: vec![ShowShortcut {
                id: InputId::new(),
                label: "Bars".into(),
                key: Some("F1".into()),
                web: true,
                color: None,
                midi: Some(MidiBinding {
                    kind: lmix_types::MidiKind::Note,
                    channel: Some(10),
                    number: 36,
                }),
                actions: vec![Action::Cut],
            }],
            midi_tbar: Some(MidiBinding {
                kind: lmix_types::MidiKind::Cc,
                channel: None,
                number: 7,
            }),
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
            graphics: vec![ShowGraphics {
                id: InputId::new(),
                name: "Titles".into(),
                layers: vec![ShowTitle {
                    layer: 2,
                    template: "lmix/name-role".into(),
                    data: TitleData::from([("name".into(), "N".into())]),
                    bindings: vec![FieldBinding {
                        field: "role".into(),
                        source: id,
                        column: "2".into(),
                        format: Some("({0})".into()),
                        row: Some(3),
                    }],
                    phase: TitlePhase::Playing,
                    step: 1,
                    transform: LayerTransform {
                        x: 0.1,
                        opacity: 0.5,
                        ..Default::default()
                    },
                }],
                triggers: vec![TitleTrigger {
                    report: Some("zero".into()),
                    layer: Some(2),
                    delay_ms: 0,
                    actions: vec![Action::Cut],
                }],
            }],
            datasets: vec![Dataset {
                name: "Guest".into(),
                template: Some("lmix/name-role".into()),
                data: TitleData::from([("name".into(), "N".into())]),
            }],
            data_sources: vec![ShowDataSource {
                id,
                name: "Guests".into(),
                spec: DataSourceSpec::Csv {
                    path: "guests.csv".into(),
                    header: true,
                    transpose: false,
                },
                row: 2,
                auto_ms: Some(5000),
                loop_rows: true,
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
