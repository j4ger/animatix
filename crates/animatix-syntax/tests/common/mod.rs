//! Helpers shared by the crate's integration tests.
//!
//! Each integration-test binary includes the whole module but exercises only a
//! subset, so the unused-item lint is silenced here rather than per test file.
#![allow(dead_code)]

use animatix_syntax::ast::Stmt;

/// Parse `source`, requiring a clean parse, and return its statements.
pub fn parse(source: &str) -> Vec<Stmt> {
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(
        errors.is_empty(),
        "parse errors {:?} for:\n{source}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    stmts.expect("no statements")
}

/// The `//` comment texts in `source`, in source order.
pub fn comment_texts(source: &str) -> Vec<String> {
    animatix_syntax::token::tokenize(source)
        .into_iter()
        .filter_map(|token| match token.kind {
            animatix_syntax::token::TokenKind::Comment(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// Documents covering every comment class the source-aware formatter must
/// preserve.
pub const COMMENT_FIXTURES: &[&str] = &[
    // File header + leading/free-standing comments between top-level statements.
    "// file header one\n// file header two\n\nlet a = 1\n\n// about b\nlet b = 2\n",
    // Free-standing and trailing comments inside a keyframe block.
    "#0s\n// leading in keyframe\ntitle.opacity = 0 // trailing assignment\n// tail in keyframe\n",
    // Trailing comment after a property.
    "title: Text, text: \"Hello\" // trailing property\n",
    // Trailing comments after non-property statements.
    "let a = 1 // trailing let\n#1s\nfade-in a [1s] // trailing action\n",
    // Comment-only file.
    "// only a comment\n// second line\n",
    // Comment inside a multi-line expression that formatting collapses.
    "let x = foo(\n  1, // first arg\n  2\n)\n",
    // Comment inside a container's inline children block.
    "row: Row, gap: 8 {\n  // child comment\n  a: Rect, size: (10, 10)\n}\n",
    // Comments at both ends of a sequence block, plus a trailing action.
    "sequence {\n  // before action\n  fade-in a [1s] // after action\n  // after action line\n}\n",
    // Trailing comments inside a type constructor's property list.
    "circ = Circle {\n  at: (0, 0), // center\n  radius: 100, // size\n}\n",
];
