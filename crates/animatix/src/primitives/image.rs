//! Image media primitive.

use crate::ast::{Expr, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::primitives::{AssignmentCtx, Primitive};
use crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{Environment, SceneDimensions, Value, preserve_instant_delayed_value};

/// The `Image` primitive.
pub struct ImagePrimitive;

/// Singleton instance of `ImagePrimitive`.
pub const IMAGE: ImagePrimitive = ImagePrimitive;

impl Primitive for ImagePrimitive {
    fn type_name(&self) -> &str {
        "Image"
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
        if property != "url" {
            return false;
        }
        let target_url = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
            .unwrap_or(Value::Str(String::new()))
            .as_str()
            .to_string();
        if target_url.is_empty() {
            return true;
        }

        match ctx.asset_cache.load_image_for(&target_url, ctx.label) {
            Ok(target_image) => {
                let t_start_ms = ctx.t_start_ms;
                let t_end_ms = ctx.t_end_ms;
                let duration_ms = ctx.duration_ms;
                let instant_delayed = ctx.instant_delayed;
                let easing = ctx.easing;
                let payload = ctx.ensure_image_payload();
                if duration_ms > 0.0 {
                    let start_val = payload.image.get(t_start_ms, None);
                    payload.image.ensure(None).add_keyframe(t_start_ms, start_val, Easing::Linear);
                } else if instant_delayed {
                    preserve_instant_delayed_value(&mut payload.image, t_start_ms);
                }
                payload.image.ensure(None).add_keyframe(t_end_ms, Some(target_image), easing);
            },
            Err(error) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::MediaLoadFailure,
                        DiagnosticPhase::Build,
                        format!("Failed to load image file '{target_url}': {error}"),
                    )
                    .with_subject(subject)
                    .with_path(&target_url),
                );
            },
        }
        true
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        crate::timeline::behavior::builtin::media::evaluate_image(ctx, text_ctx)
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new(
                "at",
                Expr::Tuple(vec![
                    Expr::Num(scene.width as f64 / 2.0),
                    Expr::Num(scene.height as f64 / 2.0),
                ]),
            ),
            Property::new("url", Expr::Str(String::new())),
            Property::new("size", Expr::Tuple(vec![Expr::Num(240.0), Expr::Num(160.0)])),
        ]
    }
}
