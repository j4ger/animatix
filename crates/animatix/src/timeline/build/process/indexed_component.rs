//! Specialize component descendants and property references under indexed roots.

use crate::ast::{Expr, Property, is_array_member_label};
use animatix_syntax::visit::{AstFolder, walk_fold_expr};

/// Return the source-side component path and its resolved array-instance path.
/// For example, any descendant under `cards__2` maps `cards` to `cards__2`.
/// Stop at the array root: component descendants may have absolute source
/// labels such as `cards.title` even beneath a renamed wrapper child `.root`.
pub(super) fn alias(parent_label: &str) -> Option<(String, String)> {
    let segments: Vec<&str> = parent_label.split('.').collect();
    for (index, segment) in segments.iter().enumerate() {
        if let Some(base) = is_array_member_label(segment) {
            let source_prefix = segments
                .iter()
                .take(index)
                .copied()
                .chain(std::iter::once(base))
                .collect::<Vec<_>>()
                .join(".");
            let concrete_prefix = segments[..=index].join(".");
            return Some((source_prefix, concrete_prefix));
        }
    }
    None
}

/// Component expansion prefixes descendants with source instance labels such
/// as `cards.frame`; specialize them under their concrete array parent.
pub(super) fn qualify_child(label: &str, parent_label: &str) -> String {
    if let Some((source_prefix, concrete_prefix)) = alias(parent_label)
        && let Some(suffix) =
            label.strip_prefix(&source_prefix).and_then(|rest| rest.strip_prefix('.'))
    {
        return format!("{concrete_prefix}.{suffix}");
    }
    label.to_string()
}

pub(super) fn specialize_property_paths(
    props: &[Property],
    source_prefix: &str,
    concrete_prefix: &str,
) -> Vec<Property> {
    let mut folder = ComponentPropertyPathFolder {
        source_prefix,
        concrete_prefix,
    };
    props
        .iter()
        .cloned()
        .map(|mut prop| {
            prop.value = folder.fold_expr(prop.value);
            prop
        })
        .collect()
}

struct ComponentPropertyPathFolder<'a> {
    source_prefix: &'a str,
    concrete_prefix: &'a str,
}

impl AstFolder for ComponentPropertyPathFolder<'_> {
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        match expr {
            Expr::Ident(name) if name == self.source_prefix => {
                Expr::Ident(self.concrete_prefix.to_string())
            },
            Expr::Path(parts) => {
                let path = parts.join(".");
                if path == self.source_prefix {
                    return Expr::Path(
                        self.concrete_prefix.split('.').map(str::to_string).collect(),
                    );
                }
                if let Some(suffix) =
                    path.strip_prefix(self.source_prefix).and_then(|rest| rest.strip_prefix('.'))
                {
                    let mut mapped: Vec<String> =
                        self.concrete_prefix.split('.').map(str::to_string).collect();
                    mapped.extend(suffix.split('.').map(str::to_string));
                    return Expr::Path(mapped);
                }
                walk_fold_expr(self, Expr::Path(parts))
            },
            other => walk_fold_expr(self, other),
        }
    }
}
