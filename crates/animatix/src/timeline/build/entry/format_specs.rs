//! Build-time validation of `format()` template specifiers.
//!
//! The evaluator falls back to default formatting for an unsupported
//! placeholder, but silence at build time is what made `format("{:.1f}", x)`
//! look accepted while printing braces. This pass walks every expression the
//! build will evaluate, including `always` assignments, and names the actor
//! and specifier for each one the evaluator cannot parse.

use super::*;
use crate::ast::{ByteSpan, Expr, InlineItem, Property, Stmt};

pub(super) fn validate_format_specs(stmts: &[Stmt], diagnostics: &mut Vec<Diagnostic>) {
    visit_stmts(stmts, None, diagnostics);
}

fn visit_stmts(stmts: &[Stmt], actor: Option<&str>, diagnostics: &mut Vec<Diagnostic>) {
    for stmt in stmts {
        match stmt {
            Stmt::ActorDecl {
                label,
                props,
                modifiers,
                children,
                ..
            } => {
                let current = if label.is_empty() {
                    actor
                } else {
                    Some(label.as_str())
                };
                visit_props(props, current, diagnostics);
                visit_modifiers(modifiers, current, diagnostics);
                visit_inline_items(children, current, diagnostics);
            },
            Stmt::Assignment {
                target,
                property,
                value,
                modifiers,
                value_span,
                ..
            } => {
                let current = target.first().map(|seg| seg.label_str()).or(actor);
                let subject = property_subject(current, property);
                visit_expr(value, current, &subject, *value_span, diagnostics);
                visit_modifiers(modifiers, current, diagnostics);
            },
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                ..
            } => {
                let current = target.first().map(|seg| seg.label_str()).or(actor);
                let subject = property_subject(current, property);
                visit_expr(value, current, &subject, *value_span, diagnostics);
            },
            Stmt::LetDecl { name, value, .. } => {
                visit_expr(value, actor, name, None, diagnostics);
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::Block { body, .. } => visit_stmts(body, actor, diagnostics),
            Stmt::Stagger {
                modifiers, body, ..
            } => {
                visit_modifiers(modifiers, actor, diagnostics);
                visit_stmts(body, actor, diagnostics);
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                visit_expr(condition, actor, actor.unwrap_or("if"), None, diagnostics);
                visit_stmts(then_branch, actor, diagnostics);
                if let Some(else_branch) = else_branch {
                    visit_stmts(else_branch, actor, diagnostics);
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                visit_expr(scrutinee, actor, actor.unwrap_or("match"), None, diagnostics);
                for (_, body) in arms {
                    visit_stmts(body, actor, diagnostics);
                }
            },
            Stmt::ForLoop {
                iterable,
                body,
                modifiers,
                ..
            } => {
                visit_expr(iterable, actor, actor.unwrap_or("for"), None, diagnostics);
                visit_modifiers(modifiers, actor, diagnostics);
                visit_stmts(body, actor, diagnostics);
            },
            Stmt::Action(action, _) => {
                for arg in &action.args {
                    visit_expr(arg, actor, actor.unwrap_or("action"), None, diagnostics);
                }
                for index in action.target_index.iter().flatten() {
                    visit_expr(index, actor, actor.unwrap_or("action"), None, diagnostics);
                }
                visit_modifiers(&action.modifiers, actor, diagnostics);
            },
            Stmt::ComponentDef(def, _) => visit_stmts(&def.body, actor, diagnostics),
            Stmt::FnDecl { body, .. } => visit_stmts(body, actor, diagnostics),
            Stmt::Scene { body, .. } => visit_stmts(body, actor, diagnostics),
            Stmt::Config { settings, .. } => visit_props(settings, actor, diagnostics),
            Stmt::Return {
                value: Some(value), ..
            }
            | Stmt::Expr(value, _) => {
                visit_expr(value, actor, actor.unwrap_or("expression"), None, diagnostics);
            },
            Stmt::Return { value: None, .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Import { .. }
            | Stmt::Play { .. }
            | Stmt::Step { .. }
            | Stmt::Comment(..) => {},
        }
    }
}

fn visit_props(props: &[Property], actor: Option<&str>, diagnostics: &mut Vec<Diagnostic>) {
    for prop in props {
        let subject = property_subject(actor, &prop.name);
        visit_expr(&prop.value, actor, &subject, prop.value_span, diagnostics);
    }
}

fn visit_modifiers(modifiers: &[Modifier], actor: Option<&str>, diagnostics: &mut Vec<Diagnostic>) {
    for modifier in modifiers {
        let subject = actor.map_or("modifier".to_string(), |label| format!("{label}.modifier"));
        visit_expr(&modifier.value, actor, &subject, None, diagnostics);
    }
}

fn visit_inline_items(
    items: &[InlineItem],
    actor: Option<&str>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for item in items {
        match item {
            InlineItem::Anonymous {
                props,
                modifiers,
                children,
                ..
            } => {
                visit_props(props, actor, diagnostics);
                visit_modifiers(modifiers, actor, diagnostics);
                visit_inline_items(children, actor, diagnostics);
            },
            InlineItem::Labeled {
                label,
                props,
                modifiers,
                children,
                ..
            } => {
                visit_props(props, Some(label), diagnostics);
                visit_modifiers(modifiers, Some(label), diagnostics);
                visit_inline_items(children, Some(label), diagnostics);
            },
            InlineItem::ForLoop { iterable, body, .. } => {
                visit_expr(iterable, actor, actor.unwrap_or("for"), None, diagnostics);
                visit_inline_items(body, actor, diagnostics);
            },
            InlineItem::SlotFill { items, .. } => visit_inline_items(items, actor, diagnostics),
            InlineItem::SlotMarker => {},
        }
    }
}

fn property_subject(actor: Option<&str>, property: &str) -> String {
    actor.map_or_else(|| property.to_string(), |label| format!("{label}.{property}"))
}

fn visit_expr(
    expr: &Expr,
    actor: Option<&str>,
    subject: &str,
    span: Option<ByteSpan>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    animatix_syntax::walk::walk_expr(expr, &mut |candidate| {
        let Expr::Call(name, args) = candidate else {
            return;
        };
        if name != "format" {
            return;
        }
        let Some(Expr::Str(template)) = args.first() else {
            return;
        };
        let named = actor.unwrap_or(subject);
        for spec in crate::timeline::eval_shared::unsupported_specifiers(template) {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnsupportedFormatSpecifier,
                    DiagnosticPhase::Build,
                    format!(
                        "`{named}`: unsupported format specifier `{{{spec}}}` in format() on \
                         `{subject}`; supported forms are `{{}}`, `{{:.N}}` and `{{:.Nf}}`. \
                         The value is rendered with default formatting."
                    ),
                )
                .with_subject(named)
                .with_byte_span(span),
            );
        }
    });
}
