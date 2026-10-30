//! What would go wrong when the cue lists run (show mode SUI.5): checked
//! after every change of the show, its targets or missing media, so the
//! operator sees it before the show.
//!
//! - a start cue whose cue or list is gone;
//! - an action naming an input that is not there;
//! - an HTTP or Home Assistant action naming no target of this machine,
//!   or one that did not answer the last check or send, or while the
//!   targets are locked (RUN.9);
//! - an input a cue uses whose media file is missing.

use std::collections::BTreeMap;

use lmix_types::cue::{Cue, CueProblem, CueWarning};
use lmix_types::input::{REF_DYNAMIC, REF_PREVIEW, REF_PROGRAM};
use lmix_types::{Action, InputId};

use crate::show::{Show, ShowCueList};
use crate::targets::TargetConfig;

/// What the machine knows besides the show.
pub struct Machine<'a> {
    pub targets: &'a [TargetConfig],
    /// Targets that did not answer, with the reason.
    pub unreached: &'a BTreeMap<String, String>,
    /// Nothing goes out to the targets (RUN.9).
    pub targets_locked: bool,
    /// Inputs whose media are not there.
    pub missing: &'a dyn Fn(InputId) -> bool,
}

/// Every warning of `show`'s cue lists.
pub fn cue_warnings(show: &Show, machine: &Machine) -> Vec<CueWarning> {
    let mut out = Vec::new();
    for list in &show.cue_lists {
        walk(show, list, &list.cues, machine, &mut out);
    }
    out
}

/// The targets the cue lists send to, each once.
pub fn used_targets(show: &Show) -> Vec<String> {
    fn add(cues: &[Cue], out: &mut Vec<String>) {
        for cue in cues {
            for name in cue.actions.iter().filter_map(action_target) {
                if !out.iter().any(|n| n == name) {
                    out.push(name.to_string());
                }
            }
            add(&cue.children, out);
        }
    }
    let mut out = Vec::new();
    for list in &show.cue_lists {
        add(&list.cues, &mut out);
    }
    out
}

fn walk(
    show: &Show,
    list: &ShowCueList,
    cues: &[Cue],
    machine: &Machine,
    out: &mut Vec<CueWarning>,
) {
    for cue in cues {
        let mut warn = |problem: CueProblem| {
            out.push(CueWarning {
                list: list.id,
                cue: cue.id,
                message: problem.message(),
                problem,
            })
        };
        if let Some(target) = &cue.target {
            if let Some(reference) = &target.list {
                if crate::cues::find_list(&show.cue_lists, reference).is_none() {
                    warn(CueProblem::StartListGone);
                }
            }
            if let Some(text) = &target.cue {
                if !show.cue_lists.iter().any(|l| has_cue(&l.cues, text)) {
                    warn(CueProblem::StartCueGone {
                        target: text.clone(),
                    });
                }
            }
        }
        for action in &cue.actions {
            if let Some(name) = action_target(action) {
                if !machine.targets.iter().any(|t| t.name == name) {
                    warn(CueProblem::TargetGone {
                        target: name.to_string(),
                    });
                } else if machine.targets_locked {
                    warn(CueProblem::TargetLocked {
                        target: name.to_string(),
                    });
                } else if let Some(reason) = machine.unreached.get(name) {
                    warn(CueProblem::TargetUnreached {
                        target: name.to_string(),
                        reason: reason.clone(),
                    });
                }
            }
            for reference in input_refs(action) {
                match find_input(show, &reference) {
                    Lookup::Here(id) if (machine.missing)(id) => warn(CueProblem::MediaMissing {
                        input: reference.clone(),
                    }),
                    Lookup::Here(_) | Lookup::Special => {}
                    Lookup::Gone => warn(CueProblem::InputGone {
                        input: reference.clone(),
                    }),
                }
            }
        }
        walk(show, list, &cue.children, machine, out);
    }
}

fn has_cue(cues: &[Cue], text: &str) -> bool {
    cues.iter().any(|c| {
        c.id.to_string() == text
            || (!c.number.is_empty() && c.number == text)
            || has_cue(&c.children, text)
    })
}

fn action_target(action: &Action) -> Option<&str> {
    match action {
        Action::HttpRequest { target, .. } | Action::HaService { target, .. } => Some(target),
        _ => None,
    }
}

/// The `input` fields of an action, as text (`"Cam 1"`, `"3"`).
fn input_refs(action: &Action) -> Vec<String> {
    let Ok(value) = serde_json::to_value(action) else {
        return Vec::new();
    };
    let Some(map) = value.as_object() else {
        return Vec::new();
    };
    map.get("input")
        .into_iter()
        .filter_map(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        })
        .collect()
}

enum Lookup {
    Here(InputId),
    /// `preview`, `program`, `dynamic1`: decided when the cue runs.
    Special,
    Gone,
}

/// An input reference the way actions resolve it: number, id, name.
fn find_input(show: &Show, reference: &str) -> Lookup {
    if reference == REF_PREVIEW || reference == REF_PROGRAM || reference.starts_with(REF_DYNAMIC) {
        return Lookup::Special;
    }
    let inputs = &show.inputs;
    let found = match reference.parse::<usize>() {
        Ok(n) => n.checked_sub(1).and_then(|i| inputs.get(i)),
        Err(_) => inputs
            .iter()
            .find(|i| i.id.to_string() == reference)
            .or_else(|| {
                inputs
                    .iter()
                    .find(|i| crate::switcher::same_input_name(&i.name, reference))
            }),
    };
    found.map_or(Lookup::Gone, |i| Lookup::Here(i.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::cue::{CueKind, CueTarget};
    use lmix_types::InputRef;

    fn show_with(cues: Vec<Cue>) -> Show {
        let mut sw = crate::switcher::Switcher::new(2);
        sw.apply(Action::AddInput {
            source: lmix_types::SourceSpec::Pattern {
                pattern: lmix_types::TestPattern::Bars,
            },
            name: Some("Cam".into()),
        })
        .unwrap();
        let mut show = sw.show().clone();
        show.cue_lists.push(ShowCueList {
            id: InputId::new(),
            name: "Main".into(),
            cues,
            source: None,
            standby: None,
            pads: false,
        });
        show
    }

    #[test]
    fn finds_what_would_go_wrong() {
        let cut = |input: &str| Action::CutDirect {
            input: InputRef::Text(input.into()),
        };
        let start = Cue {
            target: Some(CueTarget {
                list: None,
                cue: Some("99".into()),
            }),
            ..Cue::empty(CueKind::Start)
        };
        let light = Action::HaService {
            target: "Haus".into(),
            service: "light.turn_on".into(),
            entity: None,
            data: None,
        };
        let group = Cue {
            children: vec![Cue::actions("Gone", vec![cut("Beamer")])],
            ..Cue::empty(CueKind::Group)
        };
        let show = show_with(vec![
            Cue::actions("Fine", vec![cut("Cam"), cut("cam"), cut("preview")]),
            start,
            Cue::actions("Light", vec![light]),
            group,
        ]);

        let none = BTreeMap::new();
        let machine = |missing: &'static dyn Fn(InputId) -> bool| Machine {
            targets: &[],
            unreached: &none,
            targets_locked: false,
            missing,
        };
        let warnings = cue_warnings(&show, &machine(&|_| false));
        let texts: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();

        assert_eq!(
            texts,
            [
                "starts cue 99, which is not there",
                "sends to target Haus, which this machine does not have",
                "uses input Beamer, which is not there",
            ]
        );
        let cam = show.inputs[0].id;
        let is_cam = move |id| id == cam;
        let missing = cue_warnings(
            &show,
            &Machine {
                targets: &[],
                unreached: &none,
                targets_locked: false,
                missing: &is_cam,
            },
        );
        assert!(missing
            .iter()
            .any(|w| w.message.contains("media file is missing")));
    }

    #[test]
    fn unreached_targets_warn() {
        let light = |target: &str| Action::HaService {
            target: target.into(),
            service: "light.turn_on".into(),
            entity: None,
            data: None,
        };
        let group = Cue {
            children: vec![Cue::actions("Licht", vec![light("Haus")])],
            ..Cue::empty(CueKind::Group)
        };
        let show = show_with(vec![
            Cue::actions("Duft", vec![light("Duft"), light("Haus")]),
            group,
        ]);
        let target = |name: &str| TargetConfig {
            name: name.into(),
            kind: lmix_types::TargetKind::Http,
            addresses: vec!["http://ha.lan:8123".into()],
            secret: None,
            timeout_ms: 1000,
        };
        let targets = [target("Haus"), target("Duft")];
        let unreached = BTreeMap::from([("Haus".to_string(), "cannot connect".to_string())]);

        let warnings = cue_warnings(
            &show,
            &Machine {
                targets: &targets,
                unreached: &unreached,
                targets_locked: false,
                missing: &|_| false,
            },
        );

        assert_eq!(used_targets(&show), ["Duft", "Haus"]);
        let texts: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
        assert_eq!(
            texts,
            [
                "sends to target Haus, which did not answer: cannot connect",
                "sends to target Haus, which did not answer: cannot connect",
            ]
        );
    }
}
