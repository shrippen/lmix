//! What the Console's forms make of their fields: sources and outputs to
//! add, and the values their lists offer. Texts live in QML (`Texts.qml`),
//! so they can be translated. Pure, so it is tested here and QML only
//! passes strings.

use lmix_types::{
    Container, DisplayMode, Effect, InputColor, InputPress, OutputSource, OutputSpec, SourceSpec,
    TestPattern, VideoEncoding,
};
use serde::{Deserialize, Serialize};

/// Transition lengths offered on the transition buttons.
pub const DURATION_PRESETS_MS: [u32; 4] = [250, 500, 1000, 2000];

use lmix_types::output::DEFAULT_VIDEO_KBPS;

/// Serde name of a unit enum value (`"dip_to_black"`).
fn name_of<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

/// Values of a form list as JSON, in order; QML gives them their
/// (translated) texts. `None` for an unknown list.
pub fn values(list: &str) -> Option<String> {
    let names: Vec<String> = match list {
        "sources" => SOURCE_KINDS.iter().map(|k| k.to_string()).collect(),
        "patterns" => [
            TestPattern::Bars,
            TestPattern::Ball,
            TestPattern::Black,
            TestPattern::Snow,
            TestPattern::Checkers,
        ]
        .map(name_of)
        .to_vec(),
        "effects" => Effect::ALL.map(name_of).to_vec(),
        "colors" => InputColor::ALL.map(name_of).to_vec(),
        "presses" => InputPress::ALL.map(name_of).to_vec(),
        "containers" => Container::ALL.map(name_of).to_vec(),
        "outputs" => OUTPUT_KINDS.iter().map(|k| k.to_string()).collect(),
        "durations" => return serde_json::to_string(&DURATION_PRESETS_MS).ok(),
        _ => return None,
    };
    serde_json::to_string(&names).ok()
}

const SOURCE_KINDS: [&str; 13] = [
    "pattern",
    "color",
    "image",
    "file",
    "playlist",
    "url",
    "srt",
    "ndi",
    "whip",
    "web",
    "graphics",
    "device",
    "audio_device",
];

const OUTPUT_KINDS: [&str; 7] = [
    "srt",
    "rtmp",
    "record",
    "ndi",
    "window",
    "fullscreen",
    "drm",
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
        "web" => SourceSpec::Web { url: text },
        "graphics" => SourceSpec::Graphics { graphics: text },
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

/// The add-output form as the Console sends it.
#[derive(Debug, Deserialize)]
pub struct OutputForm {
    pub kind: String,
    #[serde(default)]
    pub text: String,
    /// "program" or an input id.
    pub source: String,
    #[serde(default)]
    pub container: String,
    pub video_kbps: u32,
    #[serde(default)]
    pub name: String,
    /// Aux bus for the sound; 0 keeps the source's own.
    #[serde(default)]
    pub aux: u8,
    /// Codec, rate control, keyframes of SRT and recordings (STR.3).
    #[serde(default)]
    pub encoding: VideoEncoding,
    /// Stream key of an RTMP output, apart from its server.
    #[serde(default)]
    pub key: String,
}

/// The output the "add output" form describes; `None` while a needed
/// field is empty.
pub fn output_of(
    kind: &str,
    text: &str,
    container: &str,
    video_kbps: u32,
    encoding: VideoEncoding,
    key: &str,
) -> Option<OutputSpec> {
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
            encoding,
        },
        "rtmp" => OutputSpec::Rtmp {
            url: text,
            key: key.trim().to_string(),
            video_kbps,
            encoding,
        },
        "record" => OutputSpec::Record {
            folder: text,
            container: serde_json::from_value(serde_json::Value::String(container.into()))
                .unwrap_or_default(),
            video_kbps,
            encoding,
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

/// Program, the multiview, or one input by id (ISO, MultiCorder).
pub fn output_source_of(input: &str) -> OutputSource {
    match input.trim() {
        "" | "program" => OutputSource::Program,
        "multiview" => OutputSource::Multiview,
        id => serde_json::from_value(serde_json::Value::String(id.into()))
            .map(OutputSource::Input)
            .unwrap_or(OutputSource::Program),
    }
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
        assert_eq!(
            source_of("web", " https://example.com "),
            Some(SourceSpec::Web {
                url: "https://example.com".into()
            })
        );
        assert_eq!(
            source_of("graphics", "1"),
            Some(SourceSpec::Graphics {
                graphics: "1".into()
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
            output_of("window", "", "mkv", 0, VideoEncoding::default(), ""),
            Some(OutputSpec::Display {
                mode: DisplayMode::Window
            })
        );
        assert_eq!(
            output_of("drm", "", "mkv", 0, VideoEncoding::default(), ""),
            None,
            "needs a connector"
        );
        assert_eq!(
            output_of("drm", "x", "mkv", 0, VideoEncoding::default(), ""),
            None
        );
        assert_eq!(
            output_of("record", "show", "mp4", 0, VideoEncoding::default(), ""),
            Some(OutputSpec::Record {
                folder: "show".into(),
                container: Container::Mp4,
                video_kbps: DEFAULT_VIDEO_KBPS,
                encoding: VideoEncoding::default(),
            })
        );
        assert_eq!(
            output_of("srt", " ", "mkv", 8000, VideoEncoding::default(), ""),
            None
        );
    }

    #[test]
    fn output_source_of_program_or_input() {
        assert_eq!(output_source_of(""), OutputSource::Program);
        assert_eq!(output_source_of("multiview"), OutputSource::Multiview);
        let id = lmix_types::InputId::new();
        assert_eq!(output_source_of(&id.to_string()), OutputSource::Input(id));
    }

    #[test]
    fn lists_carry_serde_names() {
        let effects: Vec<String> = serde_json::from_str(&values("effects").unwrap()).unwrap();
        assert_eq!(effects[2], "dip_to_black");
        assert!(values("sources").unwrap().contains("\"audio_device\""));
        assert!(values("containers").unwrap().contains("\"mkv\""));
        assert_eq!(
            values("presses").unwrap(),
            r#"["preview_input","cut_direct","quick_play","none"]"#
        );
        assert_eq!(values("durations").unwrap(), "[250,500,1000,2000]");
        assert_eq!(values("nope"), None);
    }

    #[test]
    fn running_time_counts_hours_minutes_seconds() {
        assert_eq!(running_time(0, 3_723_000), "1:02:03");
        assert_eq!(running_time(10, 5), "0:00:00");
    }
}
