//! The [`Primitive`] trait and the shared helpers its implementations call.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::renderer::error::RenderError;
use crate::timeline::{
    ActorCaps, ActorCategory, DEFAULT_WHITE, Environment, SceneDimensions, Value, VectorShapeState,
    VelloPath,
};

use super::commands::RenderCommand;
use super::{
    AssignmentCtx, BuildCtx, CaptionCue, EquationFragment, EvaluateCtx, RenderChildrenCtx,
    RenderCtx, TextCompileCtx,
};

/// Evaluate text paths for a text primitive at frame time.
///
/// This is the single frame-time text compile path: every text-like
/// primitive (Text, Math, Code, Typst) dispatches through
/// [`Primitive::evaluate`] into this function. It supersedes the former
/// inline `evaluate_text_node` logic in `scene_eval.rs` (removed during the
/// trait-dispatch migration).
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

    let style = ctx.style;
    let text = ctx.text.expect("text primitive requires text component");
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
        // `text_max_width` is the canonical descriptor name (animatix-core) and
        // `max_width` the legacy alias; the frame path used to read only the
        // alias, so `always { t.text_max_width = 60 }` wrote an override key
        // nobody consumed — silently, because the canonical row is flagged
        // ANIMATED. Same two-name handling as `text`/`code` above.
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
    // An explicit empty-string override means "no visible content", not "keep
    // the cached build-time glyphs". This makes `always { box.text = "" }`
    // deterministic instead of silently falling back to stale paths.
    if content_override.is_none() {
        // Keyframed content swaps are cross-faded by compiling both endpoint
        // strings and rendering them at partial opacity. Without this, the
        // String track snaps at the midpoint before the compiler runs.
        if let Some((source_text, target_text, progress)) = text
            .text_content
            .as_ref()
            .and_then(|track| track.interpolation_segment(ctx.time_ms))
            .map(|(_, prev, found, raw_progress, easing)| {
                let eased = crate::easing::apply_easing(raw_progress, *easing);
                (prev.clone(), found.clone(), eased)
            })
        {
            if source_text != target_text {
                let source_paths = text_ctx.text_compiler.compile(
                    &source_text,
                    &font_family,
                    font_size,
                    font_weight,
                    &font_style,
                    line_height,
                    letter_spacing,
                    word_spacing,
                    color,
                    kind,
                    &language,
                    text_ctx.font_context,
                    max_width,
                    &text_align,
                    &overflow,
                )?;
                let target_paths = text_ctx.text_compiler.compile(
                    &target_text,
                    &font_family,
                    font_size,
                    font_weight,
                    &font_style,
                    line_height,
                    letter_spacing,
                    word_spacing,
                    color,
                    kind,
                    &language,
                    text_ctx.font_context,
                    max_width,
                    &text_align,
                    &overflow,
                )?;
                let source_half = crate::renderer::text::measure_text_paths(&source_paths);
                let source_offset = match text_anchor.as_str() {
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
                let target_offset = match text_anchor.as_str() {
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
                let clip = crate::timeline::dispatch::apply_split_text_animation(
                    &mut crossfaded,
                    text,
                    ctx.time_ms,
                );
                return Ok((std::sync::Arc::from(crossfaded), clip));
            }
        }
    }
    if content_override.is_none() && text.char_progress.is_none() {
        if let Some(static_paths) = &text.static_paths {
            let track_anchor = text.text_anchor.get(ctx.time_ms, "center".to_string());
            if text_anchor == track_anchor {
                return Ok((std::sync::Arc::clone(static_paths), None));
            }
        }
    }
    if content_override.is_none() && text.text_paths.is_some() {
        let (paths, clip) = text.evaluate_paths_and_clip(style, ctx.time_ms);
        let track_anchor = text.text_anchor.get(ctx.time_ms, "center".to_string());
        if text_anchor != track_anchor && !paths.is_empty() {
            let half_size = crate::renderer::text::measure_text_paths(&paths);
            let old_offset = match track_anchor.as_str() {
                "left" => half_size[0],
                "right" => -half_size[0],
                _ => 0.0,
            };
            let new_offset = match text_anchor.as_str() {
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
    } else if content_override.is_some() || !content.is_empty() {
        let compiled = text_ctx.text_compiler.compile(
            &content,
            &font_family,
            font_size,
            font_weight,
            &font_style,
            line_height,
            letter_spacing,
            word_spacing,
            color,
            kind,
            &language,
            text_ctx.font_context,
            max_width,
            &text_align,
            &overflow,
        )?;
        let mut paths_vec = compiled.to_vec();
        let half_size = crate::renderer::text::measure_text_paths(&paths_vec);
        let anchor_offset_x = match text_anchor.as_str() {
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

/// Read a `size` override written by an `always` block.
///
/// The authored value is a full width/height, but every geometry track stores
/// half-extents — so this halves, matching the declaration path
/// (`timeline/build/shape.rs`) and the keyframe path
/// (`assignments::rebuild::handle_size_assignment`). Without it
/// `always { r.size = (100.0, 60.0) }` paints a 200×120 box where
/// `size: (100, 60)` paints a 100×60 one.
/// Resolve a shape's half-size (its radius) for this frame.
///
/// The three sources do not share units, and mixing them up made the same
/// literal render twice as large per frame as declared:
///
/// - `track_half` comes from `geometry.size`, which **stores half-sizes** — both the declaration
///   path and the timed-assignment path halve the authored bounding box before storing it.
/// - a `size` override from an `always` block is the **raw authored bounding box**, so it is halved
///   here.
/// - `radius_x` / `radius_y` overrides are **half-size components** and each replace one axis. They
///   are applied after `size`, so a scene can set the box and then nudge a single radius.
///
/// A wrongly-typed override keeps the keyframed size. This runs for every shape
/// every frame, so the drop is logged at debug rather than warn, which would
/// repeat the same message 60 times a second.
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

// ── Primitive trait ─────────────────────────────────────────────────────

/// A container's `gap`/`padding` that the layout pass could not read.
///
/// The layout readers take a number or a `(x, y)` tuple and leave the value
/// alone otherwise, so without this the authored property disappears with no
/// output at all — the case AGENTS.md says must never be silent.
pub(crate) fn unreadable_layout_value(label: &str, prop: &str) -> Diagnostic {
    Diagnostic::warning(
        DiagnosticCode::InvalidPropertyValue,
        DiagnosticPhase::Build,
        format!("{label}.{prop} expects a number or a (x, y) tuple; the value was ignored"),
    )
}

/// Concatenate every vector path emitted by `commands` into one clip geometry.
///
/// All `Paths` commands and all subpaths contribute: a multi-path shape (e.g. an
/// `Svg` with several path elements, or a plot emitting multiple curves) must
/// not silently lose geometry when used as a `clip_shape`.
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

/// Every actor type in Animatix implements this trait: the *behaviour* half
/// of a primitive (build/evaluate/render). The metadata half lives in the
/// `animatix-std` catalog for built-ins and in the registration info for
/// extensions, keyed by [`Primitive::type_name`].
pub trait Primitive: Send + Sync {
    // ── Identity ──

    /// Source-text type name, e.g. "Rect", "Text", "Row" — the registry key.
    /// Metadata (display name, category, icon, capabilities, child
    /// processing) lives in the `animatix-std` catalog for built-ins and in
    /// the registration info for extensions; it is looked up by this name.
    fn type_name(&self) -> &str;

    /// Property names this primitive declares from the built-in or extension
    /// property registries.
    ///
    /// The generic property writer only needs membership tests, so callers
    /// should prefer [`Self::declares_property`] instead of materializing this
    /// list and repeatedly searching it.
    fn declared_property_names(&self) -> Vec<&str> {
        Vec::new()
    }

    /// Returns true when this primitive declares `name` from a built-in or
    /// extension property registry.
    fn declares_property(&self, name: &str) -> bool {
        self.declared_property_names().contains(&name)
    }

    // ── Build: AST → Timeline ──

    /// Build the actor into the timeline.
    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>>;

    // ── Render (optional, for shapes) ──

    /// Render the primitive into Vello paths.
    /// Returns `None` for non-visual primitives.
    fn render(&self, _ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        None
    }

    /// Local-space clip geometry when this primitive is used as a `Mask`'s
    /// `clip_shape` child.
    ///
    /// The default reuses the primitive's own evaluation output, so any shape
    /// (built-in or extension) that produces vector paths can define a clip
    /// without a per-primitive override. Plot geometry is excluded: it is
    /// time-varying / `stroke_progress`-trimmed and is not a fill region, so a
    /// plot used as a `clip_shape` would clip to a moving (or empty) partial
    /// curve. Returning `None` means "not usable as a clip"; the caller then
    /// warns and falls back to a rectangular clip. The path is in the
    /// primitive's local space — the caller composes the child transform and
    /// the mask transform.
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

    /// Data this primitive contributes to a parent `Equation`'s document, or
    /// `None` when it is not an equation fragment.
    fn equation_fragment(&self, _ctx: &EvaluateCtx) -> Option<EquationFragment> {
        None
    }

    /// Data this primitive contributes as a subtitle cue to a parent
    /// `Captions` container, or `None` when it is not a cue.
    fn caption_cue(&self, _ctx: &EvaluateCtx) -> Option<CaptionCue> {
        None
    }

    // ── Child rendering (containers) ──

    /// Render this primitive's children.
    ///
    /// This is the legacy entry point; callers with `EntityId` handles should
    /// call [`Primitive::render_children_by_id`].
    fn render_children(
        &self,
        ctx: &mut RenderChildrenCtx<'_, '_, '_>,
        children: &[&str],
    ) -> Result<(), RenderError> {
        ctx.render_children_default(children);
        Ok(())
    }

    /// Render this primitive's children directly by `EntityId`, avoiding string
    /// lookups on the hot render path.
    fn render_children_by_id(
        &self,
        ctx: &mut RenderChildrenCtx<'_, '_, '_>,
        children_ids: &[crate::timeline::EntityId],
    ) -> Result<(), RenderError> {
        ctx.render_children_ids(children_ids);
        Ok(())
    }

    // ── Build-time shape state (for vector shapes) ──

    /// Apply primitive-specific defaults to the shape state.
    fn apply_defaults(&self, _state: &mut VectorShapeState) {}

    /// Apply a single property to the shape state.
    /// Returns `true` if the property was handled.
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

    // Shape-kind questions (`uses_custom_path`, `exposes_tip_size`,
    // `supports_fill`) intentionally live as `ShapeType`-based free functions
    // in `timeline/shapes/mod.rs` (`vector_shape_uses_custom_path`,
    // `vector_shape_exposes_tip_size`) rather than on this trait: they are
    // answered by the GUI edit-vertices layer which only has a `ShapeType`.

    /// Returns the colorscheme key for default color lookup.
    /// For example, "Text" returns "text.primary", shapes return "accent.primary".
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

    // ── GUI defaults ──

    /// Default properties used when creating this actor from the GUI.
    fn default_props(&self, _scene_dimensions: &SceneDimensions) -> Vec<Property> {
        vec![]
    }

    // ── Assignment-phase handling ──

    /// Handle a property assignment at the assignment phase.
    /// Return `true` if the primitive handled it (bypassing generic engine).
    /// Default implementation returns `false` (delegate to generic engine).
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

    // ── Trait-dispatch scene evaluation (Phase 10b.3) ──

    // ── Post-children build finalization (for containers) ──

    /// Finalize the actor build after all children have been processed.
    ///
    /// Layout containers (Row, Col, Grid, Stack) override this to register
    /// container metadata and apply layout, which must happen after children
    /// are processed so that child tracks exist for layout computation.
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
    /// This is the only scene-evaluation render path — there is no per-type
    /// dispatch table in `scene_eval.rs`. Every drawable
    /// primitive implements this method (`Ok(None)` is the default for
    /// non-visual primitives).
    ///
    /// Semantics of the return value:
    /// - `Ok(Some(commands))` — `scene_eval.rs` executes the commands with the actor's local
    ///   transform and inherited opacity, and records a hit region derived from their local bounds.
    /// - `Ok(None)` — "no drawable content" (e.g. empty text, missing image). Nothing is drawn and
    ///   no hit region / precise bounds are recorded (empty-content actors stay un-pickable until
    ///   they have content; pinned by `runtime_empty_text_override_clears_stale_glyphs`).
    /// - `Err(e)` — the actor does not render and a `RenderFailure` runtime diagnostic is recorded.
    fn evaluate(
        &self,
        _ctx: &EvaluateCtx,
        _text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        Ok(None)
    }
}
