pub mod entrance;
pub mod exit;
pub mod motion;
pub mod registry;
pub mod reveal;

use crate::ast::Action;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::timeline::Timeline;
use entrance::{FadeIn, WipeIn};
use exit::FadeOut;
use motion::{Move, Rotate, Scale, Shift};
use registry::{ActionSignature, BuiltinAction};
use reveal::{DrawIn, DrawOut, RevealOut, WipeOut};

fn push_unknown_action_diagnostic(action: &Action, diagnostics: &mut Vec<Diagnostic>) {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::UnknownAction,
            DiagnosticPhase::Build,
            format!(
                "Unknown action '{}'; no built-in action is currently registered for it.",
                action.verb
            ),
        )
        .with_subject(&action.verb),
    );
}

fn push_unsupported_action_target_diagnostic(
    verb: &str,
    target: &str,
    reason: &str,
    diagnostics: &mut Vec<Diagnostic>,
) {
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::UnsupportedActionTarget,
            DiagnosticPhase::Build,
            format!("Action '{verb}' does not support target '{target}': {reason}."),
        )
        .with_subject(format!("{verb} {target}")),
    );
}

pub(crate) fn ensure_target_exists(
    timeline: &Timeline,
    target: &str,
    verb: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if timeline.tracks.contains_key(target) {
        return true;
    }

    push_unsupported_action_target_diagnostic(
        verb,
        target,
        "the target is not declared yet",
        diagnostics,
    );
    false
}

pub(crate) fn ensure_vector_reveal_target(
    timeline: &Timeline,
    target: &str,
    verb: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let Some(track) = timeline.tracks.get(target) else {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "the target is not declared yet",
            diagnostics,
        );
        return false;
    };

    if track.image.last_value().is_some() {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "image targets only support opacity-based actions right now",
            diagnostics,
        );
        return false;
    }

    if !track.text_paths.default_value.is_empty() || !track.text_paths.keyframes.is_empty() {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "text-like targets only support opacity-based actions right now",
            diagnostics,
        );
        return false;
    }

    if timeline
        .nodes
        .get(target)
        .is_some_and(|node| !node.children.is_empty())
        && track.vector_paths.default_value.is_empty()
        && track.vector_paths.keyframes.is_empty()
        && track.svg_paths.is_empty()
    {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "group and layout containers do not support vector reveal actions directly",
            diagnostics,
        );
        return false;
    }

    true
}

/// Returns a list of all registered built-in actions.
fn get_builtin_actions() -> Vec<Box<dyn BuiltinAction>> {
    vec![
        Box::new(FadeIn),
        Box::new(WipeIn),
        Box::new(Move),
        Box::new(Shift),
        Box::new(Rotate),
        Box::new(Scale),
        Box::new(DrawIn),
        Box::new(DrawOut),
        Box::new(FadeOut),
        Box::new(WipeOut),
        Box::new(RevealOut),
    ]
}

/// Looks up the action by verb and executes it if found.
pub fn process_action(
    action: &Action,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let actions = get_builtin_actions();
    for builtin in actions {
        if builtin.signature().name == action.verb {
            builtin.execute(action, time_ms, timeline, diagnostics);
            return;
        }
    }

    push_unknown_action_diagnostic(action, diagnostics);
}

/// Exposes all action signatures for LSP/UI integration.
pub fn get_action_signatures() -> Vec<ActionSignature> {
    get_builtin_actions()
        .iter()
        .map(|a| a.signature())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Action, Modifier};
    use crate::diagnostics::DiagnosticCode;

    #[test]
    fn unknown_actions_emit_diagnostics() {
        let action = Action {
            verb: "spin-in".to_string(),
            targets: vec!["shape".to_string()],
            args: vec![],
            modifiers: vec![Modifier {
                name: None,
                value: crate::ast::Expr::Ident("1s".to_string()),
            }],
        };
        let mut timeline = Timeline::new();
        let mut diagnostics = Vec::new();

        process_action(&action, 0.0, &mut timeline, &mut diagnostics);

        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::UnknownAction));
    }

    #[test]
    fn action_signatures_include_reveal_out() {
        assert!(get_action_signatures()
            .iter()
            .any(|signature| signature.name == "reveal-out"));
    }

    #[test]
    fn action_signatures_include_draw_out() {
        assert!(get_action_signatures()
            .iter()
            .any(|signature| signature.name == "draw-out"));
    }
}
