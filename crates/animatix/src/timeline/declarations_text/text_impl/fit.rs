use super::*;

use crate::renderer::text::{TextKind, compile_text_cached};

impl Timeline {
    pub(super) fn fit_text(
        &self,
        label: &str,
        props: &[Property],
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        parsed: &mut ParsedTextProperties,
    ) -> Result<(), RenderError> {
        let mut bounds = [0.0; 3];
        for (index, property) in
            ["fit_min_size", "fit_max_height", "fit_max_lines"].iter().enumerate()
        {
            if let Some(prop) = props.iter().rev().find(|prop| prop.name == *property)
                && let Some(value) =
                    evaluate_expr_with_lookup_diagnostic(&prop.value, env, diagnostics, label)
            {
                bounds[index] = value.as_num() as f32;
            }
        }
        let [minimum, height, lines] = bounds;
        if bounds == [0.0; 3] {
            return Ok(());
        }
        let nominal = parsed.font_size;
        if !nominal.is_finite()
            || nominal <= 0.0
            || bounds.iter().any(|value| !value.is_finite() || *value < 0.0)
            || lines.fract() != 0.0
        {
            return Err(RenderError::TextCompilation(
                "text fit requires a positive font size, non-negative bounds, and a whole line count".into(),
            ));
        }
        let minimum = if minimum == 0.0 {
            nominal
        } else {
            minimum.min(nominal)
        };
        let family = if parsed.font_family.is_empty() {
            TextDeclarationKind::Text.default_font_family()
        } else {
            &parsed.font_family
        };
        let mut request = TextCompileRequest {
            max_width: parsed.max_width,
            // Measure every laid-out line, including trailing empty lines.
            max_lines: (parsed.max_width <= 0.0 && !parsed.text_content.contains('\n'))
                .then_some(usize::MAX),
            text_align: &parsed.text_align,
            overflow: &parsed.overflow,
            ..TextCompileRequest::new(
                TextKind::Text,
                &parsed.text_content,
                TextStyle {
                    font_family: family,
                    font_size: nominal,
                    font_weight: parsed.font_weight,
                    font_style: &parsed.font_style,
                    line_height: parsed.line_height,
                    letter_spacing: parsed.letter_spacing,
                    word_spacing: parsed.word_spacing,
                    color: parsed.color_rgba,
                },
            )
        };
        let step = ((nominal - minimum) / 64.0).max(0.25);
        for index in 0..=64 {
            let size = if index == 64 {
                minimum
            } else {
                (nominal - index as f32 * step).max(minimum)
            };
            request.style.font_size = size;
            let measured = compile_text_cached(&request, self.font_context.as_ref())?;
            if let Some(metrics) = measured.layout_metrics
                && (parsed.max_width <= 0.0
                    || metrics.width.max(measured.half_size[0] * 2.0) <= parsed.max_width + 0.001)
                && (height == 0.0
                    || metrics.height.max(measured.half_size[1] * 2.0) <= height + 0.001)
                && (lines == 0.0 || metrics.lines <= lines as usize)
            {
                parsed.font_size = size;
                return Ok(());
            }
            if size == minimum {
                break;
            }
        }
        parsed.font_size = minimum;
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::LayoutSizeFallback,
                DiagnosticPhase::Build,
                format!("Text '{label}' exceeds its fit bounds at the minimum size ({minimum}pt)."),
            )
            .with_subject(label),
        );
        Ok(())
    }
}
