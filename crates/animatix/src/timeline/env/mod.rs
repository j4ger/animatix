use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::timeline::modifier_runtime::ir::CompiledExpr;

mod fx;
pub use fx::EnvMap;

/// Error produced during expression evaluation in the timeline environment.
#[derive(Debug, Clone)]
pub enum EvalError {
    /// Referenced variable does not exist in the environment.
    UndefinedVariable(String),
    /// Value type does not match the expected type for an operation.
    TypeMismatch(String),
    /// Attempted to call a non-function value.
    NotCallable(String),
    /// Method name is not supported on the target type.
    UnsupportedMethod(String),
    /// Indexing operation is not supported.
    UnsupportedIndex,
    /// Language construct is not supported at runtime.
    UnsupportedConstruct(String),
}

/// Captured variable environment snapshot taken at closure creation time.
///
/// # Capture semantics
///
/// `CapturedEnv` stores **only** the `overrides` layer of the surrounding
/// [`Environment`] at the point of closure creation.  The `base` layer
/// (stdlib / colorscheme `NativeFn`s, ~90 entries) is intentionally excluded:
///
/// - Built-in math functions (`sin`, `cos`, `abs`, …) are resolved by
///   `eval_shared::eval_builtin_fn` *before* any environment lookup, so they are always available
///   inside closures regardless of `base`.
/// - `NativeFn` values from the base (e.g. colorscheme samplers) are re-provided at render time
///   through `build_frame_env`; call sites that invoke closures must propagate `env.base`
///   themselves (see [`CapturedEnv::merge_into`]).
///
/// **Guarantee**: every `merge_into` call site passes an [`Environment`]
/// whose `base` Arc is already set to the timeline stdlib.  Debug assertions
/// verify this invariant at runtime (non-release builds).
#[derive(Clone, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CapturedEnv(pub HashMap<String, Value>);

mod captured;
mod environment;
mod errors;
mod object_path;
mod value;

pub use environment::Environment;
pub(crate) use value::EnvStamp;
pub use value::Value;
