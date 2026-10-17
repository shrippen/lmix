//! MIDI controllers (AUT.1) read as raw bytes from a device file
//! (`/dev/snd/midiC1D0`, also a FIFO): no ALSA library needed.
//!
//! Notes and controllers run the shortcut bound to them; a controller
//! bound to the T-bar moves it. Every note and controller message is also
//! sent to clients (`Event::Midi`) so they can learn a binding. A device
//! that goes away is opened again every two seconds.

use std::collections::HashMap;
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use lmix_types::{Action, InputRef, MidiKind, MidiMessage};
use tokio::sync::mpsc;

use crate::service::Service;

/// Wait before opening a device again.
const REOPEN: Duration = Duration::from_secs(2);

/// Controller values at or above count as pressed (buttons sending CC).
const CC_PRESSED: u8 = 64;

/// Messages read ahead of being handled.
const QUEUED: usize = 256;

/// Turns raw MIDI bytes into note and controller messages. Keeps running
/// status; skips system exclusive and other messages; real-time bytes may
/// come in between.
#[derive(Default)]
pub struct Parser {
    status: Option<u8>,
    data: Vec<u8>,
    in_sysex: bool,
}

impl Parser {
    pub fn push(&mut self, byte: u8) -> Option<MidiMessage> {
        if byte >= 0xF8 {
            return None; // real-time: clock, start, stop, …
        }
        if byte == 0xF0 {
            self.in_sysex = true;
            return None;
        }
        if self.in_sysex {
            if byte == 0xF7 || byte & 0x80 != 0 {
                self.in_sysex = false;
            }
            if byte == 0xF7 || byte & 0x80 == 0 {
                return None;
            }
        }
        if byte & 0x80 != 0 {
            // Other system common messages cancel running status.
            self.status = (byte < 0xF0).then_some(byte);
            self.data.clear();
            return None;
        }
        let status = self.status?;
        self.data.push(byte);
        let needed = match status & 0xF0 {
            0xC0 | 0xD0 => 1,
            _ => 2,
        };
        if self.data.len() < needed {
            return None;
        }
        let data = std::mem::take(&mut self.data);
        let channel = (status & 0x0F) + 1;
        let (number, value) = (data[0], data.get(1).copied().unwrap_or(0));
        match status & 0xF0 {
            0x90 => Some(MidiMessage {
                kind: MidiKind::Note,
                channel,
                number,
                value,
            }),
            0x80 => Some(MidiMessage {
                kind: MidiKind::Note,
                channel,
                number,
                value: 0,
            }),
            0xB0 => Some(MidiMessage {
                kind: MidiKind::Cc,
                channel,
                number,
                value,
            }),
            _ => None,
        }
    }
}

/// Read `devices` and act on their messages until the process ends.
pub async fn serve(devices: Vec<PathBuf>, service: Arc<Service>) {
    let (tx, mut rx) = mpsc::channel(QUEUED);
    for device in devices {
        let tx = tx.clone();
        std::thread::Builder::new()
            .name("midi".into())
            .spawn(move || read_device(device, tx))
            .expect("start MIDI reader");
    }
    drop(tx);

    // Last value per controller: a button sending CC fires once on press.
    let mut last_cc: HashMap<(u8, u8), u8> = HashMap::new();
    while let Some(message) = rx.recv().await {
        let pressed = match message.kind {
            MidiKind::Note => message.value > 0,
            MidiKind::Cc => {
                let before = last_cc.insert((message.channel, message.number), message.value);
                message.value >= CC_PRESSED && before.is_none_or(|v| v < CC_PRESSED)
            }
        };
        if let Err(error) = service.midi(message, pressed).await {
            tracing::debug!(?message, %error, "MIDI action refused");
        }
    }
}

/// Blocking reads, so a FIFO or a device file works the same.
fn read_device(device: PathBuf, tx: mpsc::Sender<MidiMessage>) {
    loop {
        match std::fs::File::open(&device) {
            Ok(mut file) => {
                tracing::info!(device = %device.display(), "MIDI device open");
                let mut parser = Parser::default();
                let mut buf = [0u8; 256];
                loop {
                    match file.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            for byte in &buf[..n] {
                                if let Some(message) = parser.push(*byte) {
                                    if tx.blocking_send(message).is_err() {
                                        return;
                                    }
                                }
                            }
                        }
                    }
                }
                tracing::info!(device = %device.display(), "MIDI device closed");
            }
            Err(error) => {
                tracing::debug!(device = %device.display(), %error, "MIDI device not there")
            }
        }
        std::thread::sleep(REOPEN);
    }
}

/// What a message does, given the show's bindings.
pub fn action_for(
    message: &MidiMessage,
    pressed: bool,
    shortcut: Option<lmix_types::InputId>,
    moves_tbar: bool,
) -> Option<Action> {
    if moves_tbar && message.kind == MidiKind::Cc {
        return Some(Action::TBar {
            position: f32::from(message.value) / 127.0,
        });
    }
    let id = shortcut.filter(|_| pressed)?;
    Some(Action::RunShortcut {
        target: InputRef::Text(id.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Vec<MidiMessage> {
        let mut parser = Parser::default();
        bytes.iter().filter_map(|b| parser.push(*b)).collect()
    }

    #[test]
    fn notes_controllers_running_status_and_noise() {
        let note = |channel, number, value| MidiMessage {
            kind: MidiKind::Note,
            channel,
            number,
            value,
        };
        let cc = |channel, number, value| MidiMessage {
            kind: MidiKind::Cc,
            channel,
            number,
            value,
        };
        assert_eq!(
            parse(&[
                0x90, 36, 100, // note on, channel 1
                37, 90,   // running status
                0xF8, // clock in between
                0x81, 36, 64, // note off, channel 2
                0xF0, 0x7E, 0x01, 0xF7, // sysex
                0xBF, 7, 127, // cc 7 on channel 16
                0xC0, 5, // program change: ignored
                0xB0, 1, 0xFA, 2, // real-time inside a message
            ]),
            [
                note(1, 36, 100),
                note(1, 37, 90),
                note(2, 36, 0),
                cc(16, 7, 127),
                cc(1, 1, 2)
            ]
        );
    }

    #[test]
    fn actions_from_bindings() {
        let id = lmix_types::InputId::new();
        let cc = MidiMessage {
            kind: MidiKind::Cc,
            channel: 1,
            number: 7,
            value: 127,
        };
        assert_eq!(
            action_for(&cc, false, None, true),
            Some(Action::TBar { position: 1.0 })
        );
        assert_eq!(
            action_for(&cc, true, Some(id), false),
            Some(Action::RunShortcut {
                target: InputRef::Text(id.to_string())
            })
        );
        assert_eq!(
            action_for(&cc, false, Some(id), false),
            None,
            "release does nothing"
        );
    }
}
