//! Inert animation-plan values produced by ordinary functions and timeline
//! expressions. Plans are build-time data: they are expanded into the host
//! timeline's existing ECS stores and modifier programs before evaluation.

use std::sync::Arc;

use animatix_syntax::ast::Stmt;
use animatix_syntax::easing::Easing;
use animatix_syntax::visit::{AstVisitor, walk_expr, walk_stmt};

use crate::timeline::env::{CapturedEnv, Value};

/// A time value in canonical seconds.
#[derive(Clone, Copy, Debug, Default, PartialEq, PartialOrd)]
pub struct PlanDuration(f64);

impl PlanDuration {
    /// Construct a duration from seconds, rejecting negative and non-finite
    /// values at the boundary where they enter a plan.
    pub fn from_seconds(seconds: f64) -> Result<Self, PlanValueError> {
        if seconds.is_finite() && seconds >= 0.0 {
            Ok(Self(seconds))
        } else {
            Err(PlanValueError::InvalidDuration(seconds))
        }
    }

    /// Return this duration in seconds.
    pub fn seconds(self) -> f64 {
        self.0
    }

    /// Return this duration in milliseconds.
    pub fn milliseconds(self) -> f64 {
        self.0 * 1000.0
    }
}

/// A reference to an actor used by a plan operation.
///
/// Caller references resolve against the host timeline. Plan-local references
/// are rewritten to a fresh host label every time the owning plan is mounted.
#[derive(Clone)]
pub struct ActorRef {
    label: Arc<str>,
    scope: ActorRefScope,
    /// World identity resolved during the build, when this reference names an
    /// actor already mounted in the host timeline.
    entity_id: Option<crate::timeline::EntityId>,
    /// Immutable component-method table attached to `entity_id` in World.
    method_table_id: Option<crate::timeline::world::MethodTableId>,
    /// Build-resolved child handles for member access in frame expressions.
    children: Option<Arc<std::collections::HashMap<String, ActorRef>>>,
    /// Build-resolved indexed targets for actor-array access in frame expressions.
    array_candidates: Option<Arc<[(usize, ActorRef)]>>,
}

impl std::fmt::Debug for ActorRef {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let children_len = self.children.as_ref().map(|children| children.len());
        let array_candidates_len =
            self.array_candidates.as_ref().map(|candidates| candidates.len());
        formatter
            .debug_struct("ActorRef")
            .field("label", &self.label)
            .field("scope", &self.scope)
            .field("entity_id", &self.entity_id)
            .field("method_table_id", &self.method_table_id)
            .field("children_len", &children_len)
            .field("array_candidates_len", &array_candidates_len)
            .finish()
    }
}

impl PartialEq for ActorRef {
    fn eq(&self, other: &Self) -> bool {
        self.label == other.label && self.scope == other.scope
    }
}

impl Eq for ActorRef {}

impl std::hash::Hash for ActorRef {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::hash::Hash::hash(&self.label, state);
        std::hash::Hash::hash(&self.scope, state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ActorRefScope {
    Caller,
    Local(u64),
    /// Already bound to a host placement; its id is not a blueprint id.
    Mounted(u64),
}

impl ActorRef {
    /// Reference an actor already owned by the caller's timeline.
    pub fn caller(label: impl Into<Arc<str>>) -> Self {
        Self {
            label: label.into(),
            scope: ActorRefScope::Caller,
            entity_id: None,
            method_table_id: None,
            children: None,
            array_candidates: None,
        }
    }

    /// Reference an actor symbol declared in a plan value.
    pub(crate) fn local(plan_id: u64, label: impl Into<Arc<str>>) -> Self {
        Self {
            label: label.into(),
            scope: ActorRefScope::Local(plan_id),
            entity_id: None,
            method_table_id: None,
            children: None,
            array_candidates: None,
        }
    }

    pub(crate) fn mounted(mount_id: u64, label: impl Into<Arc<str>>) -> Self {
        Self {
            label: label.into(),
            scope: ActorRefScope::Mounted(mount_id),
            entity_id: None,
            method_table_id: None,
            children: None,
            array_candidates: None,
        }
    }

    pub(crate) fn resolved_array(
        label: impl Into<Arc<str>>,
        candidates: Vec<(usize, ActorRef)>,
    ) -> Self {
        Self {
            label: label.into(),
            scope: ActorRefScope::Caller,
            entity_id: None,
            method_table_id: None,
            children: None,
            array_candidates: Some(Arc::from(candidates)),
        }
    }

    pub(crate) fn with_children(
        &self,
        children: std::collections::HashMap<String, ActorRef>,
    ) -> Self {
        Self {
            label: Arc::clone(&self.label),
            scope: self.scope.clone(),
            entity_id: self.entity_id,
            method_table_id: self.method_table_id,
            children: Some(Arc::new(children)),
            array_candidates: self.array_candidates.clone(),
        }
    }

    /// Return the source label for diagnostics and lowering.
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Whether this reference names an actor supplied by the caller.
    pub fn is_caller(&self) -> bool {
        matches!(self.scope, ActorRefScope::Caller)
    }

    /// World identity resolved before frame evaluation.
    pub(crate) fn entity_id(&self) -> Option<crate::timeline::EntityId> {
        self.entity_id
    }

    /// Immutable component-method table handle resolved before frame evaluation.
    pub(crate) fn method_table_id(&self) -> Option<crate::timeline::world::MethodTableId> {
        self.method_table_id
    }

    pub(crate) fn with_world_identity(
        &self,
        entity_id: crate::timeline::EntityId,
        method_table_id: Option<crate::timeline::world::MethodTableId>,
    ) -> Self {
        Self {
            label: Arc::clone(&self.label),
            scope: self.scope.clone(),
            entity_id: Some(entity_id),
            method_table_id,
            children: self.children.clone(),
            array_candidates: self.array_candidates.clone(),
        }
    }

    /// Return the concrete array member represented by an indexed actor
    /// expression such as `bars[i]`.
    pub(crate) fn indexed(&self, index: usize) -> Self {
        if let Some(reference) = self.array_candidate(index) {
            return reference.clone();
        }
        Self {
            label: Arc::from(crate::ast::array_actor_label(&self.label, index)),
            scope: self.scope.clone(),
            // A symbolic array reference is resolved to its concrete entity
            // before frame evaluation; never inherit the base actor's handle.
            entity_id: None,
            method_table_id: None,
            children: None,
            array_candidates: None,
        }
    }

    pub(crate) fn indexed_resolved(&self, index: usize) -> Option<Self> {
        self.array_candidate(index).cloned()
    }

    fn array_candidate(&self, index: usize) -> Option<&Self> {
        let candidates = self.array_candidates.as_ref()?;
        let position = candidates.binary_search_by_key(&index, |(candidate, _)| *candidate).ok()?;
        Some(&candidates[position].1)
    }

    /// Reference a child actor while keeping the owning plan's identity.
    pub(crate) fn child(&self, name: &str) -> Self {
        if let Some(reference) = self.children.as_ref().and_then(|children| children.get(name)) {
            return reference.clone();
        }
        Self {
            label: Arc::from(format!("{}.{name}", self.label)),
            scope: self.scope.clone(),
            // Child identity is supplied by the build-resolved child symbol.
            entity_id: None,
            method_table_id: None,
            children: None,
            array_candidates: None,
        }
    }

    pub(crate) fn child_resolved(&self, name: &str) -> Option<Self> {
        self.children.as_ref()?.get(name).cloned()
    }

    pub(crate) fn local_plan_id(&self) -> Option<u64> {
        match self.scope {
            ActorRefScope::Caller | ActorRefScope::Mounted(_) => None,
            ActorRefScope::Local(id) => Some(id),
        }
    }
}

/// Typed, inert operations which can be appended by native timeline
/// callables. They contain values and references, never a live `Timeline` or
/// an entity mutation.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanOp {
    /// A built-in animation callable, bound to its typed source parameters.
    Builtin(BoundBuiltinCall),
    /// A property track write relative to this plan's local clock.
    Property {
        /// Actor identity, resolved to a concrete entity when mounted.
        actor: ActorRef,
        /// Registered property name.
        property: String,
        /// Validated target value.
        value: Value,
        /// Offset from the plan's local zero.
        start: PlanDuration,
        /// Interpolation length in the plan's clock.
        duration: PlanDuration,
        /// Explicit interpolation curve, when supplied.
        easing: Option<Easing>,
    },
    /// A source-authored body whose expressions and declarations are lowered
    /// only when the plan is placed in a host timeline.
    Statements(Arc<[Stmt]>),
    /// Sequentially composed child plans.
    Sequence(Arc<[Arc<TimelinePlan>]>),
    /// Child plans offset by a fixed interval in authored order.
    Stagger {
        /// Offset between consecutive child starts.
        interval: PlanDuration,
        /// Children in authored order.
        children: Arc<[Arc<TimelinePlan>]>,
    },
    /// Apply a local clock transform to a child plan.
    Retimed {
        /// Child whose offsets are interpreted in local time.
        plan: Arc<TimelinePlan>,
        /// Multiplier from local duration to parent duration.
        factor: f64,
        /// Optional requested wall-clock extent. When set, it takes priority
        /// over `factor` after the child's extent has been resolved.
        fit_duration: Option<PlanDuration>,
    },
}

/// A call to a standard timeline callable after named/positional binding.
///
/// This is deliberately separate from the parser AST. `args` contains values
/// in descriptor parameter order, including `ActorRef` and `PlanDuration`.
#[derive(Clone, Debug, PartialEq)]
pub struct BoundBuiltinCall {
    /// Canonical callable name from the standard catalog.
    pub name: String,
    /// Bound parameter names and values, in descriptor order.
    pub args: Vec<(String, Value)>,
    /// Source-facing call span, when available.
    pub byte_span: Option<animatix_syntax::ast::ByteSpan>,
}

/// A reusable, inert animation plan value.
#[derive(Clone, Debug)]
pub struct TimelinePlan {
    id: u64,
    ops: Arc<[PlanOp]>,
    captures: CapturedEnv,
    local_roots: Arc<[String]>,
    /// Source module used to resolve relative assets in this plan.
    pub(crate) source_id: Option<Arc<std::path::PathBuf>>,
    /// Source callable definitions needed by component methods captured here.
    /// This remains compiler metadata and is never stored in an ECS component.
    function_context: Option<Arc<crate::module::FunctionContext>>,
    /// Component methods declared in this particular plan body.
    instance_methods: Arc<crate::module::InstanceFnRegistry>,
    /// Whether an authored lexical binding supplied `t` when this value was
    /// created. Free `t` references otherwise use the plan's local clock.
    authored_t_capture: bool,
    /// Static authored extent in seconds, when the body is measurable.
    extent: Option<PlanDuration>,
}

impl PartialEq for TimelinePlan {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl TimelinePlan {
    /// Construct a plan from source statements and a lexical capture snapshot.
    pub fn from_statements(body: Vec<Stmt>, captures: CapturedEnv) -> Self {
        let id = next_plan_id();
        let local_roots = collect_root_actor_labels(&body);
        Self {
            id,
            ops: Arc::from([PlanOp::Statements(Arc::from(body))]),
            captures,
            local_roots: Arc::from(local_roots),
            source_id: None,
            function_context: None,
            instance_methods: Arc::new(Default::default()),
            authored_t_capture: false,
            extent: None,
        }
    }

    /// Capture a source-authored expression body and its component methods.
    pub(crate) fn capture_plan_body(
        plan_body: crate::ast::PlanBody,
        env: &crate::timeline::Environment,
    ) -> Self {
        Self::capture_statements_with_methods(plan_body.statements, plan_body.instance_methods, env)
    }

    fn capture_statements_with_methods(
        body: Vec<Stmt>,
        instance_methods: crate::module::InstanceFnRegistry,
        env: &crate::timeline::Environment,
    ) -> Self {
        let id = next_plan_id();
        let capture_env = env.clone();
        capture_env.push_let_scope([]);
        let mut labels = Vec::new();
        collect_actor_labels(&body, &mut labels);
        labels.sort();
        labels.dedup();
        for label in labels {
            capture_env
                .set_let_binding(&label, Value::ActorRef(ActorRef::local(id, label.clone())));
        }
        let mut captures = std::collections::HashMap::new();
        let mut referenced = referenced_statement_names(&body);
        referenced.extend(assignment_actor_roots(&body));
        // Capture available lexical values conservatively. Inner declarations
        // shadow them when their scopes execute; collecting all inner bound
        // names into one set would incorrectly erase an outer reference (for
        // example an actor parameter reused as a closure parameter).
        for name in referenced {
            if name.contains('.')
                && let Some(root) = name.split('.').next()
                && matches!(capture_env.get(root), Some(Value::ActorRef(_)))
            {
                // Dotted actor-property reads stay live in the frame VM; only
                // the caller ActorRef itself belongs in the lexical capture.
                continue;
            }
            if capture_env.callable_descriptor(&name).is_some()
                && matches!(
                    capture_env.get(&name),
                    Some(Value::NativeFn(_) | Value::RegisteredFn { .. })
                )
            {
                continue;
            }
            if let Some(value) = capture_env.get(&name) {
                captures.insert(name, value);
            }
        }
        let local_roots = collect_root_actor_labels(&body);
        let extent = crate::timeline::plan_lowerer::PlanExpansionGuard::enter()
            .and_then(|_depth| {
                crate::timeline::plan_lowerer::estimate_statements_extent_ms(&body, &capture_env)
            })
            .and_then(|ms| PlanDuration::from_seconds(ms / 1000.0).ok());
        capture_env.pop_let_scope();
        Self {
            id,
            ops: Arc::from([PlanOp::Statements(Arc::from(body))]),
            captures: CapturedEnv(captures),
            local_roots: Arc::from(local_roots),
            source_id: env.source_path().map(|path| Arc::new(path.to_path_buf())),
            function_context: env.function_context.clone(),
            instance_methods: Arc::new(instance_methods),
            authored_t_capture: env.has_authored_t_binding(),
            extent,
        }
    }

    /// Construct a plan from already typed operations.
    pub fn from_ops(ops: Vec<PlanOp>) -> Self {
        let id = next_plan_id();
        let extent = infer_ops_extent(&ops);
        Self {
            id,
            ops: Arc::from(ops),
            captures: CapturedEnv::default(),
            local_roots: Arc::from([]),
            source_id: None,
            function_context: None,
            instance_methods: Arc::new(Default::default()),
            authored_t_capture: false,
            extent,
        }
    }

    /// Construct an empty plan builder.
    pub fn builder() -> TimelinePlanBuilder {
        TimelinePlanBuilder::default()
    }

    /// Stable identity used to scope plan-local actor references.
    pub fn id(&self) -> u64 {
        self.id
    }

    /// The inert operations making up this plan.
    pub fn ops(&self) -> &[PlanOp] {
        &self.ops
    }

    /// Lexical values captured when this plan was constructed.
    pub fn captures(&self) -> &CapturedEnv {
        &self.captures
    }

    /// Whether a source binding supplied `t` when the plan was captured.
    pub(crate) fn has_authored_t_capture(&self) -> bool {
        self.authored_t_capture
    }

    /// Root labels declared in this plan and made private per placement.
    pub(crate) fn local_actor_roots(&self) -> &[String] {
        &self.local_roots
    }

    /// Authored extent, if statically measurable.
    pub fn extent(&self) -> Option<PlanDuration> {
        self.extent
    }

    /// Component method registry attached to this source plan.
    pub(crate) fn instance_methods(&self) -> &crate::module::InstanceFnRegistry {
        &self.instance_methods
    }

    /// Function definitions visible in the source modules that declared the
    /// plan's component methods.
    pub(crate) fn function_context(&self) -> Option<&Arc<crate::module::FunctionContext>> {
        self.function_context.as_ref()
    }

    /// Return a new plan that plays its content at a constant local-clock
    /// rate. The plan's local actor identity and lexical captures are retained.
    pub fn stretched(&self, factor: f64) -> Result<Self, PlanValueError> {
        if !factor.is_finite() || factor <= 0.0 {
            return Err(PlanValueError::InvalidScale(factor));
        }
        Ok(Self {
            id: self.id,
            ops: Arc::from([PlanOp::Retimed {
                plan: Arc::new(self.clone()),
                factor,
                fit_duration: None,
            }]),
            captures: self.captures.clone(),
            local_roots: self.local_roots.clone(),
            source_id: self.source_id.clone(),
            function_context: self.function_context.clone(),
            instance_methods: Arc::clone(&self.instance_methods),
            authored_t_capture: self.authored_t_capture,
            extent: self.extent.map(|extent| {
                PlanDuration::from_seconds(extent.seconds() * factor)
                    .expect("validated finite positive scale")
            }),
        })
    }

    /// Return a new plan fitted to the requested wall-clock duration.
    pub fn fit_duration(&self, duration: PlanDuration) -> Result<Self, PlanValueError> {
        let Some(extent) = self.extent.filter(|extent| extent.seconds() > 0.0) else {
            return Err(PlanValueError::UnmeasurableExtent);
        };
        let factor = duration.seconds() / extent.seconds();
        if !factor.is_finite() || factor <= 0.0 {
            return Err(PlanValueError::InvalidScale(factor));
        }
        Ok(Self {
            id: self.id,
            ops: Arc::from([PlanOp::Retimed {
                plan: Arc::new(self.clone()),
                factor,
                fit_duration: Some(duration),
            }]),
            captures: self.captures.clone(),
            local_roots: self.local_roots.clone(),
            source_id: self.source_id.clone(),
            function_context: self.function_context.clone(),
            instance_methods: Arc::clone(&self.instance_methods),
            authored_t_capture: self.authored_t_capture,
            extent: Some(duration),
        })
    }
}

/// Native-callable writer for building inert timeline plans.
pub struct TimelinePlanBuilder {
    id: u64,
    ops: Vec<PlanOp>,
}

impl Default for TimelinePlanBuilder {
    fn default() -> Self {
        Self {
            id: next_plan_id(),
            ops: Vec::new(),
        }
    }
}

impl TimelinePlanBuilder {
    /// Create a reference to an actor symbol owned by this plan.
    pub fn local_actor(&self, label: impl Into<Arc<str>>) -> ActorRef {
        ActorRef::local(self.id, label)
    }

    /// Append a typed built-in invocation.
    pub fn append_builtin(&mut self, call: BoundBuiltinCall) {
        self.ops.push(PlanOp::Builtin(call));
    }

    /// Append a typed property operation at plan-local time.
    pub fn append_property_op(
        &mut self,
        actor: ActorRef,
        property: impl Into<String>,
        value: Value,
        start: PlanDuration,
        duration: PlanDuration,
        easing: Option<Easing>,
    ) {
        self.ops.push(PlanOp::Property {
            actor,
            property: property.into(),
            value,
            start,
            duration,
            easing,
        });
    }

    /// Append a source-authored body without executing it.
    pub fn append_statements(&mut self, body: Vec<Stmt>) {
        self.ops.push(PlanOp::Statements(Arc::from(body)));
    }

    /// Append child plans in sequence order.
    pub fn append_sequence(&mut self, children: Vec<Arc<TimelinePlan>>) {
        self.ops.push(PlanOp::Sequence(Arc::from(children)));
    }

    /// Append child plans with a fixed stagger interval.
    pub fn append_stagger(&mut self, interval: PlanDuration, children: Vec<Arc<TimelinePlan>>) {
        self.ops.push(PlanOp::Stagger {
            interval,
            children: Arc::from(children),
        });
    }

    /// Finish building an inert plan value.
    pub fn finish(self) -> TimelinePlan {
        let extent = infer_ops_extent(&self.ops);
        TimelinePlan {
            id: self.id,
            ops: Arc::from(self.ops),
            captures: CapturedEnv::default(),
            local_roots: Arc::from([]),
            source_id: None,
            function_context: None,
            instance_methods: Arc::new(Default::default()),
            authored_t_capture: false,
            extent,
        }
    }
}

pub(crate) fn referenced_statement_names(body: &[Stmt]) -> std::collections::HashSet<String> {
    struct References(std::collections::HashSet<String>);
    impl AstVisitor<'_> for References {
        fn visit_expr(&mut self, expression: &animatix_syntax::ast::Expr) {
            use animatix_syntax::ast::Expr;
            match expression {
                Expr::Ident(name) => {
                    self.0.insert(name.clone());
                },
                Expr::Path(parts) => {
                    for end in 1..=parts.len() {
                        self.0.insert(parts[..end].join("."));
                    }
                },
                Expr::QualifiedPath(parts) => {
                    self.0.insert(parts.join("::"));
                },
                _ => {},
            }
            walk_expr(self, expression);
        }
    }
    let mut references = References(std::collections::HashSet::new());
    for statement in body {
        references.visit_stmt(statement);
    }
    references.0
}

fn assignment_actor_roots(body: &[Stmt]) -> std::collections::HashSet<String> {
    struct Targets(std::collections::HashSet<String>);
    impl AstVisitor<'_> for Targets {
        fn visit_stmt(&mut self, statement: &Stmt) {
            let target: &[crate::ast::TargetSegment] = match statement {
                Stmt::Assignment { target, .. } | Stmt::ReactiveBinding { target, .. } => target,
                _ => &[],
            };
            if let Some(segment) = target.first() {
                let root = match segment {
                    crate::ast::TargetSegment::Static(label) => label,
                    crate::ast::TargetSegment::Indexed { base, .. } => base,
                };
                self.0.insert(root.clone());
            }
            walk_stmt(self, statement);
        }
    }
    let mut targets = Targets(std::collections::HashSet::new());
    for statement in body {
        targets.visit_stmt(statement);
    }
    targets.0
}

fn infer_ops_extent(ops: &[PlanOp]) -> Option<PlanDuration> {
    let mut extent = 0.0f64;
    for op in ops {
        let op_extent = match op {
            PlanOp::Builtin(call) => {
                let mut duration = 0.0;
                let mut delay = 0.0;
                for (name, value) in &call.args {
                    match (name.as_str(), value) {
                        ("duration", Value::Duration(seconds)) => duration = *seconds,
                        ("delay", Value::Duration(seconds)) => delay = *seconds,
                        _ => {},
                    }
                }
                Some((duration + delay) * 1000.0)
            },
            PlanOp::Property {
                start, duration, ..
            } => Some(start.milliseconds() + duration.milliseconds()),
            PlanOp::Statements(_) => None,
            PlanOp::Sequence(children) => children
                .iter()
                .map(|child| child.extent.map(PlanDuration::milliseconds))
                .collect::<Option<Vec<_>>>()
                .map(|spans| spans.into_iter().sum()),
            PlanOp::Stagger { interval, children } => {
                let mut end = 0.0f64;
                for (index, child) in children.iter().enumerate() {
                    let child_span = child.extent?.milliseconds();
                    end = end.max(index as f64 * interval.milliseconds() + child_span);
                }
                Some(end)
            },
            PlanOp::Retimed {
                plan,
                factor,
                fit_duration,
            } => fit_duration
                .map(PlanDuration::milliseconds)
                .or_else(|| plan.extent.map(|extent| extent.milliseconds() * factor)),
        }?;
        extent = extent.max(op_extent);
    }
    PlanDuration::from_seconds(extent / 1000.0).ok()
}

/// Errors raised while constructing typed plan values.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanValueError {
    /// A duration was negative or non-finite.
    InvalidDuration(f64),
    /// A time-scale factor was non-finite or non-positive.
    InvalidScale(f64),
    /// A plan-local ActorRef was used with a different plan identity.
    ForeignPlanActorRef {
        /// Identity of the receiving plan.
        expected: u64,
        /// Identity stored in the supplied reference.
        actual: u64,
        /// Source label for the diagnostic.
        label: String,
    },
    /// A plan-local ActorRef was used as a caller reference.
    LocalActorRefEscaped {
        /// Source label of the escaping reference.
        label: String,
    },
    /// Requested retiming needs a measurable, positive plan extent.
    UnmeasurableExtent,
}

impl std::fmt::Display for PlanValueError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDuration(value) => write!(f, "invalid plan duration {value}"),
            Self::InvalidScale(value) => write!(f, "invalid plan time scale {value}"),
            Self::ForeignPlanActorRef {
                expected,
                actual,
                label,
            } => {
                write!(f, "actor reference '{label}' belongs to plan {actual}, not plan {expected}")
            },
            Self::LocalActorRefEscaped { label } => {
                write!(f, "plan-local actor reference '{label}' escaped its plan")
            },
            Self::UnmeasurableExtent => {
                write!(f, "plan duration cannot be fitted without a positive finite extent")
            },
        }
    }
}

fn collect_root_actor_labels(statements: &[Stmt]) -> Vec<String> {
    let mut labels = Vec::new();
    collect_actor_labels(statements, &mut labels);
    let mut roots = labels
        .into_iter()
        .map(|label| label.split('.').next().unwrap_or(&label).to_string())
        .collect::<Vec<_>>();
    roots.sort();
    roots.dedup();
    roots
}

fn collect_actor_labels(statements: &[Stmt], output: &mut Vec<String>) {
    fn collect_inline(items: &[crate::ast::InlineItem], output: &mut Vec<String>) {
        for item in items {
            match item {
                crate::ast::InlineItem::Labeled {
                    label, children, ..
                } => {
                    // Ordinary child labels already have their engine identity;
                    // component expansion supplies its own qualified prefixes.
                    output.push(label.clone());
                    collect_inline(children, output);
                },
                crate::ast::InlineItem::Anonymous { children, .. }
                | crate::ast::InlineItem::SlotFill {
                    items: children, ..
                }
                | crate::ast::InlineItem::ForLoop { body: children, .. } => {
                    collect_inline(children, output);
                },
                crate::ast::InlineItem::SlotMarker | crate::ast::InlineItem::Expr(..) => {},
            }
        }
    }

    for statement in statements {
        match statement {
            Stmt::ActorDecl {
                label, children, ..
            } => {
                output.push(label.clone());
                collect_inline(children, output);
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::ForLoop { body, .. }
            | Stmt::Block { body, .. }
            | Stmt::FnDecl { body, .. }
            | Stmt::Scene { body, .. } => collect_actor_labels(body, output),
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                collect_actor_labels(then_branch, output);
                if let Some(else_branch) = else_branch {
                    collect_actor_labels(else_branch, output);
                }
            },
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    collect_actor_labels(body, output);
                }
            },
            Stmt::Expr(crate::ast::Expr::Timeline(plan), ..)
            | Stmt::Expr(crate::ast::Expr::Sequence(plan), ..) => {
                collect_actor_labels(&plan.statements, output)
            },
            Stmt::Expr(crate::ast::Expr::Stagger { plan, .. }, ..) => {
                collect_actor_labels(&plan.statements, output)
            },
            _ => {},
        }
    }
}

impl std::error::Error for PlanValueError {}

fn next_plan_id() -> u64 {
    static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[cfg(test)]
mod actor_ref_debug_tests {
    use super::*;

    #[test]
    fn actor_ref_debug_reports_identity_without_expanding_children() {
        let actor = ActorRef::resolved_array("items", vec![(0, ActorRef::caller("nested-child"))])
            .with_world_identity(
                crate::timeline::EntityId(42),
                Some(crate::timeline::world::MethodTableId::new(7)),
            )
            .with_children(std::collections::HashMap::from([(
                "child".to_string(),
                ActorRef::caller("nested-child"),
            )]));

        let debug = format!("{actor:?}");
        assert!(debug.contains("items"));
        assert!(debug.contains("Caller"));
        assert!(debug.contains("EntityId(42)"));
        assert!(debug.contains("MethodTableId(7)"));
        assert!(debug.contains("children_len: Some(1)"));
        assert!(debug.contains("array_candidates_len: Some(1)"));
        assert!(!debug.contains("nested-child"));
    }
}
