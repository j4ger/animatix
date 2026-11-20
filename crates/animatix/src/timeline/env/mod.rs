use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::timeline::modifier_runtime::ir::CompiledExpr;

/// Callback type used to install a typed callable in an expression environment.
pub type EnvironmentCallable =
    Arc<dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync>;

mod fx;
pub use fx::EnvMap;

/// Error produced during expression evaluation in the timeline environment.
#[derive(Debug, Clone)]
pub enum EvalError {
    /// Referenced variable does not exist in the environment.
    UndefinedVariable(String),
    /// Value type does not match the expected type for an operation.
    TypeMismatch(String),
    /// A string was used where a color value is required.
    StringInColor,
    /// A numeric color-constructor channel is outside the normalized range.
    ColorChannelOutOfRange {
        /// Name of the color constructor.
        constructor: &'static str,
        /// Channel name: r, g, b, or a.
        channel: &'static str,
        /// Supplied numeric channel value.
        value: f64,
    },
    /// Attempted to call a non-function value.
    NotCallable(String),
    /// Method name is not supported on the target type.
    UnsupportedMethod(String),
    /// Indexing operation is not supported.
    UnsupportedIndex,
    /// An actor array was indexed with a negative, fractional, or non-finite value.
    InvalidActorIndex(f64),
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

mod actor_bindings;
mod actor_capture_value;
mod captured;
mod environment;
mod errors;
mod loop_scope;
mod object_path;
mod value;

pub(crate) use actor_capture_value::{rebind_actor_value, rebind_captured_env};
pub use environment::Environment;
pub(crate) use loop_scope::LoopScope;
pub(crate) use value::ComponentMethodScope;
pub(crate) use value::EnvStamp;
pub use value::Value;

/// Internal IR identifier for the frame clock, separate from authored `t`.
pub(crate) const FRAME_TIME_IDENT: &str = "\0animatix.frame.time";
