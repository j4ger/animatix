//! Text family evaluation and build systems (Text, Math, Code, Typst, Counter).

use kurbo::Shape;

use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::primitives::{EvaluateCtx, RenderCommand, TextCompileCtx, evaluate_text_paths};
use crate::renderer::error::RenderError;
use crate::renderer::text::TextKind;
use crate::renderer::types::GradientSpec;
use crate::timeline::property_registry::ActorField;
use crate::timeline::{Timeline, TrackAccessor};

// ── Shared text evaluation helper ────────────────────────────────────────────

fn evaluate_text_kind(
    ctx: &EvaluateCtx,
    text_ctx: Option<&mut TextCompileCtx>,
    kind: TextKind,
    default_font_size: f32,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let style = ctx.style;
    let has_text_ctx = text_ctx.is_some();

    let (paths, clip_path) = if let Some(text_ctx) = text_ctx {
        evaluate_text_paths(ctx, text_ctx, kind, default_font_size)
    } else {
        let text = ctx.text.expect("text primitive requires text component");
        let (paths, clip) = text.evaluate_paths_and_clip(style, ctx.time_ms);
        Ok((std::sync::Arc::from(paths), clip))
    }?;

    if paths.is_empty() {
        Ok(None)
    } else {
        let fill_gradient = if style.fill_gradient.is_some() {
            Some(Box::new(style.fill_gradient.get(ctx.time_ms, GradientSpec::default())))
        } else {
            None
        };
        let col = style.color.get(ctx.time_ms, [1.0, 1.0, 1.0, 1.0]);
        let tint_color = Some(crate::renderer::text::f32_color_to_rgba8(&col));
        let text = ctx.text.expect("text primitive requires text component");
        // `text.hardware_layout` was compiled from the declaration's static
        // glyphs. With a text context it is only a valid fast path while `paths`
        // *is* that static allocation: a frame-time `.text`/style override
        // recompiles through `evaluate_text_paths` and returns a different
        // `Arc`, so replaying the declared glyph run would hide the frame-time
        // string. Position-only overrides leave the static `Arc` untouched and
        // keep the fast path. Without a text context `evaluate_paths_and_clip`
        // rebuilds the static paths as a fresh `Arc`; no override was applied,
        // so the declared layout still describes them.
        let static_paths = text.static_paths.as_ref();
        let hardware_layout = if text.char_progress.is_none()
            && text.text_anchor.get(ctx.time_ms, "center".to_string()) == "center"
            && static_paths.is_some_and(|static_paths| {
                !has_text_ctx || std::sync::Arc::ptr_eq(static_paths, &paths)
            }) {
            text.hardware_layout.clone()
        } else {
            None
        };

        let mut commands = Vec::new();
        if kind == TextKind::Typst {
            let hl_opacity =
                ctx.highlight.map(|h| h.highlight_opacity.get(ctx.time_ms, 0.0)).unwrap_or(0.0);
            if hl_opacity > 0.001 {
                let highlight = ctx.highlight.unwrap();
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
                        highlight.highlight_color.get(ctx.time_ms, [0.3, 0.5, 1.0, 1.0]);
                    let hl_padding = highlight.highlight_padding.get(ctx.time_ms, 4.0);
                    let hl_radius = highlight.highlight_radius.get(ctx.time_ms, 3.0);
                    let hl_blend = highlight.highlight_blend;

                    let pad = hl_padding as f64;
                    let hl_rect =
                        kurbo::Rect::new(min_x - pad, min_y - pad, max_x + pad, max_y + pad);
                    let hl_color = vello::peniko::Color::from_rgba8(
                        (hl_color_arr[0] * 255.0) as u8,
                        (hl_color_arr[1] * 255.0) as u8,
                        (hl_color_arr[2] * 255.0) as u8,
                        255,
                    );
                    commands.push(RenderCommand::HighlightLayer {
                        rect: hl_rect,
                        color: hl_color,
                        blend: hl_blend,
                        alpha: hl_opacity,
                        corner_radius: hl_radius as f64,
                    });
                }
            }
        }
        commands.push(RenderCommand::Text {
            paths,
            fill_gradient,
            clip_path,
            tint_color,
            hardware_layout,
        });
        Ok(Some(commands))
    }
}

// ── Evaluate functions ───────────────────────────────────────────────────────

/// Evaluate draw commands for a `Text` actor.
pub fn evaluate_text(
    ctx: &EvaluateCtx,
    text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    evaluate_text_kind(
        ctx,
        text_ctx,
        TextKind::Text,
        crate::renderer::text::default_font_size(TextKind::Text),
    )
}

/// Evaluate draw commands for a `Math` actor.
pub fn evaluate_math(
    ctx: &EvaluateCtx,
    text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    evaluate_text_kind(
        ctx,
        text_ctx,
        TextKind::Math,
        crate::renderer::text::default_font_size(TextKind::Math),
    )
}

/// Evaluate draw commands for a `Code` actor.
pub fn evaluate_code(
    ctx: &EvaluateCtx,
    text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    evaluate_text_kind(
        ctx,
        text_ctx,
        TextKind::Code,
        crate::renderer::text::default_font_size(TextKind::Code),
    )
}

/// Evaluate draw commands for a `Typst` actor.
pub fn evaluate_typst(
    ctx: &EvaluateCtx,
    text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    evaluate_text_kind(
        ctx,
        text_ctx,
        TextKind::Typst,
        crate::renderer::text::default_font_size(TextKind::Typst),
    )
}

fn counter_numeric_override(ctx: &EvaluateCtx, property: &str) -> Option<f32> {
    match ctx.overrides.and_then(|overrides| overrides.get(property)) {
        Some(crate::timeline::Value::Num(value)) => Some(*value as f32),
        Some(other) => {
            tracing::debug!("Counter {property} override has the wrong type: {other:?}");
            None
        },
        None => None,
    }
}

fn counter_decimals_override(ctx: &EvaluateCtx) -> Option<u32> {
    match ctx.overrides.and_then(|overrides| overrides.get("decimals")) {
        Some(crate::timeline::Value::Num(value)) => Some((*value).max(0.0) as u32),
        Some(other) => {
            tracing::debug!("Counter decimals override has the wrong type: {other:?}");
            None
        },
        None => None,
    }
}

fn counter_comma_override(ctx: &EvaluateCtx) -> Option<bool> {
    match ctx.overrides.and_then(|overrides| overrides.get("comma")) {
        Some(crate::timeline::Value::Bool(value)) => Some(*value),
        Some(other) => {
            tracing::debug!("Counter comma override has the wrong type: {other:?}");
            None
        },
        None => None,
    }
}

fn counter_string_property(ctx: &EvaluateCtx, property: &'static str, default: &str) -> String {
    if let Some(value) = ctx.overrides.and_then(|overrides| overrides.get(property)) {
        return match value {
            crate::timeline::Value::Str(value) => value.clone(),
            other => {
                tracing::debug!("Counter {property} override has the wrong type: {other:?}");
                default.to_owned()
            },
        };
    }
    match ctx.read_property_value(ActorField::Tagged(property), ctx.time_ms) {
        Some(crate::timeline::property_engine::PropertyValue::String(value)) => value,
        Some(other) => {
            tracing::debug!("Counter {property} property has the wrong type: {other:?}");
            default.to_owned()
        },
        None => default.to_owned(),
    }
}

struct CounterFrameTypography {
    font_family: String,
    font_size: f32,
    font_weight: f32,
    font_style: String,
    line_height: f32,
}

fn counter_frame_typography(
    ctx: &EvaluateCtx,
    data: &crate::primitives::counter::CounterData,
) -> CounterFrameTypography {
    let tracks = ctx.text.expect("Counter requires text tracks");
    let mut typography = CounterFrameTypography {
        font_family: tracks.font_family.get(ctx.time_ms, data.font_family.clone()),
        font_size: tracks.font_size.get(ctx.time_ms, data.font_size),
        font_weight: tracks.font_weight.get(ctx.time_ms, data.font_weight),
        font_style: tracks.font_style.get(ctx.time_ms, data.font_style.clone()),
        line_height: tracks.line_height.get(ctx.time_ms, data.line_height),
    };
    if let Some(overrides) = ctx.overrides {
        if let Some(crate::timeline::Value::Str(value)) = overrides.get("font_family") {
            typography.font_family.clone_from(value);
        }
        if let Some(crate::timeline::Value::Num(value)) = overrides.get("font_size") {
            typography.font_size = *value as f32;
        }
        if let Some(crate::timeline::Value::Num(value)) = overrides.get("font_weight") {
            typography.font_weight = *value as f32;
        }
        if let Some(crate::timeline::Value::Str(value)) = overrides.get("font_style") {
            typography.font_style.clone_from(value);
        }
        if let Some(crate::timeline::Value::Num(value)) = overrides.get("line_height") {
            typography.line_height = *value as f32;
        }
        for (property, value) in [
            ("font_family", overrides.get("font_family")),
            ("font_style", overrides.get("font_style")),
        ] {
            if let Some(value) = value
                && !matches!(value, crate::timeline::Value::Str(_))
            {
                tracing::debug!("Counter {property} override has the wrong type: {value:?}");
            }
        }
        for (property, value) in [
            ("font_size", overrides.get("font_size")),
            ("font_weight", overrides.get("font_weight")),
            ("line_height", overrides.get("line_height")),
        ] {
            if let Some(value) = value
                && !matches!(value, crate::timeline::Value::Num(_))
            {
                tracing::debug!("Counter {property} override has the wrong type: {value:?}");
            }
        }
    }
    if typography.font_family.is_empty() {
        typography.font_family = crate::renderer::text::DEFAULT_FONT_FAMILY.to_owned();
    }
    typography
}

/// Evaluate draw commands for a `Counter` actor.
pub fn evaluate_counter(
    ctx: &EvaluateCtx,
    text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let style = ctx.style;
    let Some(base_data) = ctx.text.and_then(|t| t.counter_data.as_ref()) else {
        return Ok(None);
    };

    let prefix = counter_string_property(ctx, "prefix", &base_data.prefix_text);
    let suffix = counter_string_property(ctx, "suffix", &base_data.suffix_text);

    // Counter glyphs are prebaked at declaration time. Frame-time typography
    // and affix changes need a matching glyph set; compile_text_cached keeps
    // repeated samples bounded and shares the result with ordinary text.
    let mut dynamic_data = None;
    let mut dynamic_prefix = None;
    let mut dynamic_suffix = None;
    if let Some(text_ctx) = text_ctx {
        let typography = counter_frame_typography(ctx, base_data);
        let font_changed = typography.font_family != base_data.font_family
            || typography.font_size != base_data.font_size
            || typography.font_weight != base_data.font_weight
            || typography.font_style != base_data.font_style
            || typography.line_height != base_data.line_height;
        if font_changed {
            dynamic_data = Some(
                crate::primitives::counter::prebake_counter(
                    &typography.font_family,
                    typography.font_size,
                    typography.font_weight,
                    &typography.font_style,
                    typography.line_height,
                    base_data.color_rgba,
                    base_data.initial_value,
                    base_data.decimals,
                    base_data.use_comma,
                    &prefix,
                    &suffix,
                    text_ctx.font_context,
                )?
                .data,
            );
        } else {
            if prefix != base_data.prefix_text {
                dynamic_prefix = Some(crate::primitives::counter::compile_counter_affix(
                    &prefix,
                    base_data,
                    text_ctx.font_context,
                )?);
            }
            if suffix != base_data.suffix_text {
                dynamic_suffix = Some(crate::primitives::counter::compile_counter_affix(
                    &suffix,
                    base_data,
                    text_ctx.font_context,
                )?);
            }
        }
    }
    let data = dynamic_data.as_ref().unwrap_or(base_data);

    let value = counter_numeric_override(ctx, "value").unwrap_or_else(|| {
        match ctx.read_property_value(ActorField::Tagged("value"), ctx.time_ms) {
            Some(crate::timeline::property_engine::PropertyValue::F32(n)) => n,
            _ => data.initial_value,
        }
    });

    let decimals = counter_decimals_override(ctx).unwrap_or_else(|| {
        match ctx.read_property_value(ActorField::Tagged("decimals"), ctx.time_ms) {
            Some(crate::timeline::property_engine::PropertyValue::U32(n)) => n,
            _ => data.decimals,
        }
    });

    let use_comma = counter_comma_override(ctx).unwrap_or_else(|| {
        match ctx.read_property_value(ActorField::Tagged("comma"), ctx.time_ms) {
            Some(crate::timeline::property_engine::PropertyValue::Bool(b)) => b,
            _ => data.use_comma,
        }
    });

    let (paths, clip_rect) = crate::primitives::counter::render_counter_columns_with_affixes(
        data,
        value,
        decimals,
        use_comma,
        dynamic_prefix.as_ref(),
        dynamic_suffix.as_ref(),
    );

    let tint_color = style.color.as_ref().map(|color_track| {
        let col = color_track.evaluate(ctx.time_ms);
        [
            (col[0] * 255.0) as u8,
            (col[1] * 255.0) as u8,
            (col[2] * 255.0) as u8,
            (col[3] * 255.0) as u8,
        ]
    });

    let fill_gradient = if style.fill_gradient.is_some() {
        Some(Box::new(style.fill_gradient.get(ctx.time_ms, GradientSpec::default())))
    } else {
        None
    };

    let clip_path = Some(clip_rect.to_path(0.1));

    Ok(Some(vec![RenderCommand::Text {
        paths: std::sync::Arc::from(paths),
        fill_gradient,
        clip_path,
        tint_color,
        hardware_layout: None,
    }]))
}

// ── Build extra functions ────────────────────────────────────────────────────

/// Shared custom build logic for text-family actors.
pub fn build_extra_text_named(
    timeline: &mut Timeline,
    label: &str,
    ty: &'static str,
    props: &[Property],
    modifiers: &[Modifier],
    _children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    if let Err(e) = timeline.process_text_actor_decl(
        ty,
        label,
        props,
        modifiers,
        time_ms,
        parent_label,
        diagnostics,
    ) {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::InvalidModifierValue,
                DiagnosticPhase::Build,
                format!("{e}"),
            )
            .with_subject(label),
        );
    }
    Ok(())
}
