//! Running cue lists (show mode, `docs/LASTENHEFT-SHOW.md` §3.2).
//!
//! The runner keeps standby per list and the cues that run. It knows no
//! clock and no engine: callers pass the time and get back what to do
//! ([`Out`]). The service runs the actions and wakes [`Runner::tick`] at
//! [`Runner::deadline`].
//!
//! ```text
//!   start ──pre wait──► fire ──duration──► end
//!                        └──post wait──► follow (after_wait)
//!                                  end ─► follow (after_end)
//! ```
//!
//! Every step happens at its own due time, not at the time `tick` came,
//! so waits and follows never drift (RUN.8): a chain of a hundred 20 ms
//! waits ends 2 s after GO however late each tick is.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::time::Duration;

use tokio::time::Instant;

use lmix_types::cue::{
    Cue, CueEvent, CueId, CueKind, Follow, GoRearm, GroupMode, RunPhase, RunningCue, StandbyMove,
    StopMode, StopScope,
};
use lmix_types::sound::FadeTarget;
use lmix_types::{Action, ActionError, InputId, InputRef};

use crate::show::ShowCueList;

/// What the caller has to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Out {
    /// Run these actions in order.
    Actions {
        list: InputId,
        cue: CueId,
        actions: Vec<Action>,
    },
    /// Tell the clients.
    Event {
        list: InputId,
        cue: Option<CueId>,
        what: CueEvent,
    },
    /// A panic (`Fade`) or hard stop (`Hard`) happened.
    Panic { mode: StopMode },
    /// Stop every sound and picture, faded out over `fade_ms`.
    StopPlayback { fade_ms: u32 },
}

/// One started cue.
#[derive(Clone, Debug)]
struct Run {
    id: u64,
    list: InputId,
    cue: CueId,
    /// The group run that started it.
    parent: Option<u64>,
    start: Instant,
    fire_at: Instant,
    fired: bool,
    /// Known once fired; a group's when its last child is done.
    end_at: Option<Instant>,
    ended: bool,
    /// `None` for children of a timeline group.
    follow: Follow,
    follow_at: Option<Instant>,
    followed: bool,
    /// Pre wait plus duration, when known.
    total: Option<Duration>,
    kind: CueKind,
    /// A sound cue's fade when it is stopped (SND.2).
    fade_out_ms: u32,
}

impl Run {
    /// Still to show and to wait for: not ended, or a follow to come.
    fn live(&self) -> bool {
        !self.ended || (self.follow == Follow::AfterWait && !self.followed)
    }
}

/// How late cue steps ran against their due time (RUN.8, SNF.4), for
/// `/metrics`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Lateness {
    pub steps: u64,
    pub max: Duration,
    /// Steps later than [`LATE_BUDGET`].
    pub over_budget: u64,
}

/// What RUN.8 allows a wait or follow to be off.
pub const LATE_BUDGET: Duration = Duration::from_millis(10);

/// A due step of a run.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Step {
    Fire,
    Follow,
    End,
}

#[derive(Debug, Default)]
pub struct Runner {
    /// 0-based top-level standby per list; missing = 0.
    standby: HashMap<InputId, usize>,
    active: Option<InputId>,
    runs: Vec<Run>,
    last_go: Option<Instant>,
    /// Buttons that gave a GO and are not let go yet (RUN.6).
    held: HashSet<Hold>,
    next_id: u64,
    /// A panic runs until then; another panic before is a hard stop.
    panic_until: Option<Instant>,
    late: Lateness,
    /// Cues that started since the show was opened or reset (SUI.12).
    ran: BTreeSet<CueId>,
}

/// A button whose release is reported: a MIDI note or controller, an
/// OSC sender, a Console key (RUN.6). Each waits for its own release.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Hold(pub u64);

impl Hold {
    /// The button named `key`, e.g. `midi 1 note 60`.
    pub fn of(key: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        key.hash(&mut hasher);
        Self(hasher.finish())
    }
}

/// What lets a GO through (RUN.6).
#[derive(Clone, Copy, Debug)]
pub struct Guard {
    /// Shortest gap to the GO before.
    pub ms: u32,
    pub rearm: GoRearm,
    /// The button that gives this GO, if its release is reported.
    pub held: Option<Hold>,
}

#[cfg(test)]
impl Guard {
    /// A gap of `ms`, no wait for a release.
    pub fn time(ms: u32) -> Self {
        Self {
            ms,
            rearm: GoRearm::Time,
            held: None,
        }
    }
}

fn bad(reason: impl Into<String>) -> ActionError {
    ActionError::BadCueList {
        reason: reason.into(),
    }
}

fn ms(ms: u32) -> Duration {
    Duration::from_millis(u64::from(ms))
}

/// Path of the cue with `id` below `cues` (indices per level).
fn path_of(cues: &[Cue], id: CueId) -> Option<Vec<usize>> {
    for (i, cue) in cues.iter().enumerate() {
        if cue.id == id {
            return Some(vec![i]);
        }
        if let Some(mut rest) = path_of(&cue.children, id) {
            rest.insert(0, i);
            return Some(rest);
        }
    }
    None
}

/// The cue at `path`.
fn at<'a>(cues: &'a [Cue], path: &[usize]) -> Option<&'a Cue> {
    let (first, rest) = path.split_first()?;
    let cue = cues.get(*first)?;
    if rest.is_empty() {
        return Some(cue);
    }
    at(&cue.children, rest)
}

/// The cues next to the one at `path` (its list or group).
fn siblings<'a>(cues: &'a [Cue], path: &[usize]) -> Option<&'a [Cue]> {
    match path.split_last()? {
        (_, []) => Some(cues),
        (_, parent) => at(cues, parent).map(|p| p.children.as_slice()),
    }
}

/// The cue named `text` (id or number) anywhere below `cues`.
fn find_cue(cues: &[Cue], text: &str) -> Option<CueId> {
    for cue in cues {
        if cue.id.to_string() == text || (!cue.number.is_empty() && cue.number == text) {
            return Some(cue.id);
        }
        if let Some(id) = find_cue(&cue.children, text) {
            return Some(id);
        }
    }
    None
}

/// The cue named `text` (id or number) anywhere below `cues`, to change.
pub(crate) fn find_cue_mut<'a>(cues: &'a mut [Cue], text: &str) -> Option<&'a mut Cue> {
    for cue in cues {
        if cue.id.to_string() == text || (!cue.number.is_empty() && cue.number == text) {
            return Some(cue);
        }
        if let Some(found) = find_cue_mut(&mut cue.children, text) {
            return Some(found);
        }
    }
    None
}

/// Last cue of the follow chain that starts at `start`.
fn chain_end(cues: &[Cue], start: usize) -> usize {
    let mut i = start;
    while i + 1 < cues.len() && cues[i].follow != Follow::None {
        i += 1;
    }
    i
}

impl Runner {
    /// 0-based standby of `list`.
    pub fn standby(&self, list: InputId) -> usize {
        self.standby.get(&list).copied().unwrap_or(0)
    }

    /// The active list (RUN.7); the first one while none is set.
    /// The list GO goes to: the chosen one, else the first; never a pad
    /// list (RUN.12).
    pub fn active(&self, lists: &[ShowCueList]) -> Option<usize> {
        self.active
            .and_then(|id| lists.iter().position(|l| l.id == id && !l.pads))
            .or_else(|| lists.iter().position(|l| !l.pads))
    }

    pub fn set_active(&mut self, list: InputId) {
        self.active = Some(list);
    }

    /// GO in `lists[list]` (RUN.1, RUN.6).
    pub fn go(
        &mut self,
        lists: &[ShowCueList],
        list: usize,
        now: Instant,
        guard: Guard,
    ) -> Result<Vec<Out>, ActionError> {
        let entry = &lists[list];
        if entry.pads {
            return Err(bad("pads have no GO: start each cue on its own"));
        }
        let early = self
            .last_go
            .is_some_and(|last| now.duration_since(last) < ms(guard.ms));
        let waiting = guard.rearm == GoRearm::Release
            && guard.held.is_some_and(|held| self.held.contains(&held));
        if early || waiting {
            return Ok(vec![Out::Event {
                list: entry.id,
                cue: None,
                what: CueEvent::GoIgnored,
            }]);
        }
        let standby = self.standby(entry.id);
        if standby >= entry.cues.len() {
            return Err(bad("no cues left; move standby back first"));
        }
        self.last_go = Some(now);
        if let (GoRearm::Release, Some(held)) = (guard.rearm, guard.held) {
            self.held.insert(held);
        }
        let mut out = Vec::new();
        self.go_unguarded(lists, list, now, &mut out);
        out.extend(self.tick(lists, now));
        Ok(out)
    }

    /// Button `held` was let go: its next press may give GO (RUN.6).
    pub fn release(&mut self, held: Hold) {
        self.held.remove(&held);
    }

    /// Forget every held button (the rearm mode changed).
    pub fn release_all(&mut self) {
        self.held.clear();
    }

    /// Start the standby cue of `lists[list]` and move standby past its
    /// chain; nothing when the list is done.
    fn go_unguarded(&mut self, lists: &[ShowCueList], list: usize, t: Instant, out: &mut Vec<Out>) {
        let entry = &lists[list];
        let standby = self.standby(entry.id);
        let Some(cue) = entry.cues.get(standby) else {
            return;
        };
        let after = chain_end(&entry.cues, standby) + 1;
        self.standby.insert(entry.id, after);
        self.spawn(entry.id, cue, None, t, cue.follow, out);
    }

    /// Start the cue `text` (number or id) with its chain; standby stays.
    /// It is looked for in `list` first, then in every list.
    pub fn start(
        &mut self,
        lists: &[ShowCueList],
        list: Option<usize>,
        text: &str,
        now: Instant,
    ) -> Result<Vec<Out>, ActionError> {
        let (list, id) =
            Self::locate(lists, list, text).ok_or_else(|| bad(format!("no cue {text}")))?;
        let mut out = Vec::new();
        let cue = Self::cue(lists, list, id).expect("just found");
        let follow = cue.follow;
        self.spawn(lists[list].id, cue, None, now, follow, &mut out);
        out.extend(self.tick(lists, now));
        Ok(out)
    }

    /// Start the cue `text` like `start`, or stop it while it runs: one
    /// button per pad (RUN.12).
    pub fn toggle(
        &mut self,
        lists: &[ShowCueList],
        list: Option<usize>,
        text: &str,
        now: Instant,
    ) -> Result<Vec<Out>, ActionError> {
        let (list, id) =
            Self::locate(lists, list, text).ok_or_else(|| bad(format!("no cue {text}")))?;
        if self.runs.iter().any(|r| r.cue == id) {
            self.stop(lists, Some(list), Some(&id.to_string()), now)
        } else {
            self.start(lists, Some(list), &id.to_string(), now)
        }
    }

    /// The list and id of the cue `text`, in `list` first.
    fn locate(lists: &[ShowCueList], list: Option<usize>, text: &str) -> Option<(usize, CueId)> {
        let first = list.and_then(|l| find_cue(&lists[l].cues, text).map(|id| (l, id)));
        first.or_else(|| {
            lists
                .iter()
                .enumerate()
                .find_map(|(l, entry)| find_cue(&entry.cues, text).map(|id| (l, id)))
        })
    }

    fn cue(lists: &[ShowCueList], list: usize, id: CueId) -> Option<&Cue> {
        let cues = &lists[list].cues;
        at(cues, &path_of(cues, id)?)
    }

    /// Stop the cue `cue` (number or id) of `list`, or every cue of the
    /// list, or every cue (RUN.4). No fades run yet, so both modes stop
    /// at once.
    pub fn stop(
        &mut self,
        lists: &[ShowCueList],
        list: Option<usize>,
        cue: Option<&str>,
        now: Instant,
    ) -> Result<Vec<Out>, ActionError> {
        let list_id = list.map(|l| lists[l].id);
        let cue_id = match cue {
            Some(text) => Some(
                Self::locate(lists, list, text)
                    .ok_or_else(|| bad(format!("no cue {text}")))?
                    .1,
            ),
            None => None,
        };
        let victims: Vec<u64> = self
            .runs
            .iter()
            .filter(|r| list_id.is_none_or(|l| r.list == l))
            .filter(|r| cue_id.is_none_or(|c| r.cue == c))
            .map(|r| r.id)
            .collect();
        let mut out = Vec::new();
        self.stop_runs(&victims, now, SoundEnd::Own, &mut out);
        out.extend(self.tick(lists, now));
        Ok(out)
    }

    /// Move standby of `lists[list]` (RUN.2).
    pub fn move_standby(
        &mut self,
        lists: &[ShowCueList],
        list: usize,
        to: &StandbyMove,
    ) -> Result<(), ActionError> {
        let entry = &lists[list];
        let cues = &entry.cues;
        let now = self.standby(entry.id).min(cues.len());
        let next = match to {
            StandbyMove::Next => (now + 1).min(cues.len()),
            StandbyMove::Previous => now.saturating_sub(1),
            StandbyMove::NextSequence if now < cues.len() => chain_end(cues, now) + 1,
            StandbyMove::NextSequence => now,
            StandbyMove::PreviousSequence => {
                let mut i = now.saturating_sub(1);
                while i > 0 && cues[i - 1].follow != Follow::None {
                    i -= 1;
                }
                i
            }
            StandbyMove::Start => 0,
            StandbyMove::Index(n) => (*n as usize)
                .checked_sub(1)
                .filter(|i| *i < cues.len())
                .ok_or_else(|| bad(format!("no cue {n}")))?,
            // A cue inside a group: its top-level group (into groups is S2).
            StandbyMove::Cue(text) => {
                let id = find_cue(cues, text).ok_or_else(|| bad(format!("no cue {text}")))?;
                path_of(cues, id).expect("just found")[0]
            }
        };
        self.standby.insert(entry.id, next);
        Ok(())
    }

    /// Stop everything at once, every standby to the start (RUN.11).
    pub fn reset(&mut self, now: Instant) -> Vec<Out> {
        let all: Vec<u64> = self.runs.iter().map(|r| r.id).collect();
        let mut out = Vec::new();
        self.stop_runs(&all, now, SoundEnd::Caller, &mut out);
        out.push(Out::StopPlayback { fade_ms: 0 });
        self.runs.clear();
        self.standby.clear();
        self.last_go = None;
        self.panic_until = None;
        self.ran.clear();
        out
    }

    /// Panic (SAF.1): every cue of every list stops, waits and follows
    /// with them; standby stays. A second panic within `panic_ms` is a
    /// hard stop. Then the emergency list's cues all start (SAF.4).
    /// Nothing fades yet (sound and pictures come with step 2), so both
    /// stop at once.
    pub fn panic(
        &mut self,
        lists: &[ShowCueList],
        emergency: Option<usize>,
        now: Instant,
        panic_ms: u32,
    ) -> Vec<Out> {
        let again = self.panic_until.is_some_and(|until| now < until);
        if again {
            return self.hard_stop(lists, emergency, now);
        }
        self.panic_until = Some(now + ms(panic_ms));
        self.stop_everything(lists, emergency, now, StopMode::Fade, panic_ms)
    }

    /// Hard stop (SAF.3): everything at once; the emergency list runs.
    pub fn hard_stop(
        &mut self,
        lists: &[ShowCueList],
        emergency: Option<usize>,
        now: Instant,
    ) -> Vec<Out> {
        self.panic_until = None;
        self.stop_everything(lists, emergency, now, StopMode::Hard, 0)
    }

    fn stop_everything(
        &mut self,
        lists: &[ShowCueList],
        emergency: Option<usize>,
        now: Instant,
        mode: StopMode,
        fade_ms: u32,
    ) -> Vec<Out> {
        let all: Vec<u64> = self.runs.iter().map(|r| r.id).collect();
        let mut out = vec![Out::Panic { mode }];
        self.stop_runs(&all, now, SoundEnd::Caller, &mut out);
        // Every sound and picture, also those of actions and of the panic
        // before (a hard stop cuts what still fades out).
        out.push(Out::StopPlayback { fade_ms });
        if let Some(list) = emergency.and_then(|l| lists.get(l)) {
            for cue in &list.cues {
                self.spawn(list.id, cue, None, now, Follow::None, &mut out);
            }
        }
        out.extend(self.tick(lists, now));
        out
    }

    /// Standby of every list from the cue ids the show kept (SAF.6); a
    /// list without one starts at the top.
    pub fn restore_standby(&mut self, lists: &[ShowCueList]) {
        for list in lists {
            let at = list
                .standby
                .and_then(|id| list.cues.iter().position(|c| c.id == id));
            if let Some(at) = at {
                self.standby.insert(list.id, at);
            }
        }
    }

    /// The cue standby is on, `None` past the end.
    pub fn standby_cue(&self, list: &ShowCueList) -> Option<CueId> {
        list.cues.get(self.standby(list.id)).map(|c| c.id)
    }

    /// `list` got new cues instead of `old`: standby stays on the cue it
    /// was on, when that cue is still there (IMP.5).
    pub fn keep_standby(&mut self, list: &ShowCueList, old: &[Cue]) {
        let standby = self.standby(list.id);
        let Some(was) = old.get(standby) else {
            return;
        };
        if let Some(now) = list.cues.iter().position(|c| c.id == was.id) {
            self.standby.insert(list.id, now);
        }
    }

    /// After a list changed: standby at most one past its end; runs of
    /// cues that are gone are dropped, and a group left without a running
    /// child ends at `now` (its follow goes on).
    pub fn list_changed(&mut self, lists: &[ShowCueList], now: Instant) {
        for entry in lists {
            if let Some(standby) = self.standby.get_mut(&entry.id) {
                *standby = (*standby).min(entry.cues.len());
            }
        }
        self.runs.retain(|r| {
            lists
                .iter()
                .find(|l| l.id == r.list)
                .is_some_and(|l| path_of(&l.cues, r.cue).is_some())
        });
        self.standby
            .retain(|id, _| lists.iter().any(|l| l.id == *id));
        let groups: Vec<u64> = self
            .runs
            .iter()
            .filter(|r| r.kind == CueKind::Group && r.fired && !r.ended && r.end_at.is_none())
            .map(|r| r.id)
            .collect();
        for group in groups {
            self.parent_check(Some(group), now);
        }
    }

    /// How late the steps ran so far.
    pub fn lateness(&self) -> Lateness {
        self.late
    }

    /// The next time [`Runner::tick`] has something to do.
    pub fn deadline(&self) -> Option<Instant> {
        self.runs
            .iter()
            .filter_map(|r| self.due(r).map(|(t, _)| t))
            .min()
    }

    /// The step `run` waits for, and when.
    fn due(&self, run: &Run) -> Option<(Instant, Step)> {
        if !run.fired {
            return Some((run.fire_at, Step::Fire));
        }
        let follow = (run.follow == Follow::AfterWait && !run.followed)
            .then_some(run.follow_at)
            .flatten()
            .map(|t| (t, Step::Follow));
        let end = (!run.ended)
            .then_some(run.end_at)
            .flatten()
            .map(|t| (t, Step::End));
        follow.into_iter().chain(end).min()
    }

    /// Everything due by `now`, each at its own time.
    pub fn tick(&mut self, lists: &[ShowCueList], now: Instant) -> Vec<Out> {
        let mut out = Vec::new();
        loop {
            let next = self
                .runs
                .iter()
                .filter_map(|r| self.due(r).map(|(t, step)| (t, step, r.id)))
                .filter(|(t, _, _)| *t <= now)
                .min();
            let Some((t, step, id)) = next else {
                break;
            };
            let late = now.saturating_duration_since(t);
            self.late.steps += 1;
            self.late.max = self.late.max.max(late);
            if late > LATE_BUDGET {
                self.late.over_budget += 1;
            }
            match step {
                Step::Fire => self.fire(lists, id, t, &mut out),
                Step::Follow => self.follow(lists, id, t, &mut out),
                Step::End => self.end(lists, id, t, &mut out),
            }
            self.runs.retain(Run::live);
        }
        out
    }

    /// Start `cue` of `list`: its pre wait begins at `t`.
    fn spawn(
        &mut self,
        list: InputId,
        cue: &Cue,
        parent: Option<u64>,
        t: Instant,
        follow: Follow,
        out: &mut Vec<Out>,
    ) {
        self.next_id += 1;
        let pre = ms(cue.pre_wait_ms);
        let total = match cue.kind {
            CueKind::Group => None,
            CueKind::Sound | CueKind::Picture => cue.duration_ms.map(|d| pre + ms(d)),
            CueKind::Wait | CueKind::Actions | CueKind::Fade => {
                Some(pre + ms(cue.duration_ms.unwrap_or(0)))
            }
            CueKind::Note | CueKind::Start => Some(pre),
        };
        self.runs.push(Run {
            id: self.next_id,
            list,
            cue: cue.id,
            parent,
            start: t,
            fire_at: t + pre,
            fired: false,
            end_at: None,
            ended: false,
            follow,
            follow_at: None,
            followed: false,
            total,
            kind: cue.kind,
            fade_out_ms: match (&cue.sound, &cue.picture) {
                (Some(sound), _) => sound.fade_out_ms,
                (_, Some(picture)) => picture.fade_out_ms,
                _ => 0,
            },
        });
        self.ran.insert(cue.id);
        out.push(Out::Event {
            list,
            cue: Some(cue.id),
            what: CueEvent::Started,
        });
    }

    /// Cues that started since the show was opened or reset (SUI.12).
    pub fn ran(&self) -> Vec<CueId> {
        self.ran.iter().copied().collect()
    }

    fn run_mut(&mut self, id: u64) -> Option<&mut Run> {
        self.runs.iter_mut().find(|r| r.id == id)
    }

    /// The pre wait of run `id` is over: its action.
    fn fire(&mut self, lists: &[ShowCueList], id: u64, t: Instant, out: &mut Vec<Out>) {
        let run = self.run_mut(id).expect("due run").clone();
        let Some(list) = lists.iter().position(|l| l.id == run.list) else {
            self.runs.retain(|r| r.id != id);
            return;
        };
        let Some(cue) = Self::cue(lists, list, run.cue).cloned() else {
            self.runs.retain(|r| r.id != id);
            return;
        };
        if let Some(stop) = cue.stop_others {
            let victims = self.others(&run, stop.scope);
            self.stop_runs(&victims, t, SoundEnd::Fade(stop.fade_ms), out);
        }

        let mut end_at = Some(t);
        match cue.kind {
            CueKind::Actions => {
                out.push(Out::Actions {
                    list: run.list,
                    cue: cue.id,
                    actions: cue.actions.clone(),
                });
                end_at = Some(t + ms(cue.duration_ms.unwrap_or(0)));
            }
            CueKind::Note => {}
            CueKind::Wait => end_at = Some(t + ms(cue.duration_ms.unwrap_or(0))),
            CueKind::Start => self.fire_start(lists, list, &cue, t, out),
            CueKind::Sound => {
                let sound = cue.sound.clone().unwrap_or_default();
                out.push(Out::Actions {
                    list: run.list,
                    cue: cue.id,
                    actions: vec![Action::PlaySound {
                        id: media_id(cue.id),
                        sound,
                    }],
                });
                // Without a duration it ends with its file (`media_ended`).
                end_at = cue.duration_ms.map(|d| t + ms(d));
            }
            CueKind::Picture => {
                let picture = cue.picture.clone().expect("checked: a picture cue has one");
                out.push(Out::Actions {
                    list: run.list,
                    cue: cue.id,
                    actions: vec![Action::ShowPicture {
                        id: media_id(cue.id),
                        picture,
                    }],
                });
                // Without a duration a still image or held video runs
                // until it is stopped, a video with `end: stop` until its
                // end (`media_ended`).
                end_at = cue.duration_ms.map(|d| t + ms(d));
            }
            CueKind::Fade => {
                self.fire_fade(lists, list, &cue, out);
                end_at = Some(t + ms(cue.duration_ms.unwrap_or(0)));
            }
            CueKind::Group if cue.children.is_empty() => {}
            CueKind::Group => {
                end_at = None;
                match cue.mode.unwrap_or_default() {
                    GroupMode::Timeline => {
                        for child in &cue.children {
                            self.spawn(run.list, child, Some(id), t, Follow::None, out);
                        }
                    }
                    GroupMode::First => {
                        let first = &cue.children[0];
                        self.spawn(run.list, first, Some(id), t, first.follow, out);
                    }
                }
            }
        }

        let run = self.run_mut(id).expect("fired run");
        run.fired = true;
        run.end_at = end_at;
        if run.follow == Follow::AfterWait {
            run.follow_at = Some(t + ms(cue.post_wait_ms));
        }
    }

    /// A fade cue's action (SND.6, VIS.4, VIS.7): a sound cue's level, a
    /// picture cue's opacity, a zone's level or a stage's brightness. A
    /// cue that is gone, or a value its target does not have, fails the
    /// fade; times go on (CUE.19).
    fn fire_fade(&mut self, lists: &[ShowCueList], own: usize, cue: &Cue, out: &mut Vec<Out>) {
        let Some(fade) = cue.fade.clone() else {
            out.push(failed(lists[own].id, cue.id, "fade: no target"));
            return;
        };
        let duration_ms = cue.duration_ms.unwrap_or(0);
        let fail = |what: String| failed(lists[own].id, cue.id, &format!("fade: {what}"));
        let action = match &fade.target {
            FadeTarget::Cue(text) => {
                let Some((list, id)) = Self::locate(lists, Some(own), text) else {
                    out.push(fail(format!("no cue {text}")));
                    return;
                };
                let kind = Self::cue(lists, list, id).map(|c| c.kind);
                match (kind, fade.gain_db, fade.opacity) {
                    (Some(CueKind::Sound), Some(gain_db), _) => Action::FadeSound {
                        id: media_id(id),
                        gain_db,
                        duration_ms,
                        then: fade.then,
                    },
                    (Some(CueKind::Picture), _, Some(opacity)) => Action::FadePicture {
                        id: media_id(id),
                        opacity,
                        duration_ms,
                        then: fade.then,
                    },
                    (Some(CueKind::Sound), None, _) => {
                        out.push(fail(format!("cue {text} is a sound: set gain_db")));
                        return;
                    }
                    (Some(CueKind::Picture), _, None) => {
                        out.push(fail(format!("cue {text} is a picture: set opacity")));
                        return;
                    }
                    _ => {
                        out.push(fail(format!("cue {text} plays no sound or picture")));
                        return;
                    }
                }
            }
            FadeTarget::Zone(zone) => Action::SetZoneLevel {
                zone: zone.clone(),
                gain_db: fade.gain_db,
                mute: None,
                duration_ms,
            },
            FadeTarget::Stage(stage) => Action::SetStageLevel {
                stage: stage.clone(),
                brightness: fade.brightness.unwrap_or(1.0),
                duration_ms,
            },
        };
        out.push(Out::Actions {
            list: lists[own].id,
            cue: cue.id,
            actions: vec![action],
        });
    }

    /// The engine says the sound or picture of `cue` is over (its file
    /// ended, or a fade stopped it): its runs end now.
    pub fn media_ended(&mut self, lists: &[ShowCueList], cue: CueId, now: Instant) -> Vec<Out> {
        let media = |kind| matches!(kind, CueKind::Sound | CueKind::Picture);
        for run in self
            .runs
            .iter_mut()
            .filter(|r| r.cue == cue && media(r.kind) && r.fired && !r.ended)
        {
            run.end_at = Some(run.end_at.map_or(now, |at| at.min(now)));
        }
        self.tick(lists, now)
    }

    /// A start cue's target (CUE.10): a cue, or a GO in a list. A target
    /// that is gone fails the cue; times and follows go on (CUE.19).
    fn fire_start(
        &mut self,
        lists: &[ShowCueList],
        own: usize,
        cue: &Cue,
        t: Instant,
        out: &mut Vec<Out>,
    ) {
        let target = cue.target.clone().unwrap_or_default();
        let list = match &target.list {
            Some(reference) => match find_list(lists, reference) {
                Some(list) => Some(list),
                None => {
                    out.push(failed(lists[own].id, cue.id, "start: no such cue list"));
                    return;
                }
            },
            None => None,
        };
        match (target.cue.as_deref(), list) {
            (Some(text), _) => match Self::locate(lists, list.or(Some(own)), text) {
                Some((l, id)) => {
                    let started = Self::cue(lists, l, id).expect("just found");
                    self.spawn(lists[l].id, started, None, t, started.follow, out);
                }
                None => out.push(failed(
                    lists[own].id,
                    cue.id,
                    &format!("start: no cue {text}"),
                )),
            },
            (None, Some(l)) => self.go_unguarded(lists, l, t, out),
            (None, None) => out.push(failed(lists[own].id, cue.id, "start: no target")),
        }
    }

    /// The post wait of run `id` is over: start the next cue.
    fn follow(&mut self, lists: &[ShowCueList], id: u64, t: Instant, out: &mut Vec<Out>) {
        let run = self.run_mut(id).expect("due run");
        run.followed = true;
        let run = run.clone();
        self.start_next(lists, &run, t, out);
        if run.ended {
            self.parent_check(run.parent, t);
        }
    }

    /// Run `id` ended.
    fn end(&mut self, lists: &[ShowCueList], id: u64, t: Instant, out: &mut Vec<Out>) {
        let run = self.run_mut(id).expect("due run");
        run.ended = true;
        let run = run.clone();
        out.push(Out::Event {
            list: run.list,
            cue: Some(run.cue),
            what: CueEvent::Ended,
        });
        if run.follow == Follow::AfterEnd {
            self.start_next(lists, &run, t, out);
        }
        self.parent_check(run.parent, t);
    }

    /// The cue after `run`'s in its list or group, started at `t`.
    fn start_next(&mut self, lists: &[ShowCueList], run: &Run, t: Instant, out: &mut Vec<Out>) {
        let Some(entry) = lists.iter().find(|l| l.id == run.list) else {
            return;
        };
        let Some(path) = path_of(&entry.cues, run.cue) else {
            return;
        };
        let index = *path.last().expect("a path has a cue");
        let Some(next) = siblings(&entry.cues, &path).and_then(|s| s.get(index + 1)) else {
            return;
        };
        self.spawn(run.list, next, run.parent, t, next.follow, out);
    }

    /// A group ends when none of its children is live any more.
    fn parent_check(&mut self, parent: Option<u64>, t: Instant) {
        let Some(parent) = parent else {
            return;
        };
        if self
            .runs
            .iter()
            .any(|r| r.parent == Some(parent) && r.live())
        {
            return;
        }
        if let Some(group) = self.run_mut(parent) {
            if group.fired && group.end_at.is_none() {
                group.end_at = Some(t);
            }
        }
    }

    /// Runs `scope` stops for `run` (CUE.11): never itself or the groups
    /// it runs in.
    fn others(&self, run: &Run, scope: StopScope) -> Vec<u64> {
        let mut keep = vec![run.id];
        let mut up = run.parent;
        while let Some(id) = up {
            keep.push(id);
            up = self.runs.iter().find(|r| r.id == id).and_then(|r| r.parent);
        }
        self.runs
            .iter()
            .filter(|r| !keep.contains(&r.id))
            .filter(|r| match scope {
                StopScope::Siblings => r.list == run.list && r.parent == run.parent,
                StopScope::List => r.list == run.list,
                StopScope::All => true,
            })
            .map(|r| r.id)
            .collect()
    }

    /// Stop runs `ids` and everything they started; their sounds as
    /// `sounds` says.
    fn stop_runs(&mut self, ids: &[u64], t: Instant, sounds: SoundEnd, out: &mut Vec<Out>) {
        let mut stopped: Vec<u64> = ids.to_vec();
        let mut i = 0;
        while i < stopped.len() {
            let id = stopped[i];
            stopped.extend(
                self.runs
                    .iter()
                    .filter(|r| r.parent == Some(id) && !stopped.contains(&r.id))
                    .map(|r| r.id)
                    .collect::<Vec<_>>(),
            );
            i += 1;
        }
        let parents: Vec<Option<u64>> = self
            .runs
            .iter()
            .filter(|r| stopped.contains(&r.id))
            .map(|r| r.parent)
            .collect();
        for run in self.runs.iter().filter(|r| stopped.contains(&r.id)) {
            let fade_ms = match sounds {
                SoundEnd::Own => Some(run.fade_out_ms),
                SoundEnd::Fade(ms) => Some(ms),
                SoundEnd::Caller => None,
            };
            let stop = match (run.kind, run.fired, fade_ms) {
                (CueKind::Sound, true, Some(fade_ms)) => Some(Action::StopSound {
                    id: Some(media_id(run.cue)),
                    fade_ms,
                }),
                (CueKind::Picture, true, Some(fade_ms)) => Some(Action::StopPicture {
                    id: Some(media_id(run.cue)),
                    fade_ms,
                }),
                _ => None,
            };
            if let Some(stop) = stop {
                out.push(Out::Actions {
                    list: run.list,
                    cue: run.cue,
                    actions: vec![stop],
                });
            }
            out.push(Out::Event {
                list: run.list,
                cue: Some(run.cue),
                what: CueEvent::Stopped,
            });
        }
        self.runs.retain(|r| !stopped.contains(&r.id));
        for parent in parents {
            if parent.is_some_and(|p| !stopped.contains(&p)) {
                self.parent_check(parent, t);
            }
        }
    }

    /// Cues that run, in start order (RUN.3).
    pub fn running(&self, lists: &[ShowCueList], now: Instant) -> Vec<RunningCue> {
        self.runs
            .iter()
            .filter_map(|run| {
                let list = lists.iter().position(|l| l.id == run.list)?;
                let cue = Self::cue(lists, list, run.cue)?;
                let elapsed = now.saturating_duration_since(run.start).as_millis() as u64;
                let phase = match (run.fired, run.ended) {
                    (false, _) => RunPhase::PreWait,
                    (true, false) => RunPhase::Running,
                    (true, true) => RunPhase::PostWait,
                };
                Some(RunningCue {
                    list: run.list,
                    cue: run.cue,
                    number: cue.number.clone(),
                    name: cue.display_name(),
                    phase,
                    elapsed_ms: elapsed,
                    total_ms: run.total.map(|d| d.as_millis() as u64),
                    remaining_ms: run
                        .total
                        .map(|d| (d.as_millis() as u64).saturating_sub(elapsed)),
                })
            })
            .collect()
    }
}

/// How stopped runs stop their sounds and pictures.
#[derive(Clone, Copy, Debug)]
enum SoundEnd {
    /// Each with its cue's fade out.
    Own,
    Fade(u32),
    /// Not one by one: the caller stops them all.
    Caller,
}

/// The engine's id of a sound or picture cue's sound or picture.
pub fn media_id(cue: CueId) -> String {
    cue.to_string()
}

fn failed(list: InputId, cue: CueId, reason: &str) -> Out {
    Out::Event {
        list,
        cue: Some(cue),
        what: CueEvent::Failed {
            reason: reason.to_string(),
        },
    }
}

/// A cue list by number (1-based), id or name.
pub fn find_list(lists: &[ShowCueList], list: &InputRef) -> Option<usize> {
    match list {
        InputRef::Number(n) => (*n as usize).checked_sub(1).filter(|i| *i < lists.len()),
        InputRef::Text(text) => lists
            .iter()
            .position(|l| l.id.to_string() == *text)
            .or_else(|| lists.iter().position(|l| l.name == *text)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::cue::{CueTarget, StopOthers, GO_GUARD_DEFAULT_MS};
    use lmix_types::StandbyMove;

    fn cut(name: &str) -> Cue {
        Cue::actions(name, vec![Action::Cut])
    }

    fn wait(name: &str, duration_ms: u32) -> Cue {
        Cue {
            name: name.into(),
            duration_ms: Some(duration_ms),
            ..Cue::empty(CueKind::Wait)
        }
    }

    fn list(cues: Vec<Cue>) -> ShowCueList {
        ShowCueList {
            id: InputId::new(),
            name: "Main".into(),
            cues,
            source: None,
            standby: None,
            pads: false,
        }
    }

    /// Names of the cues whose actions ran, in order.
    fn fired(lists: &[ShowCueList], out: &[Out]) -> Vec<String> {
        out.iter()
            .filter_map(|o| match o {
                Out::Actions { list, cue, .. } => {
                    let l = lists.iter().position(|e| e.id == *list)?;
                    Runner::cue(lists, l, *cue).map(|c| c.name.clone())
                }
                Out::Event { .. } | Out::Panic { .. } | Out::StopPlayback { .. } => None,
            })
            .collect()
    }

    fn events(out: &[Out]) -> Vec<CueEvent> {
        out.iter()
            .filter_map(|o| match o {
                Out::Event { what, .. } => Some(what.clone()),
                Out::Actions { .. } | Out::Panic { .. } | Out::StopPlayback { .. } => None,
            })
            .collect()
    }

    fn after(t: Instant, ms_: u64) -> Instant {
        t + Duration::from_millis(ms_)
    }

    #[test]
    fn go_runs_standby_and_moves_on() {
        let lists = vec![list(vec![cut("A"), cut("B")])];
        let mut runner = Runner::default();
        let t = Instant::now();

        let out = runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        assert_eq!(fired(&lists, &out), ["A"]);
        assert_eq!(runner.standby(lists[0].id), 1);
        assert!(
            runner.running(&lists, t).is_empty(),
            "an actions cue ends at once"
        );
        assert_eq!(events(&out), [CueEvent::Started, CueEvent::Ended]);

        runner.go(&lists, 0, after(t, 1), Guard::time(0)).unwrap();
        assert_eq!(runner.standby(lists[0].id), 2);
        assert!(
            runner.go(&lists, 0, after(t, 2), Guard::time(0)).is_err(),
            "no cues left"
        );
    }

    fn sound(name: &str, fade_out_ms: u32) -> Cue {
        Cue {
            name: name.into(),
            sound: Some(lmix_types::SoundCue {
                file: format!("/fx/{name}.wav"),
                fade_out_ms,
                ..Default::default()
            }),
            ..Cue::empty(CueKind::Sound)
        }
    }

    /// Actions the outs ask to run.
    fn actions(out: &[Out]) -> Vec<Action> {
        out.iter()
            .flat_map(|o| match o {
                Out::Actions { actions, .. } => actions.clone(),
                _ => Vec::new(),
            })
            .collect()
    }

    #[test]
    fn a_sound_cue_runs_until_its_file_ends() {
        let mut rain = sound("Rain", 0);
        rain.follow = Follow::AfterEnd;
        let lists = vec![list(vec![rain, cut("Lights")])];
        let rain_id = lists[0].cues[0].id;
        let mut runner = Runner::default();
        let t = Instant::now();

        let out = runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        assert!(matches!(
            &actions(&out)[..],
            [Action::PlaySound { id, sound }] if *id == rain_id.to_string()
                && sound.file == "/fx/Rain.wav"
        ));
        assert_eq!(runner.running(&lists, after(t, 5000)).len(), 1);
        assert_eq!(runner.running(&lists, after(t, 5000))[0].total_ms, None);

        let out = runner.media_ended(&lists, rain_id, after(t, 8000));
        assert_eq!(
            fired(&lists, &out),
            ["Lights"],
            "after_end follows the file"
        );
        assert!(runner.running(&lists, after(t, 8000)).is_empty());
    }

    #[test]
    fn a_stopped_sound_fades_with_its_cue_or_the_stopper() {
        let mut rain = sound("Rain", 2000);
        rain.number = "1".into();
        let mut wind = sound("Wind", 0);
        wind.number = "2".into();
        wind.stop_others = Some(StopOthers {
            scope: StopScope::List,
            fade_ms: 500,
        });
        let lists = vec![list(vec![rain, wind])];
        let rain = lists[0].cues[0].id.to_string();
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        let out = runner
            .stop(&lists, Some(0), Some("1"), after(t, 10))
            .unwrap();
        assert_eq!(
            actions(&out),
            [Action::StopSound {
                id: Some(rain.clone()),
                fade_ms: 2000
            }],
            "its own fade out"
        );

        runner.start(&lists, Some(0), "1", after(t, 20)).unwrap();
        let out = runner.start(&lists, Some(0), "2", after(t, 30)).unwrap();
        assert!(
            actions(&out).contains(&Action::StopSound {
                id: Some(rain),
                fade_ms: 500
            }),
            "the fade of the cue that stops it"
        );
    }

    #[test]
    fn panic_fades_every_sound_and_a_second_cuts_them() {
        let lists = vec![list(vec![sound("Rain", 2000)])];
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        let out = runner.panic(&lists, None, after(t, 100), 1500);
        assert!(out.contains(&Out::StopPlayback { fade_ms: 1500 }));
        assert!(
            actions(&out).is_empty(),
            "no stop one by one: the panic stops them all"
        );
        let out = runner.panic(&lists, None, after(t, 200), 1500);
        assert!(out.contains(&Out::StopPlayback { fade_ms: 0 }));
    }

    #[test]
    fn a_fade_cue_fades_its_cue_or_zone() {
        let fade = |name: &str, target: FadeTarget| Cue {
            name: name.into(),
            duration_ms: Some(3000),
            fade: Some(lmix_types::FadeCue {
                target,
                gain_db: Some(-20.0),
                opacity: None,
                brightness: None,
                then: lmix_types::AfterFade::Stop,
            }),
            ..Cue::empty(CueKind::Fade)
        };
        let mut rain = sound("Rain", 0);
        rain.number = "12".into();
        let lists = vec![list(vec![
            rain,
            fade("Down", FadeTarget::Cue("12".into())),
            fade("Hall", FadeTarget::Zone("Hall".into())),
            fade("Lost", FadeTarget::Cue("99".into())),
        ])];
        let rain = lists[0].cues[0].id.to_string();
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        let out = runner
            .go(&lists, 0, after(t, 1000), Guard::time(0))
            .unwrap();
        assert_eq!(
            actions(&out),
            [Action::FadeSound {
                id: rain,
                gain_db: -20.0,
                duration_ms: 3000,
                then: lmix_types::AfterFade::Stop
            }]
        );
        assert_eq!(
            runner
                .running(&lists, after(t, 1000))
                .iter()
                .find(|r| r.name == "Down")
                .and_then(|r| r.total_ms),
            Some(3000),
            "a fade runs as long as it fades"
        );
        let out = runner
            .go(&lists, 0, after(t, 2000), Guard::time(0))
            .unwrap();
        assert!(matches!(
            &actions(&out)[..],
            [Action::SetZoneLevel { zone, gain_db: Some(g), .. }] if zone == "Hall" && *g == -20.0
        ));
        let out = runner
            .go(&lists, 0, after(t, 3000), Guard::time(0))
            .unwrap();
        assert!(events(&out)
            .iter()
            .any(|e| matches!(e, CueEvent::Failed { .. })));
    }

    fn picture(name: &str, end: lmix_types::PictureEnd) -> Cue {
        Cue {
            name: name.into(),
            picture: Some(lmix_types::PictureCue {
                file: format!("/pics/{name}.png"),
                stage: "Back".into(),
                layer: 1,
                opacity: 1.0,
                fit: Default::default(),
                end,
                fade_in_ms: 0,
                fade_out_ms: 1000,
                repeat: Default::default(),
            }),
            ..Cue::empty(CueKind::Picture)
        }
    }

    #[test]
    fn a_picture_cue_shows_until_stopped_and_fades_with_its_cue() {
        let mut forest = picture("Forest", lmix_types::PictureEnd::Hold);
        forest.number = "1".into();
        let mut out_ = Cue {
            number: "2".into(),
            duration_ms: Some(2000),
            fade: Some(lmix_types::FadeCue {
                target: FadeTarget::Cue("1".into()),
                gain_db: None,
                opacity: Some(0.0),
                brightness: None,
                then: lmix_types::AfterFade::Stop,
            }),
            ..Cue::empty(CueKind::Fade)
        };
        out_.name = "Out".into();
        let dark = Cue {
            number: "3".into(),
            duration_ms: Some(500),
            fade: Some(lmix_types::FadeCue {
                target: FadeTarget::Stage("Back".into()),
                gain_db: None,
                opacity: None,
                brightness: Some(0.0),
                then: lmix_types::AfterFade::Keep,
            }),
            ..Cue::empty(CueKind::Fade)
        };
        let lists = vec![list(vec![forest, out_, dark])];
        let forest = lists[0].cues[0].id.to_string();
        let mut runner = Runner::default();
        let t = Instant::now();

        let out = runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        assert!(matches!(
            &actions(&out)[..],
            [Action::ShowPicture { id, picture }] if *id == forest && picture.stage == "Back"
        ));
        assert_eq!(
            runner.running(&lists, after(t, 60_000)).len(),
            1,
            "a still image runs until it is stopped"
        );

        let out = runner
            .go(&lists, 0, after(t, 1000), Guard::time(0))
            .unwrap();
        assert_eq!(
            actions(&out),
            [Action::FadePicture {
                id: forest.clone(),
                opacity: 0.0,
                duration_ms: 2000,
                then: lmix_types::AfterFade::Stop
            }]
        );
        let out = runner
            .go(&lists, 0, after(t, 1100), Guard::time(0))
            .unwrap();
        assert!(matches!(
            &actions(&out)[..],
            [Action::SetStageLevel { stage, brightness, duration_ms: 500 }]
                if stage == "Back" && *brightness == 0.0
        ));

        // The engine reports the picture gone after its fade: the cue ends.
        let id = lists[0].cues[0].id;
        runner.media_ended(&lists, id, after(t, 3100));
        assert!(runner
            .running(&lists, after(t, 3100))
            .iter()
            .all(|r| r.cue != id));
    }

    #[test]
    fn a_stopped_picture_fades_out_and_a_panic_stops_all() {
        let lists = vec![list(vec![picture("Forest", lmix_types::PictureEnd::Stop)])];
        let forest = lists[0].cues[0].id.to_string();
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        let out = runner.stop(&lists, Some(0), None, after(t, 10)).unwrap();
        assert_eq!(
            actions(&out),
            [Action::StopPicture {
                id: Some(forest),
                fade_ms: 1000
            }]
        );
        runner.go(&lists, 0, after(t, 20), Guard::time(0)).ok();
        let out = runner.panic(&lists, None, after(t, 30), 800);
        assert!(out.contains(&Out::StopPlayback { fade_ms: 800 }));
    }

    #[test]
    fn a_fade_needs_a_value_its_target_has() {
        let mut rain = sound("Rain", 0);
        rain.number = "1".into();
        let fade = Cue {
            number: "2".into(),
            duration_ms: Some(100),
            fade: Some(lmix_types::FadeCue {
                target: FadeTarget::Cue("1".into()),
                gain_db: None,
                opacity: Some(0.5),
                brightness: None,
                then: lmix_types::AfterFade::Keep,
            }),
            ..Cue::empty(CueKind::Fade)
        };
        let lists = vec![list(vec![rain, fade])];
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        let out = runner.go(&lists, 0, after(t, 10), Guard::time(0)).unwrap();
        assert!(actions(&out).is_empty());
        assert!(events(&out)
            .iter()
            .any(|e| matches!(e, CueEvent::Failed { reason } if reason.contains("gain_db"))));
    }

    #[test]
    fn pre_wait_holds_the_action() {
        let mut a = cut("A");
        a.pre_wait_ms = 500;
        let lists = vec![list(vec![a])];
        let mut runner = Runner::default();
        let t = Instant::now();

        let out = runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        assert!(fired(&lists, &out).is_empty());
        assert_eq!(runner.deadline(), Some(after(t, 500)));
        assert_eq!(
            runner.running(&lists, after(t, 200))[0].phase,
            RunPhase::PreWait
        );
        assert_eq!(runner.running(&lists, after(t, 200))[0].total_ms, Some(500));

        assert!(runner.tick(&lists, after(t, 499)).is_empty());
        assert_eq!(fired(&lists, &runner.tick(&lists, after(t, 500))), ["A"]);
        assert_eq!(runner.deadline(), None);
    }

    #[test]
    fn follows_after_wait_and_after_end() {
        let mut a = cut("A");
        a.follow = Follow::AfterWait;
        a.post_wait_ms = 1000;
        let mut b = wait("B", 2000);
        b.follow = Follow::AfterEnd;
        let c = cut("C");
        let d = cut("D");
        let lists = vec![list(vec![a, b, c, d])];
        let mut runner = Runner::default();
        let t = Instant::now();

        let out = runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        assert_eq!(fired(&lists, &out), ["A"]);
        assert_eq!(
            runner.standby(lists[0].id),
            3,
            "standby past the chain A, B, C"
        );

        assert!(fired(&lists, &runner.tick(&lists, after(t, 999))).is_empty());
        runner.tick(&lists, after(t, 1000));
        assert_eq!(runner.running(&lists, after(t, 1500))[0].name, "B");
        // Late tick: C still counts from B's end, not from the tick.
        let out = runner.tick(&lists, after(t, 3700));
        assert_eq!(fired(&lists, &out), ["C"]);
        assert!(runner.running(&lists, after(t, 3700)).is_empty());
    }

    #[test]
    fn a_hundred_waits_do_not_drift() {
        let cues: Vec<Cue> = (0..100)
            .map(|i| {
                let mut w = wait(&format!("w{i}"), 20);
                w.follow = Follow::AfterEnd;
                w
            })
            .chain([cut("end")])
            .collect();
        let lists = vec![list(cues)];
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        // Ticks come 7 ms late every time.
        let mut now = t;
        let mut end = None;
        while end.is_none() {
            let Some(due) = runner.deadline() else { break };
            now = due.max(now) + Duration::from_millis(7);
            if !fired(&lists, &runner.tick(&lists, now)).is_empty() {
                end = Some(now);
            }
        }

        assert_eq!(runner.deadline(), None);
        // The end cue is due at exactly 2 s; this tick came 7 ms after.
        assert_eq!(end, Some(after(t, 2007)));
        let late = runner.lateness();
        assert_eq!(late.max, Duration::from_millis(7));
        assert_eq!(late.over_budget, 0);
        assert!(late.steps > 200, "fire and end of each wait");
    }

    #[test]
    fn an_actions_cue_can_last_like_its_clip() {
        let mut clip = cut("Clip");
        clip.duration_ms = Some(12_000);
        clip.follow = Follow::AfterEnd;
        let lists = vec![list(vec![clip, cut("Next")])];
        let mut runner = Runner::default();
        let t = Instant::now();

        let out = runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        assert_eq!(fired(&lists, &out), ["Clip"], "its actions at once");
        assert_eq!(
            runner.running(&lists, after(t, 5000))[0].remaining_ms,
            Some(7000)
        );
        assert_eq!(
            fired(&lists, &runner.tick(&lists, after(t, 12_000))),
            ["Next"]
        );
    }

    #[test]
    fn timeline_group_starts_children_together() {
        let mut a = cut("A");
        a.pre_wait_ms = 100;
        a.follow = Follow::AfterEnd; // ignored in a timeline
        let mut b = cut("B");
        b.pre_wait_ms = 400;
        let group = Cue {
            name: "Scene".into(),
            follow: Follow::AfterEnd,
            children: vec![a, b],
            ..Cue::empty(CueKind::Group)
        };
        let lists = vec![list(vec![group, cut("After")])];
        let mut runner = Runner::default();
        let t = Instant::now();

        runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        assert_eq!(fired(&lists, &runner.tick(&lists, after(t, 100))), ["A"]);
        let out = runner.tick(&lists, after(t, 400));
        assert_eq!(
            fired(&lists, &out),
            ["B", "After"],
            "group ends with B, then follows"
        );
    }

    #[test]
    fn first_group_chains_its_children() {
        let mut a = wait("A", 300);
        a.follow = Follow::AfterEnd;
        let b = cut("B");
        let c = cut("C");
        let group = Cue {
            mode: Some(GroupMode::First),
            children: vec![a, b, c],
            ..Cue::empty(CueKind::Group)
        };
        let lists = vec![list(vec![group])];
        let mut runner = Runner::default();
        let t = Instant::now();

        runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        let out = runner.tick(&lists, after(t, 300));

        assert_eq!(fired(&lists, &out), ["B"], "C waits: B has no follow");
        assert!(runner.running(&lists, after(t, 300)).is_empty());
    }

    #[test]
    fn double_go_is_dropped() {
        let lists = vec![list(vec![cut("A"), cut("B")])];
        let mut runner = Runner::default();
        let t = Instant::now();
        let guard = Guard::time(GO_GUARD_DEFAULT_MS);

        runner.go(&lists, 0, t, guard).unwrap();
        let out = runner.go(&lists, 0, after(t, 120), guard).unwrap();

        assert_eq!(events(&out), [CueEvent::GoIgnored]);
        assert_eq!(runner.standby(lists[0].id), 1);
        let out = runner.go(&lists, 0, after(t, 300), guard).unwrap();
        assert_eq!(fired(&lists, &out), ["B"]);
    }

    #[test]
    fn go_waits_for_release() {
        let lists = vec![list(vec![cut("A"), cut("B"), cut("C")])];
        let mut runner = Runner::default();
        let t = Instant::now();
        let key = Hold::of("midi 1 note 60");
        let held = |hold| Guard {
            ms: 100,
            rearm: GoRearm::Release,
            held: hold,
        };

        runner.go(&lists, 0, t, held(Some(key))).unwrap();
        let out = runner
            .go(&lists, 0, after(t, 500), held(Some(key)))
            .unwrap();
        assert_eq!(events(&out), [CueEvent::GoIgnored], "still held");

        // Another way is not blocked by it, only by the time guard.
        let out = runner.go(&lists, 0, after(t, 50), held(None)).unwrap();
        assert_eq!(events(&out), [CueEvent::GoIgnored], "too soon");
        let out = runner.go(&lists, 0, after(t, 600), held(None)).unwrap();
        assert_eq!(fired(&lists, &out), ["B"]);

        runner.release(key);
        let out = runner
            .go(&lists, 0, after(t, 800), held(Some(key)))
            .unwrap();
        assert_eq!(fired(&lists, &out), ["C"]);
    }

    #[test]
    fn time_rearm_ignores_holding() {
        let lists = vec![list(vec![cut("A"), cut("B")])];
        let mut runner = Runner::default();
        let t = Instant::now();
        let key = Hold::of("osc 10.0.0.2:9000");
        let guard = Guard {
            held: Some(key),
            ..Guard::time(100)
        };

        runner.go(&lists, 0, t, guard).unwrap();
        let out = runner.go(&lists, 0, after(t, 200), guard).unwrap();

        assert_eq!(fired(&lists, &out), ["B"]);
    }

    #[test]
    fn standby_moves() {
        let mut b = cut("B");
        b.follow = Follow::AfterEnd;
        let mut c = cut("C");
        c.number = "12".into();
        let lists = vec![list(vec![cut("A"), b, c, cut("D")])];
        let mut runner = Runner::default();
        let id = lists[0].id;
        let mut go = |to: StandbyMove| {
            runner.move_standby(&lists, 0, &to).unwrap();
            runner.standby(id)
        };

        assert_eq!(go(StandbyMove::Next), 1);
        assert_eq!(go(StandbyMove::NextSequence), 3, "past B and C");
        assert_eq!(go(StandbyMove::PreviousSequence), 1, "to the start of B, C");
        assert_eq!(go(StandbyMove::Cue("12".into())), 2);
        assert_eq!(go(StandbyMove::Index(4)), 3);
        assert_eq!(go(StandbyMove::Next), 4);
        assert_eq!(go(StandbyMove::Next), 4, "at most one past the end");
        assert_eq!(go(StandbyMove::Start), 0);
        assert_eq!(go(StandbyMove::Previous), 0);
        assert!(runner
            .move_standby(&lists, 0, &StandbyMove::Index(9))
            .is_err());
    }

    #[test]
    fn start_cue_and_list_targets() {
        let mut other = cut("Other");
        other.number = "50".into();
        let start_cue = Cue {
            name: "Start 50".into(),
            target: Some(CueTarget {
                list: None,
                cue: Some("50".into()),
            }),
            ..Cue::empty(CueKind::Start)
        };
        let start_list = Cue {
            target: Some(CueTarget {
                list: Some(InputRef::Text("Side".into())),
                cue: None,
            }),
            ..Cue::empty(CueKind::Start)
        };
        let broken = Cue {
            target: Some(CueTarget {
                list: None,
                cue: Some("99".into()),
            }),
            ..Cue::empty(CueKind::Start)
        };
        let mut side = list(vec![cut("Side 1"), cut("Side 2")]);
        side.name = "Side".into();
        let lists = vec![
            list(vec![start_cue, start_list, broken, cut("x")]),
            side,
            list(vec![other]),
        ];
        let mut runner = Runner::default();
        let t = Instant::now();

        assert_eq!(
            fired(&lists, &runner.go(&lists, 0, t, Guard::time(0)).unwrap()),
            ["Other"]
        );
        assert_eq!(
            runner.standby(lists[2].id),
            0,
            "a start cue does not move standby"
        );

        assert_eq!(
            fired(
                &lists,
                &runner.go(&lists, 0, after(t, 1), Guard::time(0)).unwrap()
            ),
            ["Side 1"]
        );
        assert_eq!(
            runner.standby(lists[1].id),
            1,
            "a list target is a GO there"
        );

        let out = runner.go(&lists, 0, after(t, 2), Guard::time(0)).unwrap();
        assert!(events(&out)
            .iter()
            .any(|e| matches!(e, CueEvent::Failed { .. })));
        assert_eq!(
            runner.standby(lists[0].id),
            3,
            "a broken cue still moves on"
        );
    }

    #[test]
    fn stop_others_and_stop() {
        let mut long = wait("Atmo", 60_000);
        long.number = "1".into();
        let mut next = wait("Next", 60_000);
        next.stop_others = Some(StopOthers {
            scope: StopScope::List,
            fade_ms: 3000,
        });
        let lists = vec![list(vec![long, next, wait("Third", 60_000)])];
        let mut runner = Runner::default();
        let t = Instant::now();

        runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        let out = runner
            .go(&lists, 0, after(t, 1000), Guard::time(0))
            .unwrap();
        assert!(events(&out).contains(&CueEvent::Stopped));
        let names: Vec<String> = runner
            .running(&lists, after(t, 1000))
            .into_iter()
            .map(|r| r.name)
            .collect();
        assert_eq!(names, ["Next"]);

        runner
            .go(&lists, 0, after(t, 2000), Guard::time(0))
            .unwrap();
        // By number or id; a name is no reference.
        runner
            .stop(&lists, Some(0), Some("Third"), after(t, 2100))
            .unwrap_err();
        runner.stop(&lists, None, None, after(t, 2100)).unwrap();
        assert!(runner.running(&lists, after(t, 2100)).is_empty());
    }

    #[test]
    fn reset_stops_and_rewinds() {
        let lists = vec![list(vec![wait("A", 10_000), cut("B")])];
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        let out = runner.reset(after(t, 10));

        assert_eq!(events(&out), [CueEvent::Stopped]);
        assert_eq!(runner.standby(lists[0].id), 0);
        assert_eq!(runner.deadline(), None);
    }

    /// Pads (RUN.12): no GO, never the active list, a cue toggles.
    #[test]
    fn pads_take_no_go_and_toggle() {
        let mut rain = wait("Rain", 5000);
        rain.number = "2".into();
        let mut pads = list(vec![wait("Thunder", 5000), rain]);
        pads.pads = true;
        let lists = vec![pads, list(vec![wait("A", 100)])];
        let mut runner = Runner::default();
        let t = Instant::now();
        assert_eq!(
            runner.active(&lists),
            Some(1),
            "the first list that is not pads"
        );
        runner.set_active(lists[0].id);
        assert_eq!(runner.active(&lists), Some(1));
        assert!(runner.go(&lists, 0, t, Guard::time(0)).is_err());

        let out = runner.toggle(&lists, Some(0), "2", t).unwrap();
        assert_eq!(events(&out), [CueEvent::Started]);
        assert_eq!(runner.standby(lists[0].id), 0, "standby stays");
        let out = runner.toggle(&lists, Some(0), "2", after(t, 100)).unwrap();
        assert_eq!(events(&out), [CueEvent::Stopped]);
        assert!(runner.running(&lists, after(t, 100)).is_empty());
        let out = runner.toggle(&lists, Some(0), "2", after(t, 200)).unwrap();
        assert_eq!(events(&out), [CueEvent::Started], "and again");

        // Pads as the emergency list still run with a panic (no GO there).
        let out = runner.panic(&lists, Some(0), after(t, 300), 0);
        assert_eq!(
            events(&out)
                .iter()
                .filter(|e| **e == CueEvent::Started)
                .count(),
            2,
            "both pads start"
        );
    }

    /// A cue counts as ran from its start until a reset (SUI.12), a cue
    /// in a group too.
    #[test]
    fn started_cues_count_as_ran_until_a_reset() {
        let group = Cue {
            number: "G".into(),
            mode: Some(GroupMode::Timeline),
            children: vec![wait("In", 100)],
            ..Cue::empty(CueKind::Group)
        };
        let lists = vec![list(vec![wait("A", 10), group, cut("Later")])];
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        runner.tick(&lists, after(t, 50));
        runner.go(&lists, 0, after(t, 50), Guard::time(0)).unwrap();

        let mut want = vec![
            lists[0].cues[0].id,
            lists[0].cues[1].id,
            lists[0].cues[1].children[0].id,
        ];
        want.sort();
        assert_eq!(runner.ran(), want, "ended or running, not the standby");
        runner.reset(after(t, 60));
        assert!(runner.ran().is_empty());
    }

    #[test]
    fn panic_stops_all_and_runs_the_emergency_list() {
        let mut later = cut("Later");
        later.pre_wait_ms = 2000;
        let lists = vec![
            list(vec![wait("Atmo", 60_000), later, cut("Next")]),
            list(vec![cut("Room light"), cut("Doors")]),
        ];
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();
        runner
            .start(&lists, Some(0), &lists[0].cues[1].id.to_string(), t)
            .unwrap();

        let out = runner.panic(&lists, Some(1), after(t, 100), 1000);

        assert_eq!(
            out[0],
            Out::Panic {
                mode: StopMode::Fade
            }
        );
        assert_eq!(
            fired(&lists, &out),
            ["Room light", "Doors"],
            "the whole emergency list"
        );
        assert!(runner.running(&lists, after(t, 100)).is_empty());
        assert!(
            fired(&lists, &runner.tick(&lists, after(t, 2500))).is_empty(),
            "the pre wait is gone"
        );
        assert_eq!(runner.standby(lists[0].id), 1, "standby stays");

        // Again within the panic time: hard stop; after it: a panic again.
        let out = runner.panic(&lists, None, after(t, 600), 1000);
        assert_eq!(
            out[0],
            Out::Panic {
                mode: StopMode::Hard
            }
        );
        let out = runner.panic(&lists, None, after(t, 900), 1000);
        assert_eq!(
            out[0],
            Out::Panic {
                mode: StopMode::Fade
            }
        );
    }

    #[test]
    fn standby_comes_back_from_the_show() {
        let mut lists = vec![list(vec![cut("A"), cut("B"), cut("C")])];
        let mut runner = Runner::default();
        runner
            .move_standby(&lists, 0, &StandbyMove::Index(3))
            .unwrap();
        lists[0].standby = runner.standby_cue(&lists[0]);

        let mut after_restart = Runner::default();
        after_restart.restore_standby(&lists);

        assert_eq!(after_restart.standby(lists[0].id), 2);
        assert!(
            after_restart.running(&lists, Instant::now()).is_empty(),
            "nothing starts"
        );
    }

    #[test]
    fn standby_stays_on_its_cue_when_cues_come_before() {
        let (a, b) = (cut("A"), cut("B"));
        let mut lists = vec![list(vec![a.clone(), b.clone()])];
        let mut runner = Runner::default();
        runner
            .move_standby(&lists, 0, &StandbyMove::Index(2))
            .unwrap();

        let old = std::mem::replace(&mut lists[0].cues, vec![cut("New"), a, b]);
        runner.keep_standby(&lists[0], &old);
        runner.list_changed(&lists, Instant::now());

        assert_eq!(runner.standby(lists[0].id), 2, "still on B");
    }

    #[test]
    fn a_group_ends_when_its_running_child_is_edited_away() {
        let child = wait("Rain", 60_000);
        let group = Cue {
            name: "Storm".into(),
            follow: Follow::AfterEnd,
            children: vec![child],
            ..Cue::empty(CueKind::Group)
        };
        let mut lists = vec![list(vec![group, cut("After")])];
        let mut runner = Runner::default();
        let t = Instant::now();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        // A reload takes the child out while it runs.
        lists[0].cues[0].children = vec![cut("Flash")];
        runner.list_changed(&lists, after(t, 5));
        let out = runner.tick(&lists, after(t, 10));

        assert_eq!(
            fired(&lists, &out),
            ["After"],
            "the group ended and followed"
        );
        assert!(runner.running(&lists, after(t, 10)).is_empty());
    }

    #[test]
    fn changed_lists_keep_standby_in_range() {
        let mut lists = vec![list(vec![cut("A"), cut("B"), wait("C", 5000)])];
        let mut runner = Runner::default();
        let t = Instant::now();
        runner
            .move_standby(&lists, 0, &StandbyMove::Index(3))
            .unwrap();
        runner.go(&lists, 0, t, Guard::time(0)).unwrap();

        lists[0].cues.truncate(1);
        runner.list_changed(&lists, Instant::now());

        assert_eq!(runner.standby(lists[0].id), 1);
        assert!(
            runner.running(&lists, t).is_empty(),
            "the running cue is gone"
        );
    }
}
