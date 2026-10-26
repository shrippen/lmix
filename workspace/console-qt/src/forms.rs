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
        // How many buses a show has at most, and how long their names are.
        "bus_limits" => {
            return Some(
                serde_json::json!({ "buses": lmix_types::BUSES, "name_max": lmix_types::bus::BUS_NAME_MAX })
                    .to_string(),
            )
        }
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
        "web" => SourceSpec::Web {
            url: text,
            css: String::new(),
        },
        "graphics" => SourceSpec::Graphics { graphics: text },
        "device" => SourceSpec::Device {
            id: text,
            resolution: None,
            framerate: None,
        },
        "audio_device" => SourceSpec::AudioDevice { id: text },
        _ => return None,
    })
}

/// Options of a source kind beyond its text, from the form as JSON: a
/// camera's mode. Empty fields: the device's own.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct SourceOptions {
    /// `1920x1080`.
    pub resolution: String,
    /// `30/1`.
    pub framerate: String,
}

/// `source_of` with the form's options (`SourceOptions` as JSON; empty or
/// unreadable: none). A resolution or frame rate that does not read as one
/// is left out.
pub fn source_with_options(kind: &str, value: &str, options: &str) -> Option<SourceSpec> {
    let options: SourceOptions = serde_json::from_str(options).unwrap_or_default();
    let mut source = source_of(kind, value)?;
    if let SourceSpec::Device {
        resolution,
        framerate,
        ..
    } = &mut source
    {
        *resolution = lmix_types::device_resolution(&options.resolution);
        *framerate = lmix_types::device_framerate(&options.framerate);
    }
    Some(source)
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
    /// Id of the bus for the sound; empty keeps the source's own.
    #[serde(default)]
    pub bus: String,
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

/// Program, the preview, the multiview, or one input by id (ISO,
/// MultiCorder).
pub fn output_source_of(input: &str) -> OutputSource {
    match input.trim() {
        "" | "program" => OutputSource::Program,
        "multiview" => OutputSource::Multiview,
        "preview" => OutputSource::Preview,
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

/// Short forms of words in names of inputs and buses, English and German.
const SHORT_WORDS: &[(&str, &str)] = &[
    ("MASTER", "MSTR"),
    ("HEADPHONES", "PHN"),
    ("KOPFHÖRER", "KH"),
    ("CAMERA", "CAM"),
    ("KAMERA", "CAM"),
    ("MICROPHONE", "MIC"),
    ("MIKROFON", "MIC"),
    ("PRESENTER", "PRES"),
    ("MODERATOR", "MOD"),
    ("MODERATION", "MOD"),
    ("GUEST", "GST"),
    ("GAST", "GST"),
    ("PHONE", "TEL"),
    ("TELEFON", "TEL"),
    ("ZUSCHALTUNG", "ZU"),
    ("PRESENTATION", "PRES"),
    ("PRÄSENTATION", "PRÄS"),
    ("PLAYOUT", "MAZ"),
    ("EINSPIELER", "MAZ"),
    ("MUSIC", "MUS"),
    ("MUSIKBETT", "MUSIK"),
    ("STREAM", "STRM"),
    ("RECORDING", "REC"),
    ("AUFNAHME", "REC"),
    ("BAND", "BND"),
    ("IN-EAR", "IEM"),
    ("LAPTOP", "LPT"),
    ("AUDIENCE", "AUD"),
    ("PUBLIKUM", "PUB"),
];

/// `word` split where digits start or end, without the dashes there:
/// "KAMERA-2" is "KAMERA" and "2".
fn digit_runs(word: &str) -> Vec<String> {
    let mut parts: Vec<String> = Vec::new();
    for c in word.chars() {
        let digit = c.is_ascii_digit();
        match parts.last_mut() {
            Some(last) if last.chars().all(|d| d.is_ascii_digit()) == digit => last.push(c),
            _ => parts.push(c.to_string()),
        }
    }
    parts
        .into_iter()
        .map(|part| part.trim_matches('-').to_string())
        .filter(|part| !part.is_empty())
        .collect()
}

/// `name` in at most `max` characters, in capitals, for a narrow strip:
/// first the short forms of known words, then the words without their
/// vowels, then the first word cut and the initials of the others. Numbers
/// stay whole (they tell "Camera 1" from "Camera 2"), even past `max`.
pub fn short_name(name: &str, max: usize) -> String {
    let len = |s: &str| s.chars().count();
    let up = name.trim().to_uppercase();
    if len(&up) <= max {
        return up;
    }
    let fixed = |w: &str| {
        SHORT_WORDS
            .iter()
            .find(|(long, _)| *long == w)
            .map(|(_, short)| *short)
    };
    let short = |w: &str| SHORT_WORDS.iter().any(|(_, s)| *s == w);
    // Digits in a word are a word of their own ("MIKROFON2", "KAMERA-2"),
    // so they stay whole too.
    let mut words: Vec<String> = up
        .split(|c: char| c.is_whitespace() || c == '_')
        .filter(|w| !w.is_empty())
        .flat_map(|w| match fixed(w) {
            Some(short) => vec![short.to_string()],
            None => digit_runs(w)
                .into_iter()
                .map(|part| fixed(&part).map_or(part, str::to_string))
                .collect(),
        })
        .collect();
    if len(&words.join(" ")) <= max {
        return words.join(" ");
    }
    if len(&words.concat()) <= max {
        return words.concat();
    }
    let number = |w: &str| w.chars().all(|c| c.is_ascii_digit());
    for word in &mut words {
        if !number(word) && !short(word) {
            let mut chars = word.chars();
            let first = chars.next().into_iter();
            *word = first
                .chain(chars.filter(|c| !"AEIOUÄÖÜ".contains(*c)))
                .collect();
        }
    }
    if len(&words.concat()) <= max {
        return words.concat();
    }
    let digits: String = words
        .iter()
        .filter(|w| number(w))
        .map(String::as_str)
        .collect();
    let rest: Vec<&String> = words.iter().filter(|w| !number(w)).collect();
    let room = max.saturating_sub(len(&digits)).max(1);
    let letters: String = match rest.as_slice() {
        [] => String::new(),
        [one] => one.to_string(),
        [first, others @ ..] => {
            let cut = room.saturating_sub(others.len()).max(1);
            let initials = others.iter().filter_map(|w| w.chars().next());
            first.chars().take(cut).chain(initials).collect()
        }
    };
    letters.chars().take(room).chain(digits.chars()).collect()
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
                url: "https://example.com".into(),
                css: String::new(),
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
    fn camera_mode_from_the_form() {
        assert_eq!(
            source_with_options(
                "device",
                "/dev/video0",
                r#"{"resolution":"1280x720","framerate":"60/1"}"#
            ),
            Some(SourceSpec::Device {
                id: "/dev/video0".into(),
                resolution: Some("1280x720".into()),
                framerate: Some("60/1".into()),
            })
        );
        assert_eq!(
            source_with_options("device", "cam", r#"{"resolution":"","framerate":"fast"}"#),
            Some(SourceSpec::Device {
                id: "cam".into(),
                resolution: None,
                framerate: None,
            }),
            "empty: the device's own; unreadable: left out"
        );
        assert_eq!(
            source_with_options("file", "/a.mp4", "not json"),
            Some(SourceSpec::File {
                path: "/a.mp4".into()
            }),
            "other kinds ignore the options"
        );
        assert_eq!(source_with_options("device", " ", "{}"), None);
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
        assert_eq!(output_source_of("preview"), OutputSource::Preview);
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
        let limits: serde_json::Value =
            serde_json::from_str(&values("bus_limits").unwrap()).unwrap();
        assert_eq!(limits["buses"], lmix_types::BUSES);
        assert_eq!(limits["name_max"], lmix_types::bus::BUS_NAME_MAX);
        assert_eq!(values("nope"), None);
    }

    #[test]
    fn short_names_fit_and_keep_their_numbers() {
        assert_eq!(short_name("Stream", 6), "STREAM");
        assert_eq!(short_name("Master", 5), "MSTR");
        assert_eq!(short_name("Kopfhörer", 5), "KH");
        assert_eq!(short_name("Headphones", 5), "PHN");
        assert_eq!(short_name("Camera 12", 6), "CAM 12");
        assert_eq!(short_name("Camera 12", 5), "CAM12");
        assert_eq!(short_name("In-Ear Band", 7), "IEM BND");
        assert_eq!(short_name("Mikrofon Gast", 5), "MICG");
        assert_eq!(short_name("Interview", 5), "INTRV");
        assert_eq!(short_name("Laptop Präsentation", 5), "LPTP");
        assert_eq!(short_name("Telefon Zuschaltung Studio", 5), "TELZS");
        assert_eq!(short_name("Atmo Publikum 2", 4), "ATP2");
        assert_eq!(short_name("Kamera 123", 3), "C123");
        assert_eq!(short_name("  ", 3), "");
        assert_eq!(short_name("Mikrofon1", 5), "MIC 1");
        assert_eq!(short_name("Mikrofon2", 4), "MIC2");
        assert_eq!(short_name("Kamera-2", 4), "CAM2");
        assert_eq!(short_name("In-Ear 3", 5), "IEM 3");
        for (name, max) in [
            ("Einspieler Intro", 4),
            ("Wohnzimmer Lampe", 3),
            ("Schönes Grün", 5),
        ] {
            assert!(short_name(name, max).chars().count() <= max, "{name}");
        }
    }

    #[test]
    fn running_time_counts_hours_minutes_seconds() {
        assert_eq!(running_time(0, 3_723_000), "1:02:03");
        assert_eq!(running_time(10, 5), "0:00:00");
    }
}
