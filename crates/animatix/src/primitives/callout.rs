use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::arrow::build_arrow_path;
use crate::primitives::{ActorCategory, ActorKindId, BuildCtx, EvaluateCtx, Primitive, RenderCommand, TextCompileCtx, evaluate_text_paths, sample_shape_style};
use crate::renderer::error::RenderError;
use crate::renderer::text::TextKind;
use crate::timeline::{AnimationTrack, Environment, SceneDimensions, TrackAccessor, Value, VectorShapeState, VelloPath};

/// Parse a numeric value from an AST expression.
fn parse_f32(expr: &Expr) -> Option<f32> {
    match expr {
        Expr::Num(n) => Some(*n as f32),
        _ => None,
    }
}

/// Parse a Vec2 from an AST expression (tuple of two numbers).
fn parse_vec2(expr: &Expr) -> Option<[f32; 2]> {
    match expr {
        Expr::Tuple(items) if items.len() == 2 => {
            let x = parse_f32(&items[0])?;
            let y = parse_f32(&items[1])?;
            Some([x, y])
        }
        _ => None,
    }
}

/// Parse a string from an AST expression.
/// Also accepts bare identifiers (`Ident`) and dot-path expressions (`Path`)
/// for constructs like `target: box` or `target: group.box`.
fn parse_string(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Str(s) => Some(s.clone()),
        Expr::Ident(s) => Some(s.clone()),
        Expr::Path(parts) => Some(parts.join(".")),
        _ => None,
    }
}

/// The singleton primitive descriptor for `Callout` actors.
pub struct CalloutPrimitive;
/// Singleton instance of the callout primitive descriptor.
pub const CALLOUT: CalloutPrimitive = CalloutPrimitive;

impl Primitive for CalloutPrimitive {
    fn type_name(&self) -> &'static str {
        "Callout"
    }
    fn display_name(&self) -> &'static str {
        "Callout"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Annotation
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::TEXT_T
    }
    fn is_advanced(&self) -> bool {
        false
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Callout
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Ensure track exists and set kind
        let track = ctx
            .timeline
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| AnimationTrack::new(label.to_string()));
        track.kind = ActorKindId::Callout;

        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = ctx.time_ms as u64;
        }

        // Default values
        let mut from = [-100.0, 0.0];
        let mut to = [100.0, 0.0];
        let mut head_size = 10.0f32;
        let mut label_text = String::new();
        let mut label_at = [0.0, 50.0];
        let mut callout_target = String::new();
        let mut callout_place = String::new();
        let mut callout_standoff = 40.0f32;
        let mut callout_to_offset = [0.0f32, 0.0f32];

        // Parse properties (simple numeric/string parsing without env)
        for prop in props {
            match prop.name.as_str() {
                "from" => {
                    if let Some(parsed) = parse_vec2(&prop.value) {
                        from = parsed;
                    }
                }
                "to" => {
                    if let Some(parsed) = parse_vec2(&prop.value) {
                        to = parsed;
                    }
                }
                "head_size" => {
                    if let Some(val) = parse_f32(&prop.value) {
                        head_size = val.max(1.0);
                    }
                }
                "label" => {
                    if let Some(s) = parse_string(&prop.value) {
                        label_text = s;
                    }
                }
                "label_at" => {
                    if let Some(parsed) = parse_vec2(&prop.value) {
                        label_at = parsed;
                    }
                }
                "target" => {
                    if let Some(s) = parse_string(&prop.value) {
                        callout_target = s;
                    }
                }
                "place" => {
                    if let Some(s) = parse_string(&prop.value) {
                        callout_place = s;
                    }
                }
                "standoff" => {
                    if let Some(val) = parse_f32(&prop.value) {
                        callout_standoff = val;
                    }
                }
                "to_offset" => {
                    if let Some(parsed) = parse_vec2(&prop.value) {
                        callout_to_offset = parsed;
                    }
                }
                _ => {}
            }
        }

        // Write initial keyframes to tracks so that the property engine
        // can sample them at frame time.
        track.shape.line_from.ensure([-100.0, 0.0]).add_keyframe(0, from, crate::easing::Easing::Linear);
        track.shape.line_to.ensure([100.0, 0.0]).add_keyframe(0, to, crate::easing::Easing::Linear);
        track.shape.head_size.ensure(10.0).add_keyframe(0, head_size, crate::easing::Easing::Linear);
        track.text.text_content.ensure(String::new()).add_keyframe(0, label_text, crate::easing::Easing::Linear);
        track.geometry.label_at.ensure([0.0, 0.0]).add_keyframe(0, label_at, crate::easing::Easing::Linear);
        track.geometry.callout_target.ensure(String::new()).add_keyframe(0, callout_target, crate::easing::Easing::Linear);
        track.geometry.callout_place.ensure(String::new()).add_keyframe(0, callout_place, crate::easing::Easing::Linear);
        track.geometry.callout_standoff.ensure(40.0).add_keyframe(0, callout_standoff, crate::easing::Easing::Linear);
        track.geometry.callout_to_offset.ensure([0.0, 0.0]).add_keyframe(0, callout_to_offset, crate::easing::Easing::Linear);

        Ok(())
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        use crate::timeline::Value;

        // Sample arrow properties from tracks
        let mut from = ctx.track.shape.line_from.get(ctx.time_ms, [-100.0, 0.0]);
        let mut to = ctx.track.shape.line_to.get(ctx.time_ms, [100.0, 0.0]);
        let mut head_size = ctx.track.shape.head_size.get(ctx.time_ms, 10.0);

        // ── Targeted callout mode ──
        // When a non-empty `target` is set, derive `to` and `from` from the
        // target actor's scene-space AABB, using `place` and `standoff`.
        //
        // Formula:
        //   to   = attach_point(place, target_aabb) + to_offset
        //   from = to + direction(place) * standoff
        //
        // The label still renders at `to + label_at` (manual mode unchanged).
        let target_name = ctx.track.geometry.callout_target.get(ctx.time_ms, String::new());
        if !target_name.is_empty() {
            if let Some(timeline) = ctx.timeline {
                let place = ctx.track.geometry.callout_place.get(ctx.time_ms, "right".to_string());
                let standoff = ctx.track.geometry.callout_standoff.get(ctx.time_ms, 40.0);
                let to_offset = ctx.track.geometry.callout_to_offset.get(ctx.time_ms, [0.0, 0.0]);

                if let Some(target_track) = timeline.get_track(&target_name) {
                    // Unrotated scene-space AABB: centre ± half_size
                    let centre = target_track.geometry.position.get(ctx.time_ms, [0.0, 0.0]);
                    let half = target_track.geometry.size.get(ctx.time_ms, [50.0, 50.0]);

                    let (attach, dir): ([f32; 2], [f32; 2]) = match place.as_str() {
                        "above" | "top" => (
                            [centre[0], centre[1] - half[1]],
                            [0.0, -1.0],
                        ),
                        "below" | "bottom" => (
                            [centre[0], centre[1] + half[1]],
                            [0.0, 1.0],
                        ),
                        "left" => (
                            [centre[0] - half[0], centre[1]],
                            [-1.0, 0.0],
                        ),
                        // "right" and fallback
                        _ => (
                            [centre[0] + half[0], centre[1]],
                            [1.0, 0.0],
                        ),
                    };

                    to = [attach[0] + to_offset[0], attach[1] + to_offset[1]];
                    from = [to[0] + dir[0] * standoff, to[1] + dir[1] * standoff];
                } else {
                    tracing::warn!(
                        "callout '{}': target actor '{}' not found in timeline",
                        ctx.track.label,
                        target_name
                    );
                }
            }
        }

        if let Some(overrides) = ctx.overrides {
            if let Some(Value::Vec2(f)) = overrides.get("from") {
                from = [f[0] as f32, f[1] as f32];
            }
            if let Some(Value::Vec2(t)) = overrides.get("to") {
                to = [t[0] as f32, t[1] as f32];
            }
            if let Some(Value::Num(n)) = overrides.get("head_size") {
                head_size = *n as f32;
            }
        }
        let path = build_arrow_path(from, to, head_size);

        // Sample style
        let style = sample_shape_style(ctx.track, ctx.time_ms, ctx.overrides);

        let arrow_vello = VelloPath {
            path,
            fill: Some(vello::peniko::Color::from_rgba8(
                (style.stroke_color[0] * 255.0) as u8,
                (style.stroke_color[1] * 255.0) as u8,
                (style.stroke_color[2] * 255.0) as u8,
                (style.stroke_color[3] * 255.0) as u8,
            )),
            stroke: crate::timeline::shapes::shape_stroke(
                style.stroke_color,
                style.stroke_width,
            )
            .or_else(|| {
                Some((
                    vello::peniko::Color::from_rgba8(
                        (style.stroke_color[0] * 255.0) as u8,
                        (style.stroke_color[1] * 255.0) as u8,
                        (style.stroke_color[2] * 255.0) as u8,
                        (style.stroke_color[3] * 255.0) as u8,
                    ),
                    1.0,
                ))
            }),
            line_cap: 0,
            line_join: 0,
        };

        let mut commands = vec![RenderCommand::Paths { paths: vec![arrow_vello] }];

        // Sample label text and position
        let mut label_text = ctx.track.text.text_content.get(ctx.time_ms, String::new());
        let mut label_at = ctx.track.geometry.label_at.get(ctx.time_ms, [0.0, 50.0]);

        if let Some(overrides) = ctx.overrides {
            if let Some(Value::Str(s)) = overrides.get("label") {
                label_text = s.clone();
            }
            if let Some(Value::Vec2(la)) = overrides.get("label_at") {
                label_at = [la[0] as f32, la[1] as f32];
            }
        }

        // Render label text if non-empty
        if !label_text.is_empty() {
            let offset_x = (to[0] + label_at[0]) as f64;
            let offset_y = (to[1] + label_at[1]) as f64;

            if let Some(text_ctx) = text_ctx {
                let paths = evaluate_text_paths(ctx, text_ctx, TextKind::Text, 24.0)?;

                // Translate text paths to the label position using kurbo Affine
                let translate = kurbo::Affine::translate((offset_x, offset_y));
                let translated_paths: Vec<crate::renderer::types::TextPath> = paths
                    .iter()
                    .map(|tp| {
                        let mut p = tp.clone();
                        p.path.apply_affine(translate);
                        p
                    })
                    .collect();

                if !translated_paths.is_empty() {
                    commands.push(RenderCommand::Text {
                        paths: std::sync::Arc::from(translated_paths.into_boxed_slice()),
                    });
                }
            }
        }

        Ok(Some(commands))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("from", Expr::Tuple(vec![Expr::Num(-100.0), Expr::Num(0.0)])),
            Property::new("to", Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(0.0)])),
            Property::new("head_size", Expr::Num(10.0)),
            Property::new("label", Expr::Str("Callout".to_string())),
            Property::new("label_at", Expr::Tuple(vec![Expr::Num(0.0), Expr::Num(50.0)])),
            Property::new("color", Expr::Ident("accent.primary".into())),
        ]
    }

    fn apply_defaults(&self, state: &mut VectorShapeState) {
        let VectorShapeState::Callout(callout) = state else {
            return;
        };
        if callout.head_size <= 0.0 {
            callout.head_size = 10.0;
        }
    }

    fn finalize_state(&self, _state: &mut VectorShapeState) {}

    fn uses_custom_path(&self) -> bool {
        false
    }

    fn exposes_tip_size(&self) -> bool {
        false
    }

    fn supports_fill(&self) -> bool {
        false
    }

    fn default_color_key(&self, property: &str) -> Option<&'static str> {
        match property {
            "stroke" | "stroke_color" => Some("stroke.default"),
            "color" => None,
            _ => None,
        }
    }

    fn apply_property(
        &self,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        let VectorShapeState::Callout(callout) = state else {
            return false;
        };
        match name {
            "from" => {
                if let Some(parsed) =
                    crate::timeline::lookup_parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    callout.from = parsed;
                }
                true
            }
            "to" => {
                if let Some(parsed) =
                    crate::timeline::lookup_parse_numeric_vec2_with_lookup_diagnostic(value, env, diagnostics, subject)
                {
                    callout.to = parsed;
                }
                true
            }
            "head_size" => {
                if let Value::Num(val) =
                    crate::timeline::evaluate_expr(value, env).unwrap_or(Value::Num(10.0))
                {
                    callout.head_size = val.max(1.0) as f32;
                }
                true
            }
            _ => false,
        }
    }
}
