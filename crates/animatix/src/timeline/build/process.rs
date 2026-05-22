//! Main AST statement processor: dispatches to actor declaration, assignment,
//! sequence, stagger, drive, always, for-loop, and let-decl handlers.

use super::*;
use tracing::instrument;

impl Timeline {
    // === Main AST Statement Processor ===

    #[instrument(skip(self, body, diagnostics, parent_label), fields(time_ms, statements = body.len()))]
    pub(crate) fn process_body(
        &mut self,
        time_ms: f64,
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for stmt in body {
            match stmt {
                Stmt::ActorDecl {
                    is_pub: _,
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                    ..
                } => self.process_actor_decl(
                    label,
                    ty,
                    props,
                    modifiers,
                    children,
                    time_ms,
                    parent_label,
                    diagnostics,
                ),
                Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                    easing,
                    value_span: _,
                    ..
                } => {
                    if target.is_empty() {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::InvalidAssignmentTarget,
                            DiagnosticPhase::Build,
                            format!(
                                "Assignment '{property} = ...' must include an actor label, or be placed inside a 'drive' block",
                            ),
                        ));
                    } else {
                        self.process_assignment_statement(
                            target,
                            property,
                            value,
                            modifiers,
                            *easing,
                            time_ms,
                            diagnostics,
                        );
                    }
                }
                Stmt::Always { body, .. } => {
                    self.modifiers.extend(body.clone());
                }
                Stmt::Drive { label, body, .. } => {
                    let rewritten = self.rewrite_drive_assignments(body, label);
                    self.modifiers.extend(rewritten);
                }
                Stmt::ReactiveBinding {
                    target,
                    property,
                    value,
                    value_span,
                    ..
                } => {
                    self.modifiers.push(Stmt::Assignment {
                        target: target.clone(),
                        property: property.clone(),
                        value: value.clone(),
                        modifiers: vec![],
                        easing: None,
                        value_span: *value_span,
                        span: None,
                    });
                }
                Stmt::ForLoop {
                    var,
                    iterable,
                    body,
                    ..
                } => {
                    for value in for_iter_values(iterable, &self.env) {
                        self.env.set(var, value);
                        self.process_body(time_ms, body, parent_label, diagnostics);
                    }
                }
                Stmt::Sequence { body, .. } => {
                    self.process_sequence(time_ms, body, parent_label, diagnostics);
                }
                Stmt::Stagger { modifiers, body, .. } => {
                    self.process_stagger(time_ms, modifiers, body, parent_label, diagnostics);
                }
                Stmt::Action(action, span) => {
                    process_action(action, time_ms, self, diagnostics, *span);
                }
                Stmt::LetDecl { name, value, .. } => {
                    let eval_env = self.build_eval_env(time_ms as u64);
                    match evaluate_expr(value, &eval_env) {
                        Ok(val) => {
                            self.variable_tracks
                                .entry(name.clone())
                                .or_default()
                                .keyframes
                                .insert(time_ms as u64, val);
                        }
                        Err(e) => {
                            diagnostics.push(
                                Diagnostic::error(
                                    DiagnosticCode::ModuleExportEvalError,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "Failed to evaluate variable '{}': {}; skipping.",
                                        name, e
                                    ),
                                )
                                .with_subject(name),
                            );
                        }
                    }
                }
                _ => {}
            }
        }
    }
}