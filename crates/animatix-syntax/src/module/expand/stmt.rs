use super::*;
use crate::ast::{LoopPattern, Modifier, Span, Time};
use crate::visit::{AstFolder, walk_fold_expr};

/// Component expansion for plans nested inside ordinary expressions. The
/// returned method registry stays attached to its plan instead of being
/// merged into the surrounding lexical scope.
struct PlanExprExpander<'a> {
    components: &'a HashMap<String, ComponentEntry>,
    ctx: &'a mut ExpansionCtx,
}

impl AstFolder for PlanExprExpander<'_> {
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        match expr {
            Expr::Timeline(plan) => Expr::Timeline(self.expand_plan(plan)),
            Expr::Sequence(plan) => Expr::Sequence(self.expand_plan(plan)),
            Expr::Stagger { modifiers, plan } => Expr::Stagger {
                modifiers: modifiers
                    .into_iter()
                    .map(|modifier| self.fold_modifier(modifier))
                    .collect(),
                plan: self.expand_plan(plan),
            },
            other => walk_fold_expr(self, other),
        }
    }
}

impl PlanExprExpander<'_> {
    fn expand_plan(&mut self, plan: crate::ast::PlanBody) -> crate::ast::PlanBody {
        let (statements, mut instance_methods) =
            expand_statements_inner(&plan.statements, self.components, self.ctx);
        merge_registry(&mut instance_methods, plan.instance_methods);
        crate::ast::PlanBody {
            statements,
            instance_methods,
        }
    }
}

pub(super) fn expand_expr(
    expr: &Expr,
    components: &HashMap<String, ComponentEntry>,
    ctx: &mut ExpansionCtx,
) -> Expr {
    PlanExprExpander { components, ctx }.fold_expr(expr.clone())
}

pub(super) fn expand_embedded_plans_in_stmts(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
    ctx: &mut ExpansionCtx,
) -> Vec<Stmt> {
    PlanExprExpander { components, ctx }.fold_stmts(statements.to_vec())
}

fn expand_time(
    time: &Time,
    components: &HashMap<String, ComponentEntry>,
    ctx: &mut ExpansionCtx,
) -> Time {
    match time {
        Time::Expression(expr) => Time::Expression(Box::new(expand_expr(expr, components, ctx))),
        other => other.clone(),
    }
}

pub(super) fn expand_stmt_into(
    stmt: &Stmt,
    components: &HashMap<String, ComponentEntry>,
    output: &mut Vec<Stmt>,
    registry: &mut InstanceFnRegistry,
    ctx: &mut ExpansionCtx,
) {
    StmtExpander {
        components,
        output,
        registry,
        ctx,
    }
    .expand(stmt);
}

/// Statement-expansion state, so each variant group can live in its own helper.
struct StmtExpander<'a> {
    components: &'a HashMap<String, ComponentEntry>,
    output: &'a mut Vec<Stmt>,
    registry: &'a mut InstanceFnRegistry,
    ctx: &'a mut ExpansionCtx,
}

impl<'a> StmtExpander<'a> {
    fn expand(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::Keyframe {
                time,
                cue_group,
                body,
                span,
            } => {
                let expanded_body = self.expand_body(body);
                self.output.push(Stmt::Keyframe {
                    time: expand_time(time, self.components, self.ctx),
                    cue_group: cue_group.clone(),
                    body: expanded_body,
                    span: *span,
                });
            },
            Stmt::RelativeKeyframe {
                offset,
                cue_group,
                body,
                span,
            } => {
                let expanded_body = self.expand_body(body);
                self.output.push(Stmt::RelativeKeyframe {
                    offset: expand_time(offset, self.components, self.ctx),
                    cue_group: cue_group.clone(),
                    body: expanded_body,
                    span: *span,
                });
            },
            Stmt::Always { body, span, .. } => {
                let expanded_body = self.expand_body(body);
                self.output.push(Stmt::Always {
                    body: expanded_body,
                    span: *span,
                });
            },
            Stmt::ReactiveBinding { .. } => self.expand_leaf(stmt),
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                span,
                ..
            } => {
                let condition = expand_expr(condition, self.components, self.ctx);
                self.expand_conditional(&condition, then_branch, else_branch.as_ref(), *span);
            },
            Stmt::Match {
                scrutinee,
                arms,
                span,
                ..
            } => {
                let scrutinee = expand_expr(scrutinee, self.components, self.ctx);
                self.expand_match(&scrutinee, arms, *span);
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                modifiers,
                span,
                ..
            } => {
                let iterable = expand_expr(iterable, self.components, self.ctx);
                let modifiers = modifiers
                    .iter()
                    .map(|modifier| Modifier {
                        name: modifier.name.clone(),
                        value: expand_expr(&modifier.value, self.components, self.ctx),
                    })
                    .collect::<Vec<_>>();
                self.expand_for_loop(var, index_var, &iterable, body, &modifiers, *span);
            },
            Stmt::Sequence { body, span, .. } => {
                let expanded_body = self.expand_body(body);
                self.output.push(Stmt::Sequence {
                    body: expanded_body,
                    span: *span,
                });
            },
            Stmt::Stagger {
                modifiers,
                body,
                span,
                ..
            } => {
                let expanded_body = self.expand_body(body);
                self.output.push(Stmt::Stagger {
                    modifiers: modifiers
                        .iter()
                        .map(|modifier| Modifier {
                            name: modifier.name.clone(),
                            value: expand_expr(&modifier.value, self.components, self.ctx),
                        })
                        .collect(),
                    body: expanded_body,
                    span: *span,
                });
            },
            // Instance functions are consumed during component expansion; module-
            // level functions survive so the runtime can seed pure functions.
            Stmt::FnDecl { .. } => self.expand_leaf(stmt),
            Stmt::ComponentDef(..) => {},
            Stmt::Scene {
                name,
                config,
                body,
                span,
            } => {
                let expanded_body = self.expand_body(body);
                self.output.push(Stmt::Scene {
                    name: name.clone(),
                    config: config
                        .iter()
                        .map(|property| Property {
                            name: property.name.clone(),
                            value: expand_expr(&property.value, self.components, self.ctx),
                            trailing_comment: property.trailing_comment.clone(),
                            value_span: property.value_span,
                        })
                        .collect(),
                    body: expanded_body,
                    span: *span,
                });
            },
            Stmt::ActorDecl {
                is_pub,
                label,
                array_index,
                ty,
                props,
                modifiers,
                children,
                span,
                ..
            } => {
                let array_index =
                    array_index.as_ref().map(|index| expand_expr(index, self.components, self.ctx));
                let props = props
                    .iter()
                    .map(|property| Property {
                        name: property.name.clone(),
                        value: expand_expr(&property.value, self.components, self.ctx),
                        trailing_comment: property.trailing_comment.clone(),
                        value_span: property.value_span,
                    })
                    .collect::<Vec<_>>();
                let modifiers = modifiers
                    .iter()
                    .map(|modifier| Modifier {
                        name: modifier.name.clone(),
                        value: expand_expr(&modifier.value, self.components, self.ctx),
                    })
                    .collect::<Vec<_>>();
                let children = children
                    .iter()
                    .cloned()
                    .map(|child| {
                        PlanExprExpander {
                            components: self.components,
                            ctx: self.ctx,
                        }
                        .fold_inline_item(child)
                    })
                    .collect::<Vec<_>>();
                self.expand_actor_decl(
                    *is_pub,
                    label,
                    &array_index,
                    ty,
                    &props,
                    &modifiers,
                    &children,
                    *span,
                );
            },
            _ => self.expand_leaf(stmt),
        }
    }

    fn expand_leaf(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                span,
                ..
            } => self.output.push(Stmt::ReactiveBinding {
                target: target
                    .iter()
                    .map(|segment| match segment {
                        TargetSegment::Static(label) => TargetSegment::Static(label.clone()),
                        TargetSegment::Indexed { base, index } => TargetSegment::Indexed {
                            base: base.clone(),
                            index: Box::new(expand_expr(index, self.components, self.ctx)),
                        },
                    })
                    .collect(),
                property: property.clone(),
                value: expand_expr(value, self.components, self.ctx),
                value_span: *value_span,
                span: *span,
            }),
            Stmt::FnDecl {
                is_pub,
                name,
                params,
                return_type,
                body,
                span,
            } => self.output.push(Stmt::FnDecl {
                is_pub: *is_pub,
                name: name.clone(),
                params: params
                    .iter()
                    .map(|param| crate::ast::ParamDef {
                        name: param.name.clone(),
                        param_type: param.param_type.clone(),
                        default: param
                            .default
                            .as_ref()
                            .map(|value| expand_expr(value, self.components, self.ctx)),
                    })
                    .collect(),
                return_type: return_type.clone(),
                body: expand_embedded_plans_in_stmts(body, self.components, self.ctx),
                span: *span,
            }),
            Stmt::LetDecl {
                is_pub,
                name,
                value,
                span,
            } => self.output.push(Stmt::LetDecl {
                is_pub: *is_pub,
                name: name.clone(),
                value: expand_expr(value, self.components, self.ctx),
                span: *span,
            }),
            Stmt::Assignment {
                target,
                property,
                value,
                modifiers,
                easing,
                value_span,
                span,
            } => self.output.push(Stmt::Assignment {
                target: target
                    .iter()
                    .map(|segment| match segment {
                        TargetSegment::Static(label) => TargetSegment::Static(label.clone()),
                        TargetSegment::Indexed { base, index } => TargetSegment::Indexed {
                            base: base.clone(),
                            index: Box::new(expand_expr(index, self.components, self.ctx)),
                        },
                    })
                    .collect(),
                property: property.clone(),
                value: expand_expr(value, self.components, self.ctx),
                modifiers: modifiers
                    .iter()
                    .map(|modifier| Modifier {
                        name: modifier.name.clone(),
                        value: expand_expr(&modifier.value, self.components, self.ctx),
                    })
                    .collect(),
                easing: *easing,
                value_span: *value_span,
                span: *span,
            }),
            Stmt::Block {
                body,
                instance_scope,
                span,
            } => self.output.push(Stmt::Block {
                body: expand_embedded_plans_in_stmts(body, self.components, self.ctx),
                instance_scope: instance_scope.clone(),
                span: *span,
            }),
            Stmt::Return { value, span } => self.output.push(Stmt::Return {
                value: value.as_ref().map(|value| expand_expr(value, self.components, self.ctx)),
                span: *span,
            }),
            Stmt::Expr(expr, span) => self
                .output
                .push(Stmt::Expr(expand_expr(expr, self.components, self.ctx), *span)),
            Stmt::Config { settings, span } => self.output.push(Stmt::Config {
                settings: settings
                    .iter()
                    .map(|property| Property {
                        name: property.name.clone(),
                        value: expand_expr(&property.value, self.components, self.ctx),
                        trailing_comment: property.trailing_comment.clone(),
                        value_span: property.value_span,
                    })
                    .collect(),
                span: *span,
            }),
            Stmt::Step { .. }
            | Stmt::Comment(..)
            | Stmt::Import { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Play { .. } => self.output.push(stmt.clone()),
            _ => self.output.push(stmt.clone()),
        }
    }

    /// Expand a nested statement body, merging any instance-function registry.
    fn expand_body(&mut self, body: &[Stmt]) -> Vec<Stmt> {
        let (expanded_body, sub_registry) =
            expand_statements_inner(body, self.components, self.ctx);
        merge_registry(self.registry, sub_registry);
        expanded_body
    }

    fn expand_conditional(
        &mut self,
        condition: &Expr,
        then_branch: &[Stmt],
        else_branch: Option<&Vec<Stmt>>,
        span: Option<Span>,
    ) {
        let then_expanded = self.expand_body(then_branch);
        let else_expanded = else_branch.map(|branch| self.expand_body(branch));
        self.output.push(Stmt::Conditional {
            condition: condition.clone(),
            then_branch: then_expanded,
            else_branch: else_expanded,
            span,
        });
    }

    fn expand_match(
        &mut self,
        scrutinee: &Expr,
        arms: &[(MatchPattern, Vec<Stmt>)],
        span: Option<Span>,
    ) {
        let expanded_arms: Vec<(MatchPattern, Vec<Stmt>)> = arms
            .iter()
            .map(|(pat, body)| {
                let expanded_body = self.expand_body(body);
                (pat.clone(), expanded_body)
            })
            .collect();
        self.output.push(Stmt::Match {
            scrutinee: scrutinee.clone(),
            arms: expanded_arms,
            span,
        });
    }

    fn expand_for_loop(
        &mut self,
        var: &LoopPattern,
        index_var: &Option<String>,
        iterable: &Expr,
        body: &[Stmt],
        modifiers: &[Modifier],
        span: Option<Span>,
    ) {
        let expanded_body = self.expand_body(body);
        self.output.push(Stmt::ForLoop {
            var: var.clone(),
            index_var: index_var.clone(),
            iterable: iterable.clone(),
            body: expanded_body,
            modifiers: modifiers.to_vec(),
            span,
        });
    }

    fn expand_actor_decl(
        &mut self,
        is_pub: bool,
        label: &str,
        array_index: &Option<Expr>,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        span: Option<Span>,
    ) {
        if let Some(component) = self.components.get(ty) {
            let first_diagnostic = self.ctx.diagnostics.len();
            let (instance_stmts, instance_registry) = expand_component_instance(
                label,
                array_index,
                props,
                children,
                component,
                self.components,
                self.ctx,
            );
            for diagnostic in &mut self.ctx.diagnostics[first_diagnostic..] {
                if diagnostic.code == crate::diagnostics::DiagnosticCode::InvalidComponentSlot
                    && diagnostic.location.line.is_none()
                {
                    *diagnostic = diagnostic.clone().with_ast_span(span);
                }
            }
            let instance_stmts = carry_instance_array_index(instance_stmts, array_index);
            merge_registry(self.registry, instance_registry);
            self.output.extend(instance_stmts);
        } else {
            let (expanded_children, hoisted) =
                expand_inline_items(children, self.components, self.registry, self.ctx);
            self.output.push(Stmt::ActorDecl {
                is_pub,
                is_anonymous: false,
                label: label.to_string(),
                array_index: array_index.clone(),
                ty: ty.to_string(),
                props: expand_particle_shape(ty, props, self.components, self.ctx),
                modifiers: modifiers.to_vec(),
                children: expanded_children,
                span,
            });
            // Statements hoisted from component instances among the
            // children run right after the container declaration so their
            // dotted targets exist.
            self.output.extend(hoisted);
        }
    }
}

pub(super) fn merge_registry(target: &mut InstanceFnRegistry, source: InstanceFnRegistry) {
    for (label, actions) in source {
        target.insert(label, actions);
    }
}
