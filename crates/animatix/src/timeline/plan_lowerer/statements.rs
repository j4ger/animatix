use super::*;

impl Timeline {
    pub(super) fn mount_statements_from(
        &mut self,
        body: &[Stmt],
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        mut cursor_local_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        let mut extent = local_extent_ms.unwrap_or(0.0);
        for statement in body {
            let first_new_entity_index = self.world.len();
            match statement {
                Stmt::Keyframe { time, body, span } => {
                    if let Some((marker, nested_extent)) = self.mount_plan_marker(
                        time,
                        body,
                        *span,
                        false,
                        origin_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        cursor_local_ms,
                        diagnostics,
                    ) {
                        cursor_local_ms = marker;
                        extent = extent.max(nested_extent);
                    }
                },
                Stmt::RelativeKeyframe { offset, body, span } => {
                    if let Some((marker, nested_extent)) = self.mount_plan_marker(
                        offset,
                        body,
                        *span,
                        true,
                        origin_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        cursor_local_ms,
                        diagnostics,
                    ) {
                        cursor_local_ms = marker;
                        extent = extent.max(nested_extent);
                    }
                },
                Stmt::Always { body, span } => {
                    self.mount_local_always(
                        body,
                        *span,
                        origin_ms,
                        cursor_local_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        diagnostics,
                    );
                },
                Stmt::ReactiveBinding {
                    target,
                    property,
                    value,
                    value_span,
                    span,
                } => {
                    self.mount_plan_reactive_binding(
                        target,
                        property,
                        value,
                        *value_span,
                        *span,
                        origin_ms,
                        cursor_local_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        diagnostics,
                    );
                },
                Stmt::Sequence { body, .. } => {
                    extent = extent.max(self.mount_plan_sequence(
                        body,
                        origin_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        cursor_local_ms,
                        diagnostics,
                    ));
                },
                Stmt::Stagger {
                    modifiers, body, ..
                } => {
                    extent = extent.max(self.mount_plan_stagger(
                        modifiers,
                        body,
                        origin_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        cursor_local_ms,
                        diagnostics,
                    ));
                },
                Stmt::ForLoop {
                    var,
                    index_var,
                    iterable,
                    body,
                    modifiers,
                    ..
                } => {
                    if let Some(loop_extent) = self.mount_plan_for_loop(
                        var,
                        index_var.as_ref(),
                        iterable,
                        body,
                        modifiers,
                        origin_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        cursor_local_ms,
                        diagnostics,
                    ) {
                        extent = extent.max(loop_extent);
                    }
                },
                Stmt::Conditional {
                    condition,
                    then_branch,
                    else_branch,
                    ..
                } => {
                    if let Some(branch_extent) = self.mount_plan_conditional(
                        condition,
                        then_branch,
                        else_branch.as_deref(),
                        origin_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        cursor_local_ms,
                        diagnostics,
                    ) {
                        extent = extent.max(branch_extent);
                    }
                },
                Stmt::Match {
                    scrutinee, arms, ..
                } => {
                    if let Some(branch_extent) = self.mount_plan_match(
                        scrutinee,
                        arms,
                        origin_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        cursor_local_ms,
                        diagnostics,
                    ) {
                        extent = extent.max(branch_extent);
                    }
                },
                Stmt::Block {
                    body,
                    instance_scope: None,
                    ..
                } => {
                    if let Some(block_extent) = self.mount_plan_block(
                        body,
                        origin_ms,
                        rate,
                        local_extent_ms,
                        parent_label,
                        cursor_local_ms,
                        diagnostics,
                    ) {
                        extent = extent.max(block_extent);
                    }
                },
                _ => {
                    let global_ms = origin_ms + cursor_local_ms * rate;
                    self.process_body(
                        global_ms,
                        std::slice::from_ref(statement),
                        parent_label,
                        diagnostics,
                    );
                    if let Some(span) = statement_span_ms(statement, &self.env) {
                        extent = extent.max(cursor_local_ms + span);
                    }
                },
            }
            self.mark_new_plan_actors(first_new_entity_index);
        }
        Some(extent)
    }

    #[allow(clippy::too_many_arguments)] // Keyframe marker and body share the active placement context.
    fn mount_plan_marker(
        &mut self,
        time: &Time,
        body: &[Stmt],
        span: Option<Span>,
        relative: bool,
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        cursor_local_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<(f64, f64)> {
        let marker_ms = self.resolve_marker_time(time, span, diagnostics)?;
        let cursor = if relative {
            cursor_local_ms + marker_ms
        } else {
            marker_ms
        };
        let extent = self.mount_statements_from(
            body,
            origin_ms,
            rate,
            local_extent_ms,
            parent_label,
            cursor,
            diagnostics,
        )?;
        Some((cursor, extent))
    }

    #[allow(clippy::too_many_arguments)] // Reactive shorthand shares the plan-local clock of always.
    fn mount_plan_reactive_binding(
        &mut self,
        target: &[animatix_syntax::ast::TargetSegment],
        property: &str,
        value: &Expr,
        value_span: Option<animatix_syntax::ast::ByteSpan>,
        span: Option<Span>,
        origin_ms: f64,
        cursor_local_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let body = [Stmt::Assignment {
            target: target.to_vec(),
            property: property.to_owned(),
            value: value.clone(),
            modifiers: Vec::new(),
            easing: None,
            value_span,
            span,
        }];
        self.mount_local_always(
            &body,
            span,
            origin_ms,
            cursor_local_ms,
            rate,
            local_extent_ms,
            parent_label,
            diagnostics,
        );
    }

    #[allow(clippy::too_many_arguments)] // Carries the active placement clock and lexical scope.
    fn mount_plan_for_loop(
        &mut self,
        var: &animatix_syntax::ast::LoopPattern,
        index_var: Option<&String>,
        iterable: &Expr,
        body: &[Stmt],
        modifiers: &[Modifier],
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        cursor_local_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        let global_time_ms = origin_ms + cursor_local_ms * rate;
        let step_ms = crate::timeline::build::parse_for_loop_step(
            modifiers,
            &self.env,
            global_time_ms,
            diagnostics,
        );
        let scope =
            crate::timeline::env::LoopScope::capture(&self.env, var, index_var.map(String::as_str));
        self.build_loop_scopes.push(scope.names().map(str::to_owned).collect());

        let mut extent = local_extent_ms.unwrap_or(0.0);
        let values = crate::timeline::lookup::for_iter_values(iterable, &self.env);
        for (index, value) in values.into_iter().enumerate() {
            crate::timeline::build::bind_loop_var(&mut self.env, var, value, index, diagnostics);
            if let Some(name) = index_var {
                self.env.set(name, Value::Num(index as f64));
                if name == "t" {
                    self.env.set_authored_t_binding(true);
                }
            }
            if let Some(iteration_extent) = self.mount_statements_from(
                body,
                origin_ms,
                rate,
                local_extent_ms,
                parent_label,
                cursor_local_ms + step_ms * index as f64,
                diagnostics,
            ) {
                extent = extent.max(iteration_extent);
            }
        }

        self.build_loop_scopes.pop();
        scope.restore(&mut self.env);
        Some(extent)
    }

    fn plan_conditional_branch<'a>(
        &self,
        condition: &Expr,
        then_branch: &'a [Stmt],
        else_branch: Option<&'a [Stmt]>,
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<&'a [Stmt]> {
        let eval_env = self.build_eval_env(time_ms.max(0.0) as u64);
        match evaluate_expr(condition, &eval_env) {
            Ok(value) if value.is_truthy() => Some(then_branch),
            Ok(_) => else_branch,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::InvalidPropertyValue,
                    DiagnosticPhase::Build,
                    format!("Failed to evaluate `if` condition: {error}; skipping the block."),
                ));
                None
            },
        }
    }

    fn plan_match_branch<'a>(
        &self,
        scrutinee: &Expr,
        arms: &'a [(animatix_syntax::ast::MatchPattern, Vec<Stmt>)],
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<&'a [Stmt]> {
        let eval_env = self.build_eval_env(time_ms.max(0.0) as u64);
        let value = match evaluate_expr(scrutinee, &eval_env) {
            Ok(value) => value,
            Err(error) => {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::ModuleExportEvalError,
                    DiagnosticPhase::Build,
                    format!("Failed to evaluate match scrutinee: {error}; skipping match."),
                ));
                return None;
            },
        };
        if let Some((_, body)) = arms
            .iter()
            .find(|(pattern, _)| crate::timeline::build::pattern_matches(pattern, &value))
        {
            return Some(body);
        }
        if !arms
            .iter()
            .any(|(pattern, _)| matches!(pattern, animatix_syntax::ast::MatchPattern::Wildcard))
        {
            diagnostics.push(Diagnostic::warning(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!(
                    "match scrutinee evaluated to {value:?} but no arm matched and no `_` wildcard arm was provided"
                ),
            ));
        }
        None
    }

    #[allow(clippy::too_many_arguments)] // Lexical block restoration follows the ordinary build path.
    fn mount_plan_block(
        &mut self,
        body: &[Stmt],
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        cursor_local_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        let previous_authored_t = self.env.has_authored_t_binding();
        self.block_scope.push(HashMap::new());
        let extent = self.mount_statements_from(
            body,
            origin_ms,
            rate,
            local_extent_ms,
            parent_label,
            cursor_local_ms,
            diagnostics,
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
        extent
    }

    #[allow(clippy::too_many_arguments)] // Carries the placement clock into each sequence item.
    fn mount_plan_sequence(
        &mut self,
        body: &[Stmt],
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        cursor_local_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> f64 {
        let mut extent = local_extent_ms.unwrap_or(0.0);
        let mut cursor = cursor_local_ms;
        for statement in body {
            let Some(statement_extent) = statement_span_ms(statement, &self.env) else {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::UnsupportedSequenceStatement,
                    DiagnosticPhase::Build,
                    "Cannot determine the extent of a statement in this sequence.",
                ));
                continue;
            };
            if let Some(nested_extent) = self.mount_statements_from(
                std::slice::from_ref(statement),
                origin_ms,
                rate,
                local_extent_ms,
                parent_label,
                cursor,
                diagnostics,
            ) {
                extent = extent.max(nested_extent);
            }
            cursor += statement_extent;
        }
        extent
    }

    #[allow(clippy::too_many_arguments)] // Carries the placement clock into each stagger item.
    fn mount_plan_stagger(
        &mut self,
        modifiers: &[Modifier],
        body: &[Stmt],
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        cursor_local_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> f64 {
        let mut extent = local_extent_ms.unwrap_or(0.0);
        let mut stagger_diagnostics = Vec::new();
        if let Some(parsed) =
            crate::timeline::parse_stagger_modifiers(modifiers, &mut stagger_diagnostics)
        {
            let global_ms = origin_ms + cursor_local_ms * rate;
            let delays = self.compute_stagger_delays(&parsed, body, global_ms);
            for (statement, delay_ms) in body.iter().zip(delays) {
                if let Some(nested_extent) = self.mount_statements_from(
                    std::slice::from_ref(statement),
                    origin_ms,
                    rate,
                    local_extent_ms,
                    parent_label,
                    cursor_local_ms + delay_ms,
                    diagnostics,
                ) {
                    extent = extent.max(nested_extent);
                }
            }
        } else {
            diagnostics.extend(stagger_diagnostics);
        }
        if let Some(stagger_extent) = statement_span_ms(
            &Stmt::Stagger {
                modifiers: modifiers.to_vec(),
                body: body.to_vec(),
                span: None,
            },
            &self.env,
        ) {
            extent = extent.max(cursor_local_ms + stagger_extent);
        }
        extent
    }

    #[allow(clippy::too_many_arguments)] // The branch inherits its enclosing placement context.
    fn mount_plan_conditional(
        &mut self,
        condition: &Expr,
        then_branch: &[Stmt],
        else_branch: Option<&[Stmt]>,
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        cursor_local_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        let branch = self.plan_conditional_branch(
            condition,
            then_branch,
            else_branch,
            origin_ms + cursor_local_ms * rate,
            diagnostics,
        )?;
        self.mount_statements_from(
            branch,
            origin_ms,
            rate,
            local_extent_ms,
            parent_label,
            cursor_local_ms,
            diagnostics,
        )
    }

    #[allow(clippy::too_many_arguments)] // The branch inherits its enclosing placement context.
    fn mount_plan_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[(animatix_syntax::ast::MatchPattern, Vec<Stmt>)],
        origin_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        cursor_local_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Option<f64> {
        let branch = self.plan_match_branch(
            scrutinee,
            arms,
            origin_ms + cursor_local_ms * rate,
            diagnostics,
        )?;
        self.mount_statements_from(
            branch,
            origin_ms,
            rate,
            local_extent_ms,
            parent_label,
            cursor_local_ms,
            diagnostics,
        )
    }

    fn mount_local_always(
        &mut self,
        body: &[Stmt],
        span: Option<Span>,
        origin_ms: f64,
        cursor_local_ms: f64,
        rate: f64,
        local_extent_ms: Option<f64>,
        parent_label: Option<&str>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(end_local) = local_extent_ms.filter(|end| *end > cursor_local_ms) else {
            diagnostics.push(plan_timing_error(
                "a plan-local `always` needs a finite positive clip extent",
                span,
            ));
            return;
        };
        let global_start = origin_ms + cursor_local_ms * rate;
        let global_end = origin_ms + end_local * rate;
        let body = localize_always_body(
            body,
            global_start,
            global_end,
            origin_ms,
            rate,
            self.env.has_authored_t_binding(),
        );
        self.local_time_scope_end_ms = self.local_time_scope_end_ms.max(global_end.ceil() as u64);
        self.process_body(global_start, &[Stmt::Always { body, span }], parent_label, diagnostics);
    }
}
