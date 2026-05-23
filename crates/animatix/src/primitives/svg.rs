//! SVG media primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::{AnimationTrack, Environment, SceneDimensions, Value};
use crate::timeline::property_lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::svg::parse_svg;

/// The `Svg` primitive.
pub struct SvgPrimitive;

/// Singleton instance of [`SvgPrimitive`].
pub const SVG: SvgPrimitive = SvgPrimitive;

impl Primitive for SvgPrimitive {
    fn type_name(&self) -> &'static str { "Svg" }
    fn display_name(&self) -> &'static str { "SVG" }
    fn category(&self) -> ActorCategory { ActorCategory::Media }
    fn icon_id(&self) -> &'static str { crate::icon_glyphs::VECTOR_THREE }
    fn is_advanced(&self) -> bool { true }
    fn kind_id(&self) -> ActorKindId { ActorKindId::Svg }

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
        _ctx: &mut AssignmentCtx,
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

        match std::fs::read_to_string(&target_url) {
            Ok(svg_content) => match parse_svg(&svg_content) {
                Ok(parsed_paths) => {
                    track.svg_paths = parsed_paths;
                }
                Err(error) => {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::MediaLoadFailure,
                            DiagnosticPhase::Build,
                            format!("Failed to parse SVG file '{target_url}': {error}"),
                        )
                        .with_subject(subject)
                        .with_path(&target_url),
                    );
                }
            },
            Err(error) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::MediaLoadFailure,
                        DiagnosticPhase::Build,
                        format!("Failed to read SVG file '{target_url}': {error}"),
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