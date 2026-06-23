//! Main AST statement processor: dispatches to actor declaration, assignment,
//! sequence, stagger, always, for-loop, and let-decl handlers.

use super::*;
use crate::ast::{InlineItem, LoopPattern};
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
                    array_index,
                    ty,
                    props,
                    modifiers,
                    children,
                    ..
                } => {
                    // Validate that user labels don't use the reserved `__` prefix
                    if label.starts_with("__") {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::ReservedLabelPrefix,
                                DiagnosticPhase::Build,
                                format!(
                                    "Actor label '{}' uses reserved prefix '__' which is 
                                     reserved for internally generated labels",
                                    label
                                ),
                            )
                        );
                    }
                    let resolved_label = resolve_array_index(
                        label, array_index, &self.env, diagnostics, time_ms as u64,
                    );
                    self.process_actor_decl(
                        &resolved_label,
                        ty,
                        props,
                        modifiers,
                        children,
                        time_ms,
                        parent_label,
                        diagnostics,
                    );
                }
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
                                "Assignment '{property} = ...' must include an actor label.",
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
                    index_var,
                    iterable,
                    body,
                    ..
                } => {
                    self.process_for_loop_stmts(var, index_var, iterable, body, time_ms, parent_label, diagnostics);
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
                Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. } | Stmt::Comment(..) | Stmt::Import { .. } | Stmt::Config { .. } | Stmt::Scene { .. } | Stmt::Play { .. } | Stmt::ComponentDef(..) | Stmt::ComponentAction { .. } | Stmt::Conditional { .. } => {}
            }
        }
    }

    // ─────────────────────────────────────────────────────────────
    // For-loop lowering helpers
    // ─────────────────────────────────────────────────────────────

    /// Bind a loop iteration value according to the loop variable pattern.
    fn bind_loop_var(&mut self, var: &LoopPattern, value: Value, index: usize, diagnostics: &mut Vec<Diagnostic>) {
        match var {
            LoopPattern::Single(name) => {
                self.env.set(name, value);
            }
            LoopPattern::Tuple(names) => {
                let components: Vec<Value> = match &value {
                    Value::List(items) => items.clone(),
                    Value::Vec2(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                    Value::Vec3(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                    Value::Vec4(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                    Value::Color(v) => v.iter().map(|&x| Value::Num(x)).collect(),
                    other => vec![other.clone()],
                };
                let min_len = names.len().min(components.len());
                for (i, name) in names.iter().enumerate().take(min_len) {
                    self.env.set(name, components[i].clone());
                }
                if names.len() != components.len() {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!(
                                "For loop tuple destructuring: expected {} variables but got {} components in value at index {}",
                                names.len(), components.len(), index
                            ),
                        )
                    );
                }
            }
        }
    }

    /// Lower a for-loop by iterating values, binding the loop variable (and optional index),
    /// and calling the body processor for each iteration.
    pub(super) fn process_for_loop_stmts(
        &mut self,
        var: &LoopPattern,
        index_var: &Option<String>,
        iterable: &Expr,
        body: &[Stmt],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for (idx, value) in for_iter_values(iterable, &self.env).into_iter().enumerate() {
            self.bind_loop_var(var, value, idx, diagnostics);
            if let Some(iv) = index_var {
                self.env.set(iv, Value::Num(idx as f64));
            }
            self.process_body(time_ms, body, parent_label, diagnostics);
        }
    }

    /// Same as process_for_loop_stmts but for InlineItem bodies.
    pub(super) fn process_for_loop_inline_items(
        &mut self,
        var: &LoopPattern,
        index_var: &Option<String>,
        iterable: &Expr,
        body: &[InlineItem],
        time_ms: f64,
        parent_label: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for (idx, value) in for_iter_values(iterable, &self.env).into_iter().enumerate() {
            self.bind_loop_var(var, value, idx, diagnostics);
            if let Some(iv) = index_var {
                self.env.set(iv, Value::Num(idx as f64));
            }
            self.process_inline_items(time_ms, body, parent_label, diagnostics);
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

/// Resolve an array-indexed actor label to a concrete timeline label.
///
/// For a normal actor (`array_index: None`), returns the label as-is.
/// For an array actor (`array_index: Some(expr)`), evaluates the index
/// expression and produces `{array_name}__{index}`.
///
/// Example: `bars` with `array_index: Some(Num(0))` → `"bars__0"`
pub(crate) fn resolve_array_index(
    label: &str,
    array_index: &Option<Expr>,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    time_ms: u64,
) -> String {
    match array_index {
        Some(index_expr) => {
            let eval_env = build_eval_env_static(env, time_ms);
            match evaluate_expr(index_expr, &eval_env) {
                Ok(Value::Num(n)) if n >= 0.0 && n == n.floor() => {
                    format!("{}__{}", label, n as usize)
                }
                Ok(Value::Num(n)) => {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!(
                                "Array index for '{}' must be a non-negative integer, got {}",
                                label, n
                            ),
                        )
                    );
                    label.to_string()
                }
                _ => {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            format!(
                                "Array index for '{}' must evaluate to a number",
                                label
                            ),
                        )
                    );
                    label.to_string()
                }
            }
        }
        None => label.to_string(),
    }
}

/// Build an evaluation environment from an existing env + time,
/// for use in resolving array indices outside a full Timeline context.
fn build_eval_env_static(env: &Environment, time_ms: u64) -> Environment {
    let mut eval_env = env.clone();
    eval_env.set("t", Value::Num(time_ms as f64 / 1000.0));
    eval_env
}