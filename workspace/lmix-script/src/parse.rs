//! A script folder as cue lists (IMP.1–4, IMP.6).
//!
//! Every Markdown file with at least one `lmix-cue` block is a cue list:
//!
//! ````markdown
//! ---
//! lmix-list: Session 3
//! ---
//! ## The forest                      ← a note cue, the scene mark
//!
//! The heroes reach the clearing.
//!
//! ```lmix-cue
//! number = "12"
//! name = "Forest"
//! actions = [{ action = "cut_direct", input = "Forest" }]
//! ```
//! ````
//!
//! A block is one cue in TOML, the fields of the API's cue
//! (`lmix-script schema`). Headings before a cue become note cues that
//! follow on into the cue after them, so GO on a scene mark starts the
//! scene. Media paths are relative to the folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lmix_types::cue::{Cue, CueId, CueKind, Follow};
use uuid::Uuid;

/// Ids of script cues: the same list and number give the same id on
/// every load, so a reload keeps running cues and standby (IMP.5).
const ID_SPACE: Uuid = Uuid::from_u128(0x6c6d_6978_2d73_6372_6970_742d_6375_6573);

/// Info string of a cue block.
const CUE_BLOCK: &str = "lmix-cue";

/// Front matter key of the list name.
const LIST_KEY: &str = "lmix-list";

/// Source of a list from `file` in folder `folder` (IMP.9).
pub fn source_of(folder: &str, file: &Path) -> String {
    format!("script:{folder}/{}", file.display())
}

/// Something wrong in a script, at a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub file: PathBuf,
    pub line: usize,
    pub message: String,
    /// The list is not loaded (a missing media file still is: its cue
    /// fails when it runs, CUE.19).
    pub fatal: bool,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let level = if self.fatal { "error" } else { "warning" };
        write!(
            f,
            "{}:{}: {level}: {}",
            self.file.display(),
            self.line,
            self.message
        )
    }
}

/// One file as a cue list.
#[derive(Clone, Debug)]
pub struct ScriptList {
    /// Relative to the folder.
    pub file: PathBuf,
    pub name: String,
    pub cues: Vec<Cue>,
    /// Line of each cue's block or heading (children: their block's).
    pub lines: Vec<(CueId, usize)>,
}

impl ScriptList {
    /// The line of the cue named `name` (`Cue::display_name`), for
    /// lmix-core's answers.
    pub fn line_of(&self, name: &str) -> Option<usize> {
        fn find(cues: &[Cue], name: &str) -> Option<CueId> {
            cues.iter().find_map(|c| {
                (c.display_name() == name)
                    .then_some(c.id)
                    .or_else(|| find(&c.children, name))
            })
        }
        let id = find(&self.cues, name)?;
        self.lines.iter().find(|(i, _)| *i == id).map(|(_, l)| *l)
    }
}

/// Every list of `folder` and what is wrong with them.
#[derive(Debug, Default)]
pub struct Script {
    pub lists: Vec<ScriptList>,
    pub problems: Vec<Problem>,
}

impl Script {
    /// Lists without a fatal problem.
    pub fn loadable(&self) -> impl Iterator<Item = &ScriptList> {
        self.lists
            .iter()
            .filter(|l| !self.problems.iter().any(|p| p.fatal && p.file == l.file))
    }
}

/// Read the script in `folder` (Markdown files, also in subfolders;
/// hidden ones such as `.obsidian` are skipped).
pub fn read_folder(folder: &Path) -> std::io::Result<Script> {
    let mut files = Vec::new();
    collect(folder, folder, &mut files)?;
    files.sort();
    let mut script = Script::default();
    for rel in files {
        let text = std::fs::read_to_string(folder.join(&rel))?;
        let (list, problems) = parse_file(folder, &rel, &text);
        script.problems.extend(problems);
        script.lists.extend(list);
    }
    check_numbers(&mut script);
    Ok(script)
}

fn collect(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if entry.file_type()?.is_dir() {
            collect(root, &path, out)?;
            continue;
        }
        if path.extension().is_some_and(|e| e == "md") {
            out.push(path.strip_prefix(root).expect("below root").to_path_buf());
        }
    }
    Ok(())
}

/// A number twice in the show is an error at every place (IMP.4); no
/// list with it loads. Lists that do not load anyway do not count.
fn check_numbers(script: &mut Script) {
    fn walk<'a>(cues: &'a [Cue], out: &mut Vec<&'a Cue>) {
        for cue in cues {
            out.push(cue);
            walk(&cue.children, out);
        }
    }
    let broken: Vec<PathBuf> = script
        .problems
        .iter()
        .filter(|p| p.fatal)
        .map(|p| p.file.clone())
        .collect();
    let mut places: HashMap<String, Vec<(PathBuf, usize)>> = HashMap::new();
    for list in script.lists.iter().filter(|l| !broken.contains(&l.file)) {
        let mut cues = Vec::new();
        walk(&list.cues, &mut cues);
        for cue in cues.into_iter().filter(|c| !c.number.is_empty()) {
            let line = list
                .lines
                .iter()
                .find(|(id, _)| *id == cue.id)
                .map_or(0, |(_, l)| *l);
            places
                .entry(cue.number.clone())
                .or_default()
                .push((list.file.clone(), line));
        }
    }
    let mut twice: Vec<Problem> = places
        .into_iter()
        .filter(|(_, at)| at.len() > 1)
        .flat_map(|(number, at)| {
            at.iter()
                .map(|(file, line)| {
                    let others: Vec<String> = at
                        .iter()
                        .filter(|(f, l)| !(f == file && l == line))
                        .map(|(f, l)| format!("{}:{l}", f.display()))
                        .collect();
                    Problem {
                        file: file.clone(),
                        line: *line,
                        message: format!("cue number {number} is also at {}", others.join(", ")),
                        fatal: true,
                    }
                })
                .collect::<Vec<_>>()
        })
        .collect();
    twice.sort_by(|a, b| (&a.file, a.line).cmp(&(&b.file, b.line)));
    script.problems.extend(twice);
}

/// One file: its list (if it has cue blocks) and its problems.
pub fn parse_file(folder: &Path, rel: &Path, text: &str) -> (Option<ScriptList>, Vec<Problem>) {
    let lines: Vec<&str> = text.lines().collect();
    let mut problems = Vec::new();
    let problem = |line: usize, message: String, fatal: bool| Problem {
        file: rel.to_path_buf(),
        line,
        message,
        fatal,
    };

    let mut name = rel
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut i = 0;
    if lines.first().is_some_and(|l| l.trim() == "---") {
        i = 1;
        while i < lines.len() && lines[i].trim() != "---" {
            if let Some((key, value)) = lines[i].split_once(':') {
                if key.trim() == LIST_KEY {
                    name = value.trim().trim_matches('"').to_string();
                }
            }
            i += 1;
        }
        i += 1;
    }

    // Headings wait for the next cue; ones after the last cue are dropped.
    let mut pending: Vec<(String, usize)> = Vec::new();
    let mut cues: Vec<(Cue, usize)> = Vec::new();
    let mut blocks = 0;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();
        if let Some(fence) = fence_of(trimmed) {
            let info = trimmed[fence.len()..].trim();
            let start = i + 1;
            let mut end = start;
            while end < lines.len() && !lines[end].trim_start().starts_with(&fence) {
                end += 1;
            }
            if end == lines.len() {
                problems.push(problem(
                    i + 1,
                    "code block is not closed".into(),
                    info == CUE_BLOCK,
                ));
            }
            if info == CUE_BLOCK {
                blocks += 1;
                let body = lines[start..end.min(lines.len())].join("\n");
                match parse_cue(folder, &body, start + 1) {
                    Ok((cue, warnings)) => {
                        for (heading, at) in pending.drain(..) {
                            cues.push((note(&heading), at));
                        }
                        cues.push((cue, i + 1));
                        problems.extend(warnings.into_iter().map(|(at, m)| problem(at, m, false)));
                    }
                    Err((at, message)) => problems.push(problem(at, message, true)),
                }
            }
            i = end + 1;
            continue;
        }
        if let Some(heading) = heading_of(trimmed) {
            pending.push((heading, i + 1));
        }
        i += 1;
    }
    if blocks == 0 {
        return (None, problems);
    }

    let lines_of: Vec<(CueId, usize)> = Vec::new();
    let mut list = ScriptList {
        file: rel.to_path_buf(),
        name,
        cues: Vec::new(),
        lines: lines_of,
    };
    let mut used: HashMap<String, usize> = HashMap::new();
    for (mut cue, line) in cues {
        set_ids(&list.name, &mut cue, &mut used);
        record_lines(&cue, line, &mut list.lines);
        list.cues.push(cue);
    }
    (Some(list), problems)
}

/// A scene mark: does nothing and starts the cue after it.
fn note(heading: &str) -> Cue {
    Cue {
        name: heading.to_string(),
        follow: Follow::AfterEnd,
        ..Cue::empty(CueKind::Note)
    }
}

/// "```" or "~~~" (three or more) that opens a code block.
fn fence_of(line: &str) -> Option<String> {
    let c = line.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let n = line.chars().take_while(|x| *x == c).count();
    (n >= 3).then(|| c.to_string().repeat(n))
}

/// The text of an ATX heading (`## The forest`).
fn heading_of(line: &str) -> Option<String> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &line[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// Ids from list name and number, or from the name and how often it came
/// before for cues without a number.
fn set_ids(list: &str, cue: &mut Cue, used: &mut HashMap<String, usize>) {
    let key = if cue.number.is_empty() {
        let base = format!("{list}\0name\0{}", cue.display_name());
        let n = used.entry(base.clone()).or_default();
        *n += 1;
        format!("{base}\0{n}")
    } else {
        format!("{list}\0number\0{}", cue.number)
    };
    cue.id = CueId(Uuid::new_v5(&ID_SPACE, key.as_bytes()));
    for child in &mut cue.children {
        set_ids(list, child, used);
    }
}

fn record_lines(cue: &Cue, line: usize, out: &mut Vec<(CueId, usize)>) {
    out.push((cue.id, line));
    for child in &cue.children {
        record_lines(child, line, out);
    }
}

/// Field names a cue may have: the API's.
fn cue_fields() -> Vec<String> {
    let schema = schemars::schema_for!(Cue);
    schema
        .as_value()
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|p| p.keys().cloned().collect())
        .unwrap_or_default()
}

/// A message at a line of the file.
type LineNote = (usize, String);

/// One block: the cue, and warnings with their line; or the error line.
fn parse_cue(
    folder: &Path,
    body: &str,
    first_line: usize,
) -> Result<(Cue, Vec<LineNote>), LineNote> {
    let table: toml::Table = toml::from_str(body).map_err(|e| {
        let line = e
            .span()
            .map_or(0, |s| body[..s.start].matches('\n').count());
        (first_line + line, e.message().to_string())
    })?;
    let fields = cue_fields();
    check_fields(&toml::Value::Table(table.clone()), &fields).map_err(|m| (first_line, m))?;

    let mut value = serde_json::to_value(&table).map_err(|e| (first_line, e.to_string()))?;
    let mut warnings = Vec::new();
    resolve_paths(folder, &mut value, false, &mut |m| {
        warnings.push((first_line, m))
    });
    let cue: Cue = serde_json::from_value(value).map_err(|e| (first_line, e.to_string()))?;
    if cue.kind == CueKind::Note && cue.name.is_empty() && cue.note.is_empty() {
        return Err((first_line, "a note cue needs a name or a note".into()));
    }
    warnings.extend(number_warnings(&cue).into_iter().map(|m| (first_line, m)));
    Ok((cue, warnings))
}

/// Numbers a remote control can send as they are (CUE.1): letters,
/// digits, `.`, `-`, `_`; in this cue and its children.
fn number_warnings(cue: &Cue) -> Vec<String> {
    let plain = |n: &str| {
        n.chars()
            .all(|c| c.is_alphanumeric() || matches!(c, '.' | '-' | '_'))
    };
    let mut out = Vec::new();
    if !cue.number.is_empty() && !plain(&cue.number) {
        out.push(format!(
            "cue number \"{}\" has spaces or signs; OSC and Companion want letters, digits, . - _",
            cue.number
        ));
    }
    for child in &cue.children {
        out.extend(number_warnings(child));
    }
    out
}

/// Unknown fields of a cue and its children: likely typos.
fn check_fields(value: &toml::Value, fields: &[String]) -> Result<(), String> {
    let Some(table) = value.as_table() else {
        return Ok(());
    };
    if let Some(unknown) = table.keys().find(|k| !fields.contains(k)) {
        return Err(format!(
            "unknown field `{unknown}` (lmix-script schema lists them)"
        ));
    }
    if let Some(children) = table.get("children").and_then(|c| c.as_array()) {
        for child in children {
            check_fields(child, fields)?;
        }
    }
    Ok(())
}

/// Media sources' `path`/`paths` relative to the folder; missing files
/// are warnings (IMP.4).
fn resolve_paths(
    folder: &Path,
    value: &mut serde_json::Value,
    media: bool,
    warn: &mut dyn FnMut(String),
) {
    match value {
        serde_json::Value::Object(map) => {
            let media = media
                || map
                    .get("kind")
                    .and_then(|k| k.as_str())
                    .is_some_and(|k| matches!(k, "file" | "image" | "playlist"));
            for (key, v) in map.iter_mut() {
                match (key.as_str(), v) {
                    ("path", serde_json::Value::String(path)) if media => {
                        *path = resolve(folder, path, warn);
                    }
                    ("paths", serde_json::Value::Array(paths)) if media => {
                        for p in paths.iter_mut() {
                            if let serde_json::Value::String(path) = p {
                                *path = resolve(folder, path, warn);
                            }
                        }
                    }
                    (_, v) => resolve_paths(folder, v, false, warn),
                }
            }
        }
        serde_json::Value::Array(items) => {
            for v in items {
                resolve_paths(folder, v, false, warn);
            }
        }
        _ => {}
    }
}

fn resolve(folder: &Path, path: &str, warn: &mut dyn FnMut(String)) -> String {
    let full = if Path::new(path).is_absolute() {
        PathBuf::from(path)
    } else {
        folder.join(path)
    };
    if !full.exists() {
        warn(format!("media file not found: {path}"));
    }
    full.display().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const SESSION: &str = r#"---
tags: [session]
lmix-list: Session 3
---
# Session 3

Notes for the evening, no cue yet.

## The forest

```lmix-cue
number = "12"
name = "Forest"
actions = [{ action = "cut_direct", input = "Forest" }]
```

```lmix-cue
kind = "wait"
duration_ms = 2000
follow = "after_end"
```

```python
# not a heading
```

## Epilogue
"#;

    #[test]
    fn a_file_becomes_a_list() {
        let (list, problems) = parse_file(Path::new("/s"), Path::new("s3.md"), SESSION);
        let list = list.unwrap();

        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(list.name, "Session 3");
        let names: Vec<String> = list.cues.iter().map(|c| c.display_name()).collect();
        assert_eq!(names, ["Session 3", "The forest", "Forest", "Wait 2.0 s"]);
        assert_eq!(list.cues[1].kind, CueKind::Note);
        assert_eq!(
            list.cues[1].follow,
            Follow::AfterEnd,
            "a scene mark starts its scene"
        );
        assert_eq!(list.line_of("Forest"), Some(11));
    }

    #[test]
    fn ids_stay_from_load_to_load() {
        let (a, _) = parse_file(Path::new("/s"), Path::new("s3.md"), SESSION);
        let moved = SESSION.replace(
            "## The forest",
            "## Before\n\n```lmix-cue\nkind = \"note\"\nname = \"x\"\n```\n\n## The forest",
        );
        let (b, _) = parse_file(Path::new("/s"), Path::new("s3.md"), &moved);
        let id = |l: &ScriptList, name: &str| {
            l.cues.iter().find(|c| c.display_name() == name).unwrap().id
        };

        assert_eq!(id(&a.unwrap(), "Forest"), id(&b.unwrap(), "Forest"));
    }

    #[test]
    fn errors_name_their_line() {
        let text = "# A\n\n```lmix-cue\nname = \"x\"\nkind = \"wait\"\nduraton_ms = 5\n```\n\n```lmix-cue\nname = \n```\n";
        let (_, problems) = parse_file(Path::new("/s"), Path::new("a.md"), text);

        assert_eq!(problems.len(), 2, "{problems:?}");
        assert!(problems[0].message.contains("duraton_ms"));
        assert_eq!(problems[0].line, 4);
        assert_eq!(problems[1].line, 10);
        assert!(problems.iter().all(|p| p.fatal));
    }

    #[test]
    fn media_paths_are_relative_to_the_folder() {
        let dir = std::env::temp_dir().join(format!("lmix-script-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("media")).unwrap();
        std::fs::write(dir.join("media/rain.png"), b"").unwrap();
        let text = r#"```lmix-cue
name = "Rain"
actions = [
  { action = "add_input", source = { kind = "image", path = "media/rain.png" } },
  { action = "add_input", source = { kind = "file", path = "media/gone.mp4" } },
]
```
"#;
        let (list, problems) = parse_file(&dir, Path::new("a.md"), text);
        let json = serde_json::to_string(&list.unwrap().cues[0]).unwrap();

        assert!(json.contains(&dir.join("media/rain.png").display().to_string()));
        assert_eq!(problems.len(), 1);
        assert!(!problems[0].fatal, "a missing file still loads");
        assert!(problems[0].message.contains("gone.mp4"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn numbers_with_spaces_are_a_warning() {
        let text =
            "```lmix-cue\nnumber = \"1 a\"\nname = \"x\"\nactions = [{ action = \"cut\" }]\n```\n";
        let (list, problems) = parse_file(Path::new("/s"), Path::new("a.md"), text);

        assert!(list.is_some(), "it still loads");
        assert_eq!(problems.len(), 1);
        assert!(!problems[0].fatal);
        assert!(problems[0].message.contains("\"1 a\""));
    }

    #[test]
    fn numbers_are_unique_over_all_files() {
        let dir = std::env::temp_dir().join(format!("lmix-script-n-{}", std::process::id()));
        std::fs::create_dir_all(dir.join(".obsidian")).unwrap();
        let cue =
            "```lmix-cue\nnumber = \"1\"\nname = \"x\"\nactions = [{ action = \"cut\" }]\n```\n";
        std::fs::write(dir.join("a.md"), cue).unwrap();
        std::fs::write(dir.join("b.md"), format!("# B\n\n{cue}")).unwrap();
        std::fs::write(dir.join(".obsidian/c.md"), cue).unwrap();
        std::fs::write(dir.join("notes.md"), "# Just notes\n").unwrap();

        let script = read_folder(&dir).unwrap();

        assert_eq!(
            script.lists.len(),
            2,
            "hidden folders and files without cues are no lists"
        );
        assert_eq!(script.problems.len(), 2, "both places");
        assert_eq!(script.problems[1].file, PathBuf::from("b.md"));
        assert_eq!(script.problems[1].line, 3);
        assert!(script.problems[1].message.contains("also at a.md:1"));
        assert_eq!(script.loadable().count(), 0);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
