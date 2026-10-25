//! Typst document primitive.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{AssignmentCtx, BuildCtx, Primitive};
use crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic;
use crate::timeline::{Environment, SceneDimensions, TrackAccessor, Value};

/// The `Typst` primitive.
pub struct TypstPrimitive;

/// Singleton instance of `TypstPrimitive`.
pub const TYPST: TypstPrimitive = TypstPrimitive;

impl Primitive for TypstPrimitive {
    fn type_name(&self) -> &str {
        "Typst"
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
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> bool {
        if !matches!(property, "text" | "code" | "text_anchor") {
            return false;
        }
        let text = ctx.text.as_deref_mut().expect("assignment text tier");
        let geometry = ctx.geometry.as_mut().expect("Typst assignment requires the geometry tier");
        let style = ctx.style.as_deref_mut().expect("assignment style tier");
        let target_text = if property == "text_anchor" {
            let anchor = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .unwrap_or(Value::Str("center".to_string()))
                .as_str()
                .to_string();
            text.text_anchor.ensure("center".to_string()).add_keyframe(
                ctx.t_end_ms,
                anchor,
                ctx.easing,
            );
            text.text_content.get(ctx.t_end_ms, String::new())
        } else {
            evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                .unwrap_or(Value::Str(String::new()))
                .as_str()
                .to_string()
        };
        if let Err(e) = crate::timeline::recompile_text_at_assignment(
            ctx.caps,
            style,
            text,
            geometry,
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
        let style = ctx.style.expect("text actor evaluated without a style tier");
        use crate::timeline::TrackAccessor;

        let (paths, clip_path) = if let Some(text_ctx) = text_ctx {
            evaluate_text_paths(
                ctx,
                text_ctx,
                TextKind::Typst,
                crate::renderer::text::default_font_size(TextKind::Typst),
            )
        } else {
            let text = ctx.text.expect("text actor evaluated without a text tier");
            let (paths, clip) = text.evaluate_paths_and_clip(style, ctx.time_ms);
            Ok((std::sync::Arc::from(paths), clip))
        }?;
        if paths.is_empty() {
            Ok(None)
        } else {
            // Check highlight properties for optional highlight overlay.
            let hl_opacity = ctx.highlight.highlight_opacity.get(ctx.time_ms, 0.0);
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
                        ctx.highlight.highlight_color.get(ctx.time_ms, [0.3, 0.5, 1.0, 1.0]);
                    let hl_padding = ctx.highlight.highlight_padding.get(ctx.time_ms, 4.0);
                    let hl_radius = ctx.highlight.highlight_radius.get(ctx.time_ms, 3.0);
                    let hl_blend = ctx.highlight.highlight_blend;

                    let pad = hl_padding as f64;
                    let hl_rect =
                        kurbo::Rect::new(min_x - pad, min_y - pad, max_x + pad, max_y + pad);
                    let hl_color = vello::peniko::Color::from_rgba8(
                        (hl_color_arr[0] * 255.0) as u8,
                        (hl_color_arr[1] * 255.0) as u8,
                        (hl_color_arr[2] * 255.0) as u8,
                        255,
                    );

                    let fill_gradient = if style.fill_gradient.is_some() {
                        use crate::timeline::TrackAccessor;
                        Some(Box::new(
                            style
                                .fill_gradient
                                .get(ctx.time_ms, crate::renderer::types::GradientSpec::default()),
                        ))
                    } else {
                        None
                    };
                    return Ok(Some(vec![
                        RenderCommand::HighlightLayer {
                            rect: hl_rect,
                            color: hl_color,
                            blend: hl_blend,
                            alpha: hl_opacity,
                            corner_radius: hl_radius as f64,
                        },
                        RenderCommand::Text {
                            paths,
                            fill_gradient,
                            clip_path,
                            tint_color: None,
                            hardware_layout: None,
                        },
                    ]));
                }
            }
            let fill_gradient = if style.fill_gradient.is_some() {
                use crate::timeline::TrackAccessor;
                Some(Box::new(
                    style
                        .fill_gradient
                        .get(ctx.time_ms, crate::renderer::types::GradientSpec::default()),
                ))
            } else {
                None
            };
            Ok(Some(vec![RenderCommand::Text {
                paths,
                fill_gradient,
                clip_path,
                tint_color: None,
                hardware_layout: None,
            }]))
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
            Property::new("text", Expr::Str("*bold* and _italic_".into())),
            Property::new("font_size", Expr::Num(48.0)),
        ]
    }
}
