//! Symbol table extraction; the implementation lives in `animatix-syntax`.
//!
//! The analyzer re-exports the shared type so semantic symbols stay in the
//! syntax crate with the AST and type model.

pub use animatix_syntax::symbol_table::*;
