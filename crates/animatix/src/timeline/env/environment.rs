use super::object_path::update_object_path;
use super::value::{ENV_INSTANCE_COUNTER, EnvStamp};
use super::*;

fn fresh_env_instance() -> u32 {
    ENV_INSTANCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Variable environment for expression evaluation.
///
/// Uses an override layer over an optional shared base to avoid copying
/// large stdlib maps on every frame.
pub struct Environment {
    pub(crate) overrides: HashMap<String, Value>,
    /// P2.22: Shared base layer. `get()` checks overrides first, then falls back
    /// to base. This avoids copying ~90 stdlib entries on every [`Timeline::build_frame_env`].
    pub(crate) base: Option<Arc<HashMap<String, Value>>>,
    /// P6.2+: Dual-variable binding overlay for plot sampling.
    /// Avoids cloning the entire overrides HashMap for every sample point.
    /// Two slots allow setting both `x` and `y` for scalar/vector field evaluation.
    pub(crate) bindings: [Option<(String, Value)>; 2],
    /// Memoization identity; see [`EnvStamp`]. Bumped on every mutation.
    stamp: EnvStamp,
    /// LetChain scoped stack (see `push_let_scope`/`pop_let_scope`): the
    /// expression cache consults `let_epoch` so values computed inside a
    /// let-scope are never reused outside it. `RefCell` keeps
    /// `evaluate_expr(&Expr, &Environment)` signatures unchanged.
    pub(crate) let_scopes: std::cell::RefCell<Vec<HashMap<String, Value>>>,
    /// Cache-key epoch bumped on every let-scope push/pop (see `let_epoch`).
    let_epoch: std::cell::Cell<u64>,
}

impl Clone for Environment {
    fn clone(&self) -> Self {
        Self {
            overrides: self.overrides.clone(),
            base: self.base.clone(),
            bindings: self.bindings.clone(),
            // A fresh instance id keeps the clone's cache entries from ever
            // colliding with the original's, even if both sides mutate.
            stamp: EnvStamp {
                instance: fresh_env_instance(),
                version: 0,
            },
            let_scopes: std::cell::RefCell::new(Vec::new()),
            let_epoch: std::cell::Cell::new(0),
        }
    }
}

impl Default for Environment {
    fn default() -> Self {
        Self::new()
    }
}

impl Environment {
    /// Create an empty environment with no base layer.
    pub fn new() -> Self {
        Environment {
            overrides: HashMap::new(),
            base: None,
            bindings: [None, None],
            stamp: EnvStamp {
                instance: fresh_env_instance(),
                version: 0,
            },
            let_scopes: std::cell::RefCell::new(Vec::new()),
            let_epoch: std::cell::Cell::new(0),
        }
    }

    /// Create an empty environment with the given initial capacity.
    pub fn with_capacity(capacity: usize) -> Self {
        Environment {
            overrides: HashMap::with_capacity(capacity),
            base: None,
            bindings: [None, None],
            stamp: EnvStamp {
                instance: fresh_env_instance(),
                version: 0,
            },
            let_scopes: std::cell::RefCell::new(Vec::new()),
            let_epoch: std::cell::Cell::new(0),
        }
    }

    /// Create an environment with a shared base layer.
    pub fn with_base(base: Arc<HashMap<String, Value>>) -> Self {
        Environment {
            overrides: HashMap::new(),
            base: Some(base),
            bindings: [None, None],
            stamp: EnvStamp {
                instance: fresh_env_instance(),
                version: 0,
            },
            let_scopes: std::cell::RefCell::new(Vec::new()),
            let_epoch: std::cell::Cell::new(0),
        }
    }

    /// Record a content mutation for cache identity (see [`EnvStamp`]).
    ///
    /// Must be called by every path that mutates `overrides` or `bindings`
    /// directly instead of going through the mutating methods.
    #[inline]
    pub(crate) fn mark_mutated(&mut self) {
        self.stamp.version += 1;
    }

    /// Push a LetChain scope: a set of `let` bindings visible until the
    /// matching [`Self::pop_let_scope`]. Innermost scope wins lookups (see
    /// `get`). Uses interior mutability so expression evaluation can open a
    /// scope through a shared `&Environment` reference.
    pub(crate) fn push_let_scope(&self, bindings: Vec<(String, Value)>) {
        self.let_scopes.borrow_mut().push(bindings.into_iter().collect());
        self.let_epoch.set(self.let_epoch.get() + 1);
    }

    /// Pop the innermost LetChain scope pushed by [`Self::push_let_scope`].
    pub(crate) fn pop_let_scope(&self) {
        self.let_scopes.borrow_mut().pop();
        self.let_epoch.set(self.let_epoch.get() + 1);
    }

    /// Monotonic epoch that changes on every let-scope push/pop. Part of the
    /// expression-cache key (see `utils::evaluate_expr`): a cached value
    /// computed under one let-scope must never be reused under another.
    pub(crate) fn let_epoch(&self) -> u64 {
        self.let_epoch.get()
    }

    /// Cache identity for expression memoization (see [`EnvStamp`]).
    #[inline]
    pub(crate) fn stamp(&self) -> EnvStamp {
        self.stamp
    }

    /// Reserve additional capacity in the override layer.
    pub fn reserve_overrides(&mut self, additional: usize) {
        self.overrides.reserve(additional);
    }

    /// Insert or overwrite a variable in the override layer.
    ///
    /// PF-6: on a pooled environment (see
    /// `Timeline::build_frame_env_internal`) the key already exists, so
    /// `get_mut`-first overwrites the stored value in place — no key copy at
    /// all. The `insert` path only fires for keys the pool has not seen.
    pub fn set(&mut self, name: &str, value: Value) {
        self.mark_mutated();
        match self.overrides.get_mut(name) {
            Some(slot) => *slot = value,
            None => {
                self.overrides.insert(name.to_string(), value);
            },
        }
    }

    /// [`Self::set`] taking an owned key (PF-6: callers that already built a
    /// `String` key move it in instead of paying a second copy inside `set`).
    pub fn set_owned(&mut self, name: String, value: Value) {
        self.mark_mutated();
        match self.overrides.get_mut(name.as_str()) {
            Some(slot) => *slot = value,
            None => {
                self.overrides.insert(name, value);
            },
        }
    }

    /// Reset a pooled environment so it is indistinguishable from a freshly
    /// built one: a new memoization identity (see [`EnvStamp`]) and cleared
    /// plot-sampling bindings. The overrides map is deliberately *not*
    /// cleared — its table capacity and key `String` allocations are the
    /// whole point of pooling; the frame-env rebuild overwrites every key it
    /// needs in place via `set`'s `get_mut` fast path.
    pub(crate) fn reset_for_reuse(&mut self) {
        self.stamp = EnvStamp {
            instance: fresh_env_instance(),
            version: 0,
        };
        self.bindings = [None, None];
        self.let_scopes.borrow_mut().clear();
        self.let_epoch.set(0);
    }

    /// Update one field on a frame-local `Value::Object` variable.
    ///
    /// Returns `false` when `name` does not resolve to an object, leaving the
    /// caller free to fall back to actor/property override semantics.
    pub fn set_object_field(&mut self, name: &str, field: &str, value: Value) -> bool {
        self.set_object_path(&[], name, field, value)
    }

    /// Update a field on a frame-local `Value::Object` reached through nested
    /// object fields. `path` excludes the root variable name.
    pub fn set_object_path(
        &mut self,
        path: &[String],
        root: &str,
        field: &str,
        value: Value,
    ) -> bool {
        let Some(current) = self.get(root) else {
            return false;
        };
        let field_path = path.iter().map(String::as_str).collect::<Vec<_>>();
        let Some(updated) = update_object_path(current, &field_path, field, value.clone()) else {
            return false;
        };
        self.set(root, updated);
        let mut key = root.to_string();
        for part in path {
            key.push('.');
            key.push_str(part);
        }
        key.push('.');
        key.push_str(field);
        self.set(&key, value);
        true
    }

    /// Extend this environment with all values from another.
    /// If other has a base, we adopt it (or merge if we already have one).
    pub fn extend_from(&mut self, other: &Environment) {
        self.mark_mutated();
        for (k, v) in &other.overrides {
            self.overrides.insert(k.clone(), v.clone());
        }
        if let Some(ref other_base) = other.base {
            if self.base.is_none() {
                self.base = Some(Arc::clone(other_base));
            } else {
                // Merge other_base entries into our overrides (shadowing our base)
                for (k, v) in other_base.iter() {
                    if !self.overrides.contains_key(k) {
                        self.overrides.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }

    /// Look up a variable by name, returning a clone.
    /// Checks bindings → overrides → base, in that order.
    pub fn get(&self, name: &str) -> Option<Value> {
        {
            let scopes = self.let_scopes.borrow();
            for scope in scopes.iter().rev() {
                if let Some(value) = scope.get(name) {
                    return Some(value.clone());
                }
            }
        }
        for binding in self.bindings.iter().flatten() {
            if binding.0 == name {
                return Some(binding.1.clone());
            }
        }
        self.overrides
            .get(name)
            .cloned()
            .or_else(|| self.base.as_ref().and_then(|b| b.get(name).cloned()))
    }

    /// Look up a variable by name, returning a reference (zero-copy).
    /// Checks bindings → overrides → base, in that order.
    ///
    /// NOTE: LetChain scopes (`push_let_scope`) are deliberately invisible
    /// here — this method cannot return a reference into the `RefCell`. All
    /// current callers are presence-only tests that run strictly outside any
    /// active LetChain scope; the tripwire below keeps that invariant loud.
    pub fn get_ref(&self, name: &str) -> Option<&Value> {
        debug_assert!(
            self.let_scopes.borrow().is_empty(),
            "get_ref called while a LetChain scope is active — scoped values are              invisible by reference; use get() instead (looking up '{name}')"
        );
        for binding in self.bindings.iter().flatten() {
            if binding.0 == name {
                return Some(&binding.1);
            }
        }
        self.overrides
            .get(name)
            .or_else(|| self.base.as_ref().and_then(|b| b.get(name)))
    }

    /// Look up a dotted path like `p.x`, first as an injected key and then by
    /// walking `Value::Object` fields.
    pub fn get_path(&self, path: &str) -> Option<Value> {
        if let Some(value) = self.get(path) {
            return Some(value);
        }

        let (first, rest) = path.split_once('.')?;
        let mut current = self.get(first)?;
        for field in rest.split('.') {
            current = current.get_field(field)?.clone();
        }
        Some(current)
    }

    /// Set a variable binding overlay. Uses the first available slot, or
    /// replaces an existing binding with the same name.
    pub fn set_binding(&mut self, name: &str, value: Value) {
        self.mark_mutated();
        // Replace existing binding with same name, or fill first empty slot
        for slot in self.bindings.iter_mut() {
            match slot {
                Some((existing_name, slot_val)) if existing_name == name => {
                    *slot_val = value;
                    return;
                },
                None => {
                    *slot = Some((name.to_string(), value));
                    return;
                },
                _ => {},
            }
        }
        // Both slots occupied — replace the first one
        self.bindings[0] = Some((name.to_string(), value));
    }

    /// Clear all variable binding overlays.
    pub fn clear_bindings(&mut self) {
        self.mark_mutated();
        self.bindings = [None, None];
    }

    /// Return all variable names defined in this environment, sorted.
    pub fn all_keys(&self) -> Vec<String> {
        let mut keys: Vec<String> = self.overrides.keys().cloned().collect();
        if let Some(ref base) = self.base {
            for k in base.keys() {
                if !self.overrides.contains_key(k) {
                    keys.push(k.clone());
                }
            }
        }
        keys.sort();
        keys
    }

    /// Total number of distinct variables (overrides + base, minus overlap).
    pub fn len(&self) -> usize {
        let base_count = self.base.as_ref().map(|b| b.len()).unwrap_or(0);
        let overlap = self
            .base
            .as_ref()
            .map(|b| self.overrides.keys().filter(|k| b.contains_key(*k)).count())
            .unwrap_or(0);
        self.overrides.len() + base_count - overlap
    }

    /// Returns true if no variables are defined in either layer.
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty() && self.base.as_ref().map(|b| b.is_empty()).unwrap_or(true)
    }
}
