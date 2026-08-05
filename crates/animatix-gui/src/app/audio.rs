//! Audio playback engine for GUI preview.
//!
//! Uses `rodio` for audio output. Decodes audio segments into PCM buffers
//! and plays them synchronized with the timeline playback controller.

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use animatix::timeline::AudioSegment;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamHandle, Sink, Source};

const MAX_CACHED_AUDIO: usize = 8;

fn decode_audio(path: &str) -> Result<DecodedAudio, String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open audio file '{path}': {e}"))?;
    let decoder =
        rodio::Decoder::new(file).map_err(|e| format!("Failed to decode audio '{path}': {e}"))?;

    let sample_rate = decoder.sample_rate();
    let channels = decoder.channels();
    let total_duration = decoder
        .total_duration()
        .map(|d: std::time::Duration| d.as_secs_f64())
        .unwrap_or(0.0);

    let samples: Vec<f32> = decoder.convert_samples::<f32>().collect();
    let duration_s = if sample_rate > 0 && channels > 0 {
        samples.len() as f64 / (sample_rate as f64 * channels as f64)
    } else {
        0.0
    };

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
        duration_s: total_duration.max(duration_s),
    })
}

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
    /// Bounded LRU cache of decoded audio files (keyed by source path).
    cache: HashMap<String, DecodedAudio>,
    cache_order: VecDeque<String>,
    /// Decode requests still running on background threads.
    pending: HashMap<String, Receiver<Result<DecodedAudio, String>>>,
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
    /// Source path used for cache eviction.
    source: String,
    /// The rodio sink controlling playback.
    sink: Sink,
}

impl AudioEngine {
    /// Create a new audio engine, opening the default audio output device.
    pub fn new() -> Result<Self, String> {
        let (_stream, stream_handle) =
            OutputStream::try_default().map_err(|e| format!("Failed to open audio output: {e}"))?;
        Ok(Self {
            _stream,
            stream_handle,
            cache: HashMap::new(),
            cache_order: VecDeque::new(),
            pending: HashMap::new(),
            active: Vec::new(),
            last_playing: false,
            last_sync_time_s: 0.0,
        })
    }

    /// Queue an audio file for background decoding. Returns true if it is already usable.
    fn ensure_loaded(&mut self, path: &str) -> bool {
        if self.cache.contains_key(path) {
            self.touch_cache(path);
            return true;
        }
        if self.pending.contains_key(path) {
            return false;
        }

        let path_owned = path.to_string();
        let (tx, rx) = channel();
        self.pending.insert(path_owned.clone(), rx);
        std::thread::spawn(move || {
            let _ = tx.send(decode_audio(&path_owned));
        });
        false
    }

    fn poll_loaded(&mut self) {
        let mut finished = Vec::new();
        for (path, rx) in &self.pending {
            match rx.try_recv() {
                Ok(result) => finished.push((path.clone(), result)),
                Err(TryRecvError::Disconnected) => {
                    finished.push((path.clone(), Err("audio decoder thread disconnected".into())));
                },
                Err(TryRecvError::Empty) => {},
            }
        }

        for (path, result) in finished {
            self.pending.remove(&path);
            match result {
                Ok(audio) => self.insert_cached(path, audio),
                Err(err) => tracing::warn!("Audio load failed for '{path}': {err}"),
            }
        }
    }

    fn insert_cached(&mut self, path: String, audio: DecodedAudio) {
        self.cache_order.retain(|p| p != &path);
        self.cache_order.push_back(path.clone());
        self.cache.insert(path.clone(), audio);

        let mut protected = 0;
        while self.cache.len() > MAX_CACHED_AUDIO {
            let Some(evict) = self.cache_order.pop_front() else {
                break;
            };
            if self.active.iter().any(|a| a.source == evict) {
                self.cache_order.push_back(evict);
                protected += 1;
                if protected >= self.cache.len() {
                    break;
                }
            } else {
                self.cache.remove(&evict);
            }
        }
    }

    fn touch_cache(&mut self, path: &str) {
        if let Some(pos) = self.cache_order.iter().position(|p| p == path) {
            if let Some(cached_path) = self.cache_order.remove(pos) {
                self.cache_order.push_back(cached_path);
            }
        }
    }

    /// Sync audio state with the current playback position.
    ///
    /// Call this each frame during playback. It handles:
    /// - Starting new segments when they become active
    /// - Re-syncing after a seek (time jump > 150ms or play state change)
    /// - Stopping all audio when paused
    pub fn sync(
        &mut self,
        segments: &[AudioSegment],
        time_s: f64,
        playing: bool,
        playback_speed: f32,
    ) {
        self.poll_loaded();

        if !playing {
            if self.last_playing {
                self.stop_all();
            }
            self.last_playing = false;
            return;
        }

        let time_diff = (time_s - self.last_sync_time_s).abs();
        let moved_backward = time_s < self.last_sync_time_s;
        let was_playing = self.last_playing;

        self.last_playing = true;
        self.last_sync_time_s = time_s;

        // Restart on play start, backward motion (scrub or ping-pong reversal),
        // or any seek larger than 150ms.
        if !was_playing || moved_backward || time_diff > 0.15 {
            self.restart_at(segments, time_s);
            return;
        }

        // Keep already-playing segments in sync with playback speed changes.
        let speed = playback_speed.max(0.01);
        for active in &self.active {
            active.sink.set_speed(speed);
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

        // Queue every referenced file; decoding happens on background threads.
        for seg in segments {
            self.ensure_loaded(&seg.source);
        }

        // Start segments that are already decoded. Pending files will be picked
        // up by later sync calls once their decode finishes.
        for (i, seg) in segments.iter().enumerate() {
            if let Err(e) = self.try_start_segment(seg, i, time_s) {
                tracing::warn!("Audio start failed for '{}': {e}", seg.source);
            }
        }
    }

    /// Try to start a segment at the given timeline time.
    /// Returns Ok if the segment started, Err if it couldn't.
    fn try_start_segment(
        &mut self,
        seg: &AudioSegment,
        index: usize,
        time_s: f64,
    ) -> Result<(), String> {
        let Some(audio) = self.cache.get(&seg.source) else {
            return Ok(()); // Still decoding on a background thread.
        };

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
            source: seg.source.clone(),
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
