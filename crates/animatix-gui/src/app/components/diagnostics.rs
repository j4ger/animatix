use std::borrow::Cow;

use animatix_i18n::FluentArgs;
use animatix_syntax::diagnostics::{Diagnostic, DiagnosticPhase, DiagnosticPresentation};
use eparts::widget::diagnostics::{DiagnosticEntry, DiagnosticTarget};

use crate::app::design_tokens::semantic::diagnostic as diag;
use crate::app::i18n::{Language, current_language, format, text, text_for, with_arg};

pub(crate) fn diagnostic_message(language: Language, diagnostic: &Diagnostic) -> Cow<'_, str> {
    use DiagnosticPresentation as P;

    let canonical = || Cow::Borrowed(diagnostic.message.as_str());
    let Some(presentation) = &diagnostic.presentation else {
        return canonical();
    };
    let mut args = FluentArgs::new();
    if let Some(subject) = &diagnostic.location.subject {
        args.set("subject", subject.as_str());
    }
    let id = match presentation {
        P::Parse { problem, context } => {
            return super::parse_diagnostic::message(language, problem, context)
                .map_or_else(canonical, Cow::Owned);
        },
        P::InvalidCanvasSize => "diagnostic-invalid-canvas-size",
        P::DuplicateCanvasSize => "diagnostic-duplicate-canvas-size",
        P::RemovedResolution => "diagnostic-removed-resolution",
        P::SceneCanvasSize => "diagnostic-scene-canvas-size",
        P::UndefinedLabel => "diagnostic-undefined-label",
        P::UnknownType(actor_type) => {
            args.set("type", actor_type.as_str());
            "diagnostic-unknown-type"
        },
        P::UnknownDeclarationProperty {
            actor,
            actor_type,
            property,
        } => {
            args.set("actor", actor.as_str());
            args.set("type", actor_type.as_str());
            args.set("property", property.as_str());
            "diagnostic-unknown-declaration-property"
        },
        P::UnknownEffectParameter { effect, parameter } => {
            args.set("effect", effect.as_str());
            args.set("parameter", parameter.as_str());
            "diagnostic-unknown-effect-parameter"
        },
        P::UncommonProperty {
            actor_type,
            property,
        } => {
            args.set("type", actor_type.as_str());
            args.set("property", property.as_str());
            "diagnostic-uncommon-property"
        },
        P::PropertyTypeMismatch {
            owner,
            property,
            expected,
            actual,
        } => {
            args.set("owner", owner.as_str());
            args.set("property", property.as_str());
            args.set("expected", expected.as_str());
            args.set("actual", actual.as_str());
            "diagnostic-property-type-mismatch"
        },
        P::EffectParameterTypeMismatch {
            parameter,
            expected,
            actual,
        } => {
            args.set("parameter", parameter.as_str());
            args.set("expected", expected.as_str());
            args.set("actual", actual.as_str());
            "diagnostic-effect-parameter-type-mismatch"
        },
        P::UnusedActor => "diagnostic-unused-actor",
        P::UnusedBinding => "diagnostic-unused-binding",
        P::UnusedComponent => "diagnostic-unused-component",
        P::UnusedLabel => "diagnostic-unused-label",
        P::NeverRevealed => "diagnostic-never-revealed",
        P::UnregisteredActorType(actor_type) => {
            args.set("type", actor_type.as_str());
            "diagnostic-unregistered-actor-type"
        },
    };
    language.format(id, Some(&args)).map_or_else(canonical, Cow::Owned)
}

struct AnimDiagnostic<'a> {
    diagnostic: &'a Diagnostic,
    message: Cow<'a, str>,
    phase: String,
}

impl DiagnosticEntry for AnimDiagnostic<'_> {
    fn is_error(&self) -> bool {
        self.diagnostic.is_error()
    }

    fn message(&self) -> &str {
        &self.message
    }

    fn line(&self) -> Option<usize> {
        self.diagnostic.location.line
    }

    fn column(&self) -> Option<usize> {
        self.diagnostic.location.column
    }

    fn phase_label(&self) -> Option<&str> {
        Some(&self.phase)
    }

    fn phase_color(&self) -> Option<egui::Color32> {
        Some(match self.diagnostic.phase {
            DiagnosticPhase::Parse => diag::PHASE_PARSE,
            DiagnosticPhase::Build => diag::PHASE_RESOLVE,
            DiagnosticPhase::Render => diag::PHASE_COMPILE,
        })
    }
}

pub(crate) fn diagnostic_counts(ui: &egui::Ui, errors: usize, warnings: usize) -> String {
    if errors + warnings == 0 {
        return text(ui, "gui-diagnostics-none");
    }
    let language = current_language(ui.ctx());
    let mut args = FluentArgs::new();
    args.set("errors", with_arg(language, "gui-diagnostic-error-count", "count", errors));
    args.set(
        "warnings",
        with_arg(language, "gui-diagnostic-warning-count", "count", warnings),
    );
    format(language, "gui-diagnostics-counts", args)
}

pub(crate) fn diagnostics_list(
    ui: &mut egui::Ui,
    diagnostics: &[Diagnostic],
    visible: &mut bool,
) -> Option<DiagnosticTarget> {
    let language = current_language(ui.ctx());
    let wrapped: Vec<_> = diagnostics
        .iter()
        .map(|diagnostic| AnimDiagnostic {
            diagnostic,
            message: diagnostic_message(language, diagnostic),
            phase: text_for(
                language,
                match diagnostic.phase {
                    DiagnosticPhase::Parse => "gui-diagnostic-parse",
                    DiagnosticPhase::Build => "gui-diagnostic-build",
                    DiagnosticPhase::Render => "gui-diagnostic-render",
                },
            ),
        })
        .collect();
    let title = text(ui, "gui-diagnostics-title");
    let close = text(ui, "gui-diagnostics-close");
    eparts::widget::diagnostics::diagnostics_list_with_labels(ui, &wrapped, visible, &title, &close)
}

#[cfg(test)]
mod tests {
    use super::*;
    use animatix_syntax::diagnostics::DiagnosticCode;

    #[test]
    fn config_diagnostics_sharing_a_code_have_distinct_display_messages() {
        let source = "config { canvas_size: (0, 20) }\n\nconfig { canvas_size: (10, 20) }";
        let mut diagnostics = Vec::new();
        let (statements, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "{errors:?}");
        let statements = statements.expect("valid config syntax");
        animatix_syntax::config_keys::configured_canvas_size(&statements, &mut diagnostics);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics.iter().all(|d| d.code == DiagnosticCode::InvalidConfigValue));
        let canonical = diagnostics.clone();
        for diagnostic in &diagnostics {
            assert_eq!(diagnostic_message(Language::English, diagnostic), diagnostic.message);
        }
        let zh: Vec<_> = diagnostics
            .iter()
            .map(|d| diagnostic_message(Language::SimplifiedChinese, d))
            .collect();
        assert!(zh[0].contains("正整数"));
        assert!(zh[1].contains("只能声明一次"));
        assert_ne!(diagnostics[0].location.span, diagnostics[1].location.span);
        assert_eq!(diagnostics, canonical);
    }

    #[test]
    fn diagnostic_panel_wraps_long_localized_source_values() {
        use animatix_syntax::diagnostics::ParsePresentation;
        use egui::{Pos2, Rect, Vec2, epaint::Shape};

        let value = "叙事情节的时间参数".repeat(4);
        let diagnostic = Diagnostic::error(DiagnosticCode::ParseError, DiagnosticPhase::Parse, "")
            .with_presentation(DiagnosticPresentation::Parse {
                problem: ParsePresentation::Rule {
                    id: "diagnostic-parse-duplicate-argument".into(),
                    argument: Some(value.clone()),
                },
                context: vec!["actor declaration".into()],
            });

        for language in [Language::English, Language::SimplifiedChinese] {
            let ctx = egui::Context::default();
            crate::fonts::install_fonts(&ctx);
            crate::app::i18n::set_language(&ctx, language);
            let message = diagnostic_message(language, &diagnostic).into_owned();
            assert!(message.contains(&value));
            if language == Language::SimplifiedChinese {
                assert!(message.contains("命名参数"));
                assert!(message.contains("对象声明"));
            }
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(300.0, 220.0))),
                    ..Default::default()
                },
                |ui| {
                    diagnostics_list(ui, std::slice::from_ref(&diagnostic), &mut true);
                },
            );
            output.textures_delta.clear();
            let (clip_rect, shape) = output
                .shapes
                .iter()
                .find_map(|clipped| match &clipped.shape {
                    Shape::Text(shape) if shape.galley.job.text == message => {
                        Some((clipped.clip_rect, shape))
                    },
                    _ => None,
                })
                .expect("the translated diagnostic is painted");
            assert!(shape.galley.rows.len() > 2);
            assert!(!shape.galley.elided);
            assert!(shape.pos.x + shape.galley.size().x <= clip_rect.right() + 1.0);
        }
    }
}
