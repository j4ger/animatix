use crate::ast::{Expr, Property, array_actor_label};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, EvaluateCtx, Primitive, RenderCommand, TextCompileCtx};
use crate::renderer::error::RenderError;
use crate::timeline::property_engine::parse_property_value;
use crate::timeline::property_registry::ValueType;
use crate::timeline::{ActorCaps, Environment, SceneDimensions, Value, VectorShapeState};

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

    fn handle_assignment(
        &self,
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
            let Some(geometry) = ctx.geometry.as_deref_mut() else {
                tracing::debug!(
                    "target write for '{}' dropped: no geometry tier in this assignment context",
                    ctx.label
                );
                return true;
            };
            let has_duration = ctx.t_end_ms > ctx.t_start_ms;
            let has_delay = ctx.t_start_ms > 0 && !has_duration;
            crate::timeline::property_engine::write_string(
                &mut geometry.callout_target,
                target,
                ctx.t_start_ms,
                ctx.t_end_ms,
                ctx.easing,
                String::new(),
                has_duration,
                has_delay,
            );
        }
        true
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        crate::timeline::behavior::builtin::annotation::evaluate_callout(ctx, text_ctx)
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

    fn default_color_key(&self, property: &str, _caps: &ActorCaps) -> Option<&'static str> {
        match property {
            "stroke" => Some("stroke.default"),
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
