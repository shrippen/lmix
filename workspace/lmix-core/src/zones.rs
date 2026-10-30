//! Zones of the show and where they play on this machine (show mode
//! SND.10, S-E.4). Playing into them is the engine's (S1 step 2, after
//! S-E.3); here the names, channels and patches are kept and checked.

use lmix_types::zone::{
    Zone, ZoneOutput, ZonePatch, ZoneState, DEVICE_CHANNELS, ZONES, ZONE_CHANNELS,
    ZONE_GAIN_MAX_DB, ZONE_GAIN_MIN_DB, ZONE_OUTPUTS,
};
use lmix_types::ActionError;

fn bad(reason: impl Into<String>) -> ActionError {
    ActionError::BadZone {
        reason: reason.into(),
    }
}

/// Add `zone` to `zones` or replace the one of its name.
pub fn set_zone(zones: &mut Vec<Zone>, zone: Zone) -> Result<(), ActionError> {
    let name = zone.name.trim();
    if name.is_empty() {
        return Err(bad("a zone needs a name"));
    }
    if zone.channels == 0 || zone.channels > ZONE_CHANNELS {
        return Err(bad(format!("a zone has 1–{ZONE_CHANNELS} channels")));
    }
    let zone = Zone {
        name: name.to_string(),
        ..zone
    };
    match zones.iter().position(|z| z.name == zone.name) {
        Some(i) => zones[i] = zone,
        None if zones.len() >= ZONES => return Err(bad(format!("at most {ZONES} zones"))),
        None => zones.push(zone),
    }
    Ok(())
}

/// Add `patch` or replace the one of its zone. The zone need not be in
/// the current show: patches belong to the machine.
pub fn set_patch(patches: &mut Vec<ZonePatch>, patch: ZonePatch) -> Result<(), ActionError> {
    if patch.zone.trim().is_empty() {
        return Err(bad("a patch names its zone"));
    }
    if patch.outputs.is_empty() || patch.outputs.len() > ZONE_OUTPUTS {
        return Err(bad(format!("1–{ZONE_OUTPUTS} outputs per zone")));
    }
    for output in &patch.outputs {
        check_output(output)?;
    }
    match patches.iter().position(|p| p.zone == patch.zone) {
        Some(i) => patches[i] = patch,
        None if patches.len() >= ZONES * 4 => return Err(bad("too many zone patches")),
        None => patches.push(patch),
    }
    Ok(())
}

fn check_output(output: &ZoneOutput) -> Result<(), ActionError> {
    let count = output.channels.len();
    if count == 0 || count > usize::from(ZONE_CHANNELS) {
        return Err(bad(format!("1–{ZONE_CHANNELS} device channels")));
    }
    if output
        .channels
        .iter()
        .any(|c| *c == 0 || *c > DEVICE_CHANNELS)
    {
        return Err(bad(format!("device channels are 1–{DEVICE_CHANNELS}")));
    }
    if !(ZONE_GAIN_MIN_DB..=ZONE_GAIN_MAX_DB).contains(&output.gain_db) {
        return Err(bad(format!(
            "the level is {ZONE_GAIN_MIN_DB}–{ZONE_GAIN_MAX_DB} dB"
        )));
    }
    Ok(())
}

/// The show's zones with this machine's outputs; outputs whose channel
/// count does not fit the zone are left out (they would play wrong).
pub fn states(zones: &[Zone], patches: &[ZonePatch]) -> Vec<ZoneState> {
    zones
        .iter()
        .map(|zone| ZoneState {
            name: zone.name.clone(),
            channels: zone.channels,
            outputs: patches
                .iter()
                .find(|p| p.zone == zone.name)
                .map(|p| {
                    p.outputs
                        .iter()
                        .filter(|o| o.channels.len() == usize::from(zone.channels))
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
            gain_db: 0.0,
            mute: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(channels: &[u8]) -> ZoneOutput {
        ZoneOutput {
            device: String::new(),
            channels: channels.to_vec(),
            gain_db: -3.0,
        }
    }

    #[test]
    fn zones_and_patches_are_checked() {
        let mut zones = Vec::new();
        set_zone(
            &mut zones,
            Zone {
                name: "vorn".into(),
                channels: 2,
            },
        )
        .unwrap();
        set_zone(
            &mut zones,
            Zone {
                name: " vorn ".into(),
                channels: 1,
            },
        )
        .unwrap();
        assert_eq!(zones.len(), 1, "same name: replaced");
        assert_eq!(zones[0].channels, 1);
        assert!(set_zone(
            &mut zones,
            Zone {
                name: "x".into(),
                channels: 9
            }
        )
        .is_err());
        assert!(set_zone(
            &mut zones,
            Zone {
                name: "".into(),
                channels: 2
            }
        )
        .is_err());

        let mut patches = Vec::new();
        let patch = |outputs| ZonePatch {
            zone: "vorn".into(),
            outputs,
        };
        assert!(set_patch(&mut patches, patch(vec![])).is_err());
        assert!(set_patch(&mut patches, patch(vec![output(&[0])])).is_err());
        let mut loud = output(&[1]);
        loud.gain_db = 20.0;
        assert!(set_patch(&mut patches, patch(vec![loud])).is_err());
        set_patch(&mut patches, patch(vec![output(&[3])])).unwrap();
        assert_eq!(patches.len(), 1);
    }

    #[test]
    fn a_zone_plays_only_where_the_channels_fit() {
        let zones = vec![
            Zone {
                name: "vorn".into(),
                channels: 2,
            },
            Zone {
                name: "hinten".into(),
                channels: 1,
            },
        ];
        let patches = vec![ZonePatch {
            zone: "vorn".into(),
            outputs: vec![output(&[1, 2]), output(&[5])],
        }];

        let states = states(&zones, &patches);

        assert_eq!(
            states[0].outputs,
            vec![output(&[1, 2])],
            "the mono output does not fit"
        );
        assert!(states[1].outputs.is_empty(), "hinten plays nowhere here");
    }
}
