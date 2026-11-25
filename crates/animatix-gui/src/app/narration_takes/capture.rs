use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crate::app::i18n::StatusText;

use super::history::{History, Take};
use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, select_biased};
use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::cpal::{self, FromSample, Sample, SampleFormat, SizedSample};

const BLOCK_SAMPLES: usize = 4096;
const AUDIO_BLOCKS: usize = 32;
const MAX_SECONDS: u64 = 10 * 60;
const MAX_SAMPLES: u64 = 64 * 1024 * 1024;

pub(super) struct Capture {
    control: Sender<()>,
    result: Receiver<Result<(), StatusText>>,
    started: Arc<OnceLock<Instant>>,
    pub stopping: bool,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Capture {
    pub fn start(
        ctx: egui::Context,
        history: Arc<History>,
        take: Take,
    ) -> Result<Self, StatusText> {
        let (control, requests) = bounded(2);
        let (done, result) = bounded(1);
        let started = Arc::new(OnceLock::new());
        let worker_started = Arc::clone(&started);
        let thread = std::thread::Builder::new()
            .name("narration-microphone".into())
            .spawn(move || {
                let _ = done.send(record(requests, &worker_started, &ctx, &history, &take));
                ctx.request_repaint();
            })
            .map_err(|error| {
                StatusText::message(
                    "gui-narration-mic-worker-start",
                    [("detail", error.to_string())],
                )
            })?;
        Ok(Self {
            control,
            result,
            started,
            stopping: false,
            thread: Some(thread),
        })
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.started.get().map(Instant::elapsed)
    }

    pub fn stop(&mut self) {
        if !self.stopping {
            self.stopping = true;
            let _ = self.control.try_send(());
        }
    }

    pub fn poll(&self) -> Option<Result<(), StatusText>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err(StatusText::message("gui-narration-mic-worker-stopped", [])))
            },
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn record(
    requests: Receiver<()>,
    started: &OnceLock<Instant>,
    ctx: &egui::Context,
    history: &History,
    take: &Take,
) -> Result<(), StatusText> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or_else(|| StatusText::message("gui-narration-mic-no-device", []))?;
    let config = device.default_input_config().map_err(|error| {
        StatusText::message("gui-narration-mic-configure", [("detail", error.to_string())])
    })?;
    if !matches!(requests.try_recv(), Err(TryRecvError::Empty)) {
        return Err(StatusText::message("gui-narration-record-cancelled", []));
    }

    let (free, spare) = bounded(AUDIO_BLOCKS);
    let (filled, blocks) = bounded(AUDIO_BLOCKS);
    let (errors, stream_errors) = bounded(1);
    for _ in 0..AUDIO_BLOCKS {
        free.send(Vec::with_capacity(BLOCK_SAMPLES)).map_err(recording_error)?;
    }
    let stream = open_stream(
        &device,
        &config,
        InputBuffers {
            spare,
            filled,
            errors,
        },
    )?;
    if !matches!(requests.try_recv(), Err(TryRecvError::Empty)) {
        return Err(StatusText::message("gui-narration-record-cancelled", []));
    }
    let mut recording = Recording::new(history, take, config.sample_rate().0, config.channels())?;
    stream.play().map_err(|error| {
        StatusText::message("gui-narration-mic-start", [("detail", error.to_string())])
    })?;
    let _ = started.set(Instant::now());
    ctx.request_repaint();

    while !recording.full() {
        select_biased! {
            recv(requests) -> _ => break,
            recv(stream_errors) -> error => {
                return Err(error.map(StreamFailure::message).unwrap_or_else(|_| StatusText::message("gui-narration-mic-disconnected", [])));
            },
            recv(blocks) -> block => {
                let mut block = block.map_err(|_| StatusText::message("gui-narration-mic-no-stream", []))?;
                recording.append(&block)?;
                block.clear();
                let _ = free.try_send(block);
            },
        }
    }
    drop(stream);
    if let Ok(error) = stream_errors.try_recv() {
        return Err(error.message());
    }
    // The callback has stopped. Keep samples it queued before the stop request.
    for block in blocks.try_iter() {
        recording.append(&block)?;
        if recording.full() {
            break;
        }
    }
    recording.finish()
}

struct InputBuffers {
    spare: Receiver<Vec<i16>>,
    filled: Sender<Vec<i16>>,
    errors: Sender<StreamFailure>,
}

// Keep formatting out of the real-time audio callback.
enum StreamFailure {
    DroppedSamples,
    Backend(cpal::StreamError),
}

impl StreamFailure {
    fn message(self) -> StatusText {
        match self {
            Self::DroppedSamples => StatusText::message("gui-narration-mic-dropped", []),
            Self::Backend(error) => {
                StatusText::message("gui-narration-mic-stopped", [("detail", error.to_string())])
            },
        }
    }
}

fn open_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    buffers: InputBuffers,
) -> Result<cpal::Stream, StatusText> {
    let stream = match config.sample_format() {
        SampleFormat::I8 => input::<i8>(device, config, buffers),
        SampleFormat::I16 => input::<i16>(device, config, buffers),
        SampleFormat::I32 => input::<i32>(device, config, buffers),
        SampleFormat::I64 => input::<i64>(device, config, buffers),
        SampleFormat::U8 => input::<u8>(device, config, buffers),
        SampleFormat::U16 => input::<u16>(device, config, buffers),
        SampleFormat::U32 => input::<u32>(device, config, buffers),
        SampleFormat::U64 => input::<u64>(device, config, buffers),
        SampleFormat::F32 => input::<f32>(device, config, buffers),
        SampleFormat::F64 => input::<f64>(device, config, buffers),
        _ => {
            return Err(StatusText::message(
                "gui-narration-mic-format",
                [("format", config.sample_format().to_string())],
            ));
        },
    };
    stream.map_err(|error| {
        StatusText::message("gui-narration-mic-open", [("detail", error.to_string())])
    })
}

fn input<T: SizedSample + Sample>(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    buffers: InputBuffers,
) -> Result<cpal::Stream, cpal::BuildStreamError>
where
    f32: FromSample<T>,
{
    let stream_errors = buffers.errors.clone();
    device.build_input_stream(
        &config.config(),
        move |samples: &[T], _| {
            for chunk in samples.chunks(BLOCK_SAMPLES) {
                let Ok(mut block) = buffers.spare.try_recv() else {
                    let _ = buffers.errors.try_send(StreamFailure::DroppedSamples);
                    return;
                };
                block.extend(chunk.iter().map(|sample| pcm16(sample.to_sample::<f32>())));
                if buffers.filled.try_send(block).is_err() {
                    return;
                }
            }
        },
        move |error| {
            let _ = stream_errors.try_send(StreamFailure::Backend(error));
        },
        None,
    )
}

fn pcm16(sample: f32) -> i16 {
    (sample * 32768.0).round() as i16
}

struct Recording<'a> {
    history: &'a History,
    id: &'a str,
    channels: u16,
    batch: usize,
    samples: usize,
    limit: usize,
    pending: Vec<i16>,
    complete: bool,
}

impl<'a> Recording<'a> {
    fn new(
        history: &'a History,
        take: &'a Take,
        rate: u32,
        channels: u16,
    ) -> Result<Self, StatusText> {
        if rate == 0 || channels == 0 {
            return Err(StatusText::message("gui-narration-mic-sample-layout", []));
        }
        let rate = u64::from(rate);
        let channel_count = u64::from(channels);
        let frames = (MAX_SECONDS * rate).min(MAX_SAMPLES / channel_count);
        history.start_recording(take, rate as u32, channels).map_err(recording_error)?;
        Ok(Self {
            history,
            id: &take.id,
            channels,
            batch: rate.min(MAX_SAMPLES / channel_count) as usize * usize::from(channels),
            samples: 0,
            limit: (frames * channel_count) as usize,
            pending: Vec::new(),
            complete: false,
        })
    }

    fn append(&mut self, block: &[i16]) -> Result<(), StatusText> {
        let count = block.len().min(self.limit - self.samples);
        self.pending.extend_from_slice(&block[..count]);
        self.samples += count;
        if self.pending.len() >= self.batch {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), StatusText> {
        let channels = usize::from(self.channels);
        let usable = self.pending.len() / channels * channels;
        self.history
            .append_recording(self.id, &self.pending[..usable], self.channels)
            .map_err(recording_error)?;
        self.pending.drain(..usable);
        Ok(())
    }

    fn full(&self) -> bool {
        self.samples >= self.limit
    }

    fn finish(mut self) -> Result<(), StatusText> {
        if self.samples < usize::from(self.channels) {
            return Err(StatusText::message("gui-narration-mic-no-audio", []));
        }
        self.flush()?;
        self.history.finish_recording(self.id, true).map_err(recording_error)?;
        self.complete = true;
        Ok(())
    }
}

impl Drop for Recording<'_> {
    fn drop(&mut self) {
        if !self.complete
            && let Err(error) = self.flush().and_then(|()| {
                self.history.finish_recording(self.id, false).map_err(recording_error)
            })
        {
            tracing::warn!(
                error = error.canonical(),
                "Could not finish interrupted microphone recording"
            );
        }
    }
}

fn recording_error(error: impl std::fmt::Display) -> StatusText {
    StatusText::message("gui-narration-record-detail", [("detail", error.to_string())])
}

#[cfg(test)]
mod tests;
