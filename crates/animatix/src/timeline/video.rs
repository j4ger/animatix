//! Timeline contract for local video clips. The host owns probing and decoding.

use std::fmt::Debug;
use std::path::Path;
use std::sync::Arc;

use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx, resolve_half_size};
use crate::renderer::error::RenderError;

use super::image::SceneImage;
use super::{AudioSegment, Easing, Timeline, TrackAccessor, Value};

/// Properties of one video asset on its common media clock.
#[derive(Debug, Clone, Copy)]
pub struct VideoMetadata {
    /// Width and height after applying display orientation and pixel aspect ratio.
    pub display_size: [u32; 2],
    /// End time of the video stream, measured from the media origin.
    pub duration_s: f64,
    /// Start time of the selected audio stream on the same clock, if present.
    pub audio_start_s: Option<f64>,
}

/// Renderer-neutral access to a prepared video asset.
pub trait VideoAsset: Debug + Send + Sync {
    /// Display geometry and track timing.
    fn metadata(&self) -> VideoMetadata;
    /// Image displayed at the specified media time, when the track has a frame.
    fn frame_at(&self, source_time_s: f64) -> Result<Option<SceneImage>, String>;
}

/// Host capability for opening local video assets.
pub trait VideoLoader: Send + Sync {
    /// Probe the file and prepare a seekable reader.
    fn open(&self, path: &Path) -> Result<Arc<dyn VideoAsset>, String>;
}

#[derive(Debug, Clone)]
pub(crate) struct VideoClip {
    asset: Arc<dyn VideoAsset>,
    source: String,
    start_s: f64,
    source_in: f64,
    source_out: f64,
    crop: [f64; 4],
    cover: bool,
    hold: bool,
}

impl Timeline {
    /// Media times an asynchronous host must prepare before evaluating a frame.
    /// The flag marks a held sample immediately before the clip's trim end.
    pub fn video_samples_at(&self, time_s: f64) -> impl Iterator<Item = (&str, f64, bool)> + '_ {
        self.world.video_clips.iter().filter_map(move |(_, clip)| {
            clip.source_time(time_s)
                .map(|source_time| (clip.source.as_str(), source_time, time_s >= clip.end_s()))
        })
    }
}

impl VideoClip {
    pub(crate) fn end_s(&self) -> f64 {
        self.start_s + (self.source_out - self.source_in)
    }

    fn source_time(&self, time_s: f64) -> Option<f64> {
        if time_s < self.start_s {
            return None;
        }
        // Even a hold samples strictly inside the trimmed interval.
        if time_s >= self.end_s() {
            return self.hold.then(|| self.source_out.next_down());
        }
        Some((self.source_in + (time_s - self.start_s)).min(self.source_out.next_down()))
    }

    fn command(&self, image: SceneImage, viewport: [f32; 2]) -> Option<RenderCommand> {
        let [width, height] = viewport.map(f64::from);
        if width <= 0.0 || height <= 0.0 {
            return None;
        }
        let [iw, ih] = self.asset.metadata().display_size.map(f64::from);
        let [x, y, w, h] = self.crop;
        let sw = iw * w;
        let sh = ih * h;
        let scale = if self.cover {
            (width / sw).max(height / sh)
        } else {
            (width / sw).min(height / sh)
        };
        let rect = kurbo::Rect::from_center_size((0.0, 0.0), (sw * scale, sh * scale));
        let clip = rect.intersect(kurbo::Rect::from_center_size((0.0, 0.0), (width, height)));
        Some(RenderCommand::ClippedImage {
            image,
            size: [(iw * scale) as f32, (ih * scale) as f32],
            offset: [rect.x0 - x * iw * scale, rect.y0 - y * ih * scale],
            clip,
        })
    }
}

pub(crate) fn evaluate(ctx: &EvaluateCtx<'_>) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let Some(clip) = ctx.world.and_then(|world| world.video_clips.get(ctx.id)) else {
        return Ok(None);
    };
    let Some(source_time) = clip.source_time(ctx.time_s) else {
        return Ok(None);
    };
    let image = clip.asset.frame_at(source_time).map_err(RenderError::VideoDecode)?;
    let size = resolve_half_size(
        ctx.geometry.size.get(ctx.time_ms, super::DEFAULT_LAYOUT_HALF_SIZE),
        ctx.overrides,
    )
    .map(|half| half * 2.0);
    Ok(image.and_then(|image| clip.command(image, size)).map(|command| vec![command]))
}

pub(crate) fn evaluate_builtin(
    ctx: &EvaluateCtx<'_>,
    _text_ctx: Option<&mut TextCompileCtx<'_>>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    evaluate(ctx)
}

pub(crate) fn is_clip_property(property: &str) -> bool {
    matches!(property, "source" | "in_point" | "out_point" | "fit" | "crop" | "end_behavior")
}

struct ClipProps {
    source: String,
    start: f64,
    end: Option<f64>,
    crop: [f64; 4],
    cover: bool,
    hold: bool,
    size: Option<[f32; 2]>,
    volume: f32,
}

impl Default for ClipProps {
    fn default() -> Self {
        Self {
            source: String::new(),
            start: 0.0,
            end: None,
            crop: [0.0, 0.0, 1.0, 1.0],
            cover: false,
            hold: false,
            size: None,
            volume: 0.0,
        }
    }
}

impl ClipProps {
    fn apply(&mut self, name: &str, value: Value) -> Result<(), String> {
        match (name, value) {
            ("source", Value::Str(path)) => self.source = path,
            ("in_point", Value::Duration(start)) => self.start = start,
            ("out_point", Value::Duration(end)) => self.end = Some(end),
            ("fit", Value::Str(fit)) if matches!(fit.as_str(), "contain" | "cover") => {
                self.cover = fit == "cover";
            },
            ("end_behavior", Value::Str(end)) if matches!(end.as_str(), "hide" | "hold") => {
                self.hold = end == "hold";
            },
            ("crop", Value::Vec4(crop)) if valid_crop(crop) => self.crop = crop,
            ("size", Value::Vec2(size)) if size.iter().all(|n| n.is_finite() && *n > 0.0) => {
                self.size = Some(size.map(|n| n as f32 * 0.5));
            },
            ("volume", Value::Num(gain)) if gain.is_finite() => {
                self.volume = gain.clamp(0.0, 1.0) as f32;
            },
            ("fit", _) => return Err("Video.fit expects \"contain\" or \"cover\"".into()),
            ("end_behavior", _) => {
                return Err("Video.end_behavior expects \"hide\" or \"hold\"".into());
            },
            ("crop", _) => {
                return Err("Video.crop expects a positive rectangle within (0, 0, 1, 1)".into());
            },
            ("size", _) => return Err("Video.size expects positive dimensions".into()),
            (name, _) => return Err(format!("Invalid Video.{name} value")),
        }
        Ok(())
    }
}

fn valid_crop([x, y, width, height]: [f64; 4]) -> bool {
    x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0 && x + width <= 1.0 && y + height <= 1.0
}

#[allow(clippy::too_many_arguments)] // BehaviorDef build callback signature.
pub(crate) fn build_extra(
    timeline: &mut Timeline,
    label: &str,
    ty: &'static str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    let invalid = |message: &str| {
        vec![
            Diagnostic::error(
                DiagnosticCode::VideoClipFailure,
                DiagnosticPhase::Build,
                message.to_owned(),
            )
            .with_subject(label),
        ]
    };
    if !modifiers.is_empty() {
        return Err(invalid(
            "Video starts at its keyframe. Set the source range with in_point and out_point.",
        ));
    }
    if !children.is_empty() {
        return Err(invalid("Video is a leaf actor. Put overlays beside it in a Group or Stack."));
    }
    if timeline.world.get_id(label).is_some() {
        return Err(invalid(
            "Declare a separate Video actor for each clip. Animate visual properties with assignments.",
        ));
    }

    let env = timeline.build_eval_env(time_ms as u64);
    let mut clip_props = ClipProps::default();
    let mut visual_props = Vec::new();
    for prop in props {
        match prop.name.as_str() {
            "source" | "in_point" | "out_point" | "crop" | "fit" | "end_behavior" | "size"
            | "volume" => {
                let subject = format!("{label}.{}", prop.name);
                let Some(value) = super::lookup::evaluate_expr_with_lookup_diagnostic(
                    &prop.value,
                    &env,
                    diagnostics,
                    &subject,
                ) else {
                    return Err(invalid(&format!("Could not evaluate Video.{}", prop.name)));
                };
                clip_props.apply(&prop.name, value).map_err(|message| invalid(&message))?;
            },
            _ => visual_props.push(prop.clone()),
        }
    }
    if clip_props.source.is_empty() {
        return Err(invalid("Video requires a local source file"));
    }
    let (path, asset) = Arc::make_mut(&mut timeline.asset_cache)
        .load_video_for(&clip_props.source, label)
        .map_err(|message| {
            vec![
                Diagnostic::error(
                    DiagnosticCode::VideoClipFailure,
                    DiagnosticPhase::Build,
                    message,
                )
                .with_subject(label)
                .with_path(&clip_props.source),
            ]
        })?;
    let metadata = asset.metadata();
    let requested_end = clip_props.end.unwrap_or(metadata.duration_s);
    let end = requested_end.min(metadata.duration_s);
    let start = clip_props.start;
    if !start.is_finite()
        || !requested_end.is_finite()
        || start < 0.0
        || end <= start
        || requested_end > metadata.duration_s + 0.000_001
    {
        return Err(invalid(&format!(
            "Video range must satisfy 0s <= in_point < out_point <= {:.6}s",
            metadata.duration_s,
        )));
    }

    timeline.process_media_actor_decl(
        ty,
        label,
        &visual_props,
        &[],
        time_ms,
        parent_label,
        diagnostics,
    );
    let id = timeline
        .world
        .get_id(label)
        .ok_or_else(|| invalid("Could not create Video actor"))?;
    let size = clip_props.size.unwrap_or(metadata.display_size.map(|n| n as f32 * 0.5));
    let geometry = timeline.world.geometry_mut(id);
    geometry.size.ensure(size).add_keyframe(time_ms as u64, size, Easing::Linear);
    geometry
        .ensure_layout_size(size)
        .add_keyframe(time_ms as u64, size, Easing::Linear);
    geometry.volume = Some(super::PropertyTrack::new(clip_props.volume));
    let start_s = time_ms / 1000.0;
    timeline.world.video_clips.insert(
        id,
        VideoClip {
            asset,
            source: path.clone(),
            start_s,
            source_in: start,
            source_out: end,
            crop: clip_props.crop,
            cover: clip_props.cover,
            hold: clip_props.hold,
        },
    );

    if let Some(audio_start) = metadata.audio_start_s {
        let first_sample = start.max(audio_start);
        let audio_end = timeline
            .declared_duration_s
            .map_or(end, |scene_end| end.min(start + scene_end - start_s));
        if first_sample < audio_end {
            // FFmpeg interprets colon-prefixed relative names as protocols.
            let source =
                std::fs::canonicalize(&path).map_or(path, |path| path.display().to_string());
            timeline.audio_segments.push(AudioSegment {
                actor_id: Some(id),
                source,
                start_time_s: start_s + first_sample - start,
                source_offset_s: first_sample - audio_start,
                duration_s: Some(audio_end - first_sample),
                volume: clip_props.volume,
                volume_envelope: None,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "video_tests.rs"]
mod tests;
