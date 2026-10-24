//! Shape styling: style sampling, shadow, dash, gradient, and progress trimming.

use std::collections::HashMap;

use crate::primitives::commands::RenderCommand;
use crate::primitives::{EvaluateCtx, Primitive, RenderCtx};
use crate::timeline::animation_track::StyleTracks;
use crate::timeline::{
    ActorCaps, AnimationTrack, DEFAULT_WHITE, TrackAccessor, Value, VectorShapeState,
    VectorShapeStyle,
};

/// Helper to read a color override from an `always` block's assignment table.
fn override_color(overrides: &HashMap<String, Value>, key: &str) -> Option<[f32; 4]> {
    match overrides.get(key) {
        Some(Value::Color(c) | Value::Vec4(c)) => {
            Some([c[0] as f32, c[1] as f32, c[2] as f32, c[3] as f32])
        },
        Some(Value::Str(text)) => crate::timeline::utils::color_from_text(text),
        Some(other) => {
            tracing::debug!("`{key}` override is not a color: {other:?}; keeping keyframed value");
            None
        },
        None => None,
    }
}

/// Sample shape style (color, stroke_width, stroke_color, fill_opacity) from a style
/// tier at the given time, applying property overrides when present.
pub fn sample_shape_style(
    style: &StyleTracks,
    caps: ActorCaps,
    time_ms: u64,
    overrides: Option<&HashMap<String, Value>>,
) -> VectorShapeStyle {
    let mut color = style.color.get(time_ms, DEFAULT_WHITE);
    let mut stroke_width = style.stroke_width.get(time_ms, caps.default_stroke_width());
    let mut stroke_color = style.stroke_color.get(time_ms, DEFAULT_WHITE);
    let mut fill_opacity = style.fill_opacity.get(time_ms, 1.0);
    let mut line_cap = style.line_cap.get(time_ms, 0);
    let mut line_join = style.line_join.get(time_ms, 0);

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
#[derive(Default, Clone, Debug)]
pub(crate) struct ShapeCommandMemo {
    epoch: u64,
    style: Option<VectorShapeStyle>,
    state: Option<VectorShapeState>,
    commands: Option<Vec<RenderCommand>>,
    cached_bounds: Option<Option<kurbo::Rect>>,
}

impl ShapeCommandMemo {
    /// True when the memo holds commands built from exactly
    /// `(epoch, style, state)`.
    fn matches(&self, epoch: u64, style: &VectorShapeStyle, state: &VectorShapeState) -> bool {
        self.commands.is_some()
            && self.epoch == epoch
            && self.style.as_ref() == Some(style)
            && self.state.as_ref() == Some(state)
    }

    fn store(
        &mut self,
        epoch: u64,
        style: VectorShapeStyle,
        state: &VectorShapeState,
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

impl AnimationTrack {
    /// Take the memoized command `Vec` out of the slot for reuse by the
    /// caller (see [`ShapeCommandMemo`]). `None` when the inputs changed;
    /// a stale epoch also drops the stale payload so it can never be served.
    pub(crate) fn take_shape_commands(
        &self,
        epoch: u64,
        style: &VectorShapeStyle,
        state: &VectorShapeState,
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
        style: VectorShapeStyle,
        state: &VectorShapeState,
        primitive: &dyn Primitive,
        time_ms: u64,
        shape: &crate::timeline::animation_track::ShapeTracks,
    ) -> Result<Vec<RenderCommand>, crate::renderer::error::RenderError> {
        let paths = primitive
            .render(&RenderCtx {
                state,
                style,
                time_ms,
                track: self,
                shape: Some(shape),
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
            drop(memo);
            self.offer_shape_command_bounds(bounds);
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

    let style_tracks = ctx.style.expect("shape evaluation requires the style tier");
    let style = sample_shape_style(style_tracks, ctx.caps, ctx.time_ms, ctx.overrides);
    let shape = ctx.shape.expect("shape evaluation requires the shape tier");
    let epoch = shape.vector_paths_epoch.get();

    let progress = f64::from(style_tracks.stroke_progress.get(ctx.time_ms, 1.0)).clamp(0.0, 1.0);
    let has_dash = !style_tracks.dash_pattern.get(ctx.time_ms, Vec::new()).is_empty();
    let has_gradient = shape_has_gradient(style_tracks);
    let mut commands = if progress < 1.0 {
        let built =
            ctx.track
                .build_shape_commands(epoch, style, state, primitive, ctx.time_ms, shape)?;
        let mut trimmed = trim_shape_stroke_progress(&built, progress);
        stamp_shape_dash(&mut trimmed, ctx);
        stamp_shape_gradient(&mut trimmed, ctx);
        trimmed
    } else if has_dash || has_gradient {
        let mut built =
            ctx.track
                .build_shape_commands(epoch, style, state, primitive, ctx.time_ms, shape)?;
        if has_dash {
            stamp_shape_dash(&mut built, ctx);
        }
        if has_gradient {
            stamp_shape_gradient(&mut built, ctx);
        }
        built
    } else if let Some((commands, bounds)) = ctx.track.take_shape_commands(epoch, &style, state) {
        ctx.track.offer_shape_command_bounds(bounds);
        commands
    } else {
        ctx.track
            .build_shape_commands(epoch, style, state, primitive, ctx.time_ms, shape)?
    };

    attach_shape_shadow(&mut commands, ctx, state);
    Ok(Some(commands))
}

fn attach_shape_shadow(
    commands: &mut Vec<RenderCommand>,
    ctx: &EvaluateCtx,
    state: &VectorShapeState,
) {
    use crate::timeline::ActorField;
    use crate::timeline::property_engine::PropertyValue;
    let shadow_val = ctx.read_property_value(ActorField::Tagged("shadow"), ctx.time_ms);
    if let Some(PropertyValue::Vec4(params)) = shadow_val {
        if params[2] > 0.0 || params[3] != 0.0 || params[0] != 0.0 || params[1] != 0.0 {
            let color =
                match ctx.read_property_value(ActorField::Tagged("shadow_color"), ctx.time_ms) {
                    Some(PropertyValue::Vec4(c)) | Some(PropertyValue::Color(c)) => c,
                    _ => [0.0, 0.0, 0.0, 0.25],
                };
            let (size, corner_radius) = match state {
                VectorShapeState::Rect(r) => (r.size, r.corner_radius),
                _ => ([50.0, 50.0], 0.0),
            };
            commands.insert(
                0,
                RenderCommand::Shadow {
                    size,
                    corner_radius,
                    params,
                    color,
                },
            );
        }
    }
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

    let style = ctx.style.expect("dash styling requires the style tier");
    let pattern = style.dash_pattern.get(ctx.time_ms, Vec::new());
    if pattern.is_empty() {
        return;
    }
    let offset = style.dash_offset.get(ctx.time_ms, 0.0);
    eprintln!(
        "DASH time={} offset={} keys={:?}",
        ctx.time_ms,
        offset,
        style
            .dash_offset
            .as_ref()
            .map(|t| t.keyframes.keys().copied().collect::<Vec<_>>())
    );
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
fn shape_has_gradient(style: &StyleTracks) -> bool {
    style.fill_gradient.is_some() || style.stroke_gradient.is_some()
}

/// Stamp the sampled `fill_gradient:` / `stroke_gradient:` paints onto every
/// path in `commands`, applying the shared `gradient_extend:` /
/// `gradient_space:` settings.
fn stamp_shape_gradient(commands: &mut [RenderCommand], ctx: &EvaluateCtx) {
    use crate::renderer::types::{GradientExtend, GradientSpace, GradientSpec};
    use crate::timeline::TrackAccessor;

    let style = ctx.style.expect("gradient styling requires the style tier");
    let fill = style.fill_gradient.get(ctx.time_ms, GradientSpec::default());
    let stroke = style.stroke_gradient.get(ctx.time_ms, GradientSpec::default());
    if fill.stops.is_empty() && stroke.stops.is_empty() {
        return;
    }

    let extend = match style.gradient_extend.get(ctx.time_ms, "pad".to_string()).as_str() {
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
    let space = match style.gradient_space.get(ctx.time_ms, "oklab".to_string()).as_str() {
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

/// Apply `draw-in`'s stroke trim to built shape commands.
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
