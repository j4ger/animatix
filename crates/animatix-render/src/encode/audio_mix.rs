use std::ffi::CString;
use std::path::Path;

use animatix::timeline::AudioSegment;
use rsmpeg::avformat::AVFormatContextOutput;

use super::{ExportError, audio::AudioEncoder};

/// Encode the full timeline's mixed audio as AAC in an M4A container, without rendering frames.
pub fn render_mixed_audio(
    segments: &[AudioSegment],
    duration_s: f64,
    output: &Path,
    warn: impl Fn(String),
) -> Result<(), ExportError> {
    if !duration_s.is_finite() || duration_s <= 0.0 {
        return Err(ExportError::VideoEncode("Duration must be positive".into()));
    }
    if !output.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("m4a")) {
        return Err(ExportError::VideoEncode("Audio output must have an .m4a extension".into()));
    }
    let parent = output.parent().filter(|p| !p.as_os_str().is_empty());
    let temporary = tempfile::Builder::new()
        .prefix(".animatix-")
        .suffix(".m4a")
        .tempfile_in(parent.unwrap_or_else(|| Path::new(".")))?;
    let filename = CString::new(
        temporary
            .path()
            .to_str()
            .ok_or_else(|| ExportError::VideoEncode("Invalid audio output path".into()))?,
    )?;
    let media_error = |err| ExportError::VideoEncode(format!("Audio muxing failed: {err}"));
    let mut container = AVFormatContextOutput::create(&filename).map_err(media_error)?;
    let mut audio =
        AudioEncoder::prepare(&mut container, segments, 0.0..duration_s, false, true, |event| {
            warn(event.to_string())
        })?
        .ok_or_else(|| {
            ExportError::VideoEncode("No audio starts within the export duration".into())
        })?;
    container.write_header(&mut None).map_err(media_error)?;
    audio.finish(&mut container)?;
    container.write_trailer().map_err(media_error)?;
    drop(container);
    temporary.persist(output).map_err(|err| err.error)?;
    Ok(())
}
