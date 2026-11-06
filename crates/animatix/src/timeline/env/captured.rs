//! `CapturedEnv`: lexical-scope capture and merge into a render-time environment.

use super::{CapturedEnv, Environment};

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
        // `CapturedEnv` keeps the std `HashMap` flavor (public API); the
        // conversion rehashes once per closure creation, off the frame path.
        CapturedEnv(captured.into_iter().collect())
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
    /// The presence test borrows (`get_ref`) instead of cloning — captures can
    /// be large lists (an FFT sample array captured by a plot closure), and the
    /// merge runs once per sample point.
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
