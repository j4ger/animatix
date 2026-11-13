use super::*;
use crate::visit::{AstFolder, walk_fold_inline_item, walk_fold_stmt};

/// Rewriter for module-scoped identifiers and bindings using [`AstFolder`].
struct ModuleRewriter<'a> {
    prefix: &'a str,
    root_label: Option<&'a str>,
    known_labels: &'a HashSet<String>,
    bindings: &'a HashMap<String, Expr>,
}

impl AstFolder for ModuleRewriter<'_> {
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        rewrite_expr(&expr, self.prefix, self.root_label, self.known_labels, self.bindings)
    }

    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        match stmt {
            Stmt::ActorDecl {
                is_pub,
                is_anonymous,
                label,
                array_index,
                ty,
                props,
                modifiers,
                children,
                span,
            } => Stmt::ActorDecl {
                is_pub,
                is_anonymous,
                label: rewrite_label(&label, self.prefix, self.root_label, self.known_labels),
                array_index: array_index.map(|idx| self.fold_expr(idx)),
                ty,
                props: props.into_iter().map(|p| self.fold_property(p)).collect(),
                modifiers: modifiers.into_iter().map(|m| self.fold_modifier(m)).collect(),
                children: children.into_iter().map(|c| self.fold_inline_item(c)).collect(),
                span,
            },
            Stmt::Assignment {
                target,
                property,
                value,
                modifiers,
                easing,
                value_span,
                span,
            } => Stmt::Assignment {
                target: rewrite_label_path(
                    &target,
                    self.prefix,
                    self.root_label,
                    self.known_labels,
                ),
                property,
                value: self.fold_expr(value),
                modifiers: modifiers.into_iter().map(|m| self.fold_modifier(m)).collect(),
                easing,
                value_span,
                span,
            },
            Stmt::Action(action, span) => Stmt::Action(
                Action {
                    verb: action.verb,
                    targets: action
                        .targets
                        .iter()
                        .map(|target| {
                            rewrite_label_ref(
                                target,
                                self.prefix,
                                self.root_label,
                                self.known_labels,
                            )
                        })
                        .collect(),
                    args: action.args.into_iter().map(|a| self.fold_expr(a)).collect(),
                    modifiers: action
                        .modifiers
                        .into_iter()
                        .map(|m| self.fold_modifier(m))
                        .collect(),
                    byte_span: action.byte_span,
                    target_index: action
                        .target_index
                        .into_iter()
                        .map(|opt| opt.map(|expr| self.fold_expr(expr)))
                        .collect(),
                    target_index_position: action.target_index_position,
                },
                span,
            ),
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                span,
            } => Stmt::ReactiveBinding {
                target: rewrite_label_path(
                    &target,
                    self.prefix,
                    self.root_label,
                    self.known_labels,
                ),
                property,
                value: self.fold_expr(value),
                value_span,
                span,
            },
            Stmt::LetDecl {
                is_pub,
                name,
                value,
                span,
            } => Stmt::LetDecl {
                is_pub,
                name: rewrite_label(&name, self.prefix, self.root_label, self.known_labels),
                value: self.fold_expr(value),
                span,
            },
            other => walk_fold_stmt(self, other),
        }
    }

    fn fold_inline_item(&mut self, item: InlineItem) -> InlineItem {
        match item {
            InlineItem::Labeled {
                label,
                array_index,
                ty,
                props,
                modifiers,
                children,
            } => InlineItem::Labeled {
                label: rewrite_label(&label, self.prefix, self.root_label, self.known_labels),
                array_index: array_index.map(|idx| self.fold_expr(idx)),
                ty,
                props: props.into_iter().map(|p| self.fold_property(p)).collect(),
                modifiers: modifiers.into_iter().map(|m| self.fold_modifier(m)).collect(),
                children: children.into_iter().map(|c| self.fold_inline_item(c)).collect(),
            },
            other => walk_fold_inline_item(self, other),
        }
    }
}

pub(crate) fn rewrite_stmt(
    stmt: &Stmt,
    prefix: &str,
    root_label: Option<&str>,
    known_labels: &HashSet<String>,
    bindings: &HashMap<String, Expr>,
) -> Stmt {
    let mut rewriter = ModuleRewriter {
        prefix,
        root_label,
        known_labels,
        bindings,
    };
    rewriter.fold_stmt(stmt.clone())
}
