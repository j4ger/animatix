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
//! - The grammar is expression-heavy with prefix/infix operator precedence handled via combinator
//!   chaining in `chumsky`.
//! - Actor declarations, actions, and assignments share a generic modifier syntax in brackets
//!   `[...]`.
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
//! - The lossless tokenizer in [`crate::token`] is the single lexical definition.
//! - Parser tests in `tests/parser_tests.rs` are the authority on accepted syntax.

pub(crate) mod common;
pub(crate) mod expr;
pub(crate) mod inline;
pub(crate) mod stmt;
pub(crate) mod token_parser;
pub(crate) mod top_level;

use std::cell::RefCell;
use std::ops::Range;
use std::rc::Rc;

use chumsky::prelude::*;

use crate::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};

/// Parse source and return AST, parse errors, and diagnostics (warnings).
///
/// This is the full-diagnostics entry point that includes both syntax errors
/// and semantic warnings (e.g. silently dropped brace-style properties).
pub fn parse_source_diagnostics(
    source: &str,
) -> (Option<Vec<Stmt>>, Vec<ParseError>, Vec<Diagnostic>) {
    let warnings = Rc::new(RefCell::new(Vec::new()));
    let tokens = crate::token::tokenize(source);
    let spanned = token_parser::spanned(&tokens);
    let input = token_parser::as_input(&spanned);
    let (ast, errors) = parser(Rc::clone(&warnings)).parse(input).into_output_errors();
    let mut ast = ast;
    if let Some(statements) = ast.as_mut() {
        attach_trailing_comments(statements, source, &tokens);
    }
    let owned_errors: Vec<ParseError> =
        errors.iter().map(|e| ParseError::from_rich_tokens(source, e)).collect();
    let owned_warnings = match Rc::try_unwrap(warnings) {
        Ok(cell) => cell.into_inner(),
        Err(_) => Vec::new(),
    };
    (ast, owned_errors, owned_warnings)
}

/// Parse source and return AST, errors, and identifier occurrences.
pub fn parse_source_with_occurrences(
    source: &str,
) -> (Option<Vec<Stmt>>, Vec<ParseError>, Vec<crate::occurrence::Occurrence>) {
    let (ast, errors, _warnings) = parse_source_diagnostics(source);
    let occurrences = ast
        .as_deref()
        .map(|stmts| crate::occurrence::collect(stmts, source))
        .unwrap_or_default();
    (ast, errors, occurrences)
}

/// Parse source into an AST and structured parse errors.
///
/// The tokenizer handles whitespace and comments, so this entry point accepts
/// comments in all the same positions the lexer does. Test code may call
/// [`parser_simple`] directly when warnings are not needed.
pub fn parse_source(source: &str) -> (Option<Vec<Stmt>>, Vec<ParseError>) {
    let (ast, errors, _warnings) = parse_source_diagnostics(source);
    (ast, errors)
}

/// Parse source into an AST and structured parse errors without warnings.
///
/// This is a thin wrapper over [`parse_source`] kept for callers that only need
/// the AST plus errors.
pub fn parse_simple(source: &str) -> (Option<Vec<Stmt>>, Vec<ParseError>) {
    parse_source(source)
}

/// Attach `//` line comments to the properties they follow on the same line.
///
/// The tokenizer keeps comments in its lossless stream, but the parser filters
/// them out. This restores the previous parser's `Property::trailing_comment`
/// behavior from the token stream without reintroducing comments into the
/// grammar.
fn attach_trailing_comments(statements: &mut [Stmt], source: &str, tokens: &[crate::token::Token]) {
    let comments: Vec<(usize, String)> = tokens
        .iter()
        .filter_map(|t| match &t.kind {
            crate::token::TokenKind::Comment(text) => Some((t.span.start, text.clone())),
            _ => None,
        })
        .collect();
    if comments.is_empty() {
        return;
    }

    let mut props: Vec<&mut Property> = Vec::new();
    collect_property_spans(statements, &mut props);

    for (comment_start, text) in comments {
        let mut best: Option<usize> = None;
        let mut best_end = 0usize;
        for (i, prop) in props.iter().enumerate() {
            if let Some(span) = prop.value_span {
                if span.end <= comment_start
                    && !source[span.end..comment_start].contains('\n')
                    && span.end >= best_end
                {
                    best = Some(i);
                    best_end = span.end;
                }
            }
        }
        if let Some(i) = best {
            props[i].trailing_comment = Some(text);
        }
    }
}

/// Collect mutable references to every property that can carry a trailing
/// comment (those produced by [`common::property`], all of which set
/// `value_span`).
fn collect_property_spans<'a>(stmts: &'a mut [Stmt], out: &mut Vec<&'a mut Property>) {
    for stmt in stmts.iter_mut() {
        collect_stmt_property_spans(stmt, out);
    }
}

fn collect_stmt_property_spans<'a>(stmt: &'a mut Stmt, out: &mut Vec<&'a mut Property>) {
    match stmt {
        Stmt::ActorDecl {
            props, children, ..
        } => {
            out.extend(props.iter_mut());
            for child in children {
                collect_inline_property_spans(child, out);
            }
        },
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Always { body, .. } => collect_property_spans(body, out),
        Stmt::Stagger { body, .. } => collect_property_spans(body, out),
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            collect_property_spans(then_branch, out);
            if let Some(else_branch) = else_branch {
                collect_property_spans(else_branch, out);
            }
        },
        Stmt::Match { arms, .. } => {
            for (_, body) in arms {
                collect_property_spans(body, out);
            }
        },
        Stmt::ForLoop { body, .. } => collect_property_spans(body, out),
        Stmt::ComponentDef(def, _) => collect_property_spans(&mut def.body, out),
        Stmt::ComponentAction { body, .. } => collect_property_spans(body, out),
        Stmt::Config { settings, .. } => out.extend(settings.iter_mut()),
        Stmt::Scene { config, body, .. } => {
            out.extend(config.iter_mut());
            collect_property_spans(body, out);
        },
        _ => {},
    }
}

fn collect_inline_property_spans<'a>(item: &'a mut InlineItem, out: &mut Vec<&'a mut Property>) {
    match item {
        InlineItem::Anonymous {
            props, children, ..
        }
        | InlineItem::Labeled {
            props, children, ..
        } => {
            out.extend(props.iter_mut());
            for child in children {
                collect_inline_property_spans(child, out);
            }
        },
        InlineItem::ForLoop { body, .. } => {
            for child in body {
                collect_inline_property_spans(child, out);
            }
        },
        InlineItem::SlotFill { items, .. } => {
            for child in items {
                collect_inline_property_spans(child, out);
            }
        },
        InlineItem::SlotMarker => {},
    }
}

/// A parse result with AST, parse errors, and diagnostics.
///
/// This is the canonical result type for source parsing.
/// best-effort AST even when malformed nodes are present; chumsky returns no AST
/// when the source cannot be parsed. Callers should treat `statements` as
/// `Option` to cover both backends.
#[derive(Debug)]
pub struct ParseResult {
    /// Best-effort parsed statements, if any were produced.
    pub statements: Option<Vec<Stmt>>,
    /// Structured errors, converted from either backend into a common shape.
    pub parse_errors: Vec<ParseError>,
    /// Non-fatal parse diagnostics (currently produced by chumsky).
    pub warnings: Vec<Diagnostic>,
}

/// Parse source through the canonical semantic parse pipeline.
pub fn parse_canonical(source: &str) -> ParseResult {
    let (statements, parse_errors, warnings) = parse_source_diagnostics(source);
    ParseResult {
        statements,
        parse_errors,
        warnings,
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
        Diagnostic::error(DiagnosticCode::ParseError, DiagnosticPhase::Parse, msg).with_location(
            self.line,
            self.column,
            self.span.clone(),
        )
    }

    /// Convert a chumsky `Rich` token error into a structured `ParseError`.
    pub fn from_rich_tokens(
        source: &str,
        err: &Rich<'_, crate::token::TokenKind, ByteSpan>,
    ) -> Self {
        let span = err.span();
        let start = span.start;
        let end = span.end;
        let (line0, column0) = crate::token::byte_to_line_col(source, start);
        let line = line0 + 1;
        let column = column0 + 1;

        let mut _message = String::new();
        let mut expected = Vec::new();
        let mut found = None;

        match err.reason() {
            chumsky::error::RichReason::ExpectedFound {
                expected: exp,
                found: f,
            } => {
                expected = exp.iter().map(|p| p.to_string()).collect();
                found = f.as_ref().map(|c| c.to_string());
                let expected_str = expected.join(", ");
                match (expected_str.is_empty(), found.as_ref()) {
                    (false, Some(f)) => _message = format!("expected {expected_str}, found '{f}'"),
                    (false, None) => {
                        _message = format!("expected {expected_str}, found end of input")
                    },
                    (true, Some(f)) => _message = format!("unexpected '{f}'"),
                    (true, None) => _message = "unexpected end of input".to_string(),
                }
            },
            chumsky::error::RichReason::Custom(msg) => {
                _message = msg.clone();
            },
        }

        let context: Vec<String> = err.contexts().map(|(pattern, _)| pattern.to_string()).collect();

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

/// Build the top-level `.amx` file parser that discards warnings.
///
/// Convenience wrapper for tests and contexts where diagnostics are not needed.
pub fn parser_simple<'src>()
-> impl Parser<'src, token_parser::TokInput<'src>, Vec<Stmt>, token_parser::TokErr<'src>> {
    parser(Rc::new(RefCell::new(Vec::new())))
}

/// Build the top-level `.amx` file parser.
///
/// Parses a full source file into a `Vec<Stmt>`, grouping statements into scenes
/// via [`group_scenes`]. Accepts a shared warnings collector for emitting semantic
/// diagnostics during parsing.
pub fn parser<'src>(
    warnings: Rc<RefCell<Vec<Diagnostic>>>,
) -> impl Parser<'src, token_parser::TokInput<'src>, Vec<Stmt>, token_parser::TokErr<'src>> {
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

    #[test]
    fn test_closure_parser() {
        let input = "let f = (x) => x ^ 2";
        let res = parse_simple(input).0.unwrap();

        // Find the LetDecl stmt
        if let Stmt::LetDecl {
            is_pub,
            name,
            value,
            ..
        } = &res[0]
        {
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
        let res = parse_simple(input).0.unwrap();

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
        let res = parse_simple(input).0.unwrap();

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
        let res = parse_simple(input).0.unwrap();

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
        let res = parse_simple(input).0.unwrap();

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
        let res = parse_simple(input).0.unwrap();

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
        let res = parse_simple(input).0.unwrap();

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
        let res = parse_simple(input).0.unwrap();

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
        let res = parse_simple(input).0.unwrap();
        assert_eq!(res.len(), 1);
        // Reactive bindings are not wrapped in a default keyframe
        if let Stmt::ReactiveBinding {
            target,
            property,
            value,
            ..
        } = &res[0]
        {
            assert_eq!(target, &[TargetSegment::Static("orbiter".to_string())]);
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
        let (_, errors) = parse_source(input);
        assert!(!errors.is_empty(), "Expected parse error for single-segment reactive binding");
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
        if let Stmt::Assignment {
            property, value, ..
        } = &ast[0]
        {
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
        let has_context =
            errors.iter().any(|e| e.context.iter().any(|c| c.contains("actor declaration")));
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
        if let Stmt::ActorDecl {
            label, ty, props, ..
        } = &stmts[0]
        {
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
        let snippet =
            "${1:container}: ${2:Row} {\n    ${3:child}: ${4:Text}, content: \"${5:Hi}\"\n}";
        let stmts = parse_snippet(snippet);
        assert!(stmts.is_some(), "snippet should parse");
        let stmts = stmts.unwrap();
        assert_eq!(stmts.len(), 1);
        if let Stmt::ActorDecl {
            label,
            ty,
            children,
            ..
        } = &stmts[0]
        {
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
        assert!(
            matches!(&stmts[0], Stmt::Keyframe { .. }),
            "Expected Keyframe, got {:?}",
            stmts[0]
        );
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
            assert!(
                matches!(&body[0], Stmt::Stagger { .. }),
                "Expected Stagger inside keyframe, got {:?}",
                body[0]
            );
        } else {
            // Also accept top-level stagger
            assert!(
                matches!(&stmts[0], Stmt::Stagger { .. }),
                "Expected Stagger or Keyframe, got {:?}",
                stmts[0]
            );
        }
    }

    #[test]
    fn parse_action_with_dotted_target() {
        // Single dotted target: `highlight decomp_eq.f1 [800ms]`
        let result = parse_snippet("highlight decomp_eq.f1 [800ms]");
        assert!(result.is_some(), "dotted action target should parse");
        let stmts = result.unwrap();
        // May be wrapped in a default keyframe
        let action = stmts
            .iter()
            .flat_map(|s| match s {
                Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .find(|s| matches!(s, Stmt::Action(..)));
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
        let (stmts, errors) = parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = stmts.expect("expected parse output");
        if let Stmt::ComponentAction { params, .. } = &stmts[0] {
            assert_eq!(params[0].param_type, Some(TypeAnnotation::Any));
        } else {
            panic!("expected ComponentAction, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn test_union_type_annotation_in_action_param() {
        let src = r#"action transform(x: Bool | Str = "x") { fade-in x [300ms] }"#;
        let (stmts, errors) = parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = stmts.expect("expected parse output");
        if let Stmt::ComponentAction { params, .. } = &stmts[0] {
            assert_eq!(
                params[0].param_type,
                Some(TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str,]))
            );
        } else {
            panic!("expected ComponentAction, got {:?}", stmts[0]);
        }
    }

    #[test]
    fn test_type_alias_parses_union_annotation() {
        let src = "pub type LegendMode = Bool | Str\n";
        let (stmts, errors) = parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = stmts.expect("expected parse output");
        match &stmts[0] {
            Stmt::TypeAlias {
                is_pub,
                name,
                annotation,
                ..
            } => {
                assert!(is_pub);
                assert_eq!(name, "LegendMode");
                assert_eq!(
                    annotation,
                    &TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str])
                );
            },
            other => panic!("expected TypeAlias, got {:?}", other),
        }
    }

    #[test]
    fn test_namespaced_type_alias_reference_parses() {
        let src = "pub component Card(value: types::Metric) {}\n";
        let (stmts, errors) = parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = stmts.expect("expected parse output");
        if let Stmt::ComponentDef(def, _) = &stmts[0] {
            assert_eq!(
                def.params[0].param_type,
                Some(TypeAnnotation::Alias("types::Metric".to_string()))
            );
        } else {
            panic!("expected component definition");
        }
    }

    #[test]
    fn test_legacy_dotted_type_alias_normalizes() {
        let src = "pub component Card(value: types.Metric) {}\n";
        let (stmts, errors) = parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = stmts.expect("expected parse output");
        if let Stmt::ComponentDef(def, _) = &stmts[0] {
            assert_eq!(
                def.params[0].param_type,
                Some(TypeAnnotation::Alias("types::Metric".to_string()))
            );
        } else {
            panic!("expected component definition");
        }
    }

    #[test]
    fn test_rich_type_annotations_parse() {
        let src =
            "type P3 = Vec3\ntype Pair = Tuple<Str, Num>\ntype Mapper = Fn(Num, Num) => Num\n";
        let (stmts, errors) = parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = stmts.expect("expected parse output");
        match &stmts[0] {
            Stmt::TypeAlias {
                name, annotation, ..
            } => {
                assert_eq!(name, "P3");
                assert_eq!(annotation, &TypeAnnotation::Vec3);
            },
            other => panic!("expected Vec3 type alias, got: {:?}", other),
        }
        match &stmts[1] {
            Stmt::TypeAlias {
                name, annotation, ..
            } => {
                assert_eq!(name, "Pair");
                assert_eq!(
                    annotation,
                    &TypeAnnotation::Tuple(vec![TypeAnnotation::Str, TypeAnnotation::Num])
                );
            },
            other => panic!("expected tuple type alias, got: {:?}", other),
        }
        match &stmts[2] {
            Stmt::TypeAlias {
                name, annotation, ..
            } => {
                assert_eq!(name, "Mapper");
                assert_eq!(
                    annotation,
                    &TypeAnnotation::Function {
                        params: vec![TypeAnnotation::Num, TypeAnnotation::Num],
                        ret: Box::new(TypeAnnotation::Num),
                    }
                );
            },
            other => panic!("expected function type alias, got: {:?}", other),
        }
    }

    #[test]
    fn test_rich_type_annotations_in_component_params() {
        let src =
            "pub component App(p: Vec3, pair: Tuple<Str, Num>, mapper: Fn(Num, Num) => Num) {}\n";
        let (stmts, errors) = parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = stmts.expect("expected parse output");
        if let Stmt::ComponentDef(def, _) = &stmts[0] {
            assert_eq!(def.params[0].param_type, Some(TypeAnnotation::Vec3));
            assert_eq!(
                def.params[1].param_type,
                Some(TypeAnnotation::Tuple(vec![TypeAnnotation::Str, TypeAnnotation::Num]))
            );
            assert_eq!(
                def.params[2].param_type,
                Some(TypeAnnotation::Function {
                    params: vec![TypeAnnotation::Num, TypeAnnotation::Num],
                    ret: Box::new(TypeAnnotation::Num),
                })
            );
        } else {
            panic!("expected component definition");
        }
    }

    #[test]
    fn parenthesized_param_value_stays_default_expression() {
        // `(Num, Num)` is a tuple expression default, not a type annotation.
        let src = "component C(x: (Num, Num)) {}\n";
        let (stmts, errors) = parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = stmts.expect("expected parse output");
        if let Stmt::ComponentDef(def, _) = &stmts[0] {
            assert_eq!(def.params[0].param_type, None);
            assert_eq!(
                def.params[0].default,
                Some(Expr::Tuple(vec![
                    Expr::Ident("Num".to_string()),
                    Expr::Ident("Num".to_string())
                ]))
            );
        } else {
            panic!("expected component definition");
        }
    }

    #[test]
    fn parse_action_with_multiple_dotted_targets() {
        // Comma-separated dotted targets: `highlight eq.f1, eq.f2 [500ms]`
        let result = parse_snippet("highlight eq.f1, eq.f2 [500ms]");
        assert!(result.is_some(), "comma-separated dotted targets should parse");
        let stmts = result.unwrap();
        let action = stmts
            .iter()
            .flat_map(|s| match s {
                Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .find(|s| matches!(s, Stmt::Action(..)));
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
        let action = stmts
            .iter()
            .flat_map(|s| match s {
                Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .find(|s| matches!(s, Stmt::Action(..)));
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
        let action = stmts
            .iter()
            .flat_map(|s| match s {
                Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .find(|s| matches!(s, Stmt::Action(..)));
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
        let stmt = stmts
            .iter()
            .flat_map(|s| match s {
                Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .find(|s| matches!(s, Stmt::Assignment { .. }));
        assert!(stmt.is_some(), "expected Assignment stmt");
        if let Stmt::Assignment {
            target, property, ..
        } = stmt.unwrap()
        {
            assert_eq!(target, &vec![TargetSegment::Static("dots__0".to_string())]);
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
        let stmt = stmts
            .iter()
            .flat_map(|s| match s {
                Stmt::Keyframe { body, .. } => body.iter().collect::<Vec<_>>(),
                other => vec![other],
            })
            .find(|s| matches!(s, Stmt::Assignment { .. }));
        if let Stmt::Assignment {
            target, property, ..
        } = stmt.unwrap()
        {
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
            if let Stmt::Assignment {
                target, property, ..
            } = assignment
            {
                assert_eq!(target.len(), 1, "expected exactly one target segment");
                match &target[0] {
                    TargetSegment::Indexed { base, .. } => {
                        assert_eq!(base, "bars");
                    },
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
        if let Stmt::Assignment {
            target, property, ..
        } = stmt
        {
            assert_eq!(target.len(), 1, "expected exactly one target segment");
            match &target[0] {
                TargetSegment::Static(s) => {
                    assert_eq!(s, "bars__0", "static index should resolve to '__' notation");
                },
                other => panic!("expected Static segment, got {:?}", other),
            }
            assert_eq!(property, "opacity");
        } else {
            panic!("Expected Assignment");
        }
    }

    #[test]
    fn parse_logical_operators_with_comparison_precedence() {
        let src = "let x = t > 0 && t < 1 || flag";
        let (ast, errors) = crate::parser::parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = ast.expect("parsed AST");
        let Stmt::LetDecl { value, .. } = &stmts[0] else {
            panic!("expected let declaration");
        };
        let Expr::Binary(left_or, BinaryOp::Or, right) = value else {
            panic!("expected top-level ||, got: {:?}", value);
        };
        assert_eq!(right.as_ref(), &Expr::Ident("flag".to_string()));
        let Expr::Binary(left_and, BinaryOp::And, _) = left_or.as_ref() else {
            panic!("expected left side to be &&, got: {:?}", left_or);
        };
        assert!(matches!(left_and.as_ref(), Expr::Binary(_, BinaryOp::Gt, _)));
    }

    #[test]
    fn parse_power_binds_tighter_than_multiplication() {
        let src = "let x = a + b * c ^ d";
        let (ast, errors) = crate::parser::parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = ast.expect("parsed AST");
        let Stmt::LetDecl { value, .. } = &stmts[0] else {
            panic!("expected let declaration");
        };
        let Expr::Binary(_, BinaryOp::Add, right_add) = value else {
            panic!("expected top-level +, got: {:?}", value);
        };
        let Expr::Binary(_, BinaryOp::Mul, right_mul) = right_add.as_ref() else {
            panic!("expected multiplication below addition, got: {:?}", right_add);
        };
        assert!(
            matches!(right_mul.as_ref(), Expr::Binary(_, BinaryOp::Pow, _)),
            "expected power to bind tighter than multiplication, got: {:?}",
            right_mul
        );
    }

    #[test]
    fn parse_single_parenthesized_expression_is_not_tuple() {
        let src = "let x = (a + b)";
        let (ast, errors) = crate::parser::parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = ast.expect("parsed AST");
        let Stmt::LetDecl { value, .. } = &stmts[0] else {
            panic!("expected let declaration");
        };
        assert!(
            matches!(value, Expr::Binary(_, BinaryOp::Add, _)),
            "expected parenthesized binary expression, got: {:?}",
            value
        );
    }

    #[test]
    fn parse_inline_for_loop_index_variable() {
        let src = "#0s\nrow: Row {\n  for item, i in {1, 2, 3} {\n    box[i]: Rect, size: (10, item)\n  }\n}\n";
        let (ast, errors) = crate::parser::parse_source(src);
        assert!(errors.is_empty(), "parse errors: {:?}", errors);
        let stmts = ast.expect("parsed AST");
        let Stmt::Keyframe { body, .. } = &stmts[0] else {
            panic!("expected keyframe");
        };
        let Stmt::ActorDecl { children, .. } = &body[0] else {
            panic!("expected actor declaration");
        };
        match &children[0] {
            InlineItem::ForLoop { var, index_var, .. } => {
                assert_eq!(var, &LoopPattern::Single("item".to_string()));
                assert_eq!(index_var.as_deref(), Some("i"));
            },
            other => panic!("expected inline for loop, got: {:?}", other),
        }
    }
}
