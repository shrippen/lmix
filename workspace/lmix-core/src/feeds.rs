//! Fixed OSC subscribers (show mode CTL.4): what to send, and when.
//!
//! The service calls [`Tracker::tick`] at the cue event rate; standby
//! messages go when a list's standby changed, running messages every tick
//! while a cue of the list runs and once more, with empty values, when
//! the last one ended. Panic messages go with every panic.
//!
//! ```text
//!   state ──tick──► Tracker (last standby per subscriber and list)
//!                     └─► [(target, OSC packet)] ──UDP──► Companion
//! ```

use std::collections::{HashMap, HashSet};

use lmix_types::{CueList, Feed, FeedOn, InputId, RunningCue, ShowState, StopMode};

use crate::osc::{encode_args, Arg};

/// One packet for the addresses of a target.
pub type Send = (String, Vec<u8>);

/// What the placeholders stand for.
#[derive(Clone, Debug, Default, PartialEq)]
struct Values {
    list: String,
    number: String,
    name: String,
    note: String,
    remaining_ms: Option<u64>,
    elapsed_ms: Option<u64>,
    total_ms: Option<u64>,
    mode: String,
}

fn seconds(ms: Option<u64>) -> String {
    ms.map(|ms| format!("{:.1}", ms as f64 / 1000.0))
        .unwrap_or_default()
}

fn render(template: &str, v: &Values) -> String {
    template
        .replace("{list}", &v.list)
        .replace("{number}", &v.number)
        .replace("{name}", &v.name)
        .replace("{note}", &v.note)
        .replace("{remaining}", &seconds(v.remaining_ms))
        .replace("{elapsed}", &seconds(v.elapsed_ms))
        .replace("{total}", &seconds(v.total_ms))
        .replace("{mode}", &v.mode)
}

fn arg(template: &str, v: &Values) -> Arg {
    let time = match template {
        "{remaining}" => Some(v.remaining_ms),
        "{elapsed}" => Some(v.elapsed_ms),
        "{total}" => Some(v.total_ms),
        _ => None,
    };
    match time {
        Some(ms) => Arg::Float(ms.unwrap_or(0) as f32 / 1000.0),
        None => Arg::Str(render(template, v)),
    }
}

/// The packets `feed` sends `on` an occasion.
fn packets(feed: &Feed, on: FeedOn, v: &Values) -> Vec<Send> {
    feed.messages
        .iter()
        .filter(|m| m.on == on)
        .map(|m| {
            let args: Vec<Arg> = m.args.iter().map(|a| arg(a, v)).collect();
            (
                feed.target.clone(),
                encode_args(&render(&m.address, v), &args),
            )
        })
        .collect()
}

fn standby_values(list: &CueList) -> (String, Values) {
    let cue = list.cues.get(list.standby as usize - 1);
    let key = cue.map_or_else(|| "end".to_string(), |c| c.id.to_string());
    let values = Values {
        list: list.name.clone(),
        number: cue.map(|c| c.number.clone()).unwrap_or_default(),
        name: cue.map(|c| c.display_name()).unwrap_or_default(),
        note: cue.map(|c| c.note.clone()).unwrap_or_default(),
        ..Values::default()
    };
    (key, values)
}

fn running_values(list: &CueList, run: Option<&RunningCue>) -> Values {
    let Some(run) = run else {
        return Values {
            list: list.name.clone(),
            ..Values::default()
        };
    };
    Values {
        list: list.name.clone(),
        number: run.number.clone(),
        name: run.name.clone(),
        remaining_ms: run.remaining_ms,
        elapsed_ms: Some(run.elapsed_ms),
        total_ms: run.total_ms,
        ..Values::default()
    }
}

/// What was sent last.
#[derive(Debug, Default)]
pub struct Tracker {
    /// Standby cue (or "end") per subscriber and list.
    standby: HashMap<(String, InputId), String>,
    /// Lists that had a running cue at the last tick.
    running: HashSet<InputId>,
    /// Subscribers as last seen: a changed one gets everything anew.
    seen: HashMap<String, Feed>,
}

impl Tracker {
    /// Packets due now for `state`.
    pub fn tick(&mut self, state: &ShowState) -> Vec<Send> {
        let mut out = Vec::new();
        // Subscribers that are new, changed or gone get all anew.
        for feed in &state.feeds {
            if self.seen.get(&feed.name) != Some(feed) {
                self.feed_changed(&feed.name);
                self.seen.insert(feed.name.clone(), feed.clone());
            }
        }
        self.seen
            .retain(|name, _| state.feeds.iter().any(|f| &f.name == name));
        self.standby
            .retain(|(feed, _), _| state.feeds.iter().any(|f| &f.name == feed));
        for list in &state.cue_lists {
            let (key, values) = standby_values(list);
            for feed in &state.feeds {
                let slot = (feed.name.clone(), list.id);
                if self.standby.get(&slot) != Some(&key) {
                    out.extend(packets(feed, FeedOn::Standby, &values));
                    self.standby.insert(slot, key.clone());
                }
            }

            let newest = state.running_cues.iter().rev().find(|r| r.list == list.id);
            let was = self.running.contains(&list.id);
            if newest.is_some() || was {
                let values = running_values(list, newest);
                for feed in &state.feeds {
                    out.extend(packets(feed, FeedOn::Running, &values));
                }
            }
            if newest.is_some() {
                self.running.insert(list.id);
            } else {
                self.running.remove(&list.id);
            }
        }
        out
    }

    /// Subscriber `name` gets its standby again at the next tick.
    fn feed_changed(&mut self, name: &str) {
        self.standby.retain(|(feed, _), _| feed != name);
    }
}

/// Packets of a panic (`Fade`) or hard stop (`Hard`).
pub fn panic(feeds: &[Feed], mode: StopMode) -> Vec<Send> {
    let values = Values {
        mode: match mode {
            StopMode::Fade => "fade",
            StopMode::Hard => "hard",
        }
        .into(),
        ..Values::default()
    };
    feeds
        .iter()
        .flat_map(|f| packets(f, FeedOn::Panic, &values))
        .collect()
}

/// Whether the tracker needs no ticks for `feeds` and nothing runs.
pub fn idle(feeds: &[Feed], running: bool, tracker: &Tracker) -> bool {
    feeds.is_empty() && !running && tracker.running.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::{Cue, FeedMessage, RunPhase};

    fn feed() -> Feed {
        Feed {
            name: "Deck".into(),
            target: "Companion".into(),
            messages: vec![
                FeedMessage {
                    on: FeedOn::Standby,
                    address: "/cv/{list}".into(),
                    args: vec!["{number} {name}".into()],
                },
                FeedMessage {
                    on: FeedOn::Running,
                    address: "/cv/left".into(),
                    args: vec!["{remaining}".into(), "{name}: {remaining} s".into()],
                },
                FeedMessage {
                    on: FeedOn::Panic,
                    address: "/cv/panic".into(),
                    args: vec!["{mode}".into()],
                },
            ],
        }
    }

    fn state(standby: u32, running: Option<u64>) -> ShowState {
        let mut cue = Cue::actions("Wald", vec![lmix_types::Action::Cut]);
        cue.number = "12".into();
        cue.id = lmix_types::CueId(uuid::Uuid::from_u128(12));
        let list = CueList {
            id: InputId(uuid::Uuid::nil()),
            number: 1,
            name: "Show".into(),
            cues: vec![cue.clone()],
            standby,
            active: true,
            source: None,
            emergency: false,
        };
        let mut state = empty_state();
        state.cue_lists = vec![list.clone()];
        state.feeds = vec![feed()];
        state.running_cues = running
            .map(|left| RunningCue {
                list: list.id,
                cue: cue.id,
                number: "12".into(),
                name: "Wald".into(),
                phase: RunPhase::Running,
                elapsed_ms: 1000,
                total_ms: Some(1000 + left),
                remaining_ms: Some(left),
                paused: false,
            })
            .into_iter()
            .collect();
        state
    }

    fn empty_state() -> ShowState {
        crate::switcher::Switcher::new(1).state()
    }

    fn text(packet: &[u8]) -> String {
        String::from_utf8_lossy(packet).replace('\0', " ")
    }

    #[test]
    fn standby_goes_when_it_changes() {
        let mut tracker = Tracker::default();

        let first = tracker.tick(&state(1, None));
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].0, "Companion");
        assert!(text(&first[0].1).contains("/cv/Show"));
        assert!(text(&first[0].1).contains("12 Wald"));

        assert!(
            tracker.tick(&state(1, None)).is_empty(),
            "no change, nothing sent"
        );
        let done = tracker.tick(&state(2, None));
        assert_eq!(done.len(), 1, "past the end: empty values");
        let mut changed = state(2, None);
        changed.feeds[0].messages[0].address = "/cv/standby".into();
        assert_eq!(
            tracker.tick(&changed).len(),
            1,
            "a changed subscriber gets it again"
        );
    }

    #[test]
    fn running_goes_each_tick_and_once_after() {
        let mut tracker = Tracker::default();
        tracker.tick(&state(1, None));

        let running = tracker.tick(&state(1, Some(2500)));
        assert_eq!(running.len(), 1);
        let packet = text(&running[0].1);
        assert!(packet.contains("Wald: 2.5 s"), "{packet}");
        assert!(packet.contains(",fs"), "a lone time is a float: {packet}");
        assert_eq!(tracker.tick(&state(1, Some(2250))).len(), 1);

        let after = tracker.tick(&state(1, None));
        assert_eq!(after.len(), 1, "once more, empty");
        assert!(!text(&after[0].1).contains("Wald"));
        assert!(tracker.tick(&state(1, None)).is_empty());
    }

    #[test]
    fn panic_says_which() {
        let sends = panic(&[feed()], StopMode::Hard);

        assert_eq!(sends.len(), 1);
        assert!(text(&sends[0].1).contains("hard"));
    }
}
