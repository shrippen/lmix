//! Show state machine: applies actions, decides what the engine must do.
//!
//! Pure: no I/O, no async. Every action either fails with a reason and
//! changes nothing, or changes the show and returns engine [`Command`]s.
//!
//! ```text
//!   Action ──► Switcher::apply ──► new state + [Command] ──► engine driver
//! ```

use lmix_types::action::TRANSITION_SLOTS;
use lmix_types::input::{REF_PREVIEW, REF_PROGRAM};
use lmix_types::state::API_VERSION;
use lmix_types::{
    Action, ActionError, Effect, Input, InputId, InputRef, InputStatus, ShowState, SourceSpec,
    Transition, TransitionSlot,
};
use std::collections::HashMap;
use uuid::Uuid;

use crate::show::{Show, ShowInput};

/// What the engine has to do after an action.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// Feed `slot` from `source`, or empty it.
    SetSlot {
        slot: usize,
        source: Option<SourceSpec>,
    },
    /// Show `slot` in preview.
    Preview { slot: usize },
    /// Bring `to` on program, from `from`; `Effect::Cut` for a hard cut.
    Take {
        from: usize,
        to: usize,
        effect: Effect,
        duration_ms: u32,
    },
    /// Toggle fade to black.
    FadeToBlack { duration_ms: u32 },
}

/// Show plus runtime state that is not saved.
pub struct Switcher {
    show: Show,
    slots: usize,
    path: Option<String>,
    fade_to_black: bool,
    status: HashMap<InputId, InputStatus>,
}

impl Switcher {
    pub fn new(slots: usize) -> Self {
        Self {
            show: Show::default(),
            slots,
            path: None,
            fade_to_black: false,
            status: HashMap::new(),
        }
    }

    pub fn show(&self) -> &Show {
        &self.show
    }

    pub fn path(&self) -> Option<&str> {
        self.path.as_deref()
    }

    pub fn set_path(&mut self, path: String) {
        self.path = Some(path);
    }

    /// Engine reports an input's health.
    pub fn set_status(&mut self, id: InputId, status: InputStatus) {
        self.status.insert(id, status);
    }

    /// Apply one action. File actions (new/open/save) are handled by the
    /// service, which needs I/O; it calls [`Self::replace`] for them.
    pub fn apply(&mut self, action: Action) -> Result<Vec<Command>, ActionError> {
        match action {
            Action::AddInput { source, name } => self.add(source, name),
            Action::RemoveInput { input } => self.remove(&input),
            Action::ReplaceSource { input, source } => self.replace_source(&input, source),
            Action::RenameInput { input, name } => self.rename(&input, name),
            Action::MoveInput { input, number } => self.move_input(&input, number),
            Action::PreviewInput { input } => self.preview(&input),
            Action::Cut => self.take(Effect::Cut, 0),
            Action::CutDirect { input } => self.cut_direct(&input),
            Action::Transition { slot } => self.transition(slot),
            Action::SetTransition {
                slot,
                effect,
                duration_ms,
            } => self.set_transition(slot, effect, duration_ms),
            Action::FadeToBlack { duration_ms } => Ok(self.toggle_ftb(duration_ms)),
            Action::NewShow | Action::SaveShow { .. } | Action::OpenShow { .. } => Ok(Vec::new()),
        }
    }

    /// Swap in a whole show (new/open): empty all old slots, feed the new.
    pub fn replace(&mut self, show: Show, path: Option<String>) -> Vec<Command> {
        let mut commands: Vec<Command> = self
            .show
            .inputs
            .iter()
            .map(|i| Command::SetSlot {
                slot: i.slot,
                source: None,
            })
            .collect();

        self.show = show;
        self.path = path;
        self.status.clear();
        commands.extend(self.restore_commands());
        commands
    }

    /// Commands that rebuild the engine from the current show (start-up,
    /// engine restart).
    pub fn restore_commands(&self) -> Vec<Command> {
        let mut commands: Vec<Command> = self
            .show
            .inputs
            .iter()
            .map(|i| Command::SetSlot {
                slot: i.slot,
                source: Some(i.source.clone()),
            })
            .collect();

        if let Some(slot) = self.slot_of(self.show.program) {
            commands.push(Command::Take {
                from: slot,
                to: slot,
                effect: Effect::Cut,
                duration_ms: 0,
            });
        }
        if let Some(slot) = self.slot_of(self.show.preview) {
            commands.push(Command::Preview { slot });
        }
        commands
    }

    pub fn state(&self) -> ShowState {
        let inputs = self
            .show
            .inputs
            .iter()
            .enumerate()
            .map(|(i, input)| Input {
                id: input.id,
                number: i as u32 + 1,
                name: input.name.clone(),
                source: input.source.clone(),
                slot: input.slot,
                status: self.status.get(&input.id).copied().unwrap_or_default(),
            })
            .collect();

        ShowState {
            api_version: API_VERSION,
            path: self.path.clone(),
            inputs,
            preview: self.show.preview,
            program: self.show.program,
            transitions: self.show.transitions.clone(),
            fade_to_black: self.fade_to_black,
            slots: self.slots,
        }
    }

    /// Tally per input: 0 off, 1 program, 2 preview (API.5).
    pub fn tally(&self) -> Vec<u8> {
        self.show
            .inputs
            .iter()
            .map(|i| match Some(i.id) {
                id if id == self.show.program => 1,
                id if id == self.show.preview => 2,
                _ => 0,
            })
            .collect()
    }

    // ---- Inputs ----

    fn add(&mut self, source: SourceSpec, name: Option<String>) -> Result<Vec<Command>, ActionError> {
        let slot = self.free_slot().ok_or(ActionError::SlotsFull { slots: self.slots })?;
        let id = InputId::new();
        let name = name.unwrap_or_else(|| source.default_name());

        self.show.inputs.push(ShowInput {
            id,
            name,
            source: source.clone(),
            slot,
        });
        self.status.insert(id, InputStatus::Starting);

        Ok(vec![Command::SetSlot {
            slot,
            source: Some(source),
        }])
    }

    fn remove(&mut self, input: &InputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let removed = self.show.inputs.remove(index);

        // A removed input leaves preview/program empty, never pointing nowhere.
        if self.show.preview == Some(removed.id) {
            self.show.preview = None;
        }
        if self.show.program == Some(removed.id) {
            self.show.program = None;
        }
        self.status.remove(&removed.id);

        Ok(vec![Command::SetSlot {
            slot: removed.slot,
            source: None,
        }])
    }

    fn replace_source(
        &mut self,
        input: &InputRef,
        source: SourceSpec,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        entry.source = source.clone();
        self.status.insert(entry.id, InputStatus::Starting);

        Ok(vec![Command::SetSlot {
            slot: entry.slot,
            source: Some(source),
        }])
    }

    fn rename(&mut self, input: &InputRef, name: String) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        self.show.inputs[index].name = name;
        Ok(Vec::new())
    }

    fn move_input(&mut self, input: &InputRef, number: u32) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let max = self.show.inputs.len() as u32;
        if number == 0 || number > max {
            return Err(ActionError::BadPosition { number, max });
        }

        let entry = self.show.inputs.remove(index);
        self.show.inputs.insert(number as usize - 1, entry);
        Ok(Vec::new())
    }

    // ---- Switching ----

    fn preview(&mut self, input: &InputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &self.show.inputs[index];
        self.show.preview = Some(entry.id);
        Ok(vec![Command::Preview { slot: entry.slot }])
    }

    /// Preview to program; the old program goes to preview (classic swap).
    fn take(&mut self, effect: Effect, duration_ms: u32) -> Result<Vec<Command>, ActionError> {
        let to = self.slot_of(self.show.preview).ok_or(ActionError::NoPreview)?;
        let from = self.slot_of(self.show.program).unwrap_or(to);

        std::mem::swap(&mut self.show.preview, &mut self.show.program);

        Ok(vec![Command::Take {
            from,
            to,
            effect,
            duration_ms,
        }])
    }

    fn cut_direct(&mut self, input: &InputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &self.show.inputs[index];
        let to = entry.slot;
        let from = self.slot_of(self.show.program).unwrap_or(to);
        self.show.program = Some(entry.id);

        Ok(vec![Command::Take {
            from,
            to,
            effect: Effect::Cut,
            duration_ms: 0,
        }])
    }

    fn transition(&mut self, slot: TransitionSlot) -> Result<Vec<Command>, ActionError> {
        let button = *self.button(slot)?;
        self.take(button.effect, button.duration_ms)
    }

    fn set_transition(
        &mut self,
        slot: TransitionSlot,
        effect: Effect,
        duration_ms: u32,
    ) -> Result<Vec<Command>, ActionError> {
        self.button(slot)?;
        let button = &mut self.show.transitions[slot.0 as usize - 1];
        button.effect = effect;
        button.duration_ms = duration_ms;
        Ok(Vec::new())
    }

    fn toggle_ftb(&mut self, duration_ms: u32) -> Vec<Command> {
        self.fade_to_black = !self.fade_to_black;
        vec![Command::FadeToBlack { duration_ms }]
    }

    // ---- Lookup ----

    fn button(&self, slot: TransitionSlot) -> Result<&Transition, ActionError> {
        let bad = ActionError::BadTransitionSlot {
            slot: slot.0,
            max: TRANSITION_SLOTS,
        };
        if slot.0 == 0 {
            return Err(bad);
        }
        self.show.transitions.get(slot.0 as usize - 1).ok_or(bad)
    }

    fn free_slot(&self) -> Option<usize> {
        (0..self.slots).find(|s| !self.show.inputs.iter().any(|i| i.slot == *s))
    }

    fn slot_of(&self, id: Option<InputId>) -> Option<usize> {
        let id = id?;
        self.show.inputs.iter().find(|i| i.id == id).map(|i| i.slot)
    }

    /// Resolve an [`InputRef`] to an index in the input bar.
    fn find(&self, input: &InputRef) -> Result<usize, ActionError> {
        let unknown = || ActionError::UnknownInput {
            reference: match input {
                InputRef::Number(n) => n.to_string(),
                InputRef::Text(t) => format!("\"{t}\""),
            },
        };

        let index = match input {
            InputRef::Number(n) => (*n as usize).checked_sub(1).filter(|i| *i < self.show.inputs.len()),
            InputRef::Text(text) => self.find_text(text),
        };
        index.ok_or_else(unknown)
    }

    fn find_text(&self, text: &str) -> Option<usize> {
        let by_id = |id: Option<InputId>| self.show.inputs.iter().position(|i| Some(i.id) == id);

        if text == REF_PREVIEW {
            return by_id(self.show.preview);
        }
        if text == REF_PROGRAM {
            return by_id(self.show.program);
        }
        if let Ok(uuid) = Uuid::parse_str(text) {
            return by_id(Some(InputId(uuid)));
        }
        self.show.inputs.iter().position(|i| i.name == text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::TestPattern;

    const SLOTS: usize = 4;

    fn pattern(p: TestPattern) -> SourceSpec {
        SourceSpec::Pattern { pattern: p }
    }

    fn add(sw: &mut Switcher, name: &str) {
        sw.apply(Action::AddInput {
            source: pattern(TestPattern::Bars),
            name: Some(name.into()),
        })
        .unwrap();
    }

    fn named(name: &str) -> InputRef {
        InputRef::Text(name.into())
    }

    fn two_inputs() -> Switcher {
        let mut sw = Switcher::new(SLOTS);
        add(&mut sw, "A");
        add(&mut sw, "B");
        sw
    }

    #[test]
    fn add_takes_lowest_free_slot() {
        let mut sw = two_inputs();
        sw.apply(Action::RemoveInput { input: named("A") }).unwrap();

        let cmds = sw
            .apply(Action::AddInput {
                source: pattern(TestPattern::Ball),
                name: None,
            })
            .unwrap();

        assert_eq!(
            cmds,
            vec![Command::SetSlot {
                slot: 0,
                source: Some(pattern(TestPattern::Ball))
            }]
        );
    }

    #[test]
    fn add_fails_when_slots_full() {
        let mut sw = Switcher::new(1);
        add(&mut sw, "A");

        let err = sw
            .apply(Action::AddInput {
                source: pattern(TestPattern::Bars),
                name: None,
            })
            .unwrap_err();

        assert_eq!(err, ActionError::SlotsFull { slots: 1 });
        assert_eq!(sw.state().inputs.len(), 1);
    }

    #[test]
    fn refs_resolve_by_number_name_id_and_role() {
        let mut sw = two_inputs();
        let b = sw.state().inputs[1].id;
        sw.apply(Action::PreviewInput { input: InputRef::Number(2) }).unwrap();

        assert_eq!(sw.find(&named("B")), Ok(1));
        assert_eq!(sw.find(&InputRef::Text(b.to_string())), Ok(1));
        assert_eq!(sw.find(&named(REF_PREVIEW)), Ok(1));
        assert!(sw.find(&named(REF_PROGRAM)).is_err());
        assert!(sw.find(&InputRef::Number(0)).is_err());
        assert!(sw.find(&InputRef::Number(3)).is_err());
    }

    #[test]
    fn cut_swaps_preview_and_program() {
        let mut sw = two_inputs();
        let (a, b) = (sw.state().inputs[0].id, sw.state().inputs[1].id);
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") }).unwrap();

        let cmds = sw.apply(Action::Cut).unwrap();

        assert_eq!(
            cmds,
            vec![Command::Take {
                from: 0,
                to: 1,
                effect: Effect::Cut,
                duration_ms: 0
            }]
        );
        assert_eq!(sw.state().program, Some(b));
        assert_eq!(sw.state().preview, Some(a));
        assert_eq!(sw.tally(), vec![2, 1]);
    }

    #[test]
    fn transition_uses_button_settings() {
        let mut sw = two_inputs();
        sw.apply(Action::PreviewInput { input: named("B") }).unwrap();
        sw.apply(Action::SetTransition {
            slot: TransitionSlot(3),
            effect: Effect::SlideLeft,
            duration_ms: 750,
        })
        .unwrap();

        let cmds = sw.apply(Action::Transition { slot: TransitionSlot(3) }).unwrap();

        assert_eq!(
            cmds,
            vec![Command::Take {
                from: 1,
                to: 1,
                effect: Effect::SlideLeft,
                duration_ms: 750
            }]
        );
    }

    #[test]
    fn take_without_preview_fails() {
        let mut sw = two_inputs();
        assert_eq!(sw.apply(Action::Cut), Err(ActionError::NoPreview));
    }

    #[test]
    fn bad_transition_slot_fails() {
        let mut sw = two_inputs();
        for slot in [0, TRANSITION_SLOTS + 1] {
            let err = sw.apply(Action::Transition { slot: TransitionSlot(slot) }).unwrap_err();
            assert!(matches!(err, ActionError::BadTransitionSlot { .. }));
        }
    }

    #[test]
    fn removing_program_input_clears_program() {
        let mut sw = two_inputs();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();

        sw.apply(Action::RemoveInput { input: named("A") }).unwrap();

        assert_eq!(sw.state().program, None);
        assert_eq!(sw.state().inputs[0].number, 1);
    }

    #[test]
    fn move_reorders_numbers() {
        let mut sw = two_inputs();

        sw.apply(Action::MoveInput {
            input: named("B"),
            number: 1,
        })
        .unwrap();

        let names: Vec<_> = sw.state().inputs.iter().map(|i| i.name.clone()).collect();
        assert_eq!(names, ["B", "A"]);
        assert!(sw.apply(Action::MoveInput { input: named("A"), number: 3 }).is_err());
    }

    #[test]
    fn replace_rebuilds_slots() {
        let mut sw = two_inputs();
        sw.apply(Action::PreviewInput { input: named("B") }).unwrap();
        let show = sw.show().clone();
        let mut fresh = Switcher::new(SLOTS);

        let cmds = fresh.replace(show, Some("x.lmix".into()));

        assert_eq!(cmds.len(), 3, "{cmds:?}");
        assert_eq!(cmds[2], Command::Preview { slot: 1 });
        assert_eq!(fresh.state().path.as_deref(), Some("x.lmix"));
    }

    #[test]
    fn failed_action_changes_nothing() {
        let mut sw = two_inputs();
        let before = sw.state();

        let _ = sw.apply(Action::RenameInput {
            input: named("missing"),
            name: "X".into(),
        });

        assert_eq!(sw.state(), before);
    }
}
