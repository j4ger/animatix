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

fn tagged_string_opt(track: &AnimationTrack, key: &'static str, time_ms: u64) -> Option<String> {
    match crate::timeline::dispatch::read_property_value(
        track,
        crate::timeline::property_registry::ActorField::Tagged(key),
        time_ms,
    ) {
        Some(crate::timeline::property_engine::PropertyValue::String(value)) => Some(value),
        _ => None,
    }
}

fn tagged_f32_opt(track: &AnimationTrack, key: &'static str, time_ms: u64) -> Option<f32> {
    match crate::timeline::dispatch::read_property_value(
        track,
        crate::timeline::property_registry::ActorField::Tagged(key),
        time_ms,
    ) {
        Some(crate::timeline::property_engine::PropertyValue::F32(value)) => Some(value),
        _ => None,
    }
}

fn tagged_color_opt(track: &AnimationTrack, key: &'static str, time_ms: u64) -> Option<[f32; 4]> {
    match crate::timeline::dispatch::read_property_value(
        track,
        crate::timeline::property_registry::ActorField::Tagged(key),
        time_ms,
    ) {
        Some(crate::timeline::property_engine::PropertyValue::Color(value)) => Some(value),
        Some(crate::timeline::property_engine::PropertyValue::Vec4(value)) => Some(value),
        _ => None,
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

/// Singleton instance of `LegendPrimitive`.
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

        let time_ms = ctx.time_ms as u64;
        let default_fields = [
            (
                "legend_title",
                crate::timeline::property_engine::PropertyValue::String(String::new()),
            ),
            ("legend_font_size", crate::timeline::property_engine::PropertyValue::F32(14.0)),
            ("legend_swatch_size", crate::timeline::property_engine::PropertyValue::F32(16.0)),
            ("legend_gap", crate::timeline::property_engine::PropertyValue::F32(8.0)),
            (
                "legend_text_max_width",
                crate::timeline::property_engine::PropertyValue::F32(240.0),
            ),
        ];
        for (key, value) in default_fields {
            crate::timeline::property_engine::write_property_field(
                track,
                crate::timeline::property_registry::ActorField::Tagged(key),
                value,
                time_ms,
                time_ms,
                crate::easing::Easing::Linear,
                ctx.diagnostics,
            );
        }

        for prop in props {
            let prop_subject = format!("{label}.{}", prop.name);
            let (key, value_type) = match prop.name.as_str() {
                "title" => ("legend_title", crate::timeline::property_registry::ValueType::String),
                "font_size" => {
                    ("legend_font_size", crate::timeline::property_registry::ValueType::F32)
                },
                "swatch_size" => {
                    ("legend_swatch_size", crate::timeline::property_registry::ValueType::F32)
                },
                "gap" => ("legend_gap", crate::timeline::property_registry::ValueType::F32),
                "text_max_width" => {
                    ("legend_text_max_width", crate::timeline::property_registry::ValueType::F32)
                },
                "label_color" => {
                    if matches!(&prop.value, Expr::Ident(name) if name == "auto") {
                        continue;
                    }
                    ("legend_label_color", crate::timeline::property_registry::ValueType::Color)
                },
                _ => continue,
            };
            if let Some(pv) = crate::timeline::property_engine::parse_property_value(
                value_type,
                &prop.value,
                &ctx.timeline.env,
                ctx.diagnostics,
                &prop_subject,
            ) {
                crate::timeline::property_engine::write_property_field(
                    track,
                    crate::timeline::property_registry::ActorField::Tagged(key),
                    pv,
                    time_ms,
                    time_ms,
                    crate::easing::Easing::Linear,
                    ctx.diagnostics,
                );
            }
        }

        track.legend.title = title;
        track.legend.font_size = font_size;
        track.legend.label_color = label_color;
        track.legend.swatch_size = swatch_size;
        track.legend.gap = gap;
        track.legend.text_max_width = text_max_width;

        // Legend is an annotation primitive, so it bypasses the generic actor
        // build path and must resolve its own `at` position.
        track.geometry.position.ensure([0.0, 0.0]).add_keyframe(
            time_ms,
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
        let title = tagged_string_opt(ctx.track, "legend_title", ctx.time_ms)
            .unwrap_or_else(|| ctx.track.legend.title.clone());
        let font_size = tagged_f32_opt(ctx.track, "legend_font_size", ctx.time_ms)
            .unwrap_or(ctx.track.legend.font_size)
            .max(1.0);
        let swatch_size = tagged_f32_opt(ctx.track, "legend_swatch_size", ctx.time_ms)
            .unwrap_or(ctx.track.legend.swatch_size) as f64;
        let gap = tagged_f32_opt(ctx.track, "legend_gap", ctx.time_ms)
            .unwrap_or(ctx.track.legend.gap) as f64;
        let label_offset = swatch_size + gap;
        let max_width = tagged_f32_opt(ctx.track, "legend_text_max_width", ctx.time_ms)
            .unwrap_or(ctx.track.legend.text_max_width);
        let label_color = tagged_color_opt(ctx.track, "legend_label_color", ctx.time_ms)
            .or(ctx.track.legend.label_color)
            .unwrap_or_else(|| label_color_for_background(ctx.background_color));
        let mut y_offset = 0.0f64;

        if !title.is_empty()
            && let Some(text_ctx) = text_ctx.as_deref_mut()
        {
            match compile_legend_text(text_ctx, &title, font_size + 2.0, max_width, label_color) {
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
