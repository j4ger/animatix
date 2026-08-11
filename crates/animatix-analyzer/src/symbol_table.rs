//! Symbol table extraction moved to `animatix-syntax`.
//!
//! The analyzer re-exports the shared type so existing callers keep working
//! while semantic symbols live in the syntax crate with the AST and type model.

pub use animatix_syntax::symbol_table::*;
