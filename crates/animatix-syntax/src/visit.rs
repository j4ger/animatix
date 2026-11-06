//! AST Visitor and Folder traits for uniform traversal and transformation.
//!
//! Provides [`self::AstVisitor`] for recursive read-only walks and [`self::AstFolder`]
//! for in-place or owned tree rewrites, preventing repetitive pattern-matching
//! boilers in syntax passes (e.g. module expansion, rewrites, typechecking,
//! and semantic linting).

use crate::ast::*;

/// Trait for read-only traversal over the Animatix AST.
///
/// Implementors can override specific `visit_*` hooks. Default implementations
/// recurse through all children automatically.
pub trait AstVisitor<'ast> {
    /// Visit a statement node.
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        walk_stmt(self, stmt);
    }

    /// Visit an expression node.
    fn visit_expr(&mut self, expr: &'ast Expr) {
        walk_expr(self, expr);
    }

    /// Visit an inline item node inside a container.
    fn visit_inline_item(&mut self, item: &'ast InlineItem) {
        walk_inline_item(self, item);
    }

    /// Visit a property node.
    fn visit_property(&mut self, prop: &'ast Property) {
        self.visit_expr(&prop.value);
    }

    /// Visit a modifier node.
    fn visit_modifier(&mut self, modifier: &'ast Modifier) {
        self.visit_expr(&modifier.value);
    }

    /// Visit an action invocation.
    fn visit_action(&mut self, action: &'ast Action) {
        for arg in &action.args {
            self.visit_expr(arg);
        }
        for modifier in &action.modifiers {
            self.visit_modifier(modifier);
        }
    }
}

/// Recursively walk child nodes of a [`Stmt`].
pub fn walk_stmt<'ast, V: AstVisitor<'ast> + ?Sized>(visitor: &mut V, stmt: &'ast Stmt) {
    match stmt {
        Stmt::Action(action, _) => {
            visitor.visit_action(action);
        },
        Stmt::LetDecl { value, .. } => {
            visitor.visit_expr(value);
        },
        Stmt::ActorDecl {
            props,
            modifiers,
            children,
            array_index,
            ..
        } => {
            if let Some(idx) = array_index {
                visitor.visit_expr(idx);
            }
            for prop in props {
                visitor.visit_property(prop);
            }
            for modifier in modifiers {
                visitor.visit_modifier(modifier);
            }
            for child in children {
                visitor.visit_inline_item(child);
            }
        },
        Stmt::TypeAlias { .. } => {},
        Stmt::Import { .. } => {},
        Stmt::Keyframe { body, .. } => {
            for s in body {
                visitor.visit_stmt(s);
            }
        },
        Stmt::RelativeKeyframe { body, .. } => {
            for s in body {
                visitor.visit_stmt(s);
            }
        },
        Stmt::Assignment {
            target,
            value,
            modifiers,
            ..
        } => {
            for seg in target {
                if let TargetSegment::Indexed { index, .. } = seg {
                    visitor.visit_expr(index);
                }
            }
            visitor.visit_expr(value);
            for modifier in modifiers {
                visitor.visit_modifier(modifier);
            }
        },
        Stmt::Sequence { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::Block { body, .. }
        | Stmt::Scene { body, .. } => {
            for s in body {
                visitor.visit_stmt(s);
            }
        },
        Stmt::Stagger {
            modifiers, body, ..
        } => {
            for modifier in modifiers {
                visitor.visit_modifier(modifier);
            }
            for s in body {
                visitor.visit_stmt(s);
            }
        },
        Stmt::ReactiveBinding { target, value, .. } => {
            for seg in target {
                if let TargetSegment::Indexed { index, .. } = seg {
                    visitor.visit_expr(index);
                }
            }
            visitor.visit_expr(value);
        },
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visitor.visit_expr(condition);
            for s in then_branch {
                visitor.visit_stmt(s);
            }
            if let Some(else_branch) = else_branch {
                for s in else_branch {
                    visitor.visit_stmt(s);
                }
            }
        },
        Stmt::Match {
            scrutinee, arms, ..
        } => {
            visitor.visit_expr(scrutinee);
            for (_, arm_body) in arms {
                for s in arm_body {
                    visitor.visit_stmt(s);
                }
            }
        },
        Stmt::ForLoop {
            iterable,
            body,
            modifiers,
            ..
        } => {
            visitor.visit_expr(iterable);
            for modifier in modifiers {
                visitor.visit_modifier(modifier);
            }
            for s in body {
                visitor.visit_stmt(s);
            }
        },
        Stmt::ComponentDef(comp, _) => {
            for param in &comp.params {
                if let Some(ref def) = param.default {
                    visitor.visit_expr(def);
                }
            }
            for s in &comp.body {
                visitor.visit_stmt(s);
            }
        },
        Stmt::FnDecl { body, .. } => {
            for s in body {
                visitor.visit_stmt(s);
            }
        },
        Stmt::Return { value, .. } => {
            if let Some(v) = value {
                visitor.visit_expr(v);
            }
        },
        Stmt::Expr(expr, _) => {
            visitor.visit_expr(expr);
        },
        Stmt::Config { settings, .. } => {
            for prop in settings {
                visitor.visit_property(prop);
            }
        },
        Stmt::Play { .. } | Stmt::Step { .. } | Stmt::Comment(..) => {},
    }
}

/// Recursively walk child nodes of an [`Expr`].
pub fn walk_expr<'ast, V: AstVisitor<'ast> + ?Sized>(visitor: &mut V, expr: &'ast Expr) {
    match expr {
        Expr::Num(_) | Expr::Percent(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => {},
        Expr::Ident(_) => {},
        Expr::Path(_) => {},
        Expr::Index(target, index) => {
            visitor.visit_expr(target);
            visitor.visit_expr(index);
        },
        Expr::Tuple(items) | Expr::List(items) => {
            for item in items {
                visitor.visit_expr(item);
            }
        },
        Expr::Binary(left, _, right) => {
            visitor.visit_expr(left);
            visitor.visit_expr(right);
        },
        Expr::Unary(_, inner) => {
            visitor.visit_expr(inner);
        },
        Expr::Call(_, args) => {
            for arg in args {
                visitor.visit_expr(arg);
            }
        },
        Expr::Method(receiver, _, args) => {
            visitor.visit_expr(receiver);
            for arg in args {
                visitor.visit_expr(arg);
            }
        },
        Expr::Closure(_, body) => {
            visitor.visit_expr(body);
        },
        Expr::LetChain(bindings, tail) => {
            for (_, expr) in bindings {
                visitor.visit_expr(expr);
            }
            visitor.visit_expr(tail);
        },
        Expr::Conditional(cond, then_branch, else_branch) => {
            visitor.visit_expr(cond);
            visitor.visit_expr(then_branch);
            visitor.visit_expr(else_branch);
        },
        Expr::Match(scrutinee, arms) => {
            visitor.visit_expr(scrutinee);
            for (_, arm_expr) in arms {
                visitor.visit_expr(arm_expr);
            }
        },
        Expr::Construct(_, props) => {
            for prop in props {
                visitor.visit_property(prop);
            }
        },
    }
}

/// Recursively walk child nodes of an [`InlineItem`].
pub fn walk_inline_item<'ast, V: AstVisitor<'ast> + ?Sized>(
    visitor: &mut V,
    item: &'ast InlineItem,
) {
    match item {
        InlineItem::Anonymous {
            props,
            modifiers,
            children,
            ..
        } => {
            for prop in props {
                visitor.visit_property(prop);
            }
            for modifier in modifiers {
                visitor.visit_modifier(modifier);
            }
            for child in children {
                visitor.visit_inline_item(child);
            }
        },
        InlineItem::Labeled {
            array_index,
            props,
            modifiers,
            children,
            ..
        } => {
            if let Some(idx) = array_index {
                visitor.visit_expr(idx);
            }
            for prop in props {
                visitor.visit_property(prop);
            }
            for modifier in modifiers {
                visitor.visit_modifier(modifier);
            }
            for child in children {
                visitor.visit_inline_item(child);
            }
        },
        InlineItem::ForLoop { iterable, body, .. } => {
            visitor.visit_expr(iterable);
            for child in body {
                visitor.visit_inline_item(child);
            }
        },
        InlineItem::SlotFill { items, .. } => {
            for child in items {
                visitor.visit_inline_item(child);
            }
        },
        InlineItem::SlotMarker => {},
    }
}

/// Trait for in-place or owned rewriting of the Animatix AST.
///
/// Override methods for node types you want to transform; other nodes recurse
/// into children using the default fold implementations.
pub trait AstFolder {
    /// Transform a statement node.
    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        walk_fold_stmt(self, stmt)
    }

    /// Transform an expression node.
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        walk_fold_expr(self, expr)
    }

    /// Transform an inline item node.
    fn fold_inline_item(&mut self, item: InlineItem) -> InlineItem {
        walk_fold_inline_item(self, item)
    }

    /// Transform a property node.
    fn fold_property(&mut self, prop: Property) -> Property {
        Property {
            name: prop.name,
            value: self.fold_expr(prop.value),
            trailing_comment: prop.trailing_comment,
            value_span: prop.value_span,
        }
    }

    /// Transform a modifier node.
    fn fold_modifier(&mut self, modifier: Modifier) -> Modifier {
        Modifier {
            name: modifier.name,
            value: self.fold_expr(modifier.value),
        }
    }

    /// Transform a target segment.
    fn fold_target_segment(&mut self, segment: TargetSegment) -> TargetSegment {
        match segment {
            TargetSegment::Static(s) => TargetSegment::Static(s),
            TargetSegment::Indexed { base, index } => TargetSegment::Indexed {
                base,
                index: Box::new(self.fold_expr(*index)),
            },
        }
    }

    /// Transform an action.
    fn fold_action(&mut self, action: Action) -> Action {
        Action {
            verb: action.verb,
            targets: action.targets,
            args: action.args.into_iter().map(|a| self.fold_expr(a)).collect(),
            modifiers: action.modifiers.into_iter().map(|m| self.fold_modifier(m)).collect(),
            byte_span: action.byte_span,
            target_index: action
                .target_index
                .into_iter()
                .map(|opt| opt.map(|expr| self.fold_expr(expr)))
                .collect(),
        }
    }
}

/// Fold child nodes of a [`Stmt`].
pub fn walk_fold_stmt<F: AstFolder + ?Sized>(folder: &mut F, stmt: Stmt) -> Stmt {
    match stmt {
        Stmt::Action(action, span) => Stmt::Action(folder.fold_action(action), span),
        Stmt::LetDecl {
            is_pub,
            name,
            value,
            span,
        } => Stmt::LetDecl {
            is_pub,
            name,
            value: folder.fold_expr(value),
            span,
        },
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
            label,
            array_index: array_index.map(|idx| folder.fold_expr(idx)),
            ty,
            props: props.into_iter().map(|p| folder.fold_property(p)).collect(),
            modifiers: modifiers.into_iter().map(|m| folder.fold_modifier(m)).collect(),
            children: children.into_iter().map(|c| folder.fold_inline_item(c)).collect(),
            span,
        },
        Stmt::TypeAlias { .. } | Stmt::Import { .. } => stmt,
        Stmt::Keyframe { time, body, span } => Stmt::Keyframe {
            time,
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            span,
        },
        Stmt::RelativeKeyframe { offset, body, span } => Stmt::RelativeKeyframe {
            offset,
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
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
            target: target.into_iter().map(|seg| folder.fold_target_segment(seg)).collect(),
            property,
            value: folder.fold_expr(value),
            modifiers: modifiers.into_iter().map(|m| folder.fold_modifier(m)).collect(),
            easing,
            value_span,
            span,
        },
        Stmt::Sequence { body, span } => Stmt::Sequence {
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            span,
        },
        Stmt::Stagger {
            modifiers,
            body,
            span,
        } => Stmt::Stagger {
            modifiers: modifiers.into_iter().map(|m| folder.fold_modifier(m)).collect(),
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            span,
        },
        Stmt::Always { body, span } => Stmt::Always {
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            span,
        },
        Stmt::ReactiveBinding {
            target,
            property,
            value,
            value_span,
            span,
        } => Stmt::ReactiveBinding {
            target: target.into_iter().map(|seg| folder.fold_target_segment(seg)).collect(),
            property,
            value: folder.fold_expr(value),
            value_span,
            span,
        },
        Stmt::Conditional { .. }
        | Stmt::Match { .. }
        | Stmt::ForLoop { .. }
        | Stmt::ComponentDef(..)
        | Stmt::FnDecl { .. }
        | Stmt::Config { .. }
        | Stmt::Scene { .. } => fold_stmt_compound(folder, stmt),
        Stmt::Block { body, span } => Stmt::Block {
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            span,
        },
        Stmt::Return { value, span } => Stmt::Return {
            value: value.map(|v| folder.fold_expr(v)),
            span,
        },
        Stmt::Expr(expr, span) => Stmt::Expr(folder.fold_expr(expr), span),
        Stmt::Play { .. } | Stmt::Step { .. } | Stmt::Comment(..) => stmt,
    }
}

fn fold_stmt_compound<F: AstFolder + ?Sized>(folder: &mut F, stmt: Stmt) -> Stmt {
    match stmt {
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            span,
        } => Stmt::Conditional {
            condition: folder.fold_expr(condition),
            then_branch: then_branch.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            else_branch: else_branch.map(|b| b.into_iter().map(|s| folder.fold_stmt(s)).collect()),
            span,
        },
        Stmt::Match {
            scrutinee,
            arms,
            span,
        } => Stmt::Match {
            scrutinee: folder.fold_expr(scrutinee),
            arms: arms
                .into_iter()
                .map(|(pat, body)| (pat, body.into_iter().map(|s| folder.fold_stmt(s)).collect()))
                .collect(),
            span,
        },
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            modifiers,
            span,
        } => Stmt::ForLoop {
            var,
            index_var,
            iterable: folder.fold_expr(iterable),
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            modifiers: modifiers.into_iter().map(|m| folder.fold_modifier(m)).collect(),
            span,
        },
        Stmt::ComponentDef(comp, span) => Stmt::ComponentDef(
            ComponentDef {
                is_pub: comp.is_pub,
                name: comp.name,
                params: comp
                    .params
                    .into_iter()
                    .map(|p| ParamDef {
                        name: p.name,
                        param_type: p.param_type,
                        default: p.default.map(|expr| folder.fold_expr(expr)),
                    })
                    .collect(),
                body: comp.body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            },
            span,
        ),
        Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            body,
            span,
        } => Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            span,
        },
        Stmt::Config { settings, span } => Stmt::Config {
            settings: settings.into_iter().map(|p| folder.fold_property(p)).collect(),
            span,
        },
        Stmt::Scene {
            name,
            config,
            body,
            span,
        } => Stmt::Scene {
            name,
            config: config.into_iter().map(|p| folder.fold_property(p)).collect(),
            body: body.into_iter().map(|s| folder.fold_stmt(s)).collect(),
            span,
        },
        other => other,
    }
}

/// Fold child nodes of an [`Expr`].
pub fn walk_fold_expr<F: AstFolder + ?Sized>(folder: &mut F, expr: Expr) -> Expr {
    match expr {
        Expr::Num(_) | Expr::Percent(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => expr,
        Expr::Ident(_) | Expr::Path(_) => expr,
        Expr::Index(target, index) => {
            Expr::Index(Box::new(folder.fold_expr(*target)), Box::new(folder.fold_expr(*index)))
        },
        Expr::Tuple(items) => Expr::Tuple(items.into_iter().map(|i| folder.fold_expr(i)).collect()),
        Expr::List(items) => Expr::List(items.into_iter().map(|i| folder.fold_expr(i)).collect()),
        Expr::Binary(left, op, right) => {
            Expr::Binary(Box::new(folder.fold_expr(*left)), op, Box::new(folder.fold_expr(*right)))
        },
        Expr::Unary(op, inner) => Expr::Unary(op, Box::new(folder.fold_expr(*inner))),
        Expr::Call(name, args) => {
            Expr::Call(name, args.into_iter().map(|a| folder.fold_expr(a)).collect())
        },
        Expr::Method(receiver, method, args) => Expr::Method(
            Box::new(folder.fold_expr(*receiver)),
            method,
            args.into_iter().map(|a| folder.fold_expr(a)).collect(),
        ),
        Expr::Closure(params, body) => Expr::Closure(params, Box::new(folder.fold_expr(*body))),
        Expr::LetChain(bindings, tail) => Expr::LetChain(
            bindings.into_iter().map(|(k, v)| (k, folder.fold_expr(v))).collect(),
            Box::new(folder.fold_expr(*tail)),
        ),
        Expr::Conditional(cond, then_branch, else_branch) => Expr::Conditional(
            Box::new(folder.fold_expr(*cond)),
            Box::new(folder.fold_expr(*then_branch)),
            Box::new(folder.fold_expr(*else_branch)),
        ),
        Expr::Match(scrutinee, arms) => Expr::Match(
            Box::new(folder.fold_expr(*scrutinee)),
            arms.into_iter()
                .map(|(pat, arm_expr)| (pat, Box::new(folder.fold_expr(*arm_expr))))
                .collect(),
        ),
        Expr::Construct(ty, props) => {
            Expr::Construct(ty, props.into_iter().map(|p| folder.fold_property(p)).collect())
        },
    }
}

/// Fold child nodes of an [`InlineItem`].
pub fn walk_fold_inline_item<F: AstFolder + ?Sized>(
    folder: &mut F,
    item: InlineItem,
) -> InlineItem {
    match item {
        InlineItem::Anonymous {
            ty,
            props,
            modifiers,
            children,
        } => InlineItem::Anonymous {
            ty,
            props: props.into_iter().map(|p| folder.fold_property(p)).collect(),
            modifiers: modifiers.into_iter().map(|m| folder.fold_modifier(m)).collect(),
            children: children.into_iter().map(|c| folder.fold_inline_item(c)).collect(),
        },
        InlineItem::Labeled {
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
        } => InlineItem::Labeled {
            label,
            array_index: array_index.map(|idx| folder.fold_expr(idx)),
            ty,
            props: props.into_iter().map(|p| folder.fold_property(p)).collect(),
            modifiers: modifiers.into_iter().map(|m| folder.fold_modifier(m)).collect(),
            children: children.into_iter().map(|c| folder.fold_inline_item(c)).collect(),
        },
        InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } => InlineItem::ForLoop {
            var,
            index_var,
            iterable: folder.fold_expr(iterable),
            body: body.into_iter().map(|c| folder.fold_inline_item(c)).collect(),
        },
        InlineItem::SlotFill { slot, items } => InlineItem::SlotFill {
            slot,
            items: items.into_iter().map(|c| folder.fold_inline_item(c)).collect(),
        },
        InlineItem::SlotMarker => InlineItem::SlotMarker,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct IdentifierCollector {
        idents: Vec<String>,
    }

    impl<'ast> AstVisitor<'ast> for IdentifierCollector {
        fn visit_expr(&mut self, expr: &'ast Expr) {
            if let Expr::Ident(name) = expr {
                self.idents.push(name.clone());
            }
            walk_expr(self, expr);
        }
    }

    #[test]
    fn visitor_collects_nested_identifiers() {
        let stmt = Stmt::LetDecl {
            is_pub: false,
            name: "x".to_string(),
            value: Expr::Binary(
                Box::new(Expr::Ident("a".to_string())),
                BinaryOp::Add,
                Box::new(Expr::Call("f".to_string(), vec![Expr::Ident("b".to_string())])),
            ),
            span: None,
        };

        let mut collector = IdentifierCollector { idents: Vec::new() };
        collector.visit_stmt(&stmt);
        assert_eq!(collector.idents, vec!["a".to_string(), "b".to_string()]);
    }

    struct IdentifierRenamer;

    impl AstFolder for IdentifierRenamer {
        fn fold_expr(&mut self, expr: Expr) -> Expr {
            match expr {
                Expr::Ident(name) if name == "old" => Expr::Ident("new".to_string()),
                other => walk_fold_expr(self, other),
            }
        }
    }

    #[test]
    fn folder_renames_identifiers_in_deep_tree() {
        let stmt = Stmt::Assignment {
            target: vec![TargetSegment::Static("rect".to_string())],
            property: "width".to_string(),
            value: Expr::Binary(
                Box::new(Expr::Ident("old".to_string())),
                BinaryOp::Mul,
                Box::new(Expr::Num(2.0)),
            ),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        };

        let folded = IdentifierRenamer.fold_stmt(stmt);
        match folded {
            Stmt::Assignment { value, .. } => match value {
                Expr::Binary(left, _, _) => {
                    assert_eq!(*left, Expr::Ident("new".to_string()));
                },
                other => panic!("expected binary expr, got {:?}", other),
            },
            other => panic!("expected assignment, got {:?}", other),
        }
    }
}
