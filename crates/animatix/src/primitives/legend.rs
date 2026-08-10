//! Legend primitive - auto-generates legend from scene content

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::timeline::property_track::TrackAccessor;

fn parse_vec2(expr: &Expr) -> Option<[f32; 2]> {
    if let Expr::Tuple(items) = expr
        && items.len() == 2
        && let Expr::Num(x) = items[0]
        && let Expr::Num(y) = items[1]
    {
        return Some([x as f32, y as f32]);
    }
    None
}

fn parse_f32(expr: &Expr) -> Option<f32> {
    if let Expr::Num(value) = expr {
        return Some(*value as f32);
    }
    None
}

fn parse_string(expr: &Expr) -> Option<String> {
    if let Expr::Str(value) = expr {
        return Some(value.clone());
    }
    None
}

fn compile_legend_text(
    text_ctx: &mut TextCompileCtx,
    text: &str,
    font_size: f32,
    max_width: f32,
    color: [f32; 4],
) -> Result<std::sync::Arc<[crate::renderer::types::TextPath]>, RenderError> {
    text_ctx.text_compiler.compile(
        text,
        crate::renderer::text::DEFAULT_FONT_FAMILY,
        font_size,
        400.0,
        "normal",
        1.2,
        0.0,
        0.0,
        color,
        crate::renderer::text::TextKind::Text,
        text_ctx.font_context,
        max_width,
        "left",
        "visible",
    )
}

fn text_bounds(paths: &[crate::renderer::types::TextPath]) -> Option<(f64, f64, f64, f64)> {
    use kurbo::Shape;
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for text_path in paths {
        let bounds = text_path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
        min_y = min_y.min(bounds.y0);
        max_y = max_y.max(bounds.y1);
    }
    if min_x.is_finite() && max_x.is_finite() && min_y.is_finite() && max_y.is_finite() {
        Some((min_x, min_y, max_x, max_y))
    } else {
        None
    }
}

fn label_color_for_background(background: [f32; 4]) -> [f32; 4] {
    let luminance = 0.299 * background[0] + 0.587 * background[1] + 0.114 * background[2];
    if luminance > 0.5 {
        [0.08, 0.1, 0.13, 1.0]
    } else {
        [0.94, 0.95, 1.0, 1.0]
    }
}
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
        props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        let mut title = String::new();
        let mut font_size = 14.0f32;
        let mut swatch_size = 16.0f32;
        let mut gap = 8.0f32;
        let mut text_max_width = 240.0f32;
        let mut label_color = None;
        let mut at = [0.0f32, 0.0f32];
        for prop in props {
            match prop.name.as_str() {
                "title" => {
                    if let Some(parsed) = parse_string(&prop.value) {
                        title = parsed;
                    }
                },
                "font_size" => {
                    if let Some(parsed) = parse_f32(&prop.value) {
                        font_size = parsed.max(1.0);
                    }
                },
                "swatch_size" => {
                    if let Some(parsed) = parse_f32(&prop.value) {
                        swatch_size = parsed.max(1.0);
                    }
                },
                "gap" => {
                    if let Some(parsed) = parse_f32(&prop.value) {
                        gap = parsed.max(0.0);
                    }
                },
                "text_max_width" => {
                    if let Some(parsed) = parse_f32(&prop.value) {
                        text_max_width = parsed.max(0.0);
                    }
                },
                "label_color" => {
                    if !matches!(&prop.value, Expr::Ident(name) if name == "auto") {
                        label_color = Some(crate::timeline::parse_color_in_env(
                            &prop.value,
                            &ctx.timeline.env,
                        ));
                    }
                },
                "at" => {
                    if let Some(parsed) = parse_vec2(&prop.value) {
                        at = parsed;
                    }
                },
                _ => {},
            }
        }

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

        track.legend.title = title;
        track.legend.font_size = font_size;
        track.legend.label_color = label_color;
        track.legend.swatch_size = swatch_size;
        track.legend.gap = gap;
        track.legend.text_max_width = text_max_width;

        // Legend is an annotation primitive, so it bypasses the generic actor
        // build path and must resolve its own `at` position.
        track.geometry.position.ensure([0.0, 0.0]).add_keyframe(
            ctx.time_ms as u64,
            at,
            crate::easing::Easing::Linear,
        );

        Ok(())
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        mut text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        let entries = &ctx.track.legend.entries;
        if entries.is_empty() {
            return Ok(None);
        }

        use kurbo::{Affine, Rect, Shape};
        use vello::peniko::Color;

        let mut commands = Vec::new();
        let swatch_size = ctx.track.legend.swatch_size as f64;
        let gap = ctx.track.legend.gap as f64;
        let label_offset = swatch_size + gap;
        let font_size = ctx.track.legend.font_size.max(1.0);
        let max_width = ctx.track.legend.text_max_width;
        let label_color = ctx
            .track
            .legend
            .label_color
            .unwrap_or_else(|| label_color_for_background(ctx.background_color));
        let mut y_offset = 0.0f64;

        if !ctx.track.legend.title.is_empty()
            && let Some(text_ctx) = text_ctx.as_deref_mut()
        {
            match compile_legend_text(
                text_ctx,
                &ctx.track.legend.title,
                font_size + 2.0,
                max_width,
                label_color,
            ) {
                Ok(paths) => {
                    if let Some((min_x, min_y, _, max_y)) = text_bounds(&paths) {
                        let title_height = (max_y - min_y).max(0.0);
                        let translate = Affine::translate((0.0 - min_x, y_offset - min_y));
                        let translated_paths = paths
                            .iter()
                            .map(|text_path| {
                                let mut path = text_path.clone();
                                path.path.apply_affine(translate);
                                path
                            })
                            .collect::<Vec<_>>();
                        commands.push(RenderCommand::Text {
                            paths: std::sync::Arc::from(translated_paths.into_boxed_slice()),
                        });
                        y_offset += title_height + gap;
                    }
                },
                Err(err) => {
                    tracing::warn!("Legend '{}' title failed to compile: {}", ctx.track.label, err)
                },
            }
        }

        for (label, color_rgba) in entries {
            let mut row_height = swatch_size;
            let mut compiled_text = None;

            if let Some(text_ctx) = text_ctx.as_deref_mut() {
                match compile_legend_text(text_ctx, label, font_size, max_width, label_color) {
                    Ok(paths) => {
                        if let Some(bounds) = text_bounds(&paths) {
                            row_height = row_height.max((bounds.3 - bounds.1).max(0.0) + 2.0);
                            compiled_text = Some((paths, bounds));
                        }
                    },
                    Err(err) => tracing::warn!(
                        "Legend '{}' label '{}' failed to compile: {}",
                        ctx.track.label,
                        label,
                        err
                    ),
                }
            }

            let swatch_y = y_offset + (row_height - swatch_size) / 2.0;
            let swatch_rect = Rect::new(0.0, swatch_y, swatch_size, swatch_y + swatch_size);
            let swatch_path = swatch_rect.to_path(0.1);
            let fill_color = Color::from_rgba8(
                (color_rgba[0] * 255.0) as u8,
                (color_rgba[1] * 255.0) as u8,
                (color_rgba[2] * 255.0) as u8,
                (color_rgba[3] * 255.0) as u8,
            );
            commands.push(RenderCommand::Paths {
                paths: vec![crate::timeline::VelloPath {
                    path: swatch_path,
                    fill: Some(fill_color),
                    stroke: None,
                    line_cap: 0,
                    line_join: 0,
                }],
            });

            if let Some((paths, (min_x, min_y, _, max_y))) = compiled_text {
                let text_height = (max_y - min_y).max(0.0);
                let translate = Affine::translate((
                    label_offset - min_x,
                    y_offset + (row_height - text_height) / 2.0 - min_y,
                ));
                let translated_paths = paths
                    .iter()
                    .map(|text_path| {
                        let mut path = text_path.clone();
                        path.path.apply_affine(translate);
                        path
                    })
                    .collect::<Vec<_>>();
                commands.push(RenderCommand::Text {
                    paths: std::sync::Arc::from(translated_paths.into_boxed_slice()),
                });
            } else {
                let fallback_width = 80.0f64;
                let fallback_rect = Rect::new(
                    label_offset,
                    swatch_y,
                    label_offset + fallback_width,
                    swatch_y + swatch_size,
                );
                commands.push(RenderCommand::Paths {
                    paths: vec![crate::timeline::VelloPath {
                        path: fallback_rect.to_path(0.1),
                        fill: Some(Color::from_rgba8(
                            (color_rgba[0] * 255.0) as u8,
                            (color_rgba[1] * 255.0) as u8,
                            (color_rgba[2] * 255.0) as u8,
                            (color_rgba[3] * 64.0) as u8,
                        )),
                        stroke: None,
                        line_cap: 0,
                        line_join: 0,
                    }],
                });
            }

            y_offset += row_height + gap;
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
