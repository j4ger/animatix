//! Stream PCM through FFmpeg for ISO BMFF files with edit lists.

use std::io::{self, BufReader, Read};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use super::decode::AudioFormat;

pub(in crate::app) fn packaged_executable() -> Option<PathBuf> {
    let executable = std::env::current_exe().ok()?;
    let executable_dir = executable.parent()?;
    let launcher_dir = executable_dir.parent()?.join("bin");
    let name = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    [executable_dir, launcher_dir.as_path()]
        .into_iter()
        .map(|dir| dir.join(name))
        .find(|path| path.is_file())
}

pub(super) fn is_iso_media(path: &str, header: &[u8]) -> bool {
    matches!(header.get(4..8), Some(b"ftyp" | b"moov" | b"mdat"))
        || Path::new(path).extension().and_then(|ext| ext.to_str()).is_some_and(|ext| {
            ["mp4", "m4a", "m4b", "m4p", "m4r", "mov", "3gp", "3g2"]
                .iter()
                .any(|supported| ext.eq_ignore_ascii_case(supported))
        })
}

pub(super) fn blocks(
    path: &str,
    mut consume: impl FnMut(AudioFormat, &[f32]),
) -> Result<Option<AudioFormat>, String> {
    let path = std::path::absolute(path).map_err(|error| error.to_string())?;
    let programs = packaged_executable().into_iter().chain([PathBuf::from("ffmpeg")]);
    for program in programs {
        let mut command = Command::new(&program);
        command
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-protocol_whitelist",
                "file,pipe",
                "-i",
            ])
            .arg(&path)
            .args(["-map", "0:a:0", "-c:a", "pcm_f32le", "-f", "wav", "pipe:1"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                tracing::debug!(
                    "FFmpeg audio streaming unavailable at {}: {error}",
                    program.display()
                );
                continue;
            },
        };
        let mut emitted_pcm = false;
        let result = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("FFmpeg output pipe could not be opened"))
            .and_then(|stdout| {
                read_pcm(BufReader::new(stdout), |format, chunk| {
                    emitted_pcm = true;
                    consume(format, chunk);
                })
            });
        if result.is_err() {
            let _ = child.kill();
        }
        let status = child.wait().map_err(|error| format!("Could not wait for FFmpeg: {error}"))?;
        match result {
            Ok(format) if status.success() => return Ok(Some(format)),
            result if !emitted_pcm => {
                tracing::debug!(
                    "FFmpeg could not start PCM streaming ({status}): {:?}",
                    result.err()
                );
            },
            Err(error) => return Err(format!("Could not read FFmpeg audio stream: {error}")),
            Ok(_) => return Err(format!("FFmpeg audio streaming failed with {status}")),
        }
    }
    Ok(None)
}

fn read_pcm(
    mut reader: impl Read,
    mut consume: impl FnMut(AudioFormat, &[f32]),
) -> io::Result<AudioFormat> {
    let mut format = read_wav_header(&mut reader)?;
    let mut bytes = [0; 8192];
    let mut samples = [0.0; 2048];
    let mut remainder = 0;
    let mut sample_count = 0u64;
    loop {
        let read = reader.read(&mut bytes[remainder..])?;
        if read == 0 {
            break;
        }
        let available = remainder + read;
        let complete = available / 4;
        for (sample, frame) in samples.iter_mut().zip(bytes[..complete * 4].as_chunks::<4>().0) {
            *sample = f32::from_le_bytes(*frame);
        }
        if complete > 0 {
            consume(format, &samples[..complete]);
            sample_count += complete as u64;
        }
        bytes.copy_within(complete * 4..available, 0);
        remainder = available % 4;
    }
    if remainder != 0
        || sample_count == 0
        || !sample_count.is_multiple_of(u64::from(format.channels))
    {
        return Err(invalid("FFmpeg returned incomplete PCM frames"));
    }
    format.duration_s =
        sample_count as f64 / (f64::from(format.sample_rate) * f64::from(format.channels));
    Ok(format)
}

fn read_wav_header(reader: &mut impl Read) -> io::Result<AudioFormat> {
    let mut riff = [0; 12];
    reader.read_exact(&mut riff)?;
    if &riff[..4] != b"RIFF" || &riff[8..] != b"WAVE" {
        return Err(invalid("FFmpeg did not return a WAV stream"));
    }
    let mut format = None;
    loop {
        let mut header = [0; 8];
        reader.read_exact(&mut header)?;
        let len = u64::from(u32::from_le_bytes([header[4], header[5], header[6], header[7]]));
        // FFmpeg emits unknown RIFF and data sizes when writing to a pipe.
        if &header[..4] == b"data" {
            return format.ok_or_else(|| invalid("FFmpeg WAV stream has no format chunk"));
        }
        let payload_left = if &header[..4] == b"fmt " && len >= 16 {
            let mut details = [0; 16];
            reader.read_exact(&mut details)?;
            let encoding = u16::from_le_bytes([details[0], details[1]]);
            let channels = u16::from_le_bytes([details[2], details[3]]);
            let sample_rate = u32::from_le_bytes([details[4], details[5], details[6], details[7]]);
            let sample_bits = u16::from_le_bytes([details[14], details[15]]);
            if !matches!(encoding, 3 | 0xfffe)
                || channels == 0
                || sample_rate == 0
                || sample_bits != 32
            {
                return Err(invalid("FFmpeg WAV stream is not interleaved f32 PCM"));
            }
            format = Some(AudioFormat {
                sample_rate,
                channels,
                duration_s: 0.0,
            });
            len - 16
        } else {
            len
        };
        let skip = payload_left + len % 2;
        if io::copy(&mut reader.by_ref().take(skip), &mut io::sink())? != skip {
            return Err(invalid("FFmpeg WAV stream ended inside a header"));
        }
    }
}

fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
