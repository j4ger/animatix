//! Intermediate representation for frame-time modifier and plot-closure code.
//!
//! Build-time expressions use the AST tree-walker; modifier code and plot
//! closures are lowered to this IR and interpreted by the single IR executor.
//! This module is a re-export of
//! [`crate::timeline::modifier_runtime::ir`] kept at the historical path.

pub use crate::timeline::modifier_runtime::ir::*;
