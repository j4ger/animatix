use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, TryRecvError, bounded, select_biased};
use rodio::cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rodio::cpal::{self, FromSample, Sample, SampleFormat, SizedSample};
use tempfile::NamedTempFile;

const BLOCK_SAMPLES: usize = 4096;
const AUDIO_BLOCKS: usize = 32;
const MAX_SECONDS: u64 = 10 * 60;
const MAX_SAMPLES: u64 = 64 * 1024 * 1024;

pub(super) struct Recording {
    wav: NamedTempFile,
    pub seconds: f64,
    pub reached_limit: bool,
}

impl Recording {
    pub fn path(&self) -> &Path {
        self.wav.path()
    }
}

enum Control {
    Stop,
    Discard,
}

pub(super) struct Capture {
    control: Sender<Control>,
    result: Receiver<Result<Recording, String>>,
    started: Arc<OnceLock<Instant>>,
    pub stopping: bool,
}

impl Capture {
    pub fn start(ctx: egui::Context) -> Result<Self, String> {
        let (control, requests) = bounded(2);
        let (done, result) = bounded(1);
        let started = Arc::new(OnceLock::new());
        let worker_started = Arc::clone(&started);
        std::thread::Builder::new()
            .name("narration-microphone".into())
            .spawn(move || {
                let _ = done.send(record(requests, &worker_started, &ctx));
                ctx.request_repaint();
            })
            .map_err(|error| format!("Could not start the microphone worker: {error}"))?;
        Ok(Self {
            control,
            result,
            started,
            stopping: false,
        })
    }

    pub fn elapsed(&self) -> Option<Duration> {
        self.started.get().map(Instant::elapsed)
    }

    pub fn stop(&mut self) {
        if !self.stopping {
            self.stopping = true;
            let _ = self.control.try_send(Control::Stop);
        }
    }

    pub fn poll(&self) -> Option<Result<Recording, String>> {
        match self.result.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => Some(Err("Microphone worker stopped.".into())),
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.control.try_send(Control::Discard);
    }
}

fn record(
    requests: Receiver<Control>,
    started: &OnceLock<Instant>,
    ctx: &egui::Context,
) -> Result<Recording, String> {
    let device = cpal::default_host()
        .default_input_device()
        .ok_or("No default microphone is available.")?;
    let config = device
        .default_input_config()
        .map_err(|error| format!("Could not configure the default microphone: {error}"))?;
    if !matches!(requests.try_recv(), Err(TryRecvError::Empty)) {
        return Err("Recording cancelled.".into());
    }

    let mut wav = CaptureWav::new(config.sample_rate().0, config.channels())?;
    let (free, spare) = bounded(AUDIO_BLOCKS);
    let (filled, blocks) = bounded(AUDIO_BLOCKS);
    let (errors, stream_errors) = bounded(1);
    for _ in 0..AUDIO_BLOCKS {
        free.send(Vec::with_capacity(BLOCK_SAMPLES))
            .map_err(|error| error.to_string())?;
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
        return Err("Recording cancelled.".into());
    }
    stream
        .play()
        .map_err(|error| format!("Could not start microphone capture: {error}"))?;
    let _ = started.set(Instant::now());
    ctx.request_repaint();

    while !wav.full() {
        select_biased! {
            recv(requests) -> request => {
                if matches!(request, Ok(Control::Stop)) {
                    break;
                }
                return Err("Recording discarded.".into());
            },
            recv(stream_errors) -> error => {
                return Err(error.unwrap_or_else(|_| "The microphone disconnected.".into()));
            },
            recv(blocks) -> block => {
                let mut block = block.map_err(|_| "The microphone stopped sending audio.")?;
                wav.append(&block)?;
                block.clear();
                let _ = free.try_send(block);
            },
        }
    }
    drop(stream);
    if let Ok(error) = stream_errors.try_recv() {
        return Err(error);
    }
    // The callback has stopped. Keep samples it queued before the stop request.
    for block in blocks.try_iter() {
        wav.append(&block)?;
        if wav.full() {
            break;
        }
    }
    wav.finish()
}

struct InputBuffers {
    spare: Receiver<Vec<i16>>,
    filled: Sender<Vec<i16>>,
    errors: Sender<String>,
}

fn open_stream(
    device: &cpal::Device,
    config: &cpal::SupportedStreamConfig,
    buffers: InputBuffers,
) -> Result<cpal::Stream, String> {
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
            return Err(format!("Unsupported microphone sample format {}", config.sample_format()));
        },
    };
    stream.map_err(|error| format!("Could not open the default microphone: {error}"))
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
                    let _ = buffers.errors.try_send(
                        "The microphone could not record without dropping samples.".into(),
                    );
                    return;
                };
                block.extend(chunk.iter().map(|sample| pcm16(sample.to_sample::<f32>())));
                if buffers.filled.try_send(block).is_err() {
                    return;
                }
            }
        },
        move |error| {
            let _ = stream_errors.try_send(format!("Microphone capture stopped: {error}"));
        },
        None,
    )
}

fn pcm16(sample: f32) -> i16 {
    (sample * 32768.0).round() as i16
}

struct CaptureWav {
    writer: hound::WavWriter<BufWriter<File>>,
    wav: NamedTempFile,
    rate: u32,
    channels: u16,
    samples: u64,
    limit: u64,
}

impl CaptureWav {
    fn new(rate: u32, channels: u16) -> Result<Self, String> {
        if rate == 0 || channels == 0 {
            return Err("Invalid microphone sample layout.".into());
        }
        let wav = tempfile::Builder::new()
            .prefix("animatix-microphone-")
            .suffix(".wav")
            .tempfile()
            .map_err(|error| format!("Could not create the recording: {error}"))?;
        let file =
            wav.reopen().map_err(|error| format!("Could not open the recording: {error}"))?;
        let spec = hound::WavSpec {
            channels,
            sample_rate: rate,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let writer = hound::WavWriter::new(BufWriter::new(file), spec)
            .map_err(|error| format!("Could not write the recording header: {error}"))?;
        let frames = (MAX_SECONDS * u64::from(rate)).min(MAX_SAMPLES / u64::from(channels));
        Ok(Self {
            wav,
            writer,
            rate,
            channels,
            samples: 0,
            limit: frames * u64::from(channels),
        })
    }

    fn append(&mut self, block: &[i16]) -> Result<(), String> {
        let available = usize::try_from(self.limit - self.samples).unwrap_or(usize::MAX);
        let count = block.len().min(available);
        for &sample in &block[..count] {
            self.writer
                .write_sample(sample)
                .map_err(|error| format!("Could not write the recording: {error}"))?;
        }
        self.samples += count as u64;
        Ok(())
    }

    fn full(&self) -> bool {
        self.samples == self.limit
    }

    fn finish(self) -> Result<Recording, String> {
        if self.samples == 0 {
            return Err("The microphone recorded no audio.".into());
        }
        let reached_limit = self.full();
        self.writer
            .finalize()
            .map_err(|error| format!("Could not finalize the recording: {error}"))?;
        let seconds = self.samples as f64 / (f64::from(self.rate) * f64::from(self.channels));
        Ok(Recording {
            wav: self.wav,
            seconds,
            reached_limit,
        })
    }
}

#[cfg(test)]
mod tests;
