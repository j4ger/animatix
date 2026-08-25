use super::rewrite::rewrite_stmt;
use super::{
    ComponentEntry, Expr, HashMap, HashSet, InlineItem, InstanceFnRegistry, MatchPattern, ParamDef,
    Property, Stmt, TargetSegment,
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
) -> (Vec<Stmt>, InstanceFnRegistry) {
    let mut ctx = ExpansionCtx::new();
    let mut expanded = Vec::new();
    let mut registry = InstanceFnRegistry::new();
    for stmt in statements {
        expand_stmt_into(stmt, components, &mut expanded, &mut registry, &mut ctx);
    }
    (expanded, registry)
}

fn expand_stmt_into(
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
        _ => output.push(stmt.clone()),
    }
}

fn merge_registry(target: &mut InstanceFnRegistry, source: InstanceFnRegistry) {
    for (label, actions) in source {
        target.insert(label, actions);
    }
}

/// Internal entry point that shares the [`ExpansionCtx`] across recursive calls.
fn expand_statements_inner(
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
fn expand_inline_items(
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

/// Universal actor properties that may be written directly on a component
/// instance. They are forwarded onto the expanded root actor (which carries
/// the instance label) so instance-level placement and visibility work
/// without wrapping the instance in a `Group`. Props sharing a name with a
/// component parameter remain template bindings, not forwards.
pub(crate) const INSTANCE_FORWARDED_PROPS: &[&str] = &["opacity", "at", "anchor", "offset"];

fn expand_component_instance(
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
        if let Stmt::ActorDecl { label, .. } = stmt {
            return Some(label.clone());
        }
    }
    None
}

fn collect_labels(body: &[Stmt]) -> HashSet<String> {
    let mut labels = HashSet::new();
    crate::walk::walk_stmts(body, &mut |stmt| match stmt {
        Stmt::ActorDecl {
            label, children, ..
        } => {
            labels.insert(label.clone());
            collect_inline_labels(children, &mut labels);
        },
        Stmt::ReactiveBinding { target, .. } => {
            if let Some(TargetSegment::Static(label)) = target.first() {
                labels.insert(label.clone());
            }
        },
        _ => {},
    });
    labels
}

fn collect_inline_labels(items: &[InlineItem], labels: &mut HashSet<String>) {
    crate::walk::walk_inline_items(items, &mut |item| {
        if let InlineItem::Labeled { label, .. } = item {
            labels.insert(label.clone());
        }
    });
}

fn has_slot_marker(children: &[InlineItem]) -> bool {
    children.iter().any(|item| matches!(item, InlineItem::SlotMarker))
}

fn resolve_slots(stmts: &[Stmt], slot_fills: &HashMap<String, Vec<InlineItem>>) -> Vec<Stmt> {
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
                    // Slots may sit deeper inside nested containers (e.g. a
                    // Col inside a Col inside the component body) — recurse.
                    let mut cloned = stmt.clone();
                    if let Stmt::ActorDecl { children, .. } = &mut cloned {
                        *children = resolve_slots_in_items(children, slot_fills);
                    }
                    cloned
                }
            },
            _ => stmt.clone(),
        })
        .collect()
}

/// Resolve `@slot` fills in container children at any nesting depth. A
/// container whose direct children contain a slot marker is replaced by its
/// fill (or its non-marker defaults when unfilled); other containers are
/// recursed into.
fn resolve_slots_in_items(
    items: &[InlineItem],
    slot_fills: &HashMap<String, Vec<InlineItem>>,
) -> Vec<InlineItem> {
    items
        .iter()
        .flat_map(|item| match item {
            InlineItem::Labeled {
                label,
                array_index,
                ty,
                props,
                modifiers,
                children,
            } => {
                if has_slot_marker(children) {
                    // Keep the container (label + props); swap its children for
                    // the fill — or the non-marker defaults when unfilled.
                    let replacement = slot_fills.get(label).cloned().unwrap_or_else(|| {
                        children
                            .iter()
                            .filter(|i| !matches!(i, InlineItem::SlotMarker))
                            .cloned()
                            .collect()
                    });
                    vec![InlineItem::Labeled {
                        label: label.clone(),
                        array_index: array_index.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: replacement,
                    }]
                } else {
                    let children = resolve_slots_in_items(children, slot_fills);
                    vec![InlineItem::Labeled {
                        label: label.clone(),
                        array_index: array_index.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children,
                    }]
                }
            },
            other => vec![other.clone()],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(
                index_var.as_deref(),
                Some("k"),
                "index_var must be preserved through expansion"
            );
        } else {
            panic!("Expected ForLoop");
        }
        // Find `a[k]` ActorDecl inside the ForLoop body
        if let Stmt::ForLoop { body, .. } = for_stmt {
            let act = body.iter().find_map(|s| {
                if let Stmt::ActorDecl {
                    array_index: Some(Expr::Ident(name)),
                    ..
                } = s
                {
                    Some(name.clone())
                } else {
                    None
                }
            });
            assert_eq!(
                act.as_deref(),
                Some("k"),
                "array_index must be preserved through expansion"
            );
        }
    }

    #[test]
    fn test_component_always_survives_container_instantiation() {
        // Regression: a component body's `always` (and other non-declaration
        // statements) were silently dropped when the component was
        // instantiated inside a container, because they have no inline form.
        // They must be hoisted to the enclosing statement list with their
        // targets rewritten to the instance's dotted labels.
        let source = r#"
pub component Pulsar() {
  box: Rect, size: (40, 40)
  always { box.rotation = sin(t() * 2) * 8 }
}

grp: Group {
  p: Pulsar
}
"#;
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();
        let mut components = std::collections::HashMap::new();
        for stmt in &stmts {
            if let Stmt::ComponentDef(def, _) = stmt {
                components.insert(
                    def.name.clone(),
                    super::super::ComponentEntry {
                        definition: def.clone(),
                        actions: Default::default(),
                        source_path: std::path::PathBuf::new(),
                    },
                );
            }
        }
        let (expanded, _registry) = expand_statements(&stmts, &components);

        let always_count = expanded.iter().filter(|s| matches!(s, Stmt::Always { .. })).count();
        assert_eq!(
            always_count, 1,
            "component-internal always must survive container instantiation"
        );
        // The hoisted statement must reference the rewritten target: `box`
        // is the component root, so it renames to the instance label `p`.
        let always = expanded.iter().find(|s| matches!(s, Stmt::Always { .. })).unwrap();
        let Stmt::Always { body, .. } = always else {
            panic!("expected Always");
        };
        let rewritten_target = body.iter().find_map(|s| match s {
            Stmt::Assignment {
                target, property, ..
            } => Some((target.clone(), property.clone())),
            _ => None,
        });
        assert_eq!(
            rewritten_target,
            Some((
                vec![crate::ast::TargetSegment::Static("p".to_string())],
                "rotation".to_string()
            )),
            "hoisted assignment must target the renamed instance root"
        );
    }

    #[test]
    fn test_instance_universal_props_forward_to_root() {
        // opacity/at/anchor/offset written on a component instance must land
        // on the expanded root actor, not be dropped.
        let source = r#"
pub component Badge(text: Str) {
  box: Rect, size: (160, 70)
}

b: Badge, text: "hi", opacity: 1, at: (320, 240)
"#;
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();
        let mut components = std::collections::HashMap::new();
        for stmt in &stmts {
            if let Stmt::ComponentDef(def, _) = stmt {
                components.insert(
                    def.name.clone(),
                    super::super::ComponentEntry {
                        definition: def.clone(),
                        actions: Default::default(),
                        source_path: std::path::PathBuf::new(),
                    },
                );
            }
        }
        let (expanded, _registry) = expand_statements(&stmts, &components);
        let root = expanded
            .iter()
            .find_map(|s| match s {
                Stmt::ActorDecl { label, props, .. } if label == "b" => Some(props),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expanded root actor 'b' not found"));
        for name in ["opacity", "at"] {
            assert!(
                root.iter().any(|p| p.name == name),
                "instance prop '{name}' must be forwarded to the root actor"
            );
        }
    }

    #[test]
    fn test_param_named_like_universal_prop_stays_binding() {
        // A component param that shares a name with a forwarded prop (e.g.
        // `opacity` as a template parameter) must remain a binding, not be
        // double-applied as an actor prop.
        let source = r#"
pub component Tint(opacity: Num = 1) {
  box: Rect, size: (40, 40), opacity: opacity
}

t: Tint, opacity: 0.5
"#;
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();
        let mut components = std::collections::HashMap::new();
        for stmt in &stmts {
            if let Stmt::ComponentDef(def, _) = stmt {
                components.insert(
                    def.name.clone(),
                    super::super::ComponentEntry {
                        definition: def.clone(),
                        actions: Default::default(),
                        source_path: std::path::PathBuf::new(),
                    },
                );
            }
        }
        let (expanded, _registry) = expand_statements(&stmts, &components);
        let root = expanded
            .iter()
            .find_map(|s| match s {
                Stmt::ActorDecl { label, props, .. } if label == "t" => Some(props),
                _ => None,
            })
            .expect("expanded root actor 't'");
        let opacity_count = root.iter().filter(|p| p.name == "opacity").count();
        assert_eq!(opacity_count, 1, "param-bound opacity must not be forwarded a second time");
    }

    #[test]
    fn test_expand_component_inside_scene_body() {
        let source = r#"
pub component Box(size: Num = 20) {
  box: Rect, size: (size, size)
}

# Intro
b: Box, size: 40
"#;
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();

        let mut components = std::collections::HashMap::new();
        for stmt in &stmts {
            if let Stmt::ComponentDef(def, _) = stmt {
                components.insert(
                    def.name.clone(),
                    super::super::ComponentEntry {
                        definition: def.clone(),
                        source_path: std::path::PathBuf::from("scene.amx"),
                        actions: std::collections::HashMap::new(),
                    },
                );
            }
        }

        let (expanded, _registry) = expand_statements(&stmts, &components);
        let scene = expanded
            .iter()
            .find_map(|s| {
                if let Stmt::Scene { body, .. } = s {
                    Some(body)
                } else {
                    None
                }
            })
            .expect("scene body should be expanded");
        let box_actor = scene
            .iter()
            .find_map(|s| {
                if let Stmt::ActorDecl { ty, .. } = s {
                    Some(ty)
                } else {
                    None
                }
            })
            .expect("component instance should expand inside scene body");
        assert_eq!(box_actor, "Rect");
    }
}
