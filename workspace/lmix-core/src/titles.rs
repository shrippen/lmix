//! Title templates (GFX.1, GFX.13, GFX.19): the built-in Kante set and a
//! templates folder, served under `/titles/`.
//!
//! ```text
//!   titles/                    built into the binary (build.rs)
//!     host.html                the graphics page (/graphics/<n>)
//!     sdk/lmix-title.js, .css  helper and Kante look for templates (optional)
//!     sdk/skeleton/            a template to copy into the folder
//!     kante/                   Kante tokens and fonts (scripts/sync-kante.sh)
//!     lmix/<name>/index.html   built-in templates, id "lmix/<name>"
//!                 manifest.json
//!   <templates folder>/
//!     <name>/index.html        id "<name>", manifest.json beside it
//!     <name>.html              id "<name>" (e.g. a CasparCG template),
//!     <name>.json              its manifest, if any
//! ```
//!
//! A manifest is optional: name, fields (id, label, kind, default,
//! choices), `data_format` (`json` or CasparCG `xml`), `steps`,
//! `functions`. Folder templates get a revision from their files' times,
//! so the graphics page reloads them when they change (GFX.14).

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use lmix_types::{DataFormat, Template, TemplateField};
use serde::Deserialize;

use crate::switcher::TemplateRef;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/titles.rs"));
}

/// Built-in file at `path` (relative to `titles/`).
pub fn builtin(path: &str) -> Option<&'static [u8]> {
    embedded::FILES
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}

/// The graphics page.
pub fn host_page() -> &'static [u8] {
    builtin("host.html").expect("titles/host.html is built in")
}

/// Top-level names the built-in files use; folder templates cannot take
/// them.
const RESERVED: [&str; 4] = ["lmix", "kante", "sdk", "host"];

/// A manifest as written (all optional).
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct Manifest {
    name: Option<String>,
    fields: Vec<ManifestField>,
    data_format: DataFormat,
    steps: u32,
    functions: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ManifestField {
    id: String,
    #[serde(default)]
    label: Option<String>,
    #[serde(default)]
    kind: lmix_types::FieldKind,
    #[serde(default)]
    default: Option<String>,
    #[serde(default)]
    choices: Vec<String>,
}

fn template(
    id: String,
    url: String,
    builtin: bool,
    manifest: Option<&[u8]>,
    revision: u64,
) -> Template {
    let manifest: Manifest = match manifest.map(serde_json::from_slice::<Manifest>) {
        Some(Ok(m)) => m,
        Some(Err(e)) => {
            tracing::warn!(template = %id, error = %e, "manifest unreadable; template has no fields");
            Manifest::default()
        }
        None => Manifest::default(),
    };
    Template {
        name: manifest.name.unwrap_or_else(|| id.clone()),
        fields: manifest
            .fields
            .into_iter()
            .map(|f| TemplateField {
                label: f.label.unwrap_or_else(|| f.id.clone()),
                id: f.id,
                kind: f.kind,
                default: f.default,
                choices: f.choices,
            })
            .collect(),
        data_format: manifest.data_format,
        steps: manifest.steps,
        functions: manifest.functions,
        id,
        url,
        builtin,
        revision,
    }
}

/// The built-in templates.
fn builtin_templates() -> Vec<Template> {
    embedded::FILES
        .iter()
        .filter_map(|(path, _)| {
            let name = path.strip_prefix("lmix/")?.strip_suffix("/index.html")?;
            let id = format!("lmix/{name}");
            let manifest = builtin(&format!("{id}/manifest.json"));
            Some(template(
                id.clone(),
                format!("/titles/{id}/index.html"),
                true,
                manifest,
                0,
            ))
        })
        .collect()
}

/// Newest modification of `paths`, ms since 1970; 0 if none is readable.
fn revision(paths: &[PathBuf]) -> u64 {
    paths
        .iter()
        .filter_map(|p| std::fs::metadata(p).ok()?.modified().ok())
        .filter_map(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
        .max()
        .unwrap_or(0)
}

/// Every file below `dir`.
fn files_below(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files_below(&path, out);
        } else {
            out.push(path);
        }
    }
}

/// Templates in the folder (blocking; small folders).
fn folder_templates(dir: &Path) -> Vec<Template> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path
            .file_name()
            .and_then(|n| n.to_str())
            .map(str::to_string)
        else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            if RESERVED.contains(&name.as_str()) || !path.join("index.html").is_file() {
                continue;
            }
            let mut files = Vec::new();
            files_below(&path, &mut files);
            let manifest = std::fs::read(path.join("manifest.json")).ok();
            found.push(template(
                name.clone(),
                format!("/titles/{name}/index.html"),
                false,
                manifest.as_deref(),
                revision(&files),
            ));
        } else if let Some(stem) = name.strip_suffix(".html") {
            if RESERVED.contains(&stem) {
                continue;
            }
            let manifest_path = dir.join(format!("{stem}.json"));
            let manifest = std::fs::read(&manifest_path).ok();
            found.push(template(
                stem.to_string(),
                format!("/titles/{name}"),
                false,
                manifest.as_deref(),
                revision(&[path.clone(), manifest_path]),
            ));
        }
    }
    found.sort_by(|a, b| a.id.cmp(&b.id));
    found
}

/// Built-in templates and those in the folder, as the switcher needs them.
pub fn catalog(templates: &[Template]) -> HashMap<String, TemplateRef> {
    templates
        .iter()
        .map(|t| {
            (
                t.id.clone(),
                TemplateRef {
                    url: t.url.clone(),
                    revision: t.revision,
                },
            )
        })
        .collect()
}

/// Built-in templates, then the folder's (blocking).
pub fn scan(dir: Option<&Path>) -> Vec<Template> {
    let mut all = builtin_templates();
    if let Some(dir) = dir {
        all.extend(folder_templates(dir));
    }
    all
}

/// Content type by file extension.
pub fn content_type(path: &str) -> &'static str {
    match path.rsplit('.').next().unwrap_or_default() {
        "html" | "htm" => "text/html; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "webp" => "image/webp",
        "gif" => "image/gif",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "txt" => "text/plain; charset=utf-8",
        "webm" => "video/webm",
        "mp4" => "video/mp4",
        _ => "application/octet-stream",
    }
}

/// A path below `/titles/` that cannot leave its folder.
fn safe(path: &str) -> Option<PathBuf> {
    let path = Path::new(path);
    path.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then(|| path.to_path_buf())
}

/// File at `/titles/<path>`: built-in first, else from the folder.
pub fn file(dir: Option<&Path>, path: &str) -> Option<Vec<u8>> {
    let relative = safe(path)?;
    let top = relative.components().next()?.as_os_str().to_str()?;
    if RESERVED.contains(&top) || top == "host.html" {
        return builtin(path).map(<[u8]>::to_vec);
    }
    let dir = dir?.canonicalize().ok()?;
    let full = dir.join(&relative).canonicalize().ok()?;
    // A link inside the folder must not lead out of it.
    full.starts_with(&dir).then(|| std::fs::read(full).ok())?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_set_has_the_kante_templates() {
        let ids: Vec<String> = builtin_templates().into_iter().map(|t| t.id).collect();
        for id in [
            "lmix/lower-third",
            "lmix/name-role",
            "lmix/ticker",
            "lmix/clock",
            "lmix/countdown",
            "lmix/scoreboard",
            "lmix/card",
            "lmix/text-slides",
        ] {
            assert!(ids.contains(&id.to_string()), "missing {id}: {ids:?}");
        }
        for t in builtin_templates() {
            assert!(!t.fields.is_empty(), "{} has no fields", t.id);
            assert!(builtin(&format!("{}/manifest.json", t.id)).is_some());
        }
    }

    #[test]
    fn folder_templates_with_and_without_manifest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("score")).unwrap();
        std::fs::write(dir.path().join("score/index.html"), "<html>").unwrap();
        std::fs::write(
            dir.path().join("score/manifest.json"),
            r#"{"name":"Score","fields":[{"id":"home","kind":"number"}],"steps":2}"#,
        )
        .unwrap();
        std::fs::write(dir.path().join("caspar.html"), "<html>").unwrap();
        std::fs::write(dir.path().join("caspar.json"), r#"{"data_format":"xml"}"#).unwrap();
        // Reserved names and folders without a page are skipped.
        std::fs::create_dir(dir.path().join("lmix")).unwrap();
        std::fs::write(dir.path().join("lmix/index.html"), "<html>").unwrap();
        std::fs::create_dir(dir.path().join("empty")).unwrap();

        let found = folder_templates(dir.path());

        let ids: Vec<&str> = found.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["caspar", "score"]);
        let score = &found[1];
        assert_eq!(score.name, "Score");
        assert_eq!(score.url, "/titles/score/index.html");
        assert_eq!(score.fields[0].label, "home");
        assert_eq!(score.fields[0].kind, lmix_types::FieldKind::Number);
        assert_eq!(score.steps, 2);
        assert!(score.revision > 0);
        assert_eq!(found[0].url, "/titles/caspar.html");
        assert_eq!(found[0].data_format, DataFormat::Xml);
    }

    #[test]
    fn files_stay_inside_their_folder() {
        let dir = tempfile::tempdir().unwrap();
        let inner = dir.path().join("t");
        std::fs::create_dir(&inner).unwrap();
        std::fs::write(inner.join("a.css"), "x").unwrap();
        std::fs::write(dir.path().join("secret"), "s").unwrap();

        assert_eq!(file(Some(&inner), "a.css").as_deref(), Some(&b"x"[..]));
        assert_eq!(file(Some(&inner), "../secret"), None);
        assert_eq!(file(Some(&inner), "/etc/passwd"), None);
        assert!(file(None, "kante/variables.css").is_some());
        assert!(file(None, "sdk/lmix-title.js").is_some());
        assert_eq!(file(None, "a.css"), None);
    }

    #[test]
    fn content_types() {
        assert_eq!(content_type("a/b.html"), "text/html; charset=utf-8");
        assert_eq!(content_type("f.woff2"), "font/woff2");
        assert_eq!(content_type("x"), "application/octet-stream");
    }
}
