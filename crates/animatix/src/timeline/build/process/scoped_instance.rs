//! Build-time lexical and instance scopes for expanded timeline functions.

use super::time_scope::{
    TimeScope, contains_nested_keyframe, local_keyframe_time_ms, resolve_scope, retime_statement,
    statement_extent_ms,
};
use super::*;
use crate::ast::{InstanceScope, TimeScopeSpec};

impl Timeline {
    pub(in crate::timeline) fn process_block_stmt(
        &mut self,
        body: &[Stmt],
        instance_scope: Option<&InstanceScope>,
        time_scope: Option<&TimeScopeSpec>,
        span: Option<Span>,
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        sequenced: bool,
    ) {
        // Scope options are evaluated before generated function parameters
        // enter the callee's lexical environment.
        let resolved_scope = resolve_scope(self, time_scope, body, time_ms, sequenced, diagnostics);
        if (time_scope.is_some() || contains_nested_keyframe(body)) && resolved_scope.is_none() {
            // Invalid local-time arguments invalidate this invocation instead
            // of silently falling back to scene-time execution.
            return;
        }
        if let Some(end_ms) = resolved_scope.and_then(|scope| scope.end_ms)
            && end_ms.is_finite()
            && end_ms > 0.0
        {
            self.local_time_scope_end_ms = self.local_time_scope_end_ms.max(end_ms.ceil() as u64);
        }
        self.block_scope.push(std::collections::HashMap::new());
        self.process_instance_scope(
            body,
            instance_scope,
            span,
            time_ms,
            parent_label,
            diagnostics,
            sequenced,
            resolved_scope,
        );
        if let Some(locals) = self.block_scope.pop() {
            for (name, previous) in locals {
                if let Some(value) = previous {
                    self.env.set(&name, value);
                } else {
                    self.env.overrides.remove(&name);
                    self.env.mark_mutated();
                }
            }
        }
    }

    fn process_instance_scope(
        &mut self,
        body: &[Stmt],
        instance_scope: Option<&InstanceScope>,
        span: Option<Span>,
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        sequenced: bool,
        resolved_scope: Option<TimeScope>,
    ) {
        let Some(scope) = instance_scope else {
            self.process_function_body(
                time_ms,
                body,
                parent_label,
                diagnostics,
                sequenced,
                resolved_scope,
            );
            return;
        };
        let mut parts: Vec<String> = scope.label.split('.').map(str::to_owned).collect();
        if scope.prologue_len > body.len() || scope.index_position >= parts.len() {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    "Malformed indexed component method scope; skipping its body.",
                )
                .with_ast_span(span),
            );
            return;
        }
        let (prologue, statements) = body.split_at(scope.prologue_len);
        // Evaluate the call index and arguments once in the caller's scope,
        // before any method-local variable can shadow them.
        self.process_body(time_ms, prologue, parent_label, diagnostics);
        // The compiler points at the captured local, so rebuilding every
        // actor's property environment here would be redundant.
        let index = match evaluate_expr(&scope.index, &self.env) {
            Ok(Value::Num(index))
                if index.is_finite() && index >= 0.0 && index == index.floor() =>
            {
                index as usize
            },
            result => {
                diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("Component method target index must be a non-negative integer, got {result:?}; skipping its body."),
                ).with_ast_span(span));
                return;
            },
        };
        parts[scope.index_position] =
            crate::ast::array_actor_label(&parts[scope.index_position], index);
        let concrete_root = parts.join(".");
        let bound_names: Vec<String> = prologue
            .iter()
            .filter_map(|statement| match statement {
                Stmt::LetDecl { name, .. } => Some(name.clone()),
                _ => None, // Only generated argument bindings are part of a method prologue.
            })
            .collect();
        // This normalization precedes compile_expr/lower_modifier_block.
        // Frame evaluation receives concrete actor paths, with no label-keyed
        // runtime fallback or per-entity side table.
        let specialized = animatix_syntax::module::specialize_instance_property_reads(
            statements.to_vec(),
            &scope.label,
            &concrete_root,
            &bound_names,
        );
        self.process_function_body(
            time_ms,
            &specialized,
            parent_label,
            diagnostics,
            sequenced,
            resolved_scope,
        );
    }

    fn process_function_body(
        &mut self,
        time_ms: f64,
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        sequenced: bool,
        time_scope: Option<TimeScope>,
    ) {
        if let Some(scope) = time_scope {
            self.process_time_scoped_body(body, scope, parent_label, diagnostics, sequenced);
            return;
        }
        if !sequenced {
            self.process_body(time_ms, body, parent_label, diagnostics);
            return;
        }
        // Preserve timeline-function timing inside a surrounding sequence,
        // while retaining one lexical scope for its entire invocation.
        let mut cursor = time_ms;
        for statement in body {
            let duration = self.sequence_statement_span_ms(statement).unwrap_or(0.0);
            self.process_body(cursor, std::slice::from_ref(statement), parent_label, diagnostics);
            cursor += duration;
        }
    }

    fn process_time_scoped_body(
        &mut self,
        body: &[Stmt],
        scope: TimeScope,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        sequenced: bool,
    ) {
        let mut local_cursor_ms = 0.0;
        let mut lexical_t_shadowed = false;
        for stmt in body {
            match stmt {
                Stmt::Keyframe { time, body, span } => {
                    let Some(time_ms) = local_keyframe_time_ms(time, self.beat_duration_ms())
                    else {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            "Speech-anchored keyframes are not supported inside locally retimed functions; use fixed time keyframes.",
                        ).with_ast_span(*span));
                        continue;
                    };
                    local_cursor_ms = time_ms;
                    let Some(global_ms) =
                        checked_scope_time(scope, local_cursor_ms, *span, diagnostics)
                    else {
                        continue;
                    };
                    let saved_opacity = self.default_opacity;
                    self.default_opacity = 1.0;
                    let mut rewritten = Vec::with_capacity(body.len());
                    for statement in body {
                        let (statement, next_shadowed) = retime_statement(
                            statement,
                            scope,
                            local_cursor_ms,
                            lexical_t_shadowed,
                            diagnostics,
                        );
                        rewritten.push(statement);
                        lexical_t_shadowed = next_shadowed;
                    }
                    self.process_body(global_ms, &rewritten, parent_label, diagnostics);
                    self.default_opacity = saved_opacity;
                    if sequenced {
                        local_cursor_ms += body
                            .iter()
                            .filter_map(|statement| statement_extent_ms(self, statement, false))
                            .fold(0.0, f64::max);
                    }
                    if !local_cursor_ms.is_finite() {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::InvalidModifierValue,
                            DiagnosticPhase::Build,
                            "A local keyframe cursor overflowed while advancing through the function body.",
                        ).with_ast_span(*span));
                        break;
                    }
                },
                Stmt::RelativeKeyframe { offset, body, span } => {
                    let Some(offset_ms) = local_keyframe_time_ms(offset, self.beat_duration_ms())
                    else {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::InvalidPropertyValue,
                            DiagnosticPhase::Build,
                            "Speech-anchored relative keyframes are not supported inside locally retimed functions; use fixed time keyframes.",
                        ).with_ast_span(*span));
                        continue;
                    };
                    local_cursor_ms += offset_ms;
                    let Some(global_ms) =
                        checked_scope_time(scope, local_cursor_ms, *span, diagnostics)
                    else {
                        continue;
                    };
                    let saved_opacity = self.default_opacity;
                    self.default_opacity = 1.0;
                    let mut rewritten = Vec::with_capacity(body.len());
                    for statement in body {
                        let (statement, next_shadowed) = retime_statement(
                            statement,
                            scope,
                            local_cursor_ms,
                            lexical_t_shadowed,
                            diagnostics,
                        );
                        rewritten.push(statement);
                        lexical_t_shadowed = next_shadowed;
                    }
                    self.process_body(global_ms, &rewritten, parent_label, diagnostics);
                    self.default_opacity = saved_opacity;
                    if sequenced {
                        local_cursor_ms += body
                            .iter()
                            .filter_map(|statement| statement_extent_ms(self, statement, false))
                            .fold(0.0, f64::max);
                    }
                    if !local_cursor_ms.is_finite() {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::InvalidModifierValue,
                            DiagnosticPhase::Build,
                            "A local relative keyframe cursor overflowed while advancing through the function body.",
                        ).with_ast_span(*span));
                        break;
                    }
                },
                _ => {
                    let Some(span_ms) = statement_extent_ms(self, stmt, sequenced)
                        .or(if sequenced { None } else { Some(0.0) })
                    else {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::UnsupportedSequenceStatement,
                            DiagnosticPhase::Build,
                            "Cannot infer this nested function's local time span; use fixed iterable/time literals.",
                        ));
                        continue;
                    };
                    let Some(global_ms) =
                        checked_scope_time(scope, local_cursor_ms, None, diagnostics)
                    else {
                        continue;
                    };
                    let (rewritten, next_shadowed) = retime_statement(
                        stmt,
                        scope,
                        local_cursor_ms,
                        lexical_t_shadowed,
                        diagnostics,
                    );
                    self.process_body(
                        global_ms,
                        std::slice::from_ref(&rewritten),
                        parent_label,
                        diagnostics,
                    );
                    lexical_t_shadowed = next_shadowed;
                    if sequenced {
                        local_cursor_ms += span_ms;
                    }
                    if !local_cursor_ms.is_finite() {
                        diagnostics.push(Diagnostic::error(
                            DiagnosticCode::InvalidModifierValue,
                            DiagnosticPhase::Build,
                            "A local function cursor overflowed while advancing through the body.",
                        ));
                        break;
                    }
                },
            }
        }
    }
}

fn checked_scope_time(
    scope: TimeScope,
    local_ms: f64,
    span: Option<Span>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<f64> {
    let global_ms = scope.at(local_ms);
    if local_ms.is_finite() && global_ms.is_finite() {
        return Some(global_ms);
    }
    diagnostics.push(
        Diagnostic::error(
            DiagnosticCode::InvalidModifierValue,
            DiagnosticPhase::Build,
            "Mapping a local function time to scene time overflowed; skipping this event.",
        )
        .with_ast_span(span),
    );
    None
}
