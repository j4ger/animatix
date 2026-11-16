use super::*;
use crate::ast::{BinaryOp, Expr, InlineItem};

/// Scale authored durations and rewrite only the time-reading consumers that
/// are supported by the first implementation (`always` and plot `func`).
pub(in crate::timeline::build::process) fn retime_statement(
    stmt: &Stmt,
    scope: TimeScope,
    local_cursor_ms: f64,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    let global_start = scope.at(local_cursor_ms);
    let local_t = local_time_expr(scope);
    retime_stmt(stmt, scope, global_start, &local_t, lexical_t_shadowed, diagnostics)
}

fn retime_stmt(
    stmt: &Stmt,
    scope: TimeScope,
    global_start_ms: f64,
    local_t: &Expr,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    match stmt {
        Stmt::LetDecl {
            is_pub,
            name,
            value,
            span,
        } => (
            Stmt::LetDecl {
                is_pub: *is_pub,
                name: name.clone(),
                value: rewrite_expr_t(value.clone(), local_t, lexical_t_shadowed),
                span: *span,
            },
            lexical_t_shadowed || name == "t",
        ),
        Stmt::Action(action, span) => {
            let mut action = action.clone();
            if is_unsupported_action(&action.verb) {
                diagnostics.push(unsupported_time_consumer(&action.verb));
            }
            scale_modifiers(&mut action.modifiers, scope.rate, diagnostics);
            (Stmt::Action(action, *span), lexical_t_shadowed)
        },
        Stmt::Assignment {
            target,
            property,
            value,
            modifiers,
            easing,
            value_span,
            span,
        } => {
            let mut modifiers = modifiers.clone();
            scale_modifiers(&mut modifiers, scope.rate, diagnostics);
            let local_value_t = if property == "func" {
                local_plot_time_expr(scope)
            } else {
                local_t.clone()
            };
            (
                Stmt::Assignment {
                    target: target.clone(),
                    property: property.clone(),
                    value: rewrite_expr_t(value.clone(), &local_value_t, lexical_t_shadowed),
                    modifiers,
                    easing: *easing,
                    value_span: *value_span,
                    span: *span,
                },
                lexical_t_shadowed,
            )
        },
        Stmt::ActorDecl { .. } => retime_actor_decl(
            stmt,
            scope.rate,
            &local_plot_time_expr(scope),
            lexical_t_shadowed,
            diagnostics,
        ),
        Stmt::Always { .. } => {
            retime_always(stmt, scope, global_start_ms, local_t, lexical_t_shadowed, diagnostics)
        },
        Stmt::ReactiveBinding { .. } => retime_reactive_binding(
            stmt,
            scope,
            global_start_ms,
            local_t,
            lexical_t_shadowed,
            diagnostics,
        ),
        Stmt::Sequence { body, span } => {
            let (body, shadowed) = retime_stmt_list(
                body,
                scope,
                global_start_ms,
                local_t,
                lexical_t_shadowed,
                diagnostics,
            );
            (Stmt::Sequence { body, span: *span }, shadowed)
        },
        Stmt::Stagger {
            modifiers,
            body,
            span,
        } => {
            let mut modifiers = modifiers.clone();
            scale_modifiers(&mut modifiers, scope.rate, diagnostics);
            let (body, shadowed) = retime_stmt_list(
                body,
                scope,
                global_start_ms,
                local_t,
                lexical_t_shadowed,
                diagnostics,
            );
            (
                Stmt::Stagger {
                    modifiers,
                    body,
                    span: *span,
                },
                shadowed,
            )
        },
        Stmt::ForLoop { .. } => {
            retime_for_loop(stmt, scope, global_start_ms, local_t, lexical_t_shadowed, diagnostics)
        },
        Stmt::Block { .. } => {
            retime_block(stmt, scope, global_start_ms, lexical_t_shadowed, diagnostics)
        },
        Stmt::Conditional { .. } => retime_conditional(
            stmt,
            scope,
            global_start_ms,
            local_t,
            lexical_t_shadowed,
            diagnostics,
        ),
        Stmt::Match { .. } => {
            retime_match(stmt, scope, global_start_ms, local_t, lexical_t_shadowed, diagnostics)
        },
        Stmt::Play { .. } => {
            diagnostics.push(unsupported_time_consumer("play"));
            (stmt.clone(), lexical_t_shadowed)
        },
        _ => (stmt.clone(), lexical_t_shadowed),
    }
}

fn retime_actor_decl(
    stmt: &Stmt,
    rate: f64,
    local_t: &Expr,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    let Stmt::ActorDecl {
        is_pub,
        is_anonymous,
        label,
        array_index,
        ty,
        props,
        modifiers,
        children,
        span,
    } = stmt
    else {
        unreachable!()
    };
    if is_unsupported_actor(ty, props) {
        diagnostics.push(unsupported_time_consumer(ty));
    }
    let props = props
        .iter()
        .map(|prop| crate::ast::Property {
            value: if prop.name == "func" {
                rewrite_expr_t(prop.value.clone(), local_t, lexical_t_shadowed)
            } else {
                prop.value.clone()
            },
            ..prop.clone()
        })
        .collect();
    let children = children
        .iter()
        .map(|child| retime_inline_item(child, local_t, rate, lexical_t_shadowed, diagnostics))
        .collect();
    (
        Stmt::ActorDecl {
            is_pub: *is_pub,
            is_anonymous: *is_anonymous,
            label: label.clone(),
            array_index: array_index.clone(),
            ty: ty.clone(),
            props,
            modifiers: scaled_modifiers(modifiers, rate, diagnostics),
            children,
            span: *span,
        },
        lexical_t_shadowed,
    )
}

fn retime_always(
    stmt: &Stmt,
    scope: TimeScope,
    global_start_ms: f64,
    local_t: &Expr,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    let Stmt::Always { body, span } = stmt else {
        unreachable!()
    };
    let Some(end_ms) = scope.end_ms.filter(|end| *end > global_start_ms) else {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::InvalidModifierValue,
                DiagnosticPhase::Build,
                "A locally retimed `always` needs a positive finite interval; add `clip_duration:` or a later keyframe/action boundary.",
            )
            .with_ast_span(*span),
        );
        return (stmt.clone(), lexical_t_shadowed);
    };
    let (body, _) =
        retime_stmt_list(body, scope, global_start_ms, local_t, lexical_t_shadowed, diagnostics);
    let t = Expr::Ident("t".into());
    let condition = Expr::Binary(
        Box::new(Expr::Binary(
            Box::new(t.clone()),
            BinaryOp::Gte,
            Box::new(Expr::Num(global_start_ms / 1000.0)),
        )),
        BinaryOp::And,
        Box::new(Expr::Binary(Box::new(t), BinaryOp::Lt, Box::new(Expr::Num(end_ms / 1000.0)))),
    );
    (
        Stmt::Always {
            body: vec![Stmt::Conditional {
                condition,
                then_branch: body,
                else_branch: None,
                span: *span,
            }],
            span: *span,
        },
        lexical_t_shadowed,
    )
}

fn retime_reactive_binding(
    stmt: &Stmt,
    scope: TimeScope,
    global_start_ms: f64,
    local_t: &Expr,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    let Stmt::ReactiveBinding {
        target,
        property,
        value,
        value_span,
        span,
    } = stmt
    else {
        unreachable!()
    };
    retime_always(
        &Stmt::Always {
            body: vec![Stmt::Assignment {
                target: target.clone(),
                property: property.clone(),
                value: value.clone(),
                modifiers: Vec::new(),
                easing: None,
                value_span: *value_span,
                span: *span,
            }],
            span: *span,
        },
        scope,
        global_start_ms,
        local_t,
        lexical_t_shadowed,
        diagnostics,
    )
}

fn retime_for_loop(
    stmt: &Stmt,
    scope: TimeScope,
    global_start_ms: f64,
    local_t: &Expr,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    let Stmt::ForLoop {
        var,
        index_var,
        iterable,
        body,
        modifiers,
        span,
    } = stmt
    else {
        unreachable!()
    };
    let body_shadowed =
        lexical_t_shadowed || loop_binds_t(var) || index_var.as_deref() == Some("t");
    let (body, _) =
        retime_stmt_list(body, scope, global_start_ms, local_t, body_shadowed, diagnostics);
    (
        Stmt::ForLoop {
            var: var.clone(),
            index_var: index_var.clone(),
            iterable: iterable.clone(),
            body,
            modifiers: scaled_modifiers(modifiers, scope.rate, diagnostics),
            span: *span,
        },
        lexical_t_shadowed,
    )
}

fn retime_block(
    stmt: &Stmt,
    scope: TimeScope,
    global_start_ms: f64,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    let Stmt::Block {
        body,
        instance_scope,
        time_scope,
        span,
    } = stmt
    else {
        unreachable!()
    };
    (
        Stmt::Block {
            body: body.clone(),
            instance_scope: instance_scope.clone(),
            time_scope: compose_nested_scope(
                time_scope.clone(),
                scope,
                global_start_ms,
                diagnostics,
                *span,
            ),
            span: *span,
        },
        lexical_t_shadowed,
    )
}

fn retime_conditional(
    stmt: &Stmt,
    scope: TimeScope,
    global_start_ms: f64,
    local_t: &Expr,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    let Stmt::Conditional {
        condition,
        then_branch,
        else_branch,
        span,
    } = stmt
    else {
        unreachable!()
    };
    let (then_branch, _) = retime_stmt_list(
        then_branch,
        scope,
        global_start_ms,
        local_t,
        lexical_t_shadowed,
        diagnostics,
    );
    let else_branch = else_branch.as_ref().map(|body| {
        retime_stmt_list(body, scope, global_start_ms, local_t, lexical_t_shadowed, diagnostics).0
    });
    (
        Stmt::Conditional {
            condition: rewrite_expr_t(condition.clone(), local_t, lexical_t_shadowed),
            then_branch,
            else_branch,
            span: *span,
        },
        lexical_t_shadowed,
    )
}

fn retime_match(
    stmt: &Stmt,
    scope: TimeScope,
    global_start_ms: f64,
    local_t: &Expr,
    lexical_t_shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Stmt, bool) {
    let Stmt::Match {
        scrutinee,
        arms,
        span,
    } = stmt
    else {
        unreachable!()
    };
    let arms = arms
        .iter()
        .map(|(pattern, body)| {
            let (body, _) = retime_stmt_list(
                body,
                scope,
                global_start_ms,
                local_t,
                lexical_t_shadowed,
                diagnostics,
            );
            (pattern.clone(), body)
        })
        .collect();
    (
        Stmt::Match {
            scrutinee: rewrite_expr_t(scrutinee.clone(), local_t, lexical_t_shadowed),
            arms,
            span: *span,
        },
        lexical_t_shadowed,
    )
}

fn retime_stmt_list(
    body: &[Stmt],
    scope: TimeScope,
    global_start_ms: f64,
    local_t: &Expr,
    mut shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> (Vec<Stmt>, bool) {
    let mut rewritten = Vec::with_capacity(body.len());
    for stmt in body {
        let (stmt, next_shadowed) =
            retime_stmt(stmt, scope, global_start_ms, local_t, shadowed, diagnostics);
        rewritten.push(stmt);
        shadowed = next_shadowed;
    }
    (rewritten, shadowed)
}

fn retime_inline_item(
    item: &InlineItem,
    local_t: &Expr,
    rate: f64,
    shadowed: bool,
    diagnostics: &mut Vec<Diagnostic>,
) -> InlineItem {
    match item {
        InlineItem::Labeled {
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
        } => {
            if is_unsupported_actor(ty, props) {
                diagnostics.push(unsupported_time_consumer(ty));
            }
            InlineItem::Labeled {
                label: label.clone(),
                array_index: array_index.clone(),
                ty: ty.clone(),
                props: props
                    .iter()
                    .map(|prop| crate::ast::Property {
                        value: if prop.name == "func" {
                            rewrite_expr_t(prop.value.clone(), local_t, shadowed)
                        } else {
                            prop.value.clone()
                        },
                        ..prop.clone()
                    })
                    .collect(),
                modifiers: scaled_modifiers(modifiers, rate, diagnostics),
                children: children
                    .iter()
                    .map(|child| retime_inline_item(child, local_t, rate, shadowed, diagnostics))
                    .collect(),
            }
        },
        InlineItem::Anonymous {
            ty,
            props,
            modifiers,
            children,
        } => {
            if is_unsupported_actor(ty, props) {
                diagnostics.push(unsupported_time_consumer(ty));
            }
            InlineItem::Anonymous {
                ty: ty.clone(),
                props: props
                    .iter()
                    .map(|prop| crate::ast::Property {
                        value: if prop.name == "func" {
                            rewrite_expr_t(prop.value.clone(), local_t, shadowed)
                        } else {
                            prop.value.clone()
                        },
                        ..prop.clone()
                    })
                    .collect(),
                modifiers: scaled_modifiers(modifiers, rate, diagnostics),
                children: children
                    .iter()
                    .map(|child| retime_inline_item(child, local_t, rate, shadowed, diagnostics))
                    .collect(),
            }
        },
        InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } => InlineItem::ForLoop {
            var: var.clone(),
            index_var: index_var.clone(),
            iterable: iterable.clone(),
            body: body
                .iter()
                .map(|child| {
                    retime_inline_item(
                        child,
                        local_t,
                        rate,
                        shadowed || loop_binds_t(var) || index_var.as_deref() == Some("t"),
                        diagnostics,
                    )
                })
                .collect(),
        },
        InlineItem::SlotFill { slot, items } => InlineItem::SlotFill {
            slot: slot.clone(),
            items: items
                .iter()
                .map(|child| retime_inline_item(child, local_t, rate, shadowed, diagnostics))
                .collect(),
        },
        other => other.clone(),
    }
}

fn scaled_modifiers(
    modifiers: &[Modifier],
    rate: f64,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Modifier> {
    let mut modifiers = modifiers.to_vec();
    scale_modifiers(&mut modifiers, rate, diagnostics);
    modifiers
}

fn scale_modifiers(modifiers: &mut [Modifier], rate: f64, diagnostics: &mut Vec<Diagnostic>) {
    let should_rewrite = (rate - 1.0).abs() >= f64::EPSILON;
    for modifier in modifiers {
        let time_modifier = modifier
            .name
            .as_deref()
            .is_none_or(|name| matches!(name, "delay" | "duration" | "step" | "each"));
        if !time_modifier {
            continue;
        }
        let raw = match &modifier.value {
            Expr::Ident(raw) | Expr::Str(raw) => raw,
            _ => continue,
        };
        if let Some(ms) = crate::timeline::parse_duration_literal(raw) {
            let scaled = ms * rate;
            if scaled.is_finite() {
                if should_rewrite {
                    modifier.value = Expr::Ident(format!("{}ms", scaled));
                }
            } else {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::InvalidModifierValue,
                    DiagnosticPhase::Build,
                    format!(
                        "Scaling modifier `{}` overflows local time.",
                        modifier.name.as_deref().unwrap_or("duration")
                    ),
                ));
            }
        }
    }
}

fn compose_nested_scope(
    scope: Option<TimeScopeSpec>,
    parent_scope: TimeScope,
    global_start_ms: f64,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) -> Option<TimeScopeSpec> {
    let inherited_limit = parent_scope.end_ms.map(|end_ms| (end_ms - global_start_ms).max(0.0));
    if inherited_limit.is_some_and(|limit| !limit.is_finite()) {
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::InvalidModifierValue,
                DiagnosticPhase::Build,
                "A nested time scope exceeds the finite range of its parent scope.",
            )
            .with_ast_span(span),
        );
        return None;
    }
    if (parent_scope.rate - 1.0).abs() < f64::EPSILON
        && inherited_limit.is_none()
        && scope.is_none()
    {
        return None;
    }
    let mut scope = scope.unwrap_or(TimeScopeSpec {
        time_scale: Some(Expr::Num(parent_scope.rate)),
        clip_duration: None,
        end_limit_duration_ms: None,
    });
    if (parent_scope.rate - 1.0).abs() >= f64::EPSILON {
        if let Some(current) = scope.time_scale.take() {
            scope.time_scale = Some(Expr::Binary(
                Box::new(current),
                BinaryOp::Mul,
                Box::new(Expr::Num(parent_scope.rate)),
            ));
        } else if let Some(current) = scope.clip_duration.take() {
            match parse_clip_duration_ms(&current) {
                Some(duration) if (duration * parent_scope.rate).is_finite() => {
                    scope.clip_duration =
                        Some(Expr::Ident(format!("{}ms", duration * parent_scope.rate)));
                },
                _ => {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::InvalidModifierValue,
                        DiagnosticPhase::Build,
                        "A nested `clip_duration` under a scaled function call must be a fixed finite time literal.",
                    ).with_ast_span(span));
                    scope.clip_duration = Some(current);
                },
            }
        } else {
            scope.time_scale = Some(Expr::Num(parent_scope.rate));
        }
    }
    if let Some(limit) = inherited_limit {
        scope.end_limit_duration_ms =
            Some(scope.end_limit_duration_ms.map_or(limit, |current| current.min(limit)));
    }
    Some(scope)
}

pub(super) fn local_time_expr(scope: TimeScope) -> Expr {
    Expr::Binary(
        Box::new(Expr::Binary(
            Box::new(Expr::Ident("t".into())),
            BinaryOp::Sub,
            Box::new(Expr::Num(scope.origin_ms / 1000.0)),
        )),
        BinaryOp::Div,
        Box::new(Expr::Num(scope.rate)),
    )
}

pub(super) fn local_plot_time_expr(scope: TimeScope) -> Expr {
    let local_t = local_time_expr(scope);
    let Some(end_ms) = scope.end_ms else {
        return local_t;
    };
    let local_end_seconds = (end_ms - scope.origin_ms) / scope.rate / 1000.0;
    if !local_end_seconds.is_finite() || local_end_seconds < 0.0 {
        return local_t;
    }
    Expr::Call("clamp".into(), vec![local_t, Expr::Num(0.0), Expr::Num(local_end_seconds)])
}

pub(super) fn rewrite_expr_t(expr: Expr, local_t: &Expr, shadowed: bool) -> Expr {
    match expr {
        Expr::Ident(name) if name == "t" && !shadowed => local_t.clone(),
        Expr::Tuple(items) => Expr::Tuple(
            items.into_iter().map(|item| rewrite_expr_t(item, local_t, shadowed)).collect(),
        ),
        Expr::List(items) => Expr::List(
            items.into_iter().map(|item| rewrite_expr_t(item, local_t, shadowed)).collect(),
        ),
        Expr::Index(target, index) => Expr::Index(
            Box::new(rewrite_expr_t(*target, local_t, shadowed)),
            Box::new(rewrite_expr_t(*index, local_t, shadowed)),
        ),
        Expr::Binary(left, op, right) => Expr::Binary(
            Box::new(rewrite_expr_t(*left, local_t, shadowed)),
            op,
            Box::new(rewrite_expr_t(*right, local_t, shadowed)),
        ),
        Expr::Unary(op, value) => {
            Expr::Unary(op, Box::new(rewrite_expr_t(*value, local_t, shadowed)))
        },
        Expr::Call(name, args) => Expr::Call(
            name,
            args.into_iter().map(|arg| rewrite_expr_t(arg, local_t, shadowed)).collect(),
        ),
        Expr::Method(receiver, name, args) => Expr::Method(
            Box::new(rewrite_expr_t(*receiver, local_t, shadowed)),
            name,
            args.into_iter().map(|arg| rewrite_expr_t(arg, local_t, shadowed)).collect(),
        ),
        Expr::Closure(params, body) => {
            let shadows = shadowed || params.iter().any(|name| name == "t");
            Expr::Closure(params, Box::new(rewrite_expr_t(*body, local_t, shadows)))
        },
        Expr::LetChain(bindings, tail) => {
            let mut shadows = shadowed;
            let bindings = bindings
                .into_iter()
                .map(|(name, value)| {
                    let value = rewrite_expr_t(value, local_t, shadows);
                    shadows |= name == "t";
                    (name, value)
                })
                .collect();
            Expr::LetChain(bindings, Box::new(rewrite_expr_t(*tail, local_t, shadows)))
        },
        Expr::Conditional(cond, yes, no) => Expr::Conditional(
            Box::new(rewrite_expr_t(*cond, local_t, shadowed)),
            Box::new(rewrite_expr_t(*yes, local_t, shadowed)),
            Box::new(rewrite_expr_t(*no, local_t, shadowed)),
        ),
        Expr::Match(value, arms) => Expr::Match(
            Box::new(rewrite_expr_t(*value, local_t, shadowed)),
            arms.into_iter()
                .map(|(pattern, expr)| {
                    (pattern, Box::new(rewrite_expr_t(*expr, local_t, shadowed)))
                })
                .collect(),
        ),
        Expr::Construct(name, props) => Expr::Construct(
            name,
            props
                .into_iter()
                .map(|prop| crate::ast::Property {
                    value: rewrite_expr_t(prop.value, local_t, shadowed),
                    ..prop
                })
                .collect(),
        ),
        other => other,
    }
}

pub(super) fn expr_has_free_t(expr: &Expr) -> bool {
    fn visit(expr: &Expr, shadowed: bool) -> bool {
        match expr {
            Expr::Ident(name) => name == "t" && !shadowed,
            Expr::Index(target, index) => visit(target, shadowed) || visit(index, shadowed),
            Expr::Tuple(items) | Expr::List(items) => {
                items.iter().any(|item| visit(item, shadowed))
            },
            Expr::Binary(left, _, right) => visit(left, shadowed) || visit(right, shadowed),
            Expr::Unary(_, value) => visit(value, shadowed),
            Expr::Call(_, args) => args.iter().any(|arg| visit(arg, shadowed)),
            Expr::Method(receiver, _, args) => {
                visit(receiver, shadowed) || args.iter().any(|arg| visit(arg, shadowed))
            },
            Expr::Closure(params, body) => {
                visit(body, shadowed || params.iter().any(|name| name == "t"))
            },
            Expr::LetChain(bindings, tail) => {
                let mut shadowed = shadowed;
                for (name, value) in bindings {
                    if visit(value, shadowed) {
                        return true;
                    }
                    shadowed |= name == "t";
                }
                visit(tail, shadowed)
            },
            Expr::Conditional(condition, yes, no) => {
                visit(condition, shadowed) || visit(yes, shadowed) || visit(no, shadowed)
            },
            Expr::Match(value, arms) => {
                visit(value, shadowed) || arms.iter().any(|(_, expr)| visit(expr, shadowed))
            },
            Expr::Construct(_, props) => props.iter().any(|prop| visit(&prop.value, shadowed)),
            Expr::Num(_) | Expr::Percent(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => false,
            Expr::Path(_) => false,
        }
    }
    visit(expr, false)
}

fn loop_binds_t(pattern: &crate::ast::LoopPattern) -> bool {
    match pattern {
        crate::ast::LoopPattern::Single(name) => name == "t",
        crate::ast::LoopPattern::Tuple(names) => names.iter().any(|name| name == "t"),
    }
}

fn is_unsupported_action(verb: &str) -> bool {
    matches!(verb, "play" | "idle" | "audio-play" | "play-audio")
}

fn is_unsupported_actor(ty: &str, props: &[crate::ast::Property]) -> bool {
    let ty = ty.to_ascii_lowercase();
    matches!(ty.as_str(), "audio" | "audiotrack" | "filter" | "glass" | "mask" | "shader")
        || props.iter().any(|prop| {
            matches!(prop.name.as_str(), "audio" | "filter" | "glass" | "shader" | "idle")
        })
}

fn unsupported_time_consumer(name: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::InvalidPropertyValue,
        DiagnosticPhase::Build,
        format!(
            "Locally retimed function does not support scene-clock consumer '{name}'; move it outside the function time scope."
        ),
    )
}
