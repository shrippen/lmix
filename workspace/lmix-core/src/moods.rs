//! Moods (MOOD): which mood is set and what runs on each layer.
//!
//! Like the cue runner this is pure: setting a mood gives the actions
//! that change the layers, and the service runs them. Only layers that
//! change get actions; what stays the same runs on untouched (MOOD.4).
//! A layer's sound or picture plays as its own media id
//! `mood-<layer>-<n>`, a new one per change, so the old fades out while
//! the new fades in. A cue that names a layer takes it (MOOD.5, last
//! command wins): the mood fades out there, and the next change stops the
//! cue and sets the layer again, even to what it had.

use std::collections::BTreeMap;

use lmix_types::{
    Action, CueId, InputId, InputRef, LayerContent, LayerId, LayerNow, LayerOrigin, MoodFade,
    MoodId, MoodNow, Moods, StopMode,
};

/// What runs on one layer.
#[derive(Clone, Debug)]
struct Playing {
    content: LayerContent,
    /// The media id of its sound or picture.
    media: Option<String>,
    origin: LayerOrigin,
}

/// Whether `action` fades a layer's sound or picture.
fn fades(action: &Action) -> bool {
    matches!(
        action,
        Action::PlaySound { .. }
            | Action::ShowPicture { .. }
            | Action::StopSound { .. }
            | Action::StopPicture { .. }
    )
}

#[derive(Debug, Default)]
pub struct MoodRunner {
    mood: Option<MoodId>,
    previous: Option<MoodId>,
    layers: BTreeMap<LayerId, Playing>,
    /// Media ids handed out, for new ones.
    started: u64,
    fading: Option<MoodFade>,
}

impl MoodRunner {
    /// Set mood `text` (id, group name or "group / variant") at
    /// `now_ms` (Unix ms). The fade of a layer that changes: `fade_ms`,
    /// else the layer's own, else the show's change time, within one
    /// mood group its own (MOOD.1, MOOD.4).
    pub fn set(
        &mut self,
        moods: &Moods,
        text: &str,
        fade_ms: Option<u32>,
        now_ms: u64,
    ) -> Result<Vec<Action>, String> {
        let place = moods
            .lookup(text)
            .ok_or_else(|| format!("no mood {text}"))?;
        let inside = self
            .mood
            .and_then(|id| moods.find(id))
            .is_some_and(|was| was.group.id == place.group.id);
        let change = if inside {
            moods.inside_ms
        } else {
            moods.change_ms
        };
        let id = place.id();
        let mut out = Vec::new();
        let mut longest = 0;
        for layer in &moods.layers {
            let want = moods.content(place, layer.id);
            let same = self.layers.get(&layer.id).is_some_and(|p| {
                p.content == *want && matches!(p.origin, LayerOrigin::Mood { .. })
            });
            if same {
                if let Some(playing) = self.layers.get_mut(&layer.id) {
                    playing.origin = LayerOrigin::Mood { mood: id };
                }
                continue;
            }
            let fade = fade_ms.or(layer.fade_ms).unwrap_or(change);
            let was = out.len();
            self.stop(layer.id, fade, &mut out);
            self.start(
                layer.id,
                want,
                LayerOrigin::Mood { mood: id },
                fade,
                &mut out,
            );
            // Only sound and pictures fade; actions and off to off do not.
            if out[was..].iter().any(fades) {
                longest = longest.max(fade);
            }
        }
        // Layers the show no longer has go.
        let gone: Vec<LayerId> = self
            .layers
            .keys()
            .filter(|l| !moods.layers.iter().any(|x| x.id == **l))
            .copied()
            .collect();
        for layer in gone {
            let fade = fade_ms.unwrap_or(change);
            let was = out.len();
            self.stop(layer, fade, &mut out);
            if out[was..].iter().any(fades) {
                longest = longest.max(fade);
            }
        }
        if self.mood != Some(id) {
            self.previous = self.mood;
        }
        self.mood = Some(id);
        self.fading = (longest > 0).then_some(MoodFade {
            since_ms: now_ms,
            duration_ms: longest,
        });
        Ok(out)
    }

    /// Panic or hard stop (MOOD.9): every sound and picture is stopped
    /// already (the cue runner's `StopPlayback`); the panic mood's actions
    /// run, every other layer is off.
    pub fn panic(&mut self, moods: &Moods) -> Vec<Action> {
        self.forget();
        let mut out = Vec::new();
        for layer in &moods.layers {
            if let Some(content @ LayerContent::Actions { .. }) = moods.panic.get(&layer.id) {
                self.start(layer.id, content, LayerOrigin::Panic, 0, &mut out);
            }
        }
        out
    }

    /// Cue `cue` of `list` fires and names `layer` (MOOD.5): what the
    /// mood has there fades out (the layer's fade, else the change time),
    /// a cue that had the layer stops; the layer is the cue's. A layer the
    /// show does not have, or the panic mood's (an emergency cue fires
    /// before it, MOOD.9): nothing.
    pub fn take(
        &mut self,
        moods: &Moods,
        layer: LayerId,
        list: InputId,
        cue: CueId,
    ) -> Vec<Action> {
        let Some(own) = moods.layers.iter().find(|l| l.id == layer) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let taken = LayerOrigin::Cue { list, cue };
        if self
            .layers
            .get(&layer)
            .is_some_and(|p| p.origin == taken || p.origin == LayerOrigin::Panic)
        {
            return out;
        }
        self.stop(layer, own.fade_ms.unwrap_or(moods.change_ms), &mut out);
        self.layers.insert(
            layer,
            Playing {
                content: LayerContent::Off,
                media: None,
                origin: taken,
            },
        );
        out
    }

    /// The sound or picture `id` did not start (a file missing, a zone
    /// gone): its layer is off, so the same mood again tries again.
    pub fn media_failed(&mut self, id: &str) {
        self.layers.retain(|_, p| p.media.as_deref() != Some(id));
    }

    /// Everything stopped (reset, a new show): no layer runs, no mood is
    /// set.
    pub fn forget(&mut self) {
        if self.mood.is_some() {
            self.previous = self.mood;
        }
        self.mood = None;
        self.layers.clear();
        self.fading = None;
    }

    /// For the state: layers in the show's order, then any it no longer
    /// has.
    pub fn now(&self, moods: &Moods, now_ms: u64) -> MoodNow {
        let order = |l: &LayerId| {
            moods
                .layers
                .iter()
                .position(|x| x.id == *l)
                .unwrap_or(usize::MAX)
        };
        let mut layers: Vec<LayerNow> = self
            .layers
            .iter()
            .map(|(layer, p)| LayerNow {
                layer: *layer,
                origin: p.origin.clone(),
            })
            .collect();
        layers.sort_by_key(|l| order(&l.layer));
        MoodNow {
            mood: self.mood,
            previous: self.previous,
            layers,
            fading: self
                .fading
                .filter(|f| now_ms < f.since_ms + u64::from(f.duration_ms)),
        }
    }

    fn stop(&mut self, layer: LayerId, fade_ms: u32, out: &mut Vec<Action>) {
        let Some(playing) = self.layers.remove(&layer) else {
            return;
        };
        // A cue's: it stops with its own fade.
        if let LayerOrigin::Cue { list, cue } = playing.origin {
            out.push(Action::CueStop {
                list: Some(InputRef::Text(list.to_string())),
                cue: Some(cue.to_string()),
                mode: StopMode::Fade,
            });
            return;
        }
        let Some(id) = playing.media else {
            return;
        };
        out.push(match playing.content {
            LayerContent::Picture { .. } => Action::StopPicture {
                id: Some(id),
                fade_ms,
            },
            _ => Action::StopSound {
                id: Some(id),
                fade_ms,
            },
        });
    }

    fn start(
        &mut self,
        layer: LayerId,
        content: &LayerContent,
        origin: LayerOrigin,
        fade_ms: u32,
        out: &mut Vec<Action>,
    ) {
        let media = match content {
            LayerContent::Off => return,
            LayerContent::Actions { actions } => {
                out.extend(actions.iter().cloned());
                None
            }
            LayerContent::Sound { sound } => {
                let id = self.media_id(layer);
                let mut sound = sound.clone();
                sound.fade_in_ms = fade_ms;
                out.push(Action::PlaySound {
                    id: id.clone(),
                    sound,
                });
                Some(id)
            }
            LayerContent::Picture { picture } => {
                let id = self.media_id(layer);
                let mut picture = picture.clone();
                picture.fade_in_ms = fade_ms;
                out.push(Action::ShowPicture {
                    id: id.clone(),
                    picture,
                });
                Some(id)
            }
        };
        self.layers.insert(
            layer,
            Playing {
                content: content.clone(),
                media,
                origin,
            },
        );
    }

    fn media_id(&mut self, layer: LayerId) -> String {
        self.started += 1;
        format!("mood-{layer}-{}", self.started)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lmix_types::{MoodGroup, MoodLayer, MoodVariant, SoundCue};

    struct Tavern {
        moods: Moods,
        light: LayerId,
        music: LayerId,
        picture: LayerId,
    }

    fn http(what: &str) -> Action {
        Action::HttpRequest {
            target: "Lights".into(),
            method: Default::default(),
            path: format!("/scene/{what}"),
            headers: Default::default(),
            body: None,
        }
    }

    fn actions(what: &str) -> LayerContent {
        LayerContent::Actions {
            actions: vec![http(what)],
        }
    }

    fn sound(file: &str) -> LayerContent {
        LayerContent::Sound {
            sound: SoundCue {
                file: format!("/sounds/{file}.wav"),
                ..SoundCue::default()
            },
        }
    }

    fn picture(file: &str) -> LayerContent {
        LayerContent::Picture {
            picture: serde_json::from_value(serde_json::json!({
                "file": format!("/pictures/{file}.png"),
                "stage": "Back",
            }))
            .expect("a picture"),
        }
    }

    /// Tavern (warm light, lute, the room) with Night (dim light) and
    /// Fight (drums); Forest (green light, birds) without a picture.
    fn tavern() -> Tavern {
        let layer = |name: &str| MoodLayer {
            id: LayerId::new(),
            name: name.into(),
            fade_ms: None,
        };
        let (light, music, pict) = (layer("Light"), layer("Music"), layer("Picture"));
        let (l, m, p) = (light.id, music.id, pict.id);
        let variant = |name: &str, layers: BTreeMap<LayerId, LayerContent>| MoodVariant {
            id: MoodId::new(),
            name: name.into(),
            layers,
        };
        let tavern = MoodGroup {
            id: MoodId::new(),
            name: "Tavern".into(),
            color: None,
            base: [
                (l, actions("warm")),
                (m, sound("lute")),
                (p, picture("room")),
            ]
            .into(),
            variants: vec![
                variant("Night", [(l, actions("dim"))].into()),
                variant("Fight", [(m, sound("drums"))].into()),
            ],
        };
        let forest = MoodGroup {
            id: MoodId::new(),
            name: "Forest".into(),
            color: None,
            base: [(l, actions("green")), (m, sound("birds"))].into(),
            variants: Vec::new(),
        };
        Tavern {
            moods: Moods {
                layers: vec![light, music, pict],
                groups: vec![tavern, forest],
                panic: [(l, actions("house")), (m, sound("never"))].into(),
                ..Moods::default()
            },
            light: l,
            music: m,
            picture: p,
        }
    }

    #[test]
    fn a_cue_takes_a_layer_until_the_next_change() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        runner.set(&t.moods, "Tavern", None, 0).expect("set");
        let (list, cue) = (InputId::new(), CueId::new());

        // The lute fades out over the change time; the cue has the layer.
        let out = runner.take(&t.moods, t.music, list, cue);
        assert!(
            matches!(&out[..], [Action::StopSound { fade_ms: 3000, .. }]),
            "{out:?}"
        );
        let now = runner.now(&t.moods, 0);
        assert_eq!(now.layers[1].origin, LayerOrigin::Cue { list, cue });
        assert!(
            runner.take(&t.moods, t.music, list, cue).is_empty(),
            "taken already"
        );

        // The same mood again: the cue stops and the lute comes back,
        // though the mood has not changed; the light stays untouched.
        let out = runner.set(&t.moods, "Tavern", None, 0).expect("set");
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(
            matches!(&out[0], Action::CueStop { cue: Some(c), mode: StopMode::Fade, .. }
            if *c == cue.to_string())
        );
        assert!(
            matches!(&out[1], Action::PlaySound { sound, .. } if sound.file == "/sounds/lute.wav")
        );
    }

    #[test]
    fn the_last_cue_on_a_layer_wins() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        let list = InputId::new();
        let (first, second) = (CueId::new(), CueId::new());
        assert!(
            runner.take(&t.moods, t.light, list, first).is_empty(),
            "nothing ran there"
        );
        let out = runner.take(&t.moods, t.light, list, second);
        assert!(
            matches!(&out[..], [Action::CueStop { cue: Some(c), .. }] if *c == first.to_string())
        );
        assert!(
            runner
                .take(&t.moods, LayerId::new(), list, first)
                .is_empty(),
            "no such layer"
        );
        assert_eq!(runner.now(&t.moods, 0).layers.len(), 1);
        runner.panic(&t.moods);
        assert!(
            runner.take(&t.moods, t.light, list, second).is_empty(),
            "the panic mood keeps its layer"
        );
        assert_eq!(runner.now(&t.moods, 0).layers[0].origin, LayerOrigin::Panic);
    }

    #[test]
    fn the_first_mood_starts_every_layer_it_has() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        let out = runner.set(&t.moods, "Tavern", None, 0).expect("set");
        assert_eq!(out.len(), 3, "{out:?}");
        assert_eq!(out[0], http("warm"));
        assert!(matches!(&out[1], Action::PlaySound { sound, .. }
            if sound.file == "/sounds/lute.wav" && sound.fade_in_ms == 3000));
        assert!(matches!(&out[2], Action::ShowPicture { picture, .. }
            if picture.file == "/pictures/room.png"));
        let now = runner.now(&t.moods, 0);
        assert_eq!(now.layers.len(), 3);
        assert_eq!(now.layers[0].layer, t.light, "in the show's order");
    }

    #[test]
    fn a_variant_changes_only_its_layers_and_fades_inside_the_group() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        runner.set(&t.moods, "Tavern", None, 0).expect("set");
        let out = runner
            .set(&t.moods, "Tavern / Fight", None, 10_000)
            .expect("variant");
        // The lute fades out, the drums in, over the group's time; light
        // and picture run on.
        assert_eq!(out.len(), 2, "{out:?}");
        let Action::StopSound {
            id: Some(lute),
            fade_ms: 1000,
        } = &out[0]
        else {
            panic!("stops the lute: {out:?}")
        };
        let Action::PlaySound { id: drums, sound } = &out[1] else {
            panic!("plays the drums: {out:?}")
        };
        assert_ne!(lute, drums, "a new media id crossfades");
        assert_eq!(sound.fade_in_ms, 1000);
        let fight = t.moods.lookup("Tavern / Fight").expect("fight").id();
        let now = runner.now(&t.moods, 10_500);
        assert_eq!(now.mood, Some(fight));
        assert_eq!(now.fading.map(|f| f.duration_ms), Some(1000));
        assert!(
            now.layers
                .iter()
                .all(|l| l.origin == LayerOrigin::Mood { mood: fight }),
            "a layer that ran on belongs to the new mood"
        );
        assert_eq!(runner.now(&t.moods, 11_000).fading, None, "done");
    }

    #[test]
    fn the_same_mood_again_does_nothing() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        runner.set(&t.moods, "Tavern", None, 0).expect("set");
        assert!(runner
            .set(&t.moods, "Tavern", None, 0)
            .expect("again")
            .is_empty());
        let night = runner
            .set(&t.moods, "Tavern / Night", None, 0)
            .expect("night");
        assert_eq!(
            night,
            vec![http("dim")],
            "actions run once a layer takes them"
        );
        let back = runner.set(&t.moods, "tavern", None, 0).expect("base");
        assert_eq!(back, vec![http("warm")]);
        let now = runner.now(&t.moods, 0);
        assert_eq!(
            now.previous,
            t.moods.lookup("Tavern / Night").map(|p| p.id())
        );
    }

    #[test]
    fn another_group_takes_the_change_time_and_turns_off_what_it_lacks() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        runner.set(&t.moods, "Tavern", None, 0).expect("set");
        let out = runner.set(&t.moods, "Forest", None, 0).expect("forest");
        assert!(out.contains(&http("green")));
        assert!(
            out.iter().any(|a| matches!(
                a,
                Action::StopPicture {
                    id: Some(_),
                    fade_ms: 3000
                }
            )),
            "{out:?}"
        );
        assert!(out.iter().any(|a| matches!(a,
            Action::PlaySound { sound, .. } if sound.file == "/sounds/birds.wav")));
        let now = runner.now(&t.moods, 0);
        assert!(
            !now.layers.iter().any(|l| l.layer == t.picture),
            "off is not listed"
        );
    }

    #[test]
    fn a_call_and_a_layer_bring_their_own_fade() {
        let mut t = tavern();
        t.moods.layers[1].fade_ms = Some(8000);
        let mut runner = MoodRunner::default();
        let out = runner.set(&t.moods, "Tavern", None, 0).expect("set");
        assert!(
            out.iter().any(|a| matches!(a,
            Action::PlaySound { sound, .. } if sound.fade_in_ms == 8000)),
            "the layer's own"
        );
        let out = runner.set(&t.moods, "Forest", Some(0), 0).expect("cut");
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::StopSound { fade_ms: 0, .. })),
            "the call's"
        );
        assert_eq!(runner.now(&t.moods, 0).fading, None, "a cut does not fade");
    }

    #[test]
    fn panic_runs_only_the_actions_of_the_panic_mood() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        runner.set(&t.moods, "Tavern", None, 0).expect("set");
        let out = runner.panic(&t.moods);
        assert_eq!(
            out,
            vec![http("house")],
            "the sound in the panic mood never plays"
        );
        let now = runner.now(&t.moods, 0);
        assert_eq!(now.mood, None);
        assert_eq!(
            now.layers,
            vec![LayerNow {
                layer: t.light,
                origin: LayerOrigin::Panic
            }]
        );
        // After panic every layer of the next mood starts again, the
        // light too, as panic took it.
        let out = runner.set(&t.moods, "Tavern", None, 0).expect("after");
        assert_eq!(out.len(), 3, "{out:?}");
        assert_eq!(out[0], http("warm"));
    }

    #[test]
    fn only_sound_and_pictures_make_a_fade() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        runner.set(&t.moods, "Forest", None, 0).expect("forest");
        runner.set(&t.moods, "Forest", None, 0).expect("again");
        assert_eq!(runner.now(&t.moods, 0).fading, None, "nothing changed");
        runner.set(&t.moods, "Tavern", None, 0).expect("tavern");
        runner
            .set(&t.moods, "Tavern / Night", None, 0)
            .expect("night");
        assert_eq!(
            runner.now(&t.moods, 0).fading,
            None,
            "only the light changed, over HTTP"
        );
    }

    #[test]
    fn a_layer_whose_sound_did_not_start_tries_again() {
        let t = tavern();
        let mut runner = MoodRunner::default();
        let out = runner.set(&t.moods, "Tavern", None, 0).expect("set");
        let Some(Action::PlaySound { id, .. }) = out.get(1) else {
            panic!("plays the lute: {out:?}")
        };
        runner.media_failed(id);
        assert!(!runner
            .now(&t.moods, 0)
            .layers
            .iter()
            .any(|l| l.layer == t.music));
        let again = runner.set(&t.moods, "Tavern", None, 0).expect("again");
        assert!(
            matches!(again.as_slice(), [Action::PlaySound { .. }]),
            "{again:?}"
        );
    }

    #[test]
    fn without_moods_panic_does_nothing() {
        let mut runner = MoodRunner::default();
        assert!(runner.panic(&Moods::default()).is_empty());
        assert!(runner.set(&Moods::default(), "Tavern", None, 0).is_err());
    }

    #[test]
    fn a_layer_the_show_dropped_stops_with_the_next_change() {
        let mut t = tavern();
        let mut runner = MoodRunner::default();
        runner.set(&t.moods, "Tavern", None, 0).expect("set");
        t.moods.layers.retain(|l| l.id != t.music);
        let out = runner
            .set(&t.moods, "Tavern / Night", None, 0)
            .expect("night");
        assert!(
            out.iter()
                .any(|a| matches!(a, Action::StopSound { id: Some(_), .. })),
            "{out:?}"
        );
        assert!(!runner
            .now(&t.moods, 0)
            .layers
            .iter()
            .any(|l| l.layer == t.music));
    }
}
