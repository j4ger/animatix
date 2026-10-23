//! Expression evaluator for the animation timeline.
//!
//! Uses an [`Environment`](crate::timeline::env::Environment) (`Rc<RefCell<HashMap>>`) for shared
//! mutable scope.
//! Built-ins (sin, cos, lerp, rand, format) resolve through the environment.
//! Closures evaluate against a clone of the caller environment with parameter bindings added.

mod expr;
#[cfg(test)]
mod tests;
mod value;

pub use expr::{
    clear_eval_cache, disable_eval_cache, enable_eval_cache, evaluate_expr, safe_div, safe_rem,
};
pub(crate) use expr::{eval_method_dispatch, evaluate_call_value};
// The pre-split module exposed this crate-wide; keep the path intact even
// though its only caller is `evaluate_call_value` today.
#[allow(unused_imports)]
pub(crate) use expr::invoke_closure_value;
pub use value::{
    color_from_text, parse_color, parse_color_in_env, resolve_color_in_env, time_to_ms,
    time_to_ms_at_beat,
};
