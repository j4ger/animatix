//! Timing/stagger modifier diagnostics and shared statement helpers.

use super::ModifierHost;
use crate::ast::{Expr, Stmt};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::{EASING_REGISTRY, easing_expects_args};
use crate::timeline::morph::MorphOptions;

pub(crate) fn sequence_stmt_kind(stmt: &Stmt) -> &'static str {
    match stmt {
        Stmt::Action(..) => "action",
        Stmt::Assignment { .. } => "assignment",
        Stmt::Sequence { .. } => "sequence",
        Stmt::Stagger { .. } => "stagger",
        Stmt::LetDecl { .. } => "let declaration",
        Stmt::TypeAlias { .. } => "type alias",
        Stmt::ActorDecl { .. } => "actor declaration",
        Stmt::Import { .. } => "import",
        Stmt::Keyframe { .. } => "keyframe",
        Stmt::RelativeKeyframe { .. } => "relative keyframe",
        Stmt::Always { .. } => "always block",
        Stmt::ReactiveBinding { .. } => "reactive binding",
        Stmt::Conditional { .. } => "conditional",
        Stmt::Match { .. } => "match",
        Stmt::ForLoop { .. } => "for loop",
        Stmt::ComponentDef(..) => "component definition",
        Stmt::FnDecl { .. } => "function",
        Stmt::Block { .. } => "block",
        Stmt::Return { .. } => "return",
        Stmt::Expr(..) => "expression",
        Stmt::Config { .. } => "config block",
        Stmt::Comment(..) => "comment",
        Stmt::Scene { .. } => "scene declaration",
        Stmt::Play { .. } => "play statement",
        Stmt::Step { .. } => "step marker",
    }
}

pub(crate) fn push_unsupported_stagger_statement_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    kind: &str,
) {
    let message = if kind == "actor declaration" {
        "Stagger blocks do not support actor declarations. Declare actors before the composition block, then reference them inside.".to_string()
    } else {
        format!("Stagger blocks support only actions and assignments; '{kind}' is not supported.")
    };
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::UnsupportedStaggerStatement,
            DiagnosticPhase::Build,
            message,
        )
        .with_subject("stagger"),
    );
}

pub(crate) fn config_string_value(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Str(value) | Expr::Ident(value) => Some(value.clone()),
        _ => None,
    }
}

pub(crate) fn push_unknown_target_path_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
    target_key: &str,
    suggestion: Option<&str>,
    span: Option<crate::ast::Span>,
) {
    let hint = suggestion
        .map(|candidate| format!(" Did you mean '{candidate}'?"))
        .unwrap_or_default();
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::UnknownTargetPath,
            DiagnosticPhase::Build,
            format!(
                "Assignment target '{target_key}' does not resolve to a declared actor or nested label; ignoring this assignment.{hint}"
            ),
        )
        .with_subject(subject)
        .with_ast_span(span),
    );
}

impl ModifierHost {
    pub(super) fn display_name(self) -> &'static str {
        match self {
            ModifierHost::Action => "action",
            ModifierHost::Assignment => "assignment",
            ModifierHost::Text => "text declaration",
            ModifierHost::Typst => "typst declaration",
            ModifierHost::Code => "code declaration",
            ModifierHost::ActorDeclaration => "actor declaration",
        }
    }

    pub(super) fn supports_morph_modifiers(self) -> bool {
        matches!(
            self,
            ModifierHost::Text
                | ModifierHost::Typst
                | ModifierHost::Code
                | ModifierHost::ActorDeclaration
        )
    }
}

/// Whether the action named by `subject` declares this modifier key.
///
/// The action signatures are the single source for accepted keys, so an action
/// that grows a parameter cannot go stale here; `validate_action_modifiers` is
/// what reports a real typo.
pub(super) fn action_declares_modifier(subject: Option<&str>, name: &str) -> bool {
    let Some(verb) = subject else {
        return false;
    };
    crate::timeline::actions::get_action_signatures()
        .into_iter()
        .find(|sig| sig.name == verb)
        .is_some_and(|sig| sig.modifiers.iter().any(|p| p.name == name))
}

/// Explain an `ease:` value the build layer could not resolve.
///
/// The supported-name list is read from the registry rather than written out
/// here, so adding a curve cannot leave this message behind — the previous
/// version of this string had already gone stale.
pub(super) fn ease_value_diagnostic(value: &Expr, host: ModifierHost) -> String {
    let supported = EASING_REGISTRY.iter().map(|(id, _)| *id).collect::<Vec<_>>().join(", ");
    match value {
        Expr::Call(name, _) => match easing_expects_args(name) {
            Some(form) => {
                format!("Ease '{name}' on {} expects the form {form}.", host.display_name())
            },
            None => format!(
                "Unknown easing '{name}' on {}; supported values are {supported}, or cubic-bezier(x1, y1, x2, y2).",
                host.display_name()
            ),
        },
        Expr::Ident(raw) => format!(
            "Unsupported ease value '{raw}' on {}; supported values are {supported}, or a cubic-bezier(…) / spring(…) call.",
            host.display_name()
        ),
        other => format!(
            "Unsupported ease modifier value {other:?} on {}; expected an easing name or a cubic-bezier(…) / spring(…) call.",
            host.display_name()
        ),
    }
}

pub(crate) fn parse_duration_literal(raw: &str) -> Option<f64> {
    if let Some(ms) = raw.strip_suffix("ms") {
        ms.parse::<f64>().ok()
    } else if let Some(seconds) = raw.strip_suffix('s') {
        seconds.parse::<f64>().ok().map(|seconds| seconds * 1000.0)
    } else {
        None
    }
}

pub(crate) fn push_conflicting_modifier_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    logical_name: &str,
    host: ModifierHost,
    subject: Option<&str>,
) {
    push_modifier_diagnostic(
        diagnostics,
        DiagnosticCode::ConflictingModifierKey,
        format!(
            "Conflicting '{logical_name}' modifiers on {}; using the last value provided.",
            host.display_name()
        ),
        subject,
    );
}

pub(crate) fn has_non_default_morph_options(options: MorphOptions) -> bool {
    options != MorphOptions::default()
}

pub(crate) fn push_modifier_diagnostic(
    diagnostics: &mut Vec<Diagnostic>,
    code: DiagnosticCode,
    message: String,
    subject: Option<&str>,
) {
    let diagnostic = Diagnostic::warning(code, DiagnosticPhase::Build, message);
    diagnostics.push(match subject {
        Some(subject) => diagnostic.with_subject(subject),
        None => diagnostic,
    });
}
