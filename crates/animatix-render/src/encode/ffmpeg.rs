//! `ffmpeg` discovery and CLI-based audio muxing for the export paths.

use std::sync::OnceLock;

use super::ExportError;

static FFMPEG_AVAILABLE: OnceLock<bool> = OnceLock::new();

/// Check whether `ffmpeg` is available on PATH. Result is cached after first call.
pub fn is_ffmpeg_available() -> bool {
    *FFMPEG_AVAILABLE.get_or_init(|| {
        std::process::Command::new("ffmpeg")
            .arg("-version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    })
}

/// Returns `Ok(())` if ffmpeg is available, `Err(ExportError::FfmpegNotFound)` otherwise.
pub fn require_ffmpeg() -> Result<(), ExportError> {
    if is_ffmpeg_available() {
        Ok(())
    } else {
        Err(ExportError::FfmpegNotFound)
    }
}

/// Mux audio segments into the rendered video using ffmpeg CLI.
///
/// Each segment is positioned on the global timeline via `adelay`, optionally
/// trimmed to `duration_s` via `atrim`, and its volume scaled.  All segments
/// are then mixed together with `amix` so overlapping audio (e.g. background
/// music + voiceover) blends correctly.
///
/// Creates a temporary file and renames it on success.
pub fn mux_audio_segments(
    video_path: &std::path::Path,
    segments: &[animatix::timeline::AudioSegment],
    output_path: &std::path::Path,
) -> Result<(), ExportError> {
    if segments.is_empty() {
        return Ok(());
    }

    require_ffmpeg()?;

    let is_webm = output_path.extension().map(|e| e == "webm").unwrap_or(false);

    let temp_ext = if is_webm {
        "tmp_muxed.webm"
    } else {
        "tmp_muxed.mp4"
    };
    let temp_path = output_path.with_extension(temp_ext);
    let audio_codec = if is_webm { "libopus" } else { "aac" };

    // Build a filter_complex that positions, trims, and volumes each segment,
    // then mixes them all together.
    let mut filter_chains: Vec<String> = Vec::with_capacity(segments.len());
    let mut mix_labels: Vec<String> = Vec::with_capacity(segments.len());

    for (i, seg) in segments.iter().enumerate() {
        let input_idx = i + 1; // 0 is the video file
        let label = format!("[a{i}]");
        let delay_ms = (seg.start_time_s * 1000.0).round() as i64;
        let delay_ms = delay_ms.max(0);

        let mut chain = format!("[{input_idx}:a:0]");

        // Trim to declared duration if specified.
        if let Some(dur) = seg.duration_s {
            chain.push_str(&format!("atrim=end={:.3},", dur));
        }

        // Apply per-segment volume: if an animated envelope exists, render an ffmpeg volume expression
        if let Some(env) = &seg.volume_envelope {
            if env.len() >= 2 {
                // ffmpeg volume filter supports expression: 'volume=...:eval=frame'
                // where 't' is time in seconds relative to the audio stream.
                // We construct an if(between(t,...), ...) linear interpolation expression.
                let mut expr = format!("{:.3}", env.last().map(|(_, v)| *v).unwrap_or(seg.volume));
                for i in (0..env.len() - 1).rev() {
                    let (t0_glob, v0) = env[i];
                    let (t1_glob, v1) = env[i + 1];
                    let t0 = (t0_glob - seg.start_time_s).max(0.0);
                    let t1 = (t1_glob - seg.start_time_s).max(0.0);
                    if (t1 - t0) > 1e-4 {
                        let lerp = format!(
                            "({:.3}+({:.3}-{:.3})*(t-{:.3})/{:.3})",
                            v0,
                            v1,
                            v0,
                            t0,
                            t1 - t0
                        );
                        expr = format!("if(between(t,{:.3},{:.3}),{},{})", t0, t1, lerp, expr);
                    }
                }
                chain.push_str(&format!("volume='{}':eval=frame,", expr));
            } else {
                chain.push_str(&format!("volume={:.3},", seg.volume));
            }
        } else {
            chain.push_str(&format!("volume={:.3},", seg.volume));
        }

        // Delay to global timeline position.
        chain.push_str(&format!("adelay=delays={delay_ms}"));

        chain.push_str(&label);
        filter_chains.push(chain);
        mix_labels.push(label);
    }

    let mix_filter = format!(
        "{}amix=inputs={}:duration=longest:normalize=0[outa]",
        mix_labels.join(""),
        segments.len()
    );
    filter_chains.push(mix_filter);
    let filter_complex = filter_chains.join(";");

    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.arg("-y");
    cmd.arg("-i").arg(video_path);
    for seg in segments {
        cmd.arg("-i").arg(&seg.source);
    }
    cmd.arg("-filter_complex").arg(&filter_complex);
    cmd.arg("-map").arg("0:v:0");
    cmd.arg("-map").arg("[outa]");
    cmd.arg("-c:v").arg("copy");
    cmd.arg("-c:a").arg(audio_codec);
    cmd.arg("-shortest");
    cmd.arg(&temp_path);

    tracing::debug!("ffmpeg audio mux command: {:?}", cmd);

    let status = cmd.status().map_err(|e| {
        ExportError::VideoEncode(format!("Failed to run ffmpeg for audio muxing: {e}"))
    })?;

    if !status.success() {
        std::fs::remove_file(&temp_path).ok();
        return Err(ExportError::VideoEncode("ffmpeg audio muxing failed".into()));
    }

    // Replace original with muxed version
    if let Err(e) = std::fs::rename(&temp_path, output_path) {
        std::fs::remove_file(&temp_path).ok();
        return Err(ExportError::VideoEncode(format!(
            "Failed to replace video with muxed version: {e}"
        )));
    }

    tracing::info!("Audio muxed {} segment(s) into {}", segments.len(), output_path.display());
    Ok(())
}
