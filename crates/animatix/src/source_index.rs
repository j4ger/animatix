//! Source index for mapping actor+property names to byte spans in source text.
//!
//! This enables surgical source edits: when a user changes a property via the
//! inspector widget, we look up the byte span and replace just that portion
//! of the source text rather than re-serializing the entire file.

use std::collections::HashMap;
use crate::ast::{ByteSpan, InlineItem, Stmt};

#[derive(Debug, Default)]
pub struct SourceIndex {
    /// Maps (actor_label, property_name) → ByteSpan for declaration properties.
    /// e.g., ("btn", "size") → ByteSpan { start: 50, end: 62 }
    decl_props: HashMap<(String, String), ByteSpan>,

    /// Maps (actor_label, property_name) → ByteSpan for assignment statements.
    /// e.g., ("btn", "color") → ByteSpan { start: 120, end: 130 }
    assignments: HashMap<(String, String), ByteSpan>,
}

impl SourceIndex {
    pub fn build(stmts: &[Stmt]) -> Self {
        let mut index = SourceIndex::default();
        index.walk(stmts);
        index
    }

    /// Find the byte span for an actor's property.
    ///
    /// Handles the "at" ↔ "position" name aliasing internally.
    pub fn find(&self, actor: &str, property: &str) -> Option<ByteSpan> {
        let key = (actor.to_string(), property.to_string());

        // Try exact match first
        if let Some(span) = self.decl_props.get(&key).or_else(|| self.assignments.get(&key)) {
            return Some(*span);
        }

        // Try aliased name: "position" ↔ "at"
        let aliased = match property {
            "position" => "at",
            "at" => "position",
            _ => return None,
        };
        let aliased_key = (actor.to_string(), aliased.to_string());
        self.decl_props
            .get(&aliased_key)
            .or_else(|| self.assignments.get(&aliased_key))
            .copied()
    }

    fn walk(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
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
                    // Recurse into inline children
                    self.walk_inline_items(children);
                }
                Stmt::Assignment {
                    target,
                    property,
                    value_span,
                    ..
                } => {
                    if let Some(span) = value_span {
                        // target is a path like ["container", "child"]
                        // The actor label is the last segment
                        if let Some(actor) = target.last() {
                            self.assignments.insert(
                                (actor.clone(), property.clone()),
                                *span,
                            );
                        }
                    }
                }
                Stmt::Keyframe { body, .. }
                | Stmt::RelativeKeyframe { body, .. }
                | Stmt::Sequence { body }
                | Stmt::Stagger { body, .. }
                | Stmt::Always { body }
                | Stmt::LabeledAlways { body, .. }
                | Stmt::Conditional { then_branch: body, .. }
                | Stmt::ForLoop { body, .. } => {
                    self.walk(body);
                }
                Stmt::Config { settings } => {
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
        }
    }

    fn walk_inline_items(&mut self, items: &[InlineItem]) {
        for item in items {
            match item {
                InlineItem::Labeled {
                    label, props, children, ..
                } => {
                    for prop in props {
                        if let Some(span) = prop.value_span {
                            self.decl_props.insert(
                                (label.clone(), prop.name.clone()),
                                span,
                            );
                        }
                    }
                    self.walk_inline_items(children);
                }
                InlineItem::Anonymous { props, children, .. } => {
                    for prop in props {
                        if let Some(span) = prop.value_span {
                            // Anonymous items don't have labels, skip for now
                            // or we could generate a synthetic key
                            let _ = (span, prop.name.clone());
                        }
                    }
                    self.walk_inline_items(children);
                }
                InlineItem::SlotFill { items, .. } => {
                    self.walk_inline_items(items);
                }
                InlineItem::SlotMarker => {}
            }
        }
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
            label: "btn".to_string(),
            ty: "Button".to_string(),
            props: vec![
                Property {
                    name: "size".to_string(),
                    value: Expr::Num(100.0),
                    value_span: Some(make_byte_span(20, 30)), ..Default::default() },
                Property {
                    name: "color".to_string(),
                    value: Expr::Ident("red".to_string()),
                    value_span: Some(make_byte_span(32, 42)), ..Default::default() },
            ],
            modifiers: vec![],
            children: vec![],
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
            value_span: Some(make_byte_span(50, 60)),
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
            label: "icon".to_string(),
            ty: "Image".to_string(),
            props: vec![Property {
                name: "at".to_string(),
                value: Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(20.0)]),
                value_span: Some(make_byte_span(15, 35)), ..Default::default() }],
            modifiers: vec![],
            children: vec![],
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
            value_span: Some(make_byte_span(100, 110)),
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
            },
            Stmt::Action(crate::ast::Action {
                verb: "move".to_string(),
                targets: vec!["btn".to_string()],
                args: vec![],
                modifiers: vec![],
            }),
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
            value_span: None,
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
                label: "card".to_string(),
                ty: "Rect".to_string(),
                props: vec![Property {
                    name: "size".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(100.0)]),
                    value_span: Some(make_byte_span(40, 60)), ..Default::default() }],
                modifiers: vec![],
                children: vec![],
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
