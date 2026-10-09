//! What the Console's forms and labels make of their fields: sources and
//! outputs to add, and the short texts on buttons. Pure, so it is tested
//! here and QML only passes strings.

use lmix_types::{
    Container, DisplayMode, Effect, InputColor, Latency, OutputSource, OutputSpec, OutputStats,
    SourceSpec, TestPattern,
};
use serde::Serialize;

/// Transition lengths offered on the transition buttons.
pub const DURATION_PRESETS_MS: [u32; 4] = [250, 500, 1000, 2000];

use lmix_types::output::DEFAULT_VIDEO_KBPS;

/// One choice in a QML list: the value the bridge takes, and its text.
#[derive(Serialize)]
struct Choice {
    value: String,
    label: String,
}

/// Serde name of a unit enum value (`"dip_to_black"`).
fn name_of<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

fn choices(items: impl IntoIterator<Item = (String, String)>) -> String {
    let list: Vec<Choice> = items
        .into_iter()
        .map(|(value, label)| Choice { value, label })
        .collect();
    serde_json::to_string(&list).unwrap_or_default()
}

/// Kinds of source the "add input" form offers, as JSON choices.
pub fn source_kinds() -> String {
    choices(
        SOURCE_KINDS
            .iter()
            .map(|(k, l)| (k.to_string(), l.to_string())),
    )
}

const SOURCE_KINDS: [(&str, &str); 11] = [
    ("pattern", "Test pattern"),
    ("color", "Color"),
    ("image", "Image"),
    ("file", "File"),
    ("playlist", "Playlist"),
    ("url", "URL"),
    ("srt", "SRT"),
    ("ndi", "NDI"),
    ("whip", "WHIP (browser, OBS)"),
    ("device", "Camera / capture"),
    ("audio_device", "Audio (mic, line)"),
];

/// Test patterns, as JSON choices.
pub fn patterns() -> String {
    choices(
        [
            TestPattern::Bars,
            TestPattern::Ball,
            TestPattern::Black,
            TestPattern::Snow,
            TestPattern::Checkers,
        ]
        .map(|p| (name_of(p), name_of(p))),
    )
}

/// Transition effects, as JSON choices.
pub fn effects() -> String {
    choices(Effect::ALL.map(|e| (name_of(e), e.label().to_string())))
}

/// Colour tags of inputs, as JSON choices.
pub fn input_colors() -> String {
    choices(InputColor::ALL.map(|c| (name_of(c), name_of(c))))
}

/// Recording containers, as JSON choices.
pub fn containers() -> String {
    choices(Container::ALL.map(|c| (name_of(c), c.nick().to_uppercase())))
}

/// Kinds of output the "add output" form offers, as JSON choices.
pub fn output_kinds() -> String {
    choices(
        OUTPUT_KINDS
            .iter()
            .map(|(k, l)| (k.to_string(), l.to_string())),
    )
}

const OUTPUT_KINDS: [(&str, &str); 6] = [
    ("srt", "SRT stream"),
    ("record", "Recording"),
    ("ndi", "NDI"),
    ("window", "Screen: window"),
    ("fullscreen", "Screen: fullscreen (Wayland)"),
    ("drm", "Screen: connector (no desktop)"),
];

/// The source the "add input" form describes; `None` while a needed field
/// is empty. `value` is the pattern name, the colour (`#rrggbb`), or the
/// text field (paths of a playlist separated by `;`).
pub fn source_of(kind: &str, value: &str) -> Option<SourceSpec> {
    let text = value.trim().to_string();
    if !matches!(kind, "pattern" | "color") && text.is_empty() {
        return None;
    }
    Some(match kind {
        "pattern" => SourceSpec::Pattern {
            pattern: serde_json::from_value(serde_json::Value::String(text)).ok()?,
        },
        "color" => SourceSpec::Color {
            color: valid_color(&text)?,
        },
        "image" => SourceSpec::Image { path: text },
        "file" => SourceSpec::File { path: text },
        "playlist" => SourceSpec::Playlist {
            paths: text
                .split(';')
                .map(str::trim)
                .filter(|p| !p.is_empty())
                .map(str::to_string)
                .collect(),
        },
        "url" => SourceSpec::Url { url: text },
        "srt" => SourceSpec::Srt { uri: text },
        "ndi" => SourceSpec::Ndi { name: text },
        "whip" => SourceSpec::Whip { endpoint: text },
        "device" => SourceSpec::Device { id: text },
        "audio_device" => SourceSpec::AudioDevice { id: text },
        _ => return None,
    })
}

/// `#rrggbb` in lower case, from QML's colour text (`#RRGGBB` or
/// `#AARRGGBB`).
fn valid_color(text: &str) -> Option<String> {
    let hex = text.strip_prefix('#')?;
    let rgb = match hex.len() {
        6 => hex,
        8 => &hex[2..],
        _ => return None,
    };
    rgb.chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| format!("#{}", rgb.to_lowercase()))
}

/// The output the "add output" form describes; `None` while a needed
/// field is empty.
pub fn output_of(kind: &str, text: &str, container: &str, video_kbps: u32) -> Option<OutputSpec> {
    let text = text.trim().to_string();
    let needs_text = !matches!(kind, "window" | "fullscreen");
    if needs_text && text.is_empty() {
        return None;
    }
    let video_kbps = if video_kbps == 0 {
        DEFAULT_VIDEO_KBPS
    } else {
        video_kbps
    };
    Some(match kind {
        "srt" => OutputSpec::Srt {
            uri: text,
            video_kbps,
        },
        "record" => OutputSpec::Record {
            folder: text,
            container: serde_json::from_value(serde_json::Value::String(container.into()))
                .unwrap_or_default(),
            video_kbps,
        },
        "ndi" => OutputSpec::Ndi { name: text },
        "window" => OutputSpec::Display {
            mode: DisplayMode::Window,
        },
        "fullscreen" => OutputSpec::Display {
            mode: DisplayMode::Fullscreen,
        },
        "drm" => OutputSpec::Display {
            mode: DisplayMode::Drm {
                connector: text.parse().ok()?,
            },
        },
        _ => return None,
    })
}

/// Program, or one input by id (ISO, MultiCorder).
pub fn output_source_of(input: &str) -> OutputSource {
    match input.trim() {
        "" | "program" => OutputSource::Program,
        id => serde_json::from_value(serde_json::Value::String(id.into()))
            .map(OutputSource::Input)
            .unwrap_or(OutputSource::Program),
    }
}

/// Latency result in the toolbar.
pub fn latency_text(latency: Option<&Latency>) -> String {
    match latency.map(|l| (l.median_ms, l.p95_ms)) {
        Some((Some(median), Some(p95))) => format!("PGM {median} ms (p95 {p95} ms)"),
        _ => "PGM latency –".into(),
    }
}

/// Short link state on an output button: rate, or that nobody receives.
/// "Mbit": Kante writes button text in capitals, and "MB/S" would read as
/// bytes.
pub fn link_rate(stats: &OutputStats) -> String {
    if !stats.connected {
        return "no receiver".into();
    }
    format!("{:.1} Mbit/s", f64::from(stats.kbps) / 1000.0)
}

/// Link details for the tooltip.
pub fn link_detail(stats: &OutputStats) -> String {
    let rtt = stats
        .rtt_ms
        .map_or("RTT –".to_string(), |ms| format!("RTT {ms} ms"));
    format!(
        "{} · lost {} · dropped {} · {rtt}",
        link_rate(stats),
        stats.packets_lost,
        stats.packets_dropped
    )
}

/// `h:mm:ss` from `since_ms` to `now_ms` (Unix ms).
pub fn running_time(since_ms: u64, now_ms: u64) -> String {
    let secs = now_ms.saturating_sub(since_ms) / 1000;
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sources_from_the_form() {
        assert_eq!(
            source_of("pattern", "ball"),
            Some(SourceSpec::Pattern {
                pattern: TestPattern::Ball
            })
        );
        assert_eq!(
            source_of("color", "#FF8000"),
            Some(SourceSpec::Color {
                color: "#ff8000".into()
            })
        );
        assert_eq!(
            source_of("color", "#ffff8000"),
            Some(SourceSpec::Color {
                color: "#ff8000".into()
            }),
            "QML writes the alpha first"
        );
        assert_eq!(
            source_of("playlist", " /a.mp4; ;/b.mp4 "),
            Some(SourceSpec::Playlist {
                paths: vec!["/a.mp4".into(), "/b.mp4".into()]
            })
        );
    }

    #[test]
    fn sources_need_their_text() {
        assert_eq!(source_of("file", "  "), None);
        assert_eq!(source_of("color", "orange"), None);
        assert_eq!(source_of("pattern", "nope"), None);
        assert_eq!(source_of("teleport", "x"), None);
    }

    #[test]
    fn outputs_from_the_form() {
        assert_eq!(
            output_of("window", "", "mkv", 0),
            Some(OutputSpec::Display {
                mode: DisplayMode::Window
            })
        );
        assert_eq!(output_of("drm", "", "mkv", 0), None, "needs a connector");
        assert_eq!(output_of("drm", "x", "mkv", 0), None);
        assert_eq!(
            output_of("record", "show", "mp4", 0),
            Some(OutputSpec::Record {
                folder: "show".into(),
                container: Container::Mp4,
                video_kbps: DEFAULT_VIDEO_KBPS
            })
        );
        assert_eq!(output_of("srt", " ", "mkv", 8000), None);
    }

    #[test]
    fn output_source_of_program_or_input() {
        assert_eq!(output_source_of(""), OutputSource::Program);
        let id = lmix_types::InputId::new();
        assert_eq!(output_source_of(&id.to_string()), OutputSource::Input(id));
    }

    #[test]
    fn choices_carry_serde_names_and_labels() {
        let effects: serde_json::Value = serde_json::from_str(&effects()).unwrap();
        assert_eq!(effects[2]["value"], "dip_to_black");
        assert_eq!(effects[2]["label"], "Dip");
        assert!(source_kinds().contains("\"audio_device\""));
        assert!(containers().contains("\"MKV\""));
    }

    #[test]
    fn running_time_counts_hours_minutes_seconds() {
        assert_eq!(running_time(0, 3_723_000), "1:02:03");
        assert_eq!(running_time(10, 5), "0:00:00");
    }

    #[test]
    fn link_texts() {
        let mut stats = OutputStats {
            output: lmix_types::OutputId::new(),
            connected: true,
            kbps: 6200,
            packets_lost: 0,
            packets_dropped: 0,
            rtt_ms: Some(12),
        };
        assert_eq!(link_rate(&stats), "6.2 Mbit/s");
        assert_eq!(
            link_detail(&stats),
            "6.2 Mbit/s · lost 0 · dropped 0 · RTT 12 ms"
        );
        stats.connected = false;
        assert_eq!(link_rate(&stats), "no receiver");
        assert_eq!(latency_text(None), "PGM latency –");
    }
}
