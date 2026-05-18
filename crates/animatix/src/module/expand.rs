use super::{
    ComponentEntry, Expr, HashMap, HashSet, InlineItem, InstanceActionRegistry, ParamDef,
    Property, Stmt,
    rewrite::rewrite_stmt,
};

pub(super) fn expand_statements(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
) -> (Vec<Stmt>, InstanceActionRegistry) {
    let mut expanded = Vec::new();
    let mut registry = InstanceActionRegistry::new();
    for stmt in statements {
        expand_stmt_into(stmt, components, &mut expanded, &mut registry);
    }
    (expanded, registry)
}

fn expand_stmt_into(
    stmt: &Stmt,
    components: &HashMap<String, ComponentEntry>,
    output: &mut Vec<Stmt>,
    registry: &mut InstanceActionRegistry,
) {
    match stmt {
        Stmt::Keyframe { time, body, .. } => {
            let (expanded_body, sub_registry) = expand_statements(body, components);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Keyframe {
                time: time.clone(),
                body: expanded_body,
                span: None,
            });
        }
        Stmt::RelativeKeyframe { offset, body, .. } => {
            let (expanded_body, sub_registry) = expand_statements(body, components);
            merge_registry(registry, sub_registry);
            output.push(Stmt::RelativeKeyframe {
                offset: offset.clone(),
                body: expanded_body,
                span: None,
            });
        }
        Stmt::Always { body, .. } => {
            let (expanded_body, sub_registry) = expand_statements(body, components);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Always {
                body: expanded_body,
                span: None,
            });
        }
        Stmt::LabeledAlways { label, body, .. } => {
            let (expanded_body, sub_registry) = expand_statements(body, components);
            merge_registry(registry, sub_registry);
            output.push(Stmt::LabeledAlways {
                label: label.clone(),
                body: expanded_body,
                span: None,
            });
        }
        Stmt::Drive { label, body, .. } => {
            let (expanded_body, sub_registry) = expand_statements(body, components);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Drive {
                label: label.clone(),
                body: expanded_body,
                span: None,
            });
        }
        Stmt::ReactiveBinding { target, property, value, .. } => {
            output.push(Stmt::ReactiveBinding {
                target: target.clone(),
                property: property.clone(),
                value: value.clone(),
                span: None,
            });
        }
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let (then_expanded, then_registry) = expand_statements(then_branch, components);
            merge_registry(registry, then_registry);
            let else_expanded = else_branch.as_ref().map(|branch| {
                let (expanded, sub_registry) = expand_statements(branch, components);
                merge_registry(registry, sub_registry);
                expanded
            });
            output.push(Stmt::Conditional {
                condition: condition.clone(),
                then_branch: then_expanded,
                else_branch: else_expanded,
                span: None,
            });
        }
        Stmt::ForLoop {
            var,
            iterable,
            body,
            ..
        } => {
            let (expanded_body, sub_registry) = expand_statements(body, components);
            merge_registry(registry, sub_registry);
            output.push(Stmt::ForLoop {
                var: var.clone(),
                iterable: iterable.clone(),
                body: expanded_body,
                span: None,
            });
        }
        Stmt::Sequence { body, .. } => {
            let (expanded_body, sub_registry) = expand_statements(body, components);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Sequence {
                body: expanded_body,
                span: None,
            });
        }
        Stmt::Stagger { modifiers, body, .. } => {
            let (expanded_body, sub_registry) = expand_statements(body, components);
            merge_registry(registry, sub_registry);
            output.push(Stmt::Stagger {
                modifiers: modifiers.clone(),
                body: expanded_body,
                span: None,
            });
        }
        // ComponentAction is NOT emitted into output; it's collected during instance expansion
        Stmt::ComponentAction { .. } => {}
        Stmt::ComponentDef(..) => {}
        Stmt::ActorDecl {
            label,
            ty,
            props,
            modifiers: _,
            children,
            ..
        } => {
            if let Some(component) = components.get(ty) {
                let (instance_stmts, instance_registry) = expand_component_instance(
                    label, props, children, component, components,
                );
                merge_registry(registry, instance_registry);
                output.extend(instance_stmts);
            } else {
                output.push(stmt.clone());
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

fn expand_component_instance(
    instance_label: &str,
    instance_props: &[Property],
    instance_children: &[InlineItem],
    component: &ComponentEntry,
    components: &HashMap<String, ComponentEntry>,
) -> (Vec<Stmt>, InstanceActionRegistry) {
    let bindings = component_bindings(&component.definition.params, instance_props);
    let root_label = first_labeled_stmt(&component.definition.body);
    let known_labels = collect_labels(&component.definition.body);

    // Extract slot fills from instance children (keyed by original slot name)
    let mut slot_fills: HashMap<String, Vec<InlineItem>> = HashMap::new();
    for item in instance_children {
        if let InlineItem::SlotFill { slot_name, items } = item {
            slot_fills.insert(slot_name.clone(), items.clone());
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
    let mut instance_actions: HashMap<String, Vec<Stmt>> = HashMap::new();
    let filtered: Vec<Stmt> = rewritten
        .into_iter()
        .filter_map(|stmt| match &stmt {
            Stmt::ComponentAction { name, body, .. } => {
                instance_actions.insert(name.clone(), body.clone());
                None
            }
            _ => Some(stmt),
        })
        .collect();

    let (expanded_stmts, mut registry) = expand_statements(&filtered, components);
    if !instance_actions.is_empty() {
        registry.insert(instance_label.to_string(), instance_actions);
    }
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
        match stmt {
            Stmt::Text {
                label: Some(label), ..
            }
            | Stmt::Math {
                label: Some(label), ..
            }
            | Stmt::Code {
                label: Some(label), ..
            }
            | Stmt::ActorDecl { label, .. } => return Some(label.clone()),
            Stmt::Svg {
                label: Some(label), ..
            }
            | Stmt::Image {
                label: Some(label), ..
            } => return Some(label.clone()),
            _ => {}
        }
    }
    None
}

fn collect_labels(body: &[Stmt]) -> HashSet<String> {
    let mut labels = HashSet::new();
    for stmt in body {
        collect_stmt_labels(stmt, &mut labels);
    }
    labels
}

fn collect_stmt_labels(stmt: &Stmt, labels: &mut HashSet<String>) {
    match stmt {
        Stmt::Text {
            label: Some(label), ..
        }
        | Stmt::Math {
            label: Some(label), ..
        }
        | Stmt::Code {
            label: Some(label), ..
        }
        | Stmt::ActorDecl { label, .. } => {
            labels.insert(label.clone());
        }
        Stmt::Svg {
            label: Some(label), ..
        }
        | Stmt::Image {
            label: Some(label), ..
        } => {
            labels.insert(label.clone());
        }
        Stmt::LabeledAlways { label, body, .. }
        | Stmt::Drive { label, body, .. } => {
            labels.insert(label.clone());
            for stmt in body {
                collect_stmt_labels(stmt, labels);
            }
        }
        Stmt::ReactiveBinding { target, .. } => {
            if let Some(label) = target.first() {
                labels.insert(label.clone());
            }
        }
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ComponentAction { body, .. }
        | Stmt::ForLoop { body, .. } => {
            for stmt in body {
                collect_stmt_labels(stmt, labels);
            }
        }
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            for stmt in then_branch {
                collect_stmt_labels(stmt, labels);
            }
            if let Some(else_branch) = else_branch {
                for stmt in else_branch {
                    collect_stmt_labels(stmt, labels);
                }
            }
        }
        _ => {}
    }
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
                ty,
                props,
                modifiers,
                children,
                ..
            } => {
                if has_slot_marker(&children) {
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
                        label: label.clone(),
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
