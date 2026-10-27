//! Sounds and pictures on strom's Show Playback block (show mode SND,
//! VIS): the playback flow, and play, fade, stop, zone levels and stage
//! brightness over its REST API.
//!
//! strom does not report the end of a sound or picture by itself, so
//! while any plays a poller reads the block's state and reports those
//! that are gone ([`Engine::playback_ended`](crate::engine::Engine::playback_ended)).
//!
//! What the switcher asks to hold ready (`SetPreload`) is added to the
//! block decoded and waiting; playing it then only lets it go.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use strom_types::discovery::DeviceResponse;
use strom_types::show_playback::{
    AddPictureRequest, AddSoundRequest, FadeEnd, PictureEnd, PictureFadeRequest, PictureFit,
    ShowPlaybackState, SoundFadeRequest, SoundInfo, SoundRepeat, SoundRoute, SoundStart,
    SoundState, SoundStopRequest, StageLevelRequest, ZoneLevelRequest, GAIN_MIN_DB,
};
use tokio::sync::broadcast;

use lmix_types::sound::{self, AfterFade};
use lmix_types::stage;

use super::{check, net, StromEngine};
use crate::engine::flows::encode_path_segment;
use crate::engine::playback_flow::{
    playback_flow, playback_flow_id, DeviceChannels, PLAYBACK_BLOCK,
};
use crate::engine::EngineError;
use crate::playback::{EnginePicture, EngineSound};
use crate::switcher::{Playback, Preload};

/// How often the poller reads which sounds and pictures still play.
const PLAYBACK_POLL: Duration = Duration::from_millis(100);

/// Sounds and pictures the engine plays, and who hears that they ended.
/// Their ids are cue ids, so one set holds both.
pub(super) struct Playing {
    ids: Arc<Mutex<HashSet<String>>>,
    /// Held ready in the block, not started yet.
    ready: Mutex<HashMap<String, Ready>>,
    ended: broadcast::Sender<String>,
    polling: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for Playing {
    fn default() -> Self {
        Self {
            ids: Arc::default(),
            ready: Mutex::default(),
            ended: broadcast::channel(256).0,
            polling: Arc::default(),
        }
    }
}

impl Playing {
    pub(super) fn subscribe(&self) -> broadcast::Receiver<String> {
        self.ended.subscribe()
    }

    /// The ids of `items` that are not held ready.
    fn not_ready(&self, items: Vec<SoundInfo>) -> Vec<String> {
        let ready = self.ready.lock().expect("ready lock");
        items
            .into_iter()
            .map(|item| item.id)
            .filter(|id| !ready.contains_key(id))
            .collect()
    }

    /// Take `id` out of what is held ready, if it is held as `what`.
    fn take_ready(&self, id: &str, what: &Ready) -> bool {
        let mut ready = self.ready.lock().expect("ready lock");
        if ready.get(id) == Some(what) {
            ready.remove(id);
            true
        } else {
            false
        }
    }

    /// Everything is over (the playback flow is rebuilt or dropped).
    fn forget_all(&self) {
        self.ready.lock().expect("ready lock").clear();
        let gone: Vec<String> = self.ids.lock().expect("playing lock").drain().collect();
        for id in gone {
            let _ = self.ended.send(id);
        }
    }
}

/// Which way [`StromEngine::hold`] goes.
#[derive(Clone, Copy, Debug)]
pub(super) enum Hold {
    Pause,
    Resume,
}

/// What is held ready under an id.
#[derive(Clone, Debug, PartialEq)]
enum Ready {
    Sound(EngineSound),
    Picture(EnginePicture),
}

/// strom's request for `sound` as `id`; with a fade in it starts silent.
fn sound_request(id: &str, sound: &EngineSound, start: SoundStart) -> AddSoundRequest {
    AddSoundRequest {
        id: id.to_string(),
        uri: sound.uri.clone(),
        gain_db: if sound.fade_in_ms > 0 {
            GAIN_MIN_DB
        } else {
            f64::from(sound.gain_db)
        },
        routes: sound
            .routes
            .iter()
            .map(|r| SoundRoute {
                channel: r.channel,
                zone: r.zone,
                zone_channel: r.zone_channel,
                gain_db: f64::from(r.gain_db),
            })
            .collect(),
        start,
        repeat: match sound.repeat {
            sound::SoundRepeat::Once => SoundRepeat::Once,
            sound::SoundRepeat::Times(n) => SoundRepeat::Times(n),
            sound::SoundRepeat::Forever => SoundRepeat::Forever,
        },
    }
}

/// strom's request for `picture` as `id`; with a fade in it starts clear.
fn picture_request(id: &str, picture: &EnginePicture, start: SoundStart) -> AddPictureRequest {
    AddPictureRequest {
        id: id.to_string(),
        uri: picture.uri.clone(),
        stage: picture.stage,
        layer: u32::from(picture.layer),
        opacity: if picture.fade_in_ms > 0 {
            0.0
        } else {
            f64::from(picture.opacity)
        },
        fit: match picture.fit {
            stage::PictureFit::Fit => PictureFit::Fit,
            stage::PictureFit::Fill => PictureFit::Fill,
            stage::PictureFit::Stretch => PictureFit::Stretch,
        },
        end: match picture.end {
            stage::PictureEnd::Hold => PictureEnd::Hold,
            stage::PictureEnd::Stop => PictureEnd::Stop,
        },
        start,
        repeat: match picture.repeat {
            sound::SoundRepeat::Once => SoundRepeat::Once,
            sound::SoundRepeat::Times(n) => SoundRepeat::Times(n),
            sound::SoundRepeat::Forever => SoundRepeat::Forever,
        },
    }
}

/// Fade end in strom's terms.
fn fade_end(then: AfterFade) -> FadeEnd {
    match then {
        AfterFade::Keep => FadeEnd::Keep,
        AfterFade::Stop => FadeEnd::Stop,
    }
}

impl StromEngine {
    /// Build the playback flow, or drop it without zones and stages.
    pub(super) async fn set_playback(&self, playback: &Playback) -> Result<(), EngineError> {
        self.playback.forget_all();
        let named_device = playback
            .zones
            .iter()
            .flat_map(|z| &z.outputs)
            .any(|o| !o.device.is_empty());
        let devices = if named_device {
            self.device_channels().await
        } else {
            DeviceChannels::new()
        };
        match playback_flow(playback, &devices) {
            Some(flow) => self.put_flow(&flow).await,
            None => self.drop_flow(playback_flow_id()).await,
        }
    }

    /// strom's audio outputs that say their channel positions, by
    /// `node.name`; empty when strom cannot say (no discovery).
    async fn device_channels(&self) -> DeviceChannels {
        let url = self.url("/discovery/devices?category=audio_sink");
        let found: Result<Vec<DeviceResponse>, EngineError> = async {
            let resp = self.http.get(url).send().await.map_err(net)?;
            check(resp).await?.json().await.map_err(net)
        }
        .await;
        match found {
            Ok(found) => found
                .into_iter()
                .filter(|d| !d.channel_positions.is_empty())
                .filter_map(|d| Some((d.properties.get("node.name")?.clone(), d.channel_positions)))
                .collect(),
            Err(e) => {
                tracing::warn!("audio outputs from strom: {e}; zones go unpositioned");
                DeviceChannels::new()
            }
        }
    }

    fn playback_url(&self, path: &str) -> String {
        self.url(&format!(
            "/flows/{}/blocks/{PLAYBACK_BLOCK}/show_playback{path}",
            playback_flow_id()
        ))
    }

    async fn playback_post<T: Serialize>(&self, path: &str, body: &T) -> Result<(), EngineError> {
        let resp = self
            .http
            .post(self.playback_url(path))
            .json(body)
            .send()
            .await
            .map_err(net)?;
        check(resp).await.map(|_| ())
    }

    /// Hold `preload` ready: what is new is added waiting, what was held
    /// and is no more named (or named otherwise) goes.
    pub(super) async fn set_preload(&self, preload: &Preload) -> Result<(), EngineError> {
        let wanted: HashMap<String, Ready> = preload
            .sounds
            .iter()
            .map(|(id, s)| (id.clone(), Ready::Sound(s.clone())))
            .chain(
                preload
                    .pictures
                    .iter()
                    .map(|(id, p)| (id.clone(), Ready::Picture(p.clone()))),
            )
            .collect();
        let held: Vec<(String, Ready)> = self
            .playback
            .ready
            .lock()
            .expect("ready lock")
            .iter()
            .map(|(id, what)| (id.clone(), what.clone()))
            .collect();
        for (id, what) in held {
            if wanted.get(&id) == Some(&what) {
                continue;
            }
            self.playback.ready.lock().expect("ready lock").remove(&id);
            let kind = match what {
                Ready::Sound(_) => "sounds",
                Ready::Picture(_) => "pictures",
            };
            self.stop_each(kind, vec![id], 0).await;
        }
        for (id, what) in wanted {
            let held = self
                .playback
                .ready
                .lock()
                .expect("ready lock")
                .contains_key(&id);
            if held {
                continue;
            }
            let added = match &what {
                Ready::Sound(sound) => {
                    let request = sound_request(&id, sound, SoundStart::Preload);
                    self.playback_post("/sounds", &request).await
                }
                Ready::Picture(picture) => {
                    let request = picture_request(&id, picture, SoundStart::Preload);
                    self.playback_post("/pictures", &request).await
                }
            };
            match added {
                Ok(()) => {
                    self.playback
                        .ready
                        .lock()
                        .expect("ready lock")
                        .insert(id, what);
                }
                Err(e) => tracing::debug!(error = %e, %id, "not preloaded; it loads on GO"),
            }
        }
        Ok(())
    }

    /// Pause or resume `id` (`kind`: `sounds`, `pictures`, RUN.5).
    pub(super) async fn hold(&self, kind: &str, id: &str, step: Hold) -> Result<(), EngineError> {
        let verb = match step {
            Hold::Pause => "pause",
            Hold::Resume => "resume",
        };
        let path = format!("/{kind}/{}/{verb}", encode_path_segment(id));
        self.playback_post(&path, &serde_json::json!({})).await
    }

    /// Let held `id` go (`kind`: `sounds`, `pictures`).
    async fn release(&self, kind: &str, id: &str) -> Result<(), EngineError> {
        let path = format!("/{kind}/{}/play", encode_path_segment(id));
        self.playback_post(&path, &serde_json::json!({})).await
    }

    /// Play `sound` as `id` at once, letting it go when it is held ready;
    /// with a fade in it starts silent.
    pub(super) async fn play_sound(
        &self,
        id: &str,
        sound: &EngineSound,
    ) -> Result<(), EngineError> {
        let held = self.playback.take_ready(id, &Ready::Sound(sound.clone()));
        // Held but gone from the block meanwhile: added afresh.
        if !held || self.release("sounds", id).await.is_err() {
            let request = sound_request(id, sound, SoundStart::Play);
            self.playback_post("/sounds", &request).await?;
        }
        self.track(id);
        if sound.fade_in_ms > 0 {
            self.fade_sound(id, sound.gain_db, sound.fade_in_ms, AfterFade::Keep)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn fade_sound(
        &self,
        id: &str,
        gain_db: f32,
        duration_ms: u32,
        then: AfterFade,
    ) -> Result<(), EngineError> {
        let request = SoundFadeRequest {
            gain_db: f64::from(gain_db),
            duration_ms: u64::from(duration_ms),
            end: fade_end(then),
        };
        let path = format!("/sounds/{}/fade", encode_path_segment(id));
        self.playback_post(&path, &request).await
    }

    /// Stop sound `id`, or every sound; stopping one that is over is fine.
    pub(super) async fn stop_sound(
        &self,
        id: Option<&str>,
        fade_ms: u32,
    ) -> Result<(), EngineError> {
        let ids = match id {
            Some(id) => vec![id.to_string()],
            // Every sound that plays: what is held ready stays (the
            // standby cue after a panic).
            None => match self.playback_state().await {
                Ok(state) => self.playback.not_ready(state.sounds),
                // No playback flow: nothing plays.
                Err(_) => return Ok(()),
            },
        };
        self.stop_each("sounds", ids, fade_ms).await;
        Ok(())
    }

    pub(super) async fn zone_level(
        &self,
        zone: usize,
        gain_db: Option<f32>,
        mute: Option<bool>,
        duration_ms: u32,
    ) -> Result<(), EngineError> {
        let request = ZoneLevelRequest {
            gain_db: gain_db.map(f64::from),
            mute,
            duration_ms: u64::from(duration_ms),
        };
        self.playback_post(&format!("/zones/{zone}"), &request)
            .await
    }

    /// Show `picture` as `id` at once, letting it go when it is held
    /// ready; with a fade in it starts clear.
    pub(super) async fn show_picture(
        &self,
        id: &str,
        picture: &EnginePicture,
    ) -> Result<(), EngineError> {
        let held = self
            .playback
            .take_ready(id, &Ready::Picture(picture.clone()));
        if !held || self.release("pictures", id).await.is_err() {
            let request = picture_request(id, picture, SoundStart::Play);
            self.playback_post("/pictures", &request).await?;
        }
        self.track(id);
        if picture.fade_in_ms > 0 {
            self.fade_picture(id, picture.opacity, picture.fade_in_ms, AfterFade::Keep)
                .await?;
        }
        Ok(())
    }

    pub(super) async fn fade_picture(
        &self,
        id: &str,
        opacity: f32,
        duration_ms: u32,
        then: AfterFade,
    ) -> Result<(), EngineError> {
        let request = PictureFadeRequest {
            opacity: f64::from(opacity),
            duration_ms: u64::from(duration_ms),
            end: fade_end(then),
        };
        let path = format!("/pictures/{}/fade", encode_path_segment(id));
        self.playback_post(&path, &request).await
    }

    /// Take picture `id`, or every picture, off its stage; one that is
    /// gone already is fine.
    pub(super) async fn stop_picture(
        &self,
        id: Option<&str>,
        fade_ms: u32,
    ) -> Result<(), EngineError> {
        let ids = match id {
            Some(id) => vec![id.to_string()],
            None => match self.playback_state().await {
                Ok(state) => self.playback.not_ready(state.pictures),
                Err(_) => return Ok(()),
            },
        };
        self.stop_each("pictures", ids, fade_ms).await;
        Ok(())
    }

    pub(super) async fn stage_level(
        &self,
        stage: usize,
        brightness: f32,
        duration_ms: u32,
    ) -> Result<(), EngineError> {
        let request = StageLevelRequest {
            brightness: f64::from(brightness),
            duration_ms: u64::from(duration_ms),
        };
        self.playback_post(&format!("/stages/{stage}"), &request)
            .await
    }

    /// Stop each of `ids` in `kind` (`sounds`, `pictures`).
    async fn stop_each(&self, kind: &str, ids: Vec<String>, fade_ms: u32) {
        let request = SoundStopRequest {
            fade_ms: u64::from(fade_ms),
        };
        for id in ids {
            let path = format!("/{kind}/{}/stop", encode_path_segment(&id));
            if let Err(e) = self.playback_post(&path, &request).await {
                tracing::debug!(error = %e, %id, "not stopped; over already");
            }
        }
    }

    async fn playback_state(&self) -> Result<ShowPlaybackState, EngineError> {
        let resp = self
            .http
            .get(self.playback_url(""))
            .send()
            .await
            .map_err(net)?;
        check(resp).await?.json().await.map_err(net)
    }

    /// Note `id` as playing; the poller runs while anything plays.
    fn track(&self, id: &str) {
        self.playback
            .ids
            .lock()
            .expect("playing lock")
            .insert(id.to_string());
        if self.playback.polling.swap(true, Ordering::SeqCst) {
            return;
        }
        let http = self.http.clone();
        let url = self.playback_url("");
        let ids = self.playback.ids.clone();
        let ended = self.playback.ended.clone();
        let polling = self.playback.polling.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(PLAYBACK_POLL).await;
                let state = match http.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        resp.json::<ShowPlaybackState>().await.ok()
                    }
                    // The flow is gone: so is everything on it.
                    Ok(_) => Some(ShowPlaybackState {
                        zones: Vec::new(),
                        sounds: Vec::new(),
                        stages: Vec::new(),
                        pictures: Vec::new(),
                    }),
                    Err(_) => None,
                };
                let mut ids = ids.lock().expect("playing lock");
                if let Some(state) = state {
                    for id in over(&ids, &state) {
                        ids.remove(&id);
                        let _ = ended.send(id);
                    }
                }
                if ids.is_empty() {
                    polling.store(false, Ordering::SeqCst);
                    return;
                }
            }
        });
    }
}

/// Of `playing`, those `state` shows ended, failed or no more.
fn over(playing: &HashSet<String>, state: &ShowPlaybackState) -> Vec<String> {
    playing
        .iter()
        .filter(|id| {
            state
                .sounds
                .iter()
                .chain(&state.pictures)
                .find(|s| &s.id == *id)
                .is_none_or(|s| matches!(s.state, SoundState::Ended | SoundState::Failed))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sounds_and_pictures_that_ended_or_left_are_over() {
        let playing: HashSet<String> = ["a", "b", "c", "p"].map(String::from).into();
        let info = |id: &str, state| SoundInfo {
            id: id.into(),
            state,
            error: None,
        };
        let state = ShowPlaybackState {
            zones: Vec::new(),
            sounds: vec![info("a", SoundState::Playing), info("b", SoundState::Ended)],
            stages: Vec::new(),
            pictures: vec![info("p", SoundState::Playing)],
        };
        let mut gone = over(&playing, &state);
        gone.sort();

        assert_eq!(gone, ["b", "c"]);
    }
}
