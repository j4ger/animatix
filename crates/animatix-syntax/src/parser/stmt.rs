//!
//! Statement parsers for the Animatix DSL.
//!
//! The actual statement combinator construction lives in [`super::parser()`]
//! because all statement parsers depend on shared local variables (`expr`,
//! `ident`, `property`, `modifiers`, `inline_items`, and the recursive `_stmt`).
//!
//! This module exists as a home for statement-related helpers
//! extracted in future refactors.