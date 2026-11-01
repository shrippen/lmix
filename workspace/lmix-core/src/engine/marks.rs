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
//!   Which file strom writes now: its event RecorderFileChanged; for a
//!   recording lmix-core did not start (it restarted meanwhile) the
//!   newest file of the recording in its folder (GET /api/media).
//! ```
//!
//! strom names a recording's files `<prefix>_<YYYYmmdd_HHMMSS>_<nnnnn>.<ext>`
//! (one per split, REC.4); the protocol takes that name without the
//! number, so it sorts beside them. lmix-core does not see the media
//! directory (engine and core may run on two machines): the file goes
//! through strom's media API, whole, since strom has no append.
//!
//! A protocol begun before strom named any file is provisional: named
//! after the start as lmix-core saw it, it moves beside the recording
//! once the file is known. A file of the protocol's name that is not a
//! protocol (someone saved it in another form) is kept: the protocol
//! takes the next name, `….marks-2.csv`.

use std::collections::HashMap;

use lmix_types::OutputId;

/// First line of every protocol.
pub const HEADER: &str = "time,recording,file,file_time,text\n";
/// Ending of a protocol's file name.
const SUFFIX: &str = ".marks.csv";
/// Byte order mark a spreadsheet may put in front when it saves.
const BOM: char = '\u{feff}';
/// First characters a spreadsheet reads as a formula (CSV injection).
const FORMULA_STARTS: [char; 6] = ['=', '+', '-', '@', '\t', '\r'];
/// Digits of the date, the time and the number in a recording's name.
const STAMP_DIGITS: [usize; 3] = [8, 6, 5];

/// One mark for one running recording.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordingMark {
    pub output: OutputId,
    /// Its folder below the media directory (the spec's).
    pub folder: String,
    /// Unix ms when the recording started, as lmix-core knows it (after a
    /// restart of lmix-core: when it found it running).
    pub since_ms: u64,
    /// Unix ms of the mark.
    pub at_ms: u64,
    pub text: String,
}

/// The file a recorder writes now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecorderFile {
    /// Path below the media directory, e.g. `show/lmix-3f2a9c1e_20261101_143000_00001.mkv`.
    pub path: String,
    /// Unix ms when it began; `None` when unknown (found in the folder).
    pub opened_ms: Option<u64>,
}

/// How a protocol begins.
#[derive(Debug)]
pub struct Begin {
    /// Path below the media directory.
    pub path: String,
    /// What a protocol of that name held already (`own_text`).
    pub before: Option<String>,
    /// Unix ms the column `recording` counts from.
    pub start_ms: u64,
    pub provisional: Provisional,
}

/// Whether a protocol's name is a guess (no file known yet).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provisional {
    Yes,
    No,
}

/// The protocol of one recording run, as written last.
#[derive(Debug)]
struct Protocol {
    since_ms: u64,
    path: String,
    text: String,
    start_ms: u64,
    provisional: Provisional,
    /// The guessed file it moved from, still to delete.
    stale: Option<String>,
}

/// Protocols of the recordings marked since they started.
#[derive(Debug, Default)]
pub struct Protocols {
    runs: HashMap<OutputId, Protocol>,
}

impl Protocols {
    fn run(&self, mark: &RecordingMark) -> Option<&Protocol> {
        let protocol = self.runs.get(&mark.output)?;
        (protocol.since_ms == mark.since_ms).then_some(protocol)
    }

    fn run_mut(&mut self, mark: &RecordingMark) -> Option<&mut Protocol> {
        let protocol = self.runs.get_mut(&mark.output)?;
        (protocol.since_ms == mark.since_ms).then_some(protocol)
    }

    /// Whether `mark`'s run has a protocol, and whether its name is a guess.
    pub fn state(&self, mark: &RecordingMark) -> Option<Provisional> {
        self.run(mark).map(|p| p.provisional)
    }

    /// Begin the protocol of `mark`'s run.
    pub fn begin(&mut self, mark: &RecordingMark, begin: Begin) {
        let protocol = Protocol {
            since_ms: mark.since_ms,
            path: begin.path,
            text: begin.before.unwrap_or_else(|| HEADER.to_string()),
            start_ms: begin.start_ms,
            provisional: begin.provisional,
            stale: None,
        };
        self.runs.insert(mark.output, protocol);
    }

    /// Whether the provisional protocol of `mark`'s run lies under one of
    /// the names from `base` on (`numbered`): the guess was right.
    pub fn named_from(&self, mark: &RecordingMark, base: &str, names: u32) -> bool {
        self.run(mark)
            .is_some_and(|p| (1..=names).any(|n| p.path == numbered(base, n)))
    }

    /// The name of `mark`'s protocol is no guess any more.
    pub fn confirm(&mut self, mark: &RecordingMark) {
        if let Some(protocol) = self.run_mut(mark) {
            protocol.provisional = Provisional::No;
        }
    }

    /// The provisional protocol of `mark`'s run goes to `path`, after what
    /// a protocol there held; the old file is to delete (`take_stale`).
    pub fn relocate(&mut self, mark: &RecordingMark, path: String, before: Option<String>) {
        let Some(protocol) = self.run_mut(mark) else {
            return;
        };
        let lines = protocol.text.strip_prefix(HEADER).unwrap_or(&protocol.text);
        let mut text = before.unwrap_or_else(|| HEADER.to_string());
        text.push_str(lines);
        protocol.text = text;
        protocol.provisional = Provisional::No;
        let old = std::mem::replace(&mut protocol.path, path);
        protocol.stale = Some(old);
    }

    /// The guessed file `mark`'s protocol left, to delete now; given back
    /// with `keep_stale` when that fails, so a later mark tries again.
    pub fn take_stale(&mut self, mark: &RecordingMark) -> Option<String> {
        self.run_mut(mark)?.stale.take()
    }

    pub fn keep_stale(&mut self, mark: &RecordingMark, path: String) {
        if let Some(protocol) = self.run_mut(mark) {
            protocol.stale = Some(path);
        }
    }

    /// Add `mark`'s line; the protocol's path and whole text, and its
    /// length before (`undo`). `None` before `begin`.
    pub fn add(
        &mut self,
        mark: &RecordingMark,
        file: Option<&RecorderFile>,
    ) -> Option<(&str, &str, usize)> {
        let protocol = self.run_mut(mark)?;
        let before = protocol.text.len();
        protocol.text.push_str(&line(mark, protocol.start_ms, file));
        Some((&protocol.path, &protocol.text, before))
    }

    /// Take back what `add` added (the upload failed): a mark sent again
    /// stands once.
    pub fn undo(&mut self, mark: &RecordingMark, len: usize) {
        if let Some(protocol) = self.run_mut(mark) {
            protocol.text.truncate(len);
        }
    }
}

/// The protocol's name beside `file`, e.g. `show/lmix-…_143000.marks.csv`.
pub fn protocol_beside(file: &RecorderFile) -> Option<String> {
    recording_stem(&file.path).map(|stem| format!("{stem}{SUFFIX}"))
}

/// A provisional name: `mark`'s folder, named as strom names its files,
/// after the start as lmix-core saw it.
pub fn protocol_guess(mark: &RecordingMark, prefix: &str) -> String {
    let started = LocalTime::of(mark.since_ms).compact();
    let folder = mark.folder.trim_end_matches('/');
    format!("{folder}/{prefix}_{started}{SUFFIX}")
}

/// The `n`th name for a protocol (from 1): `x.marks.csv`, `x.marks-2.csv`.
pub fn numbered(path: &str, n: u32) -> String {
    match (n, path.strip_suffix(SUFFIX)) {
        (2.., Some(stem)) => format!("{stem}.marks-{n}.csv"),
        _ => path.to_string(),
    }
}

/// A protocol file's text as lmix-core goes on with it, line ends as it
/// writes them; `None` when it is no protocol (another first line).
/// Tolerates what a spreadsheet does on saving: a byte order mark, CRLF.
pub fn own_text(text: &str) -> Option<String> {
    let text = text.trim_start_matches(BOM).replace("\r\n", "\n");
    if !text.starts_with(HEADER) {
        return None;
    }
    let mut text = text;
    if !text.ends_with('\n') {
        text.push('\n');
    }
    Some(text)
}

/// The newest file of the recording with `prefix` among a folder's files
/// (path, last written in Unix s), e.g.
/// `show/lmix-3f2a9c1e_20261101_143000_00002.mkv`: the one written last
/// (the time in a name goes back with the clock at the end of daylight
/// saving time), then by name (fixed widths: time, then number).
pub fn newest_recording<'a>(files: &[(&'a str, u64)], prefix: &str) -> Option<&'a str> {
    files
        .iter()
        .filter(|(path, _)| recording_name(split_path(path).1, prefix))
        .max_by_key(|(path, modified)| (*modified, split_path(path).1))
        .map(|(path, _)| *path)
}

/// The file found in its folder: begun with the recording if it is the
/// first, else when is unknown.
pub fn found_file(path: &str) -> RecorderFile {
    let first = recording_stem(path).filter(|_| {
        path.rsplit_once('.')
            .is_some_and(|(s, _)| s.ends_with("_00000"))
    });
    RecorderFile {
        path: path.to_string(),
        opened_ms: first.and_then(stem_start_ms),
    }
}

/// Whether `name` is `<prefix>_<8 digits>_<6 digits>_<5 digits>.<ext>`.
fn recording_name(name: &str, prefix: &str) -> bool {
    let Some(rest) = name.strip_prefix(prefix).and_then(|r| r.strip_prefix('_')) else {
        return false;
    };
    let Some((stamp, ext)) = rest.rsplit_once('.') else {
        return false;
    };
    let parts: Vec<&str> = stamp.split('_').collect();
    let digits = |(part, len): (&&str, &usize)| {
        part.len() == *len && part.chars().all(|c| c.is_ascii_digit())
    };
    ext != "csv"
        && parts.len() == STAMP_DIGITS.len()
        && parts.iter().zip(STAMP_DIGITS.iter()).all(digits)
}

/// When a recording began, from its stem `…_20261101_143000` read on this
/// machine's clock; `None` when it holds no time.
pub fn stem_start_ms(stem: &str) -> Option<u64> {
    let (rest, time) = stem.rsplit_once('_')?;
    let (_, date) = rest.rsplit_once('_')?;
    let number = |s: &str, at: usize, len: usize| s.get(at..at + len)?.parse::<i32>().ok();
    let local = LocalTime {
        year: number(date, 0, 4)?,
        month: number(date, 4, 2)?,
        day: number(date, 6, 2)?,
        hour: number(time, 0, 2)?,
        minute: number(time, 2, 2)?,
        second: number(time, 4, 2)?,
    };
    (date.len() == 8 && time.len() == 6).then(|| local.unix_ms())?
}

/// `show/lmix-3f2a9c1e_20261101_143000_00001.mkv` without its number and
/// extension: `show/lmix-3f2a9c1e_20261101_143000`.
pub fn recording_stem(path: &str) -> Option<&str> {
    let (stem, _ext) = path.rsplit_once('.')?;
    let (stem, number) = stem.rsplit_once('_')?;
    let numbered = !number.is_empty() && number.chars().all(|c| c.is_ascii_digit());
    numbered.then_some(stem)
}

/// Folder and file name of a path below the media directory.
pub fn split_path(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

/// One CSV line: time of day, time into the recording (from `start_ms`),
/// the file written then and the time into it, the text.
fn line(mark: &RecordingMark, start_ms: u64, file: Option<&RecorderFile>) -> String {
    let time = LocalTime::of(mark.at_ms).readable();
    let recording = elapsed(start_ms, mark.at_ms);
    let name = file.map_or("", |f| split_path(&f.path).1);
    let file_time = file
        .and_then(|f| f.opened_ms)
        .map(|opened| elapsed(opened, mark.at_ms))
        .unwrap_or_default();
    format!(
        "{time},{recording},{},{file_time},{}\n",
        quoted(name),
        quoted(&mark.text)
    )
}

/// `h:mm:ss` from `from_ms` to `to_ms`.
fn elapsed(from_ms: u64, to_ms: u64) -> String {
    let secs = to_ms.saturating_sub(from_ms) / 1000;
    format!("{}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60)
}

/// A CSV field in quotes, one line: `say "hi"` → `"say ""hi"""`. Text a
/// spreadsheet would read as a formula gets a `'` in front (`=1+1` →
/// `'=1+1`).
fn quoted(text: &str) -> String {
    let formula = text.starts_with(FORMULA_STARTS);
    let flat: String = text
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    let guard = if formula { "'" } else { "" };
    format!("\"{guard}{}\"", flat.replace('"', "\"\""))
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

    /// Unix ms of this local time; `None` when it is none.
    fn unix_ms(&self) -> Option<u64> {
        // SAFETY: an all-zero `tm` is valid; mktime reads and normalises
        // only it.
        let mut tm: libc::tm = unsafe { std::mem::zeroed() };
        tm.tm_year = self.year - 1900;
        tm.tm_mon = self.month - 1;
        tm.tm_mday = self.day;
        tm.tm_hour = self.hour;
        tm.tm_min = self.minute;
        tm.tm_sec = self.second;
        // Daylight saving time as it was then.
        tm.tm_isdst = -1;
        let secs = unsafe { libc::mktime(&mut tm) };
        u64::try_from(secs).ok().map(|s| s * 1000)
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
            opened_ms: Some(START_MS + opened_s * 1000),
        }
    }

    fn begin(path: &str, before: Option<String>, provisional: Provisional) -> Begin {
        Begin {
            path: path.into(),
            before,
            start_ms: START_MS,
            provisional,
        }
    }

    #[test]
    fn the_protocol_sits_beside_the_recording() {
        // Any file of the run: the split ones share the name.
        let path = protocol_beside(&file(3, 0)).unwrap();
        assert_eq!(path, "show/lmix-00000000_20261101_143000.marks.csv");

        // Before strom reported a file: its folder, named after the start.
        let path = protocol_guess(&mark(0, ""), "lmix-00000000");
        let started = LocalTime::of(START_MS).compact();
        assert_eq!(path, format!("show/lmix-00000000_{started}.marks.csv"));
        assert_eq!(started.len(), "20261101_143000".len());

        assert_eq!(numbered(&path, 1), path);
        assert_eq!(
            numbered("show/x.marks.csv", 2),
            "show/x.marks-2.csv".to_string()
        );
    }

    #[test]
    fn a_line_tells_where_in_the_recording_and_its_file() {
        let line = line(
            &mark(3725, "Q&A \"live\"\nnow"),
            START_MS,
            Some(&file(2, 3600)),
        );
        let time = LocalTime::of(START_MS + 3_725_000).readable();
        assert_eq!(
            line,
            format!("{time},1:02:05,\"lmix-00000000_20261101_143000_00002.mkv\",0:02:05,\"Q&A \"\"live\"\" now\"\n")
        );
        assert_eq!(time.len(), "2026-11-01 14:30:00".len());

        let line = super::line(&mark(5, ""), START_MS, None);
        assert!(line.ends_with(",0:00:05,\"\",,\"\"\n"), "{line}");

        // A file found in the folder: when it began is unknown.
        let found = RecorderFile {
            opened_ms: None,
            ..file(2, 0)
        };
        let line = super::line(&mark(5, ""), START_MS, Some(&found));
        assert!(line.ends_with("_00002.mkv\",,\"\"\n"), "{line}");
    }

    // A spreadsheet must not run a note as a formula.
    #[test]
    fn a_note_is_never_a_formula() {
        for text in ["=HYPERLINK(\"x\")", "+1", "-1", "@SUM(A1)", "\tx", "\rx"] {
            let field = quoted(text);
            assert!(field.starts_with("\"'"), "{text:?}: {field}");
        }
        assert_eq!(quoted("Q&A = live"), "\"Q&A = live\"");
    }

    #[test]
    fn marks_add_up_per_run() {
        let mut protocols = Protocols::default();
        let first = mark(1, "a");
        assert_eq!(protocols.state(&first), None);
        assert!(protocols.add(&first, None).is_none(), "not begun");

        protocols.begin(&first, begin("show/x.marks.csv", None, Provisional::No));
        assert_eq!(protocols.state(&first), Some(Provisional::No));
        protocols.add(&first, None).unwrap();
        let (path, text, before) = protocols.add(&mark(2, "b"), None).unwrap();
        assert_eq!(path, "show/x.marks.csv");
        assert!(text.starts_with(HEADER));
        assert_eq!(text.lines().count(), 3);

        // A failed upload takes its line back.
        protocols.undo(&first, before);
        let (_, text, _) = protocols.add(&mark(3, "c"), None).unwrap();
        assert_eq!(text.lines().count(), 3);

        // A new run of the same output starts a new protocol.
        let next = RecordingMark {
            since_ms: START_MS + 60_000,
            ..mark(61, "c")
        };
        assert_eq!(protocols.state(&next), None);

        // What a protocol held before stays.
        let before = own_text(&format!("{HEADER}earlier\n"));
        protocols.begin(&next, begin("show/y.marks.csv", before, Provisional::No));
        let (_, text, _) = protocols.add(&next, None).unwrap();
        assert_eq!(text.lines().count(), 3);
    }

    #[test]
    fn a_guessed_protocol_moves_beside_the_recording() {
        let mut protocols = Protocols::default();
        let first = mark(1, "a");
        protocols.begin(
            &first,
            begin("show/guess.marks.csv", None, Provisional::Yes),
        );
        protocols.add(&first, None).unwrap();

        assert!(!protocols.named_from(&first, "show/real.marks.csv", 2));
        protocols.relocate(&first, "show/real.marks.csv".into(), None);
        assert_eq!(protocols.state(&first), Some(Provisional::No));
        let stale = protocols.take_stale(&first);
        assert_eq!(stale.as_deref(), Some("show/guess.marks.csv"));
        assert_eq!(protocols.take_stale(&first), None, "taken once");
        let (path, text, _) = protocols.add(&mark(2, "b"), None).unwrap();
        assert_eq!(path, "show/real.marks.csv");
        assert!(text.starts_with(HEADER));
        assert_eq!(text.matches(HEADER).count(), 1);
        assert_eq!(text.lines().count(), 3);

        // A right guess, also on a further name, stays where it is.
        let other = RecordingMark {
            output: OutputId::new(),
            ..mark(1, "x")
        };
        protocols.begin(&other, begin("show/s.marks-2.csv", None, Provisional::Yes));
        assert!(protocols.named_from(&other, "show/s.marks.csv", 2));
        assert!(!protocols.named_from(&other, "show/s.marks.csv", 1));
        protocols.confirm(&other);
        assert_eq!(protocols.state(&other), Some(Provisional::No));
    }

    // Saved again by a spreadsheet: still the protocol; anything else is
    // not one.
    #[test]
    fn a_protocol_saved_elsewhere_is_still_one() {
        let saved = format!("{BOM}{}earlier", HEADER.replace('\n', "\r\n"));
        assert_eq!(own_text(&saved), Some(format!("{HEADER}earlier\n")));
        assert_eq!(own_text("time;recording;file\r\n"), None);
        assert_eq!(own_text(""), None);
    }

    #[test]
    fn the_newest_file_of_the_recording_is_found() {
        let paths = [
            ("show/lmix-00000000_20261101_120000_00004.mkv", 10),
            ("show/lmix-00000000_20261101_143000_00001.mkv", 20),
            ("show/lmix-00000000_20261101_143000_00000.mkv", 20),
            ("show/lmix-00000000_20261101_143000.marks.csv", 30),
            ("show/lmix-11111111_20261101_150000_00000.mkv", 30),
            ("show/other.mkv", 30),
        ];
        assert_eq!(
            newest_recording(&paths, "lmix-00000000"),
            Some("show/lmix-00000000_20261101_143000_00001.mkv")
        );
        assert_eq!(newest_recording(&paths[3..4], "lmix-00000000"), None);
        // The clock went back: written last counts, not the later name.
        let back = [
            ("show/lmix-00000000_20261025_023000_00000.mkv", 100),
            ("show/lmix-00000000_20261025_021500_00000.mkv", 200),
        ];
        assert_eq!(newest_recording(&back, "lmix-00000000"), Some(back[1].0));

        // The first file began with the recording, a later one when is unknown.
        let first = found_file("show/lmix-00000000_20261101_143000_00000.mkv");
        let start = stem_start_ms("show/lmix-00000000_20261101_143000").unwrap();
        assert_eq!(first.opened_ms, Some(start));
        assert_eq!(found_file(paths[1].0).opened_ms, None);
    }

    #[test]
    fn a_stem_tells_when_the_recording_began() {
        let start = stem_start_ms("show/lmix-00000000_20261101_143000").unwrap();
        assert_eq!(LocalTime::of(start).compact(), "20261101_143000");
        assert_eq!(stem_start_ms("show/odd"), None);
        assert_eq!(stem_start_ms("show/x_2026_1430"), None);
    }

    #[test]
    fn a_path_parts_into_folder_and_name() {
        assert_eq!(split_path("a/b/c.csv"), ("a/b", "c.csv"));
        assert_eq!(split_path("c.csv"), ("", "c.csv"));
        assert_eq!(recording_stem("show/odd.mkv"), None);
    }
}
