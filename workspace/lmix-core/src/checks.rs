//! What would go wrong when the cue lists run (show mode SUI.5): checked
//! after every change of the show, its targets or missing media, so the
//! operator sees it before the show.
//!
//! - a start cue whose cue or list is gone;
//! - an action naming an input that is not there;
//! - an HTTP or Home Assistant action naming no target of this machine;
//! - an input a cue uses whose media file is missing.

use lmix_types::cue::{Cue, CueProblem, CueWarning};
use lmix_types::input::{REF_DYNAMIC, REF_PREVIEW, REF_PROGRAM};
use lmix_types::{Action, InputId};

use crate::show::{Show, ShowCueList};
use crate::targets::TargetConfig;

/// Every warning of `show`'s cue lists; `missing`: inputs whose media
/// are not there.
pub fn cue_warnings(
    show: &Show,
    targets: &[TargetConfig],
    missing: &dyn Fn(InputId) -> bool,
) -> Vec<CueWarning> {
    let mut out = Vec::new();
    for list in &show.cue_lists {
        walk(show, list, &list.cues, targets, missing, &mut out);
    }
    out
}

fn walk(
    show: &Show,
    list: &ShowCueList,
    cues: &[Cue],
    targets: &[TargetConfig],
    missing: &dyn Fn(InputId) -> bool,
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
                if !targets.iter().any(|t| t.name == name) {
                    warn(CueProblem::TargetGone {
                        target: name.to_string(),
                    });
                }
            }
            for reference in input_refs(action) {
                match find_input(show, &reference) {
                    Lookup::Here(id) if missing(id) => warn(CueProblem::MediaMissing {
                        input: reference.clone(),
                    }),
                    Lookup::Here(_) | Lookup::Special => {}
                    Lookup::Gone => warn(CueProblem::InputGone {
                        input: reference.clone(),
                    }),
                }
            }
        }
        walk(show, list, &cue.children, targets, missing, out);
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
            .or_else(|| inputs.iter().find(|i| i.name == reference)),
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
            Cue::actions("Fine", vec![cut("Cam"), cut("preview")]),
            start,
            Cue::actions("Light", vec![light]),
            group,
        ]);

        let warnings = cue_warnings(&show, &[], &|_| false);
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
        let missing = cue_warnings(&show, &[], &|id| id == cam);
        assert!(missing
            .iter()
            .any(|w| w.message.contains("media file is missing")));
    }
}
