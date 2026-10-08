//! Unified primitive system for Animatix.
//!
//! Every actor type (shape, text, media, plot, container) is a `Primitive`.
//! `BUILT_INS` is the bootstrap list — each row pairs an `animatix-std`
//! identity card with the behaviour implementing it; `PrimitiveRegistry`
//! seeds built-ins through the same registration path used by extensions.
//!
//! ## Architecture
//!
//! ```text
//! BUILT_INS array (bootstrap: card + behaviour, paired by symbol)
//!        │
//!        ├──► PrimitiveRegistry (single storage for built-ins + extensions)
//!        ├──► find_primitive() — compatibility lookup for static built-ins
//!        └──► PrimitiveInfo cards (animatix_std::CATALOG, the metadata source)
//! ```
//!
//! Identity is the authored type name (`AnimationTrack::actor_type`); the
//! engine dispatches on `ActorCaps`, a `Copy` projection derived from the
//! catalog row. There is no actor-kind enum to extend.
//!
//! ## Adding a new primitive
//!
//! 1. Add the identity card: a `pub const <NAME>: PrimitiveInfo` in `animatix-std/src/catalog.rs`
//!    (type/display/icon/category/advanced/ capabilities/child processing) and its name in the
//!    `CATALOG` list. Tooling, the parser's contract tables, and the inspector palette all derive
//!    from the card.
//! 2. Create `primitives/<name>.rs` implementing `Primitive` (behaviour only — no metadata
//!    methods).
//! 3. Add `BuiltIn::new(&catalog::<NAME>, &<NAME>::CONST)` to the `BUILT_INS` array below. The card
//!    is referenced by symbol, so the two lists cannot drift positionally; a name mismatch fails
//!    the pairing test.
//! 4. If the primitive has properties, add the descriptor row to
//!    `animatix-core::property::PROPERTY_DESCRIPTORS` (name + applicability + value kind), its type
//!    row to `animatix-syntax/src/schema.rs::raw_property_types()`, and the runtime binding to
//!    `timeline/property_registry.rs::BINDINGS`; the tests in those modules fail until all three
//!    line up. Append descriptor rows — the row index is the serialized `PropertyId`.
//! 5. Document it (docs/primitives.md, docs/spec.md) and add render/hit-region coverage if it
//!    draws.
//!
//! ## Current primitives
//!
//! | Category | Primitives |
//! |----------|-----------|
//! | Shapes | Rect, Ellipse, Line, Polygon, Path |
//! | Text | Text, Math, Code |
//! | Media | Image, Svg |
//! | Plots | Graph, PlotCurve |
//! | Containers | Row, Col, Grid, Stack, Group, Mask |
use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::renderer::error::RenderError;
use crate::renderer::types::TextPath;
use crate::timeline::callout_geometry::TargetResolver;
use crate::timeline::{
    ActorCaps, ActorCategory, AnimationTrack, DEFAULT_WHITE, Environment, SceneDimensions,
    Timeline, TrackAccessor, Value, VectorShapeState, VectorShapeStyle, VelloPath,
    default_stroke_width,
};

/// Map the runtime UI category onto the schema category used by tooling.
pub(crate) fn actor_category_to_primitive_category(
    category: ActorCategory,
) -> animatix_syntax::schema::PrimitiveCategory {
    match category {
        ActorCategory::Shape => animatix_syntax::schema::PrimitiveCategory::Shape,
        ActorCategory::Text => animatix_syntax::schema::PrimitiveCategory::Text,
        ActorCategory::Media => animatix_syntax::schema::PrimitiveCategory::Media,
        ActorCategory::Plot => animatix_syntax::schema::PrimitiveCategory::Plot,
        ActorCategory::Container => animatix_syntax::schema::PrimitiveCategory::Container,
        ActorCategory::Annotation => animatix_syntax::schema::PrimitiveCategory::Annotation,
    }
}

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
) -> Result<std::sync::Arc<[crate::renderer::types::TextPath]>, crate::renderer::error::RenderError>
{
    use crate::timeline::TrackAccessor;

    let mut content = ctx.track.text.text_content.get(ctx.time_ms, String::new());
    let mut font_family = ctx.track.text.font_family.get(ctx.time_ms, String::new());
    let mut font_size = ctx.track.text.font_size.get(ctx.time_ms, default_font_size);
    let mut font_weight = ctx.track.text.font_weight.get(ctx.time_ms, 400.0);
    let mut font_style = ctx.track.text.font_style.get(ctx.time_ms, "normal".to_string());
    let mut line_height = ctx.track.text.line_height.get(ctx.time_ms, 1.2);
    let mut letter_spacing = ctx.track.text.letter_spacing.get(ctx.time_ms, 0.0);
    let mut word_spacing = ctx.track.text.word_spacing.get(ctx.time_ms, 0.0);
    let mut max_width = ctx.track.text.text_max_width.get(ctx.time_ms, 0.0);
    let mut text_align = ctx.track.text.text_align.get(ctx.time_ms, "left".to_string());
    let mut overflow = ctx.track.text.overflow.get(ctx.time_ms, "visible".to_string());
    let mut color = ctx.track.style.color.get(ctx.time_ms, DEFAULT_WHITE);
    let mut language = ctx.track.text.language.get(ctx.time_ms, String::new());

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
        if let Some((source_text, target_text, progress)) = ctx
            .track
            .text
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
                let crossfaded = crate::timeline::interpolate_text_paths(
                    &source_paths.to_vec(),
                    &target_paths.to_vec(),
                    progress,
                    crate::timeline::MorphOptions {
                        strategy: crate::timeline::MorphStrategy::Fade,
                        ..Default::default()
                    },
                );
                return Ok(std::sync::Arc::from(crossfaded));
            }
        }
    }
    if content_override.is_some() || !content.is_empty() {
        text_ctx.text_compiler.compile(
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
        )
    } else {
        Ok(std::sync::Arc::from(ctx.track.evaluate_text_paths(ctx.time_ms)))
    }
}

/// Read a color from an `always`-block override map.
///
/// A color written as text (`stroke = "#ff2d55"`) is as valid as one written as
/// a tuple: hex and named colors are accepted everywhere a color is, overrides
/// included.
fn override_color(
    overrides: &std::collections::HashMap<String, Value>,
    key: &str,
) -> Option<[f32; 4]> {
    match overrides.get(key) {
        Some(Value::Color(c) | Value::Vec4(c)) => {
            Some([c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32])
        },
        Some(Value::Str(text)) => crate::timeline::utils::color_from_text(text),
        // A non-color under a color key is an authoring error, but this runs for
        // every shape every frame, so `warn!` here would flood the log. The key-
        // framed value stands, which is what the old silent drop did as well.
        Some(other) => {
            tracing::debug!("`{key}` override is not a color: {other:?}; keeping keyframed value");
            None
        },
        None => None,
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

/// Sample shape style (color, stroke_width, stroke_color, fill_opacity) from a track
/// at the given time, applying property overrides when present.
pub fn sample_shape_style(
    track: &AnimationTrack,
    time_ms: u64,
    overrides: Option<&std::collections::HashMap<String, Value>>,
) -> VectorShapeStyle {
    let mut color = track.style.color.get(time_ms, DEFAULT_WHITE);
    let mut stroke_width =
        track.style.stroke_width.get(time_ms, default_stroke_width(&track.actor_type));
    let mut stroke_color = track.style.stroke_color.get(time_ms, DEFAULT_WHITE);
    let mut fill_opacity = track.style.fill_opacity.get(time_ms, 1.0);
    let mut line_cap = track.style.line_cap.get(time_ms, 0);
    let mut line_join = track.style.line_join.get(time_ms, 0);

    if let Some(node_overrides) = overrides {
        if let Some(c) = override_color(node_overrides, "color") {
            color = c;
        }
        if let Some(c) = override_color(node_overrides, "stroke") {
            stroke_color = c;
        }
        if let Some(Value::Num(width)) =
            node_overrides.get("stroke_width").or_else(|| node_overrides.get("width"))
        {
            stroke_width = *width as f32;
        }
        if let Some(Value::Num(opacity)) = node_overrides.get("fill_opacity") {
            fill_opacity = *opacity as f32;
        }
        if let Some(Value::Num(cap)) = node_overrides.get("line_cap") {
            line_cap = *cap as u32;
        }
        if let Some(Value::Num(join)) = node_overrides.get("line_join") {
            line_join = *join as u32;
        }
    }

    VectorShapeStyle {
        color,
        stroke_width,
        stroke_color,
        fill_opacity,
        line_cap,
        line_join,
    }
}

/// Memoized single-command output for the six vector-shape primitives.
///
/// The primitive `evaluate()` signature returns an owned `Vec<RenderCommand>`
/// (extension ABI — not changeable), so the steady-state frame paid two heap
/// allocations per static shape actor per frame (the command `Vec` plus the
/// inner paths `Vec`) even though every input is identical frame-to-frame.
/// The memo caches that `Vec` on the track, keyed by the *complete* render
/// input: every shape primitive's `render()` is a pure function of
/// `(style, state)` — anchor references and `always` overrides are resolved
/// into the state by the `evaluate()` wrappers *before* this function is
/// called (audited 2026-09-05; `PartialEq` on the state enums pins that
/// contract for future variants). The borrow protocol ("single master, take
/// and return") makes a memo hit fully allocation-free:
///
/// - **hit** (inputs equal): `take_shape_commands` hands the cached `Vec` to the caller, which
///   encodes it into the frame's vello scene, clones it for the observable `SceneItem` when item
///   collection is requested, and hands it back via `recycle_shape_commands` — the next frame's hit
///   reuses the same buffers.
/// - **miss** (inputs changed): the caller builds a fresh `Vec`, clones it once into the memo slot,
///   and still returns the fresh one.
///
/// `epoch` is `vector_paths_epoch` — the same invalidation funnel as the
/// PF-6 path memo (`invalidate_frame_cache`), which every track mutation
/// goes through; a stale epoch fails the key match and forces a rebuild.
/// Deliberately not part of the frame cache or `static_subtree_cache`: those
/// replay cached encodings and never re-run `evaluate()`, so they cannot
/// observe a stale memo.
#[derive(Default, Clone, Debug)]
pub(crate) struct ShapeCommandMemo {
    epoch: u64,
    style: Option<crate::timeline::VectorShapeStyle>,
    state: Option<crate::timeline::VectorShapeState>,
    commands: Option<Vec<RenderCommand>>,
    /// Local-space bounds of `commands`, computed once at build time — the
    /// frame path unions `cmd.local_bounds` per node per frame, and for a
    /// memo hit the answer is identical every frame (PF-4's scoped item).
    cached_bounds: Option<Option<kurbo::Rect>>,
}

impl ShapeCommandMemo {
    /// True when the memo holds commands built from exactly
    /// `(epoch, style, state)`.
    fn matches(
        &self,
        epoch: u64,
        style: &crate::timeline::VectorShapeStyle,
        state: &crate::timeline::VectorShapeState,
    ) -> bool {
        self.commands.is_some()
            && self.epoch == epoch
            && self.style.as_ref() == Some(style)
            && self.state.as_ref() == Some(state)
    }

    fn store(
        &mut self,
        epoch: u64,
        style: crate::timeline::VectorShapeStyle,
        state: &crate::timeline::VectorShapeState,
        commands: Vec<RenderCommand>,
        bounds: Option<kurbo::Rect>,
    ) {
        self.epoch = epoch;
        self.style = Some(style);
        self.state = Some(state.clone());
        self.commands = Some(commands);
        self.cached_bounds = Some(bounds);
    }
}

impl crate::timeline::AnimationTrack {
    /// Take the memoized command `Vec` out of the slot for reuse by the
    /// caller (see [`ShapeCommandMemo`]). `None` when the inputs changed;
    /// a stale epoch also drops the stale payload so it can never be served.
    pub(crate) fn take_shape_commands(
        &self,
        epoch: u64,
        style: &crate::timeline::VectorShapeStyle,
        state: &crate::timeline::VectorShapeState,
    ) -> Option<(Vec<RenderCommand>, Option<kurbo::Rect>)> {
        let mut memo = self.shape_command_memo.borrow_mut();
        if memo.matches(epoch, style, state) {
            let commands = memo.commands.take();
            return commands.map(|c| (c, memo.cached_bounds.unwrap_or(None)));
        }
        if memo.epoch != epoch {
            // Stale epoch: drop the payload AND the key, so a recycle of a
            // post-bump build can never later match against the pre-bump key.
            memo.epoch = epoch;
            memo.commands = None;
            memo.style = None;
            memo.state = None;
            memo.cached_bounds = None;
        }
        None
    }

    /// Build the fresh command `Vec` for this shape actor and stash a clone
    /// in the memo slot (see [`ShapeCommandMemo`]).
    pub(crate) fn build_shape_commands(
        &self,
        epoch: u64,
        style: crate::timeline::VectorShapeStyle,
        state: &crate::timeline::VectorShapeState,
        primitive: &dyn Primitive,
        time_ms: u64,
    ) -> Result<Vec<RenderCommand>, crate::renderer::error::RenderError> {
        let paths = primitive
            .render(&RenderCtx {
                state,
                style,
                time_ms,
                track: self,
            })
            .unwrap_or_default();
        if paths.is_empty() {
            // A shape producing no paths means "nothing to draw" — not an
            // error. Log at debug so a silently invisible shape is observable
            // in trace output without spamming per-frame warn logs.
            tracing::debug!(
                "shape primitive '{}' produced no paths at t={time_ms}ms",
                primitive.type_name()
            );
        }
        let commands = vec![RenderCommand::Paths { paths }];
        // Stash only when the slot is empty: a full slot means the previous
        // frame's payload was never taken (dynamic inputs) — keeping the
        // older valid payload avoids a clone per frame on dynamic actors
        // (the key match still guards staleness).
        let mut memo = self.shape_command_memo.borrow_mut();
        if memo.commands.is_none() {
            // Shape commands never carry images, so `display_size` is
            // irrelevant here (None).
            let bounds = commands
                .iter()
                .filter_map(|cmd| cmd.local_bounds(None))
                .reduce(|acc, rect| acc.union(rect));
            memo.store(epoch, style, state, commands.clone(), bounds);
        }
        Ok(commands)
    }

    /// Return a consumed command `Vec` to the memo slot so the next frame's
    /// hit can take it again (see [`ShapeCommandMemo`]). A `Vec` that did not
    /// come from the slot (double recycle, foreign build) is dropped.
    pub(crate) fn recycle_shape_commands(&self, commands: Vec<RenderCommand>) {
        let mut memo = self.shape_command_memo.borrow_mut();
        if memo.commands.is_none() {
            memo.commands = Some(commands);
        }
    }

    /// Clear the memo-bounds handoff slot (call before primitive evaluation —
    /// a non-shape primitive must never observe the previous node's bounds).
    pub(crate) fn begin_shape_commands(&self) {
        self.pending_shape_command_bounds.set(None);
    }

    /// Memo hit: publish the build-time-computed local bounds.
    pub(crate) fn offer_shape_command_bounds(&self, bounds: Option<kurbo::Rect>) {
        self.pending_shape_command_bounds.set(Some(bounds));
    }

    /// Consume the memo-bounds handoff slot. `None` = no memo data this node;
    /// compute the bounds from the commands as before.
    pub(crate) fn take_shape_command_bounds(&self) -> Option<Option<kurbo::Rect>> {
        self.pending_shape_command_bounds.take()
    }
}

/// Helper: sample style, render shape, wrap in RenderCommand::Paths.
///
/// Static-shape actors hit [`ShapeCommandMemo`] and get the cached command
/// `Vec` back allocation-free (take / encode / recycle); only a changed
/// `(style, state)` rebuilds.
pub(crate) fn evaluate_shape_render(
    primitive: &dyn Primitive,
    ctx: &EvaluateCtx,
    state: &VectorShapeState,
) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
    use crate::timeline::TrackAccessor;

    let style = sample_shape_style(ctx.track, ctx.time_ms, ctx.overrides);
    let epoch = ctx.track.shape.vector_paths_epoch.get();

    // `draw-in` cuts stroke-only geometry while its `stroke_progress` track is
    // between 0 and 1. The trim deliberately bypasses the shape-command memo
    // in both directions: keying the memo on progress would leave the slot
    // holding the last animated key forever (a permanent miss once the draw
    // settles), and trimming a taken payload would poison the recycle
    // protocol with cut geometry. Skipping the take keeps the untouched
    // full-length payload valid in the slot — it serves again at
    // progress == 1 — and build_shape_commands' only-store-when-empty guard
    // keeps the mid-draw clones out of it.
    let progress = f64::from(ctx.track.style.stroke_progress.get(ctx.time_ms, 1.0)).clamp(0.0, 1.0);
    if progress < 1.0 {
        let built = ctx.track.build_shape_commands(epoch, style, state, primitive, ctx.time_ms)?;
        let mut trimmed = trim_shape_stroke_progress(&built, progress);
        stamp_shape_dash(&mut trimmed, ctx);
        stamp_shape_gradient(&mut trimmed, ctx);
        return Ok(Some(trimmed));
    }

    // A dash pattern or a gradient paint rides *outside* the shape-command memo
    // (its key is `(epoch, style, state)`), because an animated `dash_offset` —
    // the marching-ants case — or an animated ramp would otherwise be served
    // from a cached encoding. Bearing frames build fresh and stamp clones;
    // every other frame takes the memo fast path untouched.
    let has_dash = !ctx.track.style.dash_pattern.get(ctx.time_ms, Vec::new()).is_empty();
    let has_gradient = shape_has_gradient(ctx);
    if has_dash || has_gradient {
        let mut built =
            ctx.track.build_shape_commands(epoch, style, state, primitive, ctx.time_ms)?;
        if has_dash {
            stamp_shape_dash(&mut built, ctx);
        }
        if has_gradient {
            stamp_shape_gradient(&mut built, ctx);
        }
        return Ok(Some(built));
    }

    if let Some((commands, bounds)) = ctx.track.take_shape_commands(epoch, &style, state) {
        ctx.track.offer_shape_command_bounds(bounds);
        return Ok(Some(commands));
    }
    ctx.track
        .build_shape_commands(epoch, style, state, primitive, ctx.time_ms)
        .map(Some)
}

/// Stamp the shared stroke decorations (dash, gradient) onto a plot's commands.
///
/// `PlotCurve` assembles its own `RenderCommand` instead of going through
/// `evaluate_shape_render`, so it has to ask for the decorations the vector
/// shapes get by hand: `dash_pattern:` and `stroke_gradient:` are declared
/// `Applicable::AllStrokePaths`, which includes `PlotCurve`.
pub(crate) fn stamp_stroke_decoration(commands: &mut [RenderCommand], ctx: &EvaluateCtx) {
    stamp_shape_dash(commands, ctx);
    stamp_shape_gradient(commands, ctx);
}

/// Stamp the sampled dash pattern/offset onto every path in `commands`.
///
/// Called only while a dash pattern is authored. During a `draw-in` the trim
/// is applied first, so a dashed stroke draws in with its (fixed-length)
/// dashes appearing as the path grows.
fn stamp_shape_dash(commands: &mut [RenderCommand], ctx: &EvaluateCtx) {
    use crate::timeline::TrackAccessor;

    let pattern = ctx.track.style.dash_pattern.get(ctx.time_ms, Vec::new());
    if pattern.is_empty() {
        return;
    }
    let offset = ctx.track.style.dash_offset.get(ctx.time_ms, 0.0);
    for cmd in commands.iter_mut() {
        if let RenderCommand::Paths { paths } = cmd {
            for vp in paths.iter_mut() {
                vp.dash_pattern = Some(pattern.clone());
                vp.dash_offset = offset;
            }
        }
    }
}

/// True when either gradient track was authored.
///
/// A `is_some` probe, not a sample: unlike `dash_pattern`, the gradient tracks
/// are only created by an authored `fill_gradient:` / `stroke_gradient:`, and
/// sampling one per frame would clone its stop `Vec` on the way to deciding
/// whether to clone it.
fn shape_has_gradient(ctx: &EvaluateCtx) -> bool {
    ctx.track.style.fill_gradient.is_some() || ctx.track.style.stroke_gradient.is_some()
}

/// Stamp the sampled `fill_gradient:` / `stroke_gradient:` paints onto every
/// path in `commands`, applying the shared `gradient_extend:` /
/// `gradient_space:` settings.
fn stamp_shape_gradient(commands: &mut [RenderCommand], ctx: &EvaluateCtx) {
    use crate::renderer::types::{GradientExtend, GradientSpace, GradientSpec};
    use crate::timeline::TrackAccessor;

    let fill = ctx.track.style.fill_gradient.get(ctx.time_ms, GradientSpec::default());
    let stroke = ctx.track.style.stroke_gradient.get(ctx.time_ms, GradientSpec::default());
    if fill.stops.is_empty() && stroke.stops.is_empty() {
        return;
    }

    let extend = match ctx.track.style.gradient_extend.get(ctx.time_ms, "pad".to_string()).as_str()
    {
        "repeat" => GradientExtend::Repeat,
        "reflect" => GradientExtend::Reflect,
        "pad" | "" => GradientExtend::Pad,
        other => {
            tracing::warn!(
                "gradient_extend: unknown extend '{other}' (expected pad, repeat or reflect); using pad"
            );
            GradientExtend::Pad
        },
    };
    let space = match ctx.track.style.gradient_space.get(ctx.time_ms, "oklab".to_string()).as_str()
    {
        "srgb" => GradientSpace::Srgb,
        "oklab" | "" => GradientSpace::Oklab,
        other => {
            tracing::warn!(
                "gradient_space: unknown color space '{other}' (expected oklab or srgb); using oklab"
            );
            GradientSpace::Oklab
        },
    };

    for cmd in commands.iter_mut() {
        if let RenderCommand::Paths { paths } = cmd {
            for vp in paths.iter_mut() {
                if !fill.stops.is_empty() {
                    let mut g = fill.clone();
                    g.extend = extend;
                    g.space = space;
                    vp.fill_gradient = Some(Box::new(g));
                }
                if !stroke.stops.is_empty() {
                    let mut g = stroke.clone();
                    g.extend = extend;
                    g.space = space;
                    vp.stroke_gradient = Some(Box::new(g));
                }
            }
        }
    }
}

/// Apply `draw-in`'s stroke trim to built shape commands: stroke-only paths
/// (the `fill: None` + `stroke: Some` combination `build_vello_path` produces
/// for `fill_opacity: 0` shapes) are cut to the leading `progress` fraction of
/// their segments, matching the plot primitive's consumption of the same
/// track. Filled paths pass through untouched so a fill keeps the
/// reveal-to-authored-opacity semantics (commit `8e244595`).
fn trim_shape_stroke_progress(commands: &[RenderCommand], progress: f64) -> Vec<RenderCommand> {
    use crate::timeline::path_progress::trim_path_by_progress;

    commands
        .iter()
        .map(|cmd| match cmd {
            RenderCommand::Paths { paths } => RenderCommand::Paths {
                paths: paths
                    .iter()
                    .map(|vp| {
                        let mut trimmed = vp.clone();
                        if trimmed.fill.is_none() && trimmed.stroke.is_some() {
                            trimmed.path =
                                std::sync::Arc::new(trim_path_by_progress(&vp.path, progress));
                            if progress <= 0.0 {
                                trimmed.stroke = None;
                            }
                        }
                        trimmed
                    })
                    .collect(),
            },
            other => other.clone(),
        })
        .collect()
}

// ── Re-export all primitive modules ──────────────────────────────────────

mod rect;
pub use rect::RECT;
mod ellipse;
pub use ellipse::ELLIPSE;
mod line;
pub use line::LINE;
mod arrow;
pub use arrow::ARROW;
mod polygon;
pub use polygon::POLYGON;
mod path;
pub use path::PATH;
mod text;
pub use text::TEXT;
mod code;
pub use code::CODE;
mod math;
pub use math::MATH;
mod image;
pub use image::IMAGE;
#[cfg(feature = "svg")]
mod svg;
#[cfg(feature = "svg")]
pub use svg::SVG;
mod bar_chart;
pub use bar_chart::BAR_CHART;
mod plot;
pub use plot::{CONTOUR_SET, GRAPH, HEATMAP, NUMBER_PLANE, PLOT_CURVE, VECTOR_FIELD};
mod row;
pub use row::ROW;
mod col;
pub use col::COL;
mod grid;
pub use grid::GRID;
mod stack;
pub use stack::STACK;
mod group;
pub use group::GROUP;
mod mask;
pub use mask::MASK;
mod filter;
pub use filter::FILTER;
mod glass;
pub use glass::GLASS;
mod registry;
pub use registry::{PrimitiveRegistrationError, PrimitiveRegistry};

mod typst;
pub use typst::TYPST;

mod audio;
pub use audio::AUDIO;

mod equation;
pub use equation::EQUATION;
mod fragment;
pub use fragment::FRAGMENT;
mod callout;
pub use callout::CALLOUT;
mod legend;
pub use legend::LEGEND;

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

/// Context passed to `Primitive::build()`.
pub struct BuildCtx<'a> {
    /// The timeline being built.
    pub timeline: &'a mut Timeline,
    /// Current time in milliseconds.
    pub time_ms: f64,
    /// Optional parent actor label.
    pub parent_label: Option<&'a str>,
    /// Build diagnostics collector.
    pub diagnostics: &'a mut Vec<Diagnostic>,
}

/// Timing and resource context for `Primitive::handle_assignment()`.
pub struct AssignmentCtx<'a> {
    /// Animation start time in milliseconds.
    pub t_start_ms: u64,
    /// Animation end time in milliseconds.
    pub t_end_ms: u64,
    /// Easing function for the animation.
    pub easing: Easing,
    /// Whether the animation is instant but delayed.
    pub instant_delayed: bool,
    /// Animation duration in milliseconds.
    pub duration_ms: f64,
    /// Font rendering context.
    pub font_context: &'a crate::renderer::text::FontContext,
    /// Text compiler for recompilation.
    pub text_compiler: &'a mut crate::renderer::text::TextCompiler,
    /// Shared asset cache for loading media referenced by assignments.
    pub asset_cache: &'a mut crate::timeline::assets::AssetCache,
}

/// Context passed to `Primitive::render()`.
pub struct RenderCtx<'a> {
    /// Current vector shape state.
    pub state: &'a VectorShapeState,
    /// Shape style (color, stroke, fill).
    pub style: VectorShapeStyle,
    /// Current time in milliseconds.
    pub time_ms: u64,
    /// The actor's track — memoization home for the shape→path conversion
    /// (PF-6); see `AnimationTrack::shape_path_memoized`.
    pub track: &'a AnimationTrack,
}

/// Context passed to `Primitive::evaluate()`.
///
/// All fields are immutable. Text-specific mutable state is in [`TextCompileCtx`].
pub struct EvaluateCtx<'a> {
    /// The animation track for this actor.
    pub track: &'a AnimationTrack,
    /// Current time in milliseconds.
    pub time_ms: u64,
    /// Local transform (parent * position * rotation * scale).
    pub local_transform: kurbo::Affine,
    /// Inherited opacity multiplier.
    pub opacity: f32,
    /// Scene dimensions.
    pub scene_dimensions: SceneDimensions,
    /// Sampled scene background color at this frame.
    pub background_color: [f32; 4],
    /// Property overrides from modifiers.
    pub overrides: Option<&'a std::collections::HashMap<String, Value>>,
    /// Pre-sampled vector paths (includes procedural plot sampling).
    pub vector_paths: &'a [VelloPath],
    /// Shared image cache for native render commands that specify a URL.
    pub asset_cache: &'a crate::timeline::assets::AssetCache,
    /// Narrow resolver for target actor bounds (targeted callout mode).
    /// Replaces the previous broad `Option<&Timeline>` field.
    pub target_resolver: Option<&'a dyn TargetResolver>,
}

/// Frame-invariant, read-only state shared by every node rendered this frame.
///
/// Bundling these is what keeps the render recursion's argument list short:
/// `evaluate_node` and friends used to take fourteen positional arguments, and a
/// mis-ordered argument there is a silent semantic change rather than a compile
/// error. Per-container values (`layout_positions`) and per-call values
/// (`allow_pending_composites`) deliberately stay parameters — they are not
/// frame-invariant.
#[derive(Clone, Copy)]
pub(crate) struct RenderFrame<'a> {
    /// Current time in milliseconds.
    pub(crate) time_ms: u64,
    /// Scene dimensions for this frame.
    pub(crate) scene_dimensions: SceneDimensions,
    /// Debug overlay options for this frame.
    pub(crate) debug_options: crate::timeline::DebugRenderOptions,
    /// Property overrides produced by modifiers, keyed by actor label.
    pub(crate) overrides:
        &'a std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    /// The frame environment, when modifiers or procedural plots needed one.
    pub(crate) frame_env: Option<&'a Environment>,
}

/// Mutable outputs of one render call.
///
/// The three table outputs and the backend handle are reborrowed unchanged when
/// a container renders its children into a different scene; see
/// [`Self::with_scene`].
pub(crate) struct RenderOutputs<'s, 'f> {
    /// Scene receiving the draw commands.
    pub(crate) scene: &'s mut vello::Scene,
    /// Hit regions (world bounds) collected for this frame.
    pub(crate) hit_regions: &'s mut Vec<(String, kurbo::Rect)>,
    /// Observable scene items, when collection was requested.
    pub(crate) program_items: &'s mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
    /// The active filter backend, when one is available.
    pub(crate) filter_backend: &'s mut Option<&'f mut dyn crate::timeline::effects::FilterBackend>,
}

impl RenderOutputs<'_, '_> {
    /// Run `f` with the draw target temporarily replaced by `scene`.
    ///
    /// The Filter strategy renders its children into an offscreen sub-scene —
    /// a stack local with a shorter lifetime than this context — so the swap
    /// happens through a fresh borrow rather than an assignment to
    /// [`Self::scene`].
    pub(crate) fn with_scene<R>(
        &mut self,
        scene: &mut vello::Scene,
        f: impl FnOnce(&mut RenderOutputs<'_, '_>) -> R,
    ) -> R {
        let mut swapped = RenderOutputs {
            scene,
            hit_regions: &mut *self.hit_regions,
            program_items: &mut *self.program_items,
            filter_backend: &mut *self.filter_backend,
        };
        f(&mut swapped)
    }

    /// [`Self::with_scene`] plus a replacement item sink.
    ///
    /// The static-subtree cache renders a subtree into a scratch scene *and*
    /// collects its items into a per-subtree slot, so both differ from the
    /// frame's; everything else is reborrowed unchanged.
    pub(crate) fn with_scene_and_items<R>(
        &mut self,
        scene: &mut vello::Scene,
        program_items: &mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
        f: impl FnOnce(&mut RenderOutputs<'_, '_>) -> R,
    ) -> R {
        let mut swapped = RenderOutputs {
            scene,
            hit_regions: &mut *self.hit_regions,
            program_items,
            filter_backend: &mut *self.filter_backend,
        };
        f(&mut swapped)
    }
}

/// Mutable context passed to [`Primitive::render_children`].
///
/// The scene-subtree renderer hands each container primitive this context so the
/// primitive drives its own recursion, instead of the pipeline branching on a
/// `ChildProcessing` value. `scene`, `hit_regions`, `program_items`, and
/// `filter_backend` are the caller-local outputs; every other field is
/// read-only frame state, handed to the recursion entry point
/// (`Timeline::evaluate_node`) through `Self::render_frame` and
/// `RenderOutputs`. The frame caches are reached through `Self::timeline`.
pub struct RenderChildrenCtx<'a, 'b, 'c> {
    /// The timeline being rendered.
    pub timeline: &'a Timeline,
    /// Label of the container whose children are being rendered.
    pub node_label: &'a str,
    /// Current time in milliseconds.
    pub time_ms: u64,
    /// The container's world transform.
    pub global_transform: kurbo::Affine,
    /// The container's inherited opacity.
    pub global_opacity: f32,
    /// Scene dimensions.
    pub scene_dimensions: SceneDimensions,
    /// Debug overlay options for this frame.
    pub debug_options: crate::timeline::DebugRenderOptions,
    /// Property overrides from modifiers, keyed by actor label.
    pub overrides: &'a std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    /// Resolved child layout positions for this frame (shared by refcount, so
    /// storing it by value is a cheap clone).
    pub layout_positions: std::sync::Arc<crate::timeline::layout::LayoutPositions>,
    /// The frame environment, when one was built.
    pub frame_env: Option<&'a Environment>,
    /// Whether the caller may park GPU filter composites for a later blit.
    pub allow_pending_composites: bool,
    /// Output scene for this frame.
    pub scene: &'b mut vello::Scene,
    /// Output hit regions (world bounds) for this frame.
    pub hit_regions: &'b mut Vec<(String, kurbo::Rect)>,
    /// Output observable scene items, when item collection was requested.
    pub program_items: &'b mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
    /// The active filter backend, when one is available.
    pub filter_backend: &'b mut Option<&'c mut dyn crate::timeline::effects::FilterBackend>,
}

impl<'a, 'b, 'c> RenderChildrenCtx<'a, 'b, 'c> {
    /// The container track these children belong to.
    pub fn track(&self) -> Option<&AnimationTrack> {
        self.timeline.tracks.get(self.node_label)
    }

    /// This context's read-only frame state, for the render recursion.
    ///
    /// The returned frame borrows the *frame data* (`'a`), not `self`, so the
    /// caller can still take the mutable outputs afterwards.
    fn render_frame(&self) -> RenderFrame<'a> {
        RenderFrame {
            time_ms: self.time_ms,
            scene_dimensions: self.scene_dimensions,
            debug_options: self.debug_options,
            overrides: self.overrides,
            frame_env: self.frame_env,
        }
    }

    /// Render one child into this context's scene, preserving the caller's
    /// `allow_pending_composites` flag.
    pub fn render_child(&mut self, child: &str) {
        let frame = self.render_frame();
        let allow_pending_composites = self.allow_pending_composites;
        let mut out = RenderOutputs {
            scene: &mut *self.scene,
            hit_regions: &mut *self.hit_regions,
            program_items: &mut *self.program_items,
            filter_backend: &mut *self.filter_backend,
        };
        self.timeline.evaluate_node(
            child,
            self.global_transform,
            self.global_opacity,
            &self.layout_positions,
            allow_pending_composites,
            &frame,
            &mut out,
        );
    }

    /// Render one child into a caller-provided scene (the Filter strategy
    /// renders into an offscreen sub-scene, which never parks composites).
    pub fn render_child_into(&mut self, scene: &mut vello::Scene, child: &str) {
        let frame = self.render_frame();
        let mut out = RenderOutputs {
            scene,
            hit_regions: &mut *self.hit_regions,
            program_items: &mut *self.program_items,
            filter_backend: &mut *self.filter_backend,
        };
        self.timeline.evaluate_node(
            child,
            self.global_transform,
            self.global_opacity,
            &self.layout_positions,
            false,
            &frame,
            &mut out,
        );
    }

    /// Default child rendering: every child in order into this context's scene.
    pub fn render_children_default(&mut self, children: &[&str]) {
        for child in children {
            self.render_child(child);
        }
    }

    /// Push a runtime render diagnostic (surfaced through the frame's
    /// diagnostics, e.g. for a filter fallback).
    pub fn push_diagnostic(&self, diagnostic: Diagnostic) {
        self.timeline.eval_caches.runtime_diagnostics.borrow_mut().push(diagnostic);
    }
}

/// Mutable context for text recompilation.
///
/// Only text primitives need this. Shape, image, and SVG primitives
/// can ignore it entirely.
pub struct TextCompileCtx<'a> {
    /// Text compiler for runtime text recompilation.
    pub text_compiler: &'a mut crate::renderer::text::TextCompiler,
    /// Font context for text rendering.
    pub font_context: &'a crate::renderer::text::FontContext,
}

/// A single render command produced by `Primitive::evaluate()`.
///
/// These commands are executed by `scene_eval.rs` into a Vello scene.
/// Separating command generation from execution lets primitives stay
/// independent of the scene evaluation loop.
///
/// # Extension boundary
///
/// The set of commands a plugin can emit is currently fixed to the four
/// variants below (the plugin ABI exposes one `append_*` callback per
/// variant). Adding a new variant requires updating the ABI
/// (`animatix-plugin-api`), [`RenderCommand::execute`], and
/// [`RenderCommand::local_bounds`] together. The enum is `#[non_exhaustive]`
/// so external code must already handle unknown variants.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum RenderCommand {
    /// Draw a set of vector paths, each with its own fill and stroke.
    Paths {
        /// The vector paths to draw.
        paths: Vec<VelloPath>,
    },
    /// Draw text glyphs.
    Text {
        /// Text glyph paths with per-glyph color and opacity.
        paths: std::sync::Arc<[TextPath]>,
    },
    /// Draw an image.
    Image {
        /// The image data.
        image: crate::timeline::image::SceneImage,
        /// Natural (display) width and height in scene units.
        natural_size: [f32; 2],
        /// Local-space placement offset.
        offset: [f64; 2],
    },
    /// A highlight layer drawn with a specific blend mode (for equation fragment highlights).
    HighlightLayer {
        /// The rounded rectangle geometry.
        rect: kurbo::Rect,
        /// Fill color.
        color: vello::peniko::Color,
        /// Blend mode (e.g. Difference, Multiply).
        blend: vello::peniko::Mix,
        /// Layer alpha (0.0–1.0).
        alpha: f32,
        /// Corner radius.
        corner_radius: f64,
    },
}

/// Translate text glyph paths by a local-space offset.
pub fn translate_text_paths(
    paths: std::sync::Arc<[crate::renderer::types::TextPath]>,
    x: f64,
    y: f64,
) -> std::sync::Arc<[crate::renderer::types::TextPath]> {
    if x == 0.0 && y == 0.0 {
        return paths;
    }
    let translate = kurbo::Affine::translate((x, y));
    paths
        .iter()
        .map(|tp| {
            let mut p = tp.clone();
            p.path.apply_affine(translate);
            p
        })
        .collect::<Vec<_>>()
        .into()
}

/// Compute the transform that maps a texture's pixel rect onto its display box.
///
/// `vello::Scene::draw_image` fills a rect of `image_width × image_height`
/// scene units (1 texture pixel = 1 unit), so a texture of `pixels` pixels
/// needs a scale of `display / pixels` to appear at its requested display
/// size. `display` is the command's `natural_size` (full actor size in scene
/// units), `offset` shifts the box in local space before the actor transform.
pub fn image_display_transform(
    display: [f32; 2],
    pixels: [f32; 2],
    offset: [f64; 2],
) -> kurbo::Affine {
    let sx = (display[0] / pixels[0].max(1.0)) as f64;
    let sy = (display[1] / pixels[1].max(1.0)) as f64;
    kurbo::Affine::translate((offset[0], offset[1])) * kurbo::Affine::scale_non_uniform(sx, sy)
}

impl RenderCommand {
    /// Execute this command into a Vello scene with the given transform and opacity.
    pub fn execute(&self, scene: &mut vello::Scene, transform: &kurbo::Affine, opacity: f32) {
        match self {
            RenderCommand::Paths { paths } => {
                use kurbo::Shape as _;
                for path in paths {
                    match (&path.fill, &path.fill_gradient) {
                        (Some(fc), Some(grad)) => {
                            let g = grad
                                .to_peniko(path.path.bounding_box(), fc.components[3] * opacity);
                            scene.fill(
                                vello::peniko::Fill::NonZero,
                                *transform,
                                &g,
                                None,
                                path.path.as_ref(),
                            );
                        },
                        (Some(fc), None) => {
                            let fc = fc.with_alpha(fc.components[3] * opacity);
                            scene.fill(
                                vello::peniko::Fill::NonZero,
                                *transform,
                                fc,
                                None,
                                path.path.as_ref(),
                            );
                        },
                        // A ramp authored on a stroke-only shape paints no fill.
                        (None, Some(_)) => {},
                        (None, None) => {},
                    }
                    if let Some((mut sc, sw)) = path.stroke {
                        sc = sc.with_alpha(sc.components[3] * opacity);
                        let cap = match path.line_cap {
                            1 => vello::kurbo::Cap::Round,
                            2 => vello::kurbo::Cap::Square,
                            _ => vello::kurbo::Cap::Butt,
                        };
                        let join = match path.line_join {
                            1 => vello::kurbo::Join::Round,
                            2 => vello::kurbo::Join::Bevel,
                            _ => vello::kurbo::Join::Miter,
                        };
                        let dash: vello::kurbo::Dashes = path
                            .dash_pattern
                            .as_ref()
                            .map(|p| p.iter().map(|f| f64::from(*f)).collect())
                            .unwrap_or_default();
                        let stroke = vello::kurbo::Stroke {
                            width: sw as f64,
                            join,
                            miter_limit: 10.0,
                            start_cap: cap,
                            end_cap: cap,
                            dash_pattern: dash,
                            dash_offset: f64::from(path.dash_offset),
                        };
                        match &path.stroke_gradient {
                            Some(grad) => {
                                let g = grad.to_peniko(
                                    path.path.bounding_box(),
                                    sc.components[3] * opacity,
                                );
                                scene.stroke(&stroke, *transform, &g, None, path.path.as_ref());
                            },
                            None => scene.stroke(&stroke, *transform, sc, None, path.path.as_ref()),
                        }
                    }
                }
            },
            RenderCommand::Text { paths } => {
                for text_path in paths.iter() {
                    let [r, g, b, a] = text_path.color;
                    let color = vello::peniko::Color::from_rgba8(
                        r,
                        g,
                        b,
                        (a as f32 * opacity * text_path.opacity) as u8,
                    );
                    scene.fill(
                        vello::peniko::Fill::NonZero,
                        *transform,
                        color,
                        None,
                        &text_path.path,
                    );
                }
            },
            RenderCommand::Image {
                image,
                natural_size,
                offset,
            } => {
                let image_transform = *transform
                    * image_display_transform(*natural_size, image.natural_size, *offset);
                let brush = vello::peniko::ImageBrush::new(image.data.clone())
                    .with_extend(vello::peniko::Extend::Pad)
                    .with_quality(vello::peniko::ImageQuality::Medium)
                    .with_alpha(opacity);
                scene.draw_image(&brush, image_transform);
            },
            RenderCommand::HighlightLayer {
                rect,
                color,
                blend,
                alpha,
                corner_radius,
            } => {
                let rounded = kurbo::RoundedRect::from_rect(*rect, *corner_radius);
                scene.push_layer(
                    vello::peniko::Fill::NonZero,
                    vello::peniko::BlendMode::new(*blend, vello::peniko::Compose::SrcOver),
                    *alpha * opacity,
                    *transform,
                    &rounded,
                );
                scene.fill(
                    vello::peniko::Fill::NonZero,
                    kurbo::Affine::IDENTITY,
                    *color,
                    None,
                    &rounded,
                );
                scene.pop_layer();
            },
        }
    }

    /// Compute the local-space bounding box of this command's geometry.
    ///
    /// Returns `None` if the command has no drawable content.
    /// The bounding box is in local coordinates (before transform).
    pub fn local_bounds(&self, display_size: Option<[f32; 2]>) -> Option<kurbo::Rect> {
        use kurbo::Shape;
        let mut bounds: Option<kurbo::Rect> = None;
        let union = |acc: Option<kurbo::Rect>, rect: kurbo::Rect| -> Option<kurbo::Rect> {
            Some(match acc {
                Some(existing) => existing.union(rect),
                None => rect,
            })
        };
        match self {
            RenderCommand::Paths { paths } => {
                for path in paths {
                    bounds = union(bounds, path.path.bounding_box());
                }
            },
            RenderCommand::Text { paths } => {
                for text_path in paths.iter() {
                    bounds = union(bounds, text_path.path.bounding_box());
                }
            },
            RenderCommand::Image { .. } => {
                if let Some([half_w, half_h]) = display_size {
                    // Centered on the local origin, matching the box the
                    // command actually draws.
                    bounds = union(
                        bounds,
                        kurbo::Rect::new(
                            (-half_w) as f64,
                            (-half_h) as f64,
                            half_w as f64,
                            half_h as f64,
                        ),
                    );
                }
            },
            RenderCommand::HighlightLayer { rect, .. } => {
                bounds = union(bounds, *rect);
            },
        }
        bounds
    }
}

/// Child-rendering strategy selected by a primitive.
pub use animatix_syntax::schema::ChildProcessingKind as ChildProcessing;

/// Data one child actor contributes to a parent `Equation`'s Typst document.
///
/// Returned by [`Primitive::equation_fragment`]; the Equation container collects
/// every child whose primitive returns `Some`, in source order, and joins them
/// into one compiled document. Making this a trait query (rather than a match on
/// the actor's type name) lets any primitive — including an extension — be an
/// equation fragment.
#[derive(Clone, Debug, PartialEq)]
pub struct EquationFragment {
    /// Raw fragment markup; the Equation container escapes and joins it.
    pub content: String,
    /// Highlight rectangle fill color.
    pub highlight_color: [f32; 4],
    /// Highlight layer opacity (`0.0` = no highlight).
    pub highlight_opacity: f32,
    /// Highlight padding around the fragment.
    pub highlight_padding: f32,
    /// Highlight corner radius.
    pub highlight_radius: f32,
    /// Highlight blend mode.
    pub highlight_blend: vello::peniko::Mix,
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
        ctx.track.recycle_shape_commands(commands);
        path
    }

    /// Data this primitive contributes to a parent `Equation`'s document, or
    /// `None` when it is not an equation fragment.
    fn equation_fragment(&self, _ctx: &EvaluateCtx) -> Option<EquationFragment> {
        None
    }

    // ── Child rendering (containers) ──

    /// Render this primitive's children.
    ///
    /// This is the single entry point the scene renderer calls — it never
    /// branches on the child-processing strategy. The default renders children
    /// through the normal scene-graph recursion; container primitives that need
    /// a dedicated pipeline (`Filter`, `Mask`, `Equation`) override it.
    fn render_children(
        &self,
        ctx: &mut RenderChildrenCtx<'_, '_, '_>,
        children: &[&str],
    ) -> Result<(), RenderError> {
        ctx.render_children_default(children);
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
        _track: &mut AnimationTrack,
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

// ── The one static array ────────────────────────────────────────────────

/// A built-in primitive: its `animatix-std` identity card paired by symbol
/// with the engine behaviour that implements it.
///
/// One row per primitive in [`BUILT_INS`]; referencing the card directly is
/// what removes the positional coupling between the catalog and the
/// behaviour list.
pub struct BuiltIn {
    /// The identity card from the `animatix-std` catalog (metadata source).
    pub info: &'static PrimitiveInfo,
    /// The compiled-in behaviour implementation.
    pub behavior: &'static dyn Primitive,
}

impl BuiltIn {
    /// Pair a catalog card with its behaviour implementation.
    pub const fn new(info: &'static PrimitiveInfo, behavior: &'static dyn Primitive) -> Self {
        Self { info, behavior }
    }
}

/// Bootstrap list of all built-in primitives.
///
/// `PrimitiveRegistry::new()` registers these through the same `register`
/// path used by extension primitives. Order mirrors the catalog for
/// presentation only; pairing is by symbol, not position.
pub static BUILT_INS: &[BuiltIn] = &[
    // Shapes
    BuiltIn::new(&animatix_std::catalog::RECT, &RECT),
    BuiltIn::new(&animatix_std::catalog::ELLIPSE, &ELLIPSE),
    BuiltIn::new(&animatix_std::catalog::LINE, &LINE),
    BuiltIn::new(&animatix_std::catalog::ARROW, &ARROW),
    BuiltIn::new(&animatix_std::catalog::POLYGON, &POLYGON),
    BuiltIn::new(&animatix_std::catalog::PATH, &PATH),
    // Text
    BuiltIn::new(&animatix_std::catalog::TEXT, &TEXT),
    BuiltIn::new(&animatix_std::catalog::CODE, &CODE),
    BuiltIn::new(&animatix_std::catalog::MATH, &MATH),
    BuiltIn::new(&animatix_std::catalog::TYPST, &TYPST),
    // Media
    BuiltIn::new(&animatix_std::catalog::IMAGE, &IMAGE),
    #[cfg(feature = "svg")]
    BuiltIn::new(&animatix_std::catalog::SVG, &SVG),
    BuiltIn::new(&animatix_std::catalog::AUDIO, &AUDIO),
    // Plots
    BuiltIn::new(&animatix_std::catalog::GRAPH, &GRAPH),
    BuiltIn::new(&animatix_std::catalog::PLOT_CURVE, &PLOT_CURVE),
    BuiltIn::new(&animatix_std::catalog::VECTOR_FIELD, &VECTOR_FIELD),
    BuiltIn::new(&animatix_std::catalog::HEATMAP, &HEATMAP),
    BuiltIn::new(&animatix_std::catalog::CONTOUR_SET, &CONTOUR_SET),
    BuiltIn::new(&animatix_std::catalog::NUMBER_PLANE, &NUMBER_PLANE),
    BuiltIn::new(&animatix_std::catalog::BAR_CHART, &BAR_CHART),
    // Containers
    BuiltIn::new(&animatix_std::catalog::ROW, &ROW),
    BuiltIn::new(&animatix_std::catalog::COL, &COL),
    BuiltIn::new(&animatix_std::catalog::GRID, &GRID),
    BuiltIn::new(&animatix_std::catalog::STACK, &STACK),
    BuiltIn::new(&animatix_std::catalog::GROUP, &GROUP),
    BuiltIn::new(&animatix_std::catalog::MASK, &MASK),
    BuiltIn::new(&animatix_std::catalog::FILTER, &FILTER),
    BuiltIn::new(&animatix_std::catalog::GLASS, &GLASS),
    BuiltIn::new(&animatix_std::catalog::EQUATION, &EQUATION),
    // Equation / Fragment
    BuiltIn::new(&animatix_std::catalog::FRAGMENT, &FRAGMENT),
    // Annotations
    BuiltIn::new(&animatix_std::catalog::CALLOUT, &CALLOUT),
    BuiltIn::new(&animatix_std::catalog::LEGEND, &LEGEND),
];

// ── Built-in metadata ───────────────────────────────────────────────────

pub use animatix_std::PrimitiveInfo;

/// The built-in primitive catalog (the `animatix-std` identity cards).
pub fn primitive_catalog() -> &'static [&'static PrimitiveInfo] {
    animatix_std::CATALOG
}

use std::sync::OnceLock;

/// Look up built-in metadata by the actor's authored type name.
pub fn primitive_info_by_name(name: &str) -> Option<&'static PrimitiveInfo> {
    animatix_std::catalog_lookup(name)
}

/// Expose built-in primitive metadata through the shared schema model.
///
/// Single conversion implementation lives on [`PrimitiveRegistry::specs`];
/// this free function is a thin wrapper over the built-in registry so the
/// spec-building logic is not duplicated.
pub fn primitive_specs() -> Vec<animatix_syntax::schema::PrimitiveSpec> {
    builtin_primitive_registry().specs()
}

// ── Dispatch helpers ────────────────────────────────────────────────────

/// Look up a built-in primitive by type name.
///
/// This compatibility helper is backed by the same [`PrimitiveRegistry`] used
/// by timeline builds, not by a separate static lookup.
pub fn find_primitive(ty: &str) -> Option<&'static dyn Primitive> {
    builtin_primitive_registry().find(ty)
}

fn builtin_primitive_registry() -> &'static PrimitiveRegistry {
    static REGISTRY: OnceLock<PrimitiveRegistry> = OnceLock::new();
    REGISTRY.get_or_init(PrimitiveRegistry::new)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_primitives_have_unique_type_names() {
        let mut seen = std::collections::HashSet::new();
        for entry in BUILT_INS.iter() {
            let name = entry.behavior.type_name();
            assert!(seen.insert(name), "Duplicate type_name: {:?}", name);
        }
    }

    #[test]
    fn size_override_is_authored_in_full_extents() {
        // Shape state stores half-extents, so the frame override has to halve
        // exactly like the declaration path does.
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("size".to_string(), Value::Vec2([100.0, 60.0]));
        assert_eq!(resolve_half_size([50.0, 50.0], Some(&overrides)), [50.0, 30.0]);
    }

    #[test]
    fn radius_overrides_replace_one_axis_of_the_size_override() {
        // `radius_x`/`radius_y` are half-size components, so they apply on top
        // of a `size` box without the halving, and each touches only its axis.
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("size".to_string(), Value::Vec2([100.0, 100.0]));
        overrides.insert("radius_x".to_string(), Value::Num(20.0));
        assert_eq!(resolve_half_size([70.0, 70.0], Some(&overrides)), [20.0, 50.0]);

        // Without a `size` override the keyframed half-size survives on the
        // axis no radius touched.
        let mut only_y = std::collections::HashMap::new();
        only_y.insert("radius_y".to_string(), Value::Num(25.0));
        assert_eq!(resolve_half_size([50.0, 50.0], Some(&only_y)), [50.0, 25.0]);
    }

    #[test]
    fn half_size_falls_back_to_the_track_without_usable_overrides() {
        // No overrides at all, and a wrongly-typed `size`, both keep the
        // keyframed value rather than collapsing the shape to zero.
        assert_eq!(resolve_half_size([40.0, 30.0], None), [40.0, 30.0]);
        let mut bad = std::collections::HashMap::new();
        bad.insert("size".to_string(), Value::Num(100.0));
        assert_eq!(resolve_half_size([40.0, 30.0], Some(&bad)), [40.0, 30.0]);
    }

    #[test]
    fn text_color_override_reaches_shape_style() {
        // `always { l.stroke = "#30d158" }` arrives in the frame override map as a
        // `Value::Str`. Hex text has to land the same way a tuple does, and a
        // non-color under a color key has to leave the keyframed value alone.
        let track = AnimationTrack::new("l".to_string(), "Line");
        let mut overrides = std::collections::HashMap::new();
        overrides.insert("color".to_string(), Value::Str("#ff2d55".to_string()));
        overrides.insert("stroke".to_string(), Value::Num(0.5));
        let style = sample_shape_style(&track, 0, Some(&overrides));
        let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(
            near(style.color, [1.0, 45.0 / 255.0, 85.0 / 255.0, 1.0]),
            "hex override: {:?}",
            style.color
        );
        assert!(
            near(style.stroke_color, DEFAULT_WHITE),
            "non-color override must not clobber: {:?}",
            style.stroke_color
        );
    }

    #[test]
    fn image_display_transform_maps_pixels_to_display_box() {
        // Map the four corners of the texture rect (drawn by Vello at its pixel
        // size, 1 px = 1 unit) through the transform and check the bbox.
        let corners = |t: &kurbo::Affine, w: f64, h: f64| -> (f64, f64) {
            let xs = [
                (*t * kurbo::Point::new(0.0, 0.0)).x,
                (*t * kurbo::Point::new(w, 0.0)).x,
                (*t * kurbo::Point::new(0.0, h)).x,
                (*t * kurbo::Point::new(w, h)).x,
            ];
            let ys = [
                (*t * kurbo::Point::new(0.0, 0.0)).y,
                (*t * kurbo::Point::new(w, 0.0)).y,
                (*t * kurbo::Point::new(0.0, h)).y,
                (*t * kurbo::Point::new(w, h)).y,
            ];
            let c = |v: &[f64]| {
                v.iter().cloned().fold(f64::NAN, f64::max)
                    - v.iter().cloned().fold(f64::NAN, f64::min)
            };
            (c(&xs), c(&ys))
        };
        // A 24×24 texture in a 240×160 actor box must scale by (240/24, 160/24):
        // the drawn quad then covers exactly the actor box (0..240 × 0..160).
        let t = image_display_transform([240.0, 160.0], [24.0, 24.0], [0.0, 0.0]);
        let (w, h) = corners(&t, 24.0, 24.0);
        assert!((w - 240.0).abs() < 0.001, "width {w}");
        assert!((h - 160.0).abs() < 0.001, "height {h}");

        // Plugin-style command: a 24×24 texture with an explicit 72×72 display
        // box stamps the texture over the full box (scale 3×3).
        let t = image_display_transform([72.0, 72.0], [24.0, 24.0], [0.0, 0.0]);
        let (w, h) = corners(&t, 24.0, 24.0);
        assert!((w - 72.0).abs() < 0.001);
        assert!((h - 72.0).abs() < 0.001);

        // Offset shifts the box in local space before the actor transform.
        let t = image_display_transform([100.0, 100.0], [1.0, 1.0], [30.0, 40.0]);
        let p = t * kurbo::Point::new(0.0, 0.0);
        assert!((p.x - 30.0).abs() < 0.001);
        assert!((p.y - 40.0).abs() < 0.001);

        // Zero / degenerate texture sizes degrade to a 1×1 guard instead of NaN.
        let t = image_display_transform([100.0, 100.0], [0.0, 0.0], [0.0, 0.0]);
        assert!(t.as_coeffs().iter().all(|c| c.is_finite()));
    }

    #[test]
    fn find_primitive_roundtrips() {
        for entry in BUILT_INS.iter() {
            let ty = entry.behavior.type_name();
            let found = find_primitive(ty);
            assert!(found.is_some(), "find_primitive({:?}) returned None", ty);
            assert_eq!(found.unwrap().type_name(), ty);
        }
    }

    #[test]
    fn child_processing_capabilities_cover_special_containers() {
        assert_eq!(
            animatix_std::catalog_lookup("Filter")
                .expect("Filter built-in")
                .child_processing,
            ChildProcessing::Filter
        );
        assert_eq!(
            animatix_std::catalog_lookup("Mask").expect("Mask built-in").child_processing,
            ChildProcessing::Mask
        );
        assert_eq!(
            animatix_std::catalog_lookup("Equation")
                .expect("Equation built-in")
                .child_processing,
            ChildProcessing::Equation
        );
        assert_eq!(
            animatix_std::catalog_lookup("Row").expect("Row built-in").child_processing,
            ChildProcessing::Generic
        );
    }

    #[test]
    fn registry_matches_primitives() {
        // Each BUILT_INS row pairs a behaviour with the catalog card it
        // references by symbol, so positional drift is impossible; this test
        // pins the remaining failure mode — a name mismatch or a set drift
        // (card added without a behaviour, or vice versa). Ordering of
        // either list is presentation-only.
        for entry in BUILT_INS.iter() {
            assert_eq!(
                &*entry.info.type_name,
                entry.behavior.type_name(),
                "BUILT_INS row pairs a behaviour with the wrong catalog card"
            );
        }
        let catalog_names: std::collections::HashSet<&str> =
            primitive_catalog().iter().map(|info| &*info.type_name).collect();
        assert_eq!(
            catalog_names.len(),
            primitive_catalog().len(),
            "duplicate type_name in the animatix-std catalog"
        );
        let behavior_names: std::collections::HashSet<&str> =
            BUILT_INS.iter().map(|entry| entry.behavior.type_name()).collect();
        assert_eq!(catalog_names, behavior_names, "catalog and BUILT_INS drifted apart");
    }

    #[test]
    fn clip_bezpath_concatenates_every_path_and_command() {
        use kurbo::Shape;
        // A multi-path shape must contribute all its geometry to the clip; the
        // earlier default dropped everything but the first path.
        let rect_path = |x: f64| VelloPath {
            path: std::sync::Arc::new(kurbo::Rect::new(x, 0.0, x + 10.0, 10.0).into_path(1e-3)),
            ..VelloPath::default()
        };
        let one = clip_bezpath_from_commands(&[RenderCommand::Paths {
            paths: vec![rect_path(0.0)],
        }])
        .expect("one path");
        let two_commands = clip_bezpath_from_commands(&[
            RenderCommand::Paths {
                paths: vec![rect_path(0.0)],
            },
            RenderCommand::Paths {
                paths: vec![rect_path(20.0)],
            },
        ])
        .expect("two commands");
        let two_paths_one_command = clip_bezpath_from_commands(&[RenderCommand::Paths {
            paths: vec![rect_path(0.0), rect_path(20.0)],
        }])
        .expect("two paths");

        assert_eq!(two_commands.elements().len(), one.elements().len() * 2);
        assert_eq!(two_paths_one_command.elements().len(), one.elements().len() * 2);
        assert!(clip_bezpath_from_commands(&[]).is_none());
    }

    #[test]
    fn primitive_specs_cover_builtins_with_capabilities() {
        let specs = primitive_specs();
        assert_eq!(specs.len(), BUILT_INS.len());
        let rect = specs.iter().find(|spec| spec.type_name == "Rect").expect("Rect is a built-in");
        assert_eq!(rect.category, animatix_syntax::schema::PrimitiveCategory::Shape);
        assert!(rect.capabilities.vector_paths);
        let row = specs.iter().find(|spec| spec.type_name == "Row").expect("Row is a built-in");
        assert_eq!(row.category, animatix_syntax::schema::PrimitiveCategory::Container);
        assert!(row.capabilities.layout_container);
    }

    #[test]
    fn every_catalog_row_derives_caps() {
        for info in animatix_std::CATALOG {
            let caps = animatix_std::caps_from_info(info);
            let _ = caps; // derivation must be total over built-ins
        }
    }

    #[test]
    fn runtime_text_content_crossfade_compiles_both_endpoints() {
        use crate::easing::Easing;
        use crate::renderer::text::{FontContext, TextCompiler, TextKind};
        use crate::timeline::property_track::PropertyTrack;
        use crate::timeline::{AnimationTrack, SceneDimensions};

        let mut track = AnimationTrack::placeholder("label".to_string());
        let mut content = PropertyTrack::new("Hello".to_string());
        content.add_keyframe(0, "Hello".to_string(), Easing::Linear);
        content.add_keyframe(1000, "Hello".to_string(), Easing::Linear);
        content.add_keyframe(2000, "World".to_string(), Easing::Linear);
        track.text.text_content = Some(content);
        track.text.font_size = Some(PropertyTrack::new(48.0));

        let font_ctx = FontContext::new();
        let mut text_compiler = TextCompiler::new();
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let ctx = EvaluateCtx {
            track: &track,
            time_ms: 1500,
            local_transform: kurbo::Affine::IDENTITY,
            opacity: 1.0,
            scene_dimensions: SceneDimensions {
                width: 640,
                height: 480,
            },
            background_color: [0.0; 4],
            overrides: None,
            vector_paths: &[],
            asset_cache: &asset_cache,
            target_resolver: None,
        };
        let mut text_ctx = TextCompileCtx {
            text_compiler: &mut text_compiler,
            font_context: &font_ctx,
        };
        let paths =
            evaluate_text_paths(&ctx, &mut text_ctx, TextKind::Text, 48.0).expect("compile text");
        assert!(paths.len() > 5, "Expected both endpoint glyph sets, got {}", paths.len());
        assert!(
            paths.iter().all(|p| p.opacity > 0.0 && p.opacity < 1.0),
            "Expected midpoint cross-fade opacities"
        );
    }
}
