//! Container and inline-item processing: layout metadata registration,
//! child ordering, and anonymous/labeled inline item expansion.

use super::*;

impl Timeline {
    /// Apply layout algorithm for Row and Col containers.
    /// Computes and sets child positions based on container type, gap, and alignment.
    ///
    /// - `gap`: spacing between children (default 0.0)
    /// - `align`: alignment perpendicular to the layout axis. For Row: "center" (default), "start"
    ///   (top), "end" (bottom) For Col: "center" (default), "start" (left), "end" (right)
    pub(super) fn process_inline_items(
        &mut self,
        time_ms: f64,
        items: &[crate::ast::InlineItem],
        parent_label: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for (index, item) in items.iter().enumerate() {
            match item {
                crate::ast::InlineItem::Anonymous {
                    ty,
                    props,
                    modifiers,
                    children,
                    ..
                } => {
                    let id = format!("__anon_{}_{}", parent_label, index);
                    let stmt = Stmt::ActorDecl {
                        is_pub: false,
                        is_anonymous: true,
                        label: id.clone(),
                        array_index: None,
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: children.clone(),
                        span: None,
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label), diagnostics);
                },
                crate::ast::InlineItem::Labeled {
                    label,
                    array_index,
                    ty,
                    props,
                    modifiers,
                    children,
                    ..
                } => {
                    let stmt = Stmt::ActorDecl {
                        is_pub: false,
                        is_anonymous: false,
                        label: label.clone(),
                        array_index: array_index.clone(),
                        ty: ty.clone(),
                        props: props.clone(),
                        modifiers: modifiers.clone(),
                        children: children.clone(),
                        span: None,
                    };
                    self.process_body(time_ms, &[stmt], Some(parent_label), diagnostics);
                },
                crate::ast::InlineItem::ForLoop {
                    var,
                    index_var,
                    iterable,
                    body,
                    ..
                } => {
                    self.process_for_loop_inline_items(
                        var,
                        index_var,
                        iterable,
                        body,
                        time_ms,
                        parent_label,
                        diagnostics,
                    );
                },
                // SlotMarker and SlotFill are resolved during component expansion.
                // At timeline build time they should never appear in the AST.
                crate::ast::InlineItem::SlotMarker | crate::ast::InlineItem::SlotFill { .. } => {
                    // Unreachable after correct component expansion.
                    // Emitting a diagnostic here is noisy for a correctness-invariant;
                    // if they appear, the timeline simply ignores them.
                },
            }
        }
    }
}
