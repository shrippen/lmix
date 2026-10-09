//! strom flows of a show.
//!
//! ```text
//!   slot flow 0: source ─► [caps] ─► tap ─► inter_output "out" ┐
//!                     └─ sound ─────────► inter_output "out_audio" ┐
//!   slot flow 1: …                                                 ├─► mixer flow:
//!   …                                                          ┘   inter_input × N ─► vision mixer
//!                                               inter_input × N ─► audio mixer ─► vm PGM audio
//! ```
//!
//! Flow ids are fixed per slot (UUID v5), so the `inter` channel name
//! `strom_<flow>_<block>` never changes: swapping a source rebuilds only
//! its slot flow, the mixer keeps running (INP.1, ROADMAP 2b.1).

use strom_types::block::{BlockInstance, Position};
use strom_types::element::{Element, Link};
use strom_types::flow::GStreamerClockType;
use strom_types::{Flow, FlowId, PropertyValue};
use uuid::Uuid;

use lmix_types::input::color_rgb;
use lmix_types::{DisplayMode, OutputId, OutputSpec, SourceSpec};

use super::LivePicture;
use crate::switcher::Feed;

/// Namespace of all lmix flow ids.
const FLOW_NAMESPACE: Uuid = Uuid::from_u128(0x6c6d_6978_2d63_6f72_652d_666c_6f77_7321);

const MEDIA_PLAYER_BLOCK: &str = "builtin.media_player";
const SRT_INPUT_BLOCK: &str = "builtin.mpegtssrt_input";
const LOCAL_INPUT_BLOCK: &str = "builtin.local_input";
const NDI_INPUT_BLOCK: &str = "builtin.ndi_input";
const NDI_OUTPUT_BLOCK: &str = "builtin.ndi_output";
const SRT_OUTPUT_BLOCK: &str = "builtin.mpegtssrt_output";
const RECORDER_BLOCK: &str = "builtin.recorder";
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
pub const MIXER_NAME: &str = "lmix mixer 2";
/// Block id of the vision mixer in the mixer flow.
pub const MIXER_BLOCK: &str = "vm";
/// Thumbnail tap on the program output: GUI monitor and picture checks.
pub const PGM_TAP: &str = "tap_pgm";
/// Program picture and sound for the output flows (`inter`).
pub const PGM_OUT: &str = "pgm_out";
const PGM_AUDIO_OUT: &str = "pgm_audio_out";
/// Outputs buffer a little more than the mixer: they are not on the
/// switching path, an encoder hiccup must not drop frames.
const OUTPUT_BUFFER_MS: &str = "500";
/// Recording files: `<folder>/lmix_00001.mkv`, …
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
/// One audio format for every slot, at the audio mixer's bus rate.
const AUDIO_CAPS: &str = "audio/x-raw,format=F32LE,layout=interleaved,rate=48000,channels=2";
/// Bus rate of the audio mixer (Hz), as in [`AUDIO_CAPS`]. Without it the
/// mixer, running before the first slot sounds, settles on 44.1 kHz and
/// refuses the slots.
const AUDIO_RATE: u64 = 48_000;

/// Format test patterns are rendered in; real sources keep theirs.
const PATTERN_WIDTH: u32 = 1280;
const PATTERN_HEIGHT: u32 = 720;
const PATTERN_FPS: u32 = 30;
const PATTERN_FORMAT: &str = "NV12";

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
fn slot_channel(slot: usize) -> String {
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
pub fn mixer_flow(slots: usize, backend: MixerBackend, monitor: MonitorOut) -> Flow {
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
            (
                "compositor_preference",
                PropertyValue::String(backend.property().into()),
            ),
            // Base time 0 (share_timing): output starts at the first frame.
            (
                "start_time_selection",
                PropertyValue::String("first".into()),
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

    add_audio_mixer(&mut flow, slots, monitor);

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
    flow.blocks
        .push(block(PGM_AUDIO_OUT, INTER_OUTPUT_BLOCK, vec![]));
    flow.links.push(link(
        format!("{AUDIO_MIXER}:main_out"),
        format!("{PGM_AUDIO_OUT}:sink"),
    ));
    flow
}

/// `inter` channels of program in the mixer flow.
fn pgm_channel(block: &str) -> String {
    format!("strom_{}_{block}", mixer_id())
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
pub fn output_flow(id: OutputId, spec: &OutputSpec, feed: Feed, backend: MixerBackend) -> Flow {
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
        Feed::Slot { slot, audio } => (slot_channel(slot), audio.then(|| slot_audio_channel(slot))),
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
    let on_gpu = matches!(feed, Feed::Program) && backend == MixerBackend::Cuda;

    match spec {
        OutputSpec::Srt { uri, video_kbps } => {
            add_encoder(&mut flow, *video_kbps);
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
        OutputSpec::Record {
            folder,
            container,
            video_kbps,
        } => {
            add_encoder(&mut flow, *video_kbps);
            flow.blocks.push(block(
                "rec",
                RECORDER_BLOCK,
                vec![
                    ("container", PropertyValue::String(container.nick().into())),
                    ("output_dir", PropertyValue::String(folder.clone())),
                    (
                        "filename_prefix",
                        PropertyValue::String(RECORDING_PREFIX.into()),
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
    flow.elements
        .push(element("convert", "videoconvert", vec![]));
    flow.elements.push(element("screen", sink, properties));
    flow.links.push(link(from.into(), "convert".into()));
    flow.links.push(link("convert".into(), "screen".into()));
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

/// Program sound as AAC into `to` (the recorder takes encoded audio only).
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
                ("start-time-selection", PropertyValue::String("first".into())),
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

/// H.264 from program: hardware when the engine has one, else software.
fn add_encoder(flow: &mut Flow, video_kbps: u32) {
    flow.blocks.push(block(
        "enc",
        VIDEO_ENCODER_BLOCK,
        vec![
            ("codec", PropertyValue::String("h264".into())),
            ("encoder_preference", PropertyValue::String("auto".into())),
            ("bitrate", PropertyValue::UInt(u64::from(video_kbps))),
        ],
    ));
    flow.links
        .push(link("pgm:src".into(), "enc:video_in".into()));
}

/// Sound of every slot into one audio mixer; its main mix is program
/// audio (into the vision mixer, which meters it on the multiview).
fn add_audio_mixer(flow: &mut Flow, slots: usize, monitor: MonitorOut) {
    for slot in 0..slots {
        let input = format!("ain{slot}");
        flow.blocks.push(block(
            &input,
            INTER_INPUT_BLOCK,
            vec![
                ("channel", PropertyValue::String(slot_audio_channel(slot))),
                ("max_time", PropertyValue::String(INTER_BUFFER_MS.into())),
            ],
        ));
        flow.links.push(link(
            format!("{input}:src"),
            format!("{AUDIO_MIXER}:input_{}", audio_channel(slot)),
        ));
    }
    flow.blocks.push(block(
        AUDIO_MIXER,
        AUDIO_MIXER_BLOCK,
        vec![
            ("num_channels", PropertyValue::UInt(slots as u64)),
            ("sample_rate", PropertyValue::UInt(AUDIO_RATE)),
            (
                "start_time_selection",
                PropertyValue::String("first".into()),
            ),
        ],
    ));
    flow.links.push(link(
        format!("{AUDIO_MIXER}:main_out"),
        format!("{MIXER_BLOCK}:pgm_audio_in"),
    ));

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

/// One slot: the source, published on the slot's inter channel.
pub fn slot_flow(slot: usize, source: &SourceSpec, backend: MixerBackend) -> Flow {
    let mut flow = Flow::new(format!("lmix slot {slot}"));
    flow.id = slot_id(slot);
    share_timing(&mut flow);

    // Sound-only sources publish no picture: their mixer input stays empty.
    if let Some(out) = add_source(&mut flow, source, backend) {
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
fn add_source(flow: &mut Flow, source: &SourceSpec, backend: MixerBackend) -> Option<String> {
    let pad = match source {
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
        SourceSpec::File { path } => add_player(flow, std::slice::from_ref(path), backend),
        SourceSpec::Url { url } => add_player(flow, std::slice::from_ref(url), backend),
        SourceSpec::Playlist { paths } => add_player(flow, paths, backend),
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

/// Still image: decoded once, repeated live in the pattern format.
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
        vec![("caps", PropertyValue::String(pattern_caps()))],
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

/// Media player with `items`, looped; carries picture and sound.
/// A hardware decoder (NVDEC, VA-API) keeps frames in GPU memory, and the
/// inter channel passes any caps on. Only the CUDA mixer reads them, so for
/// the others the player hands over system memory.
fn add_player(flow: &mut Flow, items: &[String], backend: MixerBackend) -> String {
    let playlist = serde_json::to_string(items).expect("playlist serializes");
    flow.blocks.push(block(
        PLAYER,
        MEDIA_PLAYER_BLOCK,
        vec![
            ("playlist", PropertyValue::String(playlist)),
            ("decode", PropertyValue::Bool(true)),
            ("loop_playlist", PropertyValue::Bool(true)),
            (
                "system_memory",
                PropertyValue::Bool(backend != MixerBackend::Cuda),
            ),
        ],
    ));
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
        SourceSpec::Device { .. } | SourceSpec::Color { .. } | SourceSpec::Image { .. } => {
            return None
        }
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
    use lmix_types::TestPattern;

    #[test]
    fn ids_are_stable_and_distinct() {
        assert_eq!(slot_id(3), slot_id(3));
        assert_ne!(slot_id(3), slot_id(4));
        assert_ne!(slot_id(0), mixer_id());
    }

    #[test]
    fn mixer_subscribes_each_slot_channel() {
        let flow = mixer_flow(3, MixerBackend::Cpu, MonitorOut::None);

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
        let flow = mixer_flow(3, MixerBackend::Cpu, MonitorOut::None);

        assert_eq!(black_input(3), 3);
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "black_caps" && l.to == "vm:video_in_3"));
    }

    #[test]
    fn only_the_gl_mixer_downloads_program() {
        let download = |backend| {
            let flow = mixer_flow(2, backend, MonitorOut::None);
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
            MixerBackend::Cuda,
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
            MixerBackend::Cpu,
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
            MixerBackend::Cpu,
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
        let flow = slot_flow(0, &SourceSpec::Playlist { paths }, MixerBackend::Cpu);

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
            MixerBackend::Cpu,
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
            MixerBackend::Cuda,
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
            MixerBackend::Cpu,
        );

        assert!(flow.blocks.iter().all(|b| b.id != SLOT_AUDIO_OUT));
    }

    #[test]
    fn mixer_mixes_every_slot_sound() {
        let flow = mixer_flow(3, MixerBackend::Cpu, MonitorOut::None);

        for slot in 0..3 {
            let to = format!("mix:input_{}", slot + 1);
            assert!(flow
                .links
                .iter()
                .any(|l| l.from == format!("ain{slot}:src") && l.to == to));
        }
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "mix:main_out" && l.to == "vm:pgm_audio_in"));
        assert!(flow.elements.iter().all(|e| e.id != MONITOR_SINK));

        let heard = mixer_flow(3, MixerBackend::Cpu, MonitorOut::Auto);
        assert!(heard
            .links
            .iter()
            .any(|l| l.from == "mix:monitor_out" && l.to == MONITOR_SINK));
    }

    #[test]
    fn mixer_publishes_program_for_outputs() {
        let flow = mixer_flow(2, MixerBackend::Cpu, MonitorOut::None);

        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "tap_pgm:video_out" && l.to == "pgm_out:sink"));
        assert!(flow
            .links
            .iter()
            .any(|l| l.from == "mix:main_out" && l.to == "pgm_audio_out:sink"));
    }

    #[test]
    fn recording_encodes_picture_and_sound() {
        let spec = OutputSpec::Record {
            folder: "show".into(),
            container: lmix_types::Container::Mkv,
            video_kbps: 8000,
        };
        let flow = output_flow(OutputId::new(), &spec, Feed::Program, MixerBackend::Cpu);

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
            .any(|l| l.from == "aacparse" && l.to == "rec:audio_in_0"));
    }

    #[test]
    fn iso_recording_of_a_silent_input_has_no_audio_track() {
        let spec = OutputSpec::Record {
            folder: "iso".into(),
            container: lmix_types::Container::Mkv,
            video_kbps: 8000,
        };
        let flow = output_flow(
            OutputId::new(),
            &spec,
            Feed::Slot {
                slot: 3,
                audio: false,
            },
            MixerBackend::Cuda,
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
        let flow = output_flow(OutputId::new(), &spec, Feed::Program, MixerBackend::Cpu);

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
        let flow = output_flow(OutputId::new(), &screen, Feed::Program, MixerBackend::Cuda);
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
        };
        let flow = output_flow(OutputId::new(), &record, Feed::Program, MixerBackend::Cuda);
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
            MixerBackend::Cpu,
        );

        for flow in [mixer_flow(2, MixerBackend::Cpu, MonitorOut::None), slot] {
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
            let flow = slot_flow(0, &file, backend);
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

    #[test]
    fn file_source_uses_player() {
        let flow = slot_flow(
            0,
            &SourceSpec::File {
                path: "/a b.mp4".into(),
            },
            MixerBackend::Cpu,
        );
        let player = flow.blocks.iter().find(|b| b.id == "player").unwrap();

        assert!(matches!(
            player.properties.get("playlist"),
            Some(PropertyValue::String(p)) if p == "[\"/a b.mp4\"]"
        ));
        assert!(flow.links.iter().any(|l| l.from == "player:video_out"));
    }
}
