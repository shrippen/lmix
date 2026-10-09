//! Show state machine: applies actions, decides what the engine must do.
//!
//! Pure: no I/O, no async. Every action either fails with a reason and
//! changes nothing, or changes the show and returns engine [`Command`]s.
//!
//! ```text
//!   Action ──► Switcher::apply ──► new state + [Command] ──► engine driver
//! ```

use lmix_types::action::{aux_index, OVERLAY_CHANNELS, TRANSITION_SLOTS};
use lmix_types::input::{color_rgb, AUX_BUSES, REF_PREVIEW, REF_PROGRAM, VOLUME_MAX};
use lmix_types::state::API_VERSION;
use lmix_types::{
    Action, ActionError, AuxBus, ChannelStrip, Effect, Ingest, Input, InputColor, InputId,
    InputRef, InputStatus, Listen, Look, Output, OutputId, OutputRef, OutputSource, OutputSpec,
    OutputStatus, ShowState, SignalLoss, SourceSpec, Stinger, StingerSpec, Transition,
    TransitionSlot, Transport,
};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use uuid::Uuid;

use crate::show::{Show, ShowInput, ShowOutput};

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
    /// Bring `slot` on program; `None` is black. `Effect::Cut` for a hard cut.
    Program {
        slot: Option<usize>,
        effect: Effect,
        duration_ms: u32,
    },
    /// Toggle fade to black.
    FadeToBlack { duration_ms: u32 },
    /// Solo `slot` on the monitor bus, or end its solo.
    SetListen { slot: usize, listen: Listen },
    /// Last frame (`Hold`) or black when the source of `slot` stalls.
    SetSignalLoss { slot: usize, mode: SignalLoss },
    /// Start output `id` (replaces a running one); its sound from aux
    /// bus `aux` (0-based) instead of the feed's.
    StartOutput {
        id: OutputId,
        spec: OutputSpec,
        feed: Feed,
        aux: Option<usize>,
    },
    /// Stop output `id`; stopping a stopped one is fine.
    StopOutput { id: OutputId },
    /// Channel strip of the audio mixer channel of `slot`.
    SetStrip { slot: usize, strip: ChannelStrip },
    /// Colour correction of the picture of `slot`.
    SetLook { slot: usize, look: Look },
    /// Stinger clip of `slot` on the stinger layer from its start; off:
    /// layer away and the clip paused.
    Stinger { slot: usize, on: bool },
    /// Hold the commands after it back by `ms`; the service runs them
    /// later, without blocking other actions.
    Wait { ms: u32 },
    /// Send of `slot` to aux bus `bus` (0-based), linear gain.
    SetAuxSend { slot: usize, bus: usize, gain: f64 },
    /// Master of aux bus `bus` (0-based).
    SetAuxBus { bus: usize, gain: f64, mute: bool },
    /// Mix `slot` (preview) over program by `position` (0–1), as the
    /// T-bar stands; 0 hides it again.
    TBar { slot: usize, position: f32 },
    /// Show `slot` on overlay `channel` (0-based) or take it off, faded
    /// over `duration_ms`.
    Overlay {
        channel: usize,
        slot: Option<usize>,
        duration_ms: u32,
    },
    /// Play, pause, restart or step the media player of `slot`.
    Transport { slot: usize, transport: Transport },
    /// Set the audio channel of `slot`: linear gain (1.0 = unity) and
    /// mute, faded over `ramp_ms`.
    SetAudio {
        slot: usize,
        gain: f64,
        muted: bool,
        ramp_ms: u32,
    },
}

/// What an output flow subscribes to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Feed {
    Program,
    /// One input's slot; `audio` when its source has sound.
    Slot {
        slot: usize,
        audio: bool,
    },
}

/// Anti-click fade for audio changes outside a transition.
pub const AUDIO_RAMP_MS: u32 = 20;

/// What one audio channel is set to.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Mix {
    gain: f64,
    muted: bool,
}

/// Fader gain of a volume: squared, so the scale feels even to the ear
/// (50 ≈ -12 dB, 100 = unity).
pub fn fader_gain(volume: u8) -> f64 {
    let v = f64::from(volume) / f64::from(VOLUME_MAX);
    v * v
}

/// Show plus runtime state that is not saved.
pub struct Switcher {
    show: Show,
    slots: usize,
    path: Option<String>,
    fade_to_black: bool,
    /// T-bar position while it is pulled; runtime only.
    t_bar: f32,
    /// When a running stinger ends; switching waits for it.
    stinger_until: Option<Instant>,
    /// The engine can apply looks (GL mixer).
    looks_available: bool,
    status: HashMap<InputId, InputStatus>,
    /// Audio channel settings the engine has, per slot.
    mix: HashMap<usize, Mix>,
    /// Solos; runtime only, like a desk's solo buttons.
    listen: HashMap<InputId, Listen>,
    /// Engine address as senders reach it, for WHIP ingest links.
    ingest_base: Option<String>,
    /// Output states; runtime only.
    output_status: HashMap<OutputId, OutputStatus>,
}

impl Switcher {
    pub fn new(slots: usize) -> Self {
        Self {
            show: Show::default(),
            slots,
            path: None,
            fade_to_black: false,
            t_bar: 0.0,
            stinger_until: None,
            looks_available: false,
            status: HashMap::new(),
            mix: HashMap::new(),
            listen: HashMap::new(),
            ingest_base: None,
            output_status: HashMap::new(),
        }
    }

    pub fn slots(&self) -> usize {
        self.slots
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

    pub fn set_ingest_base(&mut self, url: String) {
        self.ingest_base = Some(url.trim_end_matches('/').to_string());
    }

    /// Ingest links of a WHIP source.
    fn ingest_of(&self, source: &SourceSpec) -> Option<Ingest> {
        let (SourceSpec::Whip { endpoint }, Some(base)) = (source, &self.ingest_base) else {
            return None;
        };
        Some(Ingest {
            whip_url: format!("{base}/whip/{endpoint}"),
            browser_url: format!("{base}/player/whip-ingest"),
        })
    }

    /// Engine reports an input's health.
    pub fn set_status(&mut self, id: InputId, status: InputStatus) {
        self.status.insert(id, status);
    }

    /// Apply one action. File actions (new/open/save) are handled by the
    /// service, which needs I/O; it calls [`Self::replace`] for them.
    pub fn apply(&mut self, action: Action) -> Result<Vec<Command>, ActionError> {
        let ramp_ms = self.audio_ramp(&action);
        let mut commands = self.apply_action(action)?;
        let audio = self.audio_commands(ramp_ms);
        // A stinger switches sound at its cut, not once the clip ends.
        match commands
            .iter()
            .rposition(|c| matches!(c, Command::Wait { .. }))
        {
            Some(at) => {
                commands.splice(at..at, audio);
            }
            None => commands.extend(audio),
        }
        Ok(commands)
    }

    fn apply_action(&mut self, action: Action) -> Result<Vec<Command>, ActionError> {
        match action {
            Action::AddInput { source, name } => self.add(source, name),
            Action::RemoveInput { input } => self.remove(&input),
            Action::ReplaceSource { input, source } => self.replace_source(&input, source),
            Action::RenameInput { input, name } => self.rename(&input, name),
            Action::SetInputColor { input, color } => self.set_color(&input, color),
            Action::SetAudio { input, on } => self.set_audio(&input, |a| a.on = on),
            Action::SetAudioFollow { input, follow } => {
                self.set_audio(&input, |a| a.follow_video = follow)
            }
            Action::SetVolume { input, volume } => self.set_volume(&input, volume),
            Action::SetChannelStrip { input, strip } => self.set_strip(&input, strip),
            Action::SetLook { input, look } => self.set_look(&input, look),
            Action::SetAuxSend { input, bus, level } => self.set_aux_send(&input, bus, level),
            Action::SetAuxBus { bus, volume, mute } => self.set_aux_bus(bus, volume, mute),
            Action::SetListen { input, listen } => self.set_listen(&input, listen),
            Action::TransportInput { input, transport } => self.transport(&input, transport),
            Action::SetSignalLoss { input, mode } => self.set_signal_loss(&input, mode),
            Action::AddOutput {
                spec,
                source,
                name,
                aux,
            } => self.add_output(spec, source, name, aux),
            Action::RemoveOutput { output } => self.remove_output(&output),
            Action::StartOutput { output } => self.start_output(&output),
            Action::StopOutput { output } => self.stop_output(&output),
            Action::ClearListen => Ok(self.clear_listen()),
            Action::MoveInput { input, number } => self.move_input(&input, number),
            Action::PreviewInput { input } => self.preview(&input),
            Action::Cut => self.take(Effect::Cut, 0),
            Action::CutDirect { input } => self.cut_direct(&input),
            Action::Transition { slot } => self.transition(slot),
            Action::SetTransition {
                slot,
                effect,
                duration_ms,
                stinger,
            } => self.set_transition(slot, effect, duration_ms, stinger),
            Action::FadeToBlack { duration_ms } => Ok(self.toggle_ftb(duration_ms)),
            Action::TBar { position } => self.t_bar(position),
            Action::OverlayIn {
                channel,
                input,
                duration_ms,
            } => self.overlay_in(channel, &input, duration_ms),
            Action::OverlayOut {
                channel,
                duration_ms,
            } => self.overlay_out(channel, duration_ms),
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

        commands.extend(self.clear_listen());
        commands.extend(self.stop_all_outputs());
        self.show = show;
        self.path = path;
        self.status.clear();
        commands.extend(self.restore_commands());
        commands
    }

    /// Commands that rebuild the engine from the current show (start-up,
    /// engine restart): slots, program, preview, then every audio channel.
    pub fn restore_commands(&mut self) -> Vec<Command> {
        let mut commands = self.picture_commands();
        self.mix.clear();
        commands.extend(self.audio_commands(AUDIO_RAMP_MS));
        // A reused mixer may still hold solos from before.
        commands.extend(self.listen_commands());
        commands.extend(self.running_output_commands());
        commands.extend(self.show.inputs.iter().map(|i| Command::SetSignalLoss {
            slot: i.slot,
            mode: i.signal_loss,
        }));
        commands.extend(self.overlay_commands());
        commands.extend(self.stinger_pause_commands());
        commands.extend(self.look_commands());
        for input in self.show.inputs.iter().filter(|i| i.source.has_audio()) {
            commands.push(Command::SetStrip {
                slot: input.slot,
                strip: input.strip,
            });
            commands.extend(send_commands(input));
        }
        commands.extend(self.aux_bus_commands());
        commands
    }

    fn aux_bus_commands(&self) -> Vec<Command> {
        self.show
            .aux_buses
            .iter()
            .enumerate()
            .map(|(bus, master)| Command::SetAuxBus {
                bus,
                gain: fader_gain(master.volume),
                mute: master.mute,
            })
            .collect()
    }

    /// Every overlay channel as the engine should have it, without fade.
    fn overlay_commands(&self) -> Vec<Command> {
        self.show
            .overlays
            .iter()
            .enumerate()
            .map(|(channel, input)| Command::Overlay {
                channel,
                slot: self.slot_of(*input),
                duration_ms: 0,
            })
            .collect()
    }

    /// Solo state of every input with sound, as the engine should have it.
    fn listen_commands(&self) -> Vec<Command> {
        self.show
            .inputs
            .iter()
            .filter(|i| i.source.has_audio())
            .map(|i| Command::SetListen {
                slot: i.slot,
                listen: self.listen_of(i.id),
            })
            .collect()
    }

    fn listen_of(&self, id: InputId) -> Listen {
        self.listen.get(&id).copied().unwrap_or_default()
    }

    fn picture_commands(&self) -> Vec<Command> {
        let mut commands: Vec<Command> = self
            .show
            .inputs
            .iter()
            .map(|i| Command::SetSlot {
                slot: i.slot,
                source: Some(i.source.clone()),
            })
            .collect();

        commands.push(Command::Program {
            slot: self.slot_of(self.show.program),
            effect: Effect::Cut,
            duration_ms: 0,
        });
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
                color: input.color,
                audio: input.audio,
                has_audio: input.source.has_audio(),
                listen: self.listen_of(input.id),
                signal_loss: input.signal_loss,
                strip: input.strip,
                sends: input.sends,
                look: input.look,
                ingest: self.ingest_of(&input.source),
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
            outputs: self.outputs_state(),
            overlays: self.show.overlays,
            t_bar: self.t_bar,
            aux_buses: self.show.aux_buses,
            looks_available: self.looks_available,
        }
    }

    /// Tally per input: 0 off, 1 program, 2 preview (API.5).
    pub fn tally(&self) -> Vec<u8> {
        self.show
            .inputs
            .iter()
            .map(|i| match Some(i.id) {
                // An overlay is on air like program.
                id if id == self.show.program || self.show.overlays.contains(&id) => 1,
                id if id == self.show.preview => 2,
                _ => 0,
            })
            .collect()
    }

    // ---- Inputs ----

    fn add(
        &mut self,
        source: SourceSpec,
        name: Option<String>,
    ) -> Result<Vec<Command>, ActionError> {
        check_source(&source)?;
        let slot = self
            .free_slot()
            .ok_or(ActionError::SlotsFull { slots: self.slots })?;
        let id = InputId::new();
        let name = name.unwrap_or_else(|| source.default_name());

        self.show.inputs.push(ShowInput {
            id,
            name,
            source: source.clone(),
            slot,
            color: None,
            audio: Default::default(),
            signal_loss: SignalLoss::default(),
            strip: ChannelStrip::default(),
            sends: [0; AUX_BUSES],
            look: Look::default(),
        });
        self.status.insert(id, InputStatus::Starting);

        // The mixer input may still carry the previous occupant's setting.
        let mut commands = vec![
            Command::SetSlot {
                slot,
                source: Some(source.clone()),
            },
            Command::SetSignalLoss {
                slot,
                mode: SignalLoss::default(),
            },
        ];
        if self.looks_available && source.has_video() {
            commands.push(Command::SetLook {
                slot,
                look: Look::default(),
            });
        }
        if source.has_audio() {
            commands.push(Command::SetStrip {
                slot,
                strip: ChannelStrip::default(),
            });
            commands.extend((0..AUX_BUSES).map(|bus| Command::SetAuxSend {
                slot,
                bus,
                gain: 0.0,
            }));
        }
        Ok(commands)
    }

    fn remove(&mut self, input: &InputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let removed = self.show.inputs.remove(index);
        let mut commands = Vec::new();

        // A removed input leaves preview empty and program black.
        if self.show.preview == Some(removed.id) {
            self.show.preview = None;
        }
        if self.show.program == Some(removed.id) {
            self.show.program = None;
            commands.push(Command::Program {
                slot: None,
                effect: Effect::Cut,
                duration_ms: 0,
            });
        }
        self.status.remove(&removed.id);
        // A button whose stinger clip goes becomes a cut.
        for button in &mut self.show.transitions {
            if button.stinger.is_some_and(|s| s.input == removed.id) {
                button.effect = Effect::Cut;
                button.stinger = None;
            }
        }
        for (channel, shown) in self.show.overlays.iter_mut().enumerate() {
            if *shown == Some(removed.id) {
                *shown = None;
                commands.push(Command::Overlay {
                    channel,
                    slot: None,
                    duration_ms: 0,
                });
            }
        }
        // ISO outputs of the input end with it.
        for output in self
            .show
            .outputs
            .iter()
            .filter(|o| o.source == OutputSource::Input(removed.id))
        {
            if self
                .output_status
                .get(&output.id)
                .is_some_and(|s| matches!(s, OutputStatus::Running { .. }))
            {
                commands.push(Command::StopOutput { id: output.id });
            }
        }
        let fed: Vec<OutputId> = self
            .show
            .outputs
            .iter()
            .filter(|o| o.source == OutputSource::Input(removed.id))
            .map(|o| o.id)
            .collect();
        for id in fed {
            self.output_status.insert(
                id,
                OutputStatus::Failed {
                    message: format!("input {} was removed", removed.name),
                },
            );
        }
        // A solo left on an empty channel would keep the monitor on silence.
        if self
            .listen
            .remove(&removed.id)
            .is_some_and(|l| l != Listen::Off)
        {
            commands.push(Command::SetListen {
                slot: removed.slot,
                listen: Listen::Off,
            });
        }

        commands.push(Command::SetSlot {
            slot: removed.slot,
            source: None,
        });
        Ok(commands)
    }

    fn replace_source(
        &mut self,
        input: &InputRef,
        source: SourceSpec,
    ) -> Result<Vec<Command>, ActionError> {
        check_source(&source)?;
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        if !source.has_video() && [self.show.preview, self.show.program].contains(&Some(entry.id)) {
            return Err(ActionError::NoVideo {
                reference: entry.name.clone(),
            });
        }
        entry.source = source.clone();
        self.status.insert(entry.id, InputStatus::Starting);

        let mut commands = vec![Command::SetSlot {
            slot: entry.slot,
            source: Some(source.clone()),
        }];
        // After a source without sound the channel may hold anything.
        if source.has_audio() {
            commands.push(Command::SetStrip {
                slot: entry.slot,
                strip: entry.strip,
            });
            commands.extend(send_commands(entry));
        }
        Ok(commands)
    }

    fn rename(&mut self, input: &InputRef, name: String) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        self.show.inputs[index].name = name;
        Ok(Vec::new())
    }

    fn set_color(
        &mut self,
        input: &InputRef,
        color: Option<InputColor>,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        self.show.inputs[index].color = color;
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
        no_video_check(entry)?;
        if self.show.program == Some(entry.id) {
            return Err(ActionError::OnProgram {
                reference: entry.name.clone(),
            });
        }

        self.show.preview = Some(entry.id);
        Ok(vec![Command::Preview { slot: entry.slot }])
    }

    /// Preview to program; the old program goes to preview (classic swap).
    fn take(&mut self, effect: Effect, duration_ms: u32) -> Result<Vec<Command>, ActionError> {
        self.check_no_stinger()?;
        let to = self
            .slot_of(self.show.preview)
            .ok_or(ActionError::NoPreview)?;
        let old_program = self.slot_of(self.show.program);

        std::mem::swap(&mut self.show.preview, &mut self.show.program);
        Ok(program_then_preview(
            Some(to),
            old_program,
            effect,
            duration_ms,
        ))
    }

    /// Input straight to program. If it was in preview, the old program
    /// takes its place there.
    fn cut_direct(&mut self, input: &InputRef) -> Result<Vec<Command>, ActionError> {
        self.check_no_stinger()?;
        let index = self.find(input)?;
        let entry = &self.show.inputs[index];
        no_video_check(entry)?;
        let (id, to) = (entry.id, entry.slot);

        if self.show.preview != Some(id) {
            self.show.program = Some(id);
            return Ok(program_then_preview(Some(to), None, Effect::Cut, 0));
        }

        let old_program = self.slot_of(self.show.program);
        self.show.preview = self.show.program;
        self.show.program = Some(id);
        Ok(program_then_preview(Some(to), old_program, Effect::Cut, 0))
    }

    /// T-bar (SWI.3): preview fades in over program as it is pulled; at
    /// the end it is a take, back at the start nothing happened.
    fn t_bar(&mut self, position: f32) -> Result<Vec<Command>, ActionError> {
        if !(0.0..=1.0).contains(&position) {
            return Err(ActionError::BadTBar);
        }
        self.check_no_stinger()?;
        let slot = self
            .slot_of(self.show.preview)
            .ok_or(ActionError::NoPreview)?;
        if position >= 1.0 {
            self.t_bar = 0.0;
            return self.take(Effect::Cut, 0);
        }
        self.t_bar = position;
        Ok(vec![Command::TBar { slot, position }])
    }

    fn transition(&mut self, slot: TransitionSlot) -> Result<Vec<Command>, ActionError> {
        let button = *self.button(slot)?;
        if button.effect != Effect::Stinger {
            return self.take(button.effect, button.duration_ms);
        }
        let bad = |reason: &str| ActionError::BadStinger {
            reason: reason.into(),
        };
        let stinger = button.stinger.ok_or_else(|| bad("no clip set"))?;
        let clip = self
            .slot_of(Some(stinger.input))
            .ok_or_else(|| bad("its clip input is gone"))?;

        // Clip on, cut underneath at the cut point, clip off at the end.
        let cut = self.take(Effect::Cut, 0)?;
        let mut commands = vec![
            Command::Stinger {
                slot: clip,
                on: true,
            },
            Command::Wait { ms: stinger.cut_ms },
        ];
        commands.extend(cut);
        commands.push(Command::Wait {
            ms: button.duration_ms.saturating_sub(stinger.cut_ms),
        });
        commands.push(Command::Stinger {
            slot: clip,
            on: false,
        });
        self.stinger_until =
            Some(Instant::now() + Duration::from_millis(u64::from(button.duration_ms)));
        Ok(commands)
    }

    fn check_no_stinger(&self) -> Result<(), ActionError> {
        match self.stinger_until {
            Some(until) if Instant::now() < until => Err(ActionError::StingerPlaying),
            _ => Ok(()),
        }
    }

    fn set_transition(
        &mut self,
        slot: TransitionSlot,
        effect: Effect,
        duration_ms: u32,
        stinger: Option<StingerSpec>,
    ) -> Result<Vec<Command>, ActionError> {
        self.button(slot)?;
        let mut commands = Vec::new();
        let stinger = match (effect, stinger) {
            (Effect::Stinger, Some(spec)) => {
                let (stinger, clip) = self.stinger_of(&spec, duration_ms)?;
                // Paused until the button is pressed; it would loop unseen.
                commands.push(Command::Transport {
                    slot: clip,
                    transport: Transport::Pause,
                });
                Some(stinger)
            }
            (Effect::Stinger, None) => {
                return Err(ActionError::BadStinger {
                    reason: "choose a clip input".into(),
                })
            }
            _ => None,
        };
        let button = &mut self.show.transitions[slot.0 as usize - 1];
        button.effect = effect;
        button.duration_ms = duration_ms;
        button.stinger = stinger;
        Ok(commands)
    }

    /// A stinger clip must be a file, URL or playlist with a picture; the
    /// cut point lies within the button's duration.
    fn stinger_of(
        &self,
        spec: &StingerSpec,
        duration_ms: u32,
    ) -> Result<(Stinger, usize), ActionError> {
        let entry = &self.show.inputs[self.find(&spec.input)?];
        no_video_check(entry)?;
        if !entry.source.has_transport() {
            return Err(ActionError::BadStinger {
                reason: format!("{} is not a clip (file, URL or playlist)", entry.name),
            });
        }
        if spec.cut_ms > duration_ms {
            return Err(ActionError::BadStinger {
                reason: "the cut point lies after the end".into(),
            });
        }
        let stinger = Stinger {
            input: entry.id,
            cut_ms: spec.cut_ms,
        };
        Ok((stinger, entry.slot))
    }

    /// Clips of stinger buttons, paused (start-up, restore).
    fn stinger_pause_commands(&self) -> Vec<Command> {
        self.show
            .transitions
            .iter()
            .filter_map(|t| self.slot_of(Some(t.stinger?.input)))
            .map(|slot| Command::Transport {
                slot,
                transport: Transport::Pause,
            })
            .collect()
    }

    // ---- Audio ----

    fn set_audio(
        &mut self,
        input: &InputRef,
        change: impl FnOnce(&mut lmix_types::InputAudio),
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        change(&mut self.show.inputs[index].audio);
        Ok(Vec::new())
    }

    fn set_volume(&mut self, input: &InputRef, volume: u8) -> Result<Vec<Command>, ActionError> {
        check_volume(volume)?;
        self.set_audio(input, |a| a.volume = volume)
    }

    fn set_strip(
        &mut self,
        input: &InputRef,
        strip: ChannelStrip,
    ) -> Result<Vec<Command>, ActionError> {
        if let Some(field) = strip.out_of_range() {
            return Err(ActionError::BadStrip {
                field: field.into(),
            });
        }
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        if !entry.source.has_audio() {
            return Err(ActionError::NoAudio {
                reference: entry.name.clone(),
            });
        }
        entry.strip = strip;
        Ok(vec![Command::SetStrip {
            slot: entry.slot,
            strip,
        }])
    }

    fn set_aux_send(
        &mut self,
        input: &InputRef,
        bus: u8,
        level: u8,
    ) -> Result<Vec<Command>, ActionError> {
        let bus = aux_index(bus)?;
        check_volume(level)?;
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        if !entry.source.has_audio() {
            return Err(ActionError::NoAudio {
                reference: entry.name.clone(),
            });
        }
        entry.sends[bus] = level;
        Ok(vec![Command::SetAuxSend {
            slot: entry.slot,
            bus,
            gain: fader_gain(level),
        }])
    }

    fn set_aux_bus(
        &mut self,
        bus: u8,
        volume: u8,
        mute: bool,
    ) -> Result<Vec<Command>, ActionError> {
        let bus = aux_index(bus)?;
        check_volume(volume)?;
        self.show.aux_buses[bus] = AuxBus { volume, mute };
        Ok(vec![Command::SetAuxBus {
            bus,
            gain: fader_gain(volume),
            mute,
        }])
    }

    /// The engine says whether it can apply looks.
    pub fn set_looks_available(&mut self, available: bool) {
        self.looks_available = available;
    }

    fn set_look(&mut self, input: &InputRef, look: Look) -> Result<Vec<Command>, ActionError> {
        if let Some(field) = look.out_of_range() {
            return Err(ActionError::BadLook {
                field: field.into(),
            });
        }
        if !self.looks_available && !look.is_neutral() {
            return Err(ActionError::NoLooks);
        }
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        no_video_check(entry)?;
        entry.look = look;
        if !self.looks_available {
            return Ok(Vec::new());
        }
        Ok(vec![Command::SetLook {
            slot: entry.slot,
            look,
        }])
    }

    /// Every picture's look, for an engine that can apply them; a slot
    /// may still carry the look of a former input.
    fn look_commands(&self) -> Vec<Command> {
        if !self.looks_available {
            return Vec::new();
        }
        self.show
            .inputs
            .iter()
            .filter(|i| i.source.has_video())
            .map(|i| Command::SetLook {
                slot: i.slot,
                look: i.look,
            })
            .collect()
    }

    fn set_signal_loss(
        &mut self,
        input: &InputRef,
        mode: SignalLoss,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &mut self.show.inputs[index];
        entry.signal_loss = mode;
        Ok(vec![Command::SetSignalLoss {
            slot: entry.slot,
            mode,
        }])
    }

    // ---- Overlays ----

    fn overlay_in(
        &mut self,
        channel: u8,
        input: &InputRef,
        duration_ms: u32,
    ) -> Result<Vec<Command>, ActionError> {
        let index = overlay_index(channel)?;
        let entry = &self.show.inputs[self.find(input)?];
        no_video_check(entry)?;
        let (id, slot) = (entry.id, entry.slot);
        self.show.overlays[index] = Some(id);
        Ok(vec![Command::Overlay {
            channel: index,
            slot: Some(slot),
            duration_ms,
        }])
    }

    fn overlay_out(&mut self, channel: u8, duration_ms: u32) -> Result<Vec<Command>, ActionError> {
        let index = overlay_index(channel)?;
        if self.show.overlays[index].take().is_none() {
            return Ok(Vec::new());
        }
        Ok(vec![Command::Overlay {
            channel: index,
            slot: None,
            duration_ms,
        }])
    }

    // ---- Outputs ----

    fn add_output(
        &mut self,
        spec: OutputSpec,
        source: OutputSource,
        name: Option<String>,
        aux: Option<u8>,
    ) -> Result<Vec<Command>, ActionError> {
        if let Some(bus) = aux {
            aux_index(bus)?;
        }
        let output = ShowOutput {
            id: OutputId::new(),
            name: name.unwrap_or_default(),
            spec,
            source,
            aux,
        };
        self.feed_of(&output)?;
        let name = match (output.name.is_empty(), source) {
            (false, _) => output.name.clone(),
            (true, OutputSource::Program) => output.spec.default_name(),
            (true, OutputSource::Input(id)) => {
                let input = self
                    .show
                    .inputs
                    .iter()
                    .find(|i| i.id == id)
                    .map_or("", |i| &i.name);
                format!("{} ({input})", output.spec.default_name())
            }
        };
        self.show.outputs.push(ShowOutput { name, ..output });
        Ok(Vec::new())
    }

    /// Program, or the slot of the output's input; ISO outputs need an
    /// input with a picture.
    fn feed_of(&self, output: &ShowOutput) -> Result<Feed, ActionError> {
        let OutputSource::Input(id) = output.source else {
            return Ok(Feed::Program);
        };
        let input = self
            .show
            .inputs
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| ActionError::UnknownInput {
                reference: id.to_string(),
            })?;
        no_video_check(input)?;
        Ok(Feed::Slot {
            slot: input.slot,
            audio: input.source.has_audio(),
        })
    }

    fn remove_output(&mut self, output: &OutputRef) -> Result<Vec<Command>, ActionError> {
        let index = self.find_output(output)?;
        let removed = self.show.outputs.remove(index);
        self.output_status.remove(&removed.id);
        Ok(vec![Command::StopOutput { id: removed.id }])
    }

    /// Starting a running output restarts it (e.g. after a failure).
    fn start_output(&mut self, output: &OutputRef) -> Result<Vec<Command>, ActionError> {
        let entry = &self.show.outputs[self.find_output(output)?];
        Ok(vec![self.start_command(entry)?])
    }

    fn start_command(&self, output: &ShowOutput) -> Result<Command, ActionError> {
        Ok(Command::StartOutput {
            id: output.id,
            spec: output.spec.clone(),
            feed: self.feed_of(output)?,
            // Checked when the output was added.
            aux: output.aux.and_then(|bus| aux_index(bus).ok()),
        })
    }

    fn stop_output(&mut self, output: &OutputRef) -> Result<Vec<Command>, ActionError> {
        let id = self.show.outputs[self.find_output(output)?].id;
        Ok(vec![Command::StopOutput { id }])
    }

    fn stop_all_outputs(&mut self) -> Vec<Command> {
        let running: Vec<OutputId> = self
            .show
            .outputs
            .iter()
            .filter(|o| self.output_running(o.id))
            .map(|o| o.id)
            .collect();
        self.output_status.clear();
        running
            .into_iter()
            .map(|id| Command::StopOutput { id })
            .collect()
    }

    /// Outputs that were running before an engine restart, to start again.
    fn running_output_commands(&self) -> Vec<Command> {
        self.show
            .outputs
            .iter()
            .filter(|o| self.output_running(o.id))
            .filter_map(|o| self.start_command(o).ok())
            .collect()
    }

    fn output_running(&self, id: OutputId) -> bool {
        matches!(
            self.output_status.get(&id),
            Some(OutputStatus::Running { .. })
        )
    }

    pub fn output_is_running(&self, id: OutputId) -> bool {
        self.output_running(id)
    }

    /// Start command for a failed output; `None` when it was stopped or
    /// removed meanwhile, or its input is gone.
    pub fn restart_command(&self, id: OutputId) -> Option<Command> {
        let failed = matches!(
            self.output_status.get(&id),
            Some(OutputStatus::Failed { .. })
        );
        let output = self
            .show
            .outputs
            .iter()
            .find(|o| o.id == id)
            .filter(|_| failed)?;
        self.start_command(output).ok()
    }

    /// The engine reports what an output does.
    pub fn set_output_status(&mut self, id: OutputId, status: OutputStatus) {
        if self.show.outputs.iter().any(|o| o.id == id) {
            self.output_status.insert(id, status);
        }
    }

    pub fn output_ids(&self) -> Vec<OutputId> {
        self.show.outputs.iter().map(|o| o.id).collect()
    }

    fn outputs_state(&self) -> Vec<Output> {
        self.show
            .outputs
            .iter()
            .enumerate()
            .map(|(i, o)| Output {
                id: o.id,
                number: i as u32 + 1,
                name: o.name.clone(),
                spec: o.spec.clone(),
                source: o.source,
                aux: o.aux,
                status: self.output_status.get(&o.id).cloned().unwrap_or_default(),
            })
            .collect()
    }

    fn find_output(&self, output: &OutputRef) -> Result<usize, ActionError> {
        let unknown = || ActionError::UnknownOutput {
            reference: match output {
                OutputRef::Number(n) => n.to_string(),
                OutputRef::Text(t) => format!("\"{t}\""),
            },
        };
        let outputs = &self.show.outputs;
        let index = match output {
            OutputRef::Number(n) => (*n as usize).checked_sub(1).filter(|i| *i < outputs.len()),
            OutputRef::Text(text) => outputs
                .iter()
                .position(|o| o.name == *text || o.id.to_string() == *text),
        };
        index.ok_or_else(unknown)
    }

    /// Signal-loss mode of the input in `slot`; hold for an empty slot.
    pub fn signal_loss_of(&self, slot: usize) -> SignalLoss {
        self.show
            .inputs
            .iter()
            .find(|i| i.slot == slot)
            .map_or_else(SignalLoss::default, |i| i.signal_loss)
    }

    fn transport(
        &mut self,
        input: &InputRef,
        transport: Transport,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &self.show.inputs[index];
        if !entry.source.has_transport() {
            return Err(ActionError::NoTransport {
                reference: entry.name.clone(),
            });
        }
        Ok(vec![Command::Transport {
            slot: entry.slot,
            transport,
        }])
    }

    fn set_listen(
        &mut self,
        input: &InputRef,
        listen: Listen,
    ) -> Result<Vec<Command>, ActionError> {
        let index = self.find(input)?;
        let entry = &self.show.inputs[index];
        self.listen.insert(entry.id, listen);
        Ok(vec![Command::SetListen {
            slot: entry.slot,
            listen,
        }])
    }

    fn clear_listen(&mut self) -> Vec<Command> {
        let soloed: Vec<usize> = self
            .show
            .inputs
            .iter()
            .filter(|i| self.listen_of(i.id) != Listen::Off)
            .map(|i| i.slot)
            .collect();
        self.listen.clear();
        soloed
            .into_iter()
            .map(|slot| Command::SetListen {
                slot,
                listen: Listen::Off,
            })
            .collect()
    }

    /// Audio follows the picture: a take fades sound as long as the
    /// transition; everything else only de-clicks.
    fn audio_ramp(&self, action: &Action) -> u32 {
        let Action::Transition { slot } = action else {
            return AUDIO_RAMP_MS;
        };
        self.button(*slot)
            .map_or(AUDIO_RAMP_MS, |b| b.duration_ms.max(AUDIO_RAMP_MS))
    }

    /// Channels whose wanted setting differs from what the engine has.
    fn audio_commands(&mut self, ramp_ms: u32) -> Vec<Command> {
        let wanted: Vec<(usize, Mix)> = self
            .show
            .inputs
            .iter()
            .filter(|i| i.source.has_audio())
            .map(|i| (i.slot, self.mix_of(i)))
            .collect();
        self.mix
            .retain(|slot, _| wanted.iter().any(|(s, _)| s == slot));

        let mut commands = Vec::new();
        for (slot, mix) in wanted {
            if self.mix.get(&slot) == Some(&mix) {
                continue;
            }
            self.mix.insert(slot, mix);
            commands.push(Command::SetAudio {
                slot,
                gain: mix.gain,
                muted: mix.muted,
                ramp_ms,
            });
        }
        commands
    }

    fn mix_of(&self, input: &ShowInput) -> Mix {
        let audio = input.audio;
        let off_air = audio.follow_video && self.show.program != Some(input.id);
        Mix {
            gain: fader_gain(audio.volume),
            muted: !audio.on || off_air,
        }
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

    /// Slot that carries `input`.
    pub fn slot_for(&self, input: &InputRef) -> Result<usize, ActionError> {
        let index = self.find(input)?;
        Ok(self.show.inputs[index].slot)
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
            InputRef::Number(n) => (*n as usize)
                .checked_sub(1)
                .filter(|i| *i < self.show.inputs.len()),
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

/// Program change, then the new preview (if any). Preview comes second:
/// the engine refuses a preview that is still on program.
fn program_then_preview(
    program: Option<usize>,
    preview: Option<usize>,
    effect: Effect,
    duration_ms: u32,
) -> Vec<Command> {
    let mut commands = vec![Command::Program {
        slot: program,
        effect,
        duration_ms,
    }];
    if let Some(slot) = preview {
        commands.push(Command::Preview { slot });
    }
    commands
}

/// Reject sources the engine cannot build (a color that is not `#rrggbb`).
fn check_source(source: &SourceSpec) -> Result<(), ActionError> {
    let SourceSpec::Color { color } = source else {
        return Ok(());
    };
    match color_rgb(color) {
        Some(_) => Ok(()),
        None => Err(ActionError::BadColor {
            color: color.clone(),
        }),
    }
}

/// Audio inputs have no picture for preview or program.
/// 0-based index of overlay `channel` (1–4).
fn overlay_index(channel: u8) -> Result<usize, ActionError> {
    if !(1..=OVERLAY_CHANNELS).contains(&channel) {
        return Err(ActionError::BadOverlay {
            channel,
            max: OVERLAY_CHANNELS,
        });
    }
    Ok(usize::from(channel - 1))
}

fn check_volume(volume: u8) -> Result<(), ActionError> {
    if volume > VOLUME_MAX {
        return Err(ActionError::BadVolume {
            volume,
            max: VOLUME_MAX,
        });
    }
    Ok(())
}

/// Every aux send of an input with sound, as the show has them.
fn send_commands(input: &ShowInput) -> impl Iterator<Item = Command> + '_ {
    input
        .sends
        .iter()
        .enumerate()
        .map(|(bus, level)| Command::SetAuxSend {
            slot: input.slot,
            bus,
            gain: fader_gain(*level),
        })
}

fn no_video_check(entry: &ShowInput) -> Result<(), ActionError> {
    if entry.source.has_video() {
        return Ok(());
    }
    Err(ActionError::NoVideo {
        reference: entry.name.clone(),
    })
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
    fn color_tags_an_input_without_engine_work() {
        let mut sw = two_inputs();

        let cmds = sw
            .apply(Action::SetInputColor {
                input: named("B"),
                color: Some(InputColor::Cyan),
            })
            .unwrap();

        assert!(cmds.is_empty());
        assert_eq!(sw.state().inputs[1].color, Some(InputColor::Cyan));
    }

    fn audio_of(cmds: &[Command]) -> Vec<(usize, bool, u32)> {
        cmds.iter()
            .filter_map(|c| match c {
                Command::SetAudio {
                    slot,
                    muted,
                    ramp_ms,
                    ..
                } => Some((*slot, *muted, *ramp_ms)),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn follow_video_is_heard_only_on_program() {
        let mut sw = two_inputs();
        sw.apply(Action::SetTransition {
            slot: TransitionSlot(1),
            effect: Effect::Fade,
            duration_ms: 800,
            stinger: None,
        })
        .unwrap();

        let cmds = sw
            .apply(Action::SetAudioFollow {
                input: named("B"),
                follow: true,
            })
            .unwrap();
        assert_eq!(audio_of(&cmds), [(1, true, AUDIO_RAMP_MS)]);

        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(1),
            })
            .unwrap();
        assert_eq!(
            audio_of(&cmds),
            [(1, false, 800)],
            "fades in with the picture"
        );

        sw.apply(Action::PreviewInput { input: named("A") })
            .unwrap();
        let cmds = sw.apply(Action::Cut).unwrap();
        assert_eq!(audio_of(&cmds), [(1, true, AUDIO_RAMP_MS)]);
    }

    #[test]
    fn audio_off_and_volume() {
        let mut sw = two_inputs();

        let cmds = sw
            .apply(Action::SetAudio {
                input: named("A"),
                on: false,
            })
            .unwrap();
        assert_eq!(audio_of(&cmds), [(0, true, AUDIO_RAMP_MS)]);

        let cmds = sw
            .apply(Action::SetVolume {
                input: named("A"),
                volume: 50,
            })
            .unwrap();
        assert!(matches!(cmds[..], [Command::SetAudio { gain, .. }] if gain == 0.25));

        let err = sw
            .apply(Action::SetVolume {
                input: named("A"),
                volume: 101,
            })
            .unwrap_err();
        assert!(matches!(err, ActionError::BadVolume { .. }));
    }

    #[test]
    fn audio_inputs_stay_off_the_buses() {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::AudioDevice { id: "mic".into() },
            name: Some("Mic".into()),
        })
        .unwrap();

        for action in [
            Action::PreviewInput {
                input: named("Mic"),
            },
            Action::CutDirect {
                input: named("Mic"),
            },
        ] {
            assert!(matches!(sw.apply(action), Err(ActionError::NoVideo { .. })));
        }
        sw.apply(Action::PreviewInput { input: named("A") })
            .unwrap();
        let swap = Action::ReplaceSource {
            input: named("A"),
            source: SourceSpec::AudioDevice { id: "mic".into() },
        };
        assert!(matches!(sw.apply(swap), Err(ActionError::NoVideo { .. })));
    }

    #[test]
    fn signal_loss_defaults_to_hold_and_is_kept() {
        let mut sw = two_inputs();
        assert_eq!(sw.state().inputs[0].signal_loss, SignalLoss::Hold);

        let cmds = sw
            .apply(Action::SetSignalLoss {
                input: named("B"),
                mode: SignalLoss::Black,
            })
            .unwrap();

        assert_eq!(
            cmds,
            [Command::SetSignalLoss {
                slot: 1,
                mode: SignalLoss::Black
            }]
        );
        assert_eq!(sw.signal_loss_of(1), SignalLoss::Black);
        let restored = sw.restore_commands();
        assert!(restored.contains(&Command::SetSignalLoss {
            slot: 1,
            mode: SignalLoss::Black
        }));
    }

    fn overlay_in(sw: &mut Switcher, channel: u8, input: &str) -> Vec<Command> {
        sw.apply(Action::OverlayIn {
            channel,
            input: named(input),
            duration_ms: 500,
        })
        .unwrap()
    }

    #[test]
    fn overlay_goes_on_and_off_with_tally() {
        let mut sw = two_inputs();

        let cmds = overlay_in(&mut sw, 2, "B");

        assert_eq!(
            cmds,
            [Command::Overlay {
                channel: 1,
                slot: Some(1),
                duration_ms: 500
            }]
        );
        let b = sw.state().inputs[1].id;
        assert_eq!(sw.state().overlays, [None, Some(b), None, None]);
        assert_eq!(sw.tally(), [0, 1]);

        let cmds = sw
            .apply(Action::OverlayOut {
                channel: 2,
                duration_ms: 0,
            })
            .unwrap();

        assert_eq!(
            cmds,
            [Command::Overlay {
                channel: 1,
                slot: None,
                duration_ms: 0
            }]
        );
        assert_eq!(sw.tally(), [0, 0]);
        let again = sw.apply(Action::OverlayOut {
            channel: 2,
            duration_ms: 0,
        });
        assert_eq!(again, Ok(Vec::new()));
    }

    #[test]
    fn overlay_channel_and_source_are_checked() {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::AudioDevice { id: "mic".into() },
            name: Some("Mic".into()),
        })
        .unwrap();

        let bad = sw.apply(Action::OverlayIn {
            channel: 5,
            input: named("A"),
            duration_ms: 0,
        });
        let sound_only = sw.apply(Action::OverlayIn {
            channel: 1,
            input: named("Mic"),
            duration_ms: 0,
        });

        assert_eq!(bad, Err(ActionError::BadOverlay { channel: 5, max: 4 }));
        assert!(matches!(sound_only, Err(ActionError::NoVideo { .. })));
    }

    #[test]
    fn removed_input_leaves_its_overlay_and_restore_keeps_overlays() {
        let mut sw = two_inputs();
        overlay_in(&mut sw, 1, "A");
        overlay_in(&mut sw, 3, "B");

        let restored = sw.restore_commands();
        assert!(restored.contains(&Command::Overlay {
            channel: 2,
            slot: Some(1),
            duration_ms: 0
        }));
        assert!(restored.contains(&Command::Overlay {
            channel: 1,
            slot: None,
            duration_ms: 0
        }));

        let cmds = sw.apply(Action::RemoveInput { input: named("A") }).unwrap();

        assert!(cmds.contains(&Command::Overlay {
            channel: 0,
            slot: None,
            duration_ms: 0
        }));
        assert_eq!(sw.state().overlays[0], None);
    }

    #[test]
    fn t_bar_mixes_preview_in_and_takes_at_the_end() {
        let mut sw = two_inputs();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();

        let half = sw.apply(Action::TBar { position: 0.5 }).unwrap();

        assert_eq!(
            half,
            [Command::TBar {
                slot: 1,
                position: 0.5
            }]
        );
        assert_eq!(sw.state().t_bar, 0.5);
        assert_eq!(sw.tally(), [1, 2], "nothing taken yet");

        let end = sw.apply(Action::TBar { position: 1.0 }).unwrap();

        assert!(matches!(
            end.first(),
            Some(Command::Program {
                slot: Some(1),
                effect: Effect::Cut,
                ..
            })
        ));
        assert_eq!(sw.tally(), [2, 1], "preview went to program");
        assert_eq!(sw.state().t_bar, 0.0);
    }

    #[test]
    fn t_bar_back_to_zero_and_bad_positions() {
        let mut sw = two_inputs();
        assert_eq!(
            sw.apply(Action::TBar { position: 0.3 }),
            Err(ActionError::NoPreview)
        );
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();

        let back = sw.apply(Action::TBar { position: 0.0 }).unwrap();

        assert_eq!(
            back,
            [Command::TBar {
                slot: 1,
                position: 0.0
            }]
        );
        assert_eq!(
            sw.apply(Action::TBar { position: 1.5 }),
            Err(ActionError::BadTBar)
        );
    }

    #[test]
    fn channel_strip_is_checked_kept_and_restored() {
        let mut sw = Switcher::new(SLOTS);
        sw.apply(Action::AddInput {
            source: SourceSpec::AudioDevice { id: "mic".into() },
            name: Some("Mic".into()),
        })
        .unwrap();
        sw.apply(Action::AddInput {
            source: SourceSpec::Color {
                color: "#ff0000".into(),
            },
            name: Some("Red".into()),
        })
        .unwrap();
        let mut strip = ChannelStrip {
            gain_db: 6.0,
            delay_ms: 80,
            ..Default::default()
        };
        strip.compressor.on = true;

        let cmds = sw
            .apply(Action::SetChannelStrip {
                input: named("Mic"),
                strip,
            })
            .unwrap();

        assert_eq!(cmds, [Command::SetStrip { slot: 0, strip }]);
        assert_eq!(sw.state().inputs[0].strip, strip);
        assert!(sw
            .restore_commands()
            .contains(&Command::SetStrip { slot: 0, strip }));

        let loud = ChannelStrip {
            gain_db: 30.0,
            ..Default::default()
        };
        assert_eq!(
            sw.apply(Action::SetChannelStrip {
                input: named("Mic"),
                strip: loud,
            }),
            Err(ActionError::BadStrip {
                field: "gain_db".into()
            })
        );
        assert!(matches!(
            sw.apply(Action::SetChannelStrip {
                input: named("Red"),
                strip,
            }),
            Err(ActionError::NoAudio { .. })
        ));
    }

    #[test]
    fn a_new_input_with_sound_resets_its_channel() {
        let mut sw = Switcher::new(SLOTS);

        let cmds = sw
            .apply(Action::AddInput {
                source: SourceSpec::AudioDevice { id: "mic".into() },
                name: None,
            })
            .unwrap();

        assert!(cmds.contains(&Command::SetStrip {
            slot: 0,
            strip: ChannelStrip::default()
        }));
    }

    #[test]
    fn a_source_with_sound_gets_the_inputs_strip() {
        let mut sw = Switcher::new(SLOTS);
        sw.apply(Action::AddInput {
            source: SourceSpec::Color {
                color: "#ff0000".into(),
            },
            name: Some("Red".into()),
        })
        .unwrap();

        let cmds = sw
            .apply(Action::ReplaceSource {
                input: named("Red"),
                source: SourceSpec::AudioDevice { id: "mic".into() },
            })
            .unwrap();

        assert!(cmds.contains(&Command::SetStrip {
            slot: 0,
            strip: ChannelStrip::default()
        }));
    }

    #[test]
    fn aux_sends_and_masters_are_kept_and_restored() {
        let mut sw = two_inputs();

        let cmds = sw
            .apply(Action::SetAuxSend {
                input: named("B"),
                bus: 2,
                level: 50,
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::SetAuxSend {
                slot: 1,
                bus: 1,
                gain: fader_gain(50)
            }]
        );
        assert_eq!(sw.state().inputs[1].sends, [0, 50]);

        sw.apply(Action::SetAuxBus {
            bus: 1,
            volume: 80,
            mute: true,
        })
        .unwrap();
        assert_eq!(
            sw.state().aux_buses[0],
            AuxBus {
                volume: 80,
                mute: true
            }
        );

        let restore = sw.restore_commands();
        assert!(restore.contains(&Command::SetAuxSend {
            slot: 1,
            bus: 1,
            gain: fader_gain(50)
        }));
        assert!(restore.contains(&Command::SetAuxBus {
            bus: 0,
            gain: fader_gain(80),
            mute: true
        }));
    }

    #[test]
    fn aux_bus_numbers_are_checked() {
        let mut sw = two_inputs();
        for bus in [0, 3] {
            assert!(matches!(
                sw.apply(Action::SetAuxBus {
                    bus,
                    volume: 50,
                    mute: false
                }),
                Err(ActionError::BadAuxBus { .. })
            ));
        }
        let srt = OutputSpec::Srt {
            uri: "srt://cdn:9000".into(),
            video_kbps: 6000,
        };
        assert!(matches!(
            sw.apply(Action::AddOutput {
                spec: srt,
                source: OutputSource::Program,
                name: None,
                aux: Some(3),
            }),
            Err(ActionError::BadAuxBus { .. })
        ));
    }

    #[test]
    fn an_output_takes_its_sound_from_an_aux_bus() {
        let mut sw = two_inputs();
        let srt = OutputSpec::Srt {
            uri: "srt://cdn:9000".into(),
            video_kbps: 6000,
        };
        sw.apply(Action::AddOutput {
            spec: srt,
            source: OutputSource::Program,
            name: Some("Mix minus".into()),
            aux: Some(1),
        })
        .unwrap();

        let cmds = sw
            .apply(Action::StartOutput {
                output: OutputRef::Number(1),
            })
            .unwrap();

        assert!(matches!(
            cmds[..],
            [Command::StartOutput {
                feed: Feed::Program,
                aux: Some(0),
                ..
            }]
        ));
        assert_eq!(sw.state().outputs[0].aux, Some(1));
    }

    /// A and B, B in preview, A on program, and the clip "Sting" (slot 2)
    /// on button 2: 1000 ms, cut at 400 ms.
    fn with_stinger() -> Switcher {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::File {
                path: "sting.webm".into(),
            },
            name: Some("Sting".into()),
        })
        .unwrap();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        let cmds = sw
            .apply(Action::SetTransition {
                slot: TransitionSlot(2),
                effect: Effect::Stinger,
                duration_ms: 1000,
                stinger: Some(StingerSpec {
                    input: named("Sting"),
                    cut_ms: 400,
                }),
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::Transport {
                slot: 2,
                transport: Transport::Pause
            }],
            "the clip waits paused"
        );
        sw
    }

    #[test]
    fn a_stinger_cuts_underneath_at_its_cut_point() {
        let mut sw = with_stinger();

        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(2),
            })
            .unwrap();

        assert_eq!(
            cmds[..4],
            [
                Command::Stinger { slot: 2, on: true },
                Command::Wait { ms: 400 },
                Command::Program {
                    slot: Some(1),
                    effect: Effect::Cut,
                    duration_ms: 0
                },
                Command::Preview { slot: 0 },
            ]
        );
        assert_eq!(
            cmds[cmds.len() - 2..],
            [
                Command::Wait { ms: 600 },
                Command::Stinger { slot: 2, on: false }
            ]
        );
        assert_eq!(sw.state().program, Some(sw.state().inputs[1].id));
        assert_eq!(sw.apply(Action::Cut), Err(ActionError::StingerPlaying));
    }

    #[test]
    fn stinger_settings_are_checked() {
        let mut sw = with_stinger();
        let set = |input: &str, cut_ms| Action::SetTransition {
            slot: TransitionSlot(1),
            effect: Effect::Stinger,
            duration_ms: 1000,
            stinger: Some(StingerSpec {
                input: named(input),
                cut_ms,
            }),
        };

        for action in [set("A", 400), set("Sting", 1200)] {
            assert!(matches!(
                sw.apply(action),
                Err(ActionError::BadStinger { .. })
            ));
        }
        assert!(matches!(
            sw.apply(Action::SetTransition {
                slot: TransitionSlot(1),
                effect: Effect::Stinger,
                duration_ms: 1000,
                stinger: None,
            }),
            Err(ActionError::BadStinger { .. })
        ));
    }

    #[test]
    fn stinger_clips_wait_paused_and_leave_with_their_input() {
        let mut sw = with_stinger();

        assert!(sw.restore_commands().contains(&Command::Transport {
            slot: 2,
            transport: Transport::Pause
        }));

        sw.apply(Action::RemoveInput {
            input: named("Sting"),
        })
        .unwrap();
        let button = sw.state().transitions[1];
        assert_eq!((button.effect, button.stinger), (Effect::Cut, None));
    }

    #[test]
    fn looks_need_an_engine_that_can_apply_them() {
        let mut sw = two_inputs();
        let warm = Look {
            temperature: 0.3,
            ..Default::default()
        };

        assert_eq!(
            sw.apply(Action::SetLook {
                input: named("A"),
                look: warm,
            }),
            Err(ActionError::NoLooks)
        );

        sw.set_looks_available(true);
        let cmds = sw
            .apply(Action::SetLook {
                input: named("A"),
                look: warm,
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::SetLook {
                slot: 0,
                look: warm
            }]
        );
        assert_eq!(sw.state().inputs[0].look, warm);
        assert!(sw.state().looks_available);
        assert!(sw.restore_commands().contains(&Command::SetLook {
            slot: 0,
            look: warm
        }));

        let wild = Look {
            gamma: 5.0,
            ..Default::default()
        };
        assert_eq!(
            sw.apply(Action::SetLook {
                input: named("A"),
                look: wild,
            }),
            Err(ActionError::BadLook {
                field: "gamma".into()
            })
        );
    }

    #[test]
    fn a_new_input_clears_the_look_of_its_slot() {
        let mut sw = Switcher::new(SLOTS);
        sw.set_looks_available(true);

        let cmds = sw
            .apply(Action::AddInput {
                source: pattern(TestPattern::Ball),
                name: None,
            })
            .unwrap();

        assert!(cmds.contains(&Command::SetLook {
            slot: 0,
            look: Look::default()
        }));
    }

    #[test]
    fn whip_inputs_carry_their_ingest_links() {
        let mut sw = Switcher::new(SLOTS);
        sw.set_ingest_base("http://studio:8080/".into());
        sw.apply(Action::AddInput {
            source: SourceSpec::Whip {
                endpoint: "guest".into(),
            },
            name: None,
        })
        .unwrap();

        let ingest = sw.state().inputs[0].ingest.clone().unwrap();
        assert_eq!(ingest.whip_url, "http://studio:8080/whip/guest");
        assert_eq!(ingest.browser_url, "http://studio:8080/player/whip-ingest");
    }

    #[test]
    fn outputs_start_stop_and_restart_after_engine_loss() {
        let mut sw = Switcher::new(SLOTS);
        let srt = OutputSpec::Srt {
            uri: "srt://cdn:9000".into(),
            video_kbps: 6000,
        };
        sw.apply(Action::AddOutput {
            spec: srt.clone(),
            source: OutputSource::Program,
            name: Some("Stream".into()),
            aux: None,
        })
        .unwrap();
        let id = sw.state().outputs[0].id;

        let cmds = sw
            .apply(Action::StartOutput {
                output: OutputRef::Text("Stream".into()),
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::StartOutput {
                id,
                spec: srt.clone(),
                feed: Feed::Program,
                aux: None,
            }]
        );
        sw.set_output_status(id, OutputStatus::Running { since_ms: 1 });

        // Engine restart: running outputs come back, stopped ones do not.
        assert!(sw.restore_commands().contains(&Command::StartOutput {
            id,
            spec: srt,
            feed: Feed::Program,
            aux: None,
        }));

        let cmds = sw
            .apply(Action::RemoveOutput {
                output: OutputRef::Number(1),
            })
            .unwrap();
        assert_eq!(cmds, [Command::StopOutput { id }]);
        assert!(sw.state().outputs.is_empty());
        let err = sw
            .apply(Action::StopOutput {
                output: OutputRef::Number(1),
            })
            .unwrap_err();
        assert!(matches!(err, ActionError::UnknownOutput { .. }));
    }

    #[test]
    fn iso_output_follows_its_input() {
        let mut sw = two_inputs();
        let b = sw.state().inputs[1].id;
        let rec = OutputSpec::Record {
            folder: "iso".into(),
            container: lmix_types::Container::Mkv,
            video_kbps: 8000,
        };
        sw.apply(Action::AddOutput {
            spec: rec,
            source: OutputSource::Input(b),
            name: None,
            aux: None,
        })
        .unwrap();
        let output = sw.state().outputs[0].clone();
        assert_eq!(output.name, "Record iso (B)");

        let cmds = sw
            .apply(Action::StartOutput {
                output: OutputRef::Number(1),
            })
            .unwrap();
        assert!(matches!(
            cmds[0],
            Command::StartOutput {
                feed: Feed::Slot {
                    slot: 1,
                    audio: true
                },
                ..
            }
        ));
        sw.set_output_status(output.id, OutputStatus::Running { since_ms: 1 });

        let cmds = sw.apply(Action::RemoveInput { input: named("B") }).unwrap();
        assert!(cmds.contains(&Command::StopOutput { id: output.id }));
        assert!(matches!(
            sw.state().outputs[0].status,
            OutputStatus::Failed { .. }
        ));
        let err = sw
            .apply(Action::StartOutput {
                output: OutputRef::Number(1),
            })
            .unwrap_err();
        assert!(matches!(err, ActionError::UnknownInput { .. }));
    }

    #[test]
    fn bad_colors_are_refused() {
        let mut sw = Switcher::new(SLOTS);
        let add = |color: &str| Action::AddInput {
            source: SourceSpec::Color {
                color: color.into(),
            },
            name: None,
        };

        assert!(matches!(
            sw.apply(add("red")),
            Err(ActionError::BadColor { .. })
        ));
        assert!(sw.apply(add("#204080")).is_ok());
    }

    #[test]
    fn transport_only_for_media_players() {
        let mut sw = two_inputs();
        sw.apply(Action::AddInput {
            source: SourceSpec::File {
                path: "/clip.mp4".into(),
            },
            name: Some("Clip".into()),
        })
        .unwrap();

        let cmds = sw
            .apply(Action::TransportInput {
                input: named("Clip"),
                transport: Transport::Pause,
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::Transport {
                slot: 2,
                transport: Transport::Pause
            }]
        );

        let err = sw
            .apply(Action::TransportInput {
                input: named("A"),
                transport: Transport::Play,
            })
            .unwrap_err();
        assert!(matches!(err, ActionError::NoTransport { .. }));
    }

    #[test]
    fn listen_is_kept_per_input_and_cleared_together() {
        let mut sw = two_inputs();

        let cmds = sw
            .apply(Action::SetListen {
                input: named("A"),
                listen: Listen::Pfl,
            })
            .unwrap();
        assert_eq!(
            cmds,
            [Command::SetListen {
                slot: 0,
                listen: Listen::Pfl
            }]
        );
        sw.apply(Action::SetListen {
            input: named("B"),
            listen: Listen::Afl,
        })
        .unwrap();
        assert_eq!(sw.state().inputs[1].listen, Listen::Afl);

        let cmds = sw.apply(Action::ClearListen).unwrap();
        assert_eq!(cmds.len(), 2);
        assert!(sw.state().inputs.iter().all(|i| i.listen == Listen::Off));
    }

    #[test]
    fn removing_a_soloed_input_ends_its_solo() {
        let mut sw = two_inputs();
        sw.apply(Action::SetListen {
            input: named("B"),
            listen: Listen::Pfl,
        })
        .unwrap();

        let cmds = sw.apply(Action::RemoveInput { input: named("B") }).unwrap();

        assert!(cmds.contains(&Command::SetListen {
            slot: 1,
            listen: Listen::Off
        }));
    }

    #[test]
    fn devices_get_no_audio_commands() {
        let mut sw = Switcher::new(SLOTS);

        let cmds = sw
            .apply(Action::AddInput {
                source: SourceSpec::Device { id: "cam".into() },
                name: None,
            })
            .unwrap();

        assert!(audio_of(&cmds).is_empty());
        assert!(!sw.state().inputs[0].has_audio);
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
            vec![
                Command::SetSlot {
                    slot: 0,
                    source: Some(pattern(TestPattern::Ball))
                },
                Command::SetSignalLoss {
                    slot: 0,
                    mode: SignalLoss::Hold
                },
                Command::SetStrip {
                    slot: 0,
                    strip: ChannelStrip::default()
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 0,
                    gain: 0.0
                },
                Command::SetAuxSend {
                    slot: 0,
                    bus: 1,
                    gain: 0.0
                },
                Command::SetAudio {
                    slot: 0,
                    gain: 1.0,
                    muted: false,
                    ramp_ms: AUDIO_RAMP_MS
                },
            ]
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
        sw.apply(Action::PreviewInput {
            input: InputRef::Number(2),
        })
        .unwrap();

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
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();

        let cmds = sw.apply(Action::Cut).unwrap();

        assert_eq!(
            cmds,
            vec![
                Command::Program {
                    slot: Some(1),
                    effect: Effect::Cut,
                    duration_ms: 0
                },
                Command::Preview { slot: 0 },
            ]
        );
        assert_eq!(sw.state().program, Some(b));
        assert_eq!(sw.state().preview, Some(a));
        assert_eq!(sw.tally(), vec![2, 1]);
    }

    #[test]
    fn transition_uses_button_settings() {
        let mut sw = two_inputs();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        sw.apply(Action::SetTransition {
            slot: TransitionSlot(3),
            effect: Effect::SlideLeft,
            duration_ms: 750,
            stinger: None,
        })
        .unwrap();

        let cmds = sw
            .apply(Action::Transition {
                slot: TransitionSlot(3),
            })
            .unwrap();

        assert_eq!(
            cmds,
            vec![Command::Program {
                slot: Some(1),
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
            let err = sw
                .apply(Action::Transition {
                    slot: TransitionSlot(slot),
                })
                .unwrap_err();
            assert!(matches!(err, ActionError::BadTransitionSlot { .. }));
        }
    }

    #[test]
    fn removing_program_input_clears_program() {
        let mut sw = two_inputs();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();

        let cmds = sw.apply(Action::RemoveInput { input: named("A") }).unwrap();

        assert_eq!(sw.state().program, None);
        assert!(matches!(cmds[0], Command::Program { slot: None, .. }));
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
        assert!(sw
            .apply(Action::MoveInput {
                input: named("A"),
                number: 3
            })
            .is_err());
    }

    #[test]
    fn replace_rebuilds_slots() {
        let mut sw = two_inputs();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();
        let show = sw.show().clone();
        let mut fresh = Switcher::new(SLOTS);

        let cmds = fresh.replace(show, Some("x.lmix".into()));

        // Picture first (slots, program, preview), then audio and the rest.
        assert!(matches!(cmds[0], Command::SetSlot { slot: 0, .. }));
        assert!(matches!(cmds[1], Command::SetSlot { slot: 1, .. }));
        assert!(matches!(cmds[2], Command::Program { slot: None, .. }));
        assert_eq!(cmds[3], Command::Preview { slot: 1 });
        assert!(matches!(cmds[4], Command::SetAudio { slot: 0, .. }));
        assert_eq!(fresh.state().path.as_deref(), Some("x.lmix"));
    }

    #[test]
    fn program_input_cannot_be_previewed() {
        let mut sw = two_inputs();
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();

        let err = sw
            .apply(Action::PreviewInput { input: named("A") })
            .unwrap_err();

        assert!(matches!(err, ActionError::OnProgram { .. }));
    }

    #[test]
    fn cut_direct_of_preview_swaps() {
        let mut sw = two_inputs();
        let (a, b) = (sw.state().inputs[0].id, sw.state().inputs[1].id);
        sw.apply(Action::CutDirect { input: named("A") }).unwrap();
        sw.apply(Action::PreviewInput { input: named("B") })
            .unwrap();

        let cmds = sw.apply(Action::CutDirect { input: named("B") }).unwrap();

        assert_eq!((sw.state().program, sw.state().preview), (Some(b), Some(a)));
        assert_eq!(cmds[1], Command::Preview { slot: 0 });
    }

    #[test]
    fn failed_action_changes_nothing() {
        let mut sw = two_inputs();
        let before = sw.state();

        let _ = sw.apply(Action::SetInputColor {
            input: named("missing"),
            color: Some(InputColor::Red),
        });
        let _ = sw.apply(Action::RenameInput {
            input: named("missing"),
            name: "X".into(),
        });

        assert_eq!(sw.state(), before);
    }
}
