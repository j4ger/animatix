use super::{
    ComponentEntry, Expr, HashMap, HashSet, InlineItem, InstanceActionRegistry, MatchPattern, ParamDef,
    Property, Stmt,
    rewrite::rewrite_stmt,
};

/// Maximum nesting depth for component expansion before reporting a cycle error.
const MAX_EXPANSION_DEPTH: usize = 64;

/// Mutable state threaded through the component expansion pipeline.
struct ExpansionCtx {
    /// Monotonically increasing counter for generating unique anonymous labels.
    anon_counter: usize,
    /// Current component nesting depth (for cycle detection).
    depth: usize,
}

impl ExpansionCtx {
    fn new() -> Self {
        Self {
            anon_counter: 0,
            depth: 0,
        }
    }

    /// Generate a unique label for an anonymous component instance.
    fn next_anon_label(&mut self) -> String {
        let label = format!("__anon_{}", self.anon_counter);
        self.anon_counter += 1;
        label
    }
}

pub(super) fn expand_statements(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
) -> (Vec<Stmt>, InstanceActionRegistry) {
    let mut ctx = ExpansionCtx::new();
    let mut expanded = Vec::new();
    let mut registry = InstanceActionRegistry::new();
    for stmt in statements {
        expand_stmt_into(stmt, components, &mut expanded, &mut registry, &mut ctx);
    }
    (expanded, registry)
}

fn expand_stmt_into(
    stmt: &Stmt,
    components: &HashMap<String, ComponentEntry>,
    output: &mut Vec<Stmt>,
    registry: &mut InstanceActionRegistry,
    ctx: &mut ExpansionCtx,
) {
    match stmt {
        Stmt::Keyframe { time, body, span, .. } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Keyframe {
                time: time.clone(),
                body: expanded_body,
                span: *span,
            });
        }
        Stmt::RelativeKeyframe { offset, body, span, .. } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::RelativeKeyframe {
                offset: offset.clone(),
                body: expanded_body,
                span: *span,
            });
        }
        Stmt::Always { body, span, .. } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Always {
                body: expanded_body,
                span: *span,
            });
        }
        Stmt::ReactiveBinding { target, property, value, value_span, span, .. } => {
            output.push(Stmt::ReactiveBinding {
                target: target.clone(),
                property: property.clone(),
                value: value.clone(),
                value_span: *value_span,
                span: *span,
            });
        }
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            span,
            ..
        } => {
            let (then_expanded, then_registry) = expand_statements_inner(then_branch, components, ctx);
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
        }
        Stmt::Match {
            scrutinee,
            arms,
            span,
            ..
        } => {
            let expanded_arms: Vec<(MatchPattern, Vec<Stmt>)> = arms
                .iter()
                .map(|(pat, body)| {
                    let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
                    merge_registry(registry, sub_registry);
                    (pat.clone(), expanded_body)
                })
                .collect();
            output.push(Stmt::Match {
                scrutinee: scrutinee.clone(),
                arms: expanded_arms,
                span: *span,
            });
        }
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
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
                span: *span,
            });
        }
        Stmt::Sequence { body, span, .. } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Sequence {
                body: expanded_body,
                span: *span,
            });
        }
        Stmt::Stagger { modifiers, body, span, .. } => {
            let (expanded_body, sub_registry) = expand_statements_inner(body, components, ctx);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Stagger {
                modifiers: modifiers.clone(),
                body: expanded_body,
                span: *span,
            });
        }
        // ComponentAction is NOT emitted into output; it's collected during instance expansion
        Stmt::ComponentAction { .. } => {}
        Stmt::ComponentDef(..) => {}
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
                let (instance_stmts, instance_registry) = expand_component_instance(
                    label, props, children, component, components, ctx,
                );
                merge_registry(registry, instance_registry);
                output.extend(instance_stmts);
            } else {
                let expanded_children = expand_inline_items(children, components, registry, ctx);
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
            }
        }
        _ => output.push(stmt.clone()),
    }
}

fn merge_registry(
    target: &mut InstanceActionRegistry,
    source: InstanceActionRegistry,
) {
    for (label, actions) in source {
        target.insert(label, actions);
    }
}

/// Internal entry point that shares the [`ExpansionCtx`] across recursive calls.
fn expand_statements_inner(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
    ctx: &mut ExpansionCtx,
) -> (Vec<Stmt>, InstanceActionRegistry) {
    let mut expanded = Vec::new();
    let mut registry = InstanceActionRegistry::new();
    for stmt in statements {
        expand_stmt_into(stmt, components, &mut expanded, &mut registry, ctx);
    }
    (expanded, registry)
}

/// Recursively expand component instances inside inline items (container children).
fn expand_inline_items(
    items: &[InlineItem],
    components: &HashMap<String, ComponentEntry>,
    registry: &mut InstanceActionRegistry,
    ctx: &mut ExpansionCtx,
) -> Vec<InlineItem> {
    let mut result = Vec::new();
    for item in items {
        match item {
            InlineItem::Labeled {
                label,
                array_index,
                ty,
                props,
                modifiers,
                children,
            } => {
                if let Some(component) = components.get(ty) {
                    let (instance_stmts, instance_registry) = expand_component_instance(
                        label, props, children, component, components, ctx,
                    );
                    merge_registry(registry, instance_registry);
                    for stmt in instance_stmts {
                        if let Some(inline) = stmt_to_inline_item(&stmt) {
                            result.push(inline);
                        }
                    }
                } else {
                    let expanded_children = expand_inline_items(children, components, registry, ctx);
                    result.push(InlineItem::Labeled {
                        label: label.clone(),
                        array_index: array_index.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: expanded_children,
                    });
                }
            }
            InlineItem::Anonymous {
                ty,
                props,
                modifiers,
                children,
            } => {
                if let Some(component) = components.get(ty) {
                    let anon_label = ctx.next_anon_label();
                    let (instance_stmts, instance_registry) = expand_component_instance(
                        &anon_label, props, children, component, components, ctx,
                    );
                    merge_registry(registry, instance_registry);
                    for stmt in instance_stmts {
                        if let Some(inline) = stmt_to_inline_item(&stmt) {
                            result.push(inline);
                        }
                    }
                } else {
                    let expanded_children = expand_inline_items(children, components, registry, ctx);
                    result.push(InlineItem::Anonymous {
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: expanded_children,
                    });
                }
            }
            InlineItem::ForLoop {
                var,
                index_var,
                iterable,
                body,
            } => {
                let expanded_body = expand_inline_items(body, components, registry, ctx);
                result.push(InlineItem::ForLoop {
                    var: var.clone(),
                    index_var: index_var.clone(),
                    iterable: iterable.clone(),
                    body: expanded_body,
                });
            }
            InlineItem::SlotMarker => result.push(InlineItem::SlotMarker),
            InlineItem::SlotFill { slot, items } => {
                let expanded = expand_inline_items(items, components, registry, ctx);
                result.push(InlineItem::SlotFill {
                    slot: slot.clone(),
                    items: expanded,
                });
            }
        }
    }
    result
}

/// Convert a statement back into an inline item for container children.
fn stmt_to_inline_item(stmt: &Stmt) -> Option<InlineItem> {
    match stmt {
        Stmt::ActorDecl {
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
            ..
        } => Some(InlineItem::Labeled {
            label: label.clone(),
            array_index: array_index.clone(),
            ty: ty.clone(),
            props: props.clone(),
            modifiers: modifiers.clone(),
            children: children.clone(),
        }),
        _ => None,
    }
}

fn expand_component_instance(
    instance_label: &str,
    instance_props: &[Property],
    instance_children: &[InlineItem],
    component: &ComponentEntry,
    components: &HashMap<String, ComponentEntry>,
    ctx: &mut ExpansionCtx,
) -> (Vec<Stmt>, InstanceActionRegistry) {
    // Cycle detection: bail out if nesting is too deep.
    if ctx.depth >= MAX_EXPANSION_DEPTH {
        tracing::error!(
            "Component expansion depth limit ({}) reached at '{}'. \
             Possible circular component reference.",
            MAX_EXPANSION_DEPTH, instance_label
        );
        return (Vec::new(), InstanceActionRegistry::new());
    }
    ctx.depth += 1;

    let bindings = component_bindings(&component.definition.params, instance_props);
    let root_label = first_labeled_stmt(&component.definition.body);
    let known_labels = collect_labels(&component.definition.body);

    // Extract slot fills from instance children (keyed by original slot name)
    let mut slot_fills: HashMap<String, Vec<InlineItem>> = HashMap::new();
    for item in instance_children {
        if let InlineItem::SlotFill { slot, items } = item {
            slot_fills.insert(slot.clone(), items.clone());
        }
    }

    // Resolve slots on original component body BEFORE rewriting labels
    let resolved = resolve_slots(&component.definition.body, &slot_fills);

    let rewritten = resolved
        .iter()
        .map(|stmt| {
            rewrite_stmt(
                stmt,
                instance_label,
                root_label.as_deref(),
                &known_labels,
                &bindings,
            )
        })
        .collect::<Vec<_>>();

    // Collect custom actions from rewritten statements
    let mut instance_actions: HashMap<String, crate::module::ActionTemplate> = HashMap::new();
    let filtered: Vec<Stmt> = rewritten
        .into_iter()
        .filter_map(|stmt| match &stmt {
            Stmt::ComponentAction { name, params, body, .. } => {
                instance_actions.insert(
                    name.clone(),
                    crate::module::ActionTemplate {
                        params: params.clone(),
                        body: body.clone(),
                    },
                );
                None
            }
            _ => Some(stmt),
        })
        .collect();

    let (expanded_stmts, mut registry) = expand_statements_inner(&filtered, components, ctx);
    if !instance_actions.is_empty() {
        registry.insert(instance_label.to_string(), instance_actions);
    }

    ctx.depth -= 1;
    (expanded_stmts, registry)
}

fn component_bindings(params: &[ParamDef], instance_props: &[Property]) -> HashMap<String, Expr> {
    let mut bindings = HashMap::new();

    for param in params {
        if let Some(default) = &param.default {
            bindings.insert(param.name.clone(), default.clone());
        }
    }

    for prop in instance_props {
        bindings.insert(prop.name.clone(), prop.value.clone());
    }

    bindings
}

fn first_labeled_stmt(body: &[Stmt]) -> Option<String> {
    for stmt in body {
        if let Stmt::ActorDecl { label, .. } = stmt { return Some(label.clone()) }
    }
    None
}

fn collect_labels(body: &[Stmt]) -> HashSet<String> {
    let mut labels = HashSet::new();
    crate::walk::walk_stmts(body, &mut |stmt| {
        match stmt {
            Stmt::ActorDecl { label, .. } => {
                labels.insert(label.clone());
            }
            Stmt::ReactiveBinding { target, .. } => {
                if let Some(label) = target.first() {
                    labels.insert(label.clone());
                }
            }
            _ => {}
        }
    });
    labels
}

fn has_slot_marker(children: &[InlineItem]) -> bool {
    children
        .iter()
        .any(|item| matches!(item, InlineItem::SlotMarker))
}

fn resolve_slots(
    stmts: &[Stmt],
    slot_fills: &HashMap<String, Vec<InlineItem>>,
) -> Vec<Stmt> {
    stmts
        .iter()
        .map(|stmt| match stmt {
            Stmt::ActorDecl {
                is_pub,
                label,
                array_index,
                ty,
                props,
                modifiers,
                children,
                ..
            } => {
                if has_slot_marker(children) {
                    // Collect non-slot defaults from the container
                    let defaults: Vec<InlineItem> = children
                        .iter()
                        .filter(|item| !matches!(item, InlineItem::SlotMarker))
                        .cloned()
                        .collect();

                    // Get fill items by original slot name
                    let fill_items = slot_fills.get(label);

                    let replacement = if let Some(items) = fill_items {
                        items.clone()
                    } else if !defaults.is_empty() {
                        defaults
                    } else {
                        // Required slot, no fill, no defaults
                        Vec::new()
                    };

                    Stmt::ActorDecl {
                        is_pub: *is_pub,
                        is_anonymous: false,
                        label: label.clone(),
                        array_index: array_index.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: replacement,
                        span: None,
                    }
                } else {
                    stmt.clone()
                }
            }
            _ => stmt.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parser_simple;

    #[test]
    fn test_expand_preserves_forloop_index_var() {
        // Regression: for-loop must preserve index_var through component expansion.
        // The `for v, k in {50, 150, 250} { ... }` form must keep `k` after expansion.
        let source = "for v, k in {50, 150, 250} { a[k]: Rect, size: (20,40), at: (v, 100) }";
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();
        let components = std::collections::HashMap::new();
        let (expanded, _registry) = expand_statements(&stmts, &components);
        // Find the ForLoop
        let for_stmt = expanded.iter().find(|s| matches!(s, Stmt::ForLoop { .. })).unwrap();
        if let Stmt::ForLoop { index_var, .. } = for_stmt {
            assert_eq!(index_var.as_deref(), Some("k"), "index_var must be preserved through expansion");
        } else {
            panic!("Expected ForLoop");
        }
        // Find `a[k]` ActorDecl inside the ForLoop body
        if let Stmt::ForLoop { body, .. } = for_stmt {
            let act = body.iter().find_map(|s| {
                if let Stmt::ActorDecl { array_index: Some(Expr::Ident(name)), .. } = s {
                    Some(name.clone())
                } else {
                    None
                }
            });
            assert_eq!(act.as_deref(), Some("k"), "array_index must be preserved through expansion");
        }
    }
}
