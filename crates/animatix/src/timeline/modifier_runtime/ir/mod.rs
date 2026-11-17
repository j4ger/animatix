mod display;
mod eval;
mod exec;
pub mod fast_program;
mod fast_scalar;
mod lower;
pub mod opt;
mod types;

// display module provides `impl Display for` IR types; re-export is not needed
// because the trait implementations are used via `.to_string()` calls.
pub(crate) use eval::*;
pub use exec::*;
pub use fast_program::*;
pub use fast_scalar::FastScalarExpr;
pub(crate) use fast_scalar::{
    compile_fast_scalar_with_env, compile_fast_vec2_with_env, fast_scalar_intrinsics_are_standard,
    resolve_slot_values,
};
pub use lower::*;
pub use opt::{OptStats, optimize_expr, optimize_program};
pub use types::*;
