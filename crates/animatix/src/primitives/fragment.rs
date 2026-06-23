//! Fragment primitive.
//!
//! `Fragment` is a leaf primitive used inside an `Equation` container.
//! Each Fragment holds a piece of Typst content and highlight properties.
//! It does not render independently — the parent Equation collects all
//! Fragment content, compiles them together, and renders the glyphs with
//! per-fragment highlight overlays.

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::easing::Easing;
use crate::primitives::{
    ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, Primitive, RenderCommand,
};
use crate::timeline::property_lookup::{
    evaluate_expr_with_lookup_diagnostic, parse_color_in_env_with_lookup_diagnostic,
};
use crate::timeline::{AnimationTrack, Environment, TrackAccessor};

/// The `Fragment` primitive.
pub struct FragmentPrimitive;

/// Singleton instance of [`FragmentPrimitive`].
pub const FRAGMENT: FragmentPrimitive = FragmentPrimitive;

impl Primitive for FragmentPrimitive {
    fn type_name(&self) -> &'static str {
        "Fragment"
    }
    fn display_name(&self) -> &'static str {
        "Fragment"
    }
    fn category(&self) -> ActorCategory {
        ActorCategory::Text
    }
    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::HIGHLIGHTER
    }
    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Fragment
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Clone env before mutable borrow of ctx.timeline.tracks
        let env = ctx.timeline.env().clone();

        // Ensure the track exists.
        let track = ctx
            .timeline
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| AnimationTrack::new(label.to_string()));
        track.kind = ActorKindId::Fragment;
        track.first_seen_ms = ctx.time_ms as u64;

        // Extract `content` property and store as text_content.
        for prop in props {
            match prop.name.as_str() {
                "content" => {
                    let content_str = match &prop.value {
                        Expr::Str(s) => s.clone(),
                        other => {
                            // Try evaluating with an empty env as fallback.
                            let env = Environment::new();
                            crate::timeline::evaluate_expr(other, &env)
                                .map(|v| v.as_str().to_string())
                                .unwrap_or_default()
                        },
                    };
                    track.text.text_content.ensure(String::new()).add_keyframe(
                        ctx.time_ms as u64,
                        content_str,
                        Easing::Linear,
                    );
                },
                "highlight_color" => {
                    // Initial highlight color (stored for later use by scene_eval).
                    if let Expr::Ident(name) = &prop.value {
                        if name == "auto" {
                            // Use default highlight color
                            track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0]).add_keyframe(
                                ctx.time_ms as u64,
                                [0.3, 0.5, 1.0, 1.0],
                                Easing::Linear,
                            );
                            continue;
                        }
                    }
                    // Fall through to color parsing for non-auto values
                    if let Some(color) = parse_color_in_env_with_lookup_diagnostic(
                        label,
                        "highlight_color",
                        &prop.value,
                        &env,
                        ctx.diagnostics,
                        label,
                    ) {
                        track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0]).add_keyframe(
                            ctx.time_ms as u64,
                            color,
                            Easing::Linear,
                        );
                    }
                },
                _ => {},
            }
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
        match property {
            "content" => {
                let content_str =
                    evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                        .map(|v| v.as_str().to_string())
                        .unwrap_or_default();
                if ctx.duration_ms > 0.0 {
                    let start_val = track.text.text_content.get(ctx.t_start_ms, String::new());
                    track.text.text_content.ensure(String::new()).add_keyframe(
                        ctx.t_start_ms,
                        start_val,
                        Easing::Linear,
                    );
                }
                track.text.text_content.ensure(String::new()).add_keyframe(
                    ctx.t_end_ms,
                    content_str,
                    ctx.easing,
                );
                true
            },
            "highlight_color" => {
                let color = crate::timeline::resolve_color_in_env(value, env)
                    .ok()
                    .flatten()
                    .unwrap_or([0.3, 0.5, 1.0, 1.0]);
                if ctx.duration_ms > 0.0 {
                    let start_val = track.highlight.highlight_color.get(ctx.t_start_ms, [0.3, 0.5, 1.0, 1.0]);
                    track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0]).add_keyframe(
                        ctx.t_start_ms,
                        start_val,
                        Easing::Linear,
                    );
                }
                track.highlight.highlight_color.ensure([0.3, 0.5, 1.0, 1.0]).add_keyframe(
                    ctx.t_end_ms,
                    color,
                    ctx.easing,
                );
                true
            },
            "highlight_opacity" => {
                let opacity =
                    evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                        .map(|v| v.as_num() as f32)
                        .unwrap_or(0.0);
                if ctx.duration_ms > 0.0 {
                    let start_val = track.highlight.highlight_opacity.get(ctx.t_start_ms, 0.0);
                    track.highlight.highlight_opacity.ensure(0.0).add_keyframe(
                        ctx.t_start_ms,
                        start_val,
                        Easing::Linear,
                    );
                }
                track
                    .highlight
                    .highlight_opacity
                    .ensure(0.0)
                    .add_keyframe(ctx.t_end_ms, opacity, ctx.easing);
                true
            },
            "highlight_padding" => {
                let padding =
                    evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                        .map(|v| v.as_num() as f32)
                        .unwrap_or(4.0);
                if ctx.duration_ms > 0.0 {
                    let start_val = track.highlight.highlight_padding.get(ctx.t_start_ms, 4.0);
                    track.highlight.highlight_padding.ensure(4.0).add_keyframe(
                        ctx.t_start_ms,
                        start_val,
                        Easing::Linear,
                    );
                }
                track
                    .highlight
                    .highlight_padding
                    .ensure(4.0)
                    .add_keyframe(ctx.t_end_ms, padding, ctx.easing);
                true
            },
            "highlight_radius" => {
                let radius = evaluate_expr_with_lookup_diagnostic(value, env, diagnostics, subject)
                    .map(|v| v.as_num() as f32)
                    .unwrap_or(3.0);
                if ctx.duration_ms > 0.0 {
                    let start_val = track.highlight.highlight_radius.get(ctx.t_start_ms, 3.0);
                    track.highlight.highlight_radius.ensure(3.0).add_keyframe(
                        ctx.t_start_ms,
                        start_val,
                        Easing::Linear,
                    );
                }
                track
                    .highlight
                    .highlight_radius
                    .ensure(3.0)
                    .add_keyframe(ctx.t_end_ms, radius, ctx.easing);
                true
            },
            _ => false,
        }
    }

    fn evaluate(
        &self,
        _ctx: &crate::primitives::EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, crate::renderer::error::RenderError> {
        // Fragment does not render independently.
        // The parent Equation handles all rendering.
        Ok(None)
    }
}
