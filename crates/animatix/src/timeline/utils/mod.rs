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

pub(crate) use expr::{
    call_with_points, evaluate_call_arguments, evaluate_call_value, evaluate_callable_value,
    evaluate_expr_with_points, evaluate_method_arguments, invoke_closure_value,
};
pub use expr::{
    clear_eval_cache, disable_eval_cache, enable_eval_cache, evaluate_expr, safe_div, safe_rem,
};
pub use value::{
    parse_color, parse_color_in_env, resolve_color_in_env, time_to_ms, time_to_ms_at_beat,
};
