use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::primitives::{ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::{AnimationTrack, Environment, SceneDimensions, Value};
use crate::timeline::image::load_image;
use crate::timeline::preserve_instant_delayed_value;
use crate::timeline::property_lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::track::TrackAccessor;

pub struct ImagePrimitive;
pub const IMAGE: ImagePrimitive = ImagePrimitive;

impl Primitive for ImagePrimitive {
    fn type_name(&self) -> &'static str { "Image" }
    fn display_name(&self) -> &'static str { "Image" }
    fn category(&self) -> ActorCategory { ActorCategory::Media }
    fn icon_id(&self) -> &'static str { "image" }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Image }

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
        track: &mut AnimationTrack,
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
        let target_url = evaluate_expr_with_lookup_diagnostic(
            value, env, diagnostics, subject,
        )
        .unwrap_or(Value::Str(String::new()))
        .as_str()
        .to_string();
        if target_url.is_empty() {
            return true;
        }

        match load_image(&target_url) {
            Ok(target_image) => {
                if ctx.duration_ms > 0.0 {
                    let start_val = track.image.get(ctx.t_start_ms, None);
                    track.image.ensure(None).add_keyframe(ctx.t_start_ms, start_val, Easing::Linear);
                } else if ctx.instant_delayed {
                    preserve_instant_delayed_value(&mut track.image, ctx.t_start_ms);
                }
                track.image.ensure(None).add_keyframe(ctx.t_end_ms, Some(target_image), ctx.easing);
            }
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
            }
        }
        true
    }

    fn default_props(&self, scene: &SceneDimensions) -> Vec<Property> {
        vec![
            Property::new("at", Expr::Tuple(vec![Expr::Num(scene.width as f64 / 2.0), Expr::Num(scene.height as f64 / 2.0)])),
            Property::new("url", Expr::Str(String::new())),
            Property::new("size", Expr::Tuple(vec![Expr::Num(240.0), Expr::Num(160.0)])),
        ]
    }
}