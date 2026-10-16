//! PTZ over VISCA (PER.2): the messages for lmix's PTZ actions, framed
//! for Sony's VISCA over IP (UDP, 8-byte header) or as plain VISCA in UDP.
//! Camera address 1 (`0x81`), as cameras on IP use.

use lmix_types::{Action, ActionError, PtzFocus, PtzProtocol};

/// Fastest pan speed VISCA knows (`0x18`), and tilt (`0x14`).
const PAN_MAX: f32 = 24.0;
const TILT_MAX: f32 = 20.0;
/// Zoom and focus speeds `0`–`7`.
const VARIABLE_MAX: f32 = 7.0;

fn bad(reason: impl Into<String>) -> ActionError {
    ActionError::BadPtz {
        reason: reason.into(),
    }
}

/// A speed in −1…1.
fn check(value: f32, what: &str) -> Result<f32, ActionError> {
    if value.is_finite() && (-1.0..=1.0).contains(&value) {
        Ok(value)
    } else {
        Err(bad(format!("{what} is outside −1…1")))
    }
}

/// Speed byte for `|value|` of `max`, at least 1.
fn speed(value: f32, max: f32) -> u8 {
    (value.abs() * max).round().clamp(1.0, max) as u8
}

/// The VISCA message for a PTZ action; `None` for other actions.
pub fn message(action: &Action) -> Option<Result<Vec<u8>, ActionError>> {
    let built = match action {
        Action::PtzMove { pan, tilt, .. } => (|| {
            let (pan, tilt) = (check(*pan, "pan")?, check(*tilt, "tilt")?);
            let pan_dir = if pan < 0.0 {
                1
            } else if pan > 0.0 {
                2
            } else {
                3
            };
            let tilt_dir = if tilt > 0.0 {
                1
            } else if tilt < 0.0 {
                2
            } else {
                3
            };
            Ok(vec![
                0x81,
                0x01,
                0x06,
                0x01,
                speed(pan, PAN_MAX),
                speed(tilt, TILT_MAX),
                pan_dir,
                tilt_dir,
                0xFF,
            ])
        })(),
        Action::PtzZoom { speed: s, .. } => check(*s, "zoom speed").map(|s| {
            let p = (s.abs() * VARIABLE_MAX).round() as u8;
            let code = if s > 0.0 {
                0x20 | p
            } else if s < 0.0 {
                0x30 | p
            } else {
                0x00
            };
            vec![0x81, 0x01, 0x04, 0x07, code, 0xFF]
        }),
        Action::PtzFocus { focus, .. } => Ok(match focus {
            PtzFocus::Auto => vec![0x81, 0x01, 0x04, 0x38, 0x02, 0xFF],
            PtzFocus::Manual => vec![0x81, 0x01, 0x04, 0x38, 0x03, 0xFF],
            // Far and near at a middle speed.
            PtzFocus::Far => vec![0x81, 0x01, 0x04, 0x08, 0x23, 0xFF],
            PtzFocus::Near => vec![0x81, 0x01, 0x04, 0x08, 0x33, 0xFF],
            PtzFocus::Stop => vec![0x81, 0x01, 0x04, 0x08, 0x00, 0xFF],
        }),
        Action::PtzHome { .. } => Ok(vec![0x81, 0x01, 0x06, 0x04, 0xFF]),
        Action::PtzPreset { preset, store, .. } => {
            if *preset >= lmix_types::ptz::PTZ_PRESETS {
                Err(bad(format!("preset {preset} is outside 0–127")))
            } else {
                Ok(vec![
                    0x81,
                    0x01,
                    0x04,
                    0x3F,
                    if *store { 0x01 } else { 0x02 },
                    *preset,
                    0xFF,
                ])
            }
        }
        _ => return None,
    };
    Some(built)
}

/// `message` as the protocol sends it; `seq` numbers VISCA-over-IP
/// packets.
pub fn frame(protocol: PtzProtocol, seq: u32, message: &[u8]) -> Vec<u8> {
    match protocol {
        PtzProtocol::ViscaUdp => message.to_vec(),
        PtzProtocol::ViscaIp => {
            // Payload type 0x0100 (VISCA command), length, sequence number.
            let mut packet = vec![0x01, 0x00];
            packet.extend((message.len() as u16).to_be_bytes());
            packet.extend(seq.to_be_bytes());
            packet.extend(message);
            packet
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::InputRef;

    fn msg(action: Action) -> Vec<u8> {
        message(&action).unwrap().unwrap()
    }

    fn input() -> InputRef {
        InputRef::Number(1)
    }

    #[test]
    fn pan_tilt_drive() {
        let mv = |pan, tilt| {
            msg(Action::PtzMove {
                input: input(),
                pan,
                tilt,
            })
        };

        // Right at full speed, down slowly.
        assert_eq!(
            mv(1.0, -0.1),
            [0x81, 0x01, 0x06, 0x01, 0x18, 0x02, 0x02, 0x02, 0xFF]
        );
        // Left, up.
        assert_eq!(mv(-0.5, 0.5)[6..8], [0x01, 0x01]);
        // Stop: both directions 03.
        assert_eq!(mv(0.0, 0.0)[6..8], [0x03, 0x03]);
        assert!(message(&Action::PtzMove {
            input: input(),
            pan: 2.0,
            tilt: 0.0
        })
        .unwrap()
        .is_err());
        assert!(message(&Action::PtzMove {
            input: input(),
            pan: f32::NAN,
            tilt: 0.0
        })
        .unwrap()
        .is_err());
    }

    #[test]
    fn zoom_focus_home_preset() {
        assert_eq!(
            msg(Action::PtzZoom {
                input: input(),
                speed: 1.0
            }),
            [0x81, 0x01, 0x04, 0x07, 0x27, 0xFF]
        );
        assert_eq!(
            msg(Action::PtzZoom {
                input: input(),
                speed: -0.5
            })[4],
            0x34
        );
        assert_eq!(
            msg(Action::PtzZoom {
                input: input(),
                speed: 0.0
            })[4],
            0x00
        );
        assert_eq!(
            msg(Action::PtzFocus {
                input: input(),
                focus: PtzFocus::Auto
            }),
            [0x81, 0x01, 0x04, 0x38, 0x02, 0xFF]
        );
        assert_eq!(
            msg(Action::PtzHome { input: input() }),
            [0x81, 0x01, 0x06, 0x04, 0xFF]
        );
        assert_eq!(
            msg(Action::PtzPreset {
                input: input(),
                preset: 5,
                store: false
            }),
            [0x81, 0x01, 0x04, 0x3F, 0x02, 0x05, 0xFF]
        );
        assert_eq!(
            msg(Action::PtzPreset {
                input: input(),
                preset: 5,
                store: true
            })[4],
            0x01
        );
        assert!(message(&Action::PtzPreset {
            input: input(),
            preset: 200,
            store: false
        })
        .unwrap()
        .is_err());
        assert!(message(&Action::Cut).is_none());
    }

    #[test]
    fn visca_over_ip_header() {
        let home = [0x81, 0x01, 0x06, 0x04, 0xFF];

        assert_eq!(
            frame(PtzProtocol::ViscaIp, 7, &home),
            [0x01, 0x00, 0x00, 0x05, 0x00, 0x00, 0x00, 0x07, 0x81, 0x01, 0x06, 0x04, 0xFF]
        );
        assert_eq!(frame(PtzProtocol::ViscaUdp, 7, &home), home);
    }
}
