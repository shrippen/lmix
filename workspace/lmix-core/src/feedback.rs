//! Lamps on MIDI controllers (AUT.6): a shortcut's note or controller
//! lights up as its first action's target stands, written back to the
//! same device files lmix reads (`--midi`).
//!
//! | First action | Lamp |
//! |---|---|
//! | `preview_input`, `cut_direct`, `overlay_in`, `quick_play`, `click_input`, `go_input` (an input) | program, preview or off |
//! | `overlay_out`, `overlay_out_all` | program colour while the channel (any channel) shows something |
//! | `overlay_last` | program colour while the channel shows something, preview colour while it can show its last input again |
//! | `overlay_preview` (an input) | program colour while the channel shows the input, preview colour while it is prepared there |
//! | `fade_to_black` | program colour while faded |
//! | `start_output`, `stop_output` | program colour while the output runs |
//! | `set_lock` | program colour while locked |
//!
//! Controllers code colours in the velocity (APC mini: 1 green, 3 red,
//! 5 yellow): `--midi-lamps <program>,<preview>,<off>` sets the values,
//! by default 127, 64, 0.

use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use lmix_types::{Action, Event, InputId, InputRef, MidiBinding, MidiKind, OutputRef, ShowState};
use tokio::sync::broadcast::error::RecvError;

use crate::service::Service;

/// Velocities (or controller values) for the three lamp states.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lamps {
    pub program: u8,
    pub preview: u8,
    pub off: u8,
}

impl Default for Lamps {
    fn default() -> Self {
        Self {
            program: 127,
            preview: 64,
            off: 0,
        }
    }
}

impl std::str::FromStr for Lamps {
    type Err = String;
    fn from_str(text: &str) -> Result<Self, String> {
        let values: Vec<u8> = text
            .split(',')
            .map(|v| v.trim().parse::<u8>().ok().filter(|v| *v <= 127))
            .collect::<Option<_>>()
            .ok_or_else(|| "three values 0–127: program,preview,off".to_string())?;
        match values[..] {
            [program, preview, off] => Ok(Self {
                program,
                preview,
                off,
            }),
            _ => Err("three values 0–127: program,preview,off".into()),
        }
    }
}

/// The input an action's reference names in `state`.
fn input_of(state: &ShowState, input: &InputRef) -> Option<InputId> {
    // "dynamic1"…, as the switcher reads them.
    let dynamic = |t: &str| {
        let n = t.strip_prefix("dynamic")?.parse::<usize>().ok()?;
        state
            .dynamic_inputs
            .get(n.checked_sub(1)?)
            .copied()
            .flatten()
    };
    match input {
        InputRef::Number(n) => state
            .inputs
            .get((*n as usize).checked_sub(1)?)
            .map(|i| i.id),
        InputRef::Text(t) if t == "preview" => state.preview,
        InputRef::Text(t) if t == "program" => state.program,
        InputRef::Text(t) if dynamic(t).is_some() => dynamic(t),
        InputRef::Text(t) => state
            .inputs
            .iter()
            .find(|i| i.id.to_string() == *t || i.name == *t)
            .map(|i| i.id),
    }
}

fn output_running(state: &ShowState, output: &OutputRef) -> bool {
    let found = match output {
        OutputRef::Number(n) => (*n as usize)
            .checked_sub(1)
            .and_then(|i| state.outputs.get(i)),
        OutputRef::Text(t) => state
            .outputs
            .iter()
            .find(|o| o.name == *t || o.id.to_string() == *t),
    };
    found.is_some_and(|o| matches!(o.status, lmix_types::OutputStatus::Running { .. }))
}

/// Whether overlay `channel` (1–8) shows something.
fn shows(state: &ShowState, channel: u8) -> bool {
    usize::from(channel)
        .checked_sub(1)
        .and_then(|i| state.overlays.get(i))
        .is_some_and(Option::is_some)
}

/// Lamp value per bound note or controller in `state`.
pub fn lamps(state: &ShowState, values: Lamps) -> HashMap<MidiBinding, u8> {
    let on = |yes: bool| if yes { values.program } else { values.off };
    state
        .shortcuts
        .iter()
        // A shadowed one's note belongs to the show's shortcut.
        .filter(|s| !s.midi_shadowed)
        .filter_map(|s| {
            let binding = s.midi?;
            let value = match s.actions.first()? {
                Action::PreviewInput { input }
                | Action::CutDirect { input }
                | Action::OverlayIn { input, .. }
                | Action::QuickPlay { input: Some(input) }
                | Action::ClickInput { input }
                | Action::GoInput { input } => match input_of(state, input) {
                    Some(id) if state.program == Some(id) => values.program,
                    Some(id) if state.preview == Some(id) => values.preview,
                    _ => values.off,
                },
                Action::OverlayOut { channel, .. } => on(shows(state, *channel)),
                Action::OverlayOutAll { .. } => on(state.overlays.iter().any(Option::is_some)),
                Action::OverlayLast { channel, .. } => {
                    let last = usize::from(*channel)
                        .checked_sub(1)
                        .and_then(|i| state.overlay_last.get(i).copied().flatten());
                    match (shows(state, *channel), last) {
                        (true, _) => values.program,
                        (false, Some(_)) => values.preview,
                        (false, None) => values.off,
                    }
                }
                Action::OverlayPreview {
                    channel,
                    input: Some(input),
                } => {
                    let at = |list: &[Option<InputId>]| {
                        let id = input_of(state, input);
                        id.is_some()
                            && usize::from(*channel)
                                .checked_sub(1)
                                .and_then(|i| list.get(i).copied())
                                == Some(id)
                    };
                    if at(&state.overlays) {
                        values.program
                    } else if at(&state.overlay_preview) {
                        values.preview
                    } else {
                        values.off
                    }
                }
                Action::FadeToBlack { .. } => on(state.fade_to_black),
                Action::StartOutput { output } | Action::StopOutput { output } => {
                    on(output_running(state, output))
                }
                Action::SetLock { .. } => on(state.locked),
                Action::SetCards { .. } => on(state.cards_on),
                _ => return None,
            };
            Some((binding, value))
        })
        .collect()
}

/// MIDI bytes that set a lamp.
pub fn message(binding: &MidiBinding, value: u8) -> [u8; 3] {
    let channel = binding.channel.map_or(0, |c| c.saturating_sub(1).min(15));
    let status = match binding.kind {
        MidiKind::Note => 0x90,
        MidiKind::Cc => 0xB0,
    };
    [status | channel, binding.number & 0x7F, value & 0x7F]
}

/// Write lamp changes to `devices` until the process ends.
pub async fn serve(devices: Vec<PathBuf>, values: Lamps, service: Arc<Service>) {
    let mut events = service.subscribe();
    let mut shown: HashMap<MidiBinding, u8> = HashMap::new();
    let mut state = Some(service.state().await);
    loop {
        if let Some(state) = state.take() {
            let now = lamps(&state, values);
            let changes: Vec<u8> = now
                .iter()
                .filter(|(b, v)| shown.get(b) != Some(v))
                .flat_map(|(b, v)| message(b, *v))
                // A binding gone (shortcut removed or rebound) goes dark.
                .chain(
                    shown
                        .keys()
                        .filter(|b| !now.contains_key(b))
                        .flat_map(|b| message(b, values.off)),
                )
                .collect();
            let mut all_written = true;
            if !changes.is_empty() {
                let devices = devices.clone();
                // Device files block: written off the runtime.
                all_written = tokio::task::spawn_blocking(move || {
                    let mut ok = true;
                    for device in &devices {
                        let written = std::fs::OpenOptions::new()
                            .append(true)
                            .open(device)
                            .and_then(|mut f| f.write_all(&changes));
                        if let Err(error) = written {
                            tracing::debug!(device = %device.display(), %error, "MIDI lamps not written");
                            ok = false;
                        }
                    }
                    ok
                })
                .await
                .unwrap_or(false);
            }
            // Not written (controller unplugged): all lamps again next
            // time, so a controller plugged back in lights up.
            shown = if all_written { now } else { HashMap::new() };
        }
        state = match events.recv().await {
            Ok(Event::State { state }) => Some(state),
            Ok(_) => None,
            Err(RecvError::Lagged(_)) => Some(service.state().await),
            Err(RecvError::Closed) => return,
        };
        // Bursts of changes (a transition) settle before lamps are set.
        tokio::time::sleep(Duration::from_millis(20)).await;
        while let Ok(event) = events.try_recv() {
            if let Event::State { state: newer } = event {
                state = Some(newer);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::null::NullEngine;
    use lmix_types::{ShortcutSpec, SourceSpec, TestPattern};

    fn key(number: u8, actions: Vec<Action>) -> Action {
        Action::AddShortcut {
            shortcut: ShortcutSpec {
                label: format!("k{number}"),
                key: None,
                web: false,
                color: None,
                midi: Some(MidiBinding {
                    kind: MidiKind::Note,
                    channel: Some(1),
                    number,
                }),
                actions,
                all_shows: false,
            },
        }
    }

    #[test]
    fn lamp_values_parse() {
        assert_eq!(
            "3,1,0".parse::<Lamps>(),
            Ok(Lamps {
                program: 3,
                preview: 1,
                off: 0
            })
        );
        assert!("3,1".parse::<Lamps>().is_err());
        assert!("3,1,200".parse::<Lamps>().is_err());
    }

    #[test]
    fn messages() {
        let note = MidiBinding {
            kind: MidiKind::Note,
            channel: Some(2),
            number: 36,
        };
        assert_eq!(message(&note, 127), [0x91, 36, 127]);
        let cc = MidiBinding {
            kind: MidiKind::Cc,
            channel: None,
            number: 7,
        };
        assert_eq!(message(&cc, 0), [0xB0, 7, 0]);
    }

    #[tokio::test]
    async fn lamps_follow_program_and_preview() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let service = Service::new(4, Arc::new(NullEngine::default()), None);
        for name in ["A", "B"] {
            service
                .handle(Action::AddInput {
                    source: SourceSpec::Pattern {
                        pattern: TestPattern::Bars,
                    },
                    name: Some(name.into()),
                })
                .await
                .unwrap();
        }
        service
            .handle(key(
                1,
                vec![Action::CutDirect {
                    input: InputRef::Number(1),
                }],
            ))
            .await
            .unwrap();
        service
            .handle(key(
                2,
                vec![Action::PreviewInput {
                    input: InputRef::Text("B".into()),
                }],
            ))
            .await
            .unwrap();
        service
            .handle(key(3, vec![Action::FadeToBlack { duration_ms: 0 }]))
            .await
            .unwrap();

        let state = service.state().await;
        let off = lamps(&state, Lamps::default());
        assert_eq!(off.values().copied().max(), Some(0), "nothing on air yet");

        tokio::spawn(serve(
            vec![file.path().to_path_buf()],
            Lamps::default(),
            service.clone(),
        ));
        tokio::time::sleep(Duration::from_millis(100)).await;
        service
            .handle(Action::CutDirect {
                input: InputRef::Number(1),
            })
            .await
            .unwrap();
        service
            .handle(Action::PreviewInput {
                input: InputRef::Number(2),
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;

        let lit = lamps(&service.state().await, Lamps::default());
        let note = |n| MidiBinding {
            kind: MidiKind::Note,
            channel: Some(1),
            number: n,
        };
        assert_eq!((lit[&note(1)], lit[&note(2)], lit[&note(3)]), (127, 64, 0));
        let written = std::fs::read(file.path()).unwrap();
        assert!(
            written.windows(3).any(|w| w == [0x90, 1, 127]),
            "{written:?}"
        );
        assert!(
            written.windows(3).any(|w| w == [0x90, 2, 64]),
            "{written:?}"
        );

        // Shortcut 1 removed: its pad goes dark.
        service
            .handle(Action::RemoveShortcut {
                target: InputRef::Number(1),
            })
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let written = std::fs::read(file.path()).unwrap();
        assert_eq!(&written[written.len() - 3..], &[0x90, 1, 0]);
    }

    #[test]
    fn dynamic_and_numbered_references() {
        let mut sw = crate::switcher::Switcher::new(2);
        sw.apply(Action::AddInput {
            source: SourceSpec::Pattern {
                pattern: TestPattern::Bars,
            },
            name: None,
        })
        .unwrap();
        let mut state = sw.state();
        let id = state.inputs[0].id;
        state.dynamic_inputs[0] = Some(id);

        assert_eq!(
            input_of(&state, &InputRef::Text("dynamic1".into())),
            Some(id)
        );
        assert_eq!(input_of(&state, &InputRef::Text("dynamic2".into())), None);
        assert!(!output_running(&state, &OutputRef::Number(0)));
    }

    #[test]
    fn overlay_keys_light_as_their_channels_stand() {
        let mut sw = crate::switcher::Switcher::new(2);
        sw.apply(Action::AddInput {
            source: SourceSpec::Pattern {
                pattern: TestPattern::Bars,
            },
            name: Some("A".into()),
        })
        .unwrap();
        sw.apply(key(
            1,
            vec![Action::OverlayOutAll {
                duration_ms: Some(0),
            }],
        ))
        .unwrap();
        sw.apply(key(
            2,
            vec![Action::OverlayLast {
                channel: 2,
                duration_ms: Some(0),
            }],
        ))
        .unwrap();
        sw.apply(key(
            3,
            vec![Action::OverlayOut {
                channel: 2,
                duration_ms: Some(0),
            }],
        ))
        .unwrap();
        let note = |n| MidiBinding {
            kind: MidiKind::Note,
            channel: Some(1),
            number: n,
        };
        let lit = |sw: &crate::switcher::Switcher| {
            let lit = lamps(&sw.state(), Lamps::default());
            (lit[&note(1)], lit[&note(2)], lit[&note(3)])
        };
        assert_eq!(lit(&sw), (0, 0, 0), "no overlay, none to show again");

        sw.apply(Action::OverlayIn {
            channel: 2,
            input: InputRef::Number(1),
            duration_ms: Some(0),
        })
        .unwrap();
        assert_eq!(lit(&sw), (127, 127, 127));

        sw.apply(Action::OverlayOutAll {
            duration_ms: Some(0),
        })
        .unwrap();
        assert_eq!(lit(&sw), (0, 64, 0), "channel 2 can show A again");

        sw.apply(key(
            4,
            vec![Action::OverlayPreview {
                channel: 1,
                input: Some(InputRef::Number(1)),
            }],
        ))
        .unwrap();
        let prepared =
            |sw: &crate::switcher::Switcher| lamps(&sw.state(), Lamps::default())[&note(4)];
        assert_eq!(prepared(&sw), 0);
        sw.apply(Action::OverlayPreview {
            channel: 1,
            input: Some(InputRef::Number(1)),
        })
        .unwrap();
        assert_eq!(prepared(&sw), 64, "prepared for the next take");
        sw.apply(Action::OverlayIn {
            channel: 1,
            input: InputRef::Number(1),
            duration_ms: None,
        })
        .unwrap();
        assert_eq!(prepared(&sw), 127, "on the channel");
    }
}
