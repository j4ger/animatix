use crate::ast::{Expr, InlineItem, Modifier, Property, array_actor_label};
use crate::diagnostics::Diagnostic;
use crate::primitives::arrow::build_arrow_path;
use crate::primitives::{
    ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, EvaluateCtx, Primitive, RenderCommand,
    TextCompileCtx, evaluate_text_paths, sample_shape_style,
};
use crate::renderer::error::RenderError;
use crate::renderer::text::TextKind;
use crate::timeline::callout_geometry::derive_callout_geometry;
use crate::timeline::property_engine::{parse_property_value, write_property_field};
use crate::timeline::property_registry::{ActorField, ValueType};
use crate::timeline::{
    AnimationTrack, Environment, SceneDimensions, TrackAccessor, Value, VectorShapeState, VelloPath,
};

/// Resolve an actor reference at assignment time, including runtime indices
/// such as `bar[i]` where `i` is bound in the evaluation environment.
fn parse_actor_ref_with_env(expr: &Expr, env: &Environment) -> Option<String> {
    match expr {
        Expr::Str(s) => Some(s.clone()),
        Expr::Ident(name) => match env.get(name) {
            Some(Value::Str(s)) => Some(s.clone()),
            _ => Some(name.clone()),
        },
        Expr::Path(parts) => match crate::timeline::evaluate_expr(expr, env) {
            Ok(Value::Str(s)) => Some(s),
            _ => Some(parts.join(".")),
        },
        Expr::Index(base, index) => {
            let base_name = match base.as_ref() {
                Expr::Ident(name) => name.clone(),
                Expr::Path(parts) if !parts.is_empty() => parts.join("."),
                _ => return None,
            };
            let n = match crate::timeline::evaluate_expr(index, env).ok()? {
                Value::Num(n) if n >= 0.0 && n.floor() == n => n as usize,
                _ => return None,
            };
            Some(array_actor_label(&base_name, n))
        },
        _ => None,
    }
}

/// The singleton primitive descriptor for `Callout` actors.
pub struct CalloutPrimitive;
/// Singleton instance of the callout primitive descriptor.
pub const CALLOUT: CalloutPrimitive = CalloutPrimitive;

impl Primitive for CalloutPrimitive {
    fn type_name(&self) -> &str {
        "Callout"
    }
    fn display_name(&self) -> &str {
        "Callout"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Annotation
    }
    fn icon_id(&self) -> &str {
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
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Callout now uses the generic actor build path in timeline/build/actor.rs.
        Ok(())
    }

    fn handle_assignment(
        &self,
        track: &mut AnimationTrack,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> bool {
        if property != "target" {
            return false;
        }

        // Actor references are idiomatic in declarations (`target: box`).
        // Accept the same form on assignments instead of requiring `"box"`,
        // and keep source-level array syntax (`bar[2]`) separate from the
        // internal `bar__2` track key.
        let target_expr = match parse_actor_ref_with_env(value, env) {
            Some(target) => Expr::Str(target),
            None => value.clone(),
        };

        if let Some(target) =
            parse_property_value(ValueType::String, &target_expr, env, diagnostics, subject)
        {
            write_property_field(
                track,
                ActorField::CalloutTarget,
                target,
                ctx.t_start_ms,
                ctx.t_end_ms,
                ctx.easing,
                diagnostics,
            );
        }
        true
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        use crate::timeline::Value;

        // Derive geometry using the shared helper (handles both manual and targeted mode).
        let geom = derive_callout_geometry(
            ctx.track,
            ctx.time_ms,
            ctx.target_resolver,
            ctx.scene_dimensions,
        );
        let mut from = geom.from;
        let mut to = geom.to;
        let mut head_size = ctx.track.shape.head_size.get(ctx.time_ms, 10.0);

        // Warn when targeted but resolver didn't find the target.
        let target_name = ctx.track.geometry.callout_target.get(ctx.time_ms, String::new());
        if !target_name.is_empty() && ctx.target_resolver.is_none() {
            // Build-time diagnostic (CalloutTargetNotFound) already covers this; debug only to
            // avoid per-frame spam.
            tracing::debug!(
                "callout '{}': target actor '{}' not found in timeline",
                ctx.track.label,
                target_name
            );
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
            stroke: crate::timeline::shapes::shape_stroke(style.stroke_color, style.stroke_width)
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

        let mut commands = vec![RenderCommand::Paths {
            paths: vec![arrow_vello],
        }];

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
                // Callout labels intentionally default smaller (24pt) than the
                // shared prose default (`renderer::text::default_font_size`),
                // so keep the explicit size here.
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
                    crate::timeline::lookup_parse_numeric_vec2_with_lookup_diagnostic(
                        value,
                        env,
                        diagnostics,
                        subject,
                    )
                {
                    callout.from = parsed;
                }
                true
            },
            "to" => {
                if let Some(parsed) =
                    crate::timeline::lookup_parse_numeric_vec2_with_lookup_diagnostic(
                        value,
                        env,
                        diagnostics,
                        subject,
                    )
                {
                    callout.to = parsed;
                }
                true
            },
            "head_size" => {
                if let Value::Num(val) =
                    crate::timeline::evaluate_expr(value, env).unwrap_or(Value::Num(10.0))
                {
                    callout.head_size = val.max(1.0) as f32;
                }
                true
            },
            _ => false,
        }
    }
}
