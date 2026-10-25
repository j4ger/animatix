//! Image media primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::primitives::{AssignmentCtx, BuildCtx, Primitive};
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
    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        ctx.timeline.process_media_actor_decl(
            self.type_name(),
            label,
            props,
            modifiers,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        );
        Ok(())
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
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::RenderCommand;
        use crate::timeline::DEFAULT_LAYOUT_HALF_SIZE;

        if let Some(image) = ctx.image_track().and_then(|t| t.evaluate(ctx.time_ms)) {
            let half_size = crate::primitives::resolve_half_size(
                ctx.geometry
                    .expect("Image evaluation requires the geometry tier")
                    .size
                    .get(ctx.time_ms, DEFAULT_LAYOUT_HALF_SIZE),
                ctx.overrides,
            );
            let natural_size = [half_size[0] * 2.0, half_size[1] * 2.0];
            // Actor local space is centered on the position (every geometry
            // track stores half-sizes), so shift the box back by half its
            // display size to keep `at`/`anchor` meaning "the image is here",
            // not "the image's top-left is here".
            Ok(Some(vec![RenderCommand::Image {
                image,
                natural_size,
                offset: [(-half_size[0]) as f64, (-half_size[1]) as f64],
            }]))
        } else {
            Ok(None)
        }
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
