//! Extension context for registering runtime capabilities.
//!
//! This is the cordis-inspired container for a single build or document:
//! primitives, actions, expression functions, and typed services all register
//! through one context. A later plugin loader can construct/dispose contexts
//! around rebuilds and hot reload.

use std::any::Any;
use std::collections::HashMap;
use std::sync::Arc;

use animatix_syntax::schema::{PropertyId, PropertyValueKind};

use crate::primitives::{Primitive, PrimitiveRegistrationError, PrimitiveRegistry};
use crate::timeline::actions::registry::{ActionSignature, BuiltinAction};
use crate::timeline::{Environment, EvalError, Value};

/// A native expression function registered by an extension.
pub type ExtensionFunction =
    dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync;

/// A schema-driven external property.
///
/// The id is allocated by the context and stored in the actor's
/// [`crate::timeline::PropertyPlan`]; frame-time access never resolves the
/// property name by hashing strings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionPropertySpec {
    /// Stable id for this property within the owning context.
    pub id: PropertyId,
    /// Actor type that owns this property, e.g. `Gauge`.
    pub actor_type: String,
    /// Canonical source property name.
    pub name: String,
    /// Finite value kind used by the dynamic track.
    pub kind: PropertyValueKind,
    /// Whether the property is injected into frame environments.
    pub injectable: bool,
}

/// Error returned when an external property cannot be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PropertyRegistrationError {
    /// The property name is already taken by a built-in or context property.
    Duplicate(String),
}

impl std::fmt::Display for PropertyRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(name) => write!(f, "property '{name}' is already registered"),
        }
    }
}

impl std::error::Error for PropertyRegistrationError {}

/// A per-build container of capabilities provided by extensions.
#[derive(Default)]
pub struct ExtensionContext {
    primitives: Arc<PrimitiveRegistry>,
    properties: Vec<ExtensionPropertySpec>,
    next_property_id: u32,
    actions: Vec<Box<dyn BuiltinAction>>,
    functions: Vec<(String, Arc<ExtensionFunction>)>,
    services: HashMap<String, Arc<dyn Any + Send + Sync>>,
}

impl ExtensionContext {
    /// Create a context initialized with the built-in primitive registry.
    pub fn new() -> Self {
        Self {
            next_property_id: 1_000_000,
            ..Self::default()
        }
    }

    /// Register an extension primitive.
    pub fn register_primitive(
        &mut self,
        primitive: Arc<dyn Primitive>,
    ) -> Result<(), PrimitiveRegistrationError> {
        Arc::make_mut(&mut self.primitives).register(primitive)
    }

    /// Return the primitive registry snapshot for this context.
    pub fn primitive_registry(&self) -> Arc<PrimitiveRegistry> {
        Arc::clone(&self.primitives)
    }

    /// Remove a custom primitive registered by this context.
    pub fn remove_primitive(&mut self, name: &str) -> bool {
        Arc::make_mut(&mut self.primitives).remove(name)
    }

    /// Register an external property for an actor type.
    ///
    /// The context allocates a stable `PropertyId` so plugins do not have to
    /// coordinate ids across documents or hot reloads.
    pub fn register_property(
        &mut self,
        actor_type: impl Into<String>,
        name: impl Into<String>,
        kind: PropertyValueKind,
        injectable: bool,
    ) -> Result<PropertyId, PropertyRegistrationError> {
        let actor_type = actor_type.into();
        let name = name.into();
        if crate::timeline::property_registry::property_id(&name).is_some()
            || self
                .properties
                .iter()
                .any(|property| property.actor_type == actor_type && property.name == name)
        {
            return Err(PropertyRegistrationError::Duplicate(format!("{actor_type}.{name}")));
        }
        let id = PropertyId(self.next_property_id);
        self.next_property_id = self.next_property_id.saturating_add(1);
        self.properties.push(ExtensionPropertySpec {
            id,
            actor_type,
            name,
            kind,
            injectable,
        });
        Ok(id)
    }

    /// Remove an external property by actor type and name.
    pub fn remove_property(&mut self, actor_type: &str, name: &str) -> bool {
        let before = self.properties.len();
        self.properties
            .retain(|property| property.actor_type != actor_type || property.name != name);
        self.properties.len() != before
    }

    /// Look up an external property by actor type and name.
    pub fn property_spec(&self, actor_type: &str, name: &str) -> Option<&ExtensionPropertySpec> {
        self.properties
            .iter()
            .find(|property| property.actor_type == actor_type && property.name == name)
    }

    /// Look up an external property by stable id.
    pub fn property_spec_by_id(&self, id: PropertyId) -> Option<&ExtensionPropertySpec> {
        self.properties.iter().find(|property| property.id == id)
    }

    /// Return all external property descriptors in registration order.
    pub fn property_specs(&self) -> &[ExtensionPropertySpec] {
        &self.properties
    }

    /// Register or replace a custom action handler.
    pub fn register_action(&mut self, action: Box<dyn BuiltinAction>) -> &mut Self {
        if let Some(existing) = self
            .actions
            .iter_mut()
            .find(|existing| existing.signature().name == action.signature().name)
        {
            *existing = action;
        } else {
            self.actions.push(action);
        }
        self
    }

    /// Remove a custom action by name.
    pub fn remove_action(&mut self, name: &str) -> bool {
        let before = self.actions.len();
        self.actions.retain(|action| action.signature().name != name);
        self.actions.len() != before
    }

    /// Look up a custom action by name.
    pub fn action(&self, name: &str) -> Option<&dyn BuiltinAction> {
        self.actions
            .iter()
            .find(|action| action.signature().name == name)
            .map(|action| action.as_ref())
    }

    /// Return signatures for custom actions.
    pub fn action_signatures(&self) -> Vec<ActionSignature> {
        self.actions.iter().map(|action| action.signature()).collect()
    }

    /// Register a native expression function.
    pub fn register_function<F>(&mut self, name: impl Into<String>, call: F) -> &mut Self
    where
        F: Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync + 'static,
    {
        let name = name.into();
        if let Some(existing) = self.functions.iter_mut().find(|(existing, _)| *existing == name) {
            existing.1 = Arc::new(call);
        } else {
            self.functions.push((name, Arc::new(call)));
        }
        self
    }

    /// Remove a registered expression function.
    pub fn remove_function(&mut self, name: &str) -> bool {
        let before = self.functions.len();
        self.functions.retain(|(existing, _)| existing != name);
        self.functions.len() != before
    }

    /// Install registered functions into an environment.
    pub fn install_functions(&self, env: &mut Environment) {
        for (name, call) in &self.functions {
            env.set(name, Value::NativeFn(Arc::clone(call)));
        }
    }

    /// Provide a typed service visible to extension handlers.
    pub fn provide<T>(&mut self, name: impl Into<String>, service: T) -> &mut Self
    where
        T: Any + Send + Sync,
    {
        self.services.insert(name.into(), Arc::new(service));
        self
    }

    /// Remove a provided service.
    pub fn remove_service(&mut self, name: &str) -> bool {
        self.services.remove(name).is_some()
    }

    /// Read a typed service.
    pub fn get<T>(&self, name: &str) -> Option<&T>
    where
        T: Any + Send + Sync,
    {
        self.services.get(name).and_then(|service| service.downcast_ref::<T>())
    }

    /// Create a scoped registration guard.
    pub fn scope(&mut self) -> ExtensionScope<'_> {
        ExtensionScope {
            ctx: self,
            primitive_names: Vec::new(),
            property_names: Vec::new(),
            action_names: Vec::new(),
            function_names: Vec::new(),
            service_names: Vec::new(),
            disposed: false,
        }
    }
}

/// A guard that removes all registrations made through it when dropped.
#[must_use]
pub struct ExtensionScope<'a> {
    ctx: &'a mut ExtensionContext,
    primitive_names: Vec<String>,
    property_names: Vec<(String, String)>,
    action_names: Vec<String>,
    function_names: Vec<String>,
    service_names: Vec<String>,
    disposed: bool,
}

impl ExtensionScope<'_> {
    /// Register a primitive and remove it when this scope is disposed.
    pub fn register_primitive(
        &mut self,
        primitive: Arc<dyn Primitive>,
    ) -> Result<(), PrimitiveRegistrationError> {
        let name = primitive.type_name().to_string();
        self.ctx.register_primitive(primitive)?;
        self.primitive_names.push(name);
        Ok(())
    }

    /// Register a property and remove it when this scope is disposed.
    pub fn register_property(
        &mut self,
        actor_type: impl Into<String>,
        name: impl Into<String>,
        kind: PropertyValueKind,
        injectable: bool,
    ) -> Result<PropertyId, PropertyRegistrationError> {
        let actor_type = actor_type.into();
        let name = name.into();
        let id = self.ctx.register_property(actor_type.clone(), name.clone(), kind, injectable)?;
        self.property_names.push((actor_type, name));
        Ok(id)
    }

    /// Register an action and remove it when this scope is disposed.
    pub fn register_action(&mut self, action: Box<dyn BuiltinAction>) {
        let name = action.signature().name.clone();
        self.ctx.register_action(action);
        self.action_names.push(name);
    }

    /// Register a function and remove it when this scope is disposed.
    pub fn register_function<F>(&mut self, name: impl Into<String>, call: F)
    where
        F: Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync + 'static,
    {
        let name = name.into();
        self.ctx.register_function(name.clone(), call);
        self.function_names.push(name);
    }

    /// Provide a service and remove it when this scope is disposed.
    pub fn provide<T>(&mut self, name: impl Into<String>, service: T)
    where
        T: Any + Send + Sync,
    {
        let name = name.into();
        self.ctx.provide(name.clone(), service);
        self.service_names.push(name);
    }

    /// Dispose all registrations in this scope.
    pub fn dispose(mut self) {
        self.dispose_inner();
    }

    fn dispose_inner(&mut self) {
        if self.disposed {
            return;
        }
        for name in self.primitive_names.drain(..) {
            self.ctx.remove_primitive(&name);
        }
        for (actor_type, name) in self.property_names.drain(..) {
            self.ctx.remove_property(&actor_type, &name);
        }
        for name in self.action_names.drain(..) {
            self.ctx.remove_action(&name);
        }
        for name in self.function_names.drain(..) {
            self.ctx.remove_function(&name);
        }
        for name in self.service_names.drain(..) {
            self.ctx.remove_service(&name);
        }
        self.disposed = true;
    }
}

impl Drop for ExtensionScope<'_> {
    fn drop(&mut self) {
        self.dispose_inner();
    }
}

#[cfg(test)]
mod tests {
    use super::ExtensionContext;
    use crate::ast::Action;
    use crate::composition::BuildTarget;
    use crate::diagnostics::Diagnostic;
    use crate::easing::Easing;
    use crate::primitives::{BuildCtx, Primitive};
    use crate::timeline::actions::registry::{ActionSignature, BuiltinAction};
    use crate::timeline::{ActorCategory, ActorKindId, TrackAccessor};
    use crate::timeline::{Timeline, Value};
    use std::sync::Arc;

    struct Marker;

    impl Primitive for Marker {
        fn type_name(&self) -> &'static str {
            "Marker"
        }

        fn display_name(&self) -> &'static str {
            "Marker"
        }

        fn category(&self) -> ActorCategory {
            ActorCategory::Annotation
        }

        fn icon_id(&self) -> &'static str {
            "marker"
        }

        fn kind_id(&self) -> ActorKindId {
            ActorKindId::Text
        }

        fn build(
            &self,
            _ctx: &mut BuildCtx,
            _label: &str,
            _props: &[crate::ast::Property],
            _modifiers: &[crate::ast::Modifier],
            _children: &[crate::ast::InlineItem],
        ) -> Result<(), Vec<Diagnostic>> {
            Ok(())
        }
    }

    struct Gauge;

    impl Primitive for Gauge {
        fn type_name(&self) -> &'static str {
            "Gauge"
        }

        fn display_name(&self) -> &'static str {
            "Gauge"
        }

        fn category(&self) -> ActorCategory {
            ActorCategory::Plot
        }

        fn icon_id(&self) -> &'static str {
            "gauge"
        }

        fn kind_id(&self) -> ActorKindId {
            ActorKindId::Text
        }

        fn build(
            &self,
            ctx: &mut BuildCtx,
            label: &str,
            _props: &[crate::ast::Property],
            _modifiers: &[crate::ast::Modifier],
            _children: &[crate::ast::InlineItem],
        ) -> Result<(), Vec<Diagnostic>> {
            let track = ctx
                .timeline
                .tracks
                .entry(label.to_string())
                .or_insert_with(|| crate::timeline::AnimationTrack::new(label.to_string()));
            track.kind = ActorKindId::Text;
            track.rebuild_property_plan();
            Ok(())
        }

        fn evaluate(
            &self,
            _ctx: &crate::primitives::EvaluateCtx,
            _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
        ) -> Result<
            Option<Vec<crate::primitives::RenderCommand>>,
            crate::renderer::error::RenderError,
        > {
            Ok(Some(vec![crate::primitives::RenderCommand::Paths { paths: Vec::new() }]))
        }
    }

    struct MarkAction;

    impl BuiltinAction for MarkAction {
        fn signature(&self) -> ActionSignature {
            ActionSignature {
                name: "mark".to_string(),
                category: "Custom".to_string(),
                description: "Extension action".to_string(),
                params: vec![],
                modifiers: vec![],
            }
        }

        fn execute(
            &self,
            action: &Action,
            time_ms: f64,
            timeline: &mut Timeline,
            _diagnostics: &mut Vec<Diagnostic>,
        ) {
            for target in &action.targets {
                if let Some(track) = timeline.tracks.get_mut(target) {
                    track.style.opacity.ensure(1.0).add_keyframe(
                        time_ms as u64,
                        0.25,
                        Easing::Linear,
                    );
                }
            }
        }
    }

    #[test]
    fn context_registers_capabilities() {
        let mut ctx = ExtensionContext::new();
        ctx.register_primitive(Arc::new(Marker)).expect("register primitive");
        ctx.register_action(Box::new(MarkAction));
        ctx.register_function("double", |args, _env| {
            let Some(Value::Num(n)) = args.first() else {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "double expects one number".to_string(),
                ));
            };
            Ok(Value::Num(*n * 2.0))
        });
        ctx.provide("threshold", 42_u32);

        assert!(ctx.primitive_registry().find("Marker").is_some());
        assert!(ctx.action("mark").is_some());
        assert_eq!(ctx.get::<u32>("threshold"), Some(&42));

        let mut env = crate::timeline::Environment::new();
        ctx.install_functions(&mut env);
        assert!(matches!(env.get("double"), Some(Value::NativeFn(_))));
    }

    #[test]
    fn scope_disposes_all_registrations_on_drop() {
        let mut ctx = ExtensionContext::new();
        {
            let mut scope = ctx.scope();
            scope.register_primitive(Arc::new(Marker)).expect("register primitive");
            scope.register_action(Box::new(MarkAction));
            scope.register_function("double", |args, _env| {
                let Some(Value::Num(n)) = args.first() else {
                    return Err(crate::timeline::EvalError::TypeMismatch(
                        "double expects one number".to_string(),
                    ));
                };
                Ok(Value::Num(*n * 2.0))
            });
            scope.provide("threshold", 42_u32);
        }

        assert!(ctx.primitive_registry().find("Marker").is_none());
        assert!(ctx.action("mark").is_none());
        assert!(ctx.get::<u32>("threshold").is_none());

        let mut env = crate::timeline::Environment::new();
        ctx.install_functions(&mut env);
        assert!(env.get("double").is_none());
    }

    #[test]
    fn context_can_dispose_registered_capabilities() {
        let mut ctx = ExtensionContext::new();
        ctx.register_primitive(Arc::new(Marker)).expect("register primitive");
        ctx.register_action(Box::new(MarkAction));
        ctx.register_function("double", |args, _env| {
            let Some(Value::Num(n)) = args.first() else {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "double expects one number".to_string(),
                ));
            };
            Ok(Value::Num(*n * 2.0))
        });
        ctx.provide("threshold", 42_u32);

        assert!(ctx.remove_primitive("Marker"));
        assert!(!ctx.remove_primitive("Marker"));
        assert!(ctx.remove_action("mark"));
        assert!(ctx.remove_function("double"));
        assert!(ctx.remove_service("threshold"));
        assert!(ctx.primitive_registry().find("Marker").is_none());
        assert!(ctx.action("mark").is_none());
        assert!(ctx.get::<u32>("threshold").is_none());
    }

    #[test]
    fn build_with_context_installs_extension_functions() {
        let (ast, errors) = animatix_syntax::parser::parse_source("let answer = double(21)");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut ctx = ExtensionContext::new();
        ctx.register_function("double", |args, _env| {
            let Some(Value::Num(n)) = args.first() else {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "double expects one number".to_string(),
                ));
            };
            Ok(Value::Num(*n * 2.0))
        });

        let report =
            Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        assert_eq!(report.output.env.get("answer"), Some(Value::Num(42.0)));
    }

    #[test]
    fn build_target_accepts_extension_context() {
        let (ast, errors) = animatix_syntax::parser::parse_source("let answer = double(21)");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut ctx = ExtensionContext::new();
        ctx.register_function("double", |args, _env| {
            let Some(Value::Num(n)) = args.first() else {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "double expects one number".to_string(),
                ));
            };
            Ok(Value::Num(*n * 2.0))
        });

        let report = BuildTarget::from_ast_with_context(
            &ast,
            &std::collections::HashMap::new(),
            None,
            Arc::new(ctx),
        );
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let BuildTarget::SingleScene(timeline) = &report.output else {
            panic!("expected single-scene build target");
        };
        assert_eq!(timeline.env.get("answer"), Some(Value::Num(42.0)));
    }

    #[test]
    fn build_target_propagates_context_to_composition() {
        let (ast, errors) = animatix_syntax::parser::parse_source("# A\nlet answer = double(21)");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut ctx = ExtensionContext::new();
        ctx.register_function("double", |args, _env| {
            let Some(Value::Num(n)) = args.first() else {
                return Err(crate::timeline::EvalError::TypeMismatch(
                    "double expects one number".to_string(),
                ));
            };
            Ok(Value::Num(*n * 2.0))
        });

        let report = BuildTarget::from_ast_with_context(
            &ast,
            &std::collections::HashMap::new(),
            None,
            Arc::new(ctx),
        );
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let BuildTarget::MultiScene(composition) = &report.output else {
            panic!("expected multi-scene build target");
        };
        let scene = composition.scenes.get("A").expect("scene A");
        assert_eq!(scene.timeline.env.get("answer"), Some(Value::Num(42.0)));
    }

    #[test]
    fn context_registers_and_disposes_properties() {
        use animatix_syntax::schema::PropertyValueKind;

        let mut ctx = ExtensionContext::new();
        let id = ctx
            .register_property("Gauge", "level", PropertyValueKind::F32, true)
            .expect("register property");
        assert_eq!(ctx.property_spec("Gauge", "level").map(|spec| spec.id), Some(id));
        assert!(ctx.property_spec("Gauge", "missing").is_none());
        assert!(ctx.remove_property("Gauge", "level"));
        assert!(!ctx.remove_property("Gauge", "level"));
        assert!(ctx.property_spec("Gauge", "level").is_none());
    }

    #[test]
    fn scope_disposes_registered_properties() {
        use animatix_syntax::schema::PropertyValueKind;

        let mut ctx = ExtensionContext::new();
        {
            let mut scope = ctx.scope();
            scope
                .register_property("Gauge", "level", PropertyValueKind::F32, true)
                .expect("register property");
        }
        assert!(ctx.property_spec("Gauge", "level").is_none());
    }

    #[test]
    fn build_with_context_writes_extension_properties() {
        use animatix_syntax::schema::PropertyValueKind;

        let (ast, errors) = animatix_syntax::parser::parse_source(
            "g: Gauge, level: 42\n#1s\n g.level = 80\nalways { g.level = g.level + 1 }",
        );
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut ctx = ExtensionContext::new();
        ctx.register_primitive(Arc::new(Gauge)).expect("register Gauge");
        ctx.register_property("Gauge", "level", PropertyValueKind::F32, true)
            .expect("register level");

        let report =
            Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );

        let track = report.output.tracks.get("g").expect("gauge track");
        assert_eq!(track.actor_type.as_deref(), Some("Gauge"));
        let level = animatix_syntax::schema::PropertyId(1_000_000);
        assert_eq!(
            track.property_plan.get(level).and_then(|slot| slot.track.sample(0)),
            Some(crate::timeline::PropertyValue::F32(42.0))
        );
        assert_eq!(
            track.property_plan.get(level).and_then(|slot| slot.track.sample(500)),
            Some(crate::timeline::PropertyValue::F32(61.0))
        );
        assert_eq!(
            track.property_plan.get(level).and_then(|slot| slot.track.sample(1000)),
            Some(crate::timeline::PropertyValue::F32(80.0))
        );

        let frame_env = report.output.build_frame_env(
            500,
            crate::timeline::SceneDimensions::default(),
            &std::collections::HashMap::new(),
        );
        assert_eq!(frame_env.get("g.level"), Some(Value::Num(61.0)));
    }

    #[test]
    fn build_with_context_writes_extension_property_on_builtin_actor() {
        use animatix_syntax::schema::PropertyValueKind;

        let (ast, errors) =
            animatix_syntax::parser::parse_source("r: Rect, intensity: 5\n#1s\n r.intensity = 10");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut ctx = ExtensionContext::new();
        ctx.register_property("Rect", "intensity", PropertyValueKind::F32, true)
            .expect("register intensity");

        let report =
            Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let track = report.output.tracks.get("r").expect("rect track");
        assert_eq!(
            track
                .property_plan
                .get(animatix_syntax::schema::PropertyId(1_000_000))
                .and_then(|slot| slot.track.sample(500)),
            Some(crate::timeline::PropertyValue::F32(7.5))
        );
    }

    #[test]
    fn build_with_context_dispatches_custom_actions() {
        let (ast, errors) =
            animatix_syntax::parser::parse_source("target: Rect, size: (10, 10)\n#0s\nmark target");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut ctx = ExtensionContext::new();
        ctx.register_action(Box::new(MarkAction));
        let report =
            Timeline::build_with_context(&ast, &std::collections::HashMap::new(), Arc::new(ctx));
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let track = report.output.tracks.get("target").expect("actor track");
        assert_eq!(track.style.opacity.get(0, 1.0), 0.25);
    }
}
