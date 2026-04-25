use super::{
    ComponentEntry, Expr, HashMap, HashSet, ParamDef, Property, Stmt, rewrite::rewrite_stmt,
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
        Stmt::Always { body } => output.push(Stmt::Always {
            body: expand_statements(body, components),
        }),
        Stmt::LabeledAlways { label, body } => output.push(Stmt::LabeledAlways {
            label: label.clone(),
            body: expand_statements(body, components),
        }),
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
        } => output.push(Stmt::Conditional {
            condition: condition.clone(),
            then_branch: expand_statements(then_branch, components),
            else_branch: else_branch
                .as_ref()
                .map(|branch| expand_statements(branch, components)),
        }),
        Stmt::ForLoop {
            var,
            iterable,
            body,
        } => output.push(Stmt::ForLoop {
            var: var.clone(),
            iterable: iterable.clone(),
            body: expand_statements(body, components),
        }),
        Stmt::ComponentAction { name, params, body } => output.push(Stmt::ComponentAction {
            name: name.clone(),
            params: params.clone(),
            body: expand_statements(body, components),
        }),
        Stmt::ComponentDef(_) => {}
        Stmt::ActorDecl {
            label,
            ty,
            props,
            modifiers: _,
            children: _,
            ..
        } => {
            if let Some(component) = components.get(ty) {
                output.extend(expand_component_instance(
                    label, props, component, components,
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
    component: &ComponentEntry,
    components: &HashMap<String, ComponentEntry>,
) -> Vec<Stmt> {
    let bindings = component_bindings(&component.definition.params, instance_props);
    let root_label = first_labeled_stmt(&component.definition.body);
    let known_labels = collect_labels(&component.definition.body);

    let rewritten = component
        .definition
        .body
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
        Stmt::LabeledAlways { label, body } => {
            labels.insert(label.clone());
            for stmt in body {
                collect_stmt_labels(stmt, labels);
            }
        }
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body }
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
