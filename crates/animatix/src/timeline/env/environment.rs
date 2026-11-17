use super::object_path::update_object_path;
use super::value::{ENV_INSTANCE_COUNTER, EnvStamp};
use super::*;
use smallvec::SmallVec;

/// Inline storage for one `LetChain` scope (the common case is 1–2 bindings).
type LetScope = SmallVec<[(String, Value); 2]>;

fn fresh_env_instance() -> u32 {
    ENV_INSTANCE_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

/// Variable environment for expression evaluation.
///
/// Uses an override layer over an optional shared base to avoid copying
/// large stdlib maps on every frame.
pub struct Environment {
    pub(crate) overrides: EnvMap,
    /// Read-only numeric input layer. It sits above global mutable overrides
    /// and tracks, while lexical let scopes and function/sample bindings still
    /// shadow it normally. Frame writers therefore cannot mask an external
    /// input, and closure parameters keep their lexical meaning.
    pub(crate) runtime_inputs: Option<Arc<std::collections::BTreeMap<String, Value>>>,
    /// Shared base layer. `get()` checks overrides first, then falls back to
    /// base. Keeps the ~90-entry stdlib map out of every
    /// [`Timeline::build_frame_env`] copy.
    pub(crate) base: Option<Arc<EnvMap>>,
    /// Read-only view of scene actor references available to isolated user
    /// function environments. Lookups filter this base to ActorRef values, so
    /// module functions can resolve authored scene actors without inheriting
    /// caller locals or copying the scene environment.
    actor_ref_base: Option<Arc<EnvMap>>,
    /// Callable contracts visible in this lexical environment. The shared
    /// table avoids cloning descriptor metadata into every frame environment.
    pub(crate) callable_descriptors:
        Arc<std::collections::HashMap<String, Arc<animatix_core::callable::CallableDescriptor>>>,
    /// Build-time values may create plans and mutate the host Timeline;
    /// frame-time expressions may call only pure, frame-safe functions.
    pub(crate) evaluation_phase: EvaluationPhase,
    /// The immutable frame clock used by internal interval guards. It stays
    /// separate from the authored `t` binding so a local variable may shadow it.
    pub(crate) frame_time_seconds: Option<f64>,
    /// Whether the current lexical environment binds authored `t` explicitly.
    /// This is metadata for plan-local clock rewriting, not a frame value.
    pub(crate) authored_t_binding: bool,
    /// Source module for relative assets created by build-time functions and
    /// timeline plans. Frame environments inherit this immutable origin.
    pub(crate) source_path: Option<Arc<std::path::PathBuf>>,
    /// Loaded source functions used to bind method templates captured by
    /// timeline expressions. This is compiler metadata, not frame state.
    pub(crate) function_context: Option<Arc<crate::module::FunctionContext>>,
    /// Host-level values such as configured palette entries and CLI defines.
    /// Pure functions inherit this table without inheriting caller locals.
    host_globals: Arc<HashMap<String, Value>>,
    /// Component method templates available in the current expanded build
    /// scope. This is callable-resolution metadata, not per-actor ECS state.
    component_methods: Arc<HashMap<String, HashMap<String, Value>>>,
    /// Immutable prebound callables addressed by the `MethodTableId` stored
    /// on the corresponding World entity. IDs index this scene-local vector;
    /// frame dispatch never resolves an actor label through `component_methods`.
    pub(crate) component_method_tables: Arc<Vec<Arc<HashMap<String, Value>>>>,
    /// Active lexical component-method scope, set only while evaluating one
    /// component method. It permits private calls from that component body.
    pub(crate) active_component_scope: Option<Arc<super::value::ComponentMethodScope>>,
    /// Dual-variable binding overlay for plot sampling.
    /// Avoids cloning the entire overrides map for every sample point.
    /// Two slots allow setting both `x` and `y` for scalar/vector field evaluation.
    pub(crate) bindings: [Option<(String, Value)>; 2],
    /// Memoization identity; see [`EnvStamp`]. Bumped on every mutation.
    stamp: EnvStamp,
    /// LetChain scoped stack (see `push_let_scope`/`pop_let_scope`): the
    /// expression cache consults `let_epoch` so values computed inside a
    /// let-scope are never reused outside it. `RefCell` keeps
    /// `evaluate_expr(&Expr, &Environment)` signatures unchanged.
    pub(crate) let_scopes: std::cell::RefCell<Vec<LetScope>>,
    /// Cache-key epoch bumped on every let-scope push/pop (see `let_epoch`).
    let_epoch: std::cell::Cell<u64>,
}

impl Clone for Environment {
    fn clone(&self) -> Self {
        Self {
            overrides: self.overrides.clone(),
            runtime_inputs: self.runtime_inputs.clone(),
            base: self.base.clone(),
            actor_ref_base: self.actor_ref_base.clone(),
            callable_descriptors: Arc::clone(&self.callable_descriptors),
            evaluation_phase: self.evaluation_phase,
            frame_time_seconds: self.frame_time_seconds,
            authored_t_binding: self.authored_t_binding,
            source_path: self.source_path.clone(),
            function_context: self.function_context.clone(),
            host_globals: Arc::clone(&self.host_globals),
            component_methods: Arc::clone(&self.component_methods),
            component_method_tables: Arc::clone(&self.component_method_tables),
            active_component_scope: self.active_component_scope.clone(),
            bindings: self.bindings.clone(),
            // A fresh instance id keeps the clone's cache entries from ever
            // colliding with the original's, even if both sides mutate.
            stamp: EnvStamp {
                instance: fresh_env_instance(),
                version: 0,
            },
            let_scopes: std::cell::RefCell::new(self.let_scopes.borrow().clone()),
            let_epoch: std::cell::Cell::new(self.let_epoch.get()),
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
            overrides: EnvMap::default(),
            runtime_inputs: None,
            base: None,
            actor_ref_base: None,
            callable_descriptors: Arc::new(std::collections::HashMap::new()),
            evaluation_phase: EvaluationPhase::Build,
            frame_time_seconds: None,
            authored_t_binding: false,
            source_path: None,
            function_context: None,
            host_globals: Arc::new(HashMap::new()),
            component_methods: Arc::new(HashMap::new()),
            component_method_tables: Arc::new(Vec::new()),
            active_component_scope: None,
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
            overrides: HashMap::with_capacity_and_hasher(capacity, Default::default()),
            runtime_inputs: None,
            base: None,
            actor_ref_base: None,
            callable_descriptors: Arc::new(std::collections::HashMap::new()),
            evaluation_phase: EvaluationPhase::Build,
            frame_time_seconds: None,
            authored_t_binding: false,
            source_path: None,
            function_context: None,
            host_globals: Arc::new(HashMap::new()),
            component_methods: Arc::new(HashMap::new()),
            component_method_tables: Arc::new(Vec::new()),
            active_component_scope: None,
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
    pub fn with_base(base: Arc<EnvMap>) -> Self {
        Environment {
            overrides: EnvMap::default(),
            runtime_inputs: None,
            base: Some(base),
            actor_ref_base: None,
            callable_descriptors: Arc::new(std::collections::HashMap::new()),
            evaluation_phase: EvaluationPhase::Build,
            frame_time_seconds: None,
            authored_t_binding: false,
            source_path: None,
            function_context: None,
            host_globals: Arc::new(HashMap::new()),
            component_methods: Arc::new(HashMap::new()),
            component_method_tables: Arc::new(Vec::new()),
            active_component_scope: None,
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
    pub(crate) fn push_let_scope(&self, bindings: impl IntoIterator<Item = (String, Value)>) {
        self.let_scopes.borrow_mut().push(bindings.into_iter().collect());
        self.let_epoch.set(self.let_epoch.get() + 1);
    }

    /// Insert or replace a name in the innermost lexical scope.
    pub(crate) fn set_let_binding(&self, name: &str, value: Value) {
        let mut scopes = self.let_scopes.borrow_mut();
        let scope = scopes.last_mut().expect("lexical bindings require an active let scope");
        if let Some((_, existing)) = scope.iter_mut().find(|(key, _)| key == name) {
            *existing = value;
        } else {
            scope.push((name.to_string(), value));
        }
        self.let_epoch.set(self.let_epoch.get() + 1);
    }

    /// Rebind a function-local name, updating its nearest existing lexical
    /// binding or inserting it into the function scope. Loop scopes remain
    /// isolated unless the declaration intentionally shadows a loop name.
    pub(crate) fn set_function_local_binding(&self, name: &str, value: Value) {
        let mut scopes = self.let_scopes.borrow_mut();
        assert!(!scopes.is_empty(), "function-local bindings require an active function scope");
        let scope_index = scopes
            .iter()
            .rposition(|scope| scope.iter().any(|(key, _)| key == name))
            .unwrap_or(0);
        let scope = &mut scopes[scope_index];
        if let Some((_, existing)) = scope.iter_mut().find(|(key, _)| key == name) {
            *existing = value;
        } else {
            scope.push((name.to_string(), value));
        }
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
    /// On a pooled environment (see `Timeline::build_frame_env_internal`) the
    /// key already exists, so `get_mut`-first overwrites the stored value in
    /// place — no key copy at all. The `insert` path only fires for keys the
    /// pool has not seen.
    pub fn set(&mut self, name: &str, value: Value) {
        self.mark_mutated();
        match self.overrides.get_mut(name) {
            Some(slot) => *slot = value,
            None => {
                self.overrides.insert(name.to_string(), value);
            },
        }
    }

    /// Set an authored lexical `t` binding and record its source-level origin.
    pub(crate) fn set_authored_t(&mut self, value: Value) {
        self.set("t", value);
        self.authored_t_binding = true;
    }

    /// Restore the authored-`t` metadata captured at a lexical scope boundary.
    pub(crate) fn set_authored_t_binding(&mut self, bound: bool) {
        self.authored_t_binding = bound;
    }

    /// Whether `t` is currently supplied by a source binding.
    pub(crate) fn has_authored_t_binding(&self) -> bool {
        self.authored_t_binding
    }

    /// Set the source module used by values created in this lexical scope.
    pub(crate) fn set_source_path(&mut self, source_path: Option<Arc<std::path::PathBuf>>) {
        self.source_path = source_path;
        self.mark_mutated();
    }

    /// Install the loaded module context for plans and component-method
    /// templates created in this lexical environment.
    pub(crate) fn set_function_context(
        &mut self,
        context: Option<Arc<crate::module::FunctionContext>>,
    ) {
        self.function_context = context;
        self.mark_mutated();
    }

    /// Source module associated with this lexical scope, when file-backed.
    pub(crate) fn source_path(&self) -> Option<&std::path::Path> {
        self.source_path.as_deref().map(std::path::PathBuf::as_path)
    }

    /// Set a host-controlled global and remember it for isolated function scopes.
    pub fn set_host_global(&mut self, name: &str, value: Value) {
        self.set(name, value.clone());
        Arc::make_mut(&mut self.host_globals).insert(name.to_string(), value);
    }

    /// Whether a host value is explicitly retained across isolated scopes.
    pub(crate) fn has_host_global(&self, name: &str) -> bool {
        self.host_globals.contains_key(name)
    }

    /// Retain callable and lexical metadata when constructing a frame over a
    /// shared value base. This borrows immutable tables without copying their
    /// values into the pooled frame override map.
    pub(crate) fn inherit_frame_metadata_from(&mut self, source: &Environment) {
        self.callable_descriptors = Arc::clone(&source.callable_descriptors);
        self.host_globals = Arc::clone(&source.host_globals);
        self.component_method_tables = Arc::clone(&source.component_method_tables);
        self.actor_ref_base = source.actor_ref_base.clone();
        self.function_context = source.function_context.clone();
        self.source_path = source.source_path.clone();
        self.active_component_scope = None;
    }

    /// Install a component-bound function in the current build's callable
    /// resolution scope. The table lives on the environment and is inherited
    /// by lexical function environments; it is not timeline/entity state.
    pub(crate) fn set_component_method(&mut self, label: &str, name: &str, value: Value) {
        Arc::make_mut(&mut self.component_methods)
            .entry(label.to_string())
            .or_default()
            .insert(name.to_string(), value);
        self.mark_mutated();
    }

    /// Snapshot method resolution metadata so a mounted plan can restore the
    /// caller's method scope after lowering.
    pub(crate) fn component_methods_snapshot(
        &self,
    ) -> Arc<HashMap<String, HashMap<String, Value>>> {
        Arc::clone(&self.component_methods)
    }

    /// Mark the lexical component method currently evaluating.
    pub(crate) fn set_active_component_scope(
        &mut self,
        scope: Option<Arc<super::value::ComponentMethodScope>>,
    ) {
        self.active_component_scope = scope;
        self.mark_mutated();
    }

    /// Remove method bindings from the prior preparation/build scope.
    pub(crate) fn clear_component_methods(&mut self) {
        self.component_methods = Arc::new(HashMap::new());
        self.component_method_tables = Arc::new(Vec::new());
        self.mark_mutated();
    }

    /// Restore the caller's component-method table after a plan mount.
    pub(crate) fn restore_component_methods(
        &mut self,
        methods: Arc<HashMap<String, HashMap<String, Value>>>,
    ) {
        self.component_methods = methods;
        self.mark_mutated();
    }

    /// Resolve a component method for an actor label. Indexed labels fall back
    /// to their source root (`cards__2.badge` → `cards.badge`).
    pub(crate) fn component_method(&self, label: &str, name: &str) -> Option<Value> {
        self.component_methods_for_actor(label)
            .and_then(|methods| methods.get(name).cloned())
    }

    fn component_methods_for_actor(&self, label: &str) -> Option<HashMap<String, Value>> {
        self.component_methods
            .get(label)
            .or_else(|| {
                let source_label = label
                    .split('.')
                    .map(|part| crate::ast::is_array_member_label(part).unwrap_or(part))
                    .collect::<Vec<_>>()
                    .join(".");
                self.component_methods.get(&source_label)
            })
            .cloned()
    }

    /// Bind the methods for a newly built entity once, before frame evaluation.
    /// The resulting callable table is immutable and its handle is stored on
    /// the entity in `World`.
    pub(crate) fn bind_component_method_table(
        &mut self,
        label: &str,
        entity_id: crate::timeline::EntityId,
        mount_id: Option<u64>,
    ) -> Option<crate::timeline::world::MethodTableId> {
        let methods = self.component_methods_for_actor(label)?;
        if methods.is_empty() {
            return None;
        }
        let table_index = u32::try_from(self.component_method_tables.len())
            .expect("component method table count exceeds the World handle space");
        let table_id = crate::timeline::world::MethodTableId::new(table_index);
        let actor = mount_id
            .map(|mount_id| crate::timeline::plan_value::ActorRef::mounted(mount_id, label))
            .unwrap_or_else(|| crate::timeline::plan_value::ActorRef::caller(label))
            .with_world_identity(entity_id, Some(table_id));
        let methods =
            crate::timeline::fn_eval::bind_component_method_values(&methods, &actor, self);
        Arc::make_mut(&mut self.component_method_tables).push(Arc::new(methods));
        self.mark_mutated();
        Some(table_id)
    }

    /// Resolve a prebound method from the immutable table carried by an actor
    /// reference. This is the only component-method path used in frames.
    pub(crate) fn component_method_by_table(
        &self,
        table_id: crate::timeline::world::MethodTableId,
        name: &str,
    ) -> Option<Value> {
        self.component_method_tables
            .get(table_id.get() as usize)
            .and_then(|methods| methods.get(name))
            .cloned()
    }

    /// Resolve a method according to evaluation phase. Build-time expansion
    /// can use source labels; frame-time execution requires a World-bound id.
    pub(crate) fn component_method_for_actor(
        &self,
        actor: &crate::timeline::plan_value::ActorRef,
        name: &str,
    ) -> Option<Value> {
        if self.is_frame_phase() {
            actor
                .entity_id()
                .and_then(|_| actor.method_table_id())
                .and_then(|table| self.component_method_by_table(table, name))
        } else {
            self.component_method(actor.label(), name)
        }
    }

    /// Whether an expanded actor label has component method metadata.
    pub(crate) fn has_component_methods(&self, label: &str) -> bool {
        let source_label = label
            .split('.')
            .map(|part| crate::ast::is_array_member_label(part).unwrap_or(part))
            .collect::<Vec<_>>()
            .join(".");
        self.component_methods.contains_key(label)
            || self.component_methods.contains_key(&source_label)
    }

    /// Inherit host globals and frame-only inputs without copying caller locals.
    pub(crate) fn inherit_host_context_from(&mut self, source: &Environment) {
        self.host_globals = Arc::clone(&source.host_globals);
        self.component_methods = if source.is_frame_phase() {
            Arc::new(HashMap::new())
        } else {
            Arc::clone(&source.component_methods)
        };
        self.component_method_tables = Arc::clone(&source.component_method_tables);
        self.actor_ref_base = source.actor_ref_base.clone().or_else(|| source.base.clone());
        self.function_context = source.function_context.clone();
        self.evaluation_phase = source.evaluation_phase;
        let globals = self
            .host_globals
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect::<Vec<_>>();
        for (name, value) in globals {
            self.set(&name, value);
        }
        self.set_runtime_inputs(source.runtime_inputs.clone());
        self.frame_time_seconds = source.frame_time_seconds;
    }

    /// [`Self::set`] taking an owned key: callers that already built a `String`
    /// key move it in instead of paying a second copy inside `set`.
    pub fn set_owned(&mut self, name: String, value: Value) {
        self.mark_mutated();
        match self.overrides.get_mut(name.as_str()) {
            Some(slot) => *slot = value,
            None => {
                self.overrides.insert(name, value);
            },
        }
    }

    /// Install a callable and its shared public signature in this environment.
    pub fn register_callable(
        &mut self,
        descriptor: animatix_core::callable::CallableDescriptor,
        call: EnvironmentCallable,
    ) {
        let name = descriptor.name.clone();
        let qualified = descriptor.qualified_name();
        let descriptor = Arc::new(descriptor);
        let table = Arc::make_mut(&mut self.callable_descriptors);
        if descriptor.namespace == "std" || descriptor.namespace.is_empty() {
            table.insert(name.clone(), Arc::clone(&descriptor));
        }
        table.insert(qualified.clone(), Arc::clone(&descriptor));
        if descriptor.namespace == "std" || descriptor.namespace.is_empty() {
            self.set(&name, Value::NativeFn(Arc::clone(&call)));
        }
        if qualified != name {
            self.set(&qualified, Value::NativeFn(call));
        }
    }

    /// Find the callable contract for a qualified or unqualified name.
    pub fn callable_descriptor(
        &self,
        name: &str,
    ) -> Option<&animatix_core::callable::CallableDescriptor> {
        self.callable_descriptors
            .get(name)
            .map(Arc::as_ref)
            .or_else(|| animatix_std::callable::callable(name))
    }

    /// Copy callable bindings and signatures into a function's isolated local
    /// environment without copying caller variables or actor property values.
    pub(crate) fn inherit_callables_from(&mut self, source: &Environment) {
        self.callable_descriptors = Arc::clone(&source.callable_descriptors);
        self.evaluation_phase = source.evaluation_phase;
        self.component_method_tables = Arc::clone(&source.component_method_tables);
        self.actor_ref_base = source.actor_ref_base.clone().or_else(|| source.base.clone());
        self.active_component_scope = source.active_component_scope.clone();
        let mut callables = Vec::new();
        for (name, value) in &source.overrides {
            if matches!(
                value,
                Value::NativeFn(_) | Value::RegisteredFn { .. } | Value::UserFn { .. }
            ) {
                callables.push((name.clone(), value.clone()));
            }
        }
        for (name, value) in callables {
            self.set_owned(name, value);
        }
    }

    /// Copy only host native functions and their signatures into a clean
    /// module environment, leaving caller variables and user functions out.
    pub(crate) fn inherit_native_callables_from(&mut self, source: &Environment) {
        self.callable_descriptors = Arc::clone(&source.callable_descriptors);
        self.evaluation_phase = source.evaluation_phase;
        let callables = source
            .callable_descriptors
            .iter()
            .filter(|(_, descriptor)| descriptor.namespace != "std")
            .filter_map(|(name, _)| match source.get_ref(name) {
                Some(value @ (Value::NativeFn(_) | Value::RegisteredFn { .. })) => {
                    Some((name.clone(), value.clone()))
                },
                _ => None, // User functions are seeded from their definition module.
            })
            .collect::<Vec<_>>();
        for (name, value) in callables {
            self.set_owned(name, value);
        }
    }

    /// Mark this environment as evaluating a frame-time expression.
    pub(crate) fn set_frame_phase(&mut self) {
        self.evaluation_phase = EvaluationPhase::Frame;
    }

    /// Set the internal frame clock independently of the script-visible `t`.
    pub(crate) fn set_frame_time_seconds(&mut self, seconds: f64) {
        self.frame_time_seconds = Some(seconds);
    }

    /// Whether this environment is currently used by frame-time evaluation.
    pub(crate) fn is_frame_phase(&self) -> bool {
        self.evaluation_phase == EvaluationPhase::Frame
    }

    /// Install a read-only caller-supplied numeric input layer for this frame.
    /// Frame builders pass `None` on the legacy path so scenes without runtime
    /// inputs keep their prior lookup cost.
    pub(crate) fn set_runtime_inputs(
        &mut self,
        values: Option<Arc<std::collections::BTreeMap<String, Value>>>,
    ) {
        self.mark_mutated();
        self.runtime_inputs = values;
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
        self.runtime_inputs = None;
        self.actor_ref_base = None;
        self.frame_time_seconds = None;
        self.authored_t_binding = false;
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
    /// Checks lexical scopes → bindings → runtime inputs → overrides →
    /// actor-only frozen base → general base, in that order.
    pub fn get(&self, name: &str) -> Option<Value> {
        {
            let scopes = self.let_scopes.borrow();
            for scope in scopes.iter().rev() {
                for (k, v) in scope.iter().rev() {
                    if k == name {
                        return Some(v.clone());
                    }
                }
            }
        }
        for binding in self.bindings.iter().flatten() {
            if binding.0 == name {
                return Some(binding.1.clone());
            }
        }
        if let Some(value) = self.runtime_inputs.as_ref().and_then(|inputs| inputs.get(name)) {
            return Some(value.clone());
        }
        self.overrides
            .get(name)
            .cloned()
            .or_else(|| {
                self.actor_ref_base
                    .as_ref()
                    .and_then(|base| base.get(name))
                    .filter(|value| matches!(value, Value::ActorRef(_)))
                    .cloned()
            })
            .or_else(|| self.base.as_ref().and_then(|base| base.get(name).cloned()))
    }

    /// Look up a variable by name, returning a reference (zero-copy).
    /// Checks bindings → runtime inputs → overrides → actor-only frozen base
    /// → general base, in that order.
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
        if let Some(value) = self.runtime_inputs.as_ref().and_then(|inputs| inputs.get(name)) {
            return Some(value);
        }
        self.overrides
            .get(name)
            .or_else(|| {
                self.actor_ref_base
                    .as_ref()
                    .and_then(|base| base.get(name))
                    .filter(|value| matches!(value, Value::ActorRef(_)))
            })
            .or_else(|| self.base.as_ref().and_then(|b| b.get(name)))
    }

    /// Look up a dotted path like `p.x`, first as an injected key and then by
    /// walking `Value::Object` fields.
    pub fn get_path(&self, path: &str) -> Option<Value> {
        self.get_path_with_actor_alias(path, 0)
    }

    fn get_path_with_actor_alias(&self, path: &str, depth: usize) -> Option<Value> {
        if depth > 16 {
            return None;
        }
        if let Some(value) = self.get(path) {
            return Some(value);
        }

        let (first, rest) = path.split_once('.')?;
        let mut current = self.get(first)?;
        if let Value::ActorRef(actor) = &current {
            let actor_path = actor.label();
            if actor_path != first {
                return self.get_path_with_actor_alias(&format!("{actor_path}.{rest}"), depth + 1);
            }
        }
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
        if let Some(inputs) = &self.runtime_inputs {
            keys.extend(inputs.keys().filter(|key| !self.overrides.contains_key(*key)).cloned());
        }
        if let Some(ref base) = self.base {
            for k in base.keys() {
                if !self.overrides.contains_key(k)
                    && !self.runtime_inputs.as_ref().is_some_and(|inputs| inputs.contains_key(k))
                {
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
        let runtime_count = self.runtime_inputs.as_ref().map(|m| m.len()).unwrap_or(0);
        let runtime_overlap = self
            .runtime_inputs
            .as_ref()
            .map_or(0, |inputs| inputs.keys().filter(|k| self.overrides.contains_key(*k)).count());
        let base_overlap = self
            .base
            .as_ref()
            .map(|b| {
                self.overrides.keys().filter(|k| b.contains_key(*k)).count()
                    + inputs_overlap_with_base(self.runtime_inputs.as_deref(), b)
            })
            .unwrap_or(0);
        let triple_overlap = self.base.as_ref().map_or(0, |base| {
            self.runtime_inputs.as_ref().map_or(0, |inputs| {
                inputs
                    .keys()
                    .filter(|key| self.overrides.contains_key(*key) && base.contains_key(*key))
                    .count()
            })
        });
        self.overrides.len() + runtime_count + base_count - runtime_overlap - base_overlap
            + triple_overlap
    }

    /// Returns true if no variables are defined in either layer.
    pub fn is_empty(&self) -> bool {
        self.overrides.is_empty()
            && self.runtime_inputs.as_ref().is_none_or(|inputs| inputs.is_empty())
            && self.base.as_ref().map(|b| b.is_empty()).unwrap_or(true)
    }
}

/// Evaluation stage for expression calls.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum EvaluationPhase {
    /// Build-time evaluation can create and lower timeline plans.
    #[default]
    Build,
    /// Frame-time evaluation is pure and cannot create/execute plans.
    Frame,
}

fn inputs_overlap_with_base(
    inputs: Option<&std::collections::BTreeMap<String, Value>>,
    base: &EnvMap,
) -> usize {
    inputs.map_or(0, |inputs| inputs.keys().filter(|key| base.contains_key(*key)).count())
}
