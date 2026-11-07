//! Shared AST traversal primitives.
//!
//! Provides generic walk functions for `Stmt`, `InlineItem`, and `Expr` trees,
//! plus utility find functions used across the crate and by `animatix-gui`.

use crate::ast::{ComponentDef, Expr, InlineItem, Property, Stmt, Time};
use crate::visit::AstVisitor;

// Statement walkers

struct FnStmtVisitor<'f>(&'f mut dyn FnMut(&Stmt));

impl<'ast> crate::visit::AstVisitor<'ast> for FnStmtVisitor<'_> {
    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        (self.0)(stmt);
        crate::visit::walk_stmt(self, stmt);
    }
}

/// Walk over all statements, calling the visitor for each and recursing into block bodies.
pub fn walk_stmts(stmts: &[Stmt], visitor: &mut dyn FnMut(&Stmt)) {
    let mut v = FnStmtVisitor(visitor);
    for stmt in stmts {
        v.visit_stmt(stmt);
    }
}

/// Mutable variant of walk_stmts.
pub fn walk_stmts_mut(stmts: &mut [Stmt], visitor: &mut dyn FnMut(&mut Stmt)) {
    for stmt in stmts.iter_mut() {
        walk_stmt_mut(stmt, visitor);
    }
}

/// Visit a single statement and recurse into its children (block bodies).
pub fn walk_stmt(stmt: &Stmt, visitor: &mut dyn FnMut(&Stmt)) {
    let mut v = FnStmtVisitor(visitor);
    v.visit_stmt(stmt);
}

/// Mutable variant of walk_stmt.
pub fn walk_stmt_mut(stmt: &mut Stmt, visitor: &mut dyn FnMut(&mut Stmt)) {
    visitor(stmt);
    recurse_stmt_bodies_mut(stmt, &mut |body| walk_stmts_mut(body, visitor));
}

// Inline item walkers

struct FnInlineVisitor<'f>(&'f mut dyn FnMut(&InlineItem));

impl<'ast> crate::visit::AstVisitor<'ast> for FnInlineVisitor<'_> {
    fn visit_inline_item(&mut self, item: &'ast InlineItem) {
        (self.0)(item);
        crate::visit::walk_inline_item(self, item);
    }
}

/// Walk over inline items, calling the visitor for each and recursing into children.
pub fn walk_inline_items(items: &[InlineItem], visitor: &mut dyn FnMut(&InlineItem)) {
    let mut v = FnInlineVisitor(visitor);
    for item in items {
        v.visit_inline_item(item);
    }
}

/// Mutable variant of walk_inline_items.
pub fn walk_inline_items_mut(items: &mut [InlineItem], visitor: &mut dyn FnMut(&mut InlineItem)) {
    for item in items.iter_mut() {
        walk_inline_item_mut(item, visitor);
    }
}

/// Visit a single inline item and recurse into its children.
pub fn walk_inline_item(item: &InlineItem, visitor: &mut dyn FnMut(&InlineItem)) {
    let mut v = FnInlineVisitor(visitor);
    v.visit_inline_item(item);
}

/// Mutable variant of walk_inline_item.
pub fn walk_inline_item_mut(item: &mut InlineItem, visitor: &mut dyn FnMut(&mut InlineItem)) {
    visitor(item);
    match item {
        InlineItem::Labeled { children, .. } | InlineItem::Anonymous { children, .. } => {
            walk_inline_items_mut(children, visitor);
        },
        InlineItem::ForLoop { body, .. } => {
            walk_inline_items_mut(body, visitor);
        },
        InlineItem::SlotFill { items, .. } => {
            walk_inline_items_mut(items, visitor);
        },
        InlineItem::SlotMarker => {},
    }
}

// Expression walker

struct FnExprVisitor<'f>(&'f mut dyn FnMut(&Expr));

impl<'ast> crate::visit::AstVisitor<'ast> for FnExprVisitor<'_> {
    fn visit_expr(&mut self, expr: &'ast Expr) {
        (self.0)(expr);
        crate::visit::walk_expr(self, expr);
    }
}

/// Walk over an expression, visiting it and all sub-expressions.
pub fn walk_expr(expr: &Expr, visitor: &mut dyn FnMut(&Expr)) {
    let mut v = FnExprVisitor(visitor);
    v.visit_expr(expr);
}

// Body recursion helpers

/// Collect mutable body references from a statement (used internally).
pub fn collect_stmt_bodies_mut(stmt: &mut Stmt) -> Vec<&mut Vec<Stmt>> {
    match stmt {
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ComponentDef(crate::ast::ComponentDef { body, .. }, _)
        | Stmt::FnDecl { body, .. }
        | Stmt::Scene { body, .. }
        | Stmt::Block { body, .. } => {
            vec![body]
        },
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            let mut bodies = vec![then_branch];
            if let Some(else_body) = else_branch {
                bodies.push(else_body);
            }
            bodies
        },
        Stmt::Match { arms, .. } => arms.iter_mut().map(|(_, body)| body).collect(),
        Stmt::ForLoop { body, .. } => {
            vec![body]
        },
        _ => vec![],
    }
}

/// Recurse into statement bodies (immutable).
pub fn recurse_stmt_bodies(stmt: &Stmt, f: &mut dyn FnMut(&[Stmt])) {
    match stmt {
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ComponentDef(crate::ast::ComponentDef { body, .. }, _)
        | Stmt::FnDecl { body, .. }
        | Stmt::Scene { body, .. }
        | Stmt::Block { body, .. } => {
            f(body);
        },
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            f(then_branch);
            if let Some(else_body) = else_branch {
                f(else_body);
            }
        },
        Stmt::Match { arms, .. } => {
            for (_, body) in arms {
                f(body);
            }
        },
        Stmt::ForLoop { body, .. } => {
            f(body);
        },
        _ => {},
    }
}

/// Recurse into statement bodies (mutable).
pub fn recurse_stmt_bodies_mut(stmt: &mut Stmt, f: &mut dyn FnMut(&mut [Stmt])) {
    let bodies = collect_stmt_bodies_mut(stmt);
    for body in bodies {
        f(body);
    }
}

// Find helpers

/// Find an ActorDecl with the given label anywhere in the statement tree.
pub fn find_actor_decl<'a>(stmts: &'a [Stmt], label: &str) -> Option<&'a Stmt> {
    for stmt in stmts.iter() {
        if let Stmt::ActorDecl { label: l, .. } = stmt
            && l == label
        {
            return Some(stmt);
        }
        if let Some(found) = find_stmt_in_bodies(stmt, |body| find_actor_decl(body, label)) {
            return Some(found);
        }
    }
    None
}

/// Mutable variant of find_actor_decl.
pub fn find_actor_decl_mut<'a>(stmts: &'a mut [Stmt], label: &str) -> Option<&'a mut Stmt> {
    for stmt in stmts.iter_mut() {
        if let Stmt::ActorDecl { label: l, .. } = stmt
            && l == label
        {
            return Some(stmt);
        }
        if let Some(found) = find_stmt_in_bodies_mut(stmt, |body| find_actor_decl_mut(body, label))
        {
            return Some(found);
        }
    }
    None
}

/// Find a mutable reference to an Assignment statement for the given actor
/// and property anywhere in the statement tree.
pub fn find_assignment_mut<'a>(
    stmts: &'a mut [Stmt],
    actor: &str,
    property: &str,
) -> Option<&'a mut Stmt> {
    for stmt in stmts.iter_mut() {
        if let Stmt::Assignment {
            target,
            property: prop,
            ..
        } = stmt
            && target.last().and_then(|t| t.as_static_str()).is_some_and(|t| t == actor)
            && prop == property
        {
            return Some(stmt);
        }
        if let Some(found) =
            find_stmt_in_bodies_mut(stmt, |body| find_assignment_mut(body, actor, property))
        {
            return Some(found);
        }
    }
    None
}

/// Find a scene declaration by name at top level (no recursion into block bodies).
pub fn find_scene_mut<'a>(stmts: &'a mut [Stmt], name: &str) -> Option<&'a mut Stmt> {
    stmts
        .iter_mut()
        .find(|stmt| matches!(stmt, Stmt::Scene { name: scene_name, .. } if scene_name == name))
}

/// Find a property by name inside an actor-like statement.
pub fn find_prop_mut<'a>(stmt: &'a mut Stmt, name: &str) -> Option<&'a mut Property> {
    let props: &mut Vec<Property> = match stmt {
        Stmt::ActorDecl { props, .. } => props,
        _ => return None,
    };
    props.iter_mut().find(|p| p.name == name)
}

/// Convert a Time value to seconds as a floating-point number.
pub fn time_to_seconds(t: &Time) -> f64 {
    match t {
        Time::Seconds(s) => *s,
        Time::Milliseconds(ms) => *ms as f64 / 1000.0,
        // Beat stamps resolve against the scene tempo; callers that have no
        // config in scope fall back to the documented default.
        Time::Beats(b) => *b * crate::ast::beat_seconds(crate::ast::DEFAULT_BPM),
        Time::SpeechMarker { offset_ms, .. } | Time::SpeechWord { offset_ms, .. } => {
            *offset_ms as f64 / 1000.0
        },
    }
}

// Internal helpers for find functions

/// Helper: search child bodies of a stmt (immutable), returning early if found.
fn find_stmt_in_bodies<'a>(
    stmt: &'a Stmt,
    f: impl Fn(&'a [Stmt]) -> Option<&'a Stmt>,
) -> Option<&'a Stmt> {
    match stmt {
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ComponentDef(ComponentDef { body, .. }, _)
        | Stmt::FnDecl { body, .. }
        | Stmt::Scene { body, .. }
        | Stmt::Block { body, .. } => f(body),
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            if let Some(found) = f(then_branch) {
                return Some(found);
            }
            if let Some(else_body) = else_branch
                && let Some(found) = f(else_body)
            {
                return Some(found);
            }
            None
        },
        Stmt::ForLoop { body, .. } => f(body),
        _ => None,
    }
}

/// Helper: search child bodies of a stmt (mutable), returning early if found.
fn find_stmt_in_bodies_mut<'a>(
    stmt: &'a mut Stmt,
    mut f: impl FnMut(&'a mut [Stmt]) -> Option<&'a mut Stmt>,
) -> Option<&'a mut Stmt> {
    // This relies on splitting the match into separate arms to satisfy the borrow checker.
    // Each arm handles one variant, extracting bodies with the right lifetime.
    match stmt {
        Stmt::Keyframe { body, .. } => f(body),
        Stmt::RelativeKeyframe { body, .. } => f(body),
        Stmt::Sequence { body, .. } => f(body),
        Stmt::Stagger { body, .. } => f(body),
        Stmt::Always { body, .. } => f(body),
        Stmt::ComponentDef(ComponentDef { body, .. }, _) => f(body),
        Stmt::FnDecl { body, .. } => f(body),
        Stmt::Scene { body, .. } => f(body),
        Stmt::Block { body, .. } => f(body),
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            if let Some(found) = f(then_branch) {
                return Some(found);
            }
            if let Some(else_branch_bodies) = else_branch.as_mut()
                && let Some(found) = f(else_branch_bodies)
            {
                return Some(found);
            }
            None
        },
        Stmt::ForLoop { body, .. } => f(body),
        _ => None,
    }
}
