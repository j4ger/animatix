mod display;
mod eval;
mod exec;
pub mod fast_program;
mod lower;
pub mod opt;
mod types;

// display module provides `impl Display for` IR types; re-export is not needed
// because the trait implementations are used via `.to_string()` calls.
pub use eval::FastScalarExpr;
pub(crate) use eval::*;
pub use exec::*;
pub use fast_program::*;
pub use lower::*;
pub use opt::{OptStats, optimize_expr, optimize_program};
pub use types::*;
