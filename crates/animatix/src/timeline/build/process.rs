//! Main AST statement processor: dispatches to actor declaration, assignment,
//! sequence, stagger, always, for-loop, and let-decl handlers.

use tracing::instrument;

use super::*;
use crate::ast::{InlineItem, LoopPattern, MatchPattern};

pub(crate) fn bind_loop_var(
    env: &mut Environment,
    var: &LoopPattern,
    value: Value,
    index: usize,
    diagnostics: &mut Vec<Diagnostic>,
) {
    match var {
        LoopPattern::Single(name) => {
            env.set(name, value);
        },
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
                env.set(name, components[i].clone());
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
        },
    }
}

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
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::ReservedLabelPrefix,
                            DiagnosticPhase::Build,
                            format!(
                                "Actor label '{}' uses reserved prefix '__' which is \
                                     reserved for internally generated labels",
                                label
                            ),
                        ));
                    }
                    let resolved_label = resolve_array_index(
                        label,
                        array_index,
                        &self.env,
                        diagnostics,
                        time_ms as u64,
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
                },
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
                            format!("Assignment '{property} = ...' must include an actor label.",),
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
                },
                Stmt::Always { body, .. } => {
                    self.modifiers.extend(body.clone());
                },
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
                },
                Stmt::ForLoop {
                    var,
                    index_var,
                    iterable,
                    body,
                    modifiers,
                    ..
                } => {
                    self.process_for_loop_stmts(
                        var,
                        index_var,
                        iterable,
                        body,
                        modifiers,
                        time_ms,
                        parent_label,
                        diagnostics,
                    );
                },
                Stmt::Sequence { body, .. } => {
                    self.process_sequence(time_ms, body, parent_label, diagnostics);
                },
                Stmt::Stagger {
                    modifiers, body, ..
                } => {
                    self.process_stagger(time_ms, modifiers, body, parent_label, diagnostics);
                },
                Stmt::Action(action, span) => {
                    // Resolve `name[expr]` targets against the build
                    // environment (loop variables, `let` bindings) before
                    // dispatch, e.g. `swap bars[j], bars[j+1]` -> concrete keys.
                    let needs_resolution = action.target_index.iter().any(Option::is_some);
                    let resolved_action = if needs_resolution {
                        let mut resolved = action.clone();
                        resolved.targets = action
                            .targets
                            .iter()
                            .zip(action.target_index.iter())
                            .map(|(target, index)| {
                                resolve_action_target_index(
                                    target,
                                    index.as_ref(),
                                    &self.env,
                                    diagnostics,
                                    time_ms as u64,
                                )
                            })
                            .collect();
                        resolved.target_index = vec![None; resolved.targets.len()];
                        resolved
                    } else {
                        action.clone()
                    };
                    let action = &resolved_action;

                    // Record action metadata for GUI timeline visualization
                    let (duration_ms, easing) = parse_action_timing_simple(&action.modifiers);
                    let category = categorize_action(&action.verb);
                    for target in &action.targets {
                        self.action_events.push(ActionEvent {
                            verb: action.verb.clone(),
                            targets: vec![target.clone()],
                            start_time_ms: time_ms as u64,
                            duration_ms,
                            easing,
                            category,
                        });
                    }

                    let extensions = self.extensions.clone();
                    process_action_with_extensions(
                        action,
                        time_ms,
                        self,
                        diagnostics,
                        *span,
                        extensions.as_deref(),
                    );
                },
                Stmt::LetDecl { name, value, .. } => {
                    // G5/G6 guard: Anchor-point refs (`n0.right`) are
                    // frame-time-resolved and cannot be used in build-time
                    // `let` constants (transforms/bounds not resolved at build).
                    if let crate::ast::Expr::Path(segments) = value
                        && segments.len() == 2
                        && SceneAnchor::from_str(&segments[1]).is_some()
                    {
                        let msg = format!(
                            "'{}' references '{}' which is a frame-time anchor-point property; \
                             cannot resolve at build time. Use in 'always' or assignment instead.",
                            name,
                            segments.join(".")
                        );
                        diagnostics.push(Diagnostic::warning(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            msg,
                        ));
                        // Fall through to evaluate (will likely fail or produce (0,0)),
                        // which gives the user a second diagnostic for the eval failure.
                    }
                    let eval_env = self.build_eval_env(time_ms as u64);
                    match evaluate_expr(value, &eval_env) {
                        Ok(val) => {
                            if let Some(locals) = self.block_scope.last_mut() {
                                // Function-local binding: visible within the
                                // block, removed on exit, and never written to
                                // the scene's variable tracks.
                                locals.insert(name.clone());
                                self.env.set(name, val);
                            } else {
                                self.variable_tracks
                                    .entry(name.clone())
                                    .or_default()
                                    .keyframes
                                    .insert(time_ms as u64, val.clone());
                                // Make the shadowed value visible to subsequent statements
                                // in the same build pass (algorithm precomputation).
                                self.env.set(name, val);
                            }
                        },
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
                        },
                    }
                },
                Stmt::Match {
                    scrutinee, arms, ..
                } => {
                    // Evaluate the scrutinee at build time
                    let eval_env = self.build_eval_env(time_ms as u64);
                    let value = match evaluate_expr(scrutinee, &eval_env) {
                        Ok(v) => v,
                        Err(e) => {
                            diagnostics.push(Diagnostic::error(
                                DiagnosticCode::ModuleExportEvalError,
                                DiagnosticPhase::Build,
                                format!(
                                    "Failed to evaluate match scrutinee: {}; skipping match.",
                                    e
                                ),
                            ));
                            continue;
                        },
                    };
                    // Find the first matching arm and process its body
                    let mut matched = false;
                    for (pat, body) in arms {
                        if pattern_matches(pat, &value) {
                            self.process_body(time_ms, body, parent_label, diagnostics);
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        // Check if there's a wildcard arm (should be, but emit diagnostic if not)
                        let has_wildcard =
                            arms.iter().any(|(pat, _)| matches!(pat, MatchPattern::Wildcard));
                        if !has_wildcard {
                            diagnostics.push(
                                Diagnostic::warning(
                                    DiagnosticCode::InvalidPropertyValue,
                                    DiagnosticPhase::Build,
                                    format!(
                                        "match scrutinee evaluated to {:?} but no arm matched and no `_` wildcard arm was provided",
                                        value
                                    ),
                                )
                            );
                        }
                        // If wildcard exists and no arm matched, the wildcard would have caught it
                        // already. If no wildcard, we already warned; fall
                        // through silently.
                    }
                },
                Stmt::Conditional {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    let eval_env = self.build_eval_env(time_ms as u64);
                    let truthy = evaluate_expr(condition, &eval_env)
                        .map(|value| value.is_truthy())
                        .unwrap_or(false);
                    if truthy {
                        self.process_body(time_ms, then_branch, parent_label, diagnostics);
                    } else if let Some(else_branch) = else_branch {
                        self.process_body(time_ms, else_branch, parent_label, diagnostics);
                    }
                },
                Stmt::Keyframe { .. }
                | Stmt::RelativeKeyframe { .. }
                | Stmt::Comment(..)
                | Stmt::Import { .. }
                | Stmt::TypeAlias { .. }
                | Stmt::Config { .. }
                | Stmt::Scene { .. }
                | Stmt::Play { .. }
                | Stmt::ComponentDef(..)
                | Stmt::FnDecl { .. } => {},
                Stmt::Block { body, .. } => {
                    // Function-call expansion scope: `let` bindings inside
                    // stay local and are removed when the block exits.
                    self.block_scope.push(std::collections::HashSet::new());
                    self.process_body(time_ms, body, parent_label, diagnostics);
                    if let Some(locals) = self.block_scope.pop() {
                        for name in locals {
                            self.env.overrides.remove(&name);
                            self.env.mark_mutated();
                        }
                    }
                },
                Stmt::Return { value, span, .. } => {
                    let _ = (value, span);
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        "'return' is only valid inside a pure function body".to_string(),
                    ));
                },
                Stmt::Expr(..) => {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        "bare expressions are only valid as the tail of a pure function body"
                            .to_string(),
                    ));
                },
            }
        }
    }

    // ─────────────────────────────────────────────────────────────
    // For-loop lowering helpers
    // ─────────────────────────────────────────────────────────────

    /// Lower a for-loop by iterating values, binding the loop variable (and optional index),
    /// and calling the body processor for each iteration.
    /// After the loop, loop variables are cleaned up from the environment
    /// to prevent leaks (closures already captured them at creation time).
    ///
    /// A `[step: 250ms]` modifier advances the build-time clock by 250ms per
    /// iteration, so events emitted by the body land on distinct times
    /// (algorithm visualizations precompute a sequenced event list this way).
    pub(super) fn process_for_loop_stmts(
        &mut self,
        var: &LoopPattern,
        index_var: &Option<String>,
        iterable: &Expr,
        body: &[Stmt],
        modifiers: &[Modifier],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let step_ms = parse_for_loop_step(modifiers, diagnostics);
        for (idx, value) in for_iter_values(iterable, &self.env).into_iter().enumerate() {
            bind_loop_var(&mut self.env, var, value, idx, diagnostics);
            if let Some(iv) = index_var {
                self.env.set(iv, Value::Num(idx as f64));
            }
            self.process_body(time_ms + step_ms * idx as f64, body, parent_label, diagnostics);
        }
        // Clean up loop variables after the loop exits
        remove_loop_vars(&mut self.env, var, index_var);
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
            bind_loop_var(&mut self.env, var, value, idx, diagnostics);
            if let Some(iv) = index_var {
                self.env.set(iv, Value::Num(idx as f64));
            }
            self.process_inline_items(time_ms, body, parent_label, diagnostics);
        }
        // Clean up loop variables after the loop exits
        remove_loop_vars(&mut self.env, var, index_var);
    }
}

/// Remove loop variables from the environment after the loop exits.
/// Closures captured them at creation time (#11/#10), so it's safe to clean up.
pub(crate) fn remove_loop_vars(
    env: &mut Environment,
    var: &LoopPattern,
    index_var: &Option<String>,
) {
    match var {
        LoopPattern::Single(name) => {
            env.overrides.remove(name);
            env.mark_mutated();
        },
        LoopPattern::Tuple(names) => {
            for name in names {
                env.overrides.remove(name);
                env.mark_mutated();
            }
        },
    }
    if let Some(iv) = index_var {
        env.overrides.remove(iv);
        env.mark_mutated();
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
            },
            // Named "delay" modifier: only relevant for start_time, not duration
            Some("delay") => {},
            // Named modifiers that aren't timing-related (e.g. "by", "to", "from") — skip
            Some(_) => {},
            // Bare (unnamed) modifiers: [2s] or [500ms]
            None => {
                if let Some(raw) = config_string_value(&modifier.value) {
                    if let Some(d) = parse_duration_literal(&raw) {
                        duration_ms = d as u64;
                    }
                }
            },
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

/// Parse the `step` modifier from a for-loop header; defaults to 0ms.
///
/// `for i in {0, 1, 2} [step: 250ms] { ... }` advances the build-time clock by
/// 250ms per iteration. Invalid or negative values emit a diagnostic and fall
/// back to 0ms (no time advancement).
fn parse_for_loop_step(modifiers: &[Modifier], diagnostics: &mut Vec<Diagnostic>) -> f64 {
    for modifier in modifiers {
        if modifier.name.as_deref() == Some("step") {
            let parsed = match &modifier.value {
                Expr::Ident(raw) | Expr::Str(raw) => parse_duration_literal(raw),
                _ => None,
            };
            return match parsed {
                Some(ms) if ms >= 0.0 => ms,
                Some(ms) => {
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::InvalidModifierValue,
                        DiagnosticPhase::Build,
                        format!("For-loop 'step' must be non-negative, got {ms}ms; ignoring."),
                    ));
                    0.0
                },
                None => {
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::InvalidModifierValue,
                        DiagnosticPhase::Build,
                        "For-loop 'step' expects a time literal such as 250ms or 1s; ignoring."
                            .to_string(),
                    ));
                    0.0
                },
            };
        }
    }
    0.0
}

/// Resolve a `name[expr]` action target against the build environment.
///
/// Plain targets pass through unchanged. For an indexed target the index
/// expression is evaluated (loop variables and `let` bindings are in scope at
/// build time) and the last path segment is replaced with `base__N`, matching
/// `resolve_array_index`. An unresolved or invalid index emits a diagnostic
/// and leaves the base name in place so the normal unknown-target path
/// reports it.
pub(crate) fn resolve_action_target_index(
    target: &str,
    index: Option<&Expr>,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    time_ms: u64,
) -> String {
    let Some(index_expr) = index else {
        return target.to_string();
    };
    let eval_env = build_eval_env_static(env, time_ms);
    match evaluate_expr(index_expr, &eval_env) {
        Ok(Value::Num(n)) if n >= 0.0 && n == n.floor() => {
            let (prefix, base) = target.rsplit_once('.').unwrap_or(("", target));
            let resolved = crate::ast::array_actor_label(base, n as usize);
            if prefix.is_empty() {
                resolved
            } else {
                format!("{prefix}.{resolved}")
            }
        },
        Ok(Value::Num(n)) => {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!(
                    "Action target index for '{target}' must be a non-negative integer, got {n}"
                ),
            ));
            target.to_string()
        },
        _ => {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!("Failed to evaluate action target index for '{target}' at build time"),
            ));
            target.to_string()
        },
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
                    crate::ast::array_actor_label(label, n as usize)
                },
                Ok(Value::Num(n)) => {
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!(
                            "Array index for '{}' must be a non-negative integer, got {}",
                            label, n
                        ),
                    ));
                    label.to_string()
                },
                _ => {
                    diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!("Array index for '{}' must evaluate to a number", label),
                    ));
                    label.to_string()
                },
            }
        },
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

/// Check whether a `MatchPattern` matches a `Value`.
/// Used by both build-time `Stmt::Match` dispatch and frame-time `Expr::Match` evaluation.
pub(crate) fn pattern_matches(pat: &MatchPattern, value: &Value) -> bool {
    match pat {
        MatchPattern::Wildcard => true,
        MatchPattern::Num(n) => {
            matches!(value, Value::Num(v) if (*v - *n).abs() < f64::EPSILON)
        },
        MatchPattern::Str(s) => {
            matches!(value, Value::Str(v) if v == s)
        },
        MatchPattern::Bool(b) => {
            matches!(value, Value::Bool(v) if v == b)
        },
        MatchPattern::Range(lo, hi) => {
            // Both endpoints must be numeric
            let lo_val = match lo.as_ref() {
                MatchPattern::Num(n) => *n,
                _ => return false,
            };
            let hi_val = match hi.as_ref() {
                MatchPattern::Num(n) => *n,
                _ => return false,
            };
            matches!(value, Value::Num(v) if *v >= lo_val && *v <= hi_val)
        },
        MatchPattern::Or(pats) => pats.iter().any(|p| pattern_matches(p, value)),
        MatchPattern::Tuple(pats) => match value {
            Value::List(items) => {
                if items.len() != pats.len() {
                    return false;
                }
                pats.iter().zip(items.iter()).all(|(p, v)| pattern_matches(p, v))
            },
            Value::Vec2(arr) if pats.len() == 2 => {
                pats.iter().zip(arr.iter()).all(|(p, v)| pattern_matches(p, &Value::Num(*v)))
            },
            Value::Vec3(arr) if pats.len() == 3 => {
                pats.iter().zip(arr.iter()).all(|(p, v)| pattern_matches(p, &Value::Num(*v)))
            },
            Value::Vec4(arr) if pats.len() == 4 => {
                pats.iter().zip(arr.iter()).all(|(p, v)| pattern_matches(p, &Value::Num(*v)))
            },
            _ => false,
        },
    }
}
