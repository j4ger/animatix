use super::*;

pub(super) fn collect_labels(body: &[Stmt]) -> HashSet<String> {
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
        // Intentional: only actor declarations and reactive-binding targets
        // introduce component-body labels. `let` bindings resolve through the
        // bindings map, not known_labels; other statements introduce none.
        _ => {},
    });
    labels
}

pub(super) fn collect_inline_labels(items: &[InlineItem], labels: &mut HashSet<String>) {
    crate::walk::walk_inline_items(items, &mut |item| {
        if let InlineItem::Labeled { label, .. } = item {
            labels.insert(label.clone());
        }
    });
}

pub(super) fn has_slot_marker(children: &[InlineItem]) -> bool {
    children.iter().any(|item| matches!(item, InlineItem::SlotMarker))
}

pub(super) fn resolve_slots(
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
                    // Slots may sit deeper inside nested containers (e.g. a
                    // Col inside a Col inside the component body) — recurse.
                    let mut cloned = stmt.clone();
                    if let Stmt::ActorDecl { children, .. } = &mut cloned {
                        *children = resolve_slots_in_items(children, slot_fills);
                    }
                    cloned
                }
            },
            // Non-container variants cannot hold slot markers.
            Stmt::Action(..)
            | Stmt::LetDecl { .. }
            | Stmt::Assignment { .. }
            | Stmt::ReactiveBinding { .. }
            | Stmt::Keyframe { .. }
            | Stmt::RelativeKeyframe { .. }
            | Stmt::Always { .. }
            | Stmt::Conditional { .. }
            | Stmt::Match { .. }
            | Stmt::ForLoop { .. }
            | Stmt::Sequence { .. }
            | Stmt::Stagger { .. }
            | Stmt::FnDecl { .. }
            | Stmt::Block { .. }
            | Stmt::Return { .. }
            | Stmt::Expr(..)
            | Stmt::Config { .. }
            | Stmt::Step { .. }
            | Stmt::Import { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Play { .. }
            | Stmt::Comment(..)
            | Stmt::Scene { .. }
            | Stmt::ComponentDef(..) => stmt.clone(),
        })
        .collect()
}

/// Resolve `@slot` fills in container children at any nesting depth. A
/// container whose direct children contain a slot marker is replaced by its
/// fill (or its non-marker defaults when unfilled); other containers are
/// recursed into.
pub(super) fn resolve_slots_in_items(
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
