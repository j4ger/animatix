//! LSP request handlers, grouped by method family.
//!
//! `LanguageServer` is implemented once in `server.rs`; every request method
//! there delegates to the matching function in these modules.

pub(crate) mod actions;
pub(crate) mod formatting;
pub(crate) mod navigation;
pub(crate) mod rename;
pub(crate) mod symbols;
