//! This is a thin trait wrapper over format_core. It provides ergonomic
//! `.to_source()` calls with fixed indent = 2.
//!
//! All formatting logic lives in [`format_core`](crate::format_core). If you
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

impl ToSource for Action {
    fn to_source(&self) -> String {
        format_core::format_action(self)
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
    stmts
        .iter()
        .map(|s| s.to_source())
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Stmt, Time};
    use chumsky::Parser;

    const SHOWCASE_FIXTURE: &str = r#"// Animatix Showcase
// A 5-second demonstration of layout, animation, primitives, and composition.

config { colorscheme: "editorial-dark", resolution: (1280, 720), dynamic_layout: true }

#0s
backdrop: Rect, size: (1280, 720), color: scene.background, anchor: scene.center
title: Text, text: "Animatix", font_size: 96, color: text.primary, anchor: scene.top, offset: (0, 120)
subtitle: Text, text: "Layout-first animation", font_size: 24, color: text.secondary, anchor: scene.top, offset: (0, 200)

#0.2s
fade-in backdrop [800ms, ease: ease-out]

#0.4s
fade-in title [600ms, ease: ease-out]

#0.8s
fade-in subtitle [600ms, ease: ease-in-out]

#1.0s
features: Col, anchor: scene.left, offset: (180, 0), align: "center", gap: 16 {
  label: Text, text: "Features", font_size: 28, color: accent.warning
  a: Text, text: "Rich typography", font_size: 18, color: text.secondary
  b: Text, text: "Math expressions", font_size: 18, color: text.secondary
  c: Text, text: "Code highlighting", font_size: 18, color: text.secondary
  d: Text, text: "Vector graphics", font_size: 18, color: text.secondary
}

#1.2s
shift features [by: (60, 0), 800ms, ease: ease-out]

#1.4s
stagger [100ms] {
  fade-in label [400ms]
  fade-in a [400ms]
  fade-in b [400ms]
  fade-in c [400ms]
  fade-in d [400ms]
}

#2.6s
primitives: Row, anchor: scene.bottom, offset: (0, -140), gap: 80, align: "center" {
  orb: Ellipse, size: (0, 0), color: accent.danger
  connector: Line, from: (0, 0), to: (0, 0), stroke: accent.primary, stroke_width: 4
  box: Rect, size: (0, 0), color: accent.success
  equation: Math, math: "x", font_size: 48, color: accent.primary
}

#2.6s
fade-in primitives [400ms]

#2.8s
orb.size = (120, 120) [600ms, ease: bounce]

#2.9s
connector.from = (-50, 0) [400ms, ease: ease-out]
connector.to = (50, 0) [400ms, ease: ease-out]

#3.0s
box.size = (120, 80) [500ms, ease: ease-out]

#3.2s
equation: Math, math: "E = mc^2", font_size: 48, color: accent.primary [800ms, ease: ease-in-out]

#3.4s
orb.color = accent.success [600ms, ease: ease-in-out]

#3.6s
box.color = accent.warning [600ms, ease: ease-in-out]

#4.2s
panel: Rect, size: (400, 180), color: (0.15, 0.17, 0.22, 1.0), anchor: scene.right, offset: (-220, -40)

#4.2s
shift panel [by: (-40, 0), 600ms, ease: ease-out]

#4.2s
fade-in panel [400ms]

#4.3s
code: Code, code: "let velocity = x + 1", font_size: 20, color: text.primary, anchor: scene.right, offset: (-220, -40)
shift code [by: (-40, 0), 600ms, ease: ease-out]
fade-in code [800ms, ease: ease-in-out]

#4.6s
ending_halo: Ellipse, size: (0, 0), color: accent.primary, at: (1040, 540)
ending_logo: Svg, url: "logo.svg", at: (1040, 540), scale: 0.0
ending_beam_a: Line, from: (1040, 540), to: (1040, 540), stroke: accent.success, stroke_width: 4
ending_beam_b: Line, from: (1040, 540), to: (1040, 540), stroke: accent.danger, stroke_width: 4
ending_kicker: Text, text: "Scene → Story", font_size: 20, color: accent.warning, anchor: scene.right, offset: (-180, 84)
ending_caption: Text, text: "Primitives composed in time", font_size: 16, color: text.secondary, anchor: scene.right, offset: (-180, 116)

#4.6s
stagger [80ms] {
  fade-in ending_halo [260ms]
  fade-in ending_logo [260ms]
  fade-in ending_kicker [320ms]
  fade-in ending_caption [320ms]
}

#4.68s
ending_halo.size = (108, 108) [520ms, ease: ease-out]
scale ending_logo [by: 1.0, 480ms, ease: bounce]

#4.78s
ending_beam_a.from = (990, 500) [420ms, ease: ease-out]
ending_beam_a.to = (1090, 580) [420ms, ease: ease-out]
ending_beam_b.from = (990, 580) [420ms, ease: ease-out]
ending_beam_b.to = (1090, 500) [420ms, ease: ease-out]

#4.92s
rotate ending_logo [by: 0.12, 700ms, ease: ease-in-out]

#5.0s
sequence {
  scale title [by: 1.02, 400ms, ease: ease-in-out]
  scale title [by: 0.98, 400ms, ease: ease-in-out]
}

#5.1s
pulse ending_halo [500ms, intensity: 0.18]

#5.2s
shift primitives [by: (0, -10), 2s, ease: ease-in-out]

#5.2s
shift ending_halo [by: (0, -8), 2s, ease: ease-in-out]
shift ending_logo [by: (0, -8), 2s, ease: ease-in-out]
shift ending_beam_a [by: (0, -8), 2s, ease: ease-in-out]
shift ending_beam_b [by: (0, -8), 2s, ease: ease-in-out]

#5.5s
equation: Math, math: "E = mc^2", font_size: 48, color: accent.danger [1s, ease: ease-in-out]
"#;

    const REORDER_FIXTURE: &str = r#"// Reorder Demo
// Demonstrates the reorder action for explicit full-order animation.

config { colorscheme: "editorial-dark", resolution: (1280, 720), dynamic_layout: true }

#0s
title: Text, text: "Reorder Action", font_size: 48, color: text.primary, anchor: scene.top, offset: (0, 60)
subtitle: Text, text: "Explicit full-order container animation", font_size: 20, color: text.secondary, anchor: scene.top, offset: (0, 120)

#0s
blocks: Row, anchor: scene.center, gap: 16, align: "center" {
  red: Rect, size: (80, 80), color: accent.danger
  yellow: Rect, size: (80, 80), color: accent.warning
  green: Rect, size: (80, 80), color: accent.success
  blue: Rect, size: (80, 80), color: accent.primary
}

#0.5s
fade-in title [800ms, ease: ease-out]
fade-in subtitle [600ms, delay: 200ms, ease: ease-out]

#1.0s
fade-in blocks [600ms, ease: ease-out]

// Reverse the row
#2.0s
reorder blocks [order: (blue, green, yellow, red), 800ms, ease: ease-in-out]

// Shuffle: every other
#3.5s
reorder blocks [order: (yellow, blue, red, green), 800ms, ease: ease-in-out]

// Back to original
#5.0s
reorder blocks [order: (red, yellow, green, blue), 800ms, ease: ease-in-out]

// Highlight completion
#6.5s
red.color = accent.success [300ms]
yellow.color = accent.success [300ms]
green.color = accent.success [300ms]
blue.color = accent.success [300ms]

#7.0s
done: Text, text: "Done!", font_size: 32, color: accent.success, anchor: scene.bottom, offset: (0, -80)

#7.0s
fade-in done [400ms, ease: ease-out]
"#;

    #[test]
    fn roundtrip_simple_actor_decl() {
        let source = r#"backdrop: Rect, size: (1280, 720), color: scene.background"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_keyframe_block() {
        let source = r#"#2s
fade-in title [600ms, ease: ease-out]"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
        if let Stmt::Keyframe { time, .. } = &reparsed[0] {
            assert_eq!(*time, Time::Seconds(2.0));
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn roundtrip_actor_with_children() {
        let source = r#"#0s
features: Col, anchor: scene.left {
    label: Text, text: "Features", font_size: 28
}"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_assignment() {
        let source = r#"orb.color = accent.success [600ms, ease: ease-in-out]"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_sequence() {
        let source = r#"sequence {
    fade-in a [400ms]
    fade-in b [400ms]
}"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_config() {
        let source = r#"config { resolution: (1280, 720), dynamic_layout: true }"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_complex_expressions() {
        let source = r#"let x = (a + b) * c"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn serialize_vec2() {
        let expr = Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]);
        assert_eq!(expr.to_source(), "(100, 200)");
    }

    #[test]
    fn serialize_color_rgb() {
        let expr = Expr::Call(
            "rgb".into(),
            vec![Expr::Num(255.0), Expr::Num(128.0), Expr::Num(0.0)],
        );
        assert_eq!(expr.to_source(), "rgb(255, 128, 0)");
    }

    #[test]
    fn serialize_string_with_quotes() {
        let expr = Expr::Str(r#"Say "hello""#.into());
        assert_eq!(expr.to_source(), r#""Say \"hello\"""#);
    }

    #[test]
    fn roundtrip_comment() {
        let source = r#"// This is a comment"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
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
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn trailing_comment_preserved_on_property() {
        let source = r#"#0s
btn: Rect, size: (100, 200) // half-extents, in scene space"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
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
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();

        if let Stmt::Keyframe { body, .. } = &reparsed[0] {
            if let Stmt::ActorDecl { props, .. } = &body[0] {
                let size_prop = props.iter().find(|p| p.name == "size").unwrap();
                assert_eq!(
                    size_prop.trailing_comment.as_deref(),
                    Some(" half-extents")
                );
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
        let result = crate::parser::parser_simple().parse(source);
        let (_, errors) = result.into_output_errors();
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
    fn parse_reorder_demo() {
        let source = REORDER_FIXTURE;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_reactive_binding() {
        let source = r#"orbiter.at := tracker.at + (200 * cos(3 * t), 200 * sin(3 * t))"#;
        let parsed = crate::parser::parser_simple().parse(source).unwrap();
        assert_eq!(parsed.len(), 1);
        if let Stmt::ReactiveBinding { target, property, .. } = &parsed[0] {
            assert_eq!(
                target,
                &[TargetSegment::Static("orbiter".to_string())]
            );
            assert_eq!(property, "at");
        } else {
            panic!("Expected ReactiveBinding");
        }
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser_simple().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }
}
