//! Audio playback engine for GUI preview.
//!
//! Uses `rodio` for audio output. Decodes audio segments into PCM buffers
//! and plays them synchronized with the timeline playback controller.

use animatix::timeline::AudioSegment;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source, buffer::SamplesBuffer};
use std::collections::HashMap;

/// Decoded audio data cached in memory.
struct DecodedAudio {
    /// Interleaved PCM samples (f32, one channel per sample).
    samples: Vec<f32>,
    /// Sample rate (e.g. 44100 Hz).
    sample_rate: u32,
    /// Number of channels (1 = mono, 2 = stereo).
    channels: u16,
    /// Total duration in seconds.
    duration_s: f64,
}

/// Audio playback engine for GUI preview.
///
/// Manages decoding of audio files and synchronized playback with the timeline.
/// On seek, stops all active sinks and restarts from the new position.
/// During smooth playback, starts new segments as they become active.
pub struct AudioEngine {
    _stream: OutputStream,
    stream_handle: OutputStreamHandle,
    /// Cache of decoded audio files (keyed by source path).
    cache: HashMap<String, DecodedAudio>,
    /// Currently active audio sinks with metadata.
    active: Vec<ActiveSegment>,
    /// Whether we were playing on the last sync call.
    last_playing: bool,
    /// Timeline time of the last sync call (for detecting seeks).
    last_sync_time_s: f64,
}

/// A currently playing audio segment.
struct ActiveSegment {
    /// Index of this segment in the segments list passed to the last sync.
    segment_index: usize,
    /// The rodio sink controlling playback.
    sink: Sink,
}

impl AudioEngine {
    /// Create a new audio engine, opening the default audio output device.
    pub fn new() -> Result<Self, String> {
        let (_stream, stream_handle) = OutputStream::try_default()
            .map_err(|e| format!("Failed to open audio output: {e}"))?;
        Ok(Self {
            _stream,
            stream_handle,
            cache: HashMap::new(),
            active: Vec::new(),
            last_playing: false,
            last_sync_time_s: 0.0,
        })
    }

    /// Ensure an audio file is loaded and cached. Returns decoded audio or error.
    fn ensure_loaded(&mut self, path: &str) -> Result<(), String> {
        if self.cache.contains_key(path) {
            return Ok(());
        }
        let file = std::fs::File::open(path)
            .map_err(|e| format!("Failed to open audio file '{path}': {e}"))?;
        let decoder = rodio::Decoder::new(file)
            .map_err(|e| format!("Failed to decode audio '{path}': {e}"))?;

        let sample_rate = decoder.sample_rate();
        let channels = decoder.channels();
        let total_duration = decoder.total_duration()
            .map(|d: std::time::Duration| d.as_secs_f64())
            .unwrap_or(0.0);

        let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();
        let duration_s = if sample_rate > 0 && channels > 0 {
            samples.len() as f64 / (sample_rate as f64 * channels as f64)
        } else {
            0.0
        };

        self.cache.insert(path.to_string(), DecodedAudio {
            samples,
            sample_rate,
            channels,
            duration_s: total_duration.max(duration_s),
        });
        Ok(())
    }

    /// Sync audio state with the current playback position.
    ///
    /// Call this each frame during playback. It handles:
    /// - Starting new segments when they become active
    /// - Re-syncing after a seek (time jump > 150ms or play state change)
    /// - Stopping all audio when paused
    pub fn sync(&mut self, segments: &[AudioSegment], time_s: f64, playing: bool) {
        if !playing {
            if self.last_playing {
                self.stop_all();
            }
            self.last_playing = false;
            return;
        }

        let time_diff = (time_s - self.last_sync_time_s).abs();
        let was_playing = self.last_playing;

        self.last_playing = true;
        self.last_sync_time_s = time_s;

        // On seek (time jump > 150ms) or play start, restart everything
        if !was_playing || time_diff > 0.15 {
            self.restart_at(segments, time_s);
            return;
        }

        // During smooth playback, start any new segments that should be active
        for (i, seg) in segments.iter().enumerate() {
            if seg.start_time_s <= time_s && !self.is_active(i) {
                self.start_segment(seg, i, time_s);
            }
        }

        // Clean up finished segments
        self.active.retain(|a| !a.sink.empty());
    }

    /// Stop all currently playing audio.
    pub fn stop_all(&mut self) {
        for active in self.active.drain(..) {
            active.sink.stop();
        }
    }

    /// Stop all audio and restart from the given timeline position.
    fn restart_at(&mut self, segments: &[AudioSegment], time_s: f64) {
        self.stop_all();

        // Load all unique audio files first
        for seg in segments {
            if let Err(e) = self.ensure_loaded(&seg.source) {
                tracing::warn!("Audio load failed for '{}': {e}", seg.source);
            }
        }

        // Start segments that should be audible at this time
        for (i, seg) in segments.iter().enumerate() {
            if let Err(e) = self.try_start_segment(seg, i, time_s) {
                tracing::warn!("Audio start failed for '{}': {e}", seg.source);
            }
        }
    }

    /// Try to start a segment at the given timeline time.
    /// Returns Ok if the segment started, Err if it couldn't.
    fn try_start_segment(&mut self, seg: &AudioSegment, index: usize, time_s: f64) -> Result<(), String> {
        let audio = self.cache.get(&seg.source)
            .ok_or_else(|| format!("Audio '{}' not loaded", seg.source))?;

        // Compute the segment's end time
        let seg_end = if let Some(dur) = seg.duration_s {
            seg.start_time_s + dur
        } else {
            seg.start_time_s + audio.duration_s
        };

        // Check if this segment is audible at time_s
        if time_s < seg.start_time_s || time_s >= seg_end {
            return Ok(()); // Not yet or already finished
        }

        let offset_s = time_s - seg.start_time_s;
        let remaining_s = seg_end - seg.start_time_s - offset_s;

        if remaining_s <= 0.0 {
            return Ok(());
        }

        let remaining_s = remaining_s.min(audio.duration_s - offset_s);
        if remaining_s <= 0.0 {
            return Ok(());
        }

        // Compute the sample range for this segment at the current time
        let channels = audio.channels as usize;
        let sample_rate = audio.sample_rate as usize;
        let start_sample = (offset_s * sample_rate as f64 * channels as f64) as usize;
        let num_samples = (remaining_s * sample_rate as f64 * channels as f64) as usize;
        let start_sample = start_sample.min(audio.samples.len());
        let end_sample = (start_sample + num_samples).min(audio.samples.len());

        if end_sample <= start_sample {
            return Ok(());
        }

        let segment_data = audio.samples[start_sample..end_sample].to_vec();
        let source = SamplesBuffer::new(audio.channels, audio.sample_rate, segment_data);
        let sink = Sink::try_new(&self.stream_handle)
            .map_err(|e| format!("Failed to create audio sink: {e}"))?;

        sink.set_volume(seg.volume);
        sink.append(source);
        self.active.push(ActiveSegment {
            segment_index: index,
            sink,
        });

        Ok(())
    }

    /// Start a segment without error handling (logs on failure).
    fn start_segment(&mut self, seg: &AudioSegment, index: usize, time_s: f64) {
        if let Err(e) = self.try_start_segment(seg, index, time_s) {
            tracing::warn!("Audio: {e}");
        }
    }

    /// Check if a segment index is currently active.
    fn is_active(&self, index: usize) -> bool {
        self.active.iter().any(|a| a.segment_index == index && !a.sink.empty())
    }
}