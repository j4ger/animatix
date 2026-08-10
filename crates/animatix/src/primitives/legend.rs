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

        // Legend is an annotation primitive, so it bypasses the generic actor
        // build path and must resolve its own `at` position.
        let mut at = [0.0f32, 0.0f32];
        for prop in props {
            if prop.name == "at"
                && let Some(parsed) = parse_vec2(&prop.value)
            {
                at = parsed;
            }
        }
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
        let swatch_size = 16.0f64;
        let spacing = 8.0f64;
        let label_offset = swatch_size + spacing;
        let line_height = swatch_size + spacing;
        let label_color = label_color_for_background(ctx.background_color);
        let mut y_offset = 0.0f64;

        for (label, color_rgba) in entries {
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

            if let Some(text_ctx) = text_ctx.as_deref_mut() {
                let paths = match text_ctx.text_compiler.compile(
                    label,
                    crate::renderer::text::DEFAULT_FONT_FAMILY,
                    14.0,
                    400.0,
                    "normal",
                    1.2,
                    0.0,
                    0.0,
                    label_color,
                    crate::renderer::text::TextKind::Text,
                    text_ctx.font_context,
                    0.0,
                    "left",
                    "visible",
                ) {
                    Ok(paths) => paths,
                    Err(err) => {
                        tracing::warn!(
                            "Legend '{}' label '{}' failed to compile: {}",
                            ctx.track.label,
                            label,
                            err
                        );
                        let fallback_width = 80.0f64;
                        let fallback_rect = Rect::new(
                            label_offset,
                            y_offset,
                            label_offset + fallback_width,
                            y_offset + swatch_size,
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
                        y_offset += line_height;
                        continue;
                    },
                };

                if !paths.is_empty() {
                    let mut min_x = f64::INFINITY;
                    let mut max_x = f64::NEG_INFINITY;
                    let mut min_y = f64::INFINITY;
                    let mut max_y = f64::NEG_INFINITY;
                    for text_path in paths.iter() {
                        let bounds = text_path.path.bounding_box();
                        min_x = min_x.min(bounds.x0);
                        max_x = max_x.max(bounds.x1);
                        min_y = min_y.min(bounds.y0);
                        max_y = max_y.max(bounds.y1);
                    }
                    if min_x.is_finite()
                        && max_x.is_finite()
                        && min_y.is_finite()
                        && max_y.is_finite()
                    {
                        let text_height = (max_y - min_y).max(0.0);
                        let translate = Affine::translate((
                            label_offset - min_x,
                            y_offset + (swatch_size - text_height) / 2.0 - min_y,
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
                    }
                }
            } else {
                // Fallback for contexts that do not provide a text compiler.
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
                        (color_rgba[3] * 64.0) as u8,
                    )),
                    stroke: None,
                    line_cap: 0,
                    line_join: 0,
                };
                commands.push(RenderCommand::Paths {
                    paths: vec![text_bg_vello],
                });
            }

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
