//! Unified primitive system for Animatix.
//!
//! Every actor type (shape, text, media, plot, container) is a `Primitive`.
//! `PRIMITIVES` is the bootstrap list; `PrimitiveRegistry` seeds built-ins
//! through the same registration path used by extensions.
//!
//! ## Architecture
//!
//! ```text
//! PRIMITIVES array (bootstrap)
//!        │
//!        ├──► PrimitiveRegistry (single storage for built-ins + extensions)
//!        ├──► ActorKindMeta registry (auto-generated via OnceLock)
//!        ├──► find_primitive() — compatibility lookup for static built-ins
//!        └──► ActorKind dispatch (via PrimitiveActorKind wrapper)
//! ```
//!
//! ## Adding a new primitive
//!
//! 1. Create `primitives/<name>.rs` implementing `Primitive`
//! 2. Add `&<name>::CONST` to the `PRIMITIVES` array below
//! 3. Add variant to `ActorKindId` in `timeline/track.rs`
//! 4. If it's a shape, add variant to `ShapeKind` in `timeline/track.rs`
//!
//! Steps 3-4 are required because enums are used in match arms across
//! the codebase and cannot be auto-generated from a static array.
//! However, the metadata registry (`ActorKindMeta`) IS auto-generated
//! from `PRIMITIVES`, so you never need to touch the registry manually.
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
    ActorCategory, ActorKindId, AnimationTrack, DEFAULT_WHITE, Environment, SceneDimensions,
    Timeline, TrackAccessor, Value, VectorShapeState, VectorShapeStyle, VelloPath,
    default_stroke_width,
};

/// Evaluate text paths for a text primitive at frame time.
///
/// This replicates the logic in `scene_eval.rs::evaluate_text_node` so that
/// text primitives can dispatch via `Primitive::evaluate()` instead of the
/// legacy path.
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
    let mut stroke_width = track.style.stroke_width.get(time_ms, default_stroke_width(track.kind));
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

/// Helper: sample style, render shape, wrap in RenderCommand::Paths.
pub(crate) fn evaluate_shape_render(
    primitive: &dyn Primitive,
    ctx: &EvaluateCtx,
    state: &VectorShapeState,
) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
    let style = sample_shape_style(ctx.track, ctx.time_ms, ctx.overrides);
    let paths = primitive
        .render(&RenderCtx {
            state,
            style,
            time_ms: ctx.time_ms,
        })
        .unwrap_or_default();
    Ok(Some(vec![RenderCommand::Paths { paths }]))
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
#[cfg(feature = "render")]
mod image;
#[cfg(feature = "render")]
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
    /// Narrow resolver for target actor bounds (targeted callout mode).
    /// Replaces the previous broad `Option<&Timeline>` field.
    pub target_resolver: Option<&'a dyn TargetResolver>,
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
#[derive(Clone, Debug)]
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
    #[cfg(feature = "render")]
    Image {
        /// The image data.
        image: crate::timeline::image::SceneImage,
        /// Natural (display) width and height in scene units.
        natural_size: [f32; 2],
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

impl RenderCommand {
    /// Execute this command into a Vello scene with the given transform and opacity.
    pub fn execute(&self, scene: &mut vello::Scene, transform: &kurbo::Affine, opacity: f32) {
        match self {
            RenderCommand::Paths { paths } => {
                for path in paths {
                    if let Some(mut fc) = path.fill {
                        fc = fc.with_alpha(fc.components[3] * opacity);
                        scene.fill(vello::peniko::Fill::NonZero, *transform, fc, None, &path.path);
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
                        scene.stroke(&stroke, *transform, sc, None, &path.path);
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
            } => {
                let [nw, nh] = *natural_size;
                let image_transform = *transform
                    * kurbo::Affine::scale_non_uniform(
                        (image.natural_size[0] * 2.0 / nw) as f64,
                        (image.natural_size[1] * 2.0 / nh) as f64,
                    );
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
                    bounds = union(
                        bounds,
                        kurbo::Rect::new(0.0, 0.0, (half_w * 2.0) as f64, (half_h * 2.0) as f64),
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
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChildProcessing {
    /// Render children through the normal scene graph recursion.
    #[default]
    Generic,
    /// Render children through the offscreen filter pipeline.
    Filter,
    /// Render children inside a clip mask.
    Mask,
    /// Render children as one aggregated equation document.
    Equation,
}

/// Every actor type in Animatix implements this trait.
///
/// Metadata, build logic, and (optionally) render logic live in one place.
pub trait Primitive: Send + Sync {
    // ── Metadata ──

    /// Source-text type name, e.g. "Rect", "Text", "Row".
    fn type_name(&self) -> &'static str;

    /// Human-readable label for UI palettes and tooltips.
    fn display_name(&self) -> &'static str;

    /// UI category (Shapes, Text, Media, Plots, Containers).
    fn category(&self) -> ActorCategory;

    /// Opaque icon identifier. The GUI maps this to a concrete icon.
    fn icon_id(&self) -> &'static str;

    /// When true, shown in a "More..." submenu instead of top-level.
    fn is_advanced(&self) -> bool {
        false
    }

    /// Returns true if this primitive is a layout container.
    fn is_container(&self) -> bool {
        false
    }

    /// Returns true if this primitive renders as a vector shape.
    fn is_shape(&self) -> bool {
        false
    }

    /// Shared schema capabilities for this primitive.
    ///
    /// This is the migration point for metadata that previously lived in
    /// `PrimitiveDescriptor` string matching. Primitives can override it when
    /// they need capabilities beyond the category defaults.
    fn capabilities(&self) -> animatix_syntax::schema::PrimitiveCapabilities {
        match self.category() {
            ActorCategory::Shape => animatix_syntax::schema::PrimitiveCapabilities {
                vector_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                is_shape: true,
                ..animatix_syntax::schema::PrimitiveCapabilities::default()
            },
            ActorCategory::Text => animatix_syntax::schema::PrimitiveCapabilities {
                text_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                ..animatix_syntax::schema::PrimitiveCapabilities::default()
            },
            ActorCategory::Media => match self.type_name() {
                "Svg" => animatix_syntax::schema::PrimitiveCapabilities {
                    vector_paths: true,
                    morphable_paths: true,
                    vector_reveal_target: true,
                    ..animatix_syntax::schema::PrimitiveCapabilities::default()
                },
                "Image" => animatix_syntax::schema::PrimitiveCapabilities {
                    image_payload: true,
                    ..animatix_syntax::schema::PrimitiveCapabilities::default()
                },
                _ => animatix_syntax::schema::PrimitiveCapabilities::default(),
            },
            ActorCategory::Plot => animatix_syntax::schema::PrimitiveCapabilities {
                vector_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                plot_geometry: true,
                ..animatix_syntax::schema::PrimitiveCapabilities::default()
            },
            ActorCategory::Container => {
                if matches!(self.type_name(), "Group" | "Mask") {
                    animatix_syntax::schema::PrimitiveCapabilities {
                        is_container: true,
                        ..animatix_syntax::schema::PrimitiveCapabilities::default()
                    }
                } else {
                    animatix_syntax::schema::PrimitiveCapabilities {
                        layout_container: true,
                        is_container: true,
                        ..animatix_syntax::schema::PrimitiveCapabilities::default()
                    }
                }
            },
            ActorCategory::Annotation => animatix_syntax::schema::PrimitiveCapabilities {
                vector_paths: true,
                ..animatix_syntax::schema::PrimitiveCapabilities::default()
            },
        }
    }

    /// Child-processing capability used by the scene subtree renderer.
    ///
    /// Container primitives override this when `scene_eval` must aggregate or
    /// transform children before drawing them.
    fn child_processing(&self) -> ChildProcessing {
        ChildProcessing::Generic
    }

    /// Returns the corresponding `ActorKindId` variant.
    fn kind_id(&self) -> ActorKindId;

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

    /// Returns true if this shape uses a custom path (Polygon, Path).
    fn uses_custom_path(&self) -> bool {
        false
    }

    /// Returns true if this shape exposes tip size properties (Line with arrows).
    fn exposes_tip_size(&self) -> bool {
        false
    }

    /// Returns true if this shape supports fill.
    fn supports_fill(&self) -> bool {
        true
    }

    /// Returns the colorscheme key for default color lookup.
    /// For example, "Text" returns "text.primary", shapes return "accent.primary".
    fn default_color_key(&self, property: &str) -> Option<&'static str> {
        match property {
            "color" => match self.category() {
                ActorCategory::Text => Some("text.primary"),
                ActorCategory::Shape | ActorCategory::Plot => Some("surface.primary"),
                ActorCategory::Media => Some("text.primary"),
                ActorCategory::Container => None,
                ActorCategory::Annotation => None,
            },
            "stroke" | "stroke_color" => match self.category() {
                ActorCategory::Shape => Some("stroke.default"),
                _ => None,
            },
            _ => None,
        }
    }

    /// How the GUI should resize this actor.
    fn resize_mode(&self) -> crate::timeline::ResizeMode {
        match self.category() {
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
    /// When this returns `Some(commands)`, `scene_eval.rs` will execute the
    /// commands directly and skip the legacy manual `ActorKindId` match for
    /// this actor.  When it returns `None`, the legacy path is used.
    ///
    /// This is the migration path away from the 1000+ line `scene_eval.rs`
    /// match blocks.  New primitives should implement this method; existing
    /// primitives will be migrated incrementally.
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
    &TYPST,
    // Media
    &IMAGE,
    #[cfg(feature = "svg")]
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

// ── Auto-generated registry ─────────────────────────────────────────────

/// Static metadata generated from `PRIMITIVES`.
/// Built once at first access via `OnceLock`.
pub struct ActorKindMeta {
    /// Actor kind identifier.
    pub kind: ActorKindId,
    /// Source-text type name.
    pub type_name: &'static str,
    /// Human-readable display name.
    pub display_name: &'static str,
    /// UI category.
    pub category: ActorCategory,
    /// Icon identifier.
    pub icon_id: &'static str,
    /// Whether shown in advanced submenu.
    pub advanced: bool,
}

use std::sync::OnceLock;

static REGISTRY_LOCK: OnceLock<Vec<ActorKindMeta>> = OnceLock::new();

fn build_registry() -> Vec<ActorKindMeta> {
    PRIMITIVES
        .iter()
        .map(|p| ActorKindMeta {
            kind: p.kind_id(),
            type_name: p.type_name(),
            display_name: p.display_name(),
            category: p.category(),
            icon_id: p.icon_id(),
            advanced: p.is_advanced(),
        })
        .collect()
}

/// Get the auto-generated metadata registry.
pub fn actor_kind_registry() -> &'static [ActorKindMeta] {
    REGISTRY_LOCK.get_or_init(build_registry)
}

/// Look up metadata by `ActorKindId`.
pub fn actor_kind_meta(kind: ActorKindId) -> Option<&'static ActorKindMeta> {
    actor_kind_registry().iter().find(|m| m.kind == kind)
}

/// Look up metadata by type name.
pub fn actor_kind_meta_by_name(name: &str) -> Option<&'static ActorKindMeta> {
    actor_kind_registry().iter().find(|m| m.type_name == name)
}

/// Expose built-in primitive metadata through the shared schema model.
pub fn primitive_specs() -> Vec<animatix_syntax::schema::PrimitiveSpec> {
    actor_kind_registry()
        .iter()
        .map(|meta| {
            let capabilities = crate::primitives::find_primitive(meta.type_name)
                .map(|primitive| primitive.capabilities())
                .unwrap_or_default();
            animatix_syntax::schema::PrimitiveSpec {
                type_name: meta.type_name,
                display_name: meta.display_name,
                category: match meta.category {
                    ActorCategory::Shape => animatix_syntax::schema::PrimitiveCategory::Shape,
                    ActorCategory::Text => animatix_syntax::schema::PrimitiveCategory::Text,
                    ActorCategory::Media => animatix_syntax::schema::PrimitiveCategory::Media,
                    ActorCategory::Plot => animatix_syntax::schema::PrimitiveCategory::Plot,
                    ActorCategory::Container => {
                        animatix_syntax::schema::PrimitiveCategory::Container
                    },
                    ActorCategory::Annotation => {
                        animatix_syntax::schema::PrimitiveCategory::Annotation
                    },
                },
                icon_id: meta.icon_id,
                advanced: meta.advanced,
                capabilities: animatix_syntax::schema::PrimitiveCapabilities {
                    text_paths: capabilities.text_paths,
                    vector_paths: capabilities.vector_paths,
                    image_payload: capabilities.image_payload,
                    layout_container: capabilities.layout_container,
                    morphable_paths: capabilities.morphable_paths,
                    vector_reveal_target: capabilities.vector_reveal_target,
                    plot_geometry: capabilities.plot_geometry,
                    is_container: meta.category == ActorCategory::Container,
                    is_shape: meta.category == ActorCategory::Shape,
                },
            }
        })
        .collect()
}

// ── Dispatch helpers ────────────────────────────────────────────────────

/// Look up a primitive by its type name.
pub fn find_primitive(ty: &str) -> Option<&'static dyn Primitive> {
    PRIMITIVES.iter().find(|p| p.type_name() == ty).copied()
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
            find_primitive("Filter").expect("Filter built-in").child_processing(),
            ChildProcessing::Filter
        );
        assert_eq!(
            find_primitive("Mask").expect("Mask built-in").child_processing(),
            ChildProcessing::Mask
        );
        assert_eq!(
            find_primitive("Equation").expect("Equation built-in").child_processing(),
            ChildProcessing::Equation
        );
        assert_eq!(
            find_primitive("Row").expect("Row built-in").child_processing(),
            ChildProcessing::Generic
        );
    }

    #[test]
    fn registry_matches_primitives() {
        let registry = actor_kind_registry();
        assert_eq!(registry.len(), PRIMITIVES.len());
        for (meta, prim) in registry.iter().zip(PRIMITIVES.iter()) {
            assert_eq!(meta.kind, prim.kind_id());
            assert_eq!(meta.type_name, prim.type_name());
            assert_eq!(meta.display_name, prim.display_name());
            assert_eq!(meta.category, prim.category());
            assert_eq!(meta.icon_id, prim.icon_id());
            assert_eq!(meta.advanced, prim.is_advanced());
        }
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
    fn every_kind_id_has_meta() {
        // This enumerates all variants and verifies they're in the registry
        use crate::timeline::ShapeKind;
        let registry = actor_kind_registry();
        let kinds: std::collections::HashSet<_> = registry.iter().map(|m| m.kind).collect();

        let shape_kinds = [
            ShapeKind::Rect,
            ShapeKind::Ellipse,
            ShapeKind::Line,
            ShapeKind::Polygon,
            ShapeKind::Path,
            ShapeKind::Arrow,
        ];
        for sk in &shape_kinds {
            let id = ActorKindId::Shape(*sk);
            assert!(kinds.contains(&id), "Missing ActorKindMeta for ShapeKind::{:?}", sk);
        }

        for id in [
            ActorKindId::Text,
            ActorKindId::Code,
            ActorKindId::Typst,
            ActorKindId::Image,
            ActorKindId::Svg,
            ActorKindId::Graph,
            ActorKindId::PlotCurve,
            ActorKindId::VectorField,
            ActorKindId::Heatmap,
            ActorKindId::ContourSet,
            ActorKindId::NumberPlane,
            ActorKindId::Row,
            ActorKindId::Col,
            ActorKindId::Grid,
            ActorKindId::Stack,
            ActorKindId::Group,
            ActorKindId::Mask,
            ActorKindId::Filter,
            ActorKindId::Audio,
            ActorKindId::Equation,
            ActorKindId::Fragment,
            ActorKindId::Callout,
            ActorKindId::Legend,
        ] {
            assert!(kinds.contains(&id), "Missing ActorKindMeta for {:?}", id);
        }
    }

    #[test]
    fn runtime_text_content_crossfade_compiles_both_endpoints() {
        use crate::easing::Easing;
        use crate::renderer::text::{FontContext, TextCompiler, TextKind};
        use crate::timeline::{AnimationTrack, SceneDimensions, property_track::PropertyTrack};

        let mut track = AnimationTrack::new("label".to_string());
        track.kind = ActorKindId::Text;
        let mut content = PropertyTrack::new("Hello".to_string());
        content.add_keyframe(0, "Hello".to_string(), Easing::Linear);
        content.add_keyframe(1000, "Hello".to_string(), Easing::Linear);
        content.add_keyframe(2000, "World".to_string(), Easing::Linear);
        track.text.text_content = Some(content);
        track.text.font_size = Some(PropertyTrack::new(48.0));

        let font_ctx = FontContext::new();
        let mut text_compiler = TextCompiler::new();
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
