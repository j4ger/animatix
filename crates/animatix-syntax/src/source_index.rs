//! Source index for mapping actor+property names to byte spans in source text.
//!
//! Originally used for surgical source edits (byte-span replacement), this is
//! now primarily for diagnostics, editor navigation, and go-to-definition.
//! The GUI inspector uses source edit (AST mutation +
//! re-serialization) for write-back instead.

use std::collections::HashMap;
use crate::ast::{ByteSpan, InlineItem, Stmt};

/// Index mapping actor labels + property names to their source byte spans.
///
/// Built by walking the AST once; used by the GUI inspector for go-to-definition
/// and by diagnostics to report precise source locations.
#[derive(Debug, Default, Clone)]
pub struct SourceIndex {
    /// Maps (actor_label, property_name) → ByteSpan for declaration properties.
    /// e.g., ("btn", "size") → ByteSpan { start: 50, end: 62 }
    decl_props: HashMap<(String, String), ByteSpan>,

    /// Maps (actor_label, property_name) → ByteSpan for assignment statements.
    /// e.g., ("btn", "color") → ByteSpan { start: 120, end: 130 }
    assignments: HashMap<(String, String), ByteSpan>,
}

impl SourceIndex {
    /// Build the index by walking all statements (and nested inline items).
    pub fn build(stmts: &[Stmt]) -> Self {
        let mut index = SourceIndex::default();
        index.walk(stmts);
        index
    }

    /// Find the byte span for an actor's property.
    ///
    /// Handles the "at" ↔ "position" name aliasing internally.
    ///
    /// When the same property appears in both a declaration and an assignment
    /// (e.g. `btn: Rect, size: (100, 100)` followed by `btn.size = (200, 200)`),
    /// the **assignment** span is returned because assignments override
    /// declarations at runtime.
    pub fn find(&self, actor: &str, property: &str) -> Option<ByteSpan> {
        let key = (actor.to_string(), property.to_string());

        // Assignments take precedence over declarations (they override at runtime).
        if let Some(span) = self.assignments.get(&key).or_else(|| self.decl_props.get(&key)) {
            return Some(*span);
        }

        // Try aliased name: "position" ↔ "at"
        let aliased = match property {
            "position" => "at",
            "at" => "position",
            _ => return None,
        };
        let aliased_key = (actor.to_string(), aliased.to_string());
        self.assignments
            .get(&aliased_key)
            .or_else(|| self.decl_props.get(&aliased_key))
            .copied()
    }

    /// Return the byte span of the **last** property value for `actor`
    /// (the one with the greatest `end` offset).
    ///
    /// Useful for inserting a brand-new property after the last existing one
    /// in a declaration line.
    pub fn last_property_span(&self, actor: &str) -> Option<ByteSpan> {
        let mut best: Option<ByteSpan> = None;
        let mut consider = |span: &ByteSpan| {
            if best.is_none_or(|b| span.end > b.end) {
                best = Some(*span);
            }
        };
        for ((label, _), span) in &self.decl_props {
            if label == actor {
                consider(span);
            }
        }
        for ((label, _), span) in &self.assignments {
            if label == actor {
                consider(span);
            }
        }
        best
    }

    fn walk(&mut self, stmts: &[Stmt]) {
        crate::walk::walk_stmts(stmts, &mut |stmt| {
            match stmt {
                Stmt::ActorDecl {
                    label, props, children, ..
                } => {
                    // Index properties on the actor declaration itself
                    for prop in props {
                        if let Some(span) = prop.value_span {
                            self.decl_props.insert(
                                (label.clone(), prop.name.clone()),
                                span,
                            );
                        }
                    }
                    // Inline children are not part of the statement walk, walk them separately
                    self.walk_inline_items(children);
                }
                Stmt::Assignment {
                    target,
                    property,
                    value_span: Some(span),
                    ..
                } => {
                    // target is a path like ["container", "child"]
                    // The actor label is the last segment
                    if let Some(actor) = target.last() {
                        self.assignments.insert(
                            (actor.clone(), property.clone()),
                            *span,
                        );
                    }
                }
                Stmt::ReactiveBinding { target, property, value_span, .. } => {
                    // Reactive bindings are indexed like assignments
                    if let Some(actor) = target.last() {
                        self.assignments.insert(
                            (actor.clone(), property.clone()),
                            value_span.unwrap_or_default(),
                        );
                    }
                }
                Stmt::Config { settings, .. } => {
                    // Config properties use "at" syntax
                    for prop in settings {
                        if let Some(span) = prop.value_span {
                            self.decl_props.insert(
                                ("config".to_string(), prop.name.clone()),
                                span,
                            );
                        }
                    }
                }
                _ => {}
            }
        });
    }

    fn walk_inline_items(&mut self, items: &[InlineItem]) {
        crate::walk::walk_inline_items(items, &mut |item| {
            match item {
                InlineItem::Labeled {
                    label, props, ..
                } => {
                    for prop in props {
                        if let Some(span) = prop.value_span {
                            self.decl_props.insert(
                                (label.clone(), prop.name.clone()),
                                span,
                            );
                        }
                    }
                }
                InlineItem::Anonymous { props, .. } => {
                    // Anonymous items don't have labels, skip for now
                    let _ = props;
                }
                _ => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Property, Time};

    fn make_byte_span(start: usize, end: usize) -> ByteSpan {
        ByteSpan { start, end }
    }

    #[test]
    fn source_index_indexes_actor_decl_properties() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "btn".to_string(),
            array_index: None,
            ty: "Button".to_string(),
            props: vec![
                Property {
                    name: "size".to_string(),
                    value: Expr::Num(100.0),
                    trailing_comment: None,
                    value_span: Some(make_byte_span(20, 30)), },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("red".to_string()),
                    trailing_comment: None,
                    value_span: Some(make_byte_span(32, 42)), },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];

        let index = SourceIndex::build(&stmts);
        assert_eq!(
            index.find("btn", "size"),
            Some(make_byte_span(20, 30))
        );
        assert_eq!(
            index.find("btn", "color"),
            Some(make_byte_span(32, 42))
        );
        assert_eq!(index.find("btn", "nonexistent"), None);
    }

    #[test]
    fn source_index_indexes_assignments() {
        let stmts = vec![Stmt::Assignment {
            target: vec!["btn".to_string()],
            property: "color".to_string(),
            value: Expr::Ident("blue".to_string()),
            modifiers: vec![],
            easing: None,
            value_span: Some(make_byte_span(50, 60)),
            span: None,
        }];

        let index = SourceIndex::build(&stmts);
        assert_eq!(
            index.find("btn", "color"),
            Some(make_byte_span(50, 60))
        );
    }

    #[test]
    fn source_index_handles_at_position_aliasing() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "icon".to_string(),
            array_index: None,
            ty: "Image".to_string(),
            props: vec![Property {
                name: "at".to_string(),
                value: Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)]),
                trailing_comment: None,
                value_span: Some(make_byte_span(15, 35)), }],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];

        let index = SourceIndex::build(&stmts);
        // Query with "at"
        assert_eq!(
            index.find("icon", "at"),
            Some(make_byte_span(15, 35))
        );
        // Query with "position" should also work
        assert_eq!(
            index.find("icon", "position"),
            Some(make_byte_span(15, 35))
        );
    }

    #[test]
    fn source_index_handles_nested_paths() {
        // btn.inner.color = red
        let stmts = vec![Stmt::Assignment {
            target: vec!["btn".to_string(), "inner".to_string()],
            property: "color".to_string(),
            value: Expr::Ident("red".to_string()),
            modifiers: vec![],
            easing: None,
            value_span: Some(make_byte_span(100, 110)),
            span: None,
        }];

        let index = SourceIndex::build(&stmts);
        // The actor label is the last segment of the path
        assert_eq!(
            index.find("inner", "color"),
            Some(make_byte_span(100, 110))
        );
        // btn.inner is the full path, not the actor
        assert_eq!(index.find("btn.inner", "color"), None);
    }

    #[test]
    fn source_index_empty_for_invalid_stmts() {
        let stmts = vec![
            Stmt::LetDecl {
                is_pub: false,
                name: "x".to_string(),
                value: Expr::Num(0.0),
                span: None,
            },
            Stmt::Action(crate::ast::Action {
                verb: "move".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
                byte_span: None,
            }, None),
        ];

        let index = SourceIndex::build(&stmts);
        assert_eq!(index.find("btn", "anything"), None);
    }

    #[test]
    fn source_index_from_stmt_assignment_panics_on_none_value_span() {
        // Assignment with None value_span should not panic - it just won't be indexed
        let stmts = vec![Stmt::Assignment {
            target: vec!["btn".to_string()],
            property: "color".to_string(),
            value: Expr::Ident("red".to_string()),
            modifiers: vec![],
            easing: None,
            value_span: None,
            span: None,
        }];

        let index = SourceIndex::build(&stmts);
        assert_eq!(index.find("btn", "color"), None);
    }

    #[test]
    fn source_index_from_ast_rt_property_with_span() {
        // Real-world style: ast_rt creates Property without value_span via module rewrite
        // but our parser now populates it. This verifies the index works end-to-end.
        let stmts = vec![Stmt::Keyframe {
            time: Time::Seconds(0.0),
            body: vec![Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "card".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                    trailing_comment: None,
                    value_span: Some(make_byte_span(40, 60)), }],
                modifiers: vec![],
                children: vec![],
                span: None,
            }],
            span: None,
        }];

        let index = SourceIndex::build(&stmts);
        assert_eq!(
            index.find("card", "size"),
            Some(make_byte_span(40, 60))
        );
    }
}
