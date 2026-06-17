use super::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, Modifier, ModifierHost, Stmt, Timeline,
    parse_stagger_interval_ms, parse_timing_modifiers,
    push_unsupported_stagger_statement_diagnostic, sequence_stmt_kind,
};

impl Timeline {
    pub(super) fn sequence_statement_span_ms(&self, stmt: &Stmt) -> Option<f64> {
        let mut ignored_diagnostics = Vec::new();
        match stmt {
            Stmt::Action(action, ..) => {
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
            Stmt::Sequence { body, .. } => {
                // Total duration of a nested sequence is the sum of its children's durations
                let mut total = 0.0;
                for child in body {
                    total += self.sequence_statement_span_ms(child)?;
                }
                Some(total)
            }
            Stmt::Stagger { modifiers, body, .. } => {
                let interval_ms = parse_stagger_interval_ms(modifiers, &mut ignored_diagnostics)?;
                if body.is_empty() {
                    return Some(0.0);
                }
                // Total duration: (n-1) * interval + last_child_duration
                let last_idx = body.len() - 1;
                let last_span = self.sequence_statement_span_ms(&body[last_idx])?;
                Some(interval_ms * last_idx as f64 + last_span)
            }
            Stmt::LetDecl { .. } | Stmt::Comment(..) => Some(0.0),
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
                    Diagnostic::error(
                        DiagnosticCode::UnsupportedSequenceStatement,
                        DiagnosticPhase::Build,
                        match sequence_stmt_kind(stmt) {
                            "actor declaration" => "Sequence blocks do not support actor declarations. Declare actors before the composition block, then reference them inside.".to_string(),
                            kind => format!(
                                "Sequence blocks support only actions and assignments; '{kind}' is not supported."
                            ),
                        },
                    )
                    .with_subject("sequence"),
                );
                continue;
            };

            self.process_body(cursor_time_ms, std::slice::from_ref(stmt), parent_label, diagnostics);
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
            self.process_body(stagger_time_ms, std::slice::from_ref(stmt), parent_label, diagnostics);
        }
    }
}
