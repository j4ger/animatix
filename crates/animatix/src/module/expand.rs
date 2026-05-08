use super::{
    ComponentEntry, Expr, HashMap, HashSet, InlineItem, ParamDef, Property, Stmt,
    rewrite::rewrite_stmt,
};

pub(super) fn expand_statements(
    statements: &[Stmt],
    components: &HashMap<String, ComponentEntry>,
) -> Vec<Stmt> {
    let mut expanded = Vec::new();
    for stmt in statements {
        expand_stmt_into(stmt, components, &mut expanded);
    }
    expanded
}

fn expand_stmt_into(
    stmt: &Stmt,
    components: &HashMap<String, ComponentEntry>,
    output: &mut Vec<Stmt>,
) {
    match stmt {
        Stmt::Keyframe { time, body, .. } => output.push(Stmt::Keyframe {
            time: time.clone(),
            body: expand_statements(body, components),
            span: None,
        }),
        Stmt::RelativeKeyframe { offset, body, .. } => output.push(Stmt::RelativeKeyframe {
            offset: offset.clone(),
            body: expand_statements(body, components),
            span: None,
        }),
        Stmt::Always { body, .. } => output.push(Stmt::Always {
            body: expand_statements(body, components),
            span: None,
        }),
        Stmt::LabeledAlways { label, body, .. } => output.push(Stmt::LabeledAlways {
            label: label.clone(),
            body: expand_statements(body, components),
            span: None,
        }),
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => output.push(Stmt::Conditional {
            condition: condition.clone(),
            then_branch: expand_statements(then_branch, components),
            else_branch: else_branch
                .as_ref()
                .map(|branch| expand_statements(branch, components)),
            span: None,
        }),
        Stmt::ForLoop {
            var,
            iterable,
            body,
            ..
        } => output.push(Stmt::ForLoop {
            var: var.clone(),
            iterable: iterable.clone(),
            body: expand_statements(body, components),
            span: None,
        }),
        Stmt::ComponentAction { name, params, body, .. } => output.push(Stmt::ComponentAction {
            name: name.clone(),
            params: params.clone(),
            body: expand_statements(body, components),
            span: None,
        }),
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
                output.extend(expand_component_instance(
                    label, props, children, component, components,
                ));
            } else {
                output.push(stmt.clone());
            }
        }
        _ => output.push(stmt.clone()),
    }
}

fn expand_component_instance(
    instance_label: &str,
    instance_props: &[Property],
    instance_children: &[InlineItem],
    component: &ComponentEntry,
    components: &HashMap<String, ComponentEntry>,
) -> Vec<Stmt> {
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

    expand_statements(&rewritten, components)
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
        Stmt::LabeledAlways { label, body, .. } => {
            labels.insert(label.clone());
            for stmt in body {
                collect_stmt_labels(stmt, labels);
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
