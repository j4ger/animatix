//! Legend primitive - auto-generates legend from scene content

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{
    ActorCategory, ActorKindId, BuildCtx, EvaluateCtx, Primitive, RenderCommand, TextCompileCtx,
};
use crate::renderer::error::RenderError;
use crate::timeline::{AnimationTrack, SceneDimensions};

/// The `Legend` primitive.
pub struct LegendPrimitive;

/// Singleton instance of [`LegendPrimitive`].
pub const LEGEND: LegendPrimitive = LegendPrimitive;

impl Primitive for LegendPrimitive {
    fn type_name(&self) -> &'static str {
        "Legend"
    }

    fn display_name(&self) -> &'static str {
        "Legend"
    }

    fn category(&self) -> ActorCategory {
        ActorCategory::Annotation
    }

    fn icon_id(&self) -> &'static str {
        crate::icon_glyphs::CHART_LINE_UP
    }

    fn kind_id(&self) -> ActorKindId {
        ActorKindId::Legend
    }

    fn is_shape(&self) -> bool {
        false
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        // Ensure track exists and set kind
        let track = ctx
            .timeline
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| AnimationTrack::new(label.to_string()));
        track.kind = ActorKindId::Legend;

        if track.first_seen_ms == u64::MAX {
            track.first_seen_ms = ctx.time_ms as u64;
        }

        // Entries are populated by the post-build scene scan, so a rebuild
        // starts from an empty slate instead of retaining stale entries.
        track.legend.entries.clear();

        Ok(())
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        _text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        let entries = &ctx.track.legend.entries;
        if entries.is_empty() {
            return Ok(None);
        }

        use kurbo::{Rect, Shape};
        use vello::peniko::Color;

        let mut commands = Vec::new();
        let swatch_size = 16.0f64;
        let spacing = 8.0f64;
        let label_offset = swatch_size + spacing;
        let line_height = swatch_size + spacing;
        let mut y_offset = 0.0f64;

        for (_label, color_rgba) in entries {
            // Draw color swatch (rectangle)
            let swatch_rect = Rect::new(0.0, y_offset, swatch_size, y_offset + swatch_size);
            let swatch_path = swatch_rect.to_path(0.1);

            let fill_color = Color::from_rgba8(
                (color_rgba[0] * 255.0) as u8,
                (color_rgba[1] * 255.0) as u8,
                (color_rgba[2] * 255.0) as u8,
                (color_rgba[3] * 255.0) as u8,
            );

            let swatch_vello = crate::timeline::VelloPath {
                path: swatch_path,
                fill: Some(fill_color),
                stroke: None,
                line_cap: 0,
                line_join: 0,
            };

            commands.push(RenderCommand::Paths {
                paths: vec![swatch_vello],
            });

            // Draw label text as filled paths positioned to the right of the swatch.
            // For now, render a simple filled rectangle placeholder for each label.
            // Full text rendering would use TextCompileCtx and evaluate_text_paths.
            let text_width = 80.0f64;
            let text_rect = Rect::new(
                label_offset,
                y_offset,
                label_offset + text_width,
                y_offset + swatch_size,
            );
            let text_bg_path = text_rect.to_path(0.1);

            let text_bg_vello = crate::timeline::VelloPath {
                path: text_bg_path,
                fill: Some(Color::from_rgba8(
                    (color_rgba[0] * 255.0) as u8,
                    (color_rgba[1] * 255.0) as u8,
                    (color_rgba[2] * 255.0) as u8,
                    (color_rgba[3] * 64.0) as u8, // low opacity text background
                )),
                stroke: None,
                line_cap: 0,
                line_join: 0,
            };

            commands.push(RenderCommand::Paths {
                paths: vec![text_bg_vello],
            });

            y_offset += line_height;
        }

        Ok(Some(commands))
    }

    fn default_props(&self, _scene: &SceneDimensions) -> Vec<Property> {
        vec![Property::new(
            "at",
            Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(100.0)]),
        )]
    }
}
