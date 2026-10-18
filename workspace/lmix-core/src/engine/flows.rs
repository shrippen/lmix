//! strom flows of a show.
//!
//! ```text
//!   slot flow 0: source ─► [caps] ─► tap ─► inter_output "out" ┐
//!                     └─ sound ─────────► inter_output "out_audio" ┐
//!   slot flow 1: …                                                 ├─► mixer flow:
//!   …                                                          ┘   inter_input × N ─► vision mixer
//!                                               inter_input × N ─► audio mixer ─► vm PGM audio
//!                       stinger player ─► vm stinger_in, its sound ─► audio mixer
//! ```
//!
//! Flow ids are fixed per slot (UUID v5), so the `inter` channel name
//! `strom_<flow>_<block>` never changes: swapping a source rebuilds only
//! its slot flow, the mixer keeps running (INP.1, ROADMAP 2b.1).

use strom_types::block::{BlockInstance, Position};
use strom_types::element::{Element, Link};
use strom_types::flow::GStreamerClockType;
use strom_types::stinger::{
    ENABLE_STINGER_PROPERTY, STINGER_INPUT_PAD, STINGER_MODE_PROPERTY, STINGER_PREROLL_PROPERTY,
};
use strom_types::{Flow, FlowId, PropertyValue};
use uuid::Uuid;

use lmix_types::action::OVERLAY_CHANNELS;
use lmix_types::input::{color_rgb, AUX_BUSES, AUX_PRE_FADER};
use lmix_types::layout::LAYOUTS;
use lmix_types::{
    DisplayMode, Framerate, MasterFormat, MultiviewLayout, MultiviewMeters, OutputId, OutputSpec,
    ProgramSide, RateMode, SourceSpec, VideoCodec, VideoEncoding,
};

use super::LivePicture;
use crate::switcher::Feed;

/// Namespace of all lmix flow ids.
pub(crate) const FLOW_NAMESPACE: Uuid = Uuid::from_u128(0x6c6d_6978_2d63_6f72_652d_666c_6f77_7321);

const MEDIA_PLAYER_BLOCK: &str = "builtin.media_player";
const HTML_INPUT_BLOCK: &str = "builtin.html_input";
const SRT_INPUT_BLOCK: &str = "builtin.mpegtssrt_input";
const LOCAL_INPUT_BLOCK: &str = "builtin.local_input";
const NDI_INPUT_BLOCK: &str = "builtin.ndi_input";
const NDI_OUTPUT_BLOCK: &str = "builtin.ndi_output";
const SRT_OUTPUT_BLOCK: &str = "builtin.mpegtssrt_output";
/// Fragmented files: readable while they grow and after a crash, and a
/// track that stops does not hold up the others (DECISIONS 18.10., REC).
const RECORDER_BLOCK: &str = "builtin.liverecorder";
/// MKV before GStreamer 1.26, where matroskamux is not live yet: the
/// live recorder refuses it, this one writes MKV that stays readable too.
const MKV_RECORDER_BLOCK: &str = "builtin.recorder";
const RTMP_OUTPUT_BLOCK: &str = "builtin.rtmp_output";
const VIDEO_ENCODER_BLOCK: &str = "builtin.videoenc";
const WHIP_INPUT_BLOCK: &str = "builtin.whip_input";
const INTER_OUTPUT_BLOCK: &str = "builtin.inter_output";
const INTER_INPUT_BLOCK: &str = "builtin.inter_input";
const VISION_MIXER_BLOCK: &str = "builtin.vision_mixer";
const THUMBNAIL_BLOCK: &str = "builtin.thumbnail";
const AUDIO_MIXER_BLOCK: &str = "builtin.mixer";

/// Block id of the media player in file, URL and playlist slots.
pub const PLAYER: &str = "player";
/// Alpha of `Color` sources: fully opaque.
const OPAQUE: u32 = 0xff00_0000;
/// Block id of every slot flow's inter output.
const SLOT_OUT: &str = "out";
/// Block id of a slot flow's inter output for sound.
const SLOT_AUDIO_OUT: &str = "out_audio";
/// Headphone output of the monitor bus (`MonitorOut::Auto`).
const MONITOR_SINK: &str = "monitor_sink";
/// Block id of the audio mixer in the mixer flow: one channel per slot.
pub const AUDIO_MIXER: &str = "mix";
/// Mixer flow name; a new layout gets a new name, so `start` rebuilds a
/// running mixer of an older lmix instead of reusing it.
pub const MIXER_NAME: &str = "lmix mixer 10";
/// Block id of the vision mixer in the mixer flow.
pub const MIXER_BLOCK: &str = "vm";
/// Block id of the stinger clip player in the mixer flow: strom's stinger
/// source, its playlist the stinger library (DECISIONS 18.10., STG).
pub const STINGER_PLAYER: &str = "stinger";
/// Delay from a stinger take to the clip's first frame on air (ms); the
/// switcher times the cut from the take with it. strom's default.
pub const STINGER_PREROLL_MS: u32 = 80;
/// How long the switcher's lock waits for strom's end of a take after
/// the clip at most (ms), so a lost event does not lock it for good.
/// strom gives up on a stalled mixer 1 s + 5 s after the clip, but starts
/// that timer only after cue, analysis and play_at, and cues the clip
/// again before it frees the mixer: 2 s of margin on top.
pub const STINGER_FINISH_LIMIT_MS: u32 = 8000;
/// Thumbnail tap on the program output: GUI monitor and picture checks.
pub const PGM_TAP: &str = "tap_pgm";
/// Program picture and sound for the output flows (`inter`).
pub const PGM_OUT: &str = "pgm_out";
const PGM_AUDIO_OUT: &str = "pgm_audio_out";
/// The multiview for the outputs (OUT.2).
const MV_OUT: &str = "mv_out";
/// Outputs buffer a little more than the mixer: they are not on the
/// switching path, an encoder hiccup must not drop frames.
const OUTPUT_BUFFER_MS: &str = "500";
/// Recording files: `<folder>/lmix-<output>_<date>_<time>_00001.mkv`, …
/// The output's id in the name: two recordings into one folder that start
/// in the same second (a restore) would otherwise write the same file.
const RECORDING_PREFIX: &str = "lmix";
/// Thumbnail tap in every slot flow: input bar pictures and preview monitor.
pub const SLOT_TAP: &str = "tap";
/// Subscriber buffer of a live picture: as short as the mixer's, it
/// shows now or nothing.
const LIVE_BUFFER_MS: &str = "100";
/// Size of an input's live picture; program goes at full size.
const LIVE_INPUT_CAPS: &str = "video/x-raw,width=640,height=360";

/// Subscriber buffer of the mixer's inter inputs: low latency, some jitter
/// headroom (strom offers 100–2000 ms).
const INTER_BUFFER_MS: &str = "100";

/// Test tone of patterns: a different pitch per slot, quiet.
const TONE_BASE_HZ: f64 = 440.0;
const TONE_STEP: f64 = 1.25;
const TONE_VOLUME: f64 = 0.1;
/// One audio format for every slot. Slots stay at 48 kHz whatever the
/// mixer runs at (AUD.17): its channel strips resample.
const AUDIO_CAPS: &str = "audio/x-raw,format=F32LE,layout=interleaved,rate=48000,channels=2";

/// Format test patterns are rendered in; real sources keep theirs.
const PATTERN_WIDTH: u32 = 1280;
const PATTERN_HEIGHT: u32 = 720;
const PATTERN_FPS: u32 = 30;
const PATTERN_FORMAT: &str = "NV12";
/// Still images whose file type can carry alpha go as BGRA, so a PNG logo
/// lets program through where it is clear (SRC.4); the others (JPEG, BMP)
/// as the smaller NV12. The mixers take both. By file type, not by
/// negotiation: `videoconvert` would pick BGRA for a JPEG too.
const ALPHA_IMAGES: [&str; 4] = ["png", "webp", "tif", "tiff"];
const STILL_ALPHA_FORMAT: &str = "BGRA";

/// Where the monitor bus (solo listening, headphones) goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum MonitorOut {
    /// Nowhere: levels only (headless engine, tests).
    None,
    /// The engine machine's default audio output (`autoaudiosink`).
    Auto,
}

/// Vision mixer compositor (`compositor_preference`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum MixerBackend {
    /// NVIDIA CUDA compositor.
    Cuda,
    /// OpenGL compositor (AMD, Intel).
    Gl,
    /// CPU compositor: emergency and VM tests only.
    Cpu,
}

impl MixerBackend {
    fn property(self) -> &'static str {
        match self {
            Self::Cuda => "cuda",
            Self::Gl => "gpu",
            Self::Cpu => "cpu",
        }
    }
}

pub fn mixer_id() -> FlowId {
    Uuid::new_v5(&FLOW_NAMESPACE, b"mixer")
}

pub fn slot_id(slot: usize) -> FlowId {
    Uuid::new_v5(&FLOW_NAMESPACE, format!("slot-{slot}").as_bytes())
}

/// `inter` channel a slot flow publishes on (see strom `inter.rs`).
pub fn slot_channel(slot: usize) -> String {
    format!("strom_{}_{}", slot_id(slot), SLOT_OUT)
}

fn slot_audio_channel(slot: usize) -> String {
    format!("strom_{}_{}", slot_id(slot), SLOT_AUDIO_OUT)
}

/// Audio mixer channel of a slot (strom counts from 1).
pub fn audio_channel(slot: usize) -> usize {
    slot + 1
}

/// Mixer input that always shows black: program when lmix has nothing on
/// program (the engine always has some input there).
pub fn black_input(slots: usize) -> usize {
    slots
}

/// Mixer: one inter input per slot, plus black, into the vision mixer.
pub fn mixer_flow(
    slots: usize,
    backend: MixerBackend,
    monitor: MonitorOut,
    format: MasterFormat,
    multiview: MultiviewLayout,
    sample_rate: u32,
) -> Flow {
    let mut flow = Flow::new(MIXER_NAME);
    flow.id = mixer_id();
    share_timing(&mut flow);

    for slot in 0..slots {
        let input = format!("in{slot}");
        flow.blocks.push(block(
            &input,
            INTER_INPUT_BLOCK,
            vec![
                ("channel", PropertyValue::String(slot_channel(slot))),
                ("max_time", PropertyValue::String(INTER_BUFFER_MS.into())),
            ],
        ));
        flow.links.push(link(
            format!("{input}:src"),
            format!("{MIXER_BLOCK}:video_in_{slot}"),
        ));
    }

    // DSK layers, fed over `inter`: one per overlay. The channel is
    // pointed at a slot when a layer goes on; until then it waits on a
    // channel nobody publishes.
    for layer in 0..DSK_LAYERS {
        let input = layer_input(layer);
        flow.blocks.push(block(
            &input,
            INTER_INPUT_BLOCK,
            vec![
                ("channel", PropertyValue::String(OVERLAY_IDLE.into())),
                ("max_time", PropertyValue::String(INTER_BUFFER_MS.into())),
            ],
        ));
        flow.links.push(link(
            format!("{input}:src"),
            format!("{MIXER_BLOCK}:dsk_in_{layer}"),
        ));
    }

    let black = black_input(slots);
    flow.elements.push(element(
        "black",
        "videotestsrc",
        vec![
            ("pattern", PropertyValue::String("black".into())),
            ("is-live", PropertyValue::Bool(true)),
        ],
    ));
    flow.elements.push(element(
        "black_caps",
        "capsfilter",
        vec![("caps", PropertyValue::String(pattern_caps()))],
    ));
    flow.links.push(link("black".into(), "black_caps".into()));
    flow.links.push(link(
        "black_caps".into(),
        format!("{MIXER_BLOCK}:video_in_{black}"),
    ));

    flow.blocks.push(block(
        MIXER_BLOCK,
        VISION_MIXER_BLOCK,
        vec![
            ("num_inputs", PropertyValue::Int(black as i64 + 1)),
            // Program size and rate (OUT.1); the multiview keeps its own.
            ("pgm_resolution", PropertyValue::String(format.resolution())),
            (
                "swap_pvw_pgm",
                PropertyValue::Bool(multiview.program == ProgramSide::Left),
            ),
            (
                "show_vu_meters",
                PropertyValue::Bool(multiview.meters == MultiviewMeters::Shown),
            ),
            (
                "pgm_framerate",
                PropertyValue::String(format.framerate.fraction().into()),
            ),
            (
                "compositor_preference",
                PropertyValue::String(backend.property().into()),
            ),
            (
                "num_dsk_inputs",
                PropertyValue::String(DSK_LAYERS.to_string()),
            ),
            // One composition per layout.
            ("num_pips", PropertyValue::String(LAYOUTS.to_string())),
            // Base time 0 (share_timing): output starts at the first frame.
            (
                "start_time_selection",
                PropertyValue::String("first".into()),
            ),
            // strom draws the stinger over program, under the DSK layers.
            (ENABLE_STINGER_PROPERTY, PropertyValue::Bool(true)),
            (
                STINGER_PREROLL_PROPERTY,
                PropertyValue::UInt(u64::from(STINGER_PREROLL_MS)),
            ),
            // GL memory is bound to the mixer's GL context, which the
            // output flows do not share: program leaves in system memory.
            // CUDA program stays on the GPU for the encoders; the flows
            // that read it on the CPU download it themselves.
            (
                "gl_download",
                PropertyValue::Bool(backend == MixerBackend::Gl),
            ),
        ],
    ));

    add_stinger_player(&mut flow, backend);
    add_audio_mixer(&mut flow, slots, monitor, sample_rate);

    flow.blocks.push(block(PGM_TAP, THUMBNAIL_BLOCK, vec![]));
    flow.links.push(link(
        format!("{MIXER_BLOCK}:pgm_out"),
        format!("{PGM_TAP}:video_in"),
    ));
    // Program for the outputs, each in its own flow (crash isolation): the
    // inter sink also consumes program when no output listens, so the tap
    // branch never stalls.
    flow.blocks.push(block(PGM_OUT, INTER_OUTPUT_BLOCK, vec![]));
    flow.links.push(link(
        format!("{PGM_TAP}:video_out"),
        format!("{PGM_OUT}:sink"),
    ));
    // The multiview is rendered anyway (strom ends an unlinked pad in a
    // fakesink); this only hands it to outputs that ask (OUT.2).
    flow.blocks.push(block(MV_OUT, INTER_OUTPUT_BLOCK, vec![]));
    flow.links.push(link(
        format!("{MIXER_BLOCK}:multiview_out"),
        format!("{MV_OUT}:sink"),
    ));
    flow.blocks
        .push(block(PGM_AUDIO_OUT, INTER_OUTPUT_BLOCK, vec![]));
    flow.links.push(link(
        format!("{AUDIO_MIXER}:main_out"),
        format!("{PGM_AUDIO_OUT}:sink"),
    ));
    flow
}

/// DSK layers in the mixer: one per overlay.
const DSK_LAYERS: usize = OVERLAY_CHANNELS as usize;

/// DSK layer of overlay `channel` (0-based).
pub fn overlay_layer(channel: usize) -> usize {
    channel
}

/// strom's stinger source: a media player in stinger mode on the mixer's
/// stinger input, its sound on the audio mixer channel after the slots.
/// The engine fills its playlist (`Command::StingerLibrary`). Like the
/// slot players, it hands frames over in system memory unless the mixer
/// is CUDA's.
fn add_stinger_player(flow: &mut Flow, backend: MixerBackend) {
    flow.blocks.push(block(
        STINGER_PLAYER,
        MEDIA_PLAYER_BLOCK,
        vec![
            (STINGER_MODE_PROPERTY, PropertyValue::Bool(true)),
            (
                "system_memory",
                PropertyValue::Bool(backend != MixerBackend::Cuda),
            ),
        ],
    ));
    flow.links.push(link(
        format!("{STINGER_PLAYER}:video_out"),
        format!("{MIXER_BLOCK}:{STINGER_INPUT_PAD}"),
    ));
}

/// Audio mixer channel of the stinger clip's sound: after the slots'.
pub fn stinger_channel(slots: usize) -> usize {
    audio_channel(slots)
}

/// Program picture size: strom's default `pgm_resolution`, which the
/// mixer flow keeps.
pub const PROGRAM_SIZE: (u32, u32) = (1920, 1080);

/// The mixer's program format as built, read from its flow; `None` for
/// a mixer that names none (built before OUT.1: the strom default).
pub fn format_of(flow: &Flow) -> Option<MasterFormat> {
    let mixer = flow.blocks.iter().find(|b| b.id == MIXER_BLOCK)?;
    let text = |key: &str| match mixer.properties.get(key) {
        Some(PropertyValue::String(s)) => Some(s.clone()),
        _ => None,
    };
    let resolution = text("pgm_resolution")?;
    let (width, height) = resolution.split_once('x')?;
    let framerate =
        serde_json::from_value(serde_json::Value::String(text("pgm_framerate")?)).ok()?;
    Some(MasterFormat {
        width: width.parse().ok()?,
        height: height.parse().ok()?,
        framerate,
    })
}

/// The sample rate the audio mixer of a mixer flow was built with.
pub fn sample_rate_of(flow: &Flow) -> Option<u32> {
    let mix = flow.blocks.iter().find(|b| b.id == AUDIO_MIXER)?;
    match mix.properties.get("sample_rate") {
        Some(PropertyValue::UInt(hz)) => u32::try_from(*hz).ok(),
        _ => None,
    }
}

/// The multiview layout a mixer flow was built with; the engine's
/// defaults where a property is missing.
pub fn multiview_of(flow: &Flow) -> MultiviewLayout {
    let flag = |key: &str| {
        flow.blocks
            .iter()
            .find(|b| b.id == MIXER_BLOCK)
            .and_then(|b| match b.properties.get(key) {
                Some(PropertyValue::Bool(on)) => Some(*on),
                _ => None,
            })
    };
    MultiviewLayout {
        program: if flag("swap_pvw_pgm") == Some(true) {
            ProgramSide::Left
        } else {
            ProgramSide::Right
        },
        meters: if flag("show_vu_meters") == Some(false) {
            MultiviewMeters::Hidden
        } else {
            MultiviewMeters::Shown
        },
    }
}

/// Pad of DSK `layer` (0-based) on the program compositor: after the
/// mixer's inputs, which end with black.
pub fn dsk_pad(black: usize, layer: usize) -> String {
    format!("sink_{}", black + 1 + layer)
}

/// Block id of the `inter` input feeding DSK `layer` (0-based).
pub fn layer_input(layer: usize) -> String {
    format!("dsk{layer}")
}

/// `inter` channel of an idle overlay: nobody publishes it.
const OVERLAY_IDLE: &str = "lmix_overlay_idle";

/// `inter` channels of program in the mixer flow.
fn pgm_channel(block: &str) -> String {
    format!("strom_{}_{block}", mixer_id())
}

/// File name prefix of output `id`'s recordings: `lmix-` and the first
/// part of its id.
fn recording_prefix(id: OutputId) -> String {
    let id = id.to_string();
    format!("{RECORDING_PREFIX}-{}", &id[..8])
}

/// Whether strom's GStreamer has a live MKV muxer (1.26 and later).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiveMkv {
    Yes,
    No,
}

impl LiveMkv {
    /// From strom's GStreamer version, e.g. "1.24.2"; unknown counts as old.
    pub fn for_gstreamer(version: &str) -> Self {
        let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
        match (parts.next().flatten(), parts.next().flatten()) {
            (Some(major), Some(minor)) if (major, minor) >= (1, 26) => Self::Yes,
            _ => Self::No,
        }
    }
}

pub fn output_flow_id(id: OutputId) -> FlowId {
    Uuid::new_v5(&FLOW_NAMESPACE, format!("output-{id}").as_bytes())
}

/// One output: program picture and sound from the mixer, encoded where
/// the target needs it.
///
/// ```text
///   SRT:    pgm ─► videoenc ─► srt:video_in      pgm_audio ─► srt:audio_in_0 (AAC inside)
///   Record: pgm ─► videoenc ─► rec:video_in      pgm_audio ─► aac ─► rec:audio_in_0
///   NDI:    pgm ─────────────► ndi:video_in      pgm_audio ─► ndi:audio_in
/// ```
pub fn output_flow(
    id: OutputId,
    spec: &OutputSpec,
    feed: Feed,
    aux: Option<usize>,
    backend: MixerBackend,
    framerate: Framerate,
    live_mkv: LiveMkv,
) -> Flow {
    let mut flow = Flow::new(format!("lmix output {id}"));
    flow.id = output_flow_id(id);
    share_timing(&mut flow);
    // Stopping drains with EOS: recordings get their trailer (MP4 needs it).
    flow.properties.eos_on_stop = Some(true);

    // Program, or one slot as it arrives (ISO, MultiCorder). A slot
    // without sound gets no audio branch: a recorder would wait forever
    // for a track that never comes.
    let (video, audio) = match feed {
        Feed::Program => (pgm_channel(PGM_OUT), Some(pgm_channel(PGM_AUDIO_OUT))),
        Feed::Multiview => (pgm_channel(MV_OUT), Some(pgm_channel(PGM_AUDIO_OUT))),
        Feed::Slot { slot, audio } => (slot_channel(slot), audio.then(|| slot_audio_channel(slot))),
    };
    // An aux bus replaces the feed's sound (e.g. a mix-minus stream).
    let audio = match aux {
        Some(bus) => Some(pgm_channel(&aux_out(bus))),
        None => audio,
    };
    // Screens show picture only.
    let audio = audio.filter(|_| !matches!(spec, OutputSpec::Display { .. }));
    let channels = [("pgm", Some(video)), ("pgm_audio", audio.clone())];
    for (block_id, channel) in channels {
        let Some(channel) = channel else {
            continue;
        };
        flow.blocks.push(block(
            block_id,
            INTER_INPUT_BLOCK,
            vec![
                ("channel", PropertyValue::String(channel)),
                ("max_time", PropertyValue::String(OUTPUT_BUFFER_MS.into())),
            ],
        ));
    }
    let sound = audio.is_some();
    // Screens and NDI read frames on the CPU; the encoders take GPU memory.
    let on_gpu = matches!(feed, Feed::Program | Feed::Multiview) && backend == MixerBackend::Cuda;

    match spec {
        OutputSpec::Srt {
            uri,
            video_kbps,
            encoding,
        } => {
            add_encoder(&mut flow, *video_kbps, encoding, framerate);
            flow.blocks.push(block(
                "srt",
                SRT_OUTPUT_BLOCK,
                vec![
                    ("srt_uri", PropertyValue::String(uri.clone())),
                    ("auto_reconnect", PropertyValue::Bool(true)),
                    ("wait_for_connection", PropertyValue::Bool(false)),
                ],
            ));
            flow.links
                .push(link("enc:encoded_out".into(), "srt:video_in".into()));
            if sound {
                flow.links
                    .push(link("pgm_audio:src".into(), "srt:audio_in_0".into()));
            }
        }
        OutputSpec::Rtmp {
            url,
            key,
            video_kbps,
            encoding,
        } => {
            add_encoder(&mut flow, *video_kbps, encoding, framerate);
            flow.blocks.push(block(
                "rtmp",
                RTMP_OUTPUT_BLOCK,
                vec![
                    (
                        "location",
                        PropertyValue::String(OutputSpec::rtmp_location(url, key)),
                    ),
                    // Output flows run on the mixer's running time (direct
                    // media timing): a syncing sink waits for a clock time
                    // that never comes. The encoder paces it, as for SRT.
                    ("sync", PropertyValue::Bool(false)),
                ],
            ));
            flow.links
                .push(link("enc:encoded_out".into(), "rtmp:video_in".into()));
            // The block encodes raw sound to AAC itself.
            if sound {
                flow.links
                    .push(link("pgm_audio:src".into(), "rtmp:audio_in".into()));
            }
        }
        OutputSpec::Record {
            folder,
            container,
            video_kbps,
            encoding,
        } => {
            add_encoder(&mut flow, *video_kbps, encoding, framerate);
            let recorder = match (container, live_mkv) {
                (lmix_types::Container::Mkv, LiveMkv::No) => MKV_RECORDER_BLOCK,
                _ => RECORDER_BLOCK,
            };
            flow.blocks.push(block(
                "rec",
                recorder,
                vec![
                    ("container", PropertyValue::String(container.nick().into())),
                    ("output_dir", PropertyValue::String(folder.clone())),
                    (
                        "filename_prefix",
                        PropertyValue::String(recording_prefix(id)),
                    ),
                    ("num_audio_tracks", PropertyValue::UInt(u64::from(sound))),
                ],
            ));
            flow.links
                .push(link("enc:encoded_out".into(), "rec:video_in_0".into()));
            if sound {
                add_aac(&mut flow, "rec:audio_in_0");
            }
        }
        OutputSpec::Display { mode } => {
            let from = add_download(&mut flow, "pgm:src", on_gpu);
            add_display(&mut flow, &from, *mode);
        }
        OutputSpec::Ndi { name } => {
            flow.blocks.push(block(
                "ndi",
                NDI_OUTPUT_BLOCK,
                vec![("ndi_name", PropertyValue::String(name.clone()))],
            ));
            let from = add_download(&mut flow, "pgm:src", on_gpu);
            flow.links.push(link(from, "ndi:video_in".into()));
            if sound {
                flow.links
                    .push(link("pgm_audio:src".into(), "ndi:audio_in".into()));
            }
        }
    }
    flow
}

/// Picture to a screen. qos=false: strom drops upstream QoS at sinks with
/// a probe that GStreamer 1.24 answers with a CRITICAL per frame.
/// sync=false: frames come live from the mixer, already as old as its
/// latency; a syncing sink would drop them all as late.
fn add_display(flow: &mut Flow, from: &str, mode: DisplayMode) {
    flow.elements
        .push(element("convert", "videoconvert", vec![]));
    flow.elements.push(display_sink("screen", mode));
    flow.links.push(link(from.into(), "convert".into()));
    flow.links.push(link("convert".into(), "screen".into()));
}

/// The sink element `id` that shows raw video on a screen as `mode` says.
pub(crate) fn display_sink(id: &str, mode: DisplayMode) -> Element {
    let mut properties = vec![
        ("qos", PropertyValue::Bool(false)),
        ("sync", PropertyValue::Bool(false)),
    ];
    let sink = match mode {
        DisplayMode::Window => "glimagesink",
        DisplayMode::Fullscreen => {
            properties.push(("fullscreen", PropertyValue::Bool(true)));
            "waylandsink"
        }
        DisplayMode::Drm { connector } => {
            properties.push(("connector-id", PropertyValue::Int(i64::from(connector))));
            "kmssink"
        }
    };
    element(id, sink, properties)
}

/// Program leaves the CUDA mixer in GPU memory: elements that read frames
/// on the CPU get them downloaded first. Returns what to link from.
fn add_download(flow: &mut Flow, from: &str, on_gpu: bool) -> String {
    if !on_gpu {
        return from.to_string();
    }
    flow.elements
        .push(element("download", "cudadownload", vec![]));
    // cudadownload passes GPU memory through to anything that accepts it,
    // and unixfdsink accepts any caps.
    flow.elements.push(element(
        "sysmem",
        "capsfilter",
        vec![("caps", PropertyValue::String("video/x-raw".into()))],
    ));
    flow.links.push(link(from.into(), "download".into()));
    flow.links.push(link("download".into(), "sysmem".into()));
    "sysmem".into()
}

/// Program sound as AAC into `to`. The live recorder would encode raw
/// sound itself; the older MKV recorder takes encoded audio only.
fn add_aac(flow: &mut Flow, to: &str) {
    flow.elements.push(element("aconv", "audioconvert", vec![]));
    flow.elements.push(element("aac", "avenc_aac", vec![]));
    flow.elements.push(element("aacparse", "aacparse", vec![]));
    flow.links
        .push(link("pgm_audio:src".into(), "aconv".into()));
    flow.links.push(link("aconv".into(), "aac".into()));
    flow.links.push(link("aac".into(), "aacparse".into()));
    flow.links.push(link("aacparse".into(), to.into()));
}

pub fn live_flow_id(which: LivePicture) -> FlowId {
    let name = match which {
        LivePicture::Program => "live-program".to_string(),
        LivePicture::Slot(slot) => format!("live-slot-{slot}"),
        LivePicture::SlotFull(slot) => format!("live-slot-{slot}-full"),
    };
    Uuid::new_v5(&FLOW_NAMESPACE, name.as_bytes())
}

/// Socket file name of a live picture, inside the live directory.
pub fn live_socket_name(which: LivePicture) -> String {
    match which {
        LivePicture::Program => "program.sock".into(),
        LivePicture::Slot(slot) => format!("slot-{slot}.sock"),
        LivePicture::SlotFull(slot) => format!("slot-{slot}-full.sock"),
    }
}

/// Live picture for a Console on the engine's machine: raw frames over a
/// Unix socket, no encoding (the Console reads it with `unixfdsrc`).
/// Inputs at tile size or as they arrive, program at full size.
///
/// ```text
///   pgm ─► [cudadownload ─► caps] ─► videoscale ─► caps ─► unixfdsink   input (tile)
///   pgm ─► [cudadownload ─► caps] ─► [compositor] ─► unixfdsink         input (full), program
///
/// The download only with the CUDA mixer, the copy only before GStreamer
/// 1.28 (see [`FdSink`]).
/// ```
/// Whether `unixfdsink` takes frames that are not in fd memory: it copies
/// them itself only from GStreamer 1.28 on; before, a full-size live
/// picture (nothing scales it into the sink's memory) needs a copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FdSink {
    CopiesItself,
    NeedsFdMemory,
}

impl FdSink {
    /// First GStreamer version whose `unixfdsink` copies.
    const COPIES_FROM: (u32, u32) = (1, 28);

    /// From strom's GStreamer version, e.g. "1.24.2"; unknown counts as old.
    pub fn for_gstreamer(version: &str) -> Self {
        let mut parts = version.split('.').map(|p| p.parse::<u32>().ok());
        match (parts.next().flatten(), parts.next().flatten()) {
            (Some(major), Some(minor)) if (major, minor) >= Self::COPIES_FROM => Self::CopiesItself,
            _ => Self::NeedsFdMemory,
        }
    }
}

pub fn live_flow(which: LivePicture, socket: &str, backend: MixerBackend, sink: FdSink) -> Flow {
    let (name, channel) = match which {
        LivePicture::Program => ("program".to_string(), pgm_channel(PGM_OUT)),
        LivePicture::Slot(slot) => (format!("slot {slot}"), slot_channel(slot)),
        LivePicture::SlotFull(slot) => (format!("slot {slot} full"), slot_channel(slot)),
    };
    let mut flow = Flow::new(format!("lmix live {name}"));
    flow.id = live_flow_id(which);
    share_timing(&mut flow);
    flow.blocks.push(block(
        "pgm",
        INTER_INPUT_BLOCK,
        vec![
            ("channel", PropertyValue::String(channel)),
            ("max_time", PropertyValue::String(LIVE_BUFFER_MS.into())),
        ],
    ));
    flow.elements.push(element(
        "sock",
        "unixfdsink",
        vec![
            ("socket-path", PropertyValue::String(socket.into())),
            ("sync", PropertyValue::Bool(false)),
            // strom's QoS drop probe raises a CRITICAL per frame on sinks
            // that send QoS (see the display sink).
            ("qos", PropertyValue::Bool(false)),
        ],
    ));
    // With the CUDA mixer, program and decoded inputs may be in GPU
    // memory (`add_player`); cudadownload passes system memory through.
    let from = add_download(&mut flow, "pgm:src", backend == MixerBackend::Cuda);
    if let LivePicture::Slot(_) = which {
        flow.elements.push(element("scale", "videoscale", vec![]));
        flow.elements.push(element(
            "size",
            "capsfilter",
            vec![("caps", PropertyValue::String(LIVE_INPUT_CAPS.into()))],
        ));
        flow.links.push(link(from, "scale".into()));
        flow.links.push(link("scale".into(), "size".into()));
        flow.links.push(link("size".into(), "sock".into()));
    } else if sink == FdSink::NeedsFdMemory {
        // A compositor with one input writes every frame anew, into the
        // memfd memory unixfdsink offers.
        // Frames carry the producer's running time (base time 0): start
        // at the first one, as the mixer does.
        flow.elements.push(element(
            "copy",
            "compositor",
            vec![
                (
                    "start-time-selection",
                    PropertyValue::String("first".into()),
                ),
                ("background", PropertyValue::String("black".into())),
            ],
        ));
        flow.links.push(link(from, "copy".into()));
        flow.links.push(link("copy".into(), "sock".into()));
    } else {
        flow.links.push(link(from, "sock".into()));
    }
    flow
}

/// Program encoded as `encoding` says (STR.3): hardware when the engine
/// has one, else software. The keyframe interval goes to the encoder in
/// frames of the program rate.
fn add_encoder(flow: &mut Flow, video_kbps: u32, encoding: &VideoEncoding, framerate: Framerate) {
    let codec = match encoding.codec {
        VideoCodec::H264 => "h264",
        VideoCodec::H265 => "h265",
    };
    let rate = match encoding.rate {
        RateMode::Vbr => "vbr",
        RateMode::Cbr => "cbr",
    };
    flow.blocks.push(block(
        "enc",
        VIDEO_ENCODER_BLOCK,
        vec![
            ("codec", PropertyValue::String(codec.into())),
            ("encoder_preference", PropertyValue::String("auto".into())),
            ("bitrate", PropertyValue::UInt(u64::from(video_kbps))),
            ("rate_control", PropertyValue::String(rate.into())),
            (
                "keyframe_interval",
                PropertyValue::UInt(keyframe_frames(encoding.keyframe_ms, framerate)),
            ),
        ],
    ));
    flow.links
        .push(link("pgm:src".into(), "enc:video_in".into()));
}

/// Frames between keyframes for `ms` at `framerate`; 0 stays automatic.
fn keyframe_frames(ms: u32, framerate: Framerate) -> u64 {
    let (num, den) = framerate
        .fraction()
        .split_once('/')
        .and_then(|(n, d)| Some((n.parse::<u64>().ok()?, d.parse::<u64>().ok()?)))
        .unwrap_or((30, 1));
    (u64::from(ms) * num).div_ceil(1000 * den)
}

/// Element in the mixer flow that delays the sound of `slot`.
pub fn audio_delay(slot: usize) -> String {
    format!("adelay{slot}")
}

/// Inter output of aux bus `bus` (0-based) in the mixer flow.
fn aux_out(bus: usize) -> String {
    format!("aux{}_out", bus + 1)
}

/// `inter` buffer of slot sound in the mixer: holds the longest delay.
const AUDIO_INTER_BUFFER_MS: &str = "1000";

/// Sound of every slot into one audio mixer; its main mix is program
/// audio (into the vision mixer, which meters it on the multiview).
fn add_audio_mixer(flow: &mut Flow, slots: usize, monitor: MonitorOut, sample_rate: u32) {
    for slot in 0..slots {
        let input = format!("ain{slot}");
        flow.blocks.push(block(
            &input,
            INTER_INPUT_BLOCK,
            vec![
                ("channel", PropertyValue::String(slot_audio_channel(slot))),
                // Room for the channel delay (ChannelStrip::DELAY_MAX_MS).
                (
                    "max_time",
                    PropertyValue::String(AUDIO_INTER_BUFFER_MS.into()),
                ),
            ],
        ));
        // Channel delay (lip sync): later timestamps, the mixer plays the
        // sound that much later. Stays when the slot is rebuilt.
        let delay = audio_delay(slot);
        flow.elements.push(element(
            &delay,
            "identity",
            vec![("ts-offset", PropertyValue::Int(0))],
        ));
        flow.links.push(link(format!("{input}:src"), delay.clone()));
        flow.links.push(link(
            delay,
            format!("{AUDIO_MIXER}:input_{}", audio_channel(slot)),
        ));
    }
    let mut properties = vec![
        // The slots, then the stinger clip.
        (
            "num_channels".to_string(),
            PropertyValue::UInt(slots as u64 + 1),
        ),
        // The bus rate (AUD.17). Without it the mixer, running before
        // the first slot sounds, settles on 44.1 kHz and refuses the slots.
        (
            "sample_rate".to_string(),
            PropertyValue::UInt(u64::from(sample_rate)),
        ),
        (
            "start_time_selection".to_string(),
            PropertyValue::String("first".into()),
        ),
        (
            "num_aux_buses".to_string(),
            PropertyValue::UInt(AUX_BUSES as u64),
        ),
    ];
    // Pre or post fader is fixed when strom builds the mixer.
    for slot in 0..slots {
        for (bus, pre) in AUX_PRE_FADER.iter().enumerate() {
            properties.push((
                format!("ch{}_aux{}_pre", audio_channel(slot), bus + 1),
                PropertyValue::Bool(*pre),
            ));
        }
    }
    let properties = properties
        .iter()
        .map(|(name, value)| (name.as_str(), value.clone()))
        .collect();
    flow.blocks
        .push(block(AUDIO_MIXER, AUDIO_MIXER_BLOCK, properties));
    // The stinger clip's sound goes to program at unity, no strip.
    flow.links.push(link(
        format!("{STINGER_PLAYER}:audio_out"),
        format!("{AUDIO_MIXER}:input_{}", stinger_channel(slots)),
    ));
    flow.links.push(link(
        format!("{AUDIO_MIXER}:main_out"),
        format!("{MIXER_BLOCK}:pgm_audio_in"),
    ));
    // Aux buses on inter channels, for outputs that carry a bus's sound.
    for bus in 0..AUX_BUSES {
        let out = aux_out(bus);
        flow.blocks.push(block(&out, INTER_OUTPUT_BLOCK, vec![]));
        flow.links.push(link(
            format!("{AUDIO_MIXER}:aux_out_{}", bus + 1),
            format!("{out}:sink"),
        ));
    }

    // strom switches the monitor bus between program and the solos.
    if monitor == MonitorOut::Auto {
        flow.elements
            .push(element(MONITOR_SINK, "autoaudiosink", vec![]));
        flow.links.push(link(
            format!("{AUDIO_MIXER}:monitor_out"),
            MONITOR_SINK.into(),
        ));
    }
}

/// Block id of a web page or graphics page in a slot flow.
pub const WEB: &str = "web";

/// What a slot flow is built for besides its source.
#[derive(Clone, Debug)]
pub struct SlotEnv {
    pub backend: MixerBackend,
    /// Program format: web and graphics pages render at its size and rate.
    pub format: MasterFormat,
    /// Where the engine reaches lmix-core's pages (graphics channels),
    /// e.g. `http://127.0.0.1:8090`.
    pub pages: String,
}

#[cfg(test)]
impl SlotEnv {
    pub fn new(backend: MixerBackend) -> Self {
        Self {
            backend,
            format: MasterFormat::default(),
            pages: DEFAULT_PAGES.into(),
        }
    }
}

/// lmix-core's own address when nothing else is known.
pub const DEFAULT_PAGES: &str = "http://127.0.0.1:8090";

/// One slot: the source, published on the slot's inter channel.
pub fn slot_flow(slot: usize, source: &SourceSpec, env: &SlotEnv) -> Flow {
    let mut flow = Flow::new(format!("lmix slot {slot}"));
    flow.id = slot_id(slot);
    share_timing(&mut flow);

    // Sound-only sources publish no picture: their mixer input stays empty.
    if let Some(out) = add_source(&mut flow, source, env) {
        flow.blocks.push(block(SLOT_TAP, THUMBNAIL_BLOCK, vec![]));
        flow.links.push(link(out, format!("{SLOT_TAP}:video_in")));
        flow.blocks
            .push(block(SLOT_OUT, INTER_OUTPUT_BLOCK, vec![]));
        flow.links.push(link(
            format!("{SLOT_TAP}:video_out"),
            format!("{SLOT_OUT}:sink"),
        ));
    }

    if let Some(sound) = add_audio(&mut flow, slot, source) {
        let sound = add_audio_format(&mut flow, sound);
        flow.blocks
            .push(block(SLOT_AUDIO_OUT, INTER_OUTPUT_BLOCK, vec![]));
        flow.links
            .push(link(sound, format!("{SLOT_AUDIO_OUT}:sink")));
    }
    flow
}

/// Add the source and return the pad that carries its video; `None` for
/// sound-only sources (see `SourceSpec::has_video`).
fn add_source(flow: &mut Flow, source: &SourceSpec, env: &SlotEnv) -> Option<String> {
    let backend = env.backend;
    let pad = match source {
        SourceSpec::Web { url, .. } => add_page(flow, url, PageKind::Web, &env.format),
        SourceSpec::Graphics { graphics } => {
            let url = format!(
                "{}/graphics/{}",
                env.pages.trim_end_matches('/'),
                encode_path_segment(graphics)
            );
            add_page(flow, &url, PageKind::Graphics, &env.format)
        }
        SourceSpec::Pattern { pattern } => {
            let pattern = pattern.nick().into();
            add_test_picture(flow, vec![("pattern", PropertyValue::String(pattern))])
        }
        SourceSpec::Color { color } => {
            // The switcher rejects malformed colors; black as last resort.
            let argb = OPAQUE | color_rgb(color).unwrap_or_default();
            add_test_picture(
                flow,
                vec![
                    ("pattern", PropertyValue::String("solid-color".into())),
                    ("foreground-color", PropertyValue::UInt(u64::from(argb))),
                ],
            )
        }
        SourceSpec::Image { path } => add_still(flow, path),
        SourceSpec::File { .. } | SourceSpec::Url { .. } | SourceSpec::Playlist { .. } => {
            let items = player_items(source).expect("a player source");
            add_player(flow, &items, playout_delay_ms(source), backend)
        }
        SourceSpec::Srt { uri } => {
            flow.blocks.push(block(
                "srt",
                SRT_INPUT_BLOCK,
                vec![
                    ("srt_uri", PropertyValue::String(uri.clone())),
                    ("decode", PropertyValue::Bool(true)),
                    // Wait for the sender instead of ending at once, and keep
                    // listening when it drops: it can come back (failback).
                    ("wait_for_connection", PropertyValue::Bool(true)),
                    ("keep_listening", PropertyValue::Bool(true)),
                    ("num_video_tracks", PropertyValue::UInt(1)),
                    ("num_audio_tracks", PropertyValue::UInt(1)),
                ],
            ));
            "srt:video_out".into()
        }
        SourceSpec::Ndi { name } => {
            flow.blocks.push(block(
                "ndi",
                NDI_INPUT_BLOCK,
                vec![("ndi_name", PropertyValue::String(name.clone()))],
            ));
            "ndi:video_out".into()
        }
        SourceSpec::Whip { endpoint } => {
            flow.blocks.push(block(
                "whip",
                WHIP_INPUT_BLOCK,
                vec![
                    ("endpoint_id", PropertyValue::String(endpoint.clone())),
                    ("mode", PropertyValue::String("audio_video".into())),
                    ("decode", PropertyValue::Bool(true)),
                ],
            ));
            "whip:video_out".into()
        }
        SourceSpec::Device { id } => {
            flow.blocks.push(block(
                "dev",
                LOCAL_INPUT_BLOCK,
                vec![
                    ("stream_mode", PropertyValue::String("video".into())),
                    ("video_device", PropertyValue::String(id.clone())),
                ],
            ));
            "dev:video_out".into()
        }
        SourceSpec::AudioDevice { .. } => return None,
    };
    Some(pad)
}

/// `videotestsrc` with `properties`, live, in the pattern format.
fn add_test_picture(flow: &mut Flow, mut properties: Vec<(&str, PropertyValue)>) -> String {
    properties.push(("is-live", PropertyValue::Bool(true)));
    flow.elements
        .push(element("src", "videotestsrc", properties));
    flow.elements.push(element(
        "caps",
        "capsfilter",
        vec![("caps", PropertyValue::String(pattern_caps()))],
    ));
    flow.links.push(link("src".into(), "caps".into()));
    "caps".into()
}

/// Still image: decoded once, repeated live (`still_caps`).
fn add_still(flow: &mut Flow, path: &str) -> String {
    flow.elements.push(element(
        "file",
        "filesrc",
        vec![("location", PropertyValue::String(path.into()))],
    ));
    flow.elements.push(element("decode", "decodebin", vec![]));
    flow.elements
        .push(element("convert", "videoconvert", vec![]));
    flow.elements.push(element("scale", "videoscale", vec![]));
    flow.elements.push(element(
        "freeze",
        "imagefreeze",
        vec![("is-live", PropertyValue::Bool(true))],
    ));
    flow.elements.push(element(
        "caps",
        "capsfilter",
        vec![("caps", PropertyValue::String(still_caps(path)))],
    ));
    // decodebin's picture pad appears at runtime (src_0); strom links
    // dynamic pads only with both pad names given.
    for (from, to) in [
        ("file", "decode"),
        ("decode:src", "convert:sink"),
        ("convert", "scale"),
        ("scale", "freeze"),
        ("freeze", "caps"),
    ] {
        flow.links.push(link(from.into(), to.into()));
    }
    "caps".into()
}

/// The files of a source played by the media player; `None` for others.
pub fn player_items(source: &SourceSpec) -> Option<Vec<String>> {
    match source {
        SourceSpec::File { path } => Some(vec![path.clone()]),
        SourceSpec::Url { url } => Some(vec![url.clone()]),
        SourceSpec::Playlist { paths } => Some(paths.clone()),
        _ => None,
    }
}

/// What a page in a slot is.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PageKind {
    /// Someone's web page: with sound, kept off the local network.
    Web,
    /// lmix-core's graphics page: silent, served from this machine.
    Graphics,
}

/// The engine's browser rendering `url` at the program's size and rate
/// (strom `builtin.html_input`, Chromium through `cefsrc`).
fn add_page(flow: &mut Flow, url: &str, kind: PageKind, format: &MasterFormat) -> String {
    let stream_mode = match kind {
        PageKind::Web => "audio_video",
        PageKind::Graphics => "video",
    };
    flow.blocks.push(block(
        WEB,
        HTML_INPUT_BLOCK,
        vec![
            ("url", PropertyValue::String(url.into())),
            ("width", PropertyValue::UInt(u64::from(format.width))),
            ("height", PropertyValue::UInt(u64::from(format.height))),
            (
                "framerate",
                PropertyValue::UInt(whole_fps(format.framerate.fraction())),
            ),
            ("stream_mode", PropertyValue::String(stream_mode.into())),
            // strom refuses pages on this machine and its network unless
            // told otherwise; lmix-core's own page is one of them.
            ("strict_network", PropertyValue::Bool(kind == PageKind::Web)),
            // lmix-core calls the page's functions, sets its CSS and
            // freezes it (strom `devtools/page`).
            ("page_scripting", PropertyValue::Bool(true)),
        ],
    ));
    format!("{WEB}:video_out")
}

/// Frames per second rounded up: Chromium renders whole frames per second.
fn whole_fps(fraction: &str) -> u64 {
    let (num, den) = fraction.split_once('/').unwrap_or((fraction, "1"));
    let (num, den) = (
        num.parse::<u64>().unwrap_or(30),
        den.parse::<u64>().unwrap_or(1),
    );
    num.div_ceil(den.max(1))
}

/// `text` as one URL path segment: unreserved characters stay, the rest
/// is percent-encoded.
pub(crate) fn encode_path_segment(text: &str) -> String {
    let mut out = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Playout delay of a player on local files (ms). strom holds every
/// buffer this long before it plays (500 ms by default), to ride out
/// network stalls; a local file needs little, and the delay is how late a
/// clip starts after a cue.
pub const FILE_PLAYOUT_DELAY_MS: u64 = 100;

/// Playout delay for a player source; `None` keeps strom's default.
fn playout_delay_ms(source: &SourceSpec) -> Option<u64> {
    match source {
        SourceSpec::File { .. } | SourceSpec::Playlist { .. } => Some(FILE_PLAYOUT_DELAY_MS),
        _ => None,
    }
}

/// Media player with `items`, looped (see `Command::SetAtEnd`); carries
/// picture and sound.
/// A hardware decoder (NVDEC, VA-API) keeps frames in GPU memory, and the
/// inter channel passes any caps on. Only the CUDA mixer reads them, so for
/// the others the player hands over system memory.
fn add_player(
    flow: &mut Flow,
    items: &[String],
    delay_ms: Option<u64>,
    backend: MixerBackend,
) -> String {
    let playlist = serde_json::to_string(items).expect("playlist serializes");
    let mut properties = vec![
        ("playlist", PropertyValue::String(playlist)),
        ("decode", PropertyValue::Bool(true)),
        ("loop_playlist", PropertyValue::Bool(true)),
        (
            "system_memory",
            PropertyValue::Bool(backend != MixerBackend::Cuda),
        ),
    ];
    if let Some(ms) = delay_ms {
        properties.push(("playout_delay_ms", PropertyValue::UInt(ms)));
    }
    flow.blocks
        .push(block(PLAYER, MEDIA_PLAYER_BLOCK, properties));
    format!("{PLAYER}:video_out")
}

/// Add the source's sound and return its pad; `None` without sound
/// (see `SourceSpec::has_audio`). Blocks shared with the picture come
/// from `add_source`. A file without an audio track leaves the pad silent.
fn add_audio(flow: &mut Flow, slot: usize, source: &SourceSpec) -> Option<String> {
    if !source.has_audio() {
        return None;
    }
    let pad = match source {
        SourceSpec::Pattern { .. } => {
            let freq = TONE_BASE_HZ * TONE_STEP.powi(slot as i32 % 8);
            flow.elements.push(element(
                "tone",
                "audiotestsrc",
                vec![
                    ("is-live", PropertyValue::Bool(true)),
                    ("freq", PropertyValue::Float(freq)),
                    ("volume", PropertyValue::Float(TONE_VOLUME)),
                ],
            ));
            flow.elements.push(element(
                "tone_caps",
                "capsfilter",
                vec![("caps", PropertyValue::String(AUDIO_CAPS.into()))],
            ));
            flow.links.push(link("tone".into(), "tone_caps".into()));
            "tone_caps".into()
        }
        SourceSpec::File { .. } | SourceSpec::Url { .. } | SourceSpec::Playlist { .. } => {
            format!("{PLAYER}:audio_out")
        }
        SourceSpec::Srt { .. } => "srt:audio_out_0".into(),
        SourceSpec::Ndi { .. } => "ndi:audio_out".into(),
        SourceSpec::Whip { .. } => "whip:audio_out".into(),
        SourceSpec::Web { .. } => format!("{WEB}:audio_out"),
        SourceSpec::AudioDevice { id } => {
            flow.blocks.push(block(
                "adev",
                LOCAL_INPUT_BLOCK,
                vec![
                    ("stream_mode", PropertyValue::String("audio".into())),
                    ("audio_device", PropertyValue::String(id.clone())),
                ],
            ));
            "adev:audio_out".into()
        }
        SourceSpec::Device { .. }
        | SourceSpec::Color { .. }
        | SourceSpec::Image { .. }
        | SourceSpec::Graphics { .. } => return None,
    };
    Some(pad)
}

/// Convert and resample `sound` to [`AUDIO_CAPS`]; returns the new pad.
fn add_audio_format(flow: &mut Flow, sound: String) -> String {
    flow.elements.push(element("aconv", "audioconvert", vec![]));
    flow.elements.push(element("ares", "audioresample", vec![]));
    flow.elements.push(element(
        "aformat",
        "capsfilter",
        vec![("caps", PropertyValue::String(AUDIO_CAPS.into()))],
    ));
    flow.links.push(link(sound, "aconv".into()));
    flow.links.push(link("aconv".into(), "ares".into()));
    flow.links.push(link("ares".into(), "aformat".into()));
    "aformat".into()
}

/// Same clock and base time 0 in every lmix flow: `inter` hands buffers
/// over with their timestamps, so running times must match across flows,
/// or the mixer drops every coupled frame as late (black program).
fn share_timing(flow: &mut Flow) {
    flow.properties.clock_type = GStreamerClockType::Monotonic;
    flow.properties.direct_media_timing = Some(true);
}

/// Program size, so a still keeps its detail: per frame no more than a
/// 1080p video input, and nothing to decode.
fn still_caps(path: &str) -> String {
    let (width, height) = PROGRAM_SIZE;
    let alpha = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| ALPHA_IMAGES.contains(&e.to_ascii_lowercase().as_str()));
    let format = if alpha {
        STILL_ALPHA_FORMAT
    } else {
        PATTERN_FORMAT
    };
    format!("video/x-raw,format={format},width={width},height={height},framerate={PATTERN_FPS}/1")
}

fn pattern_caps() -> String {
    format!(
        "video/x-raw,format={PATTERN_FORMAT},width={PATTERN_WIDTH},height={PATTERN_HEIGHT},framerate={PATTERN_FPS}/1"
    )
}

fn block(id: &str, definition: &str, properties: Vec<(&str, PropertyValue)>) -> BlockInstance {
    BlockInstance {
        id: id.into(),
        block_definition_id: definition.into(),
        name: None,
        properties: properties
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        position: Position { x: 0.0, y: 0.0 },
        runtime_data: None,
        computed_external_pads: None,
    }
}

fn element(id: &str, kind: &str, properties: Vec<(&str, PropertyValue)>) -> Element {
    Element {
        id: id.into(),
        element_type: kind.into(),
        properties: properties
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect(),
        pad_properties: Default::default(),
        position: (0.0, 0.0),
    }
}

fn link(from: String, to: String) -> Link {
    Link { from, to }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mixer_carries_its_format() {
        let upright = MasterFormat {
            width: 1080,
            height: 1920,
            framerate: lmix_types::Framerate::Fps59_94,
        };
        let layout = MultiviewLayout {
            program: ProgramSide::Left,
            meters: MultiviewMeters::Hidden,
        };
        let flow = mixer_flow(
            2,
            MixerBackend::Cpu,
            MonitorOut::None,
            upright,
            layout,
            44_100,
        );
        // The multiview layout (OUT.10) and the sample rate (AUD.17) are
        // read back like the format.
        assert_eq!(multiview_of(&flow), layout);
        assert_eq!(sample_rate_of(&flow), Some(44_100));
        let plain = mixer_flow(
            2,
            MixerBackend::Cpu,
            MonitorOut::None,
            upright,
            MultiviewLayout::default(),
            48_000,
        );
        assert_eq!(multiview_of(&plain), MultiviewLayout::default());

        let vm = flow.blocks.iter().find(|b| b.id == MIXER_BLOCK).unwrap();
        assert!(matches!(
            vm.properties.get("pgm_resolution"),
            Some(PropertyValue::String(r)) if r == "1080x1920"
        ));
        assert!(matches!(
            vm.properties.get("pgm_framerate"),
            Some(PropertyValue::String(r)) if r == "60000/1001"
        ));
        assert_eq!(format_of(&flow), Some(upright));
    }

    #[test]
    fn stills_keep_alpha_where_the_file_can_have_it() {
        assert!(still_caps("/media/logo.PNG").contains("format=BGRA,width=1920,height=1080"));
        assert!(still_caps("/media/bg.webp").contains("format=BGRA"));
        assert!(still_caps("/media/photo.jpg").contains("format=NV12,width=1920"));
        assert!(still_caps("/media/noext").contains("format=NV12"));
    }
    use lmix_types::TestPattern;

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(slot_id(3), slot_id(3));
        assert_ne!(slot_id(3), slot_id(4));
        assert_ne!(slot_id(0), mixer_id());
    }

    #[test]
    fn mixer_subscribes_each_slot_channel() {
        let flow = mixer_flow(
            3,
            MixerBackend::Cpu,
            MonitorOut::None,
            MasterFormat::default(),
            MultiviewLayout::default(),
            48_000,
        );

        for slot in 0..3 {
            let input = flow
                .blocks
                .iter()
                .find(|b| b.id == format!("in{slot}"))
                .unwrap();
            assert!(matches!(
                input.properties.get("channel"),
                Some(PropertyValue::String(c)) if *c == slot_channel(slot)
            ));
            assert!(flow
                .links
                .iter()
                .any(|l| l.to == format!("vm:video_in_{slot}")));
        }
    }

    #[test]
    fn mixer_has_black_after_slots() {
        let flow = mixer_flow(
            3,
            MixerBackend::Cpu,
            MonitorOut::None,
            MasterFormat::default(),
            MultiviewLayout::default(),
            48_000,
        );

        assert_eq!(black_input(3), 3);
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "black_caps" && l.to == "vm:video_in_3"));
    }

    #[test]
    fn only_the_gl_mixer_downloads_program() {
        let download = |backend| {
            let flow = mixer_flow(
                2,
                backend,
                MonitorOut::None,
                MasterFormat::default(),
                MultiviewLayout::default(),
                48_000,
            );
            let vm = flow.blocks.iter().find(|b| b.id == "vm").unwrap();
            matches!(
                vm.properties.get("gl_download"),
                Some(PropertyValue::Bool(true))
            )
        };

        assert!(download(MixerBackend::Gl));
        assert!(!download(MixerBackend::Cuda));
    }

    #[test]
    fn slot_publishes_on_its_channel() {
        let flow = slot_flow(
            2,
            &SourceSpec::Pattern {
                pattern: TestPattern::Ball,
            },
            &SlotEnv::new(MixerBackend::Cuda),
        );

        assert_eq!(flow.id, slot_id(2));
        assert!(flow
            .blocks
            .iter()
            .any(|b| b.id == SLOT_OUT && b.block_definition_id == INTER_OUTPUT_BLOCK));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "caps" && l.to == "tap:video_in"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "tap:video_out" && l.to == "out:sink"));
        assert_eq!(slot_channel(2), format!("strom_{}_out", flow.id));
    }

    #[test]
    fn pattern_slot_publishes_a_tone() {
        let flow = slot_flow(
            1,
            &SourceSpec::Pattern {
                pattern: TestPattern::Bars,
            },
            &SlotEnv::new(MixerBackend::Cpu),
        );

        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "tone_caps" && l.to == "aconv"));
    }

    #[test]
    fn color_is_opaque_argb() {
        let flow = slot_flow(
            0,
            &SourceSpec::Color {
                color: "#ff8000".into(),
            },
            &SlotEnv::new(MixerBackend::Cpu),
        );
        let src = flow.elements.iter().find(|e| e.id == "src").unwrap();

        assert!(matches!(
            src.properties.get("foreground-color"),
            Some(PropertyValue::UInt(0xffff_8000))
        ));
    }

    #[test]
    fn playlist_shares_one_player_for_picture_and_sound() {
        let paths = vec!["/a.mp4".to_string(), "/b.mp4".to_string()];
        let flow = slot_flow(
            0,
            &SourceSpec::Playlist { paths },
            &SlotEnv::new(MixerBackend::Cpu),
        );

        assert_eq!(flow.blocks.iter().filter(|b| b.id == PLAYER).count(), 1);
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "player:audio_out" && l.to == "aconv"));
        assert!(matches!(
            flow.blocks[0].properties.get("playlist"),
            Some(PropertyValue::String(p)) if p == "[\"/a.mp4\",\"/b.mp4\"]"
        ));
    }

    #[test]
    fn audio_device_publishes_sound_only() {
        let flow = slot_flow(
            0,
            &SourceSpec::AudioDevice { id: "mic".into() },
            &SlotEnv::new(MixerBackend::Cpu),
        );

        assert!(flow
            .blocks
            .iter()
            .all(|b| b.id != SLOT_OUT && b.id != SLOT_TAP));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "adev:audio_out" && l.to == "aconv"));
    }

    #[test]
    fn image_is_frozen_live_in_pattern_format() {
        let flow = slot_flow(
            0,
            &SourceSpec::Image {
                path: "/logo.png".into(),
            },
            &SlotEnv::new(MixerBackend::Cuda),
        );

        assert!(flow
            .elements
            .iter()
            .any(|e| e.element_type == "imagefreeze"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "caps" && l.to == "tap:video_in"));
        assert!(flow.blocks.iter().all(|b| b.id != SLOT_AUDIO_OUT));
    }

    #[test]
    fn device_slot_has_no_sound() {
        let flow = slot_flow(
            0,
            &SourceSpec::Device { id: "cam".into() },
            &SlotEnv::new(MixerBackend::Cpu),
        );

        assert!(flow.blocks.iter().all(|b| b.id != SLOT_AUDIO_OUT));
    }

    #[test]
    fn mixer_mixes_every_slot_sound() {
        let flow = mixer_flow(
            3,
            MixerBackend::Cpu,
            MonitorOut::None,
            MasterFormat::default(),
            MultiviewLayout::default(),
            48_000,
        );

        for slot in 0..3 {
            let to = format!("mix:input_{}", slot + 1);
            let delay = audio_delay(slot);
            assert!(flow
                .links
                .iter()
                .any(|l| l.from == format!("ain{slot}:src") && l.to == delay));
            assert!(flow.links.iter().any(|l| l.from == delay && l.to == to));
        }
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "mix:main_out" && l.to == "vm:pgm_audio_in"));
        assert!(flow.elements.iter().all(|e| e.id != MONITOR_SINK));

        let heard = mixer_flow(
            3,
            MixerBackend::Cpu,
            MonitorOut::Auto,
            MasterFormat::default(),
            MultiviewLayout::default(),
            48_000,
        );
        assert!(heard
            .links
            .iter()
            .any(|l| l.from == "mix:monitor_out" && l.to == MONITOR_SINK));
    }

    #[test]
    fn mixer_publishes_program_for_outputs() {
        let flow = mixer_flow(
            2,
            MixerBackend::Cpu,
            MonitorOut::None,
            MasterFormat::default(),
            MultiviewLayout::default(),
            48_000,
        );

        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "tap_pgm:video_out" && l.to == "pgm_out:sink"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "mix:main_out" && l.to == "pgm_audio_out:sink"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "vm:multiview_out" && l.to == "mv_out:sink"));
    }

    #[test]
    fn an_output_can_show_the_multiview() {
        let spec = OutputSpec::Display {
            mode: lmix_types::DisplayMode::Window,
        };
        let flow = output_flow(
            OutputId::new(),
            &spec,
            Feed::Multiview,
            None,
            MixerBackend::Cpu,
            Framerate::default(),
            LiveMkv::Yes,
        );

        let video = flow.blocks.iter().find(|b| b.id == "pgm").unwrap();
        assert!(matches!(
            video.properties.get("channel"),
            Some(PropertyValue::String(c)) if *c == format!("strom_{}_mv_out", mixer_id())
        ));
    }

    #[test]
    fn mixer_plays_stingers_through_strom() {
        let flow = mixer_flow(
            2,
            MixerBackend::Cpu,
            MonitorOut::None,
            MasterFormat::default(),
            MultiviewLayout::default(),
            48_000,
        );

        // One DSK layer per overlay; strom draws the stinger beneath them.
        let vm = flow.blocks.iter().find(|b| b.id == MIXER_BLOCK).unwrap();
        assert!(matches!(
            vm.properties.get("num_dsk_inputs"),
            Some(PropertyValue::String(n)) if *n == OVERLAY_CHANNELS.to_string()
        ));
        assert_eq!(overlay_layer(0), 0);
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "dsk3:src" && l.to == "vm:dsk_in_3"));
        assert!(matches!(
            vm.properties.get(ENABLE_STINGER_PROPERTY),
            Some(PropertyValue::Bool(true))
        ));

        // The stinger source: a player in stinger mode, picture into the
        // mixer, sound into the audio mixer after the slots.
        let player = flow.blocks.iter().find(|b| b.id == STINGER_PLAYER).unwrap();
        assert_eq!(player.block_definition_id, MEDIA_PLAYER_BLOCK);
        assert!(matches!(
            player.properties.get(STINGER_MODE_PROPERTY),
            Some(PropertyValue::Bool(true))
        ));
        let linked = |from: &str, to: &str| flow.links.iter().any(|l| l.from == from && l.to == to);
        assert!(linked("stinger:video_out", "vm:stinger_in"));
        assert!(linked("stinger:audio_out", "mix:input_3"));
        let mix = flow.blocks.iter().find(|b| b.id == AUDIO_MIXER).unwrap();
        assert!(matches!(
            mix.properties.get("num_channels"),
            Some(PropertyValue::UInt(3))
        ));
    }

    #[test]
    fn mixer_publishes_its_aux_buses() {
        let flow = mixer_flow(
            2,
            MixerBackend::Cpu,
            MonitorOut::None,
            MasterFormat::default(),
            MultiviewLayout::default(),
            48_000,
        );

        let mix = flow.blocks.iter().find(|b| b.id == AUDIO_MIXER).unwrap();
        assert!(matches!(
            mix.properties.get("num_aux_buses"),
            Some(PropertyValue::UInt(7))
        ));
        assert!(matches!(
            mix.properties.get("ch2_aux1_pre"),
            Some(PropertyValue::Bool(true))
        ));
        assert!(matches!(
            mix.properties.get("ch2_aux2_pre"),
            Some(PropertyValue::Bool(false))
        ));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "mix:aux_out_2" && l.to == "aux2_out:sink"));
    }

    #[test]
    fn an_aux_bus_replaces_the_output_sound() {
        let spec = OutputSpec::Srt {
            uri: "srt://cdn:9000".into(),
            video_kbps: 6000,
            encoding: Default::default(),
        };
        let flow = output_flow(
            OutputId::new(),
            &spec,
            Feed::Program,
            Some(0),
            MixerBackend::Cpu,
            Framerate::default(),
            LiveMkv::Yes,
        );

        let audio = flow.blocks.iter().find(|b| b.id == "pgm_audio").unwrap();
        assert!(matches!(
            audio.properties.get("channel"),
            Some(PropertyValue::String(c)) if *c == format!("strom_{}_aux1_out", mixer_id())
        ));
    }

    #[test]
    fn rtmp_publishes_to_server_and_key() {
        let spec = OutputSpec::Rtmp {
            url: "rtmp://a.rtmp.example.com/live2/".into(),
            key: "abcd-1234".into(),
            video_kbps: 4500,
            encoding: Default::default(),
        };
        let flow = output_flow(
            OutputId::new(),
            &spec,
            Feed::Program,
            None,
            MixerBackend::Cpu,
            Framerate::default(),
            LiveMkv::Yes,
        );

        let rtmp = flow.blocks.iter().find(|b| b.id == "rtmp").unwrap();
        assert_eq!(rtmp.block_definition_id, RTMP_OUTPUT_BLOCK);
        assert!(matches!(
            rtmp.properties.get("location"),
            Some(PropertyValue::String(l)) if l == "rtmp://a.rtmp.example.com/live2/abcd-1234"
        ));
        // A syncing sink would wait on the mixer's running time for good.
        assert!(matches!(
            rtmp.properties.get("sync"),
            Some(PropertyValue::Bool(false))
        ));
        for (from, to) in [
            ("enc:encoded_out", "rtmp:video_in"),
            ("pgm_audio:src", "rtmp:audio_in"),
        ] {
            assert!(
                flow.links.iter().any(|l| l.from == from && l.to == to),
                "{from} -> {to}"
            );
        }
    }

    #[test]
    fn an_output_encodes_as_it_is_told() {
        let spec = OutputSpec::Srt {
            uri: "srt://cdn:9000".into(),
            video_kbps: 4000,
            encoding: VideoEncoding {
                codec: VideoCodec::H265,
                rate: RateMode::Cbr,
                keyframe_ms: 2000,
            },
        };
        let flow = output_flow(
            OutputId::new(),
            &spec,
            Feed::Program,
            None,
            MixerBackend::Cpu,
            Framerate::Fps25,
            LiveMkv::Yes,
        );

        let enc = flow.blocks.iter().find(|b| b.id == "enc").unwrap();
        let text = |name: &str| match enc.properties.get(name) {
            Some(PropertyValue::String(s)) => s.clone(),
            other => panic!("{name}: {other:?}"),
        };
        assert_eq!(text("codec"), "h265");
        assert_eq!(text("rate_control"), "cbr");
        assert!(matches!(
            enc.properties.get("keyframe_interval"),
            Some(PropertyValue::UInt(50))
        ));
        // 59.94 fps: two seconds round up to 120 frames.
        assert_eq!(keyframe_frames(2000, Framerate::Fps59_94), 120);
        assert_eq!(keyframe_frames(0, Framerate::Fps30), 0, "automatic");
    }

    #[test]
    fn recording_encodes_picture_and_sound() {
        let spec = OutputSpec::Record {
            folder: "show".into(),
            container: lmix_types::Container::Mkv,
            video_kbps: 8000,
            encoding: Default::default(),
        };
        let flow = output_flow(
            OutputId::new(),
            &spec,
            Feed::Program,
            None,
            MixerBackend::Cpu,
            Framerate::default(),
            LiveMkv::Yes,
        );

        let pgm = flow.blocks.iter().find(|b| b.id == "pgm").unwrap();
        assert!(matches!(
            pgm.properties.get("channel"),
            Some(PropertyValue::String(c)) if *c == format!("strom_{}_pgm_out", mixer_id())
        ));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "enc:encoded_out" && l.to == "rec:video_in_0"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "aacparse" && l.to == "rec:audio_in_0")); // The live recorder: readable while it grows, a stopped track does
                                                                         // not hold up the others (REC).
        let rec = flow.blocks.iter().find(|b| b.id == "rec").unwrap();
        assert_eq!(rec.block_definition_id, "builtin.liverecorder"); // Two recordings into one folder never share a file name.
        let prefix = |flow: &Flow| {
            let rec = flow.blocks.iter().find(|b| b.id == "rec").unwrap();
            format!("{:?}", rec.properties.get("filename_prefix"))
        };
        let other = output_flow(
            OutputId::new(),
            &spec,
            Feed::Program,
            None,
            MixerBackend::Cpu,
            Framerate::default(),
            LiveMkv::Yes,
        );
        assert_ne!(prefix(&flow), prefix(&other));
    }

    #[test]
    fn mkv_before_gstreamer_1_26_records_with_the_older_recorder() {
        let spec = |container| OutputSpec::Record {
            folder: "show".into(),
            container,
            video_kbps: 8000,
            encoding: Default::default(),
        };
        let recorder = |container, version: &str| {
            let flow = output_flow(
                OutputId::new(),
                &spec(container),
                Feed::Program,
                None,
                MixerBackend::Cpu,
                Framerate::default(),
                LiveMkv::for_gstreamer(version),
            );
            let rec = flow.blocks.iter().find(|b| b.id == "rec").unwrap();
            rec.block_definition_id.clone()
        };
        use lmix_types::Container::{Mkv, Mp4};
        assert_eq!(recorder(Mkv, "1.24.2"), "builtin.recorder");
        assert_eq!(
            recorder(Mkv, ""),
            "builtin.recorder",
            "unknown counts as old"
        );
        assert_eq!(recorder(Mkv, "1.28.7"), "builtin.liverecorder");
        assert_eq!(recorder(Mp4, "1.24.2"), "builtin.liverecorder");
    }

    #[test]
    fn iso_recording_of_a_silent_input_has_no_audio_track() {
        let spec = OutputSpec::Record {
            folder: "iso".into(),
            container: lmix_types::Container::Mkv,
            video_kbps: 8000,
            encoding: Default::default(),
        };
        let flow = output_flow(
            OutputId::new(),
            &spec,
            Feed::Slot {
                slot: 3,
                audio: false,
            },
            None,
            MixerBackend::Cuda,
            Framerate::default(),
            LiveMkv::Yes,
        );

        let pgm = flow.blocks.iter().find(|b| b.id == "pgm").unwrap();
        assert!(matches!(
            pgm.properties.get("channel"),
            Some(PropertyValue::String(c)) if *c == slot_channel(3)
        ));
        assert!(flow.blocks.iter().all(|b| b.id != "pgm_audio"));
        assert!(flow.links.iter().all(|l| l.to != "rec:audio_in_0"));
    }

    #[test]
    fn live_program_goes_raw_to_its_socket() {
        let flow = live_flow(
            LivePicture::Program,
            "/run/x/program.sock",
            MixerBackend::Cpu,
            FdSink::CopiesItself,
        );

        let sock = flow.elements.iter().find(|e| e.id == "sock").unwrap();
        assert_eq!(sock.element_type, "unixfdsink");
        assert!(matches!(
            sock.properties.get("socket-path"),
            Some(PropertyValue::String(path)) if path == "/run/x/program.sock"
        ));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "pgm:src" && l.to == "sock"));
        assert_ne!(flow.id, live_flow_id(LivePicture::Slot(0)));
    }

    #[test]
    fn live_input_is_scaled_down() {
        let flow = live_flow(
            LivePicture::Slot(3),
            "/run/x/slot-3.sock",
            MixerBackend::Cuda,
            FdSink::CopiesItself,
        );

        let pgm = flow.blocks.iter().find(|b| b.id == "pgm").unwrap();
        assert!(matches!(
            pgm.properties.get("channel"),
            Some(PropertyValue::String(c)) if *c == slot_channel(3)
        ));
        assert!(flow.elements.iter().any(|e| e.element_type == "videoscale"));
        // A decoder may have left the slot in CUDA memory (NVDEC).
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "pgm:src" && l.to == "download"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "sysmem" && l.to == "scale"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "size" && l.to == "sock"));
        assert_eq!(live_socket_name(LivePicture::Slot(3)), "slot-3.sock");
    }

    #[test]
    fn full_live_input_is_not_scaled() {
        let flow = live_flow(
            LivePicture::SlotFull(3),
            "/run/x/slot-3-full.sock",
            MixerBackend::Cuda,
            FdSink::CopiesItself,
        );

        assert!(flow.elements.iter().all(|e| e.element_type != "videoscale"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "sysmem" && l.to == "sock"));
        assert_ne!(
            live_flow_id(LivePicture::SlotFull(3)),
            live_flow_id(LivePicture::Slot(3))
        );
        assert_eq!(
            live_socket_name(LivePicture::SlotFull(3)),
            "slot-3-full.sock"
        );
    }

    #[test]
    fn display_output_is_picture_only_without_qos() {
        let spec = OutputSpec::Display {
            mode: DisplayMode::Drm { connector: 42 },
        };
        let flow = output_flow(
            OutputId::new(),
            &spec,
            Feed::Program,
            None,
            MixerBackend::Cpu,
            Framerate::default(),
            LiveMkv::Yes,
        );

        let screen = flow.elements.iter().find(|e| e.id == "screen").unwrap();
        assert_eq!(screen.element_type, "kmssink");
        assert!(matches!(
            screen.properties.get("qos"),
            Some(PropertyValue::Bool(false))
        ));
        assert!(flow.links.iter().all(|l| !l.from.starts_with("pgm_audio")));
        assert!(flow
            .elements
            .iter()
            .all(|e| e.element_type != "cudadownload"));
    }

    #[test]
    fn cpu_readers_of_cuda_program_download_it() {
        let screen = OutputSpec::Display {
            mode: DisplayMode::Window,
        };
        let flow = output_flow(
            OutputId::new(),
            &screen,
            Feed::Program,
            None,
            MixerBackend::Cuda,
            Framerate::default(),
            LiveMkv::Yes,
        );
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "pgm:src" && l.to == "download"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "sysmem" && l.to == "convert"));

        let live = live_flow(
            LivePicture::Program,
            "/run/x/program.sock",
            MixerBackend::Cuda,
            FdSink::CopiesItself,
        );
        assert!(live
            .links
            .iter()
            .any(|l| l.from == "sysmem" && l.to == "sock"));

        let record = OutputSpec::Record {
            folder: "show".into(),
            container: lmix_types::Container::Mkv,
            video_kbps: 8000,
            encoding: Default::default(),
        };
        let flow = output_flow(
            OutputId::new(),
            &record,
            Feed::Program,
            None,
            MixerBackend::Cuda,
            Framerate::default(),
            LiveMkv::Yes,
        );
        assert!(
            flow.elements
                .iter()
                .all(|e| e.element_type != "cudadownload"),
            "the encoder takes GPU memory"
        );
    }

    #[test]
    fn output_flows_have_their_own_ids() {
        let (a, b) = (OutputId::new(), OutputId::new());
        assert_ne!(output_flow_id(a), output_flow_id(b));
        assert_eq!(output_flow_id(a), output_flow_id(a));
    }

    #[test]
    fn all_flows_share_timing() {
        let slot = slot_flow(
            0,
            &SourceSpec::Pattern {
                pattern: TestPattern::Bars,
            },
            &SlotEnv::new(MixerBackend::Cpu),
        );

        for flow in [
            mixer_flow(
                2,
                MixerBackend::Cpu,
                MonitorOut::None,
                MasterFormat::default(),
                MultiviewLayout::default(),
                48_000,
            ),
            slot,
        ] {
            assert_eq!(flow.properties.clock_type, GStreamerClockType::Monotonic);
            assert!(flow.properties.effective_direct_media_timing());
        }
    }

    #[test]
    fn full_size_live_picture_is_copied_before_gstreamer_1_28() {
        let old = live_flow(
            LivePicture::Program,
            "/run/x/program.sock",
            MixerBackend::Cpu,
            FdSink::NeedsFdMemory,
        );
        let new = live_flow(
            LivePicture::Program,
            "/run/x/program.sock",
            MixerBackend::Cpu,
            FdSink::CopiesItself,
        );
        let tile = live_flow(
            LivePicture::Slot(0),
            "/run/x/slot-0.sock",
            MixerBackend::Cpu,
            FdSink::NeedsFdMemory,
        );
        let has_copy = |f: &Flow| f.elements.iter().any(|e| e.element_type == "compositor");

        assert!(has_copy(&old));
        assert!(old.links.iter().any(|l| l.from == "copy" && l.to == "sock"));
        assert!(!has_copy(&new));
        assert!(
            !has_copy(&tile),
            "videoscale already writes into the sink's memory"
        );
    }

    #[test]
    fn fd_sink_by_gstreamer_version() {
        assert_eq!(FdSink::for_gstreamer("1.24.2"), FdSink::NeedsFdMemory);
        assert_eq!(FdSink::for_gstreamer("1.26.7"), FdSink::NeedsFdMemory);
        assert_eq!(FdSink::for_gstreamer("1.28.0"), FdSink::CopiesItself);
        assert_eq!(FdSink::for_gstreamer("1.28.7.1"), FdSink::CopiesItself);
        assert_eq!(FdSink::for_gstreamer("2.0.0"), FdSink::CopiesItself);
        assert_eq!(FdSink::for_gstreamer(""), FdSink::NeedsFdMemory);
    }

    #[test]
    fn live_input_of_a_cpu_mixer_needs_no_download() {
        let flow = live_flow(
            LivePicture::Slot(1),
            "/run/x/s.sock",
            MixerBackend::Gl,
            FdSink::NeedsFdMemory,
        );

        assert!(flow
            .elements
            .iter()
            .all(|e| e.element_type != "cudadownload"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "pgm:src" && l.to == "scale"));
    }

    #[test]
    fn players_hand_over_system_memory_unless_the_mixer_is_cuda() {
        let file = SourceSpec::File {
            path: "/clip.webm".into(),
        };
        let system_memory = |backend| {
            let flow = slot_flow(0, &file, &SlotEnv::new(backend));
            let player = flow.blocks.iter().find(|b| b.id == "player").unwrap();
            matches!(
                player.properties.get("system_memory"),
                Some(PropertyValue::Bool(true))
            )
        };

        // NVDEC or VA-API would put GPU memory on the inter channel, which
        // the GL and CPU mixers and the live pictures cannot read.
        assert!(system_memory(MixerBackend::Gl));
        assert!(system_memory(MixerBackend::Cpu));
        // The CUDA mixer takes decoded frames without a copy.
        assert!(!system_memory(MixerBackend::Cuda));
    }

    fn html_block(flow: &Flow) -> &strom_types::BlockInstance {
        flow.blocks
            .iter()
            .find(|b| b.block_definition_id == HTML_INPUT_BLOCK)
            .expect("an HTML input")
    }

    fn uint(block: &strom_types::BlockInstance, key: &str) -> u64 {
        match block.properties.get(key) {
            Some(PropertyValue::UInt(v)) => *v,
            other => panic!("{key} is {other:?}"),
        }
    }

    fn text<'a>(block: &'a strom_types::BlockInstance, key: &str) -> &'a str {
        match block.properties.get(key) {
            Some(PropertyValue::String(v)) => v,
            other => panic!("{key} is {other:?}"),
        }
    }

    fn flag(block: &strom_types::BlockInstance, key: &str) -> bool {
        match block.properties.get(key) {
            Some(PropertyValue::Bool(v)) => *v,
            other => panic!("{key} is {other:?}"),
        }
    }

    /// A web page renders at the program's size and rate (GFX.2), with its
    /// sound (GFX.6), and stays off the machine's own network.
    #[test]
    fn web_page_renders_at_program_size_with_sound() {
        let env = SlotEnv {
            format: MasterFormat {
                width: 1080,
                height: 1920,
                framerate: lmix_types::Framerate::Fps29_97,
            },
            ..SlotEnv::new(MixerBackend::Cpu)
        };
        let source = SourceSpec::Web {
            url: "https://example.com/scoreboard".into(),
            css: String::new(),
        };
        let flow = slot_flow(0, &source, &env);
        let web = html_block(&flow);
        assert_eq!(text(web, "url"), "https://example.com/scoreboard");
        assert_eq!(uint(web, "width"), 1080);
        assert_eq!(uint(web, "height"), 1920);
        // Chromium renders whole frames per second: 29.97 asks for 30.
        assert_eq!(uint(web, "framerate"), 30);
        assert_eq!(text(web, "stream_mode"), "audio_video");
        assert!(flag(web, "strict_network"));
        // lmix-core calls the page's functions and sets its CSS.
        assert!(flag(web, "page_scripting"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == format!("{WEB}:audio_out")));
    }

    /// A graphics channel as an input (GFX.2, GFX.17): the engine renders
    /// lmix-core's own transparent page for it, silent, from the local
    /// network that strict mode would refuse.
    #[test]
    fn graphics_channel_renders_the_lmix_page() {
        let env = SlotEnv {
            pages: "http://127.0.0.1:8090".into(),
            ..SlotEnv::new(MixerBackend::Cpu)
        };
        let source = SourceSpec::Graphics {
            graphics: "Lower thirds".into(),
        };
        let flow = slot_flow(0, &source, &env);
        let page = html_block(&flow);
        assert_eq!(
            text(page, "url"),
            "http://127.0.0.1:8090/graphics/Lower%20thirds"
        );
        assert_eq!(text(page, "stream_mode"), "video");
        assert!(!flag(page, "strict_network"));
        assert!(flag(page, "page_scripting"));
        assert!(!source.has_audio());
        assert_eq!(uint(page, "width"), 1920);
    }

    #[test]
    fn local_clips_start_without_the_stream_playout_delay() {
        let delay = |source: &SourceSpec| {
            let flow = slot_flow(0, source, &SlotEnv::new(MixerBackend::Cpu));
            let player = flow.blocks.iter().find(|b| b.id == "player").unwrap();
            match player.properties.get("playout_delay_ms") {
                Some(PropertyValue::UInt(ms)) => Some(*ms),
                None => None,
                Some(other) => panic!("playout_delay_ms is {other:?}"),
            }
        };

        let file = SourceSpec::File {
            path: "/clip.webm".into(),
        };
        let playlist = SourceSpec::Playlist {
            paths: vec!["/a.webm".into(), "/b.webm".into()],
        };
        let url = SourceSpec::Url {
            url: "https://example.com/live.m3u8".into(),
        };
        let short = Some(FILE_PLAYOUT_DELAY_MS);
        assert_eq!(delay(&file), short);
        assert_eq!(delay(&playlist), short);
        // A stream keeps strom's default, which rides out network stalls.
        assert_eq!(delay(&url), None);
    }

    #[test]
    fn file_source_uses_player() {
        let flow = slot_flow(
            0,
            &SourceSpec::File {
                path: "/a b.mp4".into(),
            },
            &SlotEnv::new(MixerBackend::Cpu),
        );
        let player = flow.blocks.iter().find(|b| b.id == "player").unwrap();

        assert!(matches!(
            player.properties.get("playlist"),
            Some(PropertyValue::String(p)) if p == "[\"/a b.mp4\"]"
        ));
        assert!(flow.links.iter().any(|l| l.from == "player:video_out"));
    }
}
