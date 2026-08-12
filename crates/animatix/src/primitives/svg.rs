//! SVG media primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::primitives::{ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::property_lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::svg::measure_svg_paths;
use crate::timeline::{
    AnimationTrack, DEFAULT_LAYOUT_HALF_SIZE, Environment, SceneDimensions, Value,
    preserve_instant_delayed_value,
};

/// The `Svg` primitive.
pub struct SvgPrimitive;

/// Singleton instance of [`SvgPrimitive`].
pub const SVG: SvgPrimitive = SvgPrimitive;

impl Primitive for SvgPrimitive {
    fn type_name(&self) -> &'static str {
        "Svg"
    }
    fn display_name(&self) -> &'static str {
        "SVG"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Media
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::VECTOR_THREE
    }
    fn is_advanced(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Svg
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
        let target_url = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
            .unwrap_or(Value::Str(String::new()))
            .as_str()
            .to_string();
        if target_url.is_empty() {
            return true;
        }

        let parsed_paths = match ctx.asset_cache.load_svg_for(&target_url, &track.label) {
            Ok(parsed_paths) => parsed_paths,
            Err(error) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::MediaLoadFailure,
                        DiagnosticPhase::Build,
                        format!("Failed to load SVG file '{target_url}': {error}"),
                    )
                    .with_subject(subject)
                    .with_path(&target_url),
                );
                return true;
            },
        };

        let current_paths = track
            .svg_paths_track
            .as_ref()
            .and_then(|track| track.evaluate(ctx.t_start_ms))
            .or_else(|| {
                if track.svg_paths.is_empty() {
                    None
                } else {
                    Some(track.svg_paths.clone())
                }
            });

        if ctx.duration_ms > 0.0 {
            track.svg_paths_track.ensure(None).add_keyframe(
                ctx.t_start_ms,
                current_paths,
                Easing::Linear,
            );
        } else if ctx.instant_delayed {
            preserve_instant_delayed_value(&mut track.svg_paths_track, ctx.t_start_ms);
        }
        track.svg_paths_track.ensure(None).add_keyframe(
            ctx.t_end_ms,
            Some(parsed_paths.clone()),
            ctx.easing,
        );

        let measured_half_size = measure_svg_paths(&parsed_paths);
        if ctx.duration_ms > 0.0 {
            let start_size = track.geometry.size.get(ctx.t_start_ms, DEFAULT_LAYOUT_HALF_SIZE);
            track.geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                ctx.t_start_ms,
                start_size,
                Easing::Linear,
            );
            if let Some(layout_start) = track.layout_size_get(ctx.t_start_ms) {
                track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
                    ctx.t_start_ms,
                    layout_start,
                    Easing::Linear,
                );
            }
        } else if ctx.instant_delayed {
            preserve_instant_delayed_value(&mut track.geometry.size, ctx.t_start_ms);
            preserve_instant_delayed_value(&mut track.geometry.layout_size, ctx.t_start_ms);
        }
        track.geometry.size.ensure(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            ctx.t_end_ms,
            measured_half_size,
            ctx.easing,
        );
        track.ensure_layout_size(DEFAULT_LAYOUT_HALF_SIZE).add_keyframe(
            ctx.t_end_ms,
            measured_half_size,
            ctx.easing,
        );

        true
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::RenderCommand;

        let Some(paths) = ctx.track.svg_paths_at(ctx.time_ms) else {
            return Ok(None);
        };
        if paths.is_empty() {
            Ok(None)
        } else {
            Ok(Some(vec![RenderCommand::Paths { paths }]))
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
