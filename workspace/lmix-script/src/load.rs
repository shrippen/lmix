//! Putting a script's lists into lmix-core (IMP.1, IMP.5, IMP.9).
//!
//! A list from a file is added once and replaced on every load after; it
//! carries its source (`script:<folder>/<file>`), so lmix-core keeps its
//! standby on the same cue and the Console does not edit it. A list whose
//! file is gone (or has no cue left) is removed. A file with errors keeps
//! the version loaded before.

use std::path::Path;
use std::time::Duration;

use lmix_types::{Action, InputRef, ShowState};

use crate::parse::{source_of, Problem, Script, ScriptList};

/// lmix-core answers on the same machine at once; more is a hung core.
const TIMEOUT: Duration = Duration::from_secs(10);

/// lmix-core's HTTP API.
pub struct Api {
    base: String,
    password: Option<String>,
    http: reqwest::blocking::Client,
}

impl Api {
    pub fn new(base: &str, password: Option<String>) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            password,
            http: reqwest::blocking::Client::builder()
                .timeout(TIMEOUT)
                .build()
                .expect("http client"),
        }
    }

    fn request(
        &self,
        request: reqwest::blocking::RequestBuilder,
    ) -> reqwest::blocking::RequestBuilder {
        match &self.password {
            Some(password) => request.bearer_auth(password),
            None => request,
        }
    }

    fn state(&self) -> Result<ShowState, String> {
        let response = self
            .request(self.http.get(format!("{}/api/state", self.base)))
            .send()
            .map_err(|e| e.to_string())?;
        if !response.status().is_success() {
            return Err(format!("state: HTTP {}", response.status()));
        }
        response.json().map_err(|e| e.to_string())
    }

    /// Run `action`; a refusal is lmix-core's reason.
    fn act(&self, action: &Action) -> Result<(), Refused> {
        let response = self
            .request(self.http.post(format!("{}/api/actions", self.base)))
            .json(action)
            .send()
            .map_err(|e| Refused::Down(e.to_string()))?;
        if response.status().is_success() {
            return Ok(());
        }
        let status = response.status();
        let body: serde_json::Value = response.json().unwrap_or_default();
        let message = body
            .get("message")
            .and_then(|m| m.as_str())
            .map_or_else(|| format!("HTTP {status}"), str::to_string);
        Err(Refused::Action(message))
    }
}

enum Refused {
    /// lmix-core cannot be reached: stop.
    Down(String),
    Action(String),
}

/// What a load did.
pub struct Done {
    pub loaded: Vec<String>,
    pub refused: Vec<Problem>,
}

/// Load every list of `script` without fatal problems; remove lists of
/// this folder whose file has no list any more.
pub fn apply(api: &Api, folder: &Path, script: &Script) -> Result<Done, String> {
    let folder_name = folder
        .canonicalize()
        .map_err(|e| e.to_string())?
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let state = api.state()?;
    let mut done = Done {
        loaded: Vec::new(),
        refused: Vec::new(),
    };

    for list in script.loadable() {
        let source = source_of(&folder_name, &list.file);
        let mine = state
            .cue_lists
            .iter()
            .find(|l| l.source.as_deref() == Some(source.as_str()));
        if mine.is_none() && state.cue_lists.iter().any(|l| l.name == list.name) {
            done.refused.push(problem(
                list,
                0,
                format!(
                    "a cue list \"{}\" exists that is not from this script; rename one",
                    list.name
                ),
            ));
            continue;
        }
        let action = match mine {
            Some(existing) => Action::SetCueList {
                list: InputRef::Text(existing.id.to_string()),
                name: list.name.clone(),
                cues: list.cues.clone(),
                source: Some(source),
            },
            None => Action::AddCueList {
                name: list.name.clone(),
                cues: list.cues.clone(),
                source: Some(source),
            },
        };
        match api.act(&action) {
            Ok(()) => done.loaded.push(format!(
                "{}: {} \"{}\"",
                list.file.display(),
                if mine.is_some() { "reloaded" } else { "loaded" },
                list.name
            )),
            Err(Refused::Down(e)) => return Err(e),
            Err(Refused::Action(message)) => {
                let line = cue_named(&message)
                    .and_then(|n| list.line_of(n))
                    .unwrap_or(0);
                done.refused.push(problem(list, line, message));
            }
        }
    }

    let prefix = format!("script:{folder_name}/");
    let kept: Vec<String> = script
        .lists
        .iter()
        .map(|l| source_of(&folder_name, &l.file))
        .collect();
    for gone in state.cue_lists.iter().filter(|l| {
        l.source
            .as_deref()
            .is_some_and(|s| s.starts_with(&prefix) && !kept.iter().any(|k| k == s))
    }) {
        let remove = Action::RemoveCueList {
            list: InputRef::Text(gone.id.to_string()),
        };
        match api.act(&remove) {
            Ok(()) => done
                .loaded
                .push(format!("removed \"{}\" (its file has no cues)", gone.name)),
            Err(Refused::Down(e)) => return Err(e),
            Err(Refused::Action(message)) => eprintln!("removing \"{}\": {message}", gone.name),
        }
    }
    Ok(done)
}

fn problem(list: &ScriptList, line: usize, message: String) -> Problem {
    Problem {
        file: list.file.clone(),
        line,
        message,
        fatal: true,
    }
}

/// The cue lmix-core's reason names: "cue list: cue <name>: …".
fn cue_named(message: &str) -> Option<&str> {
    let rest = message.split_once("cue ").map(|(_, r)| r)?;
    let rest = rest.strip_prefix("list: cue ").unwrap_or(rest);
    let rest = rest.strip_prefix("cue ").unwrap_or(rest);
    rest.split_once(": ").map(|(name, _)| name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_cue_in_a_refusal() {
        assert_eq!(
            cue_named("cue list: cue Rain: a wait needs duration_ms"),
            Some("Rain")
        );
        assert_eq!(cue_named("no cue list \"x\""), None);
    }
}
