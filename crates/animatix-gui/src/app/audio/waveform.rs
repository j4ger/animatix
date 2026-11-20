//! Device-independent waveform summaries: stream PCM once on a worker, retain
//! a bounded min/max pyramid, and paint in work proportional to screen width.

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};

use super::decode::blocks;

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
    #[cfg(test)]
    fn from_samples(samples: impl Iterator<Item = f32>, rate: u32, channels: u16) -> Self {
        let mut builder = WaveformBuilder::new(rate, channels);
        builder.extend(samples);
        builder.finish()
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

struct WaveformBuilder {
    bins: Vec<Peak>,
    frames_per_bin: usize,
    peak: Peak,
    pending: usize,
    total: u64,
    rate: u32,
    channels: u16,
}

impl WaveformBuilder {
    fn new(rate: u32, channels: u16) -> Self {
        Self {
            bins: Vec::new(),
            frames_per_bin: BASE_FRAMES,
            peak: Peak::default(),
            pending: 0,
            total: 0,
            rate,
            channels,
        }
    }

    fn extend(&mut self, samples: impl IntoIterator<Item = f32>) {
        let channels = self.channels as usize;
        if self.rate == 0 || channels == 0 {
            return;
        }
        let mut samples_per_bin = self.frames_per_bin * channels;
        for sample in samples {
            self.peak.include(sample);
            self.pending += 1;
            self.total += 1;
            if self.pending == samples_per_bin {
                self.bins.push(std::mem::take(&mut self.peak));
                self.pending = 0;
                if self.bins.len() == MAX_BINS {
                    self.bins = reduce(&self.bins);
                    self.frames_per_bin *= 2;
                    samples_per_bin = self.frames_per_bin * channels;
                }
            }
        }
    }

    fn finish(mut self) -> Waveform {
        if self.pending > 0 {
            self.bins.push(self.peak);
        }
        let mut levels = vec![self.bins];
        while let Some(level) = levels.last().filter(|level| level.len() > 1) {
            levels.push(reduce(level));
        }
        Waveform {
            levels,
            bin_seconds: self.frames_per_bin as f64 / self.rate.max(1) as f64,
            duration_s: self.total as f64 / (self.rate.max(1) as f64 * self.channels.max(1) as f64),
        }
    }
}

fn reduce(bins: &[Peak]) -> Vec<Peak> {
    bins.chunks(2)
        .map(|pair| pair.get(1).map_or(pair[0], |other| pair[0].merge(*other)))
        .collect()
}

fn decode(path: &str) -> Result<Waveform, String> {
    let mut builder = None;
    let format = blocks(path, |format, samples| {
        builder
            .get_or_insert_with(|| WaveformBuilder::new(format.sample_rate, format.channels))
            .extend(samples.iter().copied());
    })?;
    Ok(builder
        .unwrap_or_else(|| WaveformBuilder::new(format.sample_rate, format.channels))
        .finish())
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
