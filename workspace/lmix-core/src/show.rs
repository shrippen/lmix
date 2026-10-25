//! The show: what gets saved to and loaded from a `.lmix` file (SHW.1–2).
//!
//! ```json
//! { "format": "lmix-show", "version": 2, "show": { "inputs": [...], ... } }
//! ```
//!
//! Plain, versioned JSON so people and tools can read it. Media are
//! referenced by path, never embedded.

use std::path::Path;

use lmix_types::action::{OVERLAY_CHANNELS, TRANSITION_SLOTS};
use std::collections::BTreeMap;

use lmix_types::{
    Action, AtEnd, AutoPlay, BusId, ChannelStrip, Cue, DataSourceSpec, Dataset, Effect, Fader,
    FieldBinding, InputAudio, InputColor, InputId, InputPress, LayerTransform, LayoutBox, Look,
    MidiBinding, OutputId, OutputSource, OutputSpec, OverlayChannel, Phones, PhonesSend, QuickPlay,
    SignalLoss, SourceSpec, TitleData, TitlePhase, TitleTrigger, Transition, TransitionSlot,
    Trigger,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Marker of a show file.
const FILE_FORMAT: &str = "lmix-show";
/// Current file version; older versions get migrated on load.
///
/// 2: cues of the show mode; a cue's `label` became `name` (CUE.21).
/// 3: named buses instead of aux buses A–G, headphones instead of the
/// monitor volume (AUD.2, AUD.4).
const FILE_VERSION: u32 = 3;
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
    /// Loop or hold the last frame at the end (INP.10).
    #[serde(default, skip_serializing_if = "AtEnd::is_loop")]
    pub at_end: AtEnd,
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
    /// What it sends to the headphones (AUD.4).
    #[serde(default, skip_serializing_if = "is_default_phones_send")]
    pub phones: PhonesSend,
    /// Send level per bus; buses missing get nothing.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub sends: BTreeMap<BusId, u8>,
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
    #[serde(default, deserialize_with = "padded")]
    pub overlays: [Option<InputId>; OVERLAY_CHANNELS as usize],
    /// Effect, length, auto off and place per channel (OVL.2).
    #[serde(
        default,
        skip_serializing_if = "all_default",
        deserialize_with = "padded"
    )]
    pub overlay_channels: [OverlayChannel; OVERLAY_CHANNELS as usize],
    /// What each channel gets with the next take (OVL.3), as preview is
    /// kept too.
    #[serde(default, deserialize_with = "padded")]
    pub overlay_preview: [Option<InputId>; OVERLAY_CHANNELS as usize],
    /// Buses besides master and headphones, in their order (AUD.2).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub buses: Vec<ShowBus>,
    /// Master of program sound (AUD.2).
    #[serde(default)]
    pub master: Fader,
    /// Picture size and rate of program (OUT.1).
    #[serde(default)]
    pub format: lmix_types::MasterFormat,
    /// Layout of the multiview (OUT.10).
    #[serde(
        default,
        skip_serializing_if = "lmix_types::MultiviewLayout::is_default"
    )]
    pub multiview: lmix_types::MultiviewLayout,
    /// Sample rate of the audio mixer, Hz (AUD.17).
    #[serde(
        default = "lmix_types::output::default_sample_rate",
        skip_serializing_if = "is_default_rate"
    )]
    pub sample_rate: u32,
    /// The headphones (AUD.4).
    #[serde(default, skip_serializing_if = "is_default_phones")]
    pub phones: Phones,
    /// Delay of new inputs with sound (AUD.6).
    #[serde(default)]
    pub default_delay_ms: u32,
    #[serde(default)]
    pub layouts: Vec<ShowLayout>,
    #[serde(default)]
    pub shortcuts: Vec<ShowShortcut>,
    #[serde(default)]
    pub cue_lists: Vec<ShowCueList>,
    /// Shortest gap between two GOs (RUN.6).
    #[serde(default = "default_go_guard")]
    pub go_guard_ms: u32,
    /// Whether GO also waits for the button to be let go (RUN.6).
    #[serde(default, skip_serializing_if = "is_time")]
    pub go_rearm: lmix_types::cue::GoRearm,
    /// Panic time (SAF.2).
    #[serde(default = "default_panic")]
    pub panic_ms: u32,
    /// The list that runs with every panic and hard stop (SAF.4).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub emergency_list: Option<InputId>,
    /// Sound zones; their devices are this machine's (SND.10).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub zones: Vec<lmix_types::Zone>,
    /// Picture stages; their screens are this machine's (VIS.1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stages: Vec<lmix_types::Stage>,
    /// Script folder whose lists lmix-script keeps loading (SSH.2). Its
    /// lists stay in the show too: without the tool the show still has
    /// its cues, and standby survives a restart (SAF.6).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script_folder: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Where standby stood, so a restart goes on there (SAF.6); nothing
    /// runs after a load.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub standby: Option<lmix_types::CueId>,
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
    /// Sound from this bus instead of the source's own.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bus: Option<BusId>,
}

/// One bus as saved; whether it is listened to is not.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ShowBus {
    pub id: BusId,
    pub name: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pre_fader: bool,
    #[serde(default)]
    pub level: Fader,
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
            buses: Vec::new(),
            master: Default::default(),
            format: Default::default(),
            multiview: Default::default(),
            sample_rate: lmix_types::output::default_sample_rate(),
            phones: Phones::default(),
            default_delay_ms: 0,
            layouts: Vec::new(),
            shortcuts: Vec::new(),
            cue_lists: Vec::new(),
            go_guard_ms: default_go_guard(),
            go_rearm: Default::default(),
            panic_ms: default_panic(),
            emergency_list: None,
            zones: Vec::new(),
            stages: Vec::new(),
            script_folder: None,
            midi_tbar: None,
            graphics: Vec::new(),
            datasets: Vec::new(),
            data_sources: Vec::new(),
        }
    }
}

/// A per-channel array saved with fewer entries (shows from before eight
/// overlay channels, OVL.1): the missing ones get their default.
fn padded<'de, D, T, const N: usize>(deserializer: D) -> Result<[T; N], D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de> + Default,
{
    let saved = Vec::<T>::deserialize(deserializer)?;
    if saved.len() > N {
        return Err(serde::de::Error::invalid_length(
            saved.len(),
            &"at most one per channel",
        ));
    }
    let mut saved = saved.into_iter();
    Ok(std::array::from_fn(|_| saved.next().unwrap_or_default()))
}

fn is_default_phones(phones: &Phones) -> bool {
    *phones == Phones::default()
}

fn is_default_phones_send(send: &PhonesSend) -> bool {
    *send == PhonesSend::default()
}

fn is_default_rate(hz: &u32) -> bool {
    *hz == lmix_types::output::default_sample_rate()
}

fn all_default(channels: &[OverlayChannel; OVERLAY_CHANNELS as usize]) -> bool {
    channels.iter().all(OverlayChannel::is_default)
}

/// Buttons 1–4: fade, cut, dip to black, wipe.
fn is_time(rearm: &lmix_types::cue::GoRearm) -> bool {
    *rearm == lmix_types::cue::GoRearm::Time
}

fn default_go_guard() -> u32 {
    lmix_types::cue::GO_GUARD_DEFAULT_MS
}

fn default_panic() -> u32 {
    lmix_types::cue::PANIC_DEFAULT_MS
}

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

/// A show read from a file, and the older file version it came in.
pub struct Loaded {
    pub show: Show,
    /// The file's version when older than this lmix's: migrated on load,
    /// saved in the new one (SHW.7).
    pub older: Option<u32>,
}

/// A show from a file's bytes; `name` for errors.
pub fn from_bytes(bytes: &[u8], name: &str) -> Result<Show, ShowFileError> {
    read_bytes(bytes, name).map(|loaded| loaded.show)
}

/// A show from a file's bytes, with its older version if any.
pub fn read_bytes(bytes: &[u8], name: &str) -> Result<Loaded, ShowFileError> {
    let format_err = |reason: String| ShowFileError::Format {
        path: name.to_string(),
        reason,
    };
    let mut value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|e| format_err(e.to_string()))?;
    let format = value.get("format").and_then(|f| f.as_str()).unwrap_or("");
    if format != FILE_FORMAT {
        return Err(format_err(format!("format is \"{format}\"")));
    }
    let version = value.get("version").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    if version > FILE_VERSION {
        return Err(ShowFileError::TooNew {
            path: name.to_string(),
            found: version,
        });
    }
    if let Some(show) = value.get_mut("show") {
        if version < 2 {
            migrate_cue_labels(show);
        }
        if version < 3 {
            migrate_aux_buses(show);
        }
    }
    let file: ShowFile = serde_json::from_value(value).map_err(|e| format_err(e.to_string()))?;
    Ok(Loaded {
        show: file.show,
        older: (version < FILE_VERSION).then_some(version),
    })
}

/// Keep the file of an older version beside it before it is written in
/// the new one: `show.lmix` → `show.lmix.v1.bak`, once (SHW.7).
pub async fn backup(path: &Path, version: u32) -> std::io::Result<()> {
    let mut name = path.as_os_str().to_owned();
    name.push(format!(".v{version}.bak"));
    let to = std::path::PathBuf::from(name);
    if tokio::fs::try_exists(&to).await? {
        return Ok(());
    }
    tokio::fs::copy(path, &to).await.map(|_| ())
}

/// Version 1 to 2: a cue's `label` is its `name`; the rest of a cue of
/// then (`actions`) is an actions cue now (CUE.21).
fn migrate_cue_labels(show: &mut serde_json::Value) {
    let Some(lists) = show.get_mut("cue_lists").and_then(|l| l.as_array_mut()) else {
        return;
    };
    let cues = lists
        .iter_mut()
        .filter_map(|l| l.get_mut("cues").and_then(|c| c.as_array_mut()))
        .flatten();
    for cue in cues.filter_map(|c| c.as_object_mut()) {
        if let Some(label) = cue.remove("label") {
            cue.insert("name".into(), label);
        }
    }
}

/// Version 2 to 3: aux buses A–G (1–7 in actions) become named buses
/// ("Bus A", A before the faders), each one the show used: an input sent
/// to it, an output took its sound, a saved action named it, or its master
/// was moved. The monitor volume is the headphones' (AUD.2, AUD.4).
/// Letters of the aux buses of version 2.
const AUX_LETTERS: [&str; 7] = ["A", "B", "C", "D", "E", "F", "G"];

fn migrate_aux_buses(show: &mut serde_json::Value) {
    use serde_json::json;
    let Some(show) = show.as_object_mut() else {
        return;
    };
    let masters: Vec<Value> = match show.remove("aux_buses") {
        Some(Value::Array(masters)) => masters,
        _ => Vec::new(),
    };
    let mut used = [false; 7];
    for (bus, master) in masters.iter().enumerate().take(7) {
        let volume = master.get("volume").and_then(Value::as_u64).unwrap_or(100);
        let mute = master.get("mute").and_then(Value::as_bool).unwrap_or(false);
        used[bus] |= volume != 100 || mute;
    }
    for input in show
        .get("inputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let sends = input.get("sends").and_then(Value::as_array);
        for (bus, level) in sends.into_iter().flatten().enumerate().take(7) {
            used[bus] |= level.as_u64().unwrap_or(0) > 0;
        }
    }
    for output in show
        .get("outputs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(bus) = output.get("aux").and_then(aux_number) {
            used[bus] = true;
        }
    }
    let mut rest = Value::Object(std::mem::take(show));
    actions(&rest, &mut |map| {
        if let Some(bus) = aux_named(map) {
            used[bus] = true;
        }
    });
    let ids: Vec<Option<String>> = used
        .iter()
        .map(|on| on.then(|| BusId::new().to_string()))
        .collect();
    let id = |bus: usize| ids[bus].clone().map(Value::String).unwrap_or(Value::Null);
    rewrite_aux_actions(&mut rest, &id);
    let Value::Object(rest) = rest else {
        return;
    };
    *show = rest;

    let buses: Vec<Value> = (0..7)
        .filter(|bus| used[*bus])
        .map(|bus| {
            let master = masters.get(bus);
            json!({
                "id": id(bus),
                "name": format!("Bus {}", AUX_LETTERS[bus]),
                "pre_fader": bus == 0,
                "level": {
                    "volume": master.and_then(|m| m.get("volume")).cloned().unwrap_or(json!(100)),
                    "mute": master.and_then(|m| m.get("mute")).cloned().unwrap_or(json!(false)),
                },
            })
        })
        .collect();
    show.insert("buses".into(), Value::Array(buses));
    for input in show
        .get_mut("inputs")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        let Some(input) = input.as_object_mut() else {
            continue;
        };
        let sends: serde_json::Map<String, Value> = match input.remove("sends") {
            Some(Value::Array(levels)) => levels
                .iter()
                .enumerate()
                .take(7)
                .filter(|(bus, level)| used[*bus] && level.as_u64().unwrap_or(0) > 0)
                .map(|(bus, level)| (ids[bus].clone().unwrap_or_default(), level.clone()))
                .collect(),
            _ => serde_json::Map::new(),
        };
        input.insert("sends".into(), Value::Object(sends));
    }
    for output in show
        .get_mut("outputs")
        .and_then(Value::as_array_mut)
        .into_iter()
        .flatten()
    {
        if let Some(output) = output.as_object_mut() {
            if let Some(aux) = output.remove("aux") {
                if let Some(bus) = aux_number(&aux) {
                    output.insert("bus".into(), id(bus));
                }
            }
        }
    }
    if let Some(volume) = show.remove("monitor_volume") {
        show.insert("phones".into(), json!({ "volume": volume }));
    }
}

/// Aux bus 1–7 of a version 2 action, as an index.
fn aux_number(v: &Value) -> Option<usize> {
    v.as_u64()
        .filter(|n| (1..=7).contains(n))
        .map(|n| n as usize - 1)
}

/// Every saved action in `value` (shortcuts, cues, triggers, wherever they
/// are, also inside a saved action: a cue that adds a shortcut).
fn actions(value: &Value, f: &mut dyn FnMut(&Map<String, Value>)) {
    match value {
        Value::Object(map) => {
            if map.get("action").is_some_and(Value::is_string) {
                f(map);
            }
            map.values().for_each(|v| actions(v, f));
        }
        Value::Array(items) => items.iter().for_each(|v| actions(v, f)),
        _ => {}
    }
}

fn actions_mut(value: &mut Value, f: &mut dyn FnMut(&mut Map<String, Value>)) {
    match value {
        Value::Object(map) => {
            map.values_mut().for_each(|v| actions_mut(v, f));
            if map.get("action").is_some_and(Value::is_string) {
                f(map);
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| actions_mut(v, f)),
        _ => {}
    }
}

/// The aux bus a version 2 action names, as an index.
fn aux_named(map: &Map<String, Value>) -> Option<usize> {
    match map.get("action")?.as_str()? {
        "set_aux_send" | "set_aux_bus" => map.get("bus").and_then(aux_number),
        "fade_volume" => map.get("target")?.get("bus").and_then(aux_number),
        "add_output" => map.get("aux").and_then(aux_number),
        _ => None,
    }
}

/// The saved actions of version 2 in `value` as version 3 has them: aux
/// bus `index` becomes `bus(index)`. A number that named no bus (0, 8)
/// stays: the action is refused when it runs, as before, and the file
/// still loads.
fn rewrite_aux_actions(value: &mut Value, bus: &dyn Fn(usize) -> Value) {
    use serde_json::json;
    actions_mut(value, &mut |map| {
        let named = aux_named(map);
        match map.get("action").and_then(Value::as_str).unwrap_or("") {
            "set_aux_send" => {
                map.insert("action".into(), json!("set_send"));
                if let Some(index) = named {
                    map.insert("bus".into(), bus(index));
                }
            }
            "set_aux_bus" => {
                map.insert("action".into(), json!("set_bus"));
                if let Some(index) = named {
                    map.insert("bus".into(), bus(index));
                }
            }
            "set_monitor_volume" => {
                let volume = map.remove("volume").unwrap_or(json!(100));
                map.insert("action".into(), json!("set_phones"));
                map.insert("volume".into(), volume);
                map.insert("mute".into(), json!(false));
            }
            "fade_volume" => {
                if let Some(index) = named {
                    map.insert("target".into(), json!({ "bus": bus(index) }));
                }
            }
            "add_output" => {
                if let Some(index) = map.remove("aux").as_ref().and_then(aux_number) {
                    map.insert("bus".into(), bus(index));
                }
            }
            _ => {}
        }
    });
}

/// The shortcuts for all shows (`--shortcuts`) as version 3 has them: their
/// actions name aux bus A–G "Bus A" …, as a migrated show calls it (a
/// name: there is no one show whose ids they could take).
pub fn migrate_shortcuts(value: &mut Value) {
    rewrite_aux_actions(value, &|index| {
        Value::String(format!("Bus {}", AUX_LETTERS[index]))
    });
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
    load_versioned(path).await.map(|loaded| loaded.show)
}

/// A show file, with its older version if any (SHW.7).
pub async fn load_versioned(path: &Path) -> Result<Loaded, ShowFileError> {
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

    read_bytes(&bytes, &shown)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::TestPattern;
    use lmix_types::TriggerOn;

    fn sample() -> Show {
        let id = InputId::new();
        let in_ear = BusId::new();
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
                at_end: AtEnd::Hold,
                click: InputPress::CutDirect,
                go: InputPress::None,
                strip: ChannelStrip {
                    delay_ms: 40,
                    ..Default::default()
                },
                phones: PhonesSend {
                    on: false,
                    level: 60,
                },
                sends: BTreeMap::from([(in_ear, 70)]),
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
            overlays: [None, Some(id), None, None, None, None, None, Some(id)],
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
                OverlayChannel::default(),
                OverlayChannel::default(),
                OverlayChannel::default(),
                OverlayChannel::default(),
            ],
            overlay_preview: [Some(id), None, None, None, None, None, None, None],
            cue_lists: vec![ShowCueList {
                id: InputId::new(),
                name: "Opening".into(),
                cues: vec![Cue::actions("Bars", vec![Action::Cut])],
                source: None,
                standby: None,
            }],
            go_guard_ms: 120,
            go_rearm: lmix_types::cue::GoRearm::Release,
            panic_ms: 2000,
            emergency_list: None,
            zones: Vec::new(),
            stages: Vec::new(),
            script_folder: None,
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
            buses: vec![
                ShowBus {
                    id: in_ear,
                    name: "In-Ear Band".into(),
                    pre_fader: true,
                    level: Fader {
                        volume: 60,
                        mute: true,
                    },
                },
                ShowBus {
                    id: BusId::new(),
                    name: "Stream".into(),
                    pre_fader: false,
                    level: Fader::default(),
                },
            ],
            master: Fader {
                volume: 90,
                mute: false,
            },
            format: lmix_types::MasterFormat {
                width: 1080,
                height: 1920,
                framerate: lmix_types::Framerate::Fps25,
            },
            multiview: lmix_types::MultiviewLayout {
                program: lmix_types::ProgramSide::Left,
                ..Default::default()
            },
            sample_rate: 44_100,
            phones: Phones {
                volume: 70,
                mute: true,
                pre_fader: true,
            },
            default_delay_ms: 40,
            outputs: vec![ShowOutput {
                id: OutputId::new(),
                name: "Stream".into(),
                spec: OutputSpec::Srt {
                    uri: "srt://cdn:9000".into(),
                    video_kbps: 6000,
                    encoding: Default::default(),
                },
                source: OutputSource::Input(id),
                bus: Some(in_ear),
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
    fn aux_buses_become_named_buses() {
        // A version 2 show: sends to A and C, an output on D, a shortcut on
        // E and, inside a saved action, on B, F's master moved; G unused.
        let mut v2 = serde_json::to_value(sample()).unwrap();
        let show = v2.as_object_mut().unwrap();
        show.remove("buses");
        show.remove("phones");
        show.insert("monitor_volume".into(), serde_json::json!(70));
        show.insert(
            "aux_buses".into(),
            serde_json::json!([{}, {}, {}, {}, {}, {"volume": 40, "mute": true}, {}]),
        );
        show["inputs"][0]["sends"] = serde_json::json!([70, 0, 25, 0, 0, 0, 0]);
        show["outputs"][0].as_object_mut().unwrap().remove("bus");
        show["outputs"][0]["aux"] = serde_json::json!(4);
        show["shortcuts"][0]["actions"] = serde_json::json!([
            {"action": "set_aux_bus", "bus": 5, "volume": 80, "mute": false},
            {"action": "set_aux_send", "input": 1, "bus": 1, "level": 50},
            {"action": "fade_volume", "target": {"bus": 3}, "volume": 0, "duration_ms": 500},
            {"action": "set_monitor_volume", "volume": 30},
            {"action": "set_aux_send", "input": 1, "bus": 9, "level": 50},
            {"action": "add_shortcut", "shortcut": {"label": "B", "actions": [
                {"action": "set_aux_send", "input": 1, "bus": 2, "level": 10},
            ]}},
        ]);
        let file = serde_json::json!({ "format": FILE_FORMAT, "version": 2, "show": v2 });
        let loaded = read_bytes(&serde_json::to_vec(&file).unwrap(), "v2").unwrap();
        assert_eq!(loaded.older, Some(2));
        let show = loaded.show;

        let names: Vec<&str> = show.buses.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(
            names,
            ["Bus A", "Bus B", "Bus C", "Bus D", "Bus E", "Bus F"]
        );
        let bus = |name: &str| show.buses.iter().find(|b| b.name == name).unwrap();
        assert!(bus("Bus A").pre_fader && !bus("Bus C").pre_fader);
        assert_eq!(
            bus("Bus F").level,
            Fader {
                volume: 40,
                mute: true
            }
        );
        assert_eq!(
            show.inputs[0].sends,
            BTreeMap::from([(bus("Bus A").id, 70), (bus("Bus C").id, 25)])
        );
        assert_eq!(show.outputs[0].bus, Some(bus("Bus D").id));
        assert_eq!(show.phones.volume, 70);
        let text = |b: &str| lmix_types::BusRef::Text(bus(b).id.to_string());
        assert_eq!(
            show.shortcuts[0].actions[..5],
            [
                Action::SetBus {
                    bus: text("Bus E"),
                    volume: 80,
                    mute: false
                },
                Action::SetSend {
                    input: lmix_types::InputRef::Number(1),
                    bus: text("Bus A"),
                    level: 50
                },
                Action::FadeVolume {
                    target: lmix_types::action::VolumeTarget::Bus(text("Bus C")),
                    volume: 0,
                    duration_ms: 500
                },
                Action::SetPhones {
                    volume: 30,
                    mute: false
                },
                // No bus 9: refused when it runs, the show still loads.
                Action::SetSend {
                    input: lmix_types::InputRef::Number(1),
                    bus: lmix_types::BusRef::Number(9),
                    level: 50
                },
            ]
        );
        let Action::AddShortcut { shortcut } = &show.shortcuts[0].actions[5] else {
            panic!("{:?}", show.shortcuts[0].actions[5]);
        };
        assert_eq!(
            shortcut.actions,
            [Action::SetSend {
                input: lmix_types::InputRef::Number(1),
                bus: text("Bus B"),
                level: 10
            }]
        );
    }

    #[test]
    fn shows_with_fewer_channels_still_load() {
        let mut saved = serde_json::to_value(sample()).unwrap();
        for key in ["overlays", "overlay_channels", "overlay_preview"] {
            saved[key].as_array_mut().unwrap().truncate(4);
        }
        let show: Show = serde_json::from_value(saved).unwrap();
        let id = show.inputs[0].id;
        assert_eq!(
            show.overlays,
            [None, Some(id), None, None, None, None, None, None]
        );
        assert_eq!(show.overlay_channels[1].auto_off_ms, Some(8000));
        assert_eq!(show.overlay_channels[7], OverlayChannel::default());
        assert_eq!(show.overlay_preview[0], Some(id));

        let mut saved = serde_json::to_value(sample()).unwrap();
        saved["overlays"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::Value::Null);
        assert!(
            serde_json::from_value::<Show>(saved).is_err(),
            "nine channels"
        );
    }

    #[test]
    fn version_1_cues_become_actions_cues() {
        let old = br#"{"format":"lmix-show","version":1,"show":{"inputs":[],"preview":null,"program":null,"transitions":[],
            "cue_lists":[{"id":"7d0c3c2e-2f43-4a39-9d5c-6c8f1f0b6a11","name":"Run","cues":[{"label":"Bars","actions":[{"action":"cut"}]}]}]}}"#;

        let show = from_bytes(old, "old.lmix").unwrap();

        let cue = &show.cue_lists[0].cues[0];
        assert_eq!(cue.name, "Bars");
        assert_eq!(cue.kind, lmix_types::CueKind::Actions);
        assert_eq!(cue.actions, vec![Action::Cut]);
        assert_eq!(cue.follow, lmix_types::Follow::None, "no times, no follow");
        assert_eq!(show.go_guard_ms, lmix_types::cue::GO_GUARD_DEFAULT_MS);
    }

    #[test]
    fn default_has_four_buttons() {
        assert_eq!(Show::default().transitions.len(), TRANSITION_SLOTS as usize);
    }
}
