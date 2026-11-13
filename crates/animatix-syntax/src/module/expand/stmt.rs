use super::*;
use crate::ast::{LoopPattern, Modifier, Span};

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
                time, body, span, ..
            } => {
                let expanded_body = self.expand_body(body);
                self.output.push(Stmt::Keyframe {
                    time: time.clone(),
                    body: expanded_body,
                    span: *span,
                });
            },
            Stmt::RelativeKeyframe {
                offset, body, span, ..
            } => {
                let expanded_body = self.expand_body(body);
                self.output.push(Stmt::RelativeKeyframe {
                    offset: offset.clone(),
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
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                span,
                ..
            } => {
                self.output.push(Stmt::ReactiveBinding {
                    target: target.clone(),
                    property: property.clone(),
                    value: value.clone(),
                    value_span: *value_span,
                    span: *span,
                });
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                span,
                ..
            } => {
                self.expand_conditional(condition, then_branch, else_branch.as_ref(), *span);
            },
            Stmt::Match {
                scrutinee,
                arms,
                span,
                ..
            } => {
                self.expand_match(scrutinee, arms, *span);
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
                self.expand_for_loop(var, index_var, iterable, body, modifiers, *span);
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
                    modifiers: modifiers.clone(),
                    body: expanded_body,
                    span: *span,
                });
            },
            // Instance functions are consumed during component expansion; module-
            // level functions survive so the runtime can seed pure functions.
            Stmt::FnDecl { .. } => self.output.push(stmt.clone()),
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
                    config: config.clone(),
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
                self.expand_actor_decl(
                    *is_pub,
                    label,
                    array_index,
                    ty,
                    props,
                    modifiers,
                    children,
                    *span,
                );
            },
            // Pass-through variants: carried into the expanded output unchanged.
            // (Actions and let-decls can reference instance labels but never
            // need child expansion; Keyframe/Always/etc. reach here via
            // expand_statements' own recursion into scene bodies; Import/
            // TypeAlias/Play are structural and never contain instances.)
            Stmt::Action(..)
            | Stmt::LetDecl { .. }
            | Stmt::Assignment { .. }
            | Stmt::Block { .. }
            | Stmt::Return { .. }
            | Stmt::Expr(..)
            | Stmt::Config { .. }
            | Stmt::Step { .. }
            | Stmt::Comment(..)
            | Stmt::Import { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Play { .. } => self.output.push(stmt.clone()),
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
            let (instance_stmts, instance_registry) = expand_component_instance(
                label,
                array_index,
                props,
                children,
                component,
                self.components,
                self.ctx,
            );
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
                props: props.to_vec(),
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
