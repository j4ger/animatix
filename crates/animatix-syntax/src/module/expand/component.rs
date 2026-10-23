use super::*;

pub(super) fn expand_statements_inner(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
    ctx: &mut ExpansionCtx,
) -> (Vec<Stmt>, InstanceFnRegistry) {
    let mut expanded = Vec::new();
    let mut registry = InstanceFnRegistry::new();
    for stmt in statements {
        expand_stmt_into(stmt, components, &mut expanded, &mut registry, ctx);
    }
    (expanded, registry)
}

/// Recursively expand component instances inside inline items (container children).
///
/// Returns the expanded inline items plus any statements hoisted out of the
/// container: component bodies may contain non-declaration statements
/// (`always`, assignments, reactive bindings, ...) which have no inline form.
/// They are rewritten to the instance's dotted labels and must be appended to
/// the enclosing statement list by the caller instead of being dropped.
pub(super) fn expand_inline_items(
    items: &[InlineItem],
    components: &HashMap<String, ComponentEntry>,
    registry: &mut InstanceFnRegistry,
    ctx: &mut ExpansionCtx,
) -> (Vec<InlineItem>, Vec<Stmt>) {
    let mut result = Vec::new();
    let mut hoisted = Vec::new();
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
                        match stmt_to_inline_item(&stmt) {
                            Some(inline) => result.push(inline),
                            // Component-internal statements (always,
                            // assignments, ...) survive via hoisting.
                            None => hoisted.push(stmt),
                        }
                    }
                } else {
                    let (expanded_children, nested_hoisted) =
                        expand_inline_items(children, components, registry, ctx);
                    hoisted.extend(nested_hoisted);
                    result.push(InlineItem::Labeled {
                        label: label.clone(),
                        array_index: array_index.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: expanded_children,
                    });
                }
            },
            InlineItem::Anonymous {
                ty,
                props,
                modifiers,
                children,
            } => {
                if let Some(component) = components.get(ty) {
                    let anon_label = ctx.next_anon_label();
                    let (instance_stmts, instance_registry) = expand_component_instance(
                        &anon_label,
                        props,
                        children,
                        component,
                        components,
                        ctx,
                    );
                    merge_registry(registry, instance_registry);
                    for stmt in instance_stmts {
                        match stmt_to_inline_item(&stmt) {
                            Some(inline) => result.push(inline),
                            None => hoisted.push(stmt),
                        }
                    }
                } else {
                    let (expanded_children, nested_hoisted) =
                        expand_inline_items(children, components, registry, ctx);
                    hoisted.extend(nested_hoisted);
                    result.push(InlineItem::Anonymous {
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: expanded_children,
                    });
                }
            },
            InlineItem::ForLoop {
                var,
                index_var,
                iterable,
                body,
            } => {
                let (expanded_body, nested_hoisted) =
                    expand_inline_items(body, components, registry, ctx);
                // Statements hoisted from inside a for-loop body escape to the
                // enclosing statement list (a for-loop inline item cannot hold
                // statements); they run once at build time.
                hoisted.extend(nested_hoisted);
                result.push(InlineItem::ForLoop {
                    var: var.clone(),
                    index_var: index_var.clone(),
                    iterable: iterable.clone(),
                    body: expanded_body,
                });
            },
            InlineItem::SlotMarker => result.push(InlineItem::SlotMarker),
            InlineItem::SlotFill { slot, items } => {
                let (expanded, nested_hoisted) =
                    expand_inline_items(items, components, registry, ctx);
                hoisted.extend(nested_hoisted);
                result.push(InlineItem::SlotFill {
                    slot: slot.clone(),
                    items: expanded,
                });
            },
        }
    }
    (result, hoisted)
}

/// Convert a statement back into an inline item for container children.
pub(super) fn stmt_to_inline_item(stmt: &Stmt) -> Option<InlineItem> {
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
        // Intentional: only actor declarations have an inline form. Callers
        // filter_map over this, so non-actor statements (actions, lets,
        // keyframes, ...) simply do not participate in inline-item contexts.
        _ => None,
    }
}

/// Universal actor properties that may be written directly on a component
/// instance. They are forwarded onto the expanded root actor (which carries
/// the instance label) so instance-level placement and visibility work
/// without wrapping the instance in a `Group`. Props sharing a name with a
/// component parameter remain template bindings, not forwards.
pub(crate) const INSTANCE_FORWARDED_PROPS: &[&str] = &["opacity", "at", "anchor", "offset"];

pub(super) fn expand_component_instance(
    instance_label: &str,
    instance_props: &[Property],
    instance_children: &[InlineItem],
    component: &ComponentEntry,
    components: &HashMap<String, ComponentEntry>,
    ctx: &mut ExpansionCtx,
) -> (Vec<Stmt>, InstanceFnRegistry) {
    // Cycle detection: bail out if nesting is too deep.
    if ctx.depth >= MAX_EXPANSION_DEPTH {
        tracing::error!(
            "Component expansion depth limit ({}) reached at '{}'. \
             Possible circular component reference.",
            MAX_EXPANSION_DEPTH,
            instance_label
        );
        return (Vec::new(), InstanceFnRegistry::new());
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
            rewrite_stmt(stmt, instance_label, root_label.as_deref(), &known_labels, &bindings)
        })
        .collect::<Vec<_>>();

    // Universal actor properties (opacity/at/anchor/offset) written on the
    // instance. These are forwarded onto the expanded root actor (single-root
    // body) or the wrapper Group (multi-root body) so instance-level
    // placement/visibility works without an extra hand-written wrapper. The
    // Group/root carries the instance label, and later duplicate props win
    // during the build, so appended forwards override the component's
    // internal defaults.
    let forwarded: Vec<Property> = instance_props
        .iter()
        .filter(|p| {
            INSTANCE_FORWARDED_PROPS.contains(&p.name.as_str())
                && !component.definition.params.iter().any(|param| param.name == p.name)
        })
        .cloned()
        .collect();

    // Collect timeline functions from rewritten statements
    let mut instance_actions: HashMap<String, crate::module::FnTemplate> = HashMap::new();
    let filtered: Vec<Stmt> = rewritten
        .into_iter()
        .filter_map(|stmt| match &stmt {
            Stmt::FnDecl {
                name,
                params,
                return_type,
                body,
                ..
            } => {
                instance_actions.insert(
                    name.clone(),
                    crate::module::FnTemplate {
                        params: params.clone(),
                        return_type: return_type.clone(),
                        body: body.clone(),
                    },
                );
                None
            },
            _ => Some(stmt),
        })
        .collect();

    let (expanded_stmts, mut registry) = expand_statements_inner(&filtered, components, ctx);
    if !instance_actions.is_empty() {
        registry.insert(instance_label.to_string(), instance_actions);
    }
    ctx.depth -= 1;

    // Split the expanded body into actor declarations (which can become
    // container children) and behavior statements (`always`, assignments, ...)
    // that must stay at the enclosing statement list.
    let mut actors: Vec<Stmt> = Vec::new();
    let mut hoisted: Vec<Stmt> = Vec::new();
    for stmt in expanded_stmts {
        if stmt_to_inline_item(&stmt).is_some() {
            actors.push(stmt);
        } else {
            hoisted.push(stmt);
        }
    }

    // Multi-statement component bodies previously expanded into sibling
    // top-level statements, orphaning the instance wrapper: the build created
    // the instance track (e.g. `card`) as a root node with an empty children
    // list while the remaining `card.*` statements became independent roots
    // (each seeded with the hidden-by-default pre-keyframe opacity). Wrap the
    // top-level actors in a Group labeled with the instance label so the build
    // links them under one parent — a single root, with opacity and transform
    // context shared across the whole subtree. The Group (not merely nesting
    // under the first actor) is required because a shape root (e.g. a `Rect`
    // frame) does not render children as a group; a generic Group container
    // does. It also receives the instance root's authored `size` so a parent
    // layout container sizes the instance cell correctly instead of collapsing
    // a zero-sized wrapper.
    if actors.len() > 1 {
        let mut children: Vec<InlineItem> = Vec::new();
        let mut root_size: Option<Property> = None;
        for mut stmt in actors {
            if let Stmt::ActorDecl { label, props, .. } = &mut stmt {
                if label == instance_label {
                    // The component's first (root) statement carries the bare
                    // instance label after rewriting. Move that label onto a
                    // dotted child name so the wrapper Group can own the
                    // instance label without a duplicate-track collision.
                    // References to the instance root now resolve to the Group
                    // (the whole component), which is the correct target after
                    // the wrap.
                    if let Some(root) = &root_label {
                        *label = format!("{}.{}", instance_label, root);
                    }
                    // Snapshot the instance root's authored size so the wrapper
                    // Group carries the component's bounds for layout parents.
                    root_size = props.iter().find(|p| p.name == "size").cloned();
                }
            }
            if let Some(inline) = stmt_to_inline_item(&stmt) {
                children.push(inline);
            } else {
                hoisted.push(stmt);
            }
        }
        let mut group_props = forwarded;
        if let Some(size) = root_size {
            group_props.push(size);
        }
        let group = Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: instance_label.to_string(),
            array_index: None,
            ty: "Group".to_string(),
            props: group_props,
            modifiers: vec![],
            children,
            span: None,
        };
        let mut out = vec![group];
        out.extend(hoisted);
        return (out, registry);
    }

    // Single-root (or no-actor) body: the instance label lives on the root
    // actor. Forward the universal instance props onto it so instance-level
    // placement/visibility works without wrapping.
    if !forwarded.is_empty() {
        for stmt in actors.iter_mut().chain(hoisted.iter_mut()) {
            if let Stmt::ActorDecl { label, props, .. } = stmt {
                if label == instance_label {
                    props.extend(forwarded.iter().cloned());
                    break;
                }
            }
        }
    }

    let mut out = actors;
    out.extend(hoisted);
    (out, registry)
}

pub(super) fn component_bindings(
    params: &[ParamDef],
    instance_props: &[Property],
) -> HashMap<String, Expr> {
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

pub(super) fn first_labeled_stmt(body: &[Stmt]) -> Option<String> {
    for stmt in body {
        if let Stmt::ActorDecl { label, .. } = stmt {
            return Some(label.clone());
        }
    }
    None
}
