//! Sounds on strom's Show Sound block (show mode SND): the sound flow,
//! and play, fade, stop and zone levels over its REST API.
//!
//! strom does not report the end of a sound by itself, so while sounds
//! play a poller reads the block's state and reports those that are gone
//! ([`Engine::sounds_ended`](crate::engine::Engine::sounds_ended)).

use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::Serialize;
use strom_types::show_sound::{
    AddSoundRequest, FadeEnd, ShowSoundState, SoundFadeRequest, SoundRoute, SoundStart, SoundState,
    SoundStopRequest, ZoneLevelRequest, GAIN_MIN_DB,
};
use tokio::sync::broadcast;

use lmix_types::sound::AfterFade;
use lmix_types::ZoneState;

use super::{check, net, StromEngine};
use crate::engine::flows::encode_path_segment;
use crate::engine::sound_flow::{sound_flow, sound_flow_id, SOUND_BLOCK};
use crate::engine::EngineError;
use crate::sound::EngineSound;

/// How often the poller reads which sounds still play.
const SOUND_POLL: Duration = Duration::from_millis(100);

/// Sounds the engine plays, and who hears that they ended.
pub(super) struct Sounds {
    playing: Arc<Mutex<HashSet<String>>>,
    ended: broadcast::Sender<String>,
    polling: Arc<std::sync::atomic::AtomicBool>,
}

impl Default for Sounds {
    fn default() -> Self {
        Self {
            playing: Arc::default(),
            ended: broadcast::channel(256).0,
            polling: Arc::default(),
        }
    }
}

impl Sounds {
    pub(super) fn subscribe(&self) -> broadcast::Receiver<String> {
        self.ended.subscribe()
    }

    /// Every sound is over (the sound flow is rebuilt or dropped).
    fn forget_all(&self) {
        let gone: Vec<String> = self.playing.lock().expect("sounds lock").drain().collect();
        for id in gone {
            let _ = self.ended.send(id);
        }
    }
}

impl StromEngine {
    /// Build the sound flow for `zones`, or drop it when there are none.
    pub(super) async fn set_zones(&self, zones: &[ZoneState]) -> Result<(), EngineError> {
        self.sounds.forget_all();
        match sound_flow(zones) {
            Some(flow) => self.put_flow(&flow).await,
            None => self.drop_flow(sound_flow_id()).await,
        }
    }

    fn sound_url(&self, path: &str) -> String {
        self.url(&format!(
            "/flows/{}/blocks/{SOUND_BLOCK}/show_sound{path}",
            sound_flow_id()
        ))
    }

    async fn sound_post<T: Serialize>(&self, path: &str, body: &T) -> Result<(), EngineError> {
        let resp = self
            .http
            .post(self.sound_url(path))
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
        self.sound_post("/sounds", &request).await?;
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
            end: match then {
                AfterFade::Keep => FadeEnd::Keep,
                AfterFade::Stop => FadeEnd::Stop,
            },
        };
        let path = format!("/sounds/{}/fade", encode_path_segment(id));
        self.sound_post(&path, &request).await
    }

    /// Stop sound `id`, or every sound; stopping one that is over is fine.
    pub(super) async fn stop_sound(
        &self,
        id: Option<&str>,
        fade_ms: u32,
    ) -> Result<(), EngineError> {
        let ids = match id {
            Some(id) => vec![id.to_string()],
            None => match self.sound_state().await {
                Ok(state) => state.sounds.into_iter().map(|s| s.id).collect(),
                // No sound flow: nothing plays.
                Err(_) => return Ok(()),
            },
        };
        let request = SoundStopRequest {
            fade_ms: u64::from(fade_ms),
        };
        for id in ids {
            let path = format!("/sounds/{}/stop", encode_path_segment(&id));
            if let Err(e) = self.sound_post(&path, &request).await {
                tracing::debug!(error = %e, %id, "sound not stopped; over already");
            }
        }
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
        self.sound_post(&format!("/zones/{zone}"), &request).await
    }

    async fn sound_state(&self) -> Result<ShowSoundState, EngineError> {
        let resp = self
            .http
            .get(self.sound_url(""))
            .send()
            .await
            .map_err(net)?;
        check(resp).await?.json().await.map_err(net)
    }

    /// Note `id` as playing; the poller runs while anything plays.
    fn track(&self, id: &str) {
        self.sounds
            .playing
            .lock()
            .expect("sounds lock")
            .insert(id.to_string());
        if self.sounds.polling.swap(true, Ordering::SeqCst) {
            return;
        }
        let http = self.http.clone();
        let url = self.sound_url("");
        let playing = self.sounds.playing.clone();
        let ended = self.sounds.ended.clone();
        let polling = self.sounds.polling.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(SOUND_POLL).await;
                let state = match http.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {
                        resp.json::<ShowSoundState>().await.ok()
                    }
                    // The flow is gone: so are its sounds.
                    Ok(_) => Some(ShowSoundState {
                        zones: Vec::new(),
                        sounds: Vec::new(),
                    }),
                    Err(_) => None,
                };
                let mut playing = playing.lock().expect("sounds lock");
                if let Some(state) = state {
                    for id in over(&playing, &state) {
                        playing.remove(&id);
                        let _ = ended.send(id);
                    }
                }
                if playing.is_empty() {
                    polling.store(false, Ordering::SeqCst);
                    return;
                }
            }
        });
    }
}

/// Of `playing`, the sounds `state` shows ended, failed or no more.
fn over(playing: &HashSet<String>, state: &ShowSoundState) -> Vec<String> {
    playing
        .iter()
        .filter(|id| {
            state
                .sounds
                .iter()
                .find(|s| &s.id == *id)
                .is_none_or(|s| matches!(s.state, SoundState::Ended | SoundState::Failed))
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use strom_types::show_sound::SoundInfo;

    #[test]
    fn sounds_that_ended_or_left_are_over() {
        let playing: HashSet<String> = ["a", "b", "c"].map(String::from).into();
        let info = |id: &str, state| SoundInfo {
            id: id.into(),
            state,
            error: None,
        };
        let state = ShowSoundState {
            zones: Vec::new(),
            sounds: vec![info("a", SoundState::Playing), info("b", SoundState::Ended)],
        };
        let mut gone = over(&playing, &state);
        gone.sort();

        assert_eq!(gone, ["b", "c"]);
    }
}
