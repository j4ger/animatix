//! Device-independent waveform summaries: stream PCM once on a worker, retain
//! a bounded min/max pyramid, and paint in work proportional to screen width.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use rodio::Source;

const BASE_FRAMES: usize = 256;
const MAX_BINS: usize = 32_768;
const MAX_FILES: usize = 16;
const MAX_WORKERS: usize = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Peak {
    pub min: f32,
    pub max: f32,
}

impl Peak {
    fn include(&mut self, sample: f32) {
        if sample.is_finite() {
            self.min = self.min.min(sample.clamp(-1.0, 1.0));
            self.max = self.max.max(sample.clamp(-1.0, 1.0));
        }
    }

    fn merge(self, other: Self) -> Self {
        Self {
            min: self.min.min(other.min),
            max: self.max.max(other.max),
        }
    }
}

pub(crate) struct Waveform {
    levels: Vec<Vec<Peak>>,
    bin_seconds: f64,
    pub duration_s: f64,
}

impl Waveform {
    fn from_samples(samples: impl Iterator<Item = f32>, rate: u32, channels: u16) -> Self {
        if rate == 0 || channels == 0 {
            return Self {
                levels: Vec::new(),
                bin_seconds: 1.0,
                duration_s: 0.0,
            };
        }
        let mut bins = Vec::new();
        let mut frames_per_bin = BASE_FRAMES;
        let mut samples_per_bin = frames_per_bin * channels as usize;
        let mut peak = Peak::default();
        let mut pending = 0usize;
        let mut total = 0u64;
        for sample in samples {
            peak.include(sample);
            pending += 1;
            total += 1;
            if pending == samples_per_bin {
                bins.push(peak);
                peak = Peak::default();
                pending = 0;
                if bins.len() == MAX_BINS {
                    bins = reduce(&bins);
                    frames_per_bin *= 2;
                    samples_per_bin = frames_per_bin * channels as usize;
                }
            }
        }
        if pending > 0 {
            bins.push(peak);
        }
        let mut levels = vec![bins];
        while levels.last().is_some_and(|level| level.len() > 1) {
            levels.push(reduce(levels.last().unwrap()));
        }
        Self {
            levels,
            bin_seconds: frames_per_bin as f64 / rate as f64,
            duration_s: total as f64 / (rate as f64 * channels as f64),
        }
    }

    pub fn columns(&self, start_s: f64, end_s: f64, columns: usize) -> Vec<Peak> {
        let columns = columns.min(4096);
        if columns == 0 || !start_s.is_finite() || !end_s.is_finite() || end_s <= start_s {
            return Vec::new();
        }
        let span = end_s - start_s;
        if !span.is_finite() {
            return Vec::new();
        }
        let step = span / columns as f64;
        let level = ((step / self.bin_seconds).max(1.0).log2().floor() as usize)
            .min(self.levels.len().saturating_sub(1));
        let Some(bins) = self.levels.get(level) else {
            return vec![Peak::default(); columns];
        };
        let bin_s = self.bin_seconds * 2f64.powi(level as i32);
        (0..columns)
            .map(|column| {
                let from = start_s + column as f64 * step;
                let to = from + step;
                if to <= 0.0 || from >= self.duration_s {
                    return Peak::default();
                }
                let first = (from.max(0.0) / bin_s).floor() as usize;
                let last = ((to.min(self.duration_s) / bin_s).ceil() as usize).min(bins.len());
                bins.get(first..last)
                    .unwrap_or_default()
                    .iter()
                    .copied()
                    .fold(Peak::default(), Peak::merge)
            })
            .collect()
    }
}

fn reduce(bins: &[Peak]) -> Vec<Peak> {
    bins.chunks(2)
        .map(|pair| pair.get(1).map_or(pair[0], |other| pair[0].merge(*other)))
        .collect()
}

fn decode(path: &str) -> Result<Waveform, String> {
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let decoder = rodio::Decoder::new(file).map_err(|e| e.to_string())?;
    let rate = decoder.sample_rate();
    let channels = decoder.channels();
    Ok(Waveform::from_samples(decoder.convert_samples::<f32>(), rate, channels))
}

enum Entry {
    Queued,
    Loading(mpsc::Receiver<Result<Waveform, String>>),
    Ready(Arc<Waveform>),
    Failed(String),
}

#[derive(Clone)]
pub(crate) enum WaveformState {
    Loading,
    Ready(Arc<Waveform>),
    Failed(String),
}

#[derive(Default)]
struct CacheState {
    revision: u64,
    entries: HashMap<String, Entry>,
    active_workers: Arc<AtomicUsize>,
}

struct DecodePermit {
    active: Arc<AtomicUsize>,
    context: egui::Context,
}

impl Drop for DecodePermit {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
        self.context.request_repaint();
    }
}

/// Retained UI state, independent of the output device. Bounded to 16 assets,
/// two decoder workers, and under 2 * MAX_BINS peaks per asset.
#[derive(Clone, Default)]
pub(crate) struct WaveformCache(Arc<Mutex<CacheState>>);

impl WaveformCache {
    pub fn get(&self, path: &str, revision: u64, ctx: &egui::Context) -> WaveformState {
        let Ok(mut state) = self.0.lock() else {
            return WaveformState::Failed("Waveform cache unavailable".into());
        };
        if state.revision != revision {
            state.entries.clear();
            state.revision = revision;
        }
        for entry in state.entries.values_mut() {
            if let Entry::Loading(receiver) = entry {
                match receiver.try_recv() {
                    Ok(Ok(waveform)) => *entry = Entry::Ready(Arc::new(waveform)),
                    Ok(Err(error)) => *entry = Entry::Failed(error),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        *entry = Entry::Failed("Waveform worker stopped".into());
                    },
                    Err(mpsc::TryRecvError::Empty) => {},
                }
            }
        }
        if !state.entries.contains_key(path) {
            if state.entries.len() >= MAX_FILES {
                return WaveformState::Failed("Waveform preview limited to 16 audio files".into());
            }
            state.entries.insert(path.to_owned(), Entry::Queued);
        }
        let loading = state.active_workers.load(Ordering::Relaxed);
        if loading < MAX_WORKERS && matches!(state.entries.get(path), Some(Entry::Queued)) {
            let path_owned = path.to_owned();
            let repaint = ctx.clone();
            let active_workers = state.active_workers.clone();
            active_workers.fetch_add(1, Ordering::Relaxed);
            let (sender, receiver) = mpsc::channel();
            state.entries.insert(path.to_owned(), Entry::Loading(receiver));
            std::thread::spawn(move || {
                let _permit = DecodePermit {
                    active: active_workers,
                    context: repaint,
                };
                let _ = sender.send(decode(&path_owned));
            });
        }
        match state.entries.get(path) {
            Some(Entry::Ready(waveform)) => WaveformState::Ready(waveform.clone()),
            Some(Entry::Failed(error)) => WaveformState::Failed(error.clone()),
            _ => WaveformState::Loading,
        }
    }
}

#[cfg(test)]
mod tests;
