use super::{
    Diagnostic, DiagnosticCode, DiagnosticPhase, Modifier, ModifierHost, ParsedStaggerModifiers,
    StaggerMetric, StaggerOrigin, Stmt, Timeline, TrackAccessor, parse_stagger_modifiers,
    parse_timing_modifiers, push_unsupported_stagger_statement_diagnostic, sequence_stmt_kind,
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
            },
            Stmt::Assignment {
                target,
                property,
                modifiers,
                ..
            } => {
                let subject =
                    format!("{}.{}", crate::timeline::assignment_target_key(target), property);
                let parsed = parse_timing_modifiers(
                    modifiers,
                    ModifierHost::Assignment,
                    Some(&subject),
                    &mut ignored_diagnostics,
                );
                Some(parsed.delay_ms + parsed.duration_ms)
            },
            Stmt::Block { body, .. } => {
                // Total duration of a function-call block is the sum of its
                // internal statements' spans (recursive).
                let mut total = 0.0;
                for stmt in body {
                    total += self.sequence_statement_span_ms(stmt).unwrap_or(0.0);
                }
                Some(total)
            },
            Stmt::Sequence { body, .. } => {
                // Total duration of a nested sequence is the sum of its children's durations
                let mut total = 0.0;
                for child in body {
                    total += self.sequence_statement_span_ms(child)?;
                }
                Some(total)
            },
            Stmt::Stagger {
                modifiers, body, ..
            } => {
                let parsed = parse_stagger_modifiers(modifiers, &mut ignored_diagnostics)?;
                if body.is_empty() {
                    return Some(0.0);
                }
                let delays = self.compute_stagger_delays(&parsed, body, 0.0);
                let mut total = 0.0;
                for (stmt, delay) in body.iter().zip(delays) {
                    let child_span = self.sequence_statement_span_ms(stmt).unwrap_or(0.0);
                    total = f64::max(total, delay + child_span);
                }
                Some(total)
            },
            Stmt::LetDecl { .. } | Stmt::TypeAlias { .. } | Stmt::Comment(..) => Some(0.0),
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
            if let Stmt::Block {
                body: block_body, ..
            } = stmt
            {
                // Function-call blocks sequence their internal statements
                // back-to-back, preserving the pre-expansion timing.
                for inner in block_body {
                    let Some(span_ms) = self.sequence_statement_span_ms(inner) else {
                        self.process_body(
                            cursor_time_ms,
                            std::slice::from_ref(inner),
                            parent_label,
                            diagnostics,
                        );
                        continue;
                    };
                    self.process_body(
                        cursor_time_ms,
                        std::slice::from_ref(inner),
                        parent_label,
                        diagnostics,
                    );
                    cursor_time_ms += span_ms;
                }
                continue;
            }
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

            self.process_body(
                cursor_time_ms,
                std::slice::from_ref(stmt),
                parent_label,
                diagnostics,
            );
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
        let Some(parsed) = parse_stagger_modifiers(modifiers, diagnostics) else {
            return;
        };

        let delays = self.compute_stagger_delays(&parsed, body, time_ms);

        for (index, stmt) in body.iter().enumerate() {
            let stagger_time_ms = time_ms + delays[index];
            if matches!(stmt, Stmt::Block { .. }) {
                self.process_body(
                    stagger_time_ms,
                    std::slice::from_ref(stmt),
                    parent_label,
                    diagnostics,
                );
                continue;
            }
            let Some(_) = self.sequence_statement_span_ms(stmt) else {
                push_unsupported_stagger_statement_diagnostic(
                    diagnostics,
                    sequence_stmt_kind(stmt),
                );
                continue;
            };

            self.process_body(
                stagger_time_ms,
                std::slice::from_ref(stmt),
                parent_label,
                diagnostics,
            );
        }
    }

    pub(super) fn compute_stagger_delays(
        &self,
        parsed: &ParsedStaggerModifiers,
        body: &[Stmt],
        time_ms: f64,
    ) -> Vec<f64> {
        let n = body.len();
        if n == 0 {
            return Vec::new();
        }

        let Some(ref origin) = parsed.from else {
            return (0..n).map(|i| i as f64 * parsed.interval_ms).collect();
        };

        let mut positions: Vec<Option<[f64; 2]>> = Vec::with_capacity(n);
        for stmt in body {
            let pos_opt = Self::statement_target_actor(stmt).and_then(|label| {
                self.tracks.get(&label).map(|track| {
                    let pos = track.geometry.position.get(time_ms as u64, [0.0, 0.0]);
                    let size = track.geometry.size.get(time_ms as u64, [0.0, 0.0]);
                    [
                        pos[0] as f64 + size[0] as f64 * 0.5,
                        pos[1] as f64 + size[1] as f64 * 0.5,
                    ]
                })
            });
            positions.push(pos_opt);
        }

        let valid_positions: Vec<[f64; 2]> = positions.iter().filter_map(|p| *p).collect();
        if valid_positions.is_empty() {
            return (0..n).map(|i| i as f64 * parsed.interval_ms).collect();
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for [x, y] in &valid_positions {
            min_x = min_x.min(*x);
            max_x = max_x.max(*x);
            min_y = min_y.min(*y);
            max_y = max_y.max(*y);
        }

        let ref_pt = match origin {
            StaggerOrigin::Center => [(min_x + max_x) * 0.5, (min_y + max_y) * 0.5],
            StaggerOrigin::TopLeft => [min_x, min_y],
            StaggerOrigin::Point(pt) => *pt,
        };

        let mut items = Vec::with_capacity(n);
        for (i, pos_opt) in positions.iter().enumerate() {
            let dist = match pos_opt {
                Some([x, y]) => match parsed.metric {
                    StaggerMetric::Euclidean => (x - ref_pt[0]).hypot(y - ref_pt[1]),
                    StaggerMetric::Manhattan => (x - ref_pt[0]).abs() + (y - ref_pt[1]).abs(),
                },
                None => 1e9 + i as f64,
            };
            items.push((i, dist));
        }

        items.sort_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        let mut ring = 0;
        let mut prev_dist: Option<f64> = None;
        let mut delays = vec![0.0; n];

        for (idx, dist) in items {
            if let Some(prev) = prev_dist {
                if (dist - prev).abs() > 0.5 {
                    ring += 1;
                }
            }
            prev_dist = Some(dist);
            delays[idx] = ring as f64 * parsed.interval_ms;
        }

        delays
    }

    fn statement_target_actor(stmt: &Stmt) -> Option<String> {
        match stmt {
            Stmt::Action(action, _) => action.targets.first().cloned(),
            Stmt::Assignment { target, .. } => {
                let key = crate::timeline::assignment_target_key(target);
                if key.is_empty() { None } else { Some(key) }
            },
            Stmt::Block { body, .. } => body.iter().find_map(Self::statement_target_actor),
            _ => None,
        }
    }
}
