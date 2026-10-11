//! Reading data sources (DAT.1–3): CSV, spreadsheets (XLSX, ODS), JSON,
//! text, RSS/Atom and XML, from a file or an address on the network, into
//! columns and rows of text.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use lmix_types::DataSourceSpec;

use crate::switcher::Rows;

/// Largest file or download read.
const MAX_BYTES: u64 = 16 * 1024 * 1024;
/// How long a download may take.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Where the data comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Place {
    File(PathBuf),
    /// `http://` or `https://`.
    Url(String),
}

impl Place {
    pub fn is_url(&self) -> bool {
        matches!(self, Self::Url(_))
    }
}

/// The source's place; relative paths count from the show file's folder.
pub fn resolve(spec: &DataSourceSpec, show: Option<&str>) -> Place {
    let text = spec.path();
    if text.starts_with("http://") || text.starts_with("https://") {
        return Place::Url(text.to_string());
    }
    let path = Path::new(text);
    Place::File(match show.and_then(|s| Path::new(s).parent()) {
        Some(dir) if path.is_relative() => dir.join(path),
        _ => path.to_path_buf(),
    })
}

/// When a file last changed, if it can be read (addresses: none).
pub async fn modified(place: &Place) -> Option<SystemTime> {
    match place {
        Place::File(path) => tokio::fs::metadata(path).await.ok()?.modified().ok(),
        Place::Url(_) => None,
    }
}

/// A client for addresses.
pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(FETCH_TIMEOUT)
        .build()
        .expect("HTTP client")
}

async fn bytes(place: &Place, http: &reqwest::Client) -> Result<Vec<u8>, String> {
    match place {
        Place::File(path) => {
            let shown = path.display();
            let meta = tokio::fs::metadata(path)
                .await
                .map_err(|e| format!("{shown}: {e}"))?;
            if meta.len() > MAX_BYTES {
                return Err(format!("{shown} is larger than 16 MB"));
            }
            tokio::fs::read(path)
                .await
                .map_err(|e| format!("{shown}: {e}"))
        }
        Place::Url(url) => {
            let resp = http
                .get(url)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status)
                .map_err(|e| format!("{url}: {e}"))?;
            if resp.content_length().is_some_and(|n| n > MAX_BYTES) {
                return Err(format!("{url} is larger than 16 MB"));
            }
            let body = resp.bytes().await.map_err(|e| format!("{url}: {e}"))?;
            if body.len() as u64 > MAX_BYTES {
                return Err(format!("{url} is larger than 16 MB"));
            }
            Ok(body.to_vec())
        }
    }
}

/// Columns and rows of the source, or why it could not be read.
pub async fn read(spec: &DataSourceSpec, place: &Place, http: &reqwest::Client) -> Rows {
    let parsed = bytes(place, http).await.and_then(|bytes| {
        if let DataSourceSpec::Sheet {
            sheet,
            header,
            transpose,
            ..
        } = spec
        {
            return spreadsheet(bytes, sheet.as_deref())
                .map(|records| table(records, *header, *transpose));
        }
        let text = String::from_utf8_lossy(&bytes);
        let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
        match spec {
            DataSourceSpec::Csv {
                header, transpose, ..
            } => Ok(csv(text, *header, *transpose)),
            DataSourceSpec::Sheet { .. } => unreachable!("read above"),
            DataSourceSpec::Json { .. } => json(text),
            DataSourceSpec::Text { paragraphs, .. } => Ok(lines(text, *paragraphs)),
            DataSourceSpec::Feed { .. } => feed(text),
            DataSourceSpec::Xml { rows, .. } => xml(text, rows),
        }
    });
    match parsed {
        Ok((columns, rows)) => Rows {
            columns,
            rows,
            error: None,
        },
        Err(error) => Rows {
            error: Some(error),
            ..Rows::default()
        },
    }
}

/// A table as CSV (comma, quotes where needed), written atomically.
pub async fn write_csv(
    path: &Path,
    columns: &[String],
    rows: &[Vec<String>],
) -> std::io::Result<()> {
    let field = |v: &String| {
        if v.contains([',', '"', '\n', '\r']) {
            format!("\"{}\"", v.replace('"', "\"\""))
        } else {
            v.clone()
        }
    };
    let mut text = String::new();
    for row in std::iter::once(columns).chain(rows.iter().map(Vec::as_slice)) {
        text.push_str(&row.iter().map(field).collect::<Vec<_>>().join(","));
        text.push('\n');
    }
    let tmp = path.with_extension("csv.tmp");
    tokio::fs::write(&tmp, text).await?;
    tokio::fs::rename(&tmp, path).await
}

/// Text of the first child element named `name` (any namespace).
fn child_text(node: roxmltree::Node, names: &[&str]) -> String {
    for name in names {
        if let Some(child) = node
            .children()
            .find(|c| c.is_element() && c.tag_name().name() == *name)
        {
            // Atom links carry the address in `href`.
            if let Some(href) = child.attribute("href") {
                return href.to_string();
            }
            let text: String = child
                .descendants()
                .filter(|d| d.is_text())
                .filter_map(|d| d.text())
                .collect();
            return text.trim().to_string();
        }
    }
    String::new()
}

/// Items of an RSS 2.0 (`item`) or Atom (`entry`) feed.
fn feed(text: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let rows: Vec<Vec<String>> = doc
        .descendants()
        .filter(|n| n.is_element() && matches!(n.tag_name().name(), "item" | "entry"))
        .map(|item| {
            vec![
                child_text(item, &["title"]),
                child_text(item, &["description", "summary", "content"]),
                child_text(item, &["link"]),
                child_text(item, &["pubDate", "updated", "published", "date"]),
                child_text(item, &["author", "creator"]),
            ]
        })
        .collect();
    if rows.is_empty()
        && doc.root_element().tag_name().name() != "rss"
        && doc.root_element().tag_name().name() != "feed"
    {
        return Err("not an RSS or Atom feed".into());
    }
    let columns = ["title", "text", "link", "date", "author"]
        .map(String::from)
        .to_vec();
    Ok((columns, rows))
}

/// Elements at `path` (names separated by `/`, below the root or anywhere
/// when it starts with `//`) as rows: attributes as `@name`, child
/// elements by name, in the order they first appear.
fn xml(text: &str, path: &str) -> Result<(Vec<String>, Vec<Vec<String>>), String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| e.to_string())?;
    let steps: Vec<&str> = path
        .trim_start_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();
    let Some((last, parents)) = steps.split_last() else {
        return Err("no path to the rows".into());
    };
    // A path ends at elements named `last` whose ancestors, nearest first,
    // match the steps before it (the root counts too).
    let matches = |node: &roxmltree::Node| {
        if node.tag_name().name() != *last {
            return false;
        }
        let mut up = node.ancestors().skip(1).filter(|a| a.is_element());
        let anywhere = path.starts_with("//");
        for step in parents.iter().rev() {
            match up.next() {
                Some(a) if a.tag_name().name() == *step => {}
                _ => return false,
            }
        }
        anywhere || up.next().is_none()
    };
    let found: Vec<roxmltree::Node> = doc
        .descendants()
        .filter(|n| n.is_element() && matches(n))
        .collect();
    let mut columns: Vec<String> = Vec::new();
    let cells = |node: &roxmltree::Node| -> Vec<(String, String)> {
        let attributes = node
            .attributes()
            .map(|a| (format!("@{}", a.name()), a.value().to_string()));
        let children = node.children().filter(|c| c.is_element()).map(|c| {
            let text: String = c
                .descendants()
                .filter(|d| d.is_text())
                .filter_map(|d| d.text())
                .collect();
            (c.tag_name().name().to_string(), text.trim().to_string())
        });
        attributes.chain(children).collect()
    };
    let parsed: Vec<Vec<(String, String)>> = found.iter().map(cells).collect();
    for row in &parsed {
        for (name, _) in row {
            if !columns.contains(name) {
                columns.push(name.clone());
            }
        }
    }
    let rows = parsed
        .iter()
        .map(|row| {
            columns
                .iter()
                .map(|c| {
                    row.iter()
                        .find(|(n, _)| n == c)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default()
                })
                .collect()
        })
        .collect();
    Ok((columns, rows))
}

/// One row per line that is not empty, or per block of lines between
/// empty ones, in the column `text`.
fn lines(text: &str, paragraphs: bool) -> (Vec<String>, Vec<Vec<String>>) {
    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut block: Vec<&str> = Vec::new();
    for line in text.lines().map(str::trim).chain([""]) {
        if !line.is_empty() {
            block.push(line);
        }
        if !block.is_empty() && (line.is_empty() || !paragraphs) {
            rows.push(vec![block.join("\n")]);
            block.clear();
        }
    }
    (vec!["text".to_string()], rows)
}

/// The cells of a sheet (the first without a name), as text.
fn spreadsheet(bytes: Vec<u8>, name: Option<&str>) -> Result<Vec<Vec<String>>, String> {
    use calamine::{Data, Reader};
    let mut book = calamine::open_workbook_auto_from_rs(std::io::Cursor::new(bytes))
        .map_err(|e| format!("not a spreadsheet: {e}"))?;
    let name = match name {
        Some(name) => name.to_string(),
        None => book
            .sheet_names()
            .first()
            .cloned()
            .ok_or("the spreadsheet has no sheets")?,
    };
    let range = book
        .worksheet_range(&name)
        .map_err(|e| format!("sheet {name}: {e}"))?;
    let cell = |c: &Data| match c {
        Data::DateTime(d) => excel_date(d.as_f64()),
        _ => c.to_string(),
    };
    Ok(range.rows().map(|r| r.iter().map(cell).collect()).collect())
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

/// An Excel date (days since 1899-12-30) as an ISO date, with the time
/// if there is one; below one day a time alone (files with the 1904
/// calendar are four years off).
fn excel_date(serial: f64) -> String {
    if (0.0..1.0).contains(&serial) {
        let s = (serial * 86_400.0).round() as i64;
        return format!("{:02}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60);
    }
    let days = serial.floor() as i64;
    let seconds = ((serial - serial.floor()) * 86_400.0).round() as i64;
    let (days, seconds) = (days + seconds / 86_400, seconds % 86_400);
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = days - 25_569 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let date = format!("{year:04}-{month:02}-{day:02}");
    if seconds == 0 {
        date
    } else {
        let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
        format!("{date} {h:02}:{m:02}:{s:02}")
    }
}

/// Column names (from the first row, or numbered) and rows.
fn csv(text: &str, header: bool, transpose: bool) -> (Vec<String>, Vec<Vec<String>>) {
    table(records(text, separator(text)), header, transpose)
}

/// Records as a table: turned (rows to columns) if asked, then the
/// column names from the first row, or numbered; empty rows left out.
fn table(
    mut rows: Vec<Vec<String>>,
    header: bool,
    transpose: bool,
) -> (Vec<String>, Vec<Vec<String>>) {
    rows.retain(|r| r.iter().any(|c| !c.trim().is_empty()));
    if transpose {
        let width = rows.iter().map(Vec::len).max().unwrap_or(0);
        rows = (0..width)
            .map(|i| {
                rows.iter()
                    .map(|r| r.get(i).cloned().unwrap_or_default())
                    .collect()
            })
            .collect();
        rows.retain(|r: &Vec<String>| r.iter().any(|c| !c.trim().is_empty()));
    }
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
            false,
        );

        assert_eq!(columns, s(&["name", "role"]));
        assert_eq!(rows, [s(&["A, B", "says \"hi\""]), s(&["two\nlines", "x"])]);
    }

    #[test]
    fn csv_guesses_semicolon_and_tab() {
        assert_eq!(csv("a;b\n1;2", true, false).1, [s(&["1", "2"])]);
        assert_eq!(csv("a\tb\n1\t2", true, false).1, [s(&["1", "2"])]);
    }

    #[test]
    fn rows_to_columns() {
        // A form: names down the first column, a row per further column.
        let (columns, rows) = csv("name,Anna,Ben\nscore,3,4\n", true, true);

        assert_eq!(columns, s(&["name", "score"]));
        assert_eq!(rows, [s(&["Anna", "3"]), s(&["Ben", "4"])]);
    }

    #[test]
    fn text_lines_or_paragraphs() {
        let text = "One\nline two\n\n\nVerse\nsecond line\n  \nLast";

        assert_eq!(lines(text, false).1.len(), 5);
        assert_eq!(
            lines(text, true).1,
            [
                s(&["One\nline two"]),
                s(&["Verse\nsecond line"]),
                s(&["Last"])
            ]
        );
    }

    #[test]
    fn spreadsheets() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("testdata");
        for file in ["table.xlsx", "table.ods"] {
            let bytes = std::fs::read(dir.join(file)).unwrap();
            let (columns, rows) = table(spreadsheet(bytes, None).unwrap(), true, false);

            assert_eq!(columns, s(&["name", "score", "date"]), "{file}");
            assert_eq!(
                rows,
                [
                    s(&["Anna", "3", "2026-10-01"]),
                    s(&["Ben", "4.5", "2026-10-02"])
                ],
                "{file}"
            );
        }
        let bytes = std::fs::read(dir.join("table.xlsx")).unwrap();
        assert!(spreadsheet(bytes, Some("Missing")).is_err());
        assert!(spreadsheet(b"name,score".to_vec(), None).is_err());
    }

    #[test]
    fn excel_dates() {
        assert_eq!(excel_date(46_296.0), "2026-10-01");
        assert_eq!(excel_date(46_296.5), "2026-10-01 12:00:00");
        assert_eq!(excel_date(1.0), "1899-12-31");
        assert_eq!(excel_date(0.75), "18:00:00");
    }

    #[test]
    fn csv_without_header_numbers_columns_and_pads_rows() {
        let (columns, rows) = csv("1,2,3\n4", false, false);

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
    async fn written_csv_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sets.csv");
        let columns = s(&["name", "text"]);
        let rows = vec![s(&["A", "says \"hi\", twice"]), s(&["B", "two\nlines"])];

        write_csv(&path, &columns, &rows).await.unwrap();
        let spec = DataSourceSpec::Csv {
            path: path.display().to_string(),
            header: true,
            transpose: false,
        };
        let back = read(&spec, &Place::File(path), &client()).await;

        assert_eq!((back.columns, back.rows), (columns, rows));
    }

    #[test]
    fn rss_and_atom_items() {
        let rss = r#"<?xml version="1.0"?><rss version="2.0"><channel><title>C</title>
            <item><title>One</title><description>First &amp; more</description><link>http://a/1</link>
            <pubDate>Sun, 11 Oct 2026 20:00:00 +0200</pubDate></item>
            <item><title>Two</title></item></channel></rss>"#;
        let (columns, rows) = feed(rss).unwrap();
        assert_eq!(columns, s(&["title", "text", "link", "date", "author"]));
        assert_eq!(rows[0][..3], s(&["One", "First & more", "http://a/1"]));
        assert_eq!(rows[1][0], "Two");

        let atom = r#"<feed xmlns="http://www.w3.org/2005/Atom"><entry><title>A</title>
            <link href="http://b/1"/><updated>2026-10-11T20:00:00Z</updated>
            <author><name>N</name></author></entry></feed>"#;
        let (_, rows) = feed(atom).unwrap();
        assert_eq!(
            rows,
            [s(&["A", "", "http://b/1", "2026-10-11T20:00:00Z", "N"])]
        );

        assert!(feed("<html/>").is_err());
        assert!(feed("not xml").is_err());
    }

    #[test]
    fn xml_rows_by_path() {
        let text = r#"<scores><game id="1"><home>A</home><away>B</away></game>
            <game id="2"><home>C</home><extra>x</extra></game><other><game id="3"/></other></scores>"#;

        let (columns, rows) = xml(text, "scores/game").unwrap();
        assert_eq!(columns, s(&["@id", "home", "away", "extra"]));
        assert_eq!(rows, [s(&["1", "A", "B", ""]), s(&["2", "C", "", "x"])]);

        let (_, anywhere) = xml(text, "//game").unwrap();
        assert_eq!(anywhere.len(), 3);
        assert!(xml(text, "").is_err());
    }

    #[test]
    fn text_lines_are_rows() {
        let (columns, rows) = lines("one\n\n  two \r\n", false);

        assert_eq!(columns, s(&["text"]));
        assert_eq!(rows, [s(&["one"]), s(&["two"])]);
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
            transpose: false,
        };
        let show = dir.path().join("show.lmix").display().to_string();
        let http = client();

        let place = resolve(&spec, Some(&show));
        let rows = read(&spec, &place, &http).await;

        assert_eq!((rows.columns, rows.rows), (s(&["name"]), vec![s(&["A"])]));
        let missing = read(&spec, &Place::File(dir.path().join("nope.csv")), &http).await;
        assert!(missing.error.is_some());
    }

    #[tokio::test]
    async fn reads_addresses() {
        // A one-shot HTTP server with a CSV body.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buf = [0u8; 1024];
            let _ = stream.read(&mut buf).await;
            let body = "a,b\n1,2\n";
            let reply = format!(
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            );
            stream.write_all(reply.as_bytes()).await.unwrap();
        });
        let spec = DataSourceSpec::Csv {
            path: format!("http://127.0.0.1:{port}/sheet.csv"),
            header: true,
            transpose: false,
        };

        let place = resolve(&spec, Some("/shows/x.lmix"));
        assert!(place.is_url());
        let rows = read(&spec, &place, &client()).await;

        assert_eq!(rows.error, None);
        assert_eq!(
            (rows.columns, rows.rows),
            (s(&["a", "b"]), vec![s(&["1", "2"])])
        );
    }
}
