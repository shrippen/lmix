//! Sound and picture cues on the engine (show mode SND, VIS): what a cue
//! plays is checked here, and zone and stage names become the engine's
//! numbers.
//!
//! ```text
//!   show zones [Hall (2), Foyer (1)]
//!   route ch 1 → Foyer ch 1   ──►   engine: channel 0 → zone 1, channel 0
//! ```

use lmix_types::cue::{Cue, CueKind};
use lmix_types::sound::{
    AfterFade, FadeCue, FadeTarget, SoundCue, SOUND_FADE_MAX_MS, SOUND_FILE_CHANNELS,
    SOUND_GAIN_MAX_DB, SOUND_GAIN_MIN_DB, SOUND_ROUTES,
};
use lmix_types::stage::{PictureCue, PictureEnd, PictureFit, Stage, PICTURE_LAYER_MAX};
use lmix_types::zone::{Zone, ZONE_CHANNELS};
use lmix_types::ActionError;

/// A sound as the engine plays it.
#[derive(Clone, Debug, PartialEq)]
pub struct EngineSound {
    /// `file://` URI.
    pub uri: String,
    pub gain_db: f32,
    /// Empty: the engine's default (file channels to the first zone).
    pub routes: Vec<EngineRoute>,
    pub fade_in_ms: u32,
}

/// A crosspoint with 0-based channels and the zone by its place in the
/// show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EngineRoute {
    pub channel: u32,
    pub zone: usize,
    pub zone_channel: u32,
    pub gain_db: f32,
}

fn bad_stage(reason: impl Into<String>) -> ActionError {
    ActionError::BadStage {
        reason: reason.into(),
    }
}

fn bad(reason: impl Into<String>) -> ActionError {
    ActionError::BadSound {
        reason: reason.into(),
    }
}

/// A cue, fade or zone level within the show's bounds (SND.13).
pub fn check_level(db: f32) -> Result<(), ActionError> {
    check_gain(db, "a level")
}

fn check_gain(db: f32, what: &str) -> Result<(), ActionError> {
    if !(SOUND_GAIN_MIN_DB..=SOUND_GAIN_MAX_DB).contains(&db) {
        return Err(bad(format!(
            "{what} is {SOUND_GAIN_MIN_DB}–{SOUND_GAIN_MAX_DB} dB"
        )));
    }
    Ok(())
}

pub fn check_fade_time(ms: u32) -> Result<(), ActionError> {
    if ms > SOUND_FADE_MAX_MS {
        return Err(bad(format!("fades are at most {SOUND_FADE_MAX_MS} ms")));
    }
    Ok(())
}

/// What a sound cue plays, without the show's zones (those may come
/// later): file, levels, channels and times.
pub fn check_sound(sound: &SoundCue) -> Result<(), ActionError> {
    if !std::path::Path::new(&sound.file).is_absolute() {
        return Err(bad(format!(
            "{:?}: the file needs an absolute path",
            sound.file
        )));
    }
    check_gain(sound.gain_db, "the level")?;
    check_fade_time(sound.fade_in_ms)?;
    check_fade_time(sound.fade_out_ms)?;
    if sound.routes.len() > SOUND_ROUTES {
        return Err(bad(format!("at most {SOUND_ROUTES} routes")));
    }
    for route in &sound.routes {
        if !(1..=SOUND_FILE_CHANNELS).contains(&route.channel) {
            return Err(bad(format!("file channels are 1–{SOUND_FILE_CHANNELS}")));
        }
        if !(1..=ZONE_CHANNELS).contains(&route.zone_channel) {
            return Err(bad(format!("zone channels are 1–{ZONE_CHANNELS}")));
        }
        check_gain(route.gain_db, "a crosspoint")?;
    }
    Ok(())
}

/// A fade sets something its target has: a level for a zone, a
/// brightness for a stage, a level or an opacity for a cue (which of them
/// counts is known once the cue is found).
pub fn check_fade(fade: &FadeCue) -> Result<(), ActionError> {
    if let Some(db) = fade.gain_db {
        check_gain(db, "the fade's level")?;
    }
    for share in [fade.opacity, fade.brightness].into_iter().flatten() {
        check_share(share)?;
    }
    let sets = |what: bool, name: &str| {
        if what {
            Ok(())
        } else {
            Err(bad(format!("a fade of a {name} sets its value")))
        }
    };
    match (&fade.target, fade.then) {
        (FadeTarget::Cue(cue), _) if cue.trim().is_empty() => Err(bad("a fade names its cue")),
        (FadeTarget::Zone(_) | FadeTarget::Stage(_), AfterFade::Stop) => {
            Err(bad("only a cue stops after its fade"))
        }
        (FadeTarget::Cue(_), _) => sets(fade.gain_db.is_some() || fade.opacity.is_some(), "cue"),
        (FadeTarget::Zone(_), _) => sets(fade.gain_db.is_some(), "zone (gain_db)"),
        (FadeTarget::Stage(_), _) => sets(fade.brightness.is_some(), "stage (brightness)"),
    }
}

/// An opacity or brightness: 0–1.
pub fn check_share(value: f32) -> Result<(), ActionError> {
    if !(0.0..=1.0).contains(&value) {
        return Err(bad("opacity and brightness are 0–1"));
    }
    Ok(())
}

/// What a picture cue shows, without the show's stages.
pub fn check_picture(picture: &PictureCue) -> Result<(), ActionError> {
    if !std::path::Path::new(&picture.file).is_absolute() {
        return Err(bad_stage(format!(
            "{:?}: the file needs an absolute path",
            picture.file
        )));
    }
    if picture.stage.trim().is_empty() {
        return Err(bad_stage("a picture names its stage"));
    }
    if picture.layer > PICTURE_LAYER_MAX {
        return Err(bad_stage(format!("layers are 0–{PICTURE_LAYER_MAX}")));
    }
    check_share(picture.opacity)?;
    check_fade_time(picture.fade_in_ms)?;
    check_fade_time(picture.fade_out_ms)
}

/// A picture as the engine shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct EnginePicture {
    pub uri: String,
    /// Place of its stage in the show.
    pub stage: usize,
    pub layer: u16,
    pub opacity: f32,
    pub fit: PictureFit,
    pub end: PictureEnd,
    pub fade_in_ms: u32,
}

/// `picture` as the engine shows it on the show's `stages`.
pub fn engine_picture(
    picture: &PictureCue,
    stages: &[Stage],
) -> Result<EnginePicture, ActionError> {
    check_picture(picture)?;
    Ok(EnginePicture {
        uri: file_uri(&picture.file),
        stage: stage_index(stages, &picture.stage)?,
        layer: picture.layer,
        opacity: picture.opacity,
        fit: picture.fit,
        end: picture.end,
        fade_in_ms: picture.fade_in_ms,
    })
}

/// Place of stage `name` in the show.
pub fn stage_index(stages: &[Stage], name: &str) -> Result<usize, ActionError> {
    stages
        .iter()
        .position(|s| s.name == name)
        .ok_or_else(|| bad_stage(format!("no stage {name}")))
}

/// The fields a sound or fade cue needs.
pub fn check_cue(cue: &Cue) -> Result<(), ActionError> {
    match cue.kind {
        CueKind::Sound => check_sound(cue.sound.as_ref().ok_or_else(|| bad("no sound"))?),
        CueKind::Picture => check_picture(cue.picture.as_ref().ok_or_else(|| bad("no picture"))?),
        CueKind::Fade => {
            check_fade(cue.fade.as_ref().ok_or_else(|| bad("no fade"))?)?;
            match cue.duration_ms {
                Some(ms) => check_fade_time(ms),
                None => Err(bad("a fade needs duration_ms")),
            }
        }
        _ => Ok(()),
    }
}

/// Place of zone `name` in the show.
pub fn zone_index(zones: &[Zone], name: &str) -> Result<usize, ActionError> {
    zones
        .iter()
        .position(|z| z.name == name)
        .ok_or_else(|| bad(format!("no zone {name}")))
}

/// `sound` as the engine plays it in the show's `zones`.
pub fn engine_sound(sound: &SoundCue, zones: &[Zone]) -> Result<EngineSound, ActionError> {
    check_sound(sound)?;
    if zones.is_empty() {
        return Err(bad("the show has no zones"));
    }
    let routes = sound
        .routes
        .iter()
        .map(|route| {
            let zone = zone_index(zones, &route.zone)?;
            if route.zone_channel > zones[zone].channels {
                return Err(bad(format!(
                    "zone {} has {} channel(s)",
                    route.zone, zones[zone].channels
                )));
            }
            Ok(EngineRoute {
                channel: u32::from(route.channel - 1),
                zone,
                zone_channel: u32::from(route.zone_channel - 1),
                gain_db: route.gain_db,
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(EngineSound {
        uri: file_uri(&sound.file),
        gain_db: sound.gain_db,
        routes,
        fade_in_ms: sound.fade_in_ms,
    })
}

/// `file://` URI of an absolute path, unsafe bytes percent-encoded.
fn file_uri(path: &str) -> String {
    let mut uri = String::from("file://");
    for byte in path.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                uri.push(byte as char)
            }
            _ => uri.push_str(&format!("%{byte:02X}")),
        }
    }
    uri
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::sound::SoundRoute;

    fn zones() -> Vec<Zone> {
        vec![
            Zone {
                name: "Hall".into(),
                channels: 2,
            },
            Zone {
                name: "Foyer".into(),
                channels: 1,
            },
        ]
    }

    fn rain(routes: Vec<SoundRoute>) -> SoundCue {
        SoundCue {
            file: "/media/fx/heavy rain.flac".into(),
            gain_db: -6.0,
            routes,
            ..Default::default()
        }
    }

    fn route(channel: u8, zone: &str, zone_channel: u8) -> SoundRoute {
        SoundRoute {
            channel,
            zone: zone.into(),
            zone_channel,
            gain_db: 0.0,
        }
    }

    #[test]
    fn zones_become_numbers_and_channels_count_from_zero() {
        let sound = engine_sound(&rain(vec![route(2, "Foyer", 1)]), &zones()).unwrap();

        assert_eq!(sound.uri, "file:///media/fx/heavy%20rain.flac");
        assert_eq!(
            sound.routes,
            vec![EngineRoute {
                channel: 1,
                zone: 1,
                zone_channel: 0,
                gain_db: 0.0
            }]
        );
    }

    #[test]
    fn a_sound_must_fit_the_show() {
        assert!(engine_sound(&rain(vec![]), &zones()).is_ok());
        assert!(engine_sound(&rain(vec![]), &[]).is_err(), "no zones");
        assert!(engine_sound(&rain(vec![route(1, "Stage", 1)]), &zones()).is_err());
        assert!(engine_sound(&rain(vec![route(1, "Foyer", 2)]), &zones()).is_err());
        assert!(check_sound(&rain(vec![route(0, "Hall", 1)])).is_err());
        let relative = SoundCue {
            file: "rain.flac".into(),
            ..Default::default()
        };
        assert!(check_sound(&relative).is_err());
        let loud = SoundCue {
            gain_db: 20.0,
            ..rain(vec![])
        };
        assert!(check_sound(&loud).is_err());
    }

    #[test]
    fn only_cues_stop_after_a_fade() {
        let fade = |target, then| FadeCue {
            target,
            gain_db: Some(SOUND_GAIN_MIN_DB),
            opacity: None,
            brightness: None,
            then,
        };
        assert!(check_fade(&fade(FadeTarget::Cue("12".into()), AfterFade::Stop)).is_ok());
        assert!(check_fade(&fade(FadeTarget::Zone("Hall".into()), AfterFade::Stop)).is_err());
        assert!(check_fade(&fade(FadeTarget::Cue(" ".into()), AfterFade::Keep)).is_err());
        assert!(
            check_fade(&fade(FadeTarget::Stage("Back".into()), AfterFade::Keep)).is_err(),
            "a stage fades its brightness, not a level"
        );
        let dim = FadeCue {
            brightness: Some(0.0),
            gain_db: None,
            ..fade(FadeTarget::Stage("Back".into()), AfterFade::Keep)
        };
        assert!(check_fade(&dim).is_ok());
        assert!(check_fade(&FadeCue {
            brightness: Some(1.5),
            ..dim
        })
        .is_err());
    }
}
