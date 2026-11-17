//! Lexical statement blocks and indexed component method specialization.

use super::*;
use crate::ast::InstanceScope;

impl Timeline {
    pub(in crate::timeline) fn process_block_stmt(
        &mut self,
        body: &[Stmt],
        instance_scope: Option<&InstanceScope>,
        span: Option<Span>,
        time_ms: f64,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        sequenced: bool,
    ) {
        let previous_authored_t = self.env.has_authored_t_binding();
        self.block_scope.push(std::collections::HashMap::new());
        self.process_instance_scope(
            body,
            instance_scope,
            span,
            time_ms,
            parent_label,
            diagnostics,
            sequenced,
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
        self.env.set_authored_t_binding(previous_authored_t);
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
    ) {
        let Some(scope) = instance_scope else {
            self.process_function_body(time_ms, body, parent_label, diagnostics, sequenced);
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
        self.process_body(time_ms, prologue, parent_label, diagnostics);
        let index = match evaluate_expr(&scope.index, &self.env) {
            Ok(Value::Num(index))
                if index.is_finite() && index >= 0.0 && index == index.floor() =>
            {
                index as usize
            },
            result => {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::InvalidPropertyValue,
                        DiagnosticPhase::Build,
                        format!("Component method target index must be a non-negative integer, got {result:?}; skipping its body."),
                    )
                    .with_ast_span(span),
                );
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
                _ => None,
            })
            .collect();
        let specialized = animatix_syntax::module::specialize_instance_property_reads(
            statements.to_vec(),
            &scope.label,
            &concrete_root,
            &bound_names,
        );
        self.process_function_body(time_ms, &specialized, parent_label, diagnostics, sequenced);
    }

    fn process_function_body(
        &mut self,
        time_ms: f64,
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
        sequenced: bool,
    ) {
        if !sequenced {
            self.process_body(time_ms, body, parent_label, diagnostics);
            return;
        }
        let mut cursor = time_ms;
        for statement in body {
            let duration = self.sequence_statement_span_ms(statement).unwrap_or(0.0);
            self.process_body(cursor, std::slice::from_ref(statement), parent_label, diagnostics);
            cursor += duration;
        }
    }
}
