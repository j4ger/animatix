//! Main AST statement processor: dispatches to actor declaration, assignment,
//! sequence, stagger, always, for-loop, and let-decl handlers.

use std::borrow::Cow;

use tracing::instrument;

use super::*;
use crate::ast::{Action, InlineItem, LoopPattern, MatchPattern, Property, Span};

mod indexed_component;
mod loop_modifiers;
mod scoped_instance;

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
                Value::List(items) => items.to_vec(),
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
                    is_anonymous,
                    span,
                } => self.process_actor_decl_stmt(
                    label,
                    *is_anonymous,
                    array_index,
                    ty,
                    props,
                    modifiers,
                    children,
                    time_ms,
                    parent_label,
                    diagnostics,
                    *span,
                ),
                Stmt::Assignment {
                    target,
                    property,
                    value,
                    modifiers,
                    easing,
                    span,
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
                            *span,
                            diagnostics,
                        );
                    }
                },
                Stmt::Always { body, .. } => {
                    self.collect_scoped_modifiers(body.clone(), diagnostics);
                },
                Stmt::ReactiveBinding {
                    target,
                    property,
                    value,
                    value_span,
                    ..
                } => {
                    self.collect_scoped_modifiers(
                        vec![Stmt::Assignment {
                            target: target.clone(),
                            property: property.clone(),
                            value: value.clone(),
                            modifiers: vec![],
                            easing: None,
                            value_span: *value_span,
                            span: None,
                        }],
                        diagnostics,
                    );
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
                    self.process_action_stmt(action, *span, time_ms, diagnostics)
                },
                Stmt::LetDecl { name, value, .. } => {
                    self.process_let_decl_stmt(name, value, time_ms, diagnostics)
                },
                Stmt::Match {
                    scrutinee, arms, ..
                } => self.process_match_stmt(scrutinee, arms, time_ms, parent_label, diagnostics),
                Stmt::Conditional {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    self.process_conditional(
                        time_ms,
                        condition,
                        then_branch,
                        else_branch.as_deref(),
                        parent_label,
                        diagnostics,
                    );
                },
                Stmt::Step { .. } => {
                    let t = time_ms as u64;
                    if !self.pause_points.contains(&t) {
                        self.pause_points.push(t);
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
                Stmt::Block {
                    body,
                    span,
                    instance_scope,
                } => self.process_block_stmt(
                    body,
                    instance_scope.as_ref(),
                    *span,
                    time_ms,
                    parent_label,
                    diagnostics,
                    false,
                ),
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

    /// Lower one actor declaration: validate the label, resolve an array
    /// index, and dispatch to the actor-declaration pipeline.
    #[allow(clippy::too_many_arguments)] // one argument per authored decl field
    fn process_actor_decl_stmt(
        &mut self,
        label: &str,
        is_anonymous: bool,
        array_index: &Option<Expr>,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        span: Option<Span>,
    ) {
        // Validate that user labels don't use the reserved `__` prefix.
        // Anonymous actors carry engine-generated `__anon_*` labels —
        // the generator's own output must not trip this check (it used
        // to make unlabeled Graph children unbuildable).
        if label.starts_with("__") && !is_anonymous {
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
        // `camera` is the reserved target for the scene camera
        // (`timeline::camera`), so an actor with that label could be
        // drawn but never addressed: every `camera.<prop> = …` write
        // goes to the camera.
        if label == crate::timeline::camera::CAMERA_TARGET {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::ReservedLabelPrefix,
                DiagnosticPhase::Build,
                format!(
                    "Actor label '{label}' is reserved: `camera.at` / `camera.zoom` \
                         / `camera.rotation` address the scene camera, so this actor \
                         could be drawn but never assigned to"
                ),
            ));
        }
        let qualified_label = parent_label
            .map(|parent| indexed_component::qualify_child(label, parent))
            .unwrap_or_else(|| label.to_string());
        let qualified_props = match parent_label.and_then(indexed_component::alias) {
            Some((source_prefix, concrete_prefix)) => {
                Cow::Owned(indexed_component::specialize_property_paths(
                    props,
                    &source_prefix,
                    &concrete_prefix,
                ))
            },
            None => Cow::Borrowed(props),
        };
        let resolved_label = resolve_array_index(
            &qualified_label,
            array_index,
            &self.env,
            diagnostics,
            time_ms as u64,
        );
        self.process_actor_decl(
            &resolved_label,
            ty,
            qualified_props.as_ref(),
            modifiers,
            children,
            time_ms,
            parent_label,
            diagnostics,
            span,
        );
        // Every actor family passes through here with its props still
        // in hand, including the ones whose `process_*_decl` returns
        // before the generic property walk. See
        // `Timeline::apply_camera_follow_decl`.
        self.apply_camera_follow_decl(&resolved_label, qualified_props.as_ref(), diagnostics);
    }

    /// Lower one action statement: resolve `name[expr]` targets against the
    /// build environment, record metadata for the GUI, and dispatch.
    fn process_action_stmt(
        &mut self,
        action: &Action,
        span: Option<Span>,
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
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
                .enumerate()
                .map(|(target_index, (target, index))| {
                    resolve_action_target_index(
                        target,
                        index.as_ref(),
                        action.target_index_position.get(target_index).copied().flatten(),
                        &self.env,
                        diagnostics,
                        time_ms as u64,
                        span,
                    )
                })
                .collect();
            resolved.target_index = vec![None; resolved.targets.len()];
            resolved.target_index_position = vec![None; resolved.targets.len()];
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
            span,
            extensions.as_deref(),
        );
    }

    /// Lower one `let` declaration: a `--set` override, then a build-time
    /// evaluated binding, either block-local or a scene variable track.
    fn process_let_decl_stmt(
        &mut self,
        name: &str,
        value: &Expr,
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // A `--set NAME=…` override wins over the file's own
        // `let NAME = …`: the authored value is the template's
        // default, and the pre-walk injection already seeded the
        // override. Skipping the assignment is what makes the
        // override reach properties resolved during this walk.
        if self.cli_defines.contains(name) {
            tracing::debug!("`let {name}` is overridden by --set; the authored default is ignored");
            return;
        }
        // Anchor-point refs (`n0.right`) are frame-time-resolved and cannot be
        // used in build-time `let` constants (transforms/bounds not resolved at build).
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
                    // block, restored on exit, and never written to
                    // the scene's variable tracks.
                    locals
                        .entry(name.to_string())
                        .or_insert_with(|| self.env.overrides.get(name).cloned());
                    self.env.set(name, val);
                } else {
                    self.variable_tracks
                        .entry(name.to_string())
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
                        format!("Failed to evaluate variable '{}': {}; skipping.", name, e),
                    )
                    .with_subject(name),
                );
            },
        }
    }

    /// Lower one build-time `match`: evaluate the scrutinee, then process the
    /// body of the first arm that matches.
    fn process_match_stmt(
        &mut self,
        scrutinee: &Expr,
        arms: &[(MatchPattern, Vec<Stmt>)],
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Evaluate the scrutinee at build time
        let eval_env = self.build_eval_env(time_ms as u64);
        let value = match evaluate_expr(scrutinee, &eval_env) {
            Ok(v) => v,
            Err(e) => {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::ModuleExportEvalError,
                    DiagnosticPhase::Build,
                    format!("Failed to evaluate match scrutinee: {}; skipping match.", e),
                ));
                return;
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
            let has_wildcard = arms.iter().any(|(pat, _)| matches!(pat, MatchPattern::Wildcard));
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
    }

    /// Process a build-time `if` statement. The condition is evaluated once
    /// against the build environment; only the selected branch's statements
    /// are lowered at this time.
    fn process_conditional(
        &mut self,
        time_ms: f64,
        condition: &Expr,
        then_branch: &[Stmt],
        else_branch: Option<&[Stmt]>,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let eval_env = self.build_eval_env(time_ms as u64);
        match evaluate_expr(condition, &eval_env) {
            Ok(value) if value.is_truthy() => {
                self.process_body(time_ms, then_branch, parent_label, diagnostics);
            },
            Ok(_) => {
                if let Some(else_branch) = else_branch {
                    self.process_body(time_ms, else_branch, parent_label, diagnostics);
                }
            },
            Err(e) => {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("Failed to evaluate `if` condition: {e}; skipping the block."),
                ));
            },
        }
    }

    // For-loop lowering helpers

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
        let scope = crate::timeline::env::LoopScope::capture(&self.env, var, index_var.as_deref());
        self.build_loop_scopes.push(scope.names().map(str::to_string).collect());
        for (idx, value) in for_iter_values(iterable, &self.env).into_iter().enumerate() {
            bind_loop_var(&mut self.env, var, value, idx, diagnostics);
            if let Some(iv) = index_var {
                self.env.set(iv, Value::Num(idx as f64));
            }
            self.process_body(time_ms + step_ms * idx as f64, body, parent_label, diagnostics);
        }
        self.build_loop_scopes.pop();
        scope.restore(&mut self.env);
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
        let scope = crate::timeline::env::LoopScope::capture(&self.env, var, index_var.as_deref());
        self.build_loop_scopes.push(scope.names().map(str::to_string).collect());
        for (idx, value) in for_iter_values(iterable, &self.env).into_iter().enumerate() {
            bind_loop_var(&mut self.env, var, value, idx, diagnostics);
            if let Some(iv) = index_var {
                self.env.set(iv, Value::Num(idx as f64));
            }
            self.process_inline_items(time_ms, body, parent_label, diagnostics);
        }
        self.build_loop_scopes.pop();
        scope.restore(&mut self.env);
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
                if let Some(raw) = config_string_value(&modifier.value)
                    && let Some(parsed_easing) = parse_easing_name(&raw)
                {
                    easing = parsed_easing;
                }
            },
            // Named "delay" modifier: only relevant for start_time, not duration
            Some("delay") => {},
            // Named modifiers that aren't timing-related (e.g. "by", "to", "from") — skip
            Some(_) => {},
            // Bare (unnamed) modifiers: [2s] or [500ms]
            None => {
                if let Some(raw) = config_string_value(&modifier.value)
                    && let Some(d) = parse_duration_literal(&raw)
                {
                    duration_ms = d as u64;
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
/// build time). The typed position selects which dotted-path segment receives
/// the concrete array suffix. An unresolved or invalid index emits a diagnostic
/// and leaves the base name in place so the normal unknown-target path
/// reports it.
pub(crate) fn resolve_action_target_index(
    target: &str,
    index: Option<&Expr>,
    index_position: Option<usize>,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    time_ms: u64,
    span: Option<crate::ast::Span>,
) -> String {
    let Some(index_expr) = index else {
        return target.to_string();
    };
    let eval_env = build_eval_env_static(env, time_ms);
    match evaluate_expr(index_expr, &eval_env) {
        Ok(Value::Num(n)) if n >= 0.0 && n == n.floor() => {
            let mut segments: Vec<String> = target.split('.').map(str::to_string).collect();
            let position = index_position.unwrap_or_else(|| segments.len().saturating_sub(1));
            if let Some(segment) = segments.get_mut(position) {
                *segment = crate::ast::array_actor_label(segment, n as usize);
                segments.join(".")
            } else {
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Action target index position {} is outside '{}'; leaving the target unchanged",
                        position, target
                    ),
                ).with_ast_span(span));
                target.to_string()
            }
        },
        Ok(Value::Num(n)) => {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Action target index for '{target}' must be a non-negative integer, got {n}"
                    ),
                )
                .with_ast_span(span),
            );
            target.to_string()
        },
        _ => {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("Failed to evaluate action target index for '{target}' at build time"),
                )
                .with_ast_span(span),
            );
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

#[cfg(test)]
mod indexed_component_label_tests {
    use super::{indexed_component::qualify_child, resolve_action_target_index};
    use crate::ast::{Expr, Span};
    use crate::timeline::Environment;

    #[test]
    fn component_children_receive_their_resolved_array_parent_identity() {
        assert_eq!(qualify_child("cards.frame", "cards__2"), "cards__2.frame");
        assert_eq!(qualify_child("cards.frame.icon", "cards__2.frame"), "cards__2.frame.icon");
        assert_eq!(qualify_child("cards.title", "cards__2.root"), "cards__2.title");
        assert_eq!(qualify_child("layout.cards.frame", "layout.cards__2"), "layout.cards__2.frame");
        assert_eq!(qualify_child("other.frame", "cards__2"), "other.frame");
    }

    #[test]
    fn invalid_index_diagnostic_keeps_the_action_call_site() {
        let mut diagnostics = Vec::new();
        let span = Span::new(12, 3, 12, 18);
        let target = resolve_action_target_index(
            "cards",
            Some(&Expr::Num(1.5)),
            Some(0),
            &Environment::new(),
            &mut diagnostics,
            0,
            Some(span),
        );
        assert_eq!(target, "cards");
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].location.line, Some(12));
    }
}
