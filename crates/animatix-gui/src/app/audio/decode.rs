//! Decode local audio to interleaved PCM blocks for playback and waveform summaries.

use std::fs::File;
use std::io::{self, BufRead, BufReader, Seek};
use std::path::Path;

use rodio::{Decoder, Source};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::errors::Error;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::{MediaSource, MediaSourceStream};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::ffmpeg;

#[derive(Clone, Copy)]
pub(super) struct AudioFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub duration_s: f64,
}

pub(super) fn blocks(
    path: &str,
    mut consume: impl FnMut(AudioFormat, &[f32]),
) -> Result<AudioFormat, String> {
    let mut file = BufReader::new(
        File::open(path).map_err(|error| format!("Failed to open audio file '{path}': {error}"))?,
    );
    let header = file.fill_buf().map_err(|error| error.to_string())?;
    let decoder = if header.starts_with(b"RIFF") && header.get(8..12) == Some(b"WAVE") {
        Decoder::new_wav(file)
    } else if header.starts_with(b"fLaC") {
        Decoder::new_flac(file)
    } else if header.starts_with(b"OggS") {
        Decoder::new_vorbis(file)
    } else {
        // FFmpeg applies container edit lists, which distinguish AAC priming
        // samples from the audio clock used by captions and word markers.
        if ffmpeg::is_iso_media(path, header)
            && let Some(format) = ffmpeg::blocks(path, &mut consume)?
        {
            return Ok(format);
        }
        // Rodio wraps the reader as unseekable. ISO BMFF may put its sample
        // index after media data, in which case the demuxer must seek.
        let mut file = file.into_inner();
        file.rewind().map_err(|error| error.to_string())?;
        return symphonia_blocks(path, file, consume)
            .map_err(|error| format!("Failed to decode audio '{path}': {error}"));
    };
    let decoder = decoder.map_err(|error| format!("Failed to decode audio '{path}': {error}"))?;
    let format = AudioFormat {
        sample_rate: decoder.sample_rate(),
        channels: decoder.channels(),
        duration_s: decoder.total_duration().map_or(0.0, |duration| duration.as_secs_f64()),
    };
    let mut decoder = decoder.convert_samples::<f32>();
    let mut chunk = [0.0; 2048];
    loop {
        let mut len = 0;
        for (slot, sample) in chunk.iter_mut().zip(&mut decoder) {
            *slot = sample;
            len += 1;
        }
        if len == 0 {
            break;
        }
        consume(format, &chunk[..len]);
    }
    Ok(format)
}

fn symphonia_blocks(
    path: &str,
    file: File,
    mut consume: impl FnMut(AudioFormat, &[f32]),
) -> Result<AudioFormat, Error> {
    if !file.is_seekable() {
        return Err(Error::Unsupported("audio source must be a seekable file"));
    }
    let mut hint = Hint::new();
    if let Some(extension) = Path::new(path).extension().and_then(|ext| ext.to_str()) {
        hint.with_extension(extension);
    }
    let stream = MediaSourceStream::new(Box::new(file), Default::default());
    let mut reader = symphonia::default::get_probe()
        .format(&hint, stream, &FormatOptions::default(), &MetadataOptions::default())?
        .format;
    let (track_id, mut decoder, duration_s) = reader
        .tracks()
        .iter()
        .find_map(|track| {
            let params = &track.codec_params;
            let decoder =
                symphonia::default::get_codecs().make(params, &DecoderOptions::default()).ok()?;
            let duration_s = params.time_base.zip(params.n_frames).map_or(0.0, |(base, frames)| {
                let time = base.calc_time(frames);
                time.seconds as f64 + time.frac
            });
            Some((track.id, decoder, duration_s))
        })
        .ok_or(Error::Unsupported("no decodable audio track"))?;
    let mut format: Option<AudioFormat> = None;
    let mut interleaved = None::<SampleBuffer<f32>>;
    loop {
        let packet = match reader.next_packet() {
            Ok(packet) => packet,
            Err(Error::IoError(error)) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = decoder.decode(&packet)?;
        let spec = *decoded.spec();
        let channels = spec.channels.count() as u16;
        if channels == 0 || spec.rate == 0 {
            return Err(Error::DecodeError("invalid audio sample layout"));
        }
        let format = *format.get_or_insert(AudioFormat {
            sample_rate: spec.rate,
            channels,
            duration_s,
        });
        if format.sample_rate != spec.rate || format.channels != channels {
            return Err(Error::Unsupported("changing sample layout"));
        }
        let needed = decoded.capacity() * channels as usize;
        if interleaved.as_ref().is_none_or(|buffer| buffer.capacity() < needed) {
            interleaved = Some(SampleBuffer::new(decoded.capacity() as u64, spec));
        }
        if let Some(interleaved) = interleaved.as_mut() {
            interleaved.copy_interleaved_ref(decoded);
            consume(format, interleaved.samples());
        }
    }
    format.ok_or(Error::DecodeError("audio contained no samples"))
}
