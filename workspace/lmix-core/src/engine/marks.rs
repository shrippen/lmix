//! Recording protocol (REC.8): `mark_recording` adds a line to a CSV file
//! beside each running recording, in the engine's media directory.
//!
//! ```text
//!   mark_recording ──▶ Protocols (lines per recording, in lmix-core)
//!                           │ the whole file after each mark
//!                           ▼
//!   strom: POST /api/media/upload?path=<folder>
//!          <folder>/lmix-3f2a9c1e_20261101_143000.marks.csv
//!
//!   strom's event RecorderFileChanged tells which file it writes now.
//! ```
//!
//! strom names a recording's files `<prefix>_<YYYYmmdd_HHMMSS>_<nnnnn>.<ext>`
//! (one per split, REC.4); the protocol takes that name without the
//! number, so it sorts beside them. lmix-core does not see the media
//! directory (engine and core may run on two machines): the file goes
//! through strom's media API, whole, since strom has no append.

use std::collections::HashMap;

use lmix_types::OutputId;

/// First line of every protocol.
pub const HEADER: &str = "time,recording,file,file_time,text\n";
/// Ending of a protocol's file name.
const SUFFIX: &str = ".marks.csv";

/// One mark for one running recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingMark {
    pub output: OutputId,
    /// Its folder below the media directory (the spec's).
    pub folder: String,
    /// Unix ms when the recording started.
    pub since_ms: u64,
    /// Unix ms of the mark.
    pub at_ms: u64,
    pub text: String,
}

/// The file a recorder writes now, as strom reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecorderFile {
    /// Path below the media directory, e.g. `show/lmix-3f2a9c1e_20261101_143000_00001.mkv`.
    pub path: String,
    /// Unix ms when lmix-core heard of it.
    pub opened_ms: u64,
}

/// The protocol of one recording run, as written last.
#[derive(Debug)]
struct Protocol {
    since_ms: u64,
    /// Path below the media directory.
    path: String,
    text: String,
}

/// Protocols of the recordings marked since they started.
#[derive(Debug, Default)]
pub struct Protocols {
    runs: HashMap<OutputId, Protocol>,
}

impl Protocols {
    /// Path of `mark`'s protocol when it has one for this run already.
    pub fn path_of(&self, mark: &RecordingMark) -> Option<&str> {
        self.runs
            .get(&mark.output)
            .filter(|p| p.since_ms == mark.since_ms)
            .map(|p| p.path.as_str())
    }

    /// Begin the protocol of `mark`'s run at `path`, with what the file
    /// held already (lmix-core restarted during the recording).
    pub fn begin(&mut self, mark: &RecordingMark, path: String, before: Option<String>) {
        let text = before
            .filter(|t| t.starts_with(HEADER))
            .unwrap_or_else(|| HEADER.to_string());
        let protocol = Protocol {
            since_ms: mark.since_ms,
            path,
            text,
        };
        self.runs.insert(mark.output, protocol);
    }

    /// Add `mark`'s line; the protocol's path and whole text. `None`
    /// before `begin`.
    pub fn add(
        &mut self,
        mark: &RecordingMark,
        file: Option<&RecorderFile>,
    ) -> Option<(&str, &str)> {
        let protocol = self
            .runs
            .get_mut(&mark.output)
            .filter(|p| p.since_ms == mark.since_ms)?;
        protocol.text.push_str(&line(mark, file));
        Some((&protocol.path, &protocol.text))
    }
}

/// Where `mark`'s protocol goes: beside the file the recorder writes, or
/// (none reported yet) in its folder, named as strom names its files.
pub fn protocol_path(mark: &RecordingMark, file: Option<&RecorderFile>, prefix: &str) -> String {
    if let Some(file) = file {
        if let Some(stem) = recording_stem(&file.path) {
            return format!("{stem}{SUFFIX}");
        }
    }
    let started = LocalTime::of(mark.since_ms).compact();
    let folder = mark.folder.trim_end_matches('/');
    format!("{folder}/{prefix}_{started}{SUFFIX}")
}

/// `show/lmix-3f2a9c1e_20261101_143000_00001.mkv` without its number and
/// extension: `show/lmix-3f2a9c1e_20261101_143000`.
fn recording_stem(path: &str) -> Option<&str> {
    let (stem, _ext) = path.rsplit_once('.')?;
    let (stem, number) = stem.rsplit_once('_')?;
    let numbered = !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
    numbered.then_some(stem)
}

/// Folder and file name of a path below the media directory.
pub fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// One CSV line: time of day, time into the recording, the file written
/// then and the time into it, the text.
fn line(mark: &RecordingMark, file: Option<&RecorderFile>) -> String {
    let time = LocalTime::of(mark.at_ms).readable();
    let recording = elapsed(mark.since_ms, mark.at_ms);
    let (name, file_time) = match file {
        Some(file) => (
            split_path(&file.path).1.to_string(),
            elapsed(file.opened_ms, mark.at_ms),
        ),
        None => (String::new(), String::new()),
    };
    format!(
        "{time},{recording},{},{file_time},{}\n",
        quoted(&name),
        quoted(&mark.text)
    )
}

/// `h:mm:ss` from `from_ms` to `to_ms`.
fn elapsed(from_ms: u64, to_ms: u64) -> String {
    let secs = to_ms.saturating_sub(from_ms) / 1000;
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// A CSV field in quotes, one line: `say "hi"` → `"say ""hi"""`.
fn quoted(text: &str) -> String {
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    format!("\"{}\"", flat.replace('"', "\"\""))
}

/// Date and time of day on this machine's clock.
struct LocalTime {
    year: i32,
    month: i32,
    day: i32,
    hour: i32,
    minute: i32,
    second: i32,
}

impl LocalTime {
    fn of(unix_ms: u64) -> Self {
        let secs = (unix_ms / 1000) as libc::time_t;
        // SAFETY: an all-zero `tm` is valid; localtime_r only writes into
        // it and is thread safe.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        unsafe { libc::localtime_r(&secs, &mut tm) };
        Self {
            year: tm.tm_year + 1900,
            month: tm.tm_mon + 1,
            day: tm.tm_mday,
            hour: tm.tm_hour,
            minute: tm.tm_min,
            second: tm.tm_sec,
        }
    }

    /// `2026-11-01 14:30:00`
    fn readable(&self) -> String {
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }

    /// `20261101_143000`, as strom puts it in file names.
    fn compact(&self) -> String {
        format!(
            "{:04}{:02}{:02}_{:02}{:02}{:02}",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const START_MS: u64 = 1_793_000_000_000;

    fn mark(at_s: u64, text: &str) -> RecordingMark {
        RecordingMark {
            output: OutputId(uuid::Uuid::nil()),
            folder: "show/".into(),
            since_ms: START_MS,
            at_ms: START_MS + at_s * 1000,
            text: text.into(),
        }
    }

    fn file(number: u32, opened_s: u64) -> RecorderFile {
        RecorderFile {
            path: format!("show/lmix-00000000_20261101_143000_{number:05}.mkv"),
            opened_ms: START_MS + opened_s * 1000,
        }
    }

    #[test]
    fn the_protocol_sits_beside_the_recording() {
        // Any file of the run: the split ones share the name.
        let path = protocol_path(&mark(0, ""), Some(&file(3, 0)), "lmix-00000000");
        assert_eq!(path, "show/lmix-00000000_20261101_143000.marks.csv");

        // Before strom reported a file: its folder, named after the start.
        let path = protocol_path(&mark(0, ""), None, "lmix-00000000");
        let started = LocalTime::of(START_MS).compact();
        assert_eq!(path, format!("show/lmix-00000000_{started}.marks.csv"));
        assert_eq!(started.len(), "20261101_143000".len());
    }

    #[test]
    fn a_line_tells_where_in_the_recording_and_its_file() {
        let line = line(&mark(3725, "Q&A \"live\"\nnow"), Some(&file(2, 3600)));
        let time = LocalTime::of(START_MS + 3_725_000).readable();
        assert_eq!(
            line,
            format!("{time},1:02:05,\"lmix-00000000_20261101_143000_00002.mkv\",0:02:05,\"Q&A \"\"live\"\" now\"\n")
        );
        assert_eq!(time.len(), "2026-11-01 14:30:00".len());

        let line = super::line(&mark(5, ""), None);
        assert!(line.ends_with(",0:00:05,\"\",,\"\"\n"), "{line}");
    }

    #[test]
    fn marks_add_up_per_run() {
        let mut protocols = Protocols::default();
        let first = mark(1, "a");
        assert_eq!(protocols.path_of(&first), None);
        assert!(protocols.add(&first, None).is_none(), "not begun");

        protocols.begin(&first, "show/x.marks.csv".into(), None);
        assert_eq!(protocols.path_of(&first), Some("show/x.marks.csv"));
        protocols.add(&first, None).unwrap();
        let (path, text) = protocols.add(&mark(2, "b"), None).unwrap();
        assert_eq!(path, "show/x.marks.csv");
        assert!(text.starts_with(HEADER));
        assert_eq!(text.lines().count(), 3);

        // A new run of the same output starts a new protocol.
        let next = RecordingMark {
            since_ms: START_MS + 60_000,
            ..mark(61, "c")
        };
        assert_eq!(protocols.path_of(&next), None);

        // What the file held before lmix-core restarted stays; anything
        // else in its place does not.
        let before = format!("{HEADER}earlier\n");
        protocols.begin(&next, "show/y.marks.csv".into(), Some(before));
        let (_, text) = protocols.add(&next, None).unwrap();
        assert_eq!(text.lines().count(), 3);
        protocols.begin(&next, "show/y.marks.csv".into(), Some("junk".into()));
        let (_, text) = protocols.add(&next, None).unwrap();
        assert_eq!(text.lines().count(), 2);
    }

    #[test]
    fn a_path_parts_into_folder_and_name() {
        assert_eq!(split_path("a/b/c.csv"), ("a/b", "c.csv"));
        assert_eq!(split_path("c.csv"), ("", "c.csv"));
        assert_eq!(recording_stem("show/odd.mkv"), None);
    }
}
