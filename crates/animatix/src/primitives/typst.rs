//! Typst document primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::{AnimationTrack, Environment, SceneDimensions, Value};

/// The `Typst` primitive.
pub struct TypstPrimitive;

/// Singleton instance of `TypstPrimitive`.
pub const TYPST: TypstPrimitive = TypstPrimitive;

impl Primitive for TypstPrimitive {
    fn type_name(&self) -> &str {
        "Typst"
    }
    fn display_name(&self) -> &str {
        "Typst"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Text
    }
    fn icon_id(&self) -> &str {
        crate::icon_glyphs::ARTICLE
    }
    fn is_advanced(&self) -> bool {
        true
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Typst
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        if let Err(e) = ctx.timeline.process_text_actor_decl(
            self.type_name(),
            label,
            props,
            modifiers,
            ctx.time_ms,
            ctx.parent_label,
            ctx.diagnostics,
        ) {
            ctx.diagnostics.push(
                Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                    crate::diagnostics::DiagnosticPhase::Build,
                    format!("{}", e),
                )
                .with_subject(label),
            );
        }
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
        if !matches!(property, "text" | "latex" | "math" | "code" | "content") {
            return false;
        }
        let target_text = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
            .unwrap_or(Value::Str(String::new()))
            .as_str()
            .to_string();
        if let Err(e) = crate::timeline::recompile_text_at_assignment(
            track,
            target_text,
            ctx.t_start_ms,
            ctx.t_end_ms,
            ctx.easing,
            ctx.instant_delayed,
            ctx.duration_ms,
            ctx.font_context,
            ctx.text_compiler,
        ) {
            diagnostics.push(
                Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!("{e}"),
                )
                .with_subject(subject),
            );
        }
        true
    }

    fn evaluate(
        &self,
        ctx: &crate::primitives::EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        use crate::primitives::{RenderCommand, evaluate_text_paths};
        use crate::renderer::text::TextKind;
        use crate::timeline::TrackAccessor;

        let paths = if let Some(text_ctx) = text_ctx {
            evaluate_text_paths(
                ctx,
                text_ctx,
                TextKind::Typst,
                crate::renderer::text::default_font_size(TextKind::Typst),
            )
        } else {
            Ok(std::sync::Arc::from(ctx.track.evaluate_text_paths(ctx.time_ms)))
        }?;
        if paths.is_empty() {
            Ok(None)
        } else {
            // Check highlight properties for optional highlight overlay.
            let hl_opacity = ctx.track.highlight.highlight_opacity.get(ctx.time_ms, 0.0);
            if hl_opacity > 0.001 {
                // Compute bounding box of all glyph paths.
                use kurbo::Shape;
                let mut min_x = f64::INFINITY;
                let mut max_x = f64::NEG_INFINITY;
                let mut min_y = f64::INFINITY;
                let mut max_y = f64::NEG_INFINITY;
                for tp in paths.iter() {
                    let b = tp.path.bounding_box();
                    min_x = min_x.min(b.x0);
                    max_x = max_x.max(b.x1);
                    min_y = min_y.min(b.y0);
                    max_y = max_y.max(b.y1);
                }
                if min_x.is_finite() && max_x.is_finite() {
                    let hl_color_arr =
                        ctx.track.highlight.highlight_color.get(ctx.time_ms, [0.3, 0.5, 1.0, 1.0]);
                    let hl_padding = ctx.track.highlight.highlight_padding.get(ctx.time_ms, 4.0);
                    let hl_radius = ctx.track.highlight.highlight_radius.get(ctx.time_ms, 3.0);
                    let hl_blend = ctx.track.highlight.highlight_blend;

                    let pad = hl_padding as f64;
                    let hl_rect =
                        kurbo::Rect::new(min_x - pad, min_y - pad, max_x + pad, max_y + pad);
                    let hl_color = vello::peniko::Color::from_rgba8(
                        (hl_color_arr[0] * 255.0) as u8,
                        (hl_color_arr[1] * 255.0) as u8,
                        (hl_color_arr[2] * 255.0) as u8,
                        255,
                    );

                    return Ok(Some(vec![
                        RenderCommand::HighlightLayer {
                            rect: hl_rect,
                            color: hl_color,
                            blend: hl_blend,
                            alpha: hl_opacity,
                            corner_radius: hl_radius as f64,
                        },
                        RenderCommand::Text { paths },
                    ]));
                }
            }
            Ok(Some(vec![RenderCommand::Text { paths }]))
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
            Property::new("content", Expr::Str("*bold* and _italic_".into())),
            Property::new("font_size", Expr::Num(48.0)),
        ]
    }
}
