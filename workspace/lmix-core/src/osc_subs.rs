//! OSC subscriptions per event and field (show mode CTL.3).
//!
//! `/lmix/subscribe` alone subscribes the mixer (tally, program, preview,
//! T-bar, FTB) and standby, as before. With arguments it adds one topic,
//! optionally with the fields to send, in that order:
//!
//! | Topic | Message | Fields (default all, in this order) |
//! |---|---|---|
//! | `mixer` | `/lmix/tally`, `/lmix/program`, … | – |
//! | `standby` | `/lmix/cue/standby` per list | `list`, `position`, `name`; also `number`, `note` |
//! | `running` | `/lmix/cue/running` per list: its newest running cue, at the cue event rate; once more with empty values when the last ended | `list`, `number`, `name`, `elapsed`, `remaining`, `total` |
//! | `cue` | `/lmix/cue/event` per cue event | `what`, `list`, `number`, `name`, `reason` |
//! | `panic` | `/lmix/panic` | `mode` (`fade`, `hard`) |
//! | `target` | `/lmix/target` per message to an output target | `target`, `result` (`ok`, `failed`), `reason` |
//!
//! `/lmix/subscribe running name remaining` sends only name and remaining
//! time. Times are floats in seconds, `position` an int (1-based), the
//! rest strings. `/lmix/unsubscribe running` drops one topic,
//! `/lmix/unsubscribe` all.
//!
//! ```text
//!   events ──► Subscription (topics, fields) ──► [OSC packets per peer]
//! ```

use std::collections::BTreeMap;

use lmix_types::{Cue, CueEvent, CueList, InputId, RunningCue, ShowState, StopMode, TargetResult};

use crate::osc::{encode_args, Arg};

/// What a subscriber hears about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Topic {
    Mixer,
    Standby,
    Running,
    Cue,
    Panic,
    Target,
}

/// One value of a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    List,
    Position,
    Number,
    Name,
    Note,
    Elapsed,
    Remaining,
    Total,
    What,
    Reason,
    Mode,
    Target,
    Result,
}

impl Topic {
    fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "mixer" => Self::Mixer,
            "standby" => Self::Standby,
            "running" => Self::Running,
            "cue" => Self::Cue,
            "panic" => Self::Panic,
            "target" => Self::Target,
            _ => return None,
        })
    }

    /// Fields it may send; the first ones by default.
    fn fields(self) -> (&'static [Field], usize) {
        use Field::*;
        match self {
            Self::Mixer => (&[], 0),
            Self::Standby => (&[List, Position, Name, Number, Note], 3),
            Self::Running => (&[List, Number, Name, Elapsed, Remaining, Total], 6),
            Self::Cue => (&[What, List, Number, Name, Reason], 5),
            Self::Panic => (&[Mode], 1),
            Self::Target => (&[Target, Result, Reason], 3),
        }
    }
}

impl Field {
    fn parse(name: &str) -> Option<Self> {
        use Field::*;
        Some(match name {
            "list" => List,
            "position" => Position,
            "number" => Number,
            "name" => Name,
            "note" => Note,
            "elapsed" => Elapsed,
            "remaining" => Remaining,
            "total" => Total,
            "what" => What,
            "reason" => Reason,
            "mode" => Mode,
            "target" => Target,
            "result" => Result,
            _ => return None,
        })
    }
}

/// A subscribe request: a topic with its fields, or the default.
pub type Wish = Option<(Topic, Vec<Field>)>;

/// Read `/lmix/subscribe [topic [field …]]`.
pub fn wish(args: &[String]) -> Result<Wish, String> {
    let Some((first, rest)) = args.split_first() else {
        return Ok(None);
    };
    let topic = Topic::parse(first).ok_or_else(|| {
        format!("unknown topic {first}: mixer, standby, running, cue, panic or target")
    })?;
    let (allowed, default) = topic.fields();
    if rest.is_empty() {
        return Ok(Some((topic, allowed[..default].to_vec())));
    }
    let fields = rest
        .iter()
        .map(|name| {
            Field::parse(name)
                .filter(|f| allowed.contains(f))
                .ok_or_else(|| format!("{first} has no field {name}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some((topic, fields)))
}

/// Read `/lmix/unsubscribe [topic]`: `None` drops all.
pub fn unwish(args: &[String]) -> Result<Option<Topic>, String> {
    args.first()
        .map(|name| Topic::parse(name).ok_or_else(|| format!("unknown topic {name}")))
        .transpose()
}

/// What one subscriber hears about, with the fields of each topic.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Subscription {
    topics: BTreeMap<Topic, Vec<Field>>,
}

impl Subscription {
    /// Add `wish`; the default is the mixer and standby as they always were.
    pub fn add(&mut self, wish: Wish) {
        match wish {
            Some((topic, fields)) => {
                self.topics.insert(topic, fields);
            }
            None => {
                self.topics.insert(Topic::Mixer, Vec::new());
                self.topics.insert(Topic::Standby, Vec::new());
            }
        }
    }

    /// Drop `topic`, or all; whether anything is left.
    pub fn remove(&mut self, topic: Option<Topic>) -> bool {
        match topic {
            Some(topic) => {
                self.topics.remove(&topic);
            }
            None => self.topics.clear(),
        }
        !self.topics.is_empty()
    }

    pub fn wants(&self, topic: Topic) -> bool {
        self.topics.contains_key(&topic)
    }

    fn fields(&self, topic: Topic) -> Option<&[Field]> {
        self.topics.get(&topic).map(Vec::as_slice)
    }
}

/// What the fields can say about one thing.
#[derive(Debug, Default)]
struct Values {
    list: String,
    position: i32,
    number: String,
    name: String,
    note: String,
    elapsed_ms: Option<u64>,
    remaining_ms: Option<u64>,
    total_ms: Option<u64>,
    what: String,
    reason: String,
    mode: String,
    target: String,
    result: String,
}

fn seconds(ms: Option<u64>) -> Arg {
    Arg::Float(ms.unwrap_or(0) as f32 / 1000.0)
}

fn args(fields: &[Field], v: &Values) -> Vec<Arg> {
    let text = |s: &String| Arg::Str(s.clone());
    fields
        .iter()
        .map(|f| match f {
            Field::List => text(&v.list),
            Field::Position => Arg::Int(v.position),
            Field::Number => text(&v.number),
            Field::Name => text(&v.name),
            Field::Note => text(&v.note),
            Field::Elapsed => seconds(v.elapsed_ms),
            Field::Remaining => seconds(v.remaining_ms),
            Field::Total => seconds(v.total_ms),
            Field::What => text(&v.what),
            Field::Reason => text(&v.reason),
            Field::Mode => text(&v.mode),
            Field::Target => text(&v.target),
            Field::Result => text(&v.result),
        })
        .collect()
}

/// Standby messages of `state` for `sub`, keyed per list (only changes
/// go out). The default fields keep the old message: list name,
/// position, cue name.
pub fn standby(state: &ShowState, sub: &Subscription) -> Vec<(String, Vec<u8>)> {
    let Some(fields) = sub.fields(Topic::Standby) else {
        return Vec::new();
    };
    let (allowed, default) = Topic::Standby.fields();
    let fields = if fields.is_empty() {
        &allowed[..default]
    } else {
        fields
    };
    state
        .cue_lists
        .iter()
        .map(|list| {
            let cue = list.cues.get(list.standby as usize - 1);
            let values = Values {
                list: list.name.clone(),
                position: list.standby as i32,
                number: cue.map(|c| c.number.clone()).unwrap_or_default(),
                name: cue.map(Cue::display_name).unwrap_or_default(),
                note: cue.map(|c| c.note.clone()).unwrap_or_default(),
                ..Values::default()
            };
            (
                format!("cue/{}", list.id),
                encode_args("/lmix/cue/standby", &args(fields, &values)),
            )
        })
        .collect()
}

/// The newest running cue per list of `lists` (`None`: the list's last
/// one ended), for `sub`.
pub fn running(
    lists: &[CueList],
    cues: &[RunningCue],
    ended: &[InputId],
    sub: &Subscription,
) -> Vec<Vec<u8>> {
    let Some(fields) = sub.fields(Topic::Running) else {
        return Vec::new();
    };
    let name = |id: InputId| {
        lists
            .iter()
            .find(|l| l.id == id)
            .map(|l| l.name.clone())
            .unwrap_or_default()
    };
    let mut newest: BTreeMap<String, &RunningCue> = BTreeMap::new();
    for run in cues {
        newest.insert(run.list.to_string(), run);
    }
    let mut out: Vec<Vec<u8>> = newest
        .values()
        .map(|run| {
            let values = Values {
                list: name(run.list),
                number: run.number.clone(),
                name: run.name.clone(),
                elapsed_ms: Some(run.elapsed_ms),
                remaining_ms: run.remaining_ms,
                total_ms: run.total_ms,
                ..Values::default()
            };
            encode_args("/lmix/cue/running", &args(fields, &values))
        })
        .collect();
    out.extend(ended.iter().map(|id| {
        let values = Values {
            list: name(*id),
            ..Values::default()
        };
        encode_args("/lmix/cue/running", &args(fields, &values))
    }));
    out
}

/// A cue event for `sub`, with names from `lists`.
pub fn cue(
    lists: &[CueList],
    list: InputId,
    cue: Option<lmix_types::CueId>,
    what: &CueEvent,
    sub: &Subscription,
) -> Option<Vec<u8>> {
    let fields = sub.fields(Topic::Cue)?;
    let owner = lists.iter().find(|l| l.id == list);
    let found = owner.zip(cue).and_then(|(l, id)| find(&l.cues, id));
    let what_name = serde_json::to_value(what)
        .ok()
        .and_then(|v| v.get("what").and_then(|w| w.as_str()).map(str::to_string))
        .unwrap_or_default();
    let reason = match what {
        CueEvent::Failed { reason } => reason.clone(),
        _ => String::new(),
    };
    let values = Values {
        what: what_name,
        list: owner.map(|l| l.name.clone()).unwrap_or_default(),
        number: found.map(|c| c.number.clone()).unwrap_or_default(),
        name: found.map(Cue::display_name).unwrap_or_default(),
        reason,
        ..Values::default()
    };
    Some(encode_args("/lmix/cue/event", &args(fields, &values)))
}

fn find(cues: &[Cue], id: lmix_types::CueId) -> Option<&Cue> {
    cues.iter().find_map(|c| {
        if c.id == id {
            Some(c)
        } else {
            find(&c.children, id)
        }
    })
}

/// A panic (`fade`) or hard stop (`hard`) for `sub`.
pub fn panic(mode: StopMode, sub: &Subscription) -> Option<Vec<u8>> {
    let fields = sub.fields(Topic::Panic)?;
    let values = Values {
        mode: match mode {
            StopMode::Fade => "fade",
            StopMode::Hard => "hard",
        }
        .into(),
        ..Values::default()
    };
    Some(encode_args("/lmix/panic", &args(fields, &values)))
}

/// How a message to an output target went, for `sub`.
pub fn target(name: &str, result: &TargetResult, sub: &Subscription) -> Option<Vec<u8>> {
    let fields = sub.fields(Topic::Target)?;
    let (result, reason) = match result {
        TargetResult::Ok => ("ok", String::new()),
        TargetResult::Failed { reason } => ("failed", reason.clone()),
    };
    let values = Values {
        target: name.into(),
        result: result.into(),
        reason,
        ..Values::default()
    };
    Some(encode_args("/lmix/target", &args(fields, &values)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::RunPhase;

    fn strings(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn wishes_name_topics_and_fields() {
        assert_eq!(wish(&[]), Ok(None));
        assert_eq!(
            wish(&strings(&["running", "name", "remaining"])),
            Ok(Some((Topic::Running, vec![Field::Name, Field::Remaining])))
        );
        assert_eq!(
            wish(&strings(&["standby"])),
            Ok(Some((
                Topic::Standby,
                vec![Field::List, Field::Position, Field::Name]
            )))
        );
        assert!(wish(&strings(&["weather"])).is_err());
        assert!(
            wish(&strings(&["panic", "name"])).is_err(),
            "panic has no name"
        );
        assert_eq!(unwish(&strings(&["cue"])), Ok(Some(Topic::Cue)));
        assert_eq!(unwish(&[]), Ok(None));

        let mut sub = Subscription::default();
        sub.add(None);
        assert!(sub.wants(Topic::Mixer) && sub.wants(Topic::Standby));
        assert!(!sub.wants(Topic::Running));
        assert!(sub.remove(Some(Topic::Mixer)), "standby is left");
        assert!(!sub.remove(None));
    }

    fn list(name: &str) -> CueList {
        let mut cue = Cue::actions("Licht an", vec![]);
        cue.number = "3".into();
        cue.note = "langsam".into();
        CueList {
            id: InputId::new(),
            number: 1,
            name: name.into(),
            cues: vec![cue],
            standby: 1,
            active: true,
            source: None,
            emergency: false,
        }
    }

    #[test]
    fn standby_sends_the_chosen_fields() {
        let mut state = crate::switcher::Switcher::new(1).state();
        state.cue_lists = vec![list("Szene")];
        let mut old = Subscription::default();
        old.add(None);
        let mut chosen = Subscription::default();
        chosen.add(wish(&strings(&["standby", "number", "note"])).unwrap());

        assert_eq!(
            standby(&state, &old)[0].1,
            encode_args(
                "/lmix/cue/standby",
                &[
                    Arg::Str("Szene".into()),
                    Arg::Int(1),
                    Arg::Str("Licht an".into())
                ]
            ),
            "as before"
        );
        assert_eq!(
            standby(&state, &chosen)[0].1,
            encode_args(
                "/lmix/cue/standby",
                &[Arg::Str("3".into()), Arg::Str("langsam".into())]
            )
        );
        assert!(standby(&state, &Subscription::default()).is_empty());
    }

    #[test]
    fn running_cue_and_target_messages() {
        let lists = vec![list("Szene")];
        let id = lists[0].id;
        let run = RunningCue {
            list: id,
            cue: lists[0].cues[0].id,
            number: "3".into(),
            name: "Licht an".into(),
            phase: RunPhase::Running,
            elapsed_ms: 1500,
            total_ms: Some(4000),
            remaining_ms: Some(2500),
        };
        let mut sub = Subscription::default();
        sub.add(wish(&strings(&["running", "name", "remaining"])).unwrap());
        sub.add(wish(&strings(&["cue"])).unwrap());
        sub.add(wish(&strings(&["target", "result"])).unwrap());

        assert_eq!(
            running(&lists, std::slice::from_ref(&run), &[], &sub),
            [encode_args(
                "/lmix/cue/running",
                &[Arg::Str("Licht an".into()), Arg::Float(2.5)]
            )]
        );
        assert_eq!(
            running(&lists, &[], &[id], &sub),
            [encode_args(
                "/lmix/cue/running",
                &[Arg::Str(String::new()), Arg::Float(0.0)]
            )],
            "empty once the last ended"
        );
        let failed = CueEvent::Failed {
            reason: "no answer".into(),
        };
        assert_eq!(
            cue(&lists, id, Some(run.cue), &failed, &sub),
            Some(encode_args(
                "/lmix/cue/event",
                &[
                    Arg::Str("failed".into()),
                    Arg::Str("Szene".into()),
                    Arg::Str("3".into()),
                    Arg::Str("Licht an".into()),
                    Arg::Str("no answer".into())
                ]
            ))
        );
        assert_eq!(
            target("Haus", &TargetResult::Ok, &sub),
            Some(encode_args("/lmix/target", &[Arg::Str("ok".into())]))
        );
        assert_eq!(panic(StopMode::Hard, &sub), None, "not subscribed");
    }
}
