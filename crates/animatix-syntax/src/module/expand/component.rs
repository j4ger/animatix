use super::*;
use crate::visit::{AstFolder, walk_fold_stmt};

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
                        label,
                        array_index,
                        props,
                        children,
                        component,
                        components,
                        ctx,
                    );
                    let instance_stmts = carry_instance_array_index(instance_stmts, array_index);
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
                        &None,
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

/// Keep an indexed component declaration's identity on the expanded root.
/// The build loop resolves this expression after binding the loop variables,
/// just as it does for an indexed primitive declaration.
pub(super) fn carry_instance_array_index(
    mut statements: Vec<Stmt>,
    array_index: &Option<Expr>,
) -> Vec<Stmt> {
    let Some(index) = array_index else {
        return statements;
    };
    if let Some(Stmt::ActorDecl { array_index, .. }) =
        statements.iter_mut().find(|stmt| matches!(stmt, Stmt::ActorDecl { .. }))
    {
        *array_index = Some(index.clone());
    }
    statements
}

/// Attach an indexed component's root index to its internal actor targets.
/// Declarations keep their source labels for hierarchy construction; indexed
/// assignments and actions carry the root index so build/runtime resolution
/// can address the concrete instance.
fn specialize_component_references(
    statements: Vec<Stmt>,
    instance_label: &str,
    index: Option<&Expr>,
) -> Vec<Stmt> {
    let Some(index) = index else {
        return statements;
    };
    let mut folder = ComponentIndexFolder {
        root: instance_label,
        index,
    };
    folder.fold_stmts(statements)
}

struct ComponentIndexFolder<'a> {
    root: &'a str,
    index: &'a Expr,
}

impl AstFolder for ComponentIndexFolder<'_> {
    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        match stmt {
            Stmt::Assignment {
                target,
                property,
                value,
                modifiers,
                easing,
                value_span,
                span,
            } => {
                let target = self.specialize_target_path(target);
                Stmt::Assignment {
                    target,
                    property,
                    value: self.fold_expr(value),
                    modifiers: modifiers.into_iter().map(|m| self.fold_modifier(m)).collect(),
                    easing,
                    value_span,
                    span,
                }
            },
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                span,
            } => Stmt::ReactiveBinding {
                target: self.specialize_target_path(target),
                property,
                value: self.fold_expr(value),
                value_span,
                span,
            },
            other => walk_fold_stmt(self, other),
        }
    }
    fn fold_action(&mut self, action: crate::ast::Action) -> crate::ast::Action {
        ComponentIndexFolder::fold_action(self, action)
    }
}

impl ComponentIndexFolder<'_> {
    fn specialize_target_path(
        &self,
        mut target: Vec<crate::ast::TargetSegment>,
    ) -> Vec<crate::ast::TargetSegment> {
        let root_segments: Vec<&str> = self.root.split('.').collect();
        let root_len = root_segments.len();
        if target.len() < root_len {
            return target;
        }
        let matches_root = target.iter().take(root_len).zip(&root_segments).all(
            |(segment, root_segment)| {
                matches!(segment, crate::ast::TargetSegment::Static(label) if label == root_segment)
            },
        );
        if matches_root {
            target[root_len - 1] = crate::ast::TargetSegment::Indexed {
                base: root_segments[root_len - 1].to_string(),
                index: Box::new(self.index.clone()),
            };
        }
        target
    }

    fn fold_action(&mut self, mut action: crate::ast::Action) -> crate::ast::Action {
        if action.target_index.len() < action.targets.len() {
            action.target_index.resize(action.targets.len(), None);
        }
        if action.target_index_position.len() < action.targets.len() {
            action.target_index_position.resize(action.targets.len(), None);
        }
        for ((target, index), position) in action
            .targets
            .iter()
            .zip(&mut action.target_index)
            .zip(&mut action.target_index_position)
        {
            let targets_instance = target == self.root
                || target.strip_prefix(self.root).is_some_and(|rest| rest.starts_with('.'));
            if targets_instance && index.is_none() {
                *index = Some(self.index.clone());
                *position = Some(self.root.split('.').count().saturating_sub(1));
            }
        }
        action.args = action.args.into_iter().map(|arg| self.fold_expr(arg)).collect();
        action.modifiers = action.modifiers.into_iter().map(|m| self.fold_modifier(m)).collect();
        action.target_index = action
            .target_index
            .into_iter()
            .map(|index| index.map(|expr| self.fold_expr(expr)))
            .collect();
        action.target_index_position.resize(action.targets.len(), None);
        action
    }
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

/// Apply an instance-level `opacity` to a single-root component's root actor
/// as a multiplicative group opacity.
///
/// Forwarding the value as a plain property would make it replace the root
/// actor's own authored opacity (`0.5` on both would stay `0.5` instead of
/// `0.25`) — a component instance must behave like a group, not like a style
/// override. Compose the root's existing expression with the instance one; a
/// root without an authored opacity gets the instance value unchanged.
fn multiply_instance_opacity(props: &mut Vec<Property>, instance_opacity: &Property) {
    let existing = props.iter().rposition(|p| p.name == "opacity").map(|index| props.remove(index));
    let value = match existing {
        Some(root_opacity) => Expr::Binary(
            Box::new(root_opacity.value),
            crate::ast::BinaryOp::Mul,
            Box::new(instance_opacity.value.clone()),
        ),
        None => instance_opacity.value.clone(),
    };
    props.push(Property {
        name: "opacity".to_string(),
        value,
        value_span: instance_opacity.value_span,
        trailing_comment: None,
    });
}

pub(super) fn expand_component_instance(
    instance_label: &str,
    instance_array_index: &Option<Expr>,
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
    let rewritten =
        specialize_component_references(rewritten, instance_label, instance_array_index.as_ref());

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
                        body: specialize_component_references(
                            body.clone(),
                            instance_label,
                            instance_array_index.as_ref(),
                        ),
                        instance_index: instance_array_index.clone(),
                    },
                );
                None
            },
            _ => Some(stmt),
        })
        .collect();

    let (expanded_stmts, mut registry) = expand_statements_inner(&filtered, components, ctx);
    specialize_nested_instance_methods(
        &mut registry,
        instance_label,
        instance_array_index.as_ref(),
    );
    let expanded_stmts = specialize_component_references(
        expanded_stmts,
        instance_label,
        instance_array_index.as_ref(),
    );
    if !instance_actions.is_empty() {
        registry.insert(instance_label.to_string(), instance_actions);
    }
    ctx.depth -= 1;

    assemble_component_instance(
        expanded_stmts,
        instance_label,
        root_label.as_deref(),
        forwarded,
        registry,
    )
}

fn assemble_component_instance(
    expanded_stmts: Vec<Stmt>,
    instance_label: &str,
    root_label: Option<&str>,
    forwarded: Vec<Property>,
    registry: InstanceFnRegistry,
) -> (Vec<Stmt>, InstanceFnRegistry) {
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

    // Multi-statement component bodies must not expand into sibling top-level
    // statements: doing so orphans the instance wrapper, because the build
    // creates the instance track (e.g. `card`) as a root node with an empty
    // children list while the remaining `card.*` statements become independent
    // roots (each seeded with the hidden-by-default pre-keyframe opacity). Wrap
    // the top-level actors in a Group labeled with the instance label so the
    // build links them under one parent — a single root, with opacity and
    // transform context shared across the whole subtree. The Group (not merely
    // nesting under the first actor) is required because a shape root (e.g. a
    // `Rect` frame) does not render children as a group; a generic Group
    // container does. It also receives the instance root's authored `size` so a
    // parent layout container sizes the instance cell correctly instead of
    // collapsing a zero-sized wrapper.
    if actors.len() > 1 {
        let mut children: Vec<InlineItem> = Vec::new();
        let mut root_size: Option<Property> = None;
        for mut stmt in actors {
            if let Stmt::ActorDecl { label, props, .. } = &mut stmt
                && label == instance_label
            {
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
                // The wrapper receives the authored relative size. Its root
                // child then fills that allocated cell instead of applying
                // the same percentage a second time.
                normalize_group_root_size(props);
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
    // actor. Forward placement props onto it so instance-level placement works
    // without wrapping; `opacity` is composed multiplicatively with the root's
    // own authored value so the instance acts as a group opacity.
    let instance_opacity = forwarded.iter().find(|p| p.name == "opacity").cloned();
    let forwarded: Vec<Property> = forwarded.into_iter().filter(|p| p.name != "opacity").collect();
    if !forwarded.is_empty() || instance_opacity.is_some() {
        for stmt in actors.iter_mut().chain(hoisted.iter_mut()) {
            if let Stmt::ActorDecl { label, props, .. } = stmt
                && label == instance_label
            {
                props.extend(forwarded.iter().cloned());
                if let Some(opacity) = &instance_opacity {
                    multiply_instance_opacity(props, opacity);
                }
                break;
            }
        }
    }

    let mut out = actors;
    out.extend(hoisted);
    (out, registry)
}

/// Convert relative dimensions on a multi-actor component's inner root to
/// `fill`; the wrapper Group owns the original authored dimensions.
fn normalize_group_root_size(props: &mut [Property]) {
    let Some(size) = props.iter_mut().find(|property| property.name == "size") else {
        return;
    };
    size.value = match &size.value {
        Expr::Tuple(axes) => Expr::Tuple(axes.iter().map(relative_axis_to_fill).collect()),
        Expr::Percent(_) => Expr::Ident("fill".to_string()),
        Expr::Str(value) if value == "fill" || is_percent_literal(value) => {
            Expr::Ident("fill".to_string())
        },
        _ => size.value.clone(),
    };
}

fn relative_axis_to_fill(axis: &Expr) -> Expr {
    match axis {
        Expr::Percent(_) => Expr::Ident("fill".to_string()),
        Expr::Str(value) if value == "fill" || is_percent_literal(value) => {
            Expr::Ident("fill".to_string())
        },
        Expr::Ident(value) if value == "fill" => Expr::Ident("fill".to_string()),
        _ => axis.clone(),
    }
}

fn is_percent_literal(value: &str) -> bool {
    value.strip_suffix('%').is_some_and(|number| number.parse::<f64>().is_ok())
}

/// Carry an indexed parent component's identity into methods on nested
/// component instances. Their labels include the parent path (for example
/// `cards.badge`), while the array index belongs to the parent's first path
/// segment. A nested component with its own index keeps its more specific
/// binding.
fn specialize_nested_instance_methods(
    registry: &mut InstanceFnRegistry,
    instance_label: &str,
    instance_index: Option<&Expr>,
) {
    let Some(index) = instance_index else {
        return;
    };
    for (label, methods) in registry {
        if !label.strip_prefix(instance_label).is_some_and(|rest| rest.starts_with('.')) {
            continue;
        }
        for template in methods.values_mut() {
            if template.instance_index.is_some() {
                continue;
            }
            template.body =
                specialize_component_references(template.body.clone(), instance_label, Some(index));
            template.instance_index = Some(index.clone());
        }
    }
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

#[cfg(test)]
mod component_index_tests {
    use super::*;
    use crate::ast::TargetSegment;

    #[test]
    fn dotted_instance_prefix_indexes_the_component_root_segment() {
        let index = Expr::Num(3.0);
        let mut folder = ComponentIndexFolder {
            root: "layout.cards",
            index: &index,
        };
        assert_eq!(
            folder.specialize_target_path(vec![
                TargetSegment::Static("layout".into()),
                TargetSegment::Static("cards".into()),
                TargetSegment::Static("title".into()),
            ]),
            vec![
                TargetSegment::Static("layout".into()),
                TargetSegment::Indexed {
                    base: "cards".into(),
                    index: Box::new(Expr::Num(3.0)),
                },
                TargetSegment::Static("title".into()),
            ]
        );
        let action = folder.fold_action(crate::ast::Action {
            verb: "fade-in".into(),
            targets: vec!["layout.cards.title".into()],
            target_index: vec![None],
            target_index_position: vec![None],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
        });
        assert_eq!(action.targets, vec!["layout.cards.title"]);
        assert_eq!(action.target_index, vec![Some(Expr::Num(3.0))]);
        assert_eq!(action.target_index_position, vec![Some(1)]);
    }

    #[test]
    fn group_root_size_normalizes_only_literal_relative_axes() {
        let mut props = vec![Property {
            name: "size".into(),
            value: Expr::Tuple(vec![Expr::Str("50%".into()), Expr::Str("fit".into())]),
            value_span: None,
            trailing_comment: None,
        }];
        normalize_group_root_size(&mut props);
        assert_eq!(
            props[0].value,
            Expr::Tuple(vec![Expr::Ident("fill".into()), Expr::Str("fit".into())]),
        );
        assert_eq!(relative_axis_to_fill(&Expr::Num(120.0)), Expr::Num(120.0));
        assert_eq!(relative_axis_to_fill(&Expr::Ident("auto".into())), Expr::Ident("auto".into()));
    }
}
