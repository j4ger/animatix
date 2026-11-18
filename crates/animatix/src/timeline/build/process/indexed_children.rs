//! Give the children of a generated container their concrete instance labels.

use std::collections::{HashMap, HashSet};

use crate::ast::InlineItem;
use crate::timeline::Timeline;

type Aliases = HashMap<String, Option<String>>;

pub(super) fn scope_children(
    timeline: &mut Timeline,
    children: &[InlineItem],
    source_label: &str,
    concrete_label: &str,
) -> Vec<InlineItem> {
    let mut seen = HashSet::new();
    let mut repeated = HashSet::new();
    animatix_syntax::walk::walk_inline_items(children, &mut |item| {
        if let InlineItem::Labeled { label, .. } = item
            && !seen.insert(label.clone())
        {
            repeated.insert(label.clone());
        }
    });
    let scope = IndexedChildren {
        source_label,
        concrete_label,
        repeated,
    };
    let children = scope.qualify_group(children.to_vec(), concrete_label, false);
    // Include the concrete paths when selecting which properties to inject.
    let scan = crate::timeline::build::referenced_roots::scan_inline_references(&children);
    timeline.merge_reference_scan(scan);
    children
}

struct IndexedChildren<'a> {
    source_label: &'a str,
    concrete_label: &'a str,
    repeated: HashSet<String>,
}

impl IndexedChildren<'_> {
    fn qualify_group(
        &self,
        mut children: Vec<InlineItem>,
        parent: &str,
        in_nested_loop: bool,
    ) -> Vec<InlineItem> {
        let mut aliases = HashMap::new();
        self.qualify_items(&mut children, parent, in_nested_loop, &mut aliases);
        let aliases: HashMap<_, _> = aliases
            .into_iter()
            .filter_map(|(source, concrete)| Some((source, concrete?)))
            .collect();
        if aliases.is_empty() {
            return children;
        }
        // Each subtree has already resolved its own names. Resolve any
        // remaining references to this container's children.
        crate::timeline::actor_aliases::resolve_inline_actor_aliases(children, aliases)
    }

    fn qualify_items(
        &self,
        children: &mut [InlineItem],
        parent: &str,
        in_nested_loop: bool,
        aliases: &mut Aliases,
    ) {
        for child in children {
            match child {
                InlineItem::Labeled {
                    label,
                    ty,
                    array_index,
                    children,
                    ..
                } => {
                    if crate::timeline::effects::effect_for_type(ty).is_some()
                        || array_index.is_some() && !in_nested_loop
                    {
                        // Keep filter stage names and directly authored array labels.
                        continue;
                    }
                    let concrete = self.qualify_label(label, parent);
                    aliases
                        .entry(label.clone())
                        .and_modify(|existing| {
                            if existing.as_deref() != Some(&concrete) {
                                *existing = None;
                            }
                        })
                        .or_insert_with(|| Some(concrete.clone()));
                    *label = concrete;
                    if array_index.is_none() {
                        *children =
                            self.qualify_group(std::mem::take(children), label, in_nested_loop);
                    }
                    // Nested indexed children scope their own children after
                    // their index is evaluated by the build walk.
                },
                InlineItem::ForLoop { body, .. } => {
                    self.qualify_items(body, parent, true, aliases);
                },
                InlineItem::Anonymous { children, .. } => {
                    *children =
                        self.qualify_group(std::mem::take(children), parent, in_nested_loop);
                },
                InlineItem::Expr(..) | InlineItem::SlotMarker | InlineItem::SlotFill { .. } => {
                    // Expressions contain no inline declarations, and slots were expanded earlier.
                },
            }
        }
    }

    fn qualify_label(&self, label: &str, parent: &str) -> String {
        // The component expander already gives descendants such as `cards.title`
        // their component's namespace, even below an ordinary wrapper. Keep
        // that identity where a single source declaration owns it.
        if let Some(tail) = label.strip_prefix(self.source_label)
            && tail.starts_with('.')
            && !self.repeated.contains(label)
        {
            return format!("{}{tail}", self.concrete_label);
        }
        // Other descendants are local to their generated container instance.
        // Component expansion may have prefixed the name with an outer instance.
        let leaf = label.rsplit_once('.').map_or(label, |(_, leaf)| leaf);
        format!("{parent}.{leaf}")
    }
}
