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
                    // Record action metadata for GUI timeline visualization
                    let (duration_ms, easing) = parse_action_timing_simple(&action.modifiers);
                    let category = categorize_action(&action.verb);
                    for target in &action.targets {
                        self.action_events.push(
                            ActionEvent {
                                verb: action.verb.clone(),
                                targets: vec![target.clone()],
                                start_time_ms: time_ms as u64,
                                duration_ms,
                                easing,
                                category,
                            },
                        );
                    }

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
                Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. } | Stmt::Comment(..) | Stmt::Import { .. } | Stmt::Use { .. } | Stmt::Config { .. } | Stmt::Scene { .. } | Stmt::Play { .. } | Stmt::ComponentDef(..) | Stmt::ComponentAction { .. } | Stmt::Conditional { .. } => {}
            }
        }
    }
}

/// Parse duration and easing from an action's modifiers without emitting diagnostics.
/// This is called before `process_action` (which calls `parse_timing_modifiers`) to
/// avoid duplicating diagnostic warnings.
fn parse_action_timing_simple(modifiers: &[Modifier]) -> (u64, Easing) {
    let mut duration_ms: u64 = 1000;
    let mut easing = Easing::EaseOut;

    for modifier in modifiers {
        match modifier.name.as_deref() {
            // Named "ease" modifier: [ease: bounce]
            Some("ease") => {
                if let Some(raw) = config_string_value(&modifier.value) {
                    if let Some(parsed_easing) = parse_easing_name(&raw) {
                        easing = parsed_easing;
                    }
                }
            }
            // Named "delay" modifier: only relevant for start_time, not duration
            Some("delay") => {}
            // Named modifiers that aren't timing-related (e.g. "by", "to", "from") — skip
            Some(_) => {}
            // Bare (unnamed) modifiers: [2s] or [500ms]
            None => {
                if let Some(raw) = config_string_value(&modifier.value) {
                    if let Some(d) = parse_duration_literal(&raw) {
                        duration_ms = d as u64;
                    }
                }
            }
        }
    }

    (duration_ms, easing)
}

/// Categorize an action verb for UI color coding.
fn categorize_action(verb: &str) -> ActionCategory {
    match verb {
        "fade-in" | "wipe-in" => ActionCategory::Entrance,
        "fade-out" | "wipe-out" => ActionCategory::Exit,
        "move" | "shift" | "rotate" | "scale" => ActionCategory::Motion,
        "bounce" | "pulse" | "shake" => ActionCategory::Effect,
        "swap" | "reorder" => ActionCategory::Reorder,
        "draw-in" | "reveal-in" | "draw-out" | "reveal-out" => ActionCategory::Reveal,
        _ => ActionCategory::Motion,
    }
}