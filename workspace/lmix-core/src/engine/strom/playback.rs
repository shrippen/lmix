//! Sounds and pictures on strom's Show Playback block (show mode SND,
//! VIS): the playback flow, and play, fade, stop, zone levels and stage
//! brightness over its REST API.
//!
//! strom does not report the end of a sound or picture by itself, so
//! while any plays a poller reads the block's state and reports those
//! that are gone ([`Engine::playback_ended`](crate::engine::Engine::playback_ended)).

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use strom_types::show_playback::{
    AddPictureRequest, AddSoundRequest, FadeEnd, PictureEnd, PictureFadeRequest, PictureFit,
    ShowPlaybackState, SoundFadeRequest, SoundRoute, SoundStart, SoundState, SoundStopRequest,
    StageLevelRequest, ZoneLevelRequest, GAIN_MIN_DB,
};
use tokio::sync::broadcast;

use lmix_types::sound::AfterFade;
use lmix_types::stage;

use super::{check, net, StromEngine};
use crate::engine::flows::encode_path_segment;
use crate::engine::playback_flow::{playback_flow, playback_flow_id, PLAYBACK_BLOCK};
use crate::engine::EngineError;
use crate::playback::{EnginePicture, EngineSound};
use crate::switcher::Playback;

/// How often the poller reads which sounds and pictures still play.
const PLAYBACK_POLL: Duration = Duration::from_millis(100);

/// Sounds and pictures the engine plays, and who hears that they ended.
/// Their ids are cue ids, so one set holds both.
pub(super) struct Playing {
    ids: Arc<Mutex<HashSet<String>>>,
    ended: broadcast::Sender<String>,
    polling: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for Playing {
    fn default() -> Self {
        Self {
            ids: Arc::default(),
            ended: broadcast::channel(256).0,
            polling: Arc::default(),
        }
    }
}

impl Playing {
    pub(super) fn subscribe(&self) -> broadcast::Receiver<String> {
        self.ended.subscribe()
    }

    /// Everything is over (the playback flow is rebuilt or dropped).
    fn forget_all(&self) {
        let gone: Vec<String> = self.ids.lock().expect("playing lock").drain().collect();
        for id in gone {
            let _ = self.ended.send(id);
        }
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
        match playback_flow(playback) {
            Some(flow) => self.put_flow(&flow).await,
            None => self.drop_flow(playback_flow_id()).await,
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

    /// Play `sound` as `id` at once; with a fade in it starts silent.
    pub(super) async fn play_sound(
        &self,
        id: &str,
        sound: &EngineSound,
    ) -> Result<(), EngineError> {
        let request = AddSoundRequest {
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
            start: SoundStart::Play,
        };
        self.playback_post("/sounds", &request).await?;
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
            None => match self.playback_state().await {
                Ok(state) => state.sounds.into_iter().map(|s| s.id).collect(),
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

    /// Show `picture` as `id` at once; with a fade in it starts clear.
    pub(super) async fn show_picture(
        &self,
        id: &str,
        picture: &EnginePicture,
    ) -> Result<(), EngineError> {
        let request = AddPictureRequest {
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
            start: SoundStart::Play,
        };
        self.playback_post("/pictures", &request).await?;
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
                Ok(state) => state.pictures.into_iter().map(|p| p.id).collect(),
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
    use strom_types::show_playback::SoundInfo;

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
