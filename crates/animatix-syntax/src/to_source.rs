//! This is a thin trait wrapper over format_core. It provides ergonomic
//! `.to_source()` calls with fixed indent = 2.
//!
//! All formatting logic lives in [`format_core`]. If you
//! need configurable formatting, use [`formatter::Formatter`](crate::formatter::Formatter).
//! This module is the canonical 2-space serialization API.

use crate::ast::*;
use crate::format_core;

/// Trait for serializing AST nodes back to `.amx` source text.
pub trait ToSource {
    /// Serialize to a source text string.
    fn to_source(&self) -> String;
}

// ── Recursive types: delegate to format_core ────────────────────────────

impl ToSource for Expr {
    fn to_source(&self) -> String {
        format_core::format_expr(self)
    }
}

impl ToSource for InlineItem {
    fn to_source(&self) -> String {
        format_core::format_inline_item(self, 0, 2)
    }
}

impl ToSource for Stmt {
    fn to_source(&self) -> String {
        format_core::format_stmt_raw(self, 0, 2)
    }
}

impl ToSource for ComponentDef {
    fn to_source(&self) -> String {
        format_core::format_component_def(self, 0, 2)
    }
}

// ── Leaf types: delegate to format_core ──────────────────────────────

impl ToSource for Property {
    fn to_source(&self) -> String {
        format_core::format_property(self)
    }
}

impl ToSource for Modifier {
    fn to_source(&self) -> String {
        format_core::format_modifier(self)
    }
}

impl ToSource for Time {
    fn to_source(&self) -> String {
        format_core::format_time(self)
    }
}

impl ToSource for ParamDef {
    fn to_source(&self) -> String {
        format_core::format_param_def(self)
    }
}

impl ToSource for Transition {
    fn to_source(&self) -> String {
        format_core::format_transition(self)
    }
}

// ── Free functions ──────────────────────────────────────────────────────

/// Serialize a single expression to source text.
///
/// Convenience free-function equivalent to `expr.to_source()`.
pub fn expr_to_source(expr: &Expr) -> String {
    format_core::format_expr(expr)
}

/// Serialize a top-level statement list (the contents of an `.amx` file).
///
/// Keyframe blocks are separated by a single blank line for readability.
///
/// This is the main entry-point for GUI write-back: after the inspector mutates
/// the AST, the entire tree is re-serialized via this function to produce the
/// updated source text.
pub fn stmts_to_source(stmts: &[Stmt]) -> String {
    stmts.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Stmt};

    // Negative-value checks for the serialiser (exact text, comment
    // preservation, single-expression shapes) live here. Whole-document
    // roundtrip fidelity is covered by `tests/ast_format_coverage.rs`, which
    // probes every `Stmt` variant and compares scrubbed ASTs for structural
    // equality, so a length-only comparison here would duplicate it weakly.

    const SHOWCASE_FIXTURE: &str = r#"// Animatix Showcase
config { colorscheme: "editorial-dark", canvas_size: (1280, 720) }

fn reveal(actor: Actor, duration: Duration = 500ms) -> Timeline {
  #{
    fade_in(target: actor, duration: duration, ease: ease_out)
  }
}

#0s
backdrop: Rect, size: (1280, 720), color: scene.background
title: Text, text: "Animatix", font_size: 96, color: text.primary

#0.5s
reveal(actor: backdrop, duration: 800ms)

#1s
#{
  sequence {
    shift(target: title, by: (0, 12), duration: 300ms, ease: ease_in_out)
    fade_in(target: title, duration: 600ms)
  }
}
"#;

    #[test]
    fn serialize_vec2() {
        let expr = Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]);
        assert_eq!(expr.to_source(), "(100, 200)");
    }

    #[test]
    fn serialize_color_rgb() {
        let expr = Expr::call("rgb", vec![Expr::Num(1.0), Expr::Num(0.5), Expr::Num(0.0)]);
        assert_eq!(expr.to_source(), "rgb(1, 0.5, 0)");
    }

    #[test]
    fn serialize_string_with_quotes() {
        let expr = Expr::Str(r#"Say "hello""#.into());
        assert_eq!(expr.to_source(), r#""Say \"hello\"""#);
    }

    #[test]
    fn serialize_property() {
        let prop = Property {
            name: "size".into(),
            value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
            value_span: None,
            trailing_comment: None,
        };
        assert_eq!(prop.to_source(), "size: (100, 200)");
    }

    #[test]
    fn roundtrip_full_showcase_file() {
        let source = SHOWCASE_FIXTURE;
        let parsed = crate::parser::parse_source(source).0.unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parse_source(&serialized).0.unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn trailing_comment_preserved_on_property() {
        let source = r#"#0s
btn: Rect, size: (100, 200) // half-extents, in scene space"#;
        let parsed = crate::parser::parse_source(source).0.unwrap();
        let serialized = stmts_to_source(&parsed);
        assert!(
            serialized.contains("// half-extents, in scene space"),
            "trailing comment lost in serialization: {}",
            serialized
        );
    }

    #[test]
    fn trailing_comment_roundtrips() {
        let source = r#"#0s
btn: Rect, size: (100, 200) // half-extents"#;
        let parsed = crate::parser::parse_source(source).0.unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parse_source(&serialized).0.unwrap();

        if let Stmt::Keyframe { body, .. } = &reparsed[0] {
            if let Stmt::ActorDecl { props, .. } = &body[0] {
                let size_prop = props.iter().find(|p| p.name == "size").unwrap();
                assert_eq!(size_prop.trailing_comment.as_deref(), Some(" half-extents"));
            } else {
                panic!("Expected ActorDecl");
            }
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn block_comment_rejected() {
        let source = r#"/* block comment */ btn: Rect"#;
        let result = crate::parser::parse_source(source);
        let (_, errors) = result;
        assert!(!errors.is_empty(), "block comment should be rejected");
    }

    #[test]
    fn serialize_property_with_trailing_comment() {
        let prop = Property {
            name: "size".into(),
            value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
            value_span: None,
            trailing_comment: Some(" half-extents".into()),
        };
        assert_eq!(prop.to_source(), "size: (100, 200)  // half-extents");
    }

    #[test]
    fn roundtrip_reactive_binding() {
        let source = r#"orbiter.at := tracker.at + (200 * cos(3 * t), 200 * sin(3 * t))"#;
        let parsed = crate::parser::parse_source(source).0.unwrap();
        assert_eq!(parsed.len(), 1);
        if let Stmt::ReactiveBinding {
            target, property, ..
        } = &parsed[0]
        {
            assert_eq!(target, &[TargetSegment::Static("orbiter".to_string())]);
            assert_eq!(property, "at");
        } else {
            panic!("Expected ReactiveBinding");
        }
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parse_source(&serialized).0.unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }
}
