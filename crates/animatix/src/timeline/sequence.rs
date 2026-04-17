use super::{
    parse_stagger_interval_ms, parse_timing_modifiers,
    push_unsupported_stagger_statement_diagnostic, sequence_stmt_kind, Diagnostic, DiagnosticCode,
    DiagnosticPhase, Modifier, ModifierHost, Stmt, Timeline,
};

impl Timeline {
    pub(super) fn sequence_statement_span_ms(&self, stmt: &Stmt) -> Option<f64> {
        let mut ignored_diagnostics = Vec::new();
        match stmt {
            Stmt::Action(action) => {
                let parsed = parse_timing_modifiers(
                    &action.modifiers,
                    ModifierHost::Action,
                    Some(&action.verb),
                    &mut ignored_diagnostics,
                );
                Some(parsed.delay_ms + parsed.duration_ms)
            }
            Stmt::Assignment {
                target,
                property,
                modifiers,
                ..
            } => {
                let subject = format!("{}.{}", target.join("."), property);
                let parsed = parse_timing_modifiers(
                    modifiers,
                    ModifierHost::Assignment,
                    Some(&subject),
                    &mut ignored_diagnostics,
                );
                Some(parsed.delay_ms + parsed.duration_ms)
            }
            _ => None,
        }
    }

    pub(super) fn process_sequence(
        &mut self,
        time_ms: f64,
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let mut cursor_time_ms = time_ms;

        for stmt in body {
            let Some(span_ms) = self.sequence_statement_span_ms(stmt) else {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnsupportedSequenceStatement,
                        DiagnosticPhase::Build,
                        format!(
                            "Sequence blocks currently support only actions and assignments; '{}' is not supported in sequence v1a.",
                            sequence_stmt_kind(stmt)
                        ),
                    )
                    .with_subject("sequence"),
                );
                continue;
            };

            self.process_body(cursor_time_ms, &[stmt.clone()], parent_label, diagnostics);
            cursor_time_ms += span_ms;
        }
    }

    pub(super) fn process_stagger(
        &mut self,
        time_ms: f64,
        modifiers: &[Modifier],
        body: &[Stmt],
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(interval_ms) = parse_stagger_interval_ms(modifiers, diagnostics) else {
            return;
        };

        for (index, stmt) in body.iter().enumerate() {
            let Some(_) = self.sequence_statement_span_ms(stmt) else {
                push_unsupported_stagger_statement_diagnostic(
                    diagnostics,
                    sequence_stmt_kind(stmt),
                );
                continue;
            };

            let stagger_time_ms = time_ms + interval_ms * index as f64;
            self.process_body(stagger_time_ms, &[stmt.clone()], parent_label, diagnostics);
        }
    }
}
