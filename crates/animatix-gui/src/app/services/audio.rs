//! Audio preview engine service trait.
//!
//! Implemented by the WGPU/audio backend for the eframe runtime.

use crate::app::PreviewPaneState;

/// Source of audio data for the preview engine.
pub enum AudioSource<'a> {
    Timeline(&'a animatix::timeline::Timeline),
    Composition(&'a animatix::composition::Composition),
}

/// State for audio playback synchronization.
pub struct AudioPlaybackState {
    pub current_time_s: f64,
    pub is_playing: bool,
    pub playback_speed: f64,
    pub duration_s: f64,
}

impl From<&PreviewPaneState> for AudioPlaybackState {
    fn from(preview: &PreviewPaneState) -> Self {
        Self {
            current_time_s: preview.playback.current_time_s(),
            is_playing: preview.playback.is_playing,
            playback_speed: preview.playback.playback_speed as f64,
            duration_s: preview.playback.duration_s,
        }
    }
}

/// Trait for audio preview (sound playback during animation preview).
pub trait AudioPreviewEngine {
    /// Sync the audio engine with the current playback state.
    /// The engine should seek/stops/play as needed.
    fn sync(&mut self, source: AudioSource<'_>, playback: &AudioPlaybackState);

    /// Stop all audio playback.
    fn stop(&mut self);

    /// Enable or disable audio output.
    fn set_enabled(&mut self, enabled: bool);
}