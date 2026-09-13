//! Unified primitive system for Animatix.
//!
//! Every actor type (shape, text, media, plot, container) is a `Primitive`.
//! `PRIMITIVES` is the bootstrap list; `PrimitiveRegistry` seeds built-ins
//! through the same registration path used by extensions.
//!
//! ## Architecture
//!
//! ```text
//! PRIMITIVES array (bootstrap, behaviour)
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
//! 1. Add the identity card: a `PrimitiveInfo` row in
//!    `animatix-std/src/catalog.rs::CATALOG` (type/display/icon/category/
//!    advanced/capabilities/child processing). Tooling, the parser's contract
//!    tables, and the inspector palette all derive from it.
//! 2. Create `primitives/<name>.rs` implementing `Primitive` (behaviour only —
//!    no metadata methods).
//! 3. Add `&<name>::CONST` to the `PRIMITIVES` array below.
//! 4. If the primitive has properties, add rows to
//!    `animatix-syntax/src/schema.rs::raw_property_specs()` (one applicability
//!    predicate + type); `property_specs()` materializes the per-type lists
//!    over the catalog.
//! 5. Document it (docs/primitives.md, docs/spec.md) and add render/hit-region
//!    coverage if it draws.
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
use crate::diagnostics::Diagnostic;
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

    let mut content_override: Option<String> = None;
    if let Some(ov) = ctx.overrides {
        if let Some(Value::Str(s)) = ov
            .get("text")
            .or_else(|| ov.get("code"))
            .or_else(|| ov.get("math"))
            .or_else(|| ov.get("latex"))
            .or_else(|| ov.get("content"))
        {
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
        if let Some(Value::Num(n)) = ov.get("max_width") {
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
            text_ctx.font_context,
            max_width,
            &text_align,
            &overflow,
        )
    } else {
        Ok(std::sync::Arc::from(ctx.track.evaluate_text_paths(ctx.time_ms)))
    }
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
        if let Some(Value::Color(c) | Value::Vec4(c)) = node_overrides.get("color") {
            color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
        }
        if let Some(Value::Color(c) | Value::Vec4(c)) =
            node_overrides.get("stroke_color").or_else(|| node_overrides.get("stroke"))
        {
            stroke_color = [c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32];
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
/// - **hit** (inputs equal): `take_shape_commands` hands the cached `Vec` to
///   the caller, which encodes it into the frame's vello scene, clones it for
///   the observable `SceneItem` when item collection is requested, and hands
///   it back via `recycle_shape_commands` — the next frame's hit reuses the
///   same buffers.
/// - **miss** (inputs changed): the caller builds a fresh `Vec`, clones it
///   once into the memo slot, and still returns the fresh one.
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
    let style = sample_shape_style(ctx.track, ctx.time_ms, ctx.overrides);
    let epoch = ctx.track.shape.vector_paths_epoch.get();
    if let Some((commands, bounds)) = ctx.track.take_shape_commands(epoch, &style, state) {
        ctx.track.offer_shape_command_bounds(bounds);
        return Ok(Some(commands));
    }
    ctx.track
        .build_shape_commands(epoch, style, state, primitive, ctx.time_ms)
        .map(Some)
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
mod svg;
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

/// Mutable context passed to [`Primitive::render_children`].
///
/// The scene-subtree renderer hands each container primitive this context so the
/// primitive drives its own recursion, instead of the pipeline branching on a
/// `ChildProcessing` value. `scene`, `hit_regions`, `program_items`, and
/// `filter_backend` are the caller-local outputs; every other field is
/// read-only frame state. The recursion entry point (`Timeline::evaluate_node`)
/// and the frame caches are reached through [`Self::timeline`].
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

impl RenderChildrenCtx<'_, '_, '_> {
    /// The container track these children belong to.
    pub fn track(&self) -> Option<&AnimationTrack> {
        self.timeline.tracks.get(self.node_label)
    }

    /// Render one child into this context's scene, preserving the caller's
    /// `allow_pending_composites` flag.
    pub fn render_child(&mut self, child: &str) {
        let allow_pending_composites = self.allow_pending_composites;
        self.timeline.evaluate_node(
            child,
            self.time_ms,
            self.global_transform,
            self.global_opacity,
            self.scene_dimensions,
            self.debug_options,
            &mut *self.scene,
            self.overrides,
            &self.layout_positions,
            &mut *self.hit_regions,
            self.frame_env,
            &mut *self.filter_backend,
            allow_pending_composites,
            &mut *self.program_items,
        );
    }

    /// Render one child into a caller-provided scene (the Filter strategy
    /// renders into an offscreen sub-scene, which never parks composites).
    pub fn render_child_into(&mut self, scene: &mut vello::Scene, child: &str) {
        self.timeline.evaluate_node(
            child,
            self.time_ms,
            self.global_transform,
            self.global_opacity,
            self.scene_dimensions,
            self.debug_options,
            scene,
            self.overrides,
            &self.layout_positions,
            &mut *self.hit_regions,
            self.frame_env,
            &mut *self.filter_backend,
            false,
            &mut *self.program_items,
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
                for path in paths {
                    if let Some(mut fc) = path.fill {
                        fc = fc.with_alpha(fc.components[3] * opacity);
                        scene.fill(
                            vello::peniko::Fill::NonZero,
                            *transform,
                            fc,
                            None,
                            path.path.as_ref(),
                        );
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
                        let stroke = vello::kurbo::Stroke {
                            width: sw as f64,
                            join,
                            miter_limit: 10.0,
                            start_cap: cap,
                            end_cap: cap,
                            dash_pattern: Default::default(),
                            dash_offset: 0.0,
                        };
                        scene.stroke(&stroke, *transform, sc, None, path.path.as_ref());
                    }
                }
            },
            RenderCommand::Text { paths } => {
                for text_path in paths.iter() {
                    let color = match &text_path.color {
                        ::typst::visualize::Paint::Solid(color) => {
                            let rgba = color.to_vec4_u8();
                            vello::peniko::Color::from_rgba8(
                                rgba[0],
                                rgba[1],
                                rgba[2],
                                (rgba[3] as f32 * opacity * text_path.opacity) as u8,
                            )
                        },
                        _ => vello::peniko::Color::WHITE,
                    };
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
            "stroke" | "stroke_color" => match caps.category {
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
    /// - `Ok(Some(commands))` — `scene_eval.rs` executes the commands with the
    ///   actor's local transform and inherited opacity, and records a hit
    ///   region derived from their local bounds.
    /// - `Ok(None)` — "no drawable content" (e.g. empty text, missing image).
    ///   Nothing is drawn and no hit region / precise bounds are recorded
    ///   (empty-content actors stay un-pickable until they have content;
    ///   pinned by `runtime_empty_text_override_clears_stale_glyphs`).
    /// - `Err(e)` — the actor does not render and a `RenderFailure` runtime
    ///   diagnostic is recorded.
    fn evaluate(
        &self,
        _ctx: &EvaluateCtx,
        _text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        Ok(None)
    }
}

// ── The one static array ────────────────────────────────────────────────

/// Bootstrap list of all built-in primitives.
///
/// `PrimitiveRegistry::new()` registers these through the same `register`
/// path used by extension primitives.
pub static PRIMITIVES: &[&dyn Primitive] = &[
    // Shapes
    &RECT,
    &ELLIPSE,
    &LINE,
    &ARROW,
    &POLYGON,
    &PATH,
    // Text
    &TEXT,
    &CODE,
    &MATH,
    &TYPST,
    // Media
    &IMAGE,
    &SVG,
    &AUDIO,
    // Plots
    &GRAPH,
    &PLOT_CURVE,
    &VECTOR_FIELD,
    &HEATMAP,
    &CONTOUR_SET,
    &NUMBER_PLANE,
    &BAR_CHART,
    // Containers
    &ROW,
    &COL,
    &GRID,
    &STACK,
    &GROUP,
    &MASK,
    &FILTER,
    // Equation / Fragment
    &EQUATION,
    &FRAGMENT,
    // Annotations
    &CALLOUT,
    &LEGEND,
];

// ── Built-in metadata ───────────────────────────────────────────────────

pub use animatix_std::PrimitiveInfo;

/// The built-in primitive catalog (the `animatix-std` identity cards).
pub fn primitive_catalog() -> &'static [PrimitiveInfo] {
    animatix_std::CATALOG
}

use std::sync::OnceLock;

/// Look up built-in metadata by the actor's authored type name.
pub fn primitive_info_by_name(name: &str) -> Option<&'static PrimitiveInfo> {
    primitive_catalog().iter().find(|m| m.type_name == name)
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
        for p in PRIMITIVES.iter() {
            let name = p.type_name();
            assert!(seen.insert(name), "Duplicate type_name: {:?}", name);
        }
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
        for p in PRIMITIVES.iter() {
            let found = find_primitive(p.type_name());
            assert!(found.is_some(), "find_primitive({:?}) returned None", p.type_name());
            assert_eq!(found.unwrap().type_name(), p.type_name());
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
        // The catalog (metadata) and the PRIMITIVES array (behaviour) must
        // stay in lockstep — one row per registered behaviour.
        let registry = primitive_catalog();
        assert_eq!(registry.len(), PRIMITIVES.len());
        for (meta, prim) in registry.iter().zip(PRIMITIVES.iter()) {
            assert_eq!(meta.type_name, prim.type_name());
        }
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
        assert_eq!(specs.len(), PRIMITIVES.len());
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
        use crate::timeline::{AnimationTrack, SceneDimensions, property_track::PropertyTrack};

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
