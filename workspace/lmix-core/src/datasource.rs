//! Reading data sources (DAT.1–3): CSV and JSON files into columns and
//! rows of text.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use lmix_types::DataSourceSpec;

use crate::switcher::Rows;

/// Largest file read.
const MAX_BYTES: u64 = 16 * 1024 * 1024;

/// The file's path; relative paths count from the show file's folder.
pub fn resolve(spec: &DataSourceSpec, show: Option<&str>) -> PathBuf {
    let path = Path::new(spec.path());
    match show.and_then(|s| Path::new(s).parent()) {
        Some(dir) if path.is_relative() => dir.join(path),
        _ => path.to_path_buf(),
    }
}

/// When the file last changed, if it can be read.
pub async fn modified(path: &Path) -> Option<SystemTime> {
    tokio::fs::metadata(path).await.ok()?.modified().ok()
}

/// Columns and rows of the file, or why it could not be read.
pub async fn read(spec: &DataSourceSpec, path: &Path) -> Rows {
    let failed = |error: String| Rows {
        error: Some(error),
        ..Rows::default()
    };
    match tokio::fs::metadata(path).await {
        Ok(meta) if meta.len() > MAX_BYTES => {
            return failed(format!("{} is larger than 16 MB", path.display()))
        }
        Ok(_) => {}
        Err(e) => return failed(format!("{}: {e}", path.display())),
    }
    let text = match tokio::fs::read(path).await {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        Err(e) => return failed(format!("{}: {e}", path.display())),
    };
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let parsed = match spec {
        DataSourceSpec::Csv { header, .. } => Ok(csv(text, *header)),
        DataSourceSpec::Json { .. } => json(text),
    };
    match parsed {
        Ok((columns, rows)) => Rows {
            columns,
            rows,
            error: None,
        },
        Err(error) => failed(error),
    }
}

/// The separator the first line uses most, outside quotes.
fn separator(text: &str) -> char {
    let first = text.lines().next().unwrap_or_default();
    let mut quoted = false;
    let mut counts = [(',', 0), (';', 0), ('\t', 0)];
    for c in first.chars() {
        if c == '"' {
            quoted = !quoted;
        } else if !quoted {
            for (sep, n) in &mut counts {
                if c == *sep {
                    *n += 1;
                }
            }
        }
    }
    counts
        .iter()
        .max_by_key(|(_, n)| *n)
        .filter(|(_, n)| *n > 0)
        .map_or(',', |(c, _)| *c)
}

/// Records of a CSV text: quoted fields may hold separators, line breaks
/// and doubled quotes.
fn records(text: &str, sep: char) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut record = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    chars.next();
                    field.push('"');
                }
                '"' => quoted = false,
                _ => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            '\r' => {}
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
            }
            c if c == sep => record.push(std::mem::take(&mut field)),
            _ => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    // Blank lines are no rows.
    records.retain(|r| !(r.len() == 1 && r[0].is_empty()));
    records
}

/// Column names (from the first row, or numbered) and rows.
fn csv(text: &str, header: bool) -> (Vec<String>, Vec<Vec<String>>) {
    let mut rows = records(text, separator(text));
    let width = rows.iter().map(Vec::len).max().unwrap_or(0);
    let columns: Vec<String> = if header && !rows.is_empty() {
        let mut names = rows.remove(0);
        names.resize_with(width, String::new);
        names
            .into_iter()
            .enumerate()
            .map(|(i, n)| {
                if n.trim().is_empty() {
                    (i + 1).to_string()
                } else {
                    n.trim().to_string()
                }
            })
            .collect()
    } else {
        (1..=width).map(|i| i.to_string()).collect()
    };
    for row in &mut rows {
        row.resize_with(columns.len(), String::new);
    }
    (columns, rows)
}

/// A JSON object with its keys in file order (serde_json's map sorts
/// them).
struct Ordered(Vec<(String, serde_json::Value)>);

impl<'de> serde::Deserialize<'de> for Ordered {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visit;
        impl<'de> serde::de::Visitor<'de> for Visit {
            type Value = Ordered;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("an object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> Result<Ordered, A::Error> {
                let mut entries = Vec::new();
                while let Some(entry) = map.next_entry()? {
                    entries.push(entry);
                }
                Ok(Ordered(entries))
            }
        }
        deserializer.deserialize_map(Visit)
    }
}

/// A JSON array of objects: keys in order of first appearance.
fn json(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let items: Vec<Ordered> =
        serde_json::from_str(text).map_err(|_| "not a JSON array of objects".to_string())?;
    let mut columns: Vec<String> = Vec::new();
    for item in &items {
        for (key, _) in &item.0 {
            if !columns.contains(key) {
                columns.push(key.clone());
            }
        }
    }
    let text_of = |v: Option<&serde_json::Value>| match v {
        None | Some(serde_json::Value::Null) => String::new(),
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(other) => other.to_string(),
    };
    let rows = items
        .iter()
        .map(|item| {
            columns
                .iter()
                .map(|c| text_of(item.0.iter().find(|(k, _)| k == c).map(|(_, v)| v)))
                .collect()
        })
        .collect();
    Ok((columns, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn csv_with_quotes_and_header() {
        let (columns, rows) = csv(
            "name,role\r\n\"A, B\",\"says \"\"hi\"\"\"\n\"two\nlines\",x\n\n",
            true,
        );

        assert_eq!(columns, s(&["name", "role"]));
        assert_eq!(rows, [s(&["A, B", "says \"hi\""]), s(&["two\nlines", "x"])]);
    }

    #[test]
    fn csv_guesses_semicolon_and_tab() {
        assert_eq!(csv("a;b\n1;2", true).1, [s(&["1", "2"])]);
        assert_eq!(csv("a\tb\n1\t2", true).1, [s(&["1", "2"])]);
    }

    #[test]
    fn csv_without_header_numbers_columns_and_pads_rows() {
        let (columns, rows) = csv("1,2,3\n4", false);

        assert_eq!(columns, s(&["1", "2", "3"]));
        assert_eq!(rows[1], s(&["4", "", ""]));
    }

    #[test]
    fn json_array_of_objects() {
        let (columns, rows) =
            json(r#"[{"name":"A","n":1},{"name":"B","x":true,"n":null}]"#).unwrap();

        assert_eq!(columns, s(&["name", "n", "x"]));
        assert_eq!(rows, [s(&["A", "1", ""]), s(&["B", "", "true"])]);
        assert!(json(r#"{"a":1}"#).is_err());
        assert!(json("[1]").is_err());
    }

    #[tokio::test]
    async fn reads_files_relative_to_the_show() {
        let dir = tempfile::tempdir().unwrap();
        tokio::fs::write(dir.path().join("g.csv"), "\u{feff}name\nA\n")
            .await
            .unwrap();
        let spec = DataSourceSpec::Csv {
            path: "g.csv".into(),
            header: true,
        };
        let show = dir.path().join("show.lmix").display().to_string();

        let path = resolve(&spec, Some(&show));
        let rows = read(&spec, &path).await;

        assert_eq!((rows.columns, rows.rows), (s(&["name"]), vec![s(&["A"])]));
        let missing = read(&spec, &dir.path().join("nope.csv")).await;
        assert!(missing.error.is_some());
    }
}
