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
        let hardware_layout = if text.char_progress.is_none()
            && text.text_anchor.get(ctx.time_ms, "center".to_string()) == "center"
        {
            text.hardware_layout.clone()
        } else {
            None
        };
        Ok(Some(vec![RenderCommand::Text {
            paths,
            fill_gradient,
            clip_path,
            tint_color,
            hardware_layout,
        }]))
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

/// Evaluate draw commands for a `Counter` actor.
pub fn evaluate_counter(
    ctx: &EvaluateCtx,
    _text_ctx: Option<&mut TextCompileCtx>,
) -> Result<Option<Vec<RenderCommand>>, RenderError> {
    let style = ctx.style;
    let Some(data) = ctx.text.and_then(|t| t.counter_data.as_ref()) else {
        return Ok(None);
    };

    let value = match ctx.read_property_value(ActorField::Tagged("value"), ctx.time_ms) {
        Some(crate::timeline::property_engine::PropertyValue::F32(n)) => n,
        _ => data.initial_value,
    };

    let decimals = match ctx.read_property_value(ActorField::Tagged("decimals"), ctx.time_ms) {
        Some(crate::timeline::property_engine::PropertyValue::U32(n)) => n,
        _ => data.decimals,
    };

    let use_comma = match ctx.read_property_value(ActorField::Tagged("comma"), ctx.time_ms) {
        Some(crate::timeline::property_engine::PropertyValue::Bool(b)) => b,
        _ => data.use_comma,
    };

    let (paths, clip_rect) =
        crate::primitives::counter::render_counter_columns(data, value, decimals, use_comma);

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

fn build_extra_text_named(
    ty: &'static str,
    timeline: &mut Timeline,
    label: &str,
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

/// Custom build logic for `Text` actors.
pub fn build_extra_text(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_text_named(
        "Text",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `Code` actors.
pub fn build_extra_code(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_text_named(
        "Code",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `Math` actors.
pub fn build_extra_math(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_text_named(
        "Math",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `Typst` actors.
pub fn build_extra_typst(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_text_named(
        "Typst",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}

/// Custom build logic for `Counter` actors.
pub fn build_extra_counter(
    timeline: &mut Timeline,
    label: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    time_ms: f64,
    parent_label: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), Vec<Diagnostic>> {
    build_extra_text_named(
        "Counter",
        timeline,
        label,
        props,
        modifiers,
        children,
        time_ms,
        parent_label,
        diagnostics,
    )
}
