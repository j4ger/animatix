//! The [`Primitive`] trait and the shared helpers its implementations call.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::renderer::error::RenderError;
use crate::renderer::text::{TextCompileRequest, TextStyle};
use crate::timeline::{
    ActorCaps, ActorCategory, DEFAULT_WHITE, Environment, SceneDimensions, Value, VectorShapeState,
    VelloPath,
};

use super::commands::RenderCommand;
use super::{
    AssignmentCtx, BuildCtx, CaptionCue, EquationFragment, EvaluateCtx, RenderChildrenCtx,
    RenderCtx, TextCompileCtx,
};

/// One frame's compiled text geometry and its optional clip path.
type TextPathOutcome = (std::sync::Arc<[crate::renderer::types::TextPath]>, Option<kurbo::BezPath>);

/// Frame-sampled text inputs shared by the compile paths below.
struct TextFrameValues {
    content: String,
    font_family: String,
    font_size: f32,
    font_weight: f32,
    font_style: String,
    line_height: f32,
    letter_spacing: f32,
    word_spacing: f32,
    max_width: f32,
    text_align: String,
    text_anchor: String,
    overflow: String,
    color: [f32; 4],
    language: String,
    content_override: Option<String>,
}

impl TextFrameValues {
    /// One style for every compile in a frame; the content and wrapping policy
    /// differ per call, typography does not.
    fn text_style(&self) -> TextStyle<'_> {
        TextStyle {
            font_family: &self.font_family,
            font_size: self.font_size,
            font_weight: self.font_weight,
            font_style: &self.font_style,
            line_height: self.line_height,
            letter_spacing: self.letter_spacing,
            word_spacing: self.word_spacing,
            color: self.color,
        }
    }
}

/// Sample the text tracks and modifier overrides for this frame.
fn text_frame_values(
    ctx: &EvaluateCtx,
    text: &crate::timeline::animation_track::TextTracks,
    default_font_size: f32,
) -> TextFrameValues {
    use crate::timeline::TrackAccessor;

    let style = ctx.style;
    let mut content = text.text_content.get(ctx.time_ms, String::new());
    let mut font_family = text.font_family.get(ctx.time_ms, String::new());
    let mut font_size = text.font_size.get(ctx.time_ms, default_font_size);
    let mut font_weight = text.font_weight.get(ctx.time_ms, 400.0);
    let mut font_style = text.font_style.get(ctx.time_ms, "normal".to_string());
    let mut line_height = text.line_height.get(ctx.time_ms, 1.2);
    let mut letter_spacing = text.letter_spacing.get(ctx.time_ms, 0.0);
    let mut word_spacing = text.word_spacing.get(ctx.time_ms, 0.0);
    let mut max_width = text.text_max_width.get(ctx.time_ms, 0.0);
    let mut text_align = text.text_align.get(ctx.time_ms, "left".to_string());
    let mut text_anchor = text.text_anchor.get(ctx.time_ms, "center".to_string());
    let mut overflow = text.overflow.get(ctx.time_ms, "visible".to_string());
    let mut color = style.color.get(ctx.time_ms, DEFAULT_WHITE);
    let mut language = text.language.get(ctx.time_ms, String::new());

    let mut content_override: Option<String> = None;
    if let Some(ov) = ctx.overrides {
        if let Some(Value::Str(s)) = ov.get("text").or_else(|| ov.get("code")) {
            content_override = Some(s.clone());
            content = s.clone();
        }
        if let Some(Value::Str(s)) = ov.get("font_family") {
            font_family = s.clone();
        }
        if let Some(Value::Num(n)) = ov.get("font_size") {
            font_size = *n as f32;
        }
        if let Some(Value::Num(n)) = ov.get("font_weight") {
            font_weight = *n as f32;
        }
        if let Some(Value::Str(s)) = ov.get("font_style") {
            font_style = s.clone();
        }
        if let Some(Value::Num(n)) = ov.get("line_height") {
            line_height = *n as f32;
        }
        if let Some(Value::Num(n)) = ov.get("letter_spacing") {
            letter_spacing = *n as f32;
        }
        if let Some(Value::Num(n)) = ov.get("word_spacing") {
            word_spacing = *n as f32;
        }
        // `text_max_width` is the canonical descriptor name and `max_width` the
        // legacy alias; both must be read or an `always` write of the canonical key
        // is silently ignored. Same two-name handling as `text`/`code` above.
        if let Some(Value::Num(n)) = ov.get("text_max_width").or_else(|| ov.get("max_width")) {
            max_width = *n as f32;
        }
        if let Some(Value::Str(s)) = ov.get("text_align") {
            text_align = s.clone();
        }
        if let Some(Value::Str(s)) = ov.get("text_anchor") {
            text_anchor = s.clone();
        }
        if let Some(Value::Str(s)) = ov.get("overflow") {
            overflow = s.clone();
        }
        if let Some(Value::Color(c) | Value::Vec4(c)) = ov.get("color") {
            color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
        }
        if let Some(Value::Str(s)) = ov.get("language") {
            language = s.clone();
        }
    }

    TextFrameValues {
        content,
        font_family,
        font_size,
        font_weight,
        font_style,
        line_height,
        letter_spacing,
        word_spacing,
        max_width,
        text_align,
        text_anchor,
        overflow,
        color,
        language,
        content_override,
    }
}

/// Cross-fade keyframed content by compiling both endpoint strings.
///
/// Returns `None` when the frame has an explicit content override or is not
/// inside a content-segment transition.
fn crossfade_text_paths(
    ctx: &EvaluateCtx,
    text_ctx: &mut TextCompileCtx,
    kind: crate::renderer::text::TextKind,
    text: &crate::timeline::animation_track::TextTracks,
    values: &TextFrameValues,
) -> Result<Option<TextPathOutcome>, crate::renderer::error::RenderError> {
    // An explicit empty-string override means "no visible content", not "keep the
    // cached build-time glyphs", so `always { box.text = "" }` is deterministic.
    if values.content_override.is_some() {
        return Ok(None);
    }
    // Keyframed content swaps cross-fade by compiling both endpoint strings;
    // without this the String track snaps at the midpoint.
    let Some((source_text, target_text, progress)) = text
        .text_content
        .as_ref()
        .and_then(|track| track.interpolation_segment(ctx.time_ms))
        .map(|(_, prev, found, raw_progress, easing)| {
            let eased = crate::easing::apply_easing(raw_progress, *easing);
            (prev.clone(), found.clone(), eased)
        })
    else {
        return Ok(None);
    };
    if source_text == target_text {
        return Ok(None);
    }

    let mut request = TextCompileRequest {
        kind,
        content: &source_text,
        language: &values.language,
        palette: text_ctx.text_compiler.highlight_palette.as_ref(),
        allow_fast_path: text_ctx.text_compiler.text_fast_path,
        style: values.text_style(),
        max_width: values.max_width,
        max_lines: None,
        text_align: &values.text_align,
        overflow: &values.overflow,
    };
    let source_paths = text_ctx.text_compiler.compile_request(&request, text_ctx.font_context)?;
    request.content = &target_text;
    let target_paths = text_ctx.text_compiler.compile_request(&request, text_ctx.font_context)?;
    let source_half = crate::renderer::text::measure_text_paths(&source_paths);
    let source_offset = match values.text_anchor.as_str() {
        "left" => source_half[0],
        "right" => -source_half[0],
        _ => 0.0,
    };
    let mut source_vec = source_paths.to_vec();
    if source_offset != 0.0 {
        let affine = kurbo::Affine::translate((source_offset as f64, 0.0));
        let offset_vec = kurbo::Vec2::new(source_offset as f64, 0.0);
        for p in &mut source_vec {
            p.path.apply_affine(affine);
            p.glyph_center[0] += source_offset;
            p.bounds = p.bounds + offset_vec;
        }
    }

    let target_half = crate::renderer::text::measure_text_paths(&target_paths);
    let target_offset = match values.text_anchor.as_str() {
        "left" => target_half[0],
        "right" => -target_half[0],
        _ => 0.0,
    };
    let mut target_vec = target_paths.to_vec();
    if target_offset != 0.0 {
        let affine = kurbo::Affine::translate((target_offset as f64, 0.0));
        let offset_vec = kurbo::Vec2::new(target_offset as f64, 0.0);
        for p in &mut target_vec {
            p.path.apply_affine(affine);
            p.glyph_center[0] += target_offset;
            p.bounds = p.bounds + offset_vec;
        }
    }

    let mut crossfaded = crate::timeline::interpolate_text_paths(
        &source_vec,
        &target_vec,
        progress,
        crate::timeline::MorphOptions {
            strategy: crate::timeline::MorphStrategy::Fade,
            ..Default::default()
        },
    );
    let clip =
        crate::timeline::dispatch::apply_split_text_animation(&mut crossfaded, text, ctx.time_ms);
    Ok(Some((std::sync::Arc::from(crossfaded), clip)))
}

/// Remaining frame-time compile paths, in the original priority order: cached
/// glyphs, a fresh compile from the sampled content, then the track fallback.
fn evaluate_uncached_text_paths(
    ctx: &EvaluateCtx,
    text_ctx: &mut TextCompileCtx,
    kind: crate::renderer::text::TextKind,
    text: &crate::timeline::animation_track::TextTracks,
    values: &TextFrameValues,
) -> Result<TextPathOutcome, crate::renderer::error::RenderError> {
    use crate::timeline::TrackAccessor;

    let style = ctx.style;
    if values.content_override.is_none() && text.text_paths.is_some() {
        let (paths, clip) = text.evaluate_paths_and_clip(style, ctx.time_ms);
        let track_anchor = text.text_anchor.get(ctx.time_ms, "center".to_string());
        if values.text_anchor != track_anchor && !paths.is_empty() {
            let half_size = crate::renderer::text::measure_text_paths(&paths);
            let old_offset = match track_anchor.as_str() {
                "left" => half_size[0],
                "right" => -half_size[0],
                _ => 0.0,
            };
            let new_offset = match values.text_anchor.as_str() {
                "left" => half_size[0],
                "right" => -half_size[0],
                _ => 0.0,
            };
            let delta_x = new_offset - old_offset;
            if delta_x != 0.0 {
                let affine = kurbo::Affine::translate((delta_x as f64, 0.0));
                let offset_vec = kurbo::Vec2::new(delta_x as f64, 0.0);
                let mut shifted = paths.clone();
                for p in &mut shifted {
                    p.path.apply_affine(affine);
                    p.glyph_center[0] += delta_x;
                    p.bounds = p.bounds + offset_vec;
                }
                return Ok((std::sync::Arc::from(shifted), clip));
            }
        }
        Ok((std::sync::Arc::from(paths), clip))
    } else if values.content_override.is_some() || !values.content.is_empty() {
        let request = TextCompileRequest {
            kind,
            content: &values.content,
            language: &values.language,
            palette: text_ctx.text_compiler.highlight_palette.as_ref(),
            allow_fast_path: text_ctx.text_compiler.text_fast_path,
            style: values.text_style(),
            max_width: values.max_width,
            max_lines: None,
            text_align: &values.text_align,
            overflow: &values.overflow,
        };
        let compiled = text_ctx.text_compiler.compile_request(&request, text_ctx.font_context)?;
        let mut paths_vec = compiled.to_vec();
        let half_size = crate::renderer::text::measure_text_paths(&paths_vec);
        let anchor_offset_x = match values.text_anchor.as_str() {
            "left" => half_size[0],
            "right" => -half_size[0],
            _ => 0.0,
        };
        if anchor_offset_x != 0.0 {
            let affine = kurbo::Affine::translate((anchor_offset_x as f64, 0.0));
            let offset_vec = kurbo::Vec2::new(anchor_offset_x as f64, 0.0);
            for p in &mut paths_vec {
                p.path.apply_affine(affine);
                p.glyph_center[0] += anchor_offset_x;
                p.bounds = p.bounds + offset_vec;
            }
        }
        let clip = crate::timeline::dispatch::apply_split_text_animation(
            &mut paths_vec,
            text,
            ctx.time_ms,
        );
        Ok((std::sync::Arc::from(paths_vec), clip))
    } else {
        let (paths, clip) = text.evaluate_paths_and_clip(style, ctx.time_ms);
        Ok((std::sync::Arc::from(paths), clip))
    }
}

/// Evaluate text paths for a text primitive at frame time: the single frame-time
/// text compile path for every text-like primitive (Text, Math, Code, Typst).
pub fn evaluate_text_paths(
    ctx: &EvaluateCtx,
    text_ctx: &mut TextCompileCtx,
    kind: crate::renderer::text::TextKind,
    default_font_size: f32,
) -> Result<
    (std::sync::Arc<[crate::renderer::types::TextPath]>, Option<kurbo::BezPath>),
    crate::renderer::error::RenderError,
> {
    use crate::timeline::TrackAccessor;

    let text = ctx.text.expect("text primitive requires text component");
    let values = text_frame_values(ctx, text, default_font_size);

    if let Some(result) = crossfade_text_paths(ctx, text_ctx, kind, text, &values)? {
        return Ok(result);
    }
    if values.content_override.is_none()
        && text.char_progress.is_none()
        && let Some(static_paths) = &text.static_paths
    {
        let track_anchor = text.text_anchor.get(ctx.time_ms, "center".to_string());
        if values.text_anchor == track_anchor {
            return Ok((std::sync::Arc::clone(static_paths), None));
        }
    }
    evaluate_uncached_text_paths(ctx, text_ctx, kind, text, &values)
}

/// Resolve a shape's half-size (radius) for this frame.
///
/// The sources do not share units: `track_half` is already a **half-size**
/// (`geometry.size` stores half-sizes), a `size` override is the **raw authored
/// bounding box** and is halved here, and `radius_x`/`radius_y` overrides replace
/// one **half-size component** each, applied after `size`. Without the halving,
/// `always { r.size = (100.0, 60.0) }` would paint twice the declared size. A
/// wrongly-typed override keeps the keyframed size; this runs per shape per frame,
/// so the drop logs at debug rather than warn.
pub(crate) fn resolve_half_size(
    track_half: [f32; 2],
    overrides: Option<&std::collections::HashMap<String, Value>>,
) -> [f32; 2] {
    let Some(overrides) = overrides else {
        return track_half;
    };
    let mut half = track_half;
    match overrides.get("size") {
        Some(Value::Vec2([w, h])) => half = [*w as f32 / 2.0, *h as f32 / 2.0],
        Some(other) => {
            tracing::debug!(
                "`size` override is not a (w, h) pair: {other:?}; keeping keyframed size"
            );
        },
        None => {},
    }
    if let Some(Value::Num(r)) = overrides.get("radius_x") {
        half[0] = *r as f32;
    }
    if let Some(Value::Num(r)) = overrides.get("radius_y") {
        half[1] = *r as f32;
    }
    half
}

/// A container's `gap`/`padding` the layout pass could not read.
///
/// The layout readers accept a number or `(x, y)` and otherwise leave the value
/// alone; without this warning the authored property would vanish silently.
pub(crate) fn unreadable_layout_value(label: &str, prop: &str) -> Diagnostic {
    Diagnostic::warning(
        DiagnosticCode::InvalidPropertyValue,
        DiagnosticPhase::Build,
        format!("{label}.{prop} expects a number or a (x, y) tuple; the value was ignored"),
    )
}

/// Concatenate every vector path emitted by `commands` into one clip geometry.
///
/// Every `Paths` command and subpath contributes, so a multi-path shape used as a
/// `clip_shape` cannot silently lose geometry.
pub(crate) fn clip_bezpath_from_commands(commands: &[RenderCommand]) -> Option<kurbo::BezPath> {
    let mut path = kurbo::BezPath::new();
    for command in commands {
        if let RenderCommand::Paths { paths } = command {
            for vello_path in paths {
                path.extend(vello_path.path.iter());
            }
        }
    }
    (!path.elements().is_empty()).then_some(path)
}

/// The behaviour half of a primitive (build/evaluate/render). The metadata half
/// lives in `animatix-std` for built-ins and in registration info for extensions,
/// keyed by [`Primitive::type_name`].
pub trait Primitive: Send + Sync {
    /// Source-text type name, e.g. "Rect", "Text", "Row" — the registry key for metadata lookup.
    fn type_name(&self) -> &str;

    /// Property names this primitive declares from the built-in or extension
    /// registries. Prefer [`Self::declares_property`] for membership tests rather
    /// than materializing and searching this list.
    fn declared_property_names(&self) -> Vec<&str> {
        Vec::new()
    }

    /// `true` when this primitive declares `name` from a built-in or extension registry.
    fn declares_property(&self, name: &str) -> bool {
        self.declared_property_names().contains(&name)
    }

    /// Build the actor into the timeline.
    ///
    /// Built-ins are lowered by their behavior family's `build_extra` or the generic
    /// actor pipeline, so only runtime-registered extension primitives override this.
    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }

    /// Render the primitive into Vello paths; `None` for non-visual primitives.
    fn render(&self, _ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        None
    }

    /// Local-space clip geometry when this primitive is used as a `Mask`'s
    /// `clip_shape` child.
    ///
    /// Defaults to the primitive's own evaluation output, so any path-producing shape
    /// can clip without an override. Plot geometry is excluded (time-varying and
    /// stroke-trimmed, not a fill region); `None` means "not usable as a clip" and the
    /// caller warns and falls back to a rectangular clip. The path is in local space —
    /// the caller composes the child and mask transforms.
    fn clip_path(&self, ctx: &EvaluateCtx, caps: &ActorCaps) -> Option<kurbo::BezPath> {
        if caps.plot_geometry {
            return None;
        }
        let commands = self.evaluate(ctx, None).ok()??;
        let path = clip_bezpath_from_commands(&commands);
        // Return the memo payload for reuse (shape primitives take/recycle it).
        ctx.world
            .expect("shape memo requires a world")
            .recycle_shape_commands(ctx.id, commands);
        path
    }

    /// Data this primitive contributes to a parent `Equation`'s document, if any.
    fn equation_fragment(&self, _ctx: &EvaluateCtx) -> Option<EquationFragment> {
        None
    }

    /// Data this primitive contributes as a subtitle cue to a parent `Captions` container, if any.
    fn caption_cue(&self, _ctx: &EvaluateCtx) -> Option<CaptionCue> {
        None
    }

    /// Render this primitive's children directly by `EntityId`, avoiding string
    /// lookups on the hot render path. The only child-render entry point the scene
    /// evaluator calls.
    fn render_children_by_id(
        &self,
        ctx: &mut RenderChildrenCtx<'_, '_, '_>,
        children_ids: &[crate::timeline::EntityId],
    ) -> Result<(), RenderError> {
        ctx.render_children_ids(children_ids);
        Ok(())
    }

    /// Apply primitive-specific defaults to the shape state.
    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    /// Apply one property to the shape state; `true` if handled.
    fn apply_property(
        &self,
        _name: &str,
        _value: &Expr,
        _env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
        _state: &mut VectorShapeState,
    ) -> bool {
        false
    }

    /// Finalize the shape state after all properties have been applied.
    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    // Shape-kind questions (`uses_custom_path`, `exposes_tip_size`, `supports_fill`)
    // live as `ShapeType`-based free functions in `timeline/shapes/mod.rs`, because
    // the GUI edit-vertices layer only has a `ShapeType`.

    /// Colorscheme key for default color lookup (e.g. "text.primary", "accent.primary").
    fn default_color_key(&self, property: &str, caps: &ActorCaps) -> Option<&'static str> {
        match property {
            "color" => match caps.category {
                ActorCategory::Text => Some("text.primary"),
                ActorCategory::Shape | ActorCategory::Plot => Some("surface.primary"),
                ActorCategory::Media => Some("text.primary"),
                ActorCategory::Container => None,
                ActorCategory::Annotation => None,
            },
            "stroke" => match caps.category {
                ActorCategory::Shape => Some("stroke.default"),
                _ => None,
            },
            _ => None,
        }
    }

    /// How the GUI should resize this actor.
    fn resize_mode(&self, caps: &ActorCaps) -> crate::timeline::ResizeMode {
        match caps.category {
            ActorCategory::Text | ActorCategory::Media | ActorCategory::Plot => {
                crate::timeline::ResizeMode::Scale
            },
            _ => crate::timeline::ResizeMode::Size,
        }
    }

    /// Default properties used when creating this actor from the GUI.
    fn default_props(&self, _scene_dimensions: &SceneDimensions) -> Vec<Property> {
        vec![]
    }

    /// Assignment-phase handler; return `true` if handled (bypassing the generic
    /// engine). The default `false` delegates to it.
    fn handle_assignment(
        &self,
        _property: &str,
        _value: &Expr,
        _ctx: &mut AssignmentCtx,
        _env: &Environment,
        _diagnostics: &mut Vec<Diagnostic>,
        _subject: &str,
    ) -> bool {
        false
    }

    /// Finalize the actor build after all children are processed.
    ///
    /// Layout containers (Row, Col, Grid, Stack) override this so child tracks exist
    /// before layout runs.
    fn finalize_container_build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }

    /// Evaluate this primitive at frame time and return render commands.
    ///
    /// Built-in primitives dispatch through `BehaviorDef` family function pointers;
    /// this method is the extension-adapter surface for runtime-registered primitives.
    /// `Ok(Some)` is executed with the actor's local transform and inherited opacity,
    /// and records a hit region from local bounds; `Ok(None)` draws nothing and records
    /// no hit region or bounds (empty-content actors stay un-pickable, pinned by
    /// `runtime_empty_text_override_clears_stale_glyphs`); `Err` records a
    /// `RenderFailure` runtime diagnostic.
    fn evaluate(
        &self,
        _ctx: &EvaluateCtx,
        _text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        Ok(None)
    }
}
