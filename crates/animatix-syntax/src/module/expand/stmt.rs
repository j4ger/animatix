use super::*;

pub(super) fn expand_stmt_into(
    stmt: &Stmt,
    components: &HashMap<String, ComponentEntry>,
    output: &mut Vec<Stmt>,
    registry: &mut InstanceFnRegistry,
    ctx: &mut ExpansionCtx,
) {
    match stmt {
        Stmt::Keyframe {
            time, body, span, ..
        } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Keyframe {
                time: time.clone(),
                body: expanded_body,
                span: *span,
            });
        },
        Stmt::RelativeKeyframe {
            offset, body, span, ..
        } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::RelativeKeyframe {
                offset: offset.clone(),
                body: expanded_body,
                span: *span,
            });
        },
        Stmt::Always { body, span, .. } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Always {
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
            output.push(Stmt::ReactiveBinding {
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
            let (then_expanded, then_registry) =
                expand_statements_inner(then_branch, components, ctx);
            merge_registry(registry, then_registry);
            let else_expanded = else_branch.as_ref().map(|branch| {
                let (expanded, sub_registry) = expand_statements_inner(branch, components, ctx);
                merge_registry(registry, sub_registry);
                expanded
            });
            output.push(Stmt::Conditional {
                condition: condition.clone(),
                then_branch: then_expanded,
                else_branch: else_expanded,
                span: *span,
            });
        },
        Stmt::Match {
            scrutinee,
            arms,
            span,
            ..
        } => {
            let expanded_arms: Vec<(MatchPattern, Vec<Stmt>)> = arms
                .iter()
                .map(|(pat, body)| {
                    let (expanded_body, sub_registry) =
                        expand_statements_inner(body, components, ctx);
                    merge_registry(registry, sub_registry);
                    (pat.clone(), expanded_body)
                })
                .collect();
            output.push(Stmt::Match {
                scrutinee: scrutinee.clone(),
                arms: expanded_arms,
                span: *span,
            });
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
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::ForLoop {
                var: var.clone(),
                index_var: index_var.clone(),
                iterable: iterable.clone(),
                body: expanded_body,
                modifiers: modifiers.clone(),
                span: *span,
            });
        },
        Stmt::Sequence { body, span, .. } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Sequence {
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
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Stagger {
                modifiers: modifiers.clone(),
                body: expanded_body,
                span: *span,
            });
        },
        // Instance functions are consumed during component expansion; module-
        // level functions survive so the runtime can seed pure functions.
        Stmt::FnDecl { .. } => output.push(stmt.clone()),
        Stmt::ComponentDef(..) => {},
        Stmt::Scene {
            name,
            config,
            body,
            span,
        } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Scene {
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
            if let Some(component) = components.get(ty) {
                let (instance_stmts, instance_registry) =
                    expand_component_instance(label, props, children, component, components, ctx);
                merge_registry(registry, instance_registry);
                output.extend(instance_stmts);
            } else {
                let (expanded_children, hoisted) =
                    expand_inline_items(children, components, registry, ctx);
                output.push(Stmt::ActorDecl {
                    is_pub: *is_pub,
                    is_anonymous: false,
                    label: label.clone(),
                    array_index: array_index.clone(),
                    ty: ty.clone(),
                    props: props.clone(),
                    modifiers: modifiers.clone(),
                    children: expanded_children,
                    span: *span,
                });
                // Statements hoisted from component instances among the
                // children run right after the container declaration so their
                // dotted targets exist.
                output.extend(hoisted);
            }
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
        | Stmt::Play { .. } => output.push(stmt.clone()),
    }
}

pub(super) fn merge_registry(target: &mut InstanceFnRegistry, source: InstanceFnRegistry) {
    for (label, actions) in source {
        target.insert(label, actions);
    }
}
