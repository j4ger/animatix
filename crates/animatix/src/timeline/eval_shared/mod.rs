//! Shared evaluation helpers for binary operations and builtin functions.
//!
//! These are pure functions that can be called from both the tree-walker
//! (`utils.rs`) and the IR/VM (`ir/eval.rs`) evaluation paths, eliminating
//! code duplication between the two execution engines.
//!
//! Each function operates on already-evaluated [`Value`] arguments and returns
//! a [`Result<Value, EvalError>`] — no environment lookups or expression
//! recursion happen here.

use crate::ast::BinaryOp;
use crate::timeline::utils::{safe_div, safe_rem};
use crate::timeline::{EvalError, Value};

/// Evaluate a binary operation on two runtime values.
///
/// Supports:
/// - Num-Num ops (arithmetic, comparison, logical)
/// - Vec-Vec elementwise ops (Add, Sub, Mul, Div, Mod)
/// - Vec-Num / Num-Vec broadcasting
/// - Color-Color and Color-Num / Num-Color ops
/// - Arbitrary-type Eq/Neq (structural equality)
///
/// Returns [`EvalError::TypeMismatch`] for unsupported type/op combinations.
mod binary;
mod builtins;
mod convert;
mod curves;
mod format;
#[cfg(test)]
mod tests;

pub use binary::eval_binary_op;
pub use builtins::eval_builtin_fn;
pub use convert::{value_to_color, value_to_f64, value_to_list, value_to_vec2, value_to_vec3};
