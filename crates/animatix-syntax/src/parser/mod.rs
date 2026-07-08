//!
//! # Animatix Parser
//!
//! Combinator-based recursive descent parser using [`chumsky`]. This parser is the
//! executable source of truth for accepted `.amx` syntax.
//!
//! ## Entry Points
//!
//! - [`parser()`] — parses a full `.amx` file into `Vec<Stmt>`
//!
//! ## Key Design Notes
//!
//! - The grammar is expression-heavy with prefix/infix operator precedence handled via
//!   combinator chaining in `chumsky`.
//! - Actor declarations, actions, and assignments share a generic modifier syntax in
//!   brackets `[...]`.
//! - `Text`, `Math`, `Code` are parsed as generic actor declarations.
//! - The parser accepts some syntax that the runtime may reject (e.g., method/index/construct
//!   expressions) — honest runtime diagnostics handle the mismatch.
//!
//! ## Submodules
//!
//! | Module | Contents |
//! |--------|----------|
//! | [`common`] | Shared parser type aliases and factory helpers (`ident`, `property`, `modifier`, `time`, etc.) |
//! | [`expr`] | Expression parser with operator precedence, closures, and conditionals |
//! | [`inline`] | Inline item parser (`InlineItem`, `FlatItem`, slot markers/fills) |
//! | [`stmt`] | Recursive statement parser (declarations, assignments, actions, composition) |
//! | [`top_level`] | Top-level parser (`config`, scenes, keyframes, `group_scenes`) |
//!
//! ## Relationship to Other Systems
//!
//! - [`crate::ast`] defines the AST nodes this parser produces.
//! - `tree-sitter-animatix/` is a synchronized derivative for editor tooling.
//! - Parser tests in `tests/parser_tests.rs` are the authority on accepted syntax.

pub(crate) mod expr;
pub(crate) mod inline;
pub(crate) mod stmt;
pub(crate) mod top_level;
pub(crate) mod common;

use crate::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use chumsky::prelude::*;
use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

/// Replace `//` line comments with spaces while preserving newlines.
///
/// This allows comments inside `{}`, `[]`, and `()` delimiters where chumsky's
/// `.padded()` only skips whitespace. String literals are respected so `//`
/// inside a string is left untouched.
///
/// Comment text is replaced with spaces (not removed) so that byte spans in
/// parse errors remain valid for the original source.
pub fn strip_comments(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;

    while let Some(c) = chars.next() {
        if in_string {
            result.push(c);
            if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            result.push(c);
            in_string = true;
        } else if c == '/' && chars.peek() == Some(&'/') {
            // Line comment: replace with spaces until newline
            chars.next(); // consume second '/'
            result.push(' ');
            result.push(' ');
            while let Some(&next) = chars.peek() {
                if next == '\n' {
                    break;
                }
                chars.next();
                result.push(' ');
            }
        } else {
            result.push(c);
        }
    }

    result
}

/// Parse source after replacing line comments with spaces.
///
/// Use this for all production parsing so that `//` comments work everywhere,
/// including inside delimiters. Test code may call `parser_simple().parse()` directly.
/// Parse source and return AST, parse errors, and diagnostics (warnings).
///
/// This is the full-diagnostics entry point that includes both syntax errors
/// and semantic warnings (e.g. silently dropped brace-style properties).
pub fn parse_source_diagnostics(
    source: &str,
) -> (Option<Vec<Stmt>>, Vec<ParseError>, Vec<Diagnostic>) {
    let warnings = Rc::new(RefCell::new(Vec::new()));
    let stripped = strip_comments(source);
    let (ast, errors) = parser(Rc::clone(&warnings)).parse(&stripped).into_output_errors();
    let owned_errors: Vec<ParseError> =
        errors.iter().map(|e| ParseError::from_rich(source, e)).collect();
    let owned_warnings = match Rc::try_unwrap(warnings) {
        Ok(cell) => cell.into_inner(),
        Err(_) => Vec::new(),
    };
    (ast, owned_errors, owned_warnings)
}

/// Parse source after replacing line comments with spaces.
///
/// Use this for all production parsing so that `//` comments work everywhere,
/// including inside delimiters. Test code may call `parser_simple().parse()` directly.
pub fn parse_source(source: &str) -> (Option<Vec<Stmt>>, Vec<ParseError>) {
    let (ast, errors, _warnings) = parse_source_diagnostics(source);
    (ast, errors)
}

/// Parse source using tree-sitter and return the same format as [`parse_source`].
///
/// This is the incremental parsing entry point. Tree-sitter re-parses only the
/// changed region of the file, making this significantly faster for large files
/// with small edits. Falls back to the chumsky parser if tree-sitter fails.
pub fn parse_source_ts(source: &str) -> (Option<Vec<Stmt>>, Vec<ParseError>) {
    match crate::ts_convert::parse_source(source) {
        Some(result) => {
            let errors: Vec<ParseError> = result.diagnostics.iter().map(|d| {
                let span = d.location.span.clone().unwrap_or(0..0);
                ParseError {
                    message: d.message.clone(),
                    span,
                    line: d.location.line.unwrap_or(1),
                    column: d.location.column.unwrap_or(1),
                    expected: Vec::new(),
                    found: None,
                    context: Vec::new(),
                }
            }).collect();
            (Some(result.statements), errors)
        }
        None => {
            // Tree-sitter failed — fall back to chumsky
            parse_source(source)
        }
    }
}

/// Strip VS Code–style tab-stop placeholders from snippet text.
///
/// Placeholders have the form `${N:default}` or `${N}` (where N is a
/// non-negative integer).  The function returns the text with every
/// placeholder replaced by its default value (or removed if empty).
///
/// This is used by the GUI insertion palette so that snippet templates
/// can be parsed as valid `.amx` source.
pub fn strip_snippet_tabstops(snippet: &str) -> String {
    let mut result = String::with_capacity(snippet.len());
    let mut chars = snippet.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '$' && chars.peek() == Some(&'{') {
            chars.next(); // consume '{'
            // Skip the tab-stop number
            while chars.peek().is_some_and(|ch| ch.is_ascii_digit()) {
                chars.next();
            }
            // If there's a ':', collect the default text
            if chars.peek() == Some(&':') {
                chars.next(); // consume ':'
                let mut depth = 1i32;
                while let Some(&ch) = chars.peek() {
                    if ch == '{' {
                        depth += 1;
                    } else if ch == '}' {
                        depth -= 1;
                        if depth == 0 {
                            chars.next(); // consume '}'
                            break;
                        }
                    }
                    result.push(ch);
                    chars.next();
                }
            } else {
                // No default — just skip to closing '}'
                while chars.next_if(|&ch| ch != '}').is_some() {}
                chars.next(); // consume '}'
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Parse a snippet template into a list of statements.
///
/// The snippet text may contain VS Code–style tab-stop placeholders
/// (`${1:label}`, `${2:}`, etc.). These are stripped before parsing.
///
/// Returns `Some(stmts)` if parsing succeeds, `None` otherwise.
pub fn parse_snippet(snippet: &str) -> Option<Vec<Stmt>> {
    let cleaned = strip_snippet_tabstops(snippet);
    let (ast, _errors) = parse_source(&cleaned);
    ast
}

/// A structured parse error with human-readable location and context.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// Human-readable error message.
    pub message: String,
    /// Byte span in the source where the error occurred.
    pub span: Range<usize>,
    /// 1-based line number of the error.
    pub line: usize,
    /// 1-based column number of the error.
    pub column: usize,
    /// Descriptions of what the parser expected at this point.
    pub expected: Vec<String>,
    /// The token or text the parser actually found, if any.
    pub found: Option<String>,
    /// Parser context stack (labels of enclosing grammar rules).
    pub context: Vec<String>,
}

impl ParseError {
    /// Convert this parse error into a `Diagnostic` for use with the unified
    /// [`format_diagnostic`](crate::diagnostics::format_diagnostic) formatter.
    ///
    /// The resulting diagnostic has:
    /// * `code`: [`DiagnosticCode::ParseError`]
    /// * `phase`: [`DiagnosticPhase::Parse`]
    /// * `severity`: [`DiagnosticSeverity::Error`]
    /// * `message`: the parse error message plus parser context
    /// * `location`: the line, column, and byte span of the error
    pub fn to_diagnostic(&self) -> Diagnostic {
        let mut msg = self.message.clone();
        if !self.context.is_empty() {
            msg.push_str(&format!("\n  in {}", self.context.join(" > ")));
        }
        Diagnostic::error(DiagnosticCode::ParseError, DiagnosticPhase::Parse, msg)
            .with_location(self.line, self.column, self.span.clone())
    }

    /// Convert a chumsky `Rich` error into a structured `ParseError`.
    pub fn from_rich(source: &str, err: &Rich<'_, char>) -> Self {
        let span = err.span();
        let start = span.start;
        let end = span.end;
        let (line, column) = byte_offset_to_line_col(source, start);

        let mut _message = String::new();
        let mut expected = Vec::new();
        let mut found = None;

        match err.reason() {
            chumsky::error::RichReason::ExpectedFound { expected: exp, found: f } => {
                expected = exp.iter().map(|p| p.to_string()).collect();
                found = f.as_ref().map(|c| c.to_string());
                let expected_str = expected.join(", ");
                match (expected_str.is_empty(), found.as_ref()) {
                    (false, Some(f)) => _message = format!("expected {expected_str}, found '{f}'"),
                    (false, None) => {
                        _message = format!("expected {expected_str}, found end of input")
                    }
                    (true, Some(f)) => _message = format!("unexpected '{f}'"),
                    (true, None) => _message = "unexpected end of input".to_string(),
                }
            }
            chumsky::error::RichReason::Custom(msg) => {
                _message = msg.clone();
            }
        }

        let context: Vec<String> = err
            .contexts()
            .map(|(pattern, _)| pattern.to_string())
            .collect();

        Self {
            message: _message,
            span: start..end,
            line,
            column,
            expected,
            found,
            context,
        }
    }
}

/// Convert a byte offset into a 1-based (line, column) pair.
fn byte_offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1usize;
    let mut col = 1usize;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Build the top-level `.amx` file parser that discards warnings.
///
/// Convenience wrapper for tests and contexts where diagnostics are not needed.
pub fn parser_simple<'src>(
) -> impl Parser<'src, &'src str, Vec<Stmt>, extra::Err<Rich<'src, char>>> {
    parser(Rc::new(RefCell::new(Vec::new())))
}

/// Build the top-level `.amx` file parser.
///
/// Parses a full source file into a `Vec<Stmt>`, grouping statements into scenes
/// via [`group_scenes`]. Accepts a shared warnings collector for emitting semantic
/// diagnostics during parsing.
pub fn parser<'src>(
    warnings: Rc<RefCell<Vec<Diagnostic>>>,
) -> impl Parser<'src, &'src str, Vec<Stmt>, extra::Err<Rich<'src, char>>> {
    let expr = expr::parser();
    let property = common::property(expr.clone());
    let modifier = common::modifier(expr.clone(), common::time());
    let modifiers = common::modifiers(modifier);
    let inline_items = inline::parser(expr.clone(), property.clone(), modifiers.clone(), warnings);
    let stmt = stmt::parser(expr.clone(), property.clone(), modifiers.clone(), inline_items);
    top_level::parser(stmt, property, modifiers)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chumsky::Parser;

    #[test]
    fn test_closure_parser() {
        let input = "let f = (x) => x ^ 2";
        let res = parser_simple().parse(input).unwrap();

        // Find the LetDecl stmt
        if let Stmt::LetDecl { is_pub, name, value, .. } = &res[0] {
            assert!(!(*is_pub));
            assert_eq!(name, "f");
            assert_eq!(
                *value,
                Expr::Closure(
                    vec!["x".to_string()],
                    Box::new(Expr::Binary(
                        Box::new(Expr::Ident("x".to_string())),
                        BinaryOp::Pow,
                        Box::new(Expr::Num(2.0))
                    ))
                )
            );
        } else {
            panic!("Expected LetDecl");
        }
    }

    #[test]
    fn test_text_shorthand_parser() {
        let input = r#"a: "hello world""#;
        let res = parser_simple().parse(input).unwrap();

        if let Stmt::ActorDecl {
            is_pub,
            label,
            ty,
            props,
            modifiers,
            children,
            ..
        } = &res[0]
        {
            assert!(!(*is_pub));
            assert_eq!(label, "a");
            assert_eq!(ty, "Text");
            assert_eq!(props.len(), 1);
            assert_eq!(props[0].name, "text");
            assert_eq!(props[0].value, Expr::Str("hello world".to_string()));
            assert!(modifiers.is_empty());
            assert!(children.is_empty());
        } else {
            panic!("Expected ActorDecl, got {:?}", res[0]);
        }
    }

    #[test]
    fn test_text_shorthand_with_modifiers() {
        let input = r#"title: "Slide 1" [2s, ease: ease-in-out]"#;
        let res = parser_simple().parse(input).unwrap();

        if let Stmt::ActorDecl {
            label,
            ty,
            props,
            modifiers,
            ..
        } = &res[0]
        {
            assert_eq!(label, "title");
            assert_eq!(ty, "Text");
            assert_eq!(props.len(), 1);
            assert_eq!(props[0].name, "text");
            assert_eq!(props[0].value, Expr::Str("Slide 1".to_string()));
            assert_eq!(modifiers.len(), 2);
        } else {
            panic!("Expected ActorDecl, got {:?}", res[0]);
        }
    }

    #[test]
    fn test_typst_shorthand_parser() {
        let input = "eq: $$ x^2 + y^2 $$";
        let res = parser_simple().parse(input).unwrap();

        if let Stmt::ActorDecl {
            is_pub,
            label,
            ty,
            props,
            modifiers,
            children,
            ..
        } = &res[0]
        {
            assert!(!(*is_pub));
            assert_eq!(label, "eq");
            assert_eq!(ty, "Typst");
            assert_eq!(props.len(), 1);
            assert_eq!(props[0].name, "content");
            assert_eq!(props[0].value, Expr::Str("x^2 + y^2".to_string()));
            assert!(modifiers.is_empty());
            assert!(children.is_empty());
        } else {
            panic!("Expected ActorDecl, got {:?}", res[0]);
        }
    }

    #[test]
    fn test_typst_shorthand_with_modifiers() {
        let input = "eq: $$ x^2 $$ [2s]";
        let res = parser_simple().parse(input).unwrap();

        if let Stmt::ActorDecl {
            label,
            ty,
            props,
            modifiers,
            ..
        } = &res[0]
        {
            assert_eq!(label, "eq");
            assert_eq!(ty, "Typst");
            assert_eq!(props.len(), 1);
            assert_eq!(props[0].name, "content");
            assert_eq!(props[0].value, Expr::Str("x^2".to_string()));
            assert_eq!(modifiers.len(), 1);
        } else {
            panic!("Expected ActorDecl, got {:?}", res[0]);
        }
    }

    #[test]
    fn test_vec2_value_span_accuracy() {
        // Reproduce the bug: size: (2494.552, 1377.7778) should have correct span
        let input = r#"backdrop: Rect, size: (2494.552, 1377.7778), color: scene.background"#;
        let res = parser_simple().parse(input).unwrap();

        if let Stmt::ActorDecl { props, .. } = &res[0] {
            let size_prop = props.iter().find(|p| p.name == "size").unwrap();
            let span = size_prop.value_span.unwrap();

            // The value in source is "(2494.552, 1377.7778)"
            // Find its actual position in the input
            let value_start = input.find("(2494.552").unwrap();
            let value_end = input.find("1377.7778)").unwrap() + "1377.7778)".len();

            assert_eq!(span.start, value_start, "span start mismatch");
            assert_eq!(span.end, value_end, "span end mismatch");

            // Verify the span extracts the correct text
            let extracted = &input[span.start..span.end];
            assert_eq!(extracted, "(2494.552, 1377.7778)", "span extracts wrong text");
        } else {
            panic!("Expected ActorDecl");
        }
    }

    #[test]
    fn test_vec2_value_span_with_trailing_comma() {
        // Test with trailing comma in tuple: (2494.552, 1377.7778,)
        let input = r#"backdrop: Rect, size: (2494.552, 1377.7778,), color: scene.background"#;
        let res = parser_simple().parse(input).unwrap();

        if let Stmt::ActorDecl { props, .. } = &res[0] {
            let size_prop = props.iter().find(|p| p.name == "size").unwrap();
            let span = size_prop.value_span.unwrap();

            // The value in source is "(2494.552, 1377.7778,)"
            let value_start = input.find("(2494.552").unwrap();
            let value_end = input.find("1377.7778,)").unwrap() + "1377.7778,)".len();

            assert_eq!(span.start, value_start, "span start mismatch");
            assert_eq!(span.end, value_end, "span end mismatch");

            let extracted = &input[span.start..span.end];
            assert_eq!(extracted, "(2494.552, 1377.7778,)", "span extracts wrong text");
        } else {
            panic!("Expected ActorDecl");
        }
    }

    #[test]
    fn test_multiple_properties_span_independence() {
        // Test that spans for multiple properties don't overlap
        let input = r#"backdrop: Rect, size: (100, 200), color: red, anchor: center"#;
        let res = parser_simple().parse(input).unwrap();

        if let Stmt::ActorDecl { props, .. } = &res[0] {
            let size_prop = props.iter().find(|p| p.name == "size").unwrap();
            let color_prop = props.iter().find(|p| p.name == "color").unwrap();
            let anchor_prop = props.iter().find(|p| p.name == "anchor").unwrap();

            let size_span = size_prop.value_span.unwrap();
            let color_span = color_prop.value_span.unwrap();
            let anchor_span = anchor_prop.value_span.unwrap();

            // Verify spans don't overlap
            assert!(size_span.end <= color_span.start, "size span overlaps color span");
            assert!(color_span.end <= anchor_span.start, "color span overlaps anchor span");

            // Verify extracted text
            let size_text = &input[size_span.start..size_span.end];
            let color_text = &input[color_span.start..color_span.end];
            let anchor_text = &input[anchor_span.start..anchor_span.end];

            assert_eq!(size_text, "(100, 200)");
            assert_eq!(color_text, "red");
            assert_eq!(anchor_text, "center");
        } else {
            panic!("Expected ActorDecl");
        }
    }


    #[test]
    fn test_reactive_binding_parser() {
        let input = r#"orbiter.at := tracker.at + (200 * cos(3 * t), 200 * sin(3 * t))"#;
        let res = parser_simple().parse(input).unwrap();
        assert_eq!(res.len(), 1);
        // Reactive bindings are not wrapped in a default keyframe
        if let Stmt::ReactiveBinding { target, property, value, .. } = &res[0] {
            assert_eq!(
                target,
                &[TargetSegment::Static("orbiter".to_string())]
            );
            assert_eq!(property, "at");
            // Verify it's a binary expression (tracker.at + (...))
            if let Expr::Binary(left, BinaryOp::Add, _right) = value {
                if let Expr::Path(parts) = left.as_ref() {
                    assert_eq!(parts, &["tracker", "at"]);
                } else {
                    panic!("Expected Path for left side");
                }
            } else {
                panic!("Expected Binary Add expression");
            }
        } else {
            panic!("Expected ReactiveBinding");
        }
    }

    #[test]
    fn test_reactive_binding_rejects_single_segment() {
        let input = r#"at := (100, 200)"#;
        let res = parser_simple().parse(input);
        assert!(res.has_errors(), "Expected parse error for single-segment reactive binding");
    }

    #[test]
    fn test_comments_inside_delimiters() {
        let input = r#"
            circ = Circle {
                at: (0, 0), // center
                radius: 100, // size
            }
        "#;
        let (ast, errors) = parse_source(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);
        let ast = ast.expect("parsed AST");
        assert_eq!(ast.len(), 1);
        if let Stmt::Assignment { property, value, .. } = &ast[0] {
            assert_eq!(property, "circ");
            if let Expr::Construct(name, props) = value {
                assert_eq!(name, "Circle");
                assert_eq!(props.len(), 2);
            } else {
                panic!("Expected Construct value");
            }
        } else {
            panic!("Expected Assignment, got {:?}", ast[0]);
        }
    }

    #[test]
    fn test_comment_strip_preserves_strings() {
        let input = r#"
            label = Text {
                text: "Visit https://example.com // not a comment",
            }
        "#;
        let (ast, errors) = parse_source(input);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);
        let ast = ast.expect("parsed AST");
        if let Stmt::Assignment { value, .. } = &ast[0] {
            if let Expr::Construct(_, props) = value {
                if let Expr::Str(text) = &props[0].value {
                    assert_eq!(text, "Visit https://example.com // not a comment");
                } else {
                    panic!("Expected string value");
                }
            } else {
                panic!("Expected Construct");
            }
        } else {
            panic!("Expected Assignment");
        }
    }

    #[test]
    fn test_parse_error_includes_context() {
        // Trigger a parse error inside an actor declaration to verify context
        let input = r#"
            circ: Circle {
                at: (0, 0),
                radius: // missing value
            }
        "#;
        let (_ast, errors) = parse_source(input);
        assert!(!errors.is_empty(), "Expected parse errors");
        // The error should include context about being in an actor declaration
        let has_context = errors.iter().any(|e| {
            e.context.iter().any(|c| c.contains("actor declaration"))
        });
        assert!(
            has_context,
            "Expected error context to include 'actor declaration', got: {:?}",
            errors
        );
    }

    // ── Snippet tab-stop tests ──

    #[test]
    fn strip_tabstops_simple() {
        let input = "${1:label}: ${2:Text} {}";
        let result = strip_snippet_tabstops(input);
        assert_eq!(result, "label: Text {}");
    }

    #[test]
    fn strip_tabstops_nested_braces() {
        let input = "${1:label}: ${2:Text} {\n    ${3:content}: \"${4:}\",\n}";
        let result = strip_snippet_tabstops(input);
        assert_eq!(result, "label: Text {\n    content: \"\",\n}");
    }

    #[test]
    fn strip_tabstops_no_default() {
        let input = "# ${1:0s}\n${2}";
        let result = strip_snippet_tabstops(input);
        assert_eq!(result, "# 0s\n");
    }

    #[test]
    fn strip_tabstops_preserves_dollar_signs() {
        let input = "price: \"$100\"";
        let result = strip_snippet_tabstops(input);
        assert_eq!(result, "price: \"$100\"");
    }

    #[test]
    fn parse_snippet_actor_inline_props() {
        // AMX uses comma-separated inline properties, not braces
        let snippet = "${1:label}: ${2:Text}, ${3:content}: \"${4:Hello}\"";
        let stmts = parse_snippet(snippet);
        assert!(stmts.is_some(), "snippet should parse");
        let stmts = stmts.unwrap();
        assert_eq!(stmts.len(), 1);
        if let Stmt::ActorDecl { label, ty, props, .. } = &stmts[0] {
            assert_eq!(label, "label");
            assert_eq!(ty, "Text");
            assert_eq!(props.len(), 1);
            assert_eq!(props[0].name, "content");
        } else {
            panic!("Expected ActorDecl, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn parse_snippet_actor_with_children() {
        // AMX children (nested items) use braces
        let snippet = "${1:container}: ${2:Row} {\n    ${3:child}: ${4:Text}, content: \"${5:Hi}\"\n}";
        let stmts = parse_snippet(snippet);
        assert!(stmts.is_some(), "snippet should parse");
        let stmts = stmts.unwrap();
        assert_eq!(stmts.len(), 1);
        if let Stmt::ActorDecl { label, ty, children, .. } = &stmts[0] {
            assert_eq!(label, "container");
            assert_eq!(ty, "Row");
            assert_eq!(children.len(), 1);
        } else {
            panic!("Expected ActorDecl with children, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn parse_snippet_keyframe() {
        let snippet = "# ${1:0s}\n${2}";
        let stmts = parse_snippet(snippet);
        assert!(stmts.is_some(), "keyframe snippet should parse");
        let stmts = stmts.unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(&stmts[0], Stmt::Keyframe { .. }), "Expected Keyframe, got {:?}", stmts[0]);
    }

    #[test]
    fn parse_snippet_always() {
        let snippet = "always {\n    ${1:}\n}";
        let stmts = parse_snippet(snippet);
        assert!(stmts.is_some(), "always snippet should parse");
        let stmts = stmts.unwrap();
        assert_eq!(stmts.len(), 1);
        assert!(matches!(&stmts[0], Stmt::Always { .. }), "Expected Always, got {:?}", stmts[0]);
    }

    #[test]
    fn parse_snippet_stagger() {
        let snippet = "stagger [${1:150ms}] {\n    ${2:}\n}";
        let stmts = parse_snippet(snippet);
        assert!(stmts.is_some(), "stagger snippet should parse");
        let stmts = stmts.unwrap();
        // Parser wraps bare statements in a default keyframe (#0s)
        assert_eq!(stmts.len(), 1);
        if let Stmt::Keyframe { body, .. } = &stmts[0] {
            assert_eq!(body.len(), 1);
            assert!(matches!(&body[0], Stmt::Stagger { .. }), "Expected Stagger inside keyframe, got {:?}", body[0]);
        } else {
            // Also accept top-level stagger
            assert!(matches!(&stmts[0], Stmt::Stagger { .. }), "Expected Stagger or Keyframe, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn parse_action_with_dotted_target() {
        // Single dotted target: `highlight decomp_eq.f1 [800ms]`
        let result = parse_snippet("highlight decomp_eq.f1 [800ms]");
        assert!(result.is_some(), "dotted action target should parse");
        let stmts = result.unwrap();
        // May be wrapped in a default keyframe
        let action = stmts.iter().flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        }).find(|s| matches!(s, Stmt::Action(..)));
        assert!(action.is_some(), "expected Action stmt, got {:?}", stmts);
        if let Stmt::Action(action, _) = action.unwrap() {
            assert_eq!(action.verb, "highlight");
            assert_eq!(action.targets, vec!["decomp_eq.f1"]);
        }
    }

    #[test]
    fn test_method_call_with_args() {
        // Parse an always block with a method call: graph.map(mx, my)
        let input = "always { ball.at = descent_graph.map(mx, my) }";
        let (stmts, errs) = parse_source(input);
        assert!(errs.is_empty(), "Method call parse failed: {:?}", errs);
        let stmts = stmts.unwrap();
        assert_eq!(stmts.len(), 1);
        if let Stmt::Always { body, .. } = &stmts[0] {
            // Body should have one assignment statement
            if let Stmt::Assignment { value, .. } = &body[0] {
                // The value should be a Method call: graph.map(mx, my)
                assert!(
                    matches!(
                        &value,
                        Expr::Method(receiver, name, args)
                            if matches!(receiver.as_ref(), Expr::Ident(n) if n == "descent_graph")
                            && name == "map"
                            && args.len() == 2
                    ),
                    "Expected Method(Ident(descent_graph), 'map', [mx, my]), got {:?}",
                    value
                );
            } else {
                panic!("Expected Assignment, got {:?}", body[0]);
            }
        } else {
            panic!("Expected Always, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn test_method_call_on_dot_path() {
        // Parse chained method: a.b.c(d)
        let input = "always { x = a.b.c(d) }";
        let (stmts, errs) = parse_source(input);
        assert!(errs.is_empty(), "Chained method parse failed: {:?}", errs);
        let stmts = stmts.unwrap();
        assert_eq!(stmts.len(), 1);
        if let Stmt::Always { body, .. } = &stmts[0] {
            if let Stmt::Assignment { value, .. } = &body[0] {
                assert!(
                    matches!(
                        &value,
                        Expr::Method(receiver, name, args)
                            if matches!(receiver.as_ref(), Expr::Path(parts) if parts == &["a", "b"])
                            && name == "c"
                            && args.len() == 1
                    ),
                    "Expected Method(Path([a,b]), 'c', [d]), got {:?}",
                    value
                );
            } else {
                panic!("Expected Assignment, got {:?}", body[0]);
            }
        } else {
            panic!("Expected Always, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn test_field_access_still_works() {
        // Ensure bare field access (no parens) still produces Path
        let input = "always { x = descent_graph.at }";
        let (stmts, errs) = parse_source(input);
        assert!(errs.is_empty(), "Field access parse failed: {:?}", errs);
        let stmts = stmts.unwrap();
        if let Stmt::Always { body, .. } = &stmts[0] {
            if let Stmt::Assignment { value, .. } = &body[0] {
                assert!(
                    matches!(&value, Expr::Path(parts) if parts == &["descent_graph", "at"]),
                    "Expected Path([descent_graph, at]), got {:?}",
                    value
                );
            }
        }
    }

    #[test]
    fn test_any_type_annotation_in_action_param() {
        // `Any` in a parameter type annotation should parse without error.
        let src = r#"action transform(x: Any = 0) { fade-in x [300ms] }"#;
        let res = parser_simple().parse(src);
        assert!(!res.has_errors(), "parse errors: {:?}", res.errors().collect::<Vec<_>>());
        let stmts = res.output().expect("expected parse output");
        if let Stmt::ComponentAction { params, .. } = &stmts[0] {
            assert_eq!(params[0].param_type, Some(TypeAnnotation::Any));
        } else {
            panic!("expected ComponentAction, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn parse_action_with_multiple_dotted_targets() {
        // Comma-separated dotted targets: `highlight eq.f1, eq.f2 [500ms]`
        let result = parse_snippet("highlight eq.f1, eq.f2 [500ms]");
        assert!(result.is_some(), "comma-separated dotted targets should parse");
        let stmts = result.unwrap();
        let action = stmts.iter().flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        }).find(|s| matches!(s, Stmt::Action(..)));
        assert!(action.is_some(), "expected Action stmt");
        if let Stmt::Action(action, _) = action.unwrap() {
            assert_eq!(action.verb, "highlight");
            assert_eq!(action.targets, vec!["eq.f1", "eq.f2"]);
        }
    }

    // -----------------------------------------------------------------------
    // Gap 1a: subscript in value expressions
    // -----------------------------------------------------------------------

    #[test]
    fn parse_subscript_expr_integer() {
        // `items[0]` in value position → Expr::Index
        let src = "always { x = items[0] }";
        let (stmts, errs) = parse_source(src);
        assert!(errs.is_empty(), "unexpected parse errors: {:?}", errs);
        let stmts = stmts.unwrap();
        if let Stmt::Always { body, .. } = &stmts[0] {
            if let Stmt::Assignment { value, .. } = &body[0] {
                assert!(
                    matches!(value, Expr::Index(base, idx)
                        if matches!(base.as_ref(), Expr::Ident(n) if n == "items")
                        && matches!(idx.as_ref(), Expr::Num(n) if *n == 0.0)
                    ),
                    "expected Index(Ident(items), Num(0)), got {:?}",
                    value
                );
            } else {
                panic!("expected Assignment, got {:?}", body[0]);
            }
        } else {
            panic!("expected Always, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn parse_subscript_does_not_consume_modifier_bracket() {
        // `fade-in x [300ms]` — the `[300ms]` must be a modifier, not a subscript.
        let src = "fade-in x [300ms]";
        let result = parse_snippet(src);
        assert!(result.is_some(), "should parse without error");
        let stmts = result.unwrap();
        let action = stmts.iter().flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        }).find(|s| matches!(s, Stmt::Action(..)));
        assert!(action.is_some(), "expected Action stmt");
        if let Stmt::Action(a, _) = action.unwrap() {
            assert_eq!(a.targets, vec!["x"], "target should be 'x', not 'x[300ms]'");
            assert!(!a.modifiers.is_empty(), "modifier list should not be empty");
        }
    }

    // -----------------------------------------------------------------------
    // Gap 1b: array-indexed targets
    // -----------------------------------------------------------------------

    #[test]
    fn parse_indexed_action_target() {
        // `fade-in dots[0] [300ms]` → target resolves to "dots__0"
        let src = "fade-in dots[0] [300ms]";
        let result = parse_snippet(src);
        assert!(result.is_some(), "indexed action target should parse");
        let stmts = result.unwrap();
        let action = stmts.iter().flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        }).find(|s| matches!(s, Stmt::Action(..)));
        assert!(action.is_some(), "expected Action stmt");
        if let Stmt::Action(a, _) = action.unwrap() {
            assert_eq!(a.targets, vec!["dots__0"]);
        }
    }

    #[test]
    fn parse_indexed_assignment_target() {
        // `dots[0].opacity = 1` → target ["dots__0"], property "opacity"
        let src = "dots[0].opacity = 1";
        let result = parse_snippet(src);
        assert!(result.is_some(), "indexed assignment target should parse");
        let stmts = result.unwrap();
        let stmt = stmts.iter().flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        }).find(|s| matches!(s, Stmt::Assignment { .. }));
        assert!(stmt.is_some(), "expected Assignment stmt");
        if let Stmt::Assignment { target, property, .. } = stmt.unwrap() {
            assert_eq!(
                target,
                &vec![TargetSegment::Static("dots__0".to_string())]
            );
            assert_eq!(property, "opacity");
        }
    }

    #[test]
    fn parse_indexed_dotted_assignment_target() {
        // `dots[1].at.x = 5` → target ["dots__1", "at"], property "x"
        let src = "dots[1].at.x = 5";
        let result = parse_snippet(src);
        assert!(result.is_some(), "dotted indexed assignment should parse");
        let stmts = result.unwrap();
        let stmt = stmts.iter().flat_map(|s| match s {
            Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
            other => vec![other],
        }).find(|s| matches!(s, Stmt::Assignment { .. }));
        if let Stmt::Assignment { target, property, .. } = stmt.unwrap() {
            assert_eq!(
                target,
                &vec![
                    TargetSegment::Static("dots__1".to_string()),
                    TargetSegment::Static("at".to_string()),
                ]
            );
            assert_eq!(property, "x");
        }
    }

    #[test]
    fn parse_indexed_runtime_target() {
        // `bars[i].color = red` inside an `always` block should produce
        // TargetSegment::Indexed for the runtime index.
        let src = "always { bars[i].color = red }";
        let result = crate::parser::parse_source(src).0.unwrap();
        let always = result.iter().find(|s| matches!(s, Stmt::Always { .. })).unwrap();
        if let Stmt::Always { body, .. } = always {
            let assignment = body.iter().find(|s| matches!(s, Stmt::Assignment { .. })).unwrap();
            if let Stmt::Assignment { target, property, .. } = assignment {
                assert_eq!(target.len(), 1, "expected exactly one target segment");
                match &target[0] {
                    TargetSegment::Indexed { base, .. } => {
                        assert_eq!(base, "bars");
                    }
                    other => panic!("expected Indexed segment, got {:?}", other),
                }
                assert_eq!(property, "color");
            } else {
                panic!("Expected Assignment");
            }
        } else {
            panic!("Expected Always");
        }
    }

    #[test]
    fn parse_static_indexed_target() {
        // `bars[0].opacity = 0.5` should produce TargetSegment::Static("bars__0")
        let src = "bars[0].opacity = 0.5";
        let result = crate::parser::parse_source(src).0.unwrap();
        let stmt = result.iter().find(|s| matches!(s, Stmt::Assignment { .. })).unwrap();
        if let Stmt::Assignment { target, property, .. } = stmt {
            assert_eq!(target.len(), 1, "expected exactly one target segment");
            match &target[0] {
                TargetSegment::Static(s) => {
                    assert_eq!(s, "bars__0", "static index should resolve to '__' notation");
                }
                other => panic!("expected Static segment, got {:?}", other),
            }
            assert_eq!(property, "opacity");
        } else {
            panic!("Expected Assignment");
        }
    }
}