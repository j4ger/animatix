use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::timeline::modifier_runtime::ir::CompiledExpr;

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

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EvalError::UndefinedVariable(v) => write!(f, "Undefined variable: {}", v),
            EvalError::TypeMismatch(e) => write!(f, "Type mismatch: {}", e),
            EvalError::NotCallable(n) => write!(f, "Not callable: {}", n),
            EvalError::UnsupportedMethod(name) => {
                write!(f, "Unsupported runtime method call: {}", name)
            },
            EvalError::UnsupportedIndex => {
                write!(f, "Unsupported runtime index expression")
            },
            EvalError::UnsupportedConstruct(name) => {
                write!(f, "Unsupported runtime construct expression: {}", name)
            },
        }
    }
}

impl std::error::Error for EvalError {}

impl From<crate::timeline::modifier_runtime::ir::IrLowerError> for EvalError {
    fn from(err: crate::timeline::modifier_runtime::ir::IrLowerError) -> Self {
        EvalError::UnsupportedConstruct(err.to_string())
    }
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

impl CapturedEnv {
    /// Create a `CapturedEnv` from the environment's override layer.
    /// Captures the lexical scope at the point of closure creation,
    /// including any active LetChain scopes (innermost wins — see
    /// `push_let_scope`). Stdlib base values are NOT captured; invocation
    /// re-provides the caller's base.
    pub fn snapshot(env: &Environment) -> Self {
        let mut captured = env.overrides.clone();
        {
            let scopes = env.let_scopes.borrow();
            for scope in scopes.iter() {
                // Later (innermost) scopes must win: merge under.
                for (k, v) in scope {
                    captured.insert(k.clone(), v.clone());
                }
            }
        }
        CapturedEnv(captured)
    }

    /// Merge this captured environment into a mutable environment at render
    /// time, so that captured variables are available during closure evaluation.
    ///
    /// # Precondition
    /// `env` should already carry the stdlib `base` Arc so that `NativeFn`
    /// values (e.g. colorscheme samplers) remain reachable inside the closure.
    /// Built-in math functions are exempt (they bypass the env lookup), but
    /// other runtime-provided `NativeFn`s depend on the base being present.
    /// In debug builds, call sites that evaluate closures assert this invariant
    /// (see `evaluate_call` in `utils.rs`).
    pub fn merge_into(&self, env: &mut Environment) {
        for (k, v) in &self.0 {
            env.set(k, v.clone());
        }
    }

    /// Merge captured variables only when the target env does not already
    /// define them, returning the keys that were inserted. Plot sampling uses
    /// this so frame-time values written by `always { freq = ... }` can shadow
    /// build-time closure captures, then removes only the newly inserted keys
    /// so captures do not leak between plot actors sharing one env.
    ///
    /// PF-6 round 10: the presence test borrows (`get_ref`) instead of
    /// cloning — captures can be large lists (an FFT sample array captured by
    /// a plot closure was deep-cloned per sample point: 85 KB/frame on
    /// `fft_explain` measured by `export_alloc_driver`).
    pub fn merge_missing_into(&self, env: &mut Environment) -> Vec<String> {
        let mut inserted = Vec::new();
        for (k, v) in &self.0 {
            if env.get_ref(k).is_none() {
                env.set(k, v.clone());
                inserted.push(k.clone());
            }
        }
        inserted
    }
}

impl std::fmt::Debug for CapturedEnv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "CapturedEnv({:?})", self.0)
    }
}

mod environment;
mod object_path;
mod value;

pub use environment::Environment;
pub(crate) use value::EnvStamp;
pub use value::Value;
