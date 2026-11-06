//! Helpers shared by in-crate unit tests.
//!
//! Gated behind `#[cfg(test)]` in `lib.rs`; nothing here is part of the public
//! API.

use crate::ast::{Property, Stmt};

/// Parse `source`, requiring a clean parse, and return its statements.
pub(crate) fn parse(source: &str) -> Vec<Stmt> {
    let (stmts, errors) = crate::parser::parse_source(source);
    assert!(
        errors.is_empty(),
        "parse errors for {source:?}: {:?}",
        errors.iter().map(|e| &e.message).collect::<Vec<_>>()
    );
    stmts.expect("parsed AST")
}

/// An actor declaration with no properties or modifiers: `label: ty`.
pub(crate) fn actor(label: &str, ty: &str) -> Stmt {
    actor_with_props(label, ty, Vec::new())
}

/// An actor declaration carrying authored `props`.
pub(crate) fn actor_with_props(label: &str, ty: &str, props: Vec<Property>) -> Stmt {
    Stmt::ActorDecl {
        is_pub: false,
        is_anonymous: false,
        label: label.to_string(),
        array_index: None,
        ty: ty.to_string(),
        props,
        modifiers: vec![],
        children: vec![],
        span: None,
    }
}
