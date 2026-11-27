//! Prepared browser video metadata and synchronous handoff of sampled pixels.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use animatix::composition::BuildTarget;
use animatix::timeline::Timeline;
use animatix::timeline::image::SceneImage;
use animatix::timeline::video::{VideoAsset, VideoMetadata};
use js_sys::Uint8ClampedArray;
use serde::Deserialize;
use wasm_bindgen::prelude::*;

use super::AmxPlayer;

pub(super) type VideoFrames = HashMap<String, Arc<BrowserVideoFrames>>;
type Sample<'a> = (&'a str, f64, bool);

#[derive(Debug)]
pub(super) struct BrowserVideoFrames {
    metadata: VideoMetadata,
    // Different actors can sample the same source at different times in one render.
    frames: Mutex<Vec<(u64, SceneImage)>>,
}

#[derive(Deserialize)]
struct BrowserMetadata {
    width: u32,
    height: u32,
    duration_s: f64,
}

impl BrowserVideoFrames {
    pub(super) fn new(id: &str, payload: JsValue) -> Result<Arc<Self>, JsError> {
        let BrowserMetadata {
            width,
            height,
            duration_s,
        } = serde_wasm_bindgen::from_value(payload)
            .map_err(|error| JsError::new(&format!("video '{id}' metadata: {error}")))?;
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || u64::from(width) * u64::from(height) > 16_777_216
            || !duration_s.is_finite()
            || duration_s <= 0.0
        {
            return Err(JsError::new(&format!(
                "video '{id}' has unsupported dimensions or duration"
            )));
        }
        Ok(Arc::new(Self {
            metadata: VideoMetadata {
                display_size: [width, height],
                duration_s,
                // Browser playback is a visual layer. The native renderer mixes audio.
                audio_start_s: None,
            },
            frames: Mutex::default(),
        }))
    }

    fn image(&self, payload: &JsValue) -> Result<SceneImage, JsError> {
        let pixels = payload
            .dyn_ref::<Uint8ClampedArray>()
            .ok_or_else(|| JsError::new("video frame must be a Uint8ClampedArray"))?;
        let [width, height] = self.metadata.display_size;
        if pixels.length() != width * height * 4 {
            return Err(JsError::new("video frame byte count differs from its display dimensions"));
        }
        Ok(SceneImage::from_rgba8(width, height, pixels.to_vec()))
    }
}

impl VideoAsset for BrowserVideoFrames {
    fn metadata(&self) -> VideoMetadata {
        self.metadata
    }

    fn frame_at(&self, time_s: f64) -> Result<Option<SceneImage>, String> {
        let frames = self.frames.lock().map_err(|_| "Browser video samples were interrupted")?;
        Ok(frames
            .iter()
            .find(|(time, _)| *time == time_s.to_bits())
            .map(|(_, image)| image.clone()))
    }
}

impl AmxPlayer {
    pub(super) fn has_video_samples(&self, time_s: f64) -> bool {
        self.videos.is_empty()
            || self.prepared_video_time == Some(time_s.to_bits())
            || self.video_requests(time_s).is_empty()
    }

    fn video_requests<'a>(&'a self, time_s: f64) -> Vec<Sample<'a>> {
        let mut samples = Vec::new();
        let mut add = |timeline: &'a Timeline, local| {
            for sample in timeline.video_samples_at(local) {
                if !samples.contains(&sample) {
                    samples.push(sample);
                }
            }
        };
        match &self.target {
            Some(BuildTarget::SingleScene(timeline)) => add(timeline, time_s),
            Some(BuildTarget::MultiScene(composition)) => {
                let (scene, local, blend) = composition.evaluate(time_s);
                if let Some(blend) = blend {
                    if let Some(from) = composition.scenes.get(&blend.from_scene) {
                        add(&from.timeline, blend.from_local);
                    }
                    if let Some(to) = composition.scenes.get(&blend.to_scene) {
                        add(&to.timeline, blend.to_local);
                    }
                } else if let Some(scene) = composition.scenes.get(&scene) {
                    add(&scene.timeline, local);
                }
            },
            None => {},
        }
        samples
    }
}

#[wasm_bindgen]
impl AmxPlayer {
    /// Return parallel video requests as [assetId, sourceSeconds, beforeTrimEnd].
    pub fn video_frame_requests(&self, time_s: f64) -> Result<JsValue, JsError> {
        serde_wasm_bindgen::to_value(&self.video_requests(time_s))
            .map_err(|error| JsError::new(&error.to_string()))
    }

    /// Supply tightly packed RGBA8 samples in request order and present the frame.
    pub fn render_video_frame(
        &mut self,
        time_s: f64,
        payloads: Vec<JsValue>,
    ) -> Result<bool, JsError> {
        let requests = self.video_requests(time_s);
        if requests.len() != payloads.len() {
            return Err(JsError::new("video frame payload count differs from the request count"));
        }
        let mut by_source = HashMap::<_, Vec<_>>::new();
        for ((id, source_time, _), payload) in requests.into_iter().zip(payloads) {
            let video =
                self.videos.get(id).ok_or_else(|| JsError::new("video source is unprepared"))?;
            by_source
                .entry(id)
                .or_default()
                .push((source_time.to_bits(), video.image(&payload)?));
        }
        for (id, video) in &self.videos {
            *video
                .frames
                .lock()
                .map_err(|_| JsError::new("video samples were interrupted"))? =
                by_source.remove(id.as_str()).unwrap_or_default();
        }
        self.prepared_video_time = Some(time_s.to_bits());
        self.render_frame(time_s)
    }
}
