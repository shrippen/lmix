//! Show packages (SHW.6): the show with its media, data files and own
//! title templates in one file, to move a show to another machine.
//!
//! ```text
//!   <name>.lmixpkg (tar)
//!     show.lmix              paths point into the package
//!     media/<n>-<file>       files, stills and playlist items of inputs
//!     data/<n>-<file>        files of data sources (not addresses)
//!     templates/<id>/…       own title templates (built-in ones stay out)
//!     templates/<id>.html …
//! ```
//!
//! Importing unpacks into a folder, points the inputs at the media there
//! (absolute paths, as the engine needs them), adds the templates the
//! templates folder lacks (an existing one is kept), and opens the show.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};

use lmix_types::SourceSpec;

use crate::show::{self, Show};

/// The show file inside a package.
const SHOW_FILE: &str = "show.lmix";

/// What a package holds.
#[derive(Debug, Default, PartialEq)]
pub struct Packed {
    pub files: usize,
    pub bytes: u64,
}

/// Absolute path of `path` (relative ones count from the show's folder).
fn absolute(path: &str, show: Option<&str>) -> PathBuf {
    let p = Path::new(path);
    if p.is_absolute() {
        return p.to_path_buf();
    }
    match show.and_then(|s| Path::new(s).parent()) {
        Some(dir) => dir.join(p),
        None => std::env::current_dir().unwrap_or_default().join(p),
    }
}

fn is_address(path: &str) -> bool {
    path.contains("://")
}

/// Files to pack: package name → file on disk, collected once each.
#[derive(Default)]
struct Files {
    by_source: HashMap<PathBuf, String>,
    list: Vec<(String, PathBuf)>,
}

impl Files {
    /// Package name for `source` under `folder`.
    fn add(&mut self, folder: &str, source: PathBuf) -> String {
        if let Some(name) = self.by_source.get(&source) {
            return name.clone();
        }
        let file = source
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_else(|| "file".into());
        let name = format!("{folder}/{}-{file}", self.list.len() + 1);
        self.by_source.insert(source.clone(), name.clone());
        self.list.push((name.clone(), source));
        name
    }
}

/// Files of an own template in the templates folder: (package name, file).
fn template_files(dir: &Path, id: &str) -> Result<Vec<(String, PathBuf)>, String> {
    let folder = dir.join(id);
    if folder.is_dir() {
        let mut out = Vec::new();
        let mut stack = vec![folder.clone()];
        while let Some(at) = stack.pop() {
            for entry in std::fs::read_dir(&at).map_err(|e| format!("{}: {e}", at.display()))? {
                let path = entry.map_err(|e| e.to_string())?.path();
                if path.is_dir() {
                    stack.push(path);
                } else {
                    let inner = path
                        .strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/");
                    out.push((format!("templates/{inner}"), path));
                }
            }
        }
        return Ok(out);
    }
    let page = dir.join(format!("{id}.html"));
    if !page.is_file() {
        return Err(format!("template {id} is not in the templates folder"));
    }
    Ok(["html", "json", "png"]
        .iter()
        .map(|ext| {
            (
                format!("templates/{id}.{ext}"),
                dir.join(format!("{id}.{ext}")),
            )
        })
        .filter(|(_, p)| p.is_file())
        .collect())
}

/// Write `show` with its files into the package `out` (blocking).
pub fn export(
    show: &Show,
    show_path: Option<&str>,
    templates: Option<&Path>,
    out: &Path,
) -> Result<Packed, String> {
    let mut packed_show = show.clone();
    let mut files = Files::default();
    for input in &mut packed_show.inputs {
        match &mut input.source {
            SourceSpec::File { path } | SourceSpec::Image { path } if !is_address(path) => {
                *path = files.add("media", absolute(path, show_path));
            }
            SourceSpec::Playlist { paths } => {
                for path in paths.iter_mut().filter(|p| !is_address(p)) {
                    *path = files.add("media", absolute(path, show_path));
                }
            }
            _ => {}
        }
    }
    for source in &mut packed_show.data_sources {
        let path = source.spec.path().to_string();
        if !is_address(&path) && !is_google_sheet(&source.spec) {
            let name = files.add("data", absolute(&path, show_path));
            set_path(&mut source.spec, name);
        }
    }
    let mut extra = Vec::new();
    let mut ids: Vec<&str> = templates_used(show)
        .into_iter()
        .filter(|id| !id.starts_with("lmix/"))
        .collect();
    ids.sort();
    ids.dedup();
    for id in ids {
        let dir = templates.ok_or_else(|| format!("template {id}: no templates folder"))?;
        extra.extend(template_files(dir, id)?);
    }

    let missing: Vec<String> = files
        .list
        .iter()
        .chain(&extra)
        .filter(|(_, p)| !p.is_file())
        .map(|(_, p)| p.display().to_string())
        .collect();
    if !missing.is_empty() {
        return Err(format!("missing: {}", missing.join(", ")));
    }

    let tmp = out.with_extension("lmixpkg.tmp");
    let write = || -> std::io::Result<Packed> {
        let mut tar = tar::Builder::new(File::create(&tmp)?);
        let body = show::to_bytes(&packed_show);
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        header.set_mtime(now);
        header.set_cksum();
        tar.append_data(&mut header, SHOW_FILE, body.as_slice())?;
        let mut packed = Packed::default();
        for (name, path) in files.list.iter().chain(&extra) {
            tar.append_path_with_name(path, name)?;
            packed.files += 1;
            packed.bytes += std::fs::metadata(path)?.len();
        }
        tar.into_inner()?.sync_all()?;
        Ok(packed)
    };
    let packed = write().map_err(|e| {
        // A half-written package may be gigabytes.
        let _ = std::fs::remove_file(&tmp);
        format!("{}: {e}", out.display())
    })?;
    std::fs::rename(&tmp, out).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(packed)
}

/// Templates the show uses: loaded on layers, or loaded by actions of
/// shortcuts, cues and triggers.
fn templates_used(show: &Show) -> Vec<&str> {
    let mut actions: Vec<&lmix_types::Action> = Vec::new();
    actions.extend(show.shortcuts.iter().flat_map(|s| &s.actions));
    actions.extend(
        show.cue_lists
            .iter()
            .flat_map(|l| l.cues.iter().flat_map(|c| &c.actions)),
    );
    actions.extend(
        show.inputs
            .iter()
            .flat_map(|i| i.triggers.iter().flat_map(|t| &t.actions)),
    );
    actions.extend(
        show.graphics
            .iter()
            .flat_map(|g| g.triggers.iter().flat_map(|t| &t.actions)),
    );
    show.graphics
        .iter()
        .flat_map(|g| g.layers.iter().map(|t| t.template.as_str()))
        .chain(actions.into_iter().filter_map(|a| match a {
            lmix_types::Action::TitleLoad { template, .. } => Some(template.as_str()),
            _ => None,
        }))
        .collect()
}

/// A Google Sheet's path may be a bare id: no file either way.
fn is_google_sheet(spec: &lmix_types::DataSourceSpec) -> bool {
    matches!(spec, lmix_types::DataSourceSpec::GoogleSheet { .. })
}

fn set_path(spec: &mut lmix_types::DataSourceSpec, to: String) {
    use lmix_types::DataSourceSpec as D;
    match spec {
        D::Csv { path, .. }
        | D::Sheet { path, .. }
        | D::GoogleSheet { path, .. }
        | D::Json { path }
        | D::Text { path, .. }
        | D::Feed { path }
        | D::Xml { path, .. } => *path = to,
    }
}

/// Unpack `package` into `folder` (new or empty) and give the show with
/// the inputs pointing there, the show file's path, and notes on
/// templates kept (blocking).
pub fn import(
    package: &Path,
    folder: &Path,
    templates: Option<&Path>,
) -> Result<(Show, PathBuf, Vec<String>), String> {
    let err = |e: std::io::Error| format!("{}: {e}", package.display());
    if folder.exists() && std::fs::read_dir(folder).map_err(err)?.next().is_some() {
        return Err(format!("{} is not empty", folder.display()));
    }
    std::fs::create_dir_all(folder).map_err(err)?;
    let mut archive = tar::Archive::new(File::open(package).map_err(err)?);
    for entry in archive.entries().map_err(err)? {
        let mut entry = entry.map_err(err)?;
        // Plain files and folders only: no links that could lead out.
        if !matches!(
            entry.header().entry_type(),
            tar::EntryType::Regular | tar::EntryType::Directory
        ) {
            continue;
        }
        // unpack_in refuses names that leave the folder.
        entry.unpack_in(folder).map_err(err)?;
    }
    let show_file = folder.join(SHOW_FILE);
    let bytes = std::fs::read(&show_file).map_err(|e| format!("{}: {e}", show_file.display()))?;
    let mut show =
        show::from_bytes(&bytes, &show_file.display().to_string()).map_err(|e| e.to_string())?;
    // Paths of the package stay in its folder.
    let mut outside = Vec::new();
    let mut inside = |path: &mut String| {
        let p = Path::new(path.as_str());
        if is_address(path) || p.is_absolute() {
            return;
        }
        if p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            outside.push(path.clone());
            return;
        }
        *path = folder.join(path.as_str()).display().to_string();
    };
    for input in &mut show.inputs {
        match &mut input.source {
            SourceSpec::File { path } | SourceSpec::Image { path } => inside(path),
            SourceSpec::Playlist { paths } => paths.iter_mut().for_each(&mut inside),
            _ => {}
        }
    }
    for source in &show.data_sources {
        let path = source.spec.path();
        if !is_address(path)
            && !is_google_sheet(&source.spec)
            && Path::new(path)
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            outside.push(path.to_string());
        }
    }
    if !outside.is_empty() {
        return Err(format!("paths leave the package: {}", outside.join(", ")));
    }
    std::fs::write(&show_file, show::to_bytes(&show))
        .map_err(|e| format!("{}: {e}", show_file.display()))?;

    // Own templates: into the templates folder, unless one is there.
    let mut notes = Vec::new();
    let packed_templates = folder.join("templates");
    if let (Some(dir), true) = (templates, packed_templates.is_dir()) {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for entry in std::fs::read_dir(&packed_templates).map_err(err)? {
            let from = entry.map_err(err)?.path();
            let to = dir.join(from.file_name().unwrap());
            if to.exists() {
                notes.push(format!("kept {}", to.display()));
                continue;
            }
            copy_all(&from, &to).map_err(|e| format!("{}: {e}", to.display()))?;
        }
    }
    Ok((show, show_file, notes))
}

fn copy_all(from: &Path, to: &Path) -> std::io::Result<()> {
    if from.is_dir() {
        std::fs::create_dir_all(to)?;
        for entry in std::fs::read_dir(from)? {
            let path = entry?.path();
            copy_all(&path, &to.join(path.file_name().unwrap()))?;
        }
        Ok(())
    } else {
        std::fs::copy(from, to).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::show::{ShowDataSource, ShowGraphics, ShowInput, ShowTitle};
    use lmix_types::{DataSourceSpec, InputId};

    fn input(source: SourceSpec) -> ShowInput {
        ShowInput {
            id: InputId::new(),
            name: "x".into(),
            source,
            slot: 0,
            color: None,
            audio: Default::default(),
            signal_loss: Default::default(),
            strip: Default::default(),
            sends: [0; lmix_types::input::AUX_BUSES],
            look: Default::default(),
            triggers: Vec::new(),
            ptz: None,
        }
    }

    #[test]
    fn a_show_moves_with_its_files() {
        let here = tempfile::tempdir().unwrap();
        let shows = here.path().join("shows");
        std::fs::create_dir_all(shows.join("clips")).unwrap();
        std::fs::write(shows.join("clips/a.webm"), "AAA").unwrap();
        std::fs::write(here.path().join("b.png"), "BB").unwrap();
        std::fs::write(shows.join("guests.csv"), "name\nA\n").unwrap();
        let templates = here.path().join("templates");
        std::fs::create_dir_all(templates.join("own")).unwrap();
        std::fs::write(templates.join("own/index.html"), "<p>").unwrap();
        let show_path = shows.join("s.lmix").display().to_string();

        let show = Show {
            inputs: vec![
                input(SourceSpec::File {
                    path: "clips/a.webm".into(),
                }),
                input(SourceSpec::Playlist {
                    paths: vec![
                        here.path().join("b.png").display().to_string(),
                        "clips/a.webm".into(),
                        "https://x/y.mp4".into(),
                    ],
                }),
            ],
            data_sources: vec![ShowDataSource {
                id: InputId::new(),
                name: "Guests".into(),
                spec: DataSourceSpec::Csv {
                    path: "guests.csv".into(),
                    header: true,
                    transpose: false,
                },
                row: 1,
                auto_ms: None,
                loop_rows: false,
            }],
            graphics: vec![ShowGraphics {
                id: InputId::new(),
                name: "G".into(),
                layers: ["own", "lmix/clock"]
                    .iter()
                    .enumerate()
                    .map(|(i, t)| ShowTitle {
                        layer: i as u32 + 1,
                        template: t.to_string(),
                        data: Default::default(),
                        bindings: Vec::new(),
                        phase: Default::default(),
                        step: 0,
                        transform: Default::default(),
                    })
                    .collect(),
                triggers: Vec::new(),
            }],
            ..Show::default()
        };
        let package = here.path().join("s.lmixpkg");

        let packed = export(&show, Some(&show_path), Some(&templates), &package).unwrap();
        // a.webm once, b.png, guests.csv, own/index.html.
        assert_eq!(
            packed,
            Packed {
                files: 4,
                bytes: 3 + 2 + 7 + 3
            }
        );

        let elsewhere = here.path().join("there");
        let other_templates = here.path().join("other-templates");
        let (opened, file, notes) = import(&package, &elsewhere, Some(&other_templates)).unwrap();

        assert_eq!(file, elsewhere.join("show.lmix"));
        assert!(notes.is_empty());
        let SourceSpec::File { path } = &opened.inputs[0].source else {
            panic!()
        };
        assert_eq!(std::fs::read_to_string(path).unwrap(), "AAA");
        let SourceSpec::Playlist { paths } = &opened.inputs[1].source else {
            panic!()
        };
        assert_eq!(std::fs::read_to_string(&paths[0]).unwrap(), "BB");
        assert_eq!(paths[1], *path, "the same file once");
        assert_eq!(paths[2], "https://x/y.mp4");
        // Data files stay relative to the show file.
        let data = elsewhere.join(opened.data_sources[0].spec.path());
        assert_eq!(std::fs::read_to_string(data).unwrap(), "name\nA\n");
        assert!(other_templates.join("own/index.html").is_file());

        // A template only a shortcut loads comes along too.
        let mut with_key = show.clone();
        with_key.graphics[0].layers.retain(|t| t.template != "own");
        with_key.shortcuts.push(crate::show::ShowShortcut {
            id: InputId::new(),
            label: "k".into(),
            key: None,
            web: false,
            color: None,
            midi: None,
            actions: vec![lmix_types::Action::TitleLoad {
                graphics: lmix_types::InputRef::Number(1),
                layer: 1,
                template: "own".into(),
                data: Default::default(),
                play: true,
            }],
        });
        let p2 = here.path().join("k.lmixpkg");
        assert_eq!(
            export(&with_key, Some(&show_path), Some(&templates), &p2)
                .unwrap()
                .files,
            4
        );

        // Not into a folder with something in it; a template there is kept.
        assert!(import(&package, &elsewhere, None).is_err());
        let (_, _, notes) =
            import(&package, &here.path().join("third"), Some(&other_templates)).unwrap();
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn missing_files_are_named() {
        let here = tempfile::tempdir().unwrap();
        let show = Show {
            inputs: vec![input(SourceSpec::File {
                path: "/nowhere/clip.webm".into(),
            })],
            ..Show::default()
        };

        let error = export(&show, None, None, &here.path().join("p.lmixpkg")).unwrap_err();

        assert!(error.contains("/nowhere/clip.webm"), "{error}");
    }
}
