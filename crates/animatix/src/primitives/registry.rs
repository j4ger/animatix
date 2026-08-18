//! Runtime primitive registry for built-in and extension primitives.

use std::sync::Arc;

use super::{
    ActorCategory, ActorKindId, AssignmentCtx, BuildCtx, ChildProcessing, EvaluateCtx, PRIMITIVES,
    Primitive, RenderCommand, RenderCtx, TextCompileCtx, VectorShapeState, VelloPath,
};
use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::renderer::error::RenderError;
use crate::timeline::{AnimationTrack, Environment, SceneDimensions};

/// A registry that stores built-in and extension primitives in one list.
#[derive(Clone, Default)]
pub struct PrimitiveRegistry {
    primitives: Vec<Arc<dyn Primitive>>,
    builtin_count: usize,
}

impl PrimitiveRegistry {
    /// Create a registry seeded with all built-in primitives.
    pub fn new() -> Self {
        let mut registry = Self::default();
        for primitive in PRIMITIVES {
            registry
                .register(Arc::new(BuiltinPrimitive(*primitive)))
                .expect("built-in primitive names are unique");
        }
        registry.builtin_count = registry.primitives.len();
        registry
    }

    /// Register a primitive through the same path used by extensions.
    pub fn register(
        &mut self,
        primitive: Arc<dyn Primitive>,
    ) -> Result<(), PrimitiveRegistrationError> {
        let name = primitive.type_name();
        if self.find(name).is_some() {
            return Err(PrimitiveRegistrationError::Duplicate(name.to_string()));
        }
        self.primitives.push(primitive);
        Ok(())
    }

    /// Remove a non-built-in registered primitive by type name.
    pub fn remove(&mut self, name: &str) -> bool {
        let Some(index) = self.primitives.iter().position(|p| p.type_name() == name) else {
            return false;
        };
        if index < self.builtin_count {
            return false;
        }
        self.primitives.remove(index);
        true
    }

    /// Look up a primitive by type name.
    pub fn find(&self, name: &str) -> Option<&dyn Primitive> {
        self.primitives
            .iter()
            .find(|primitive| primitive.type_name() == name)
            .map(|primitive| primitive.as_ref())
    }

    /// Iterate all primitives in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Primitive> {
        self.primitives.iter().map(|primitive| primitive.as_ref())
    }

    /// Number of registered primitives.
    pub fn len(&self) -> usize {
        self.primitives.len()
    }

    /// Returns `true` when no primitives are registered.
    pub fn is_empty(&self) -> bool {
        self.primitives.is_empty()
    }

    /// Convert the registry to shared schema specs.
    pub fn specs(&self) -> Vec<animatix_syntax::schema::PrimitiveSpec> {
        self.iter()
            .map(|primitive| {
                let capabilities = primitive.capabilities();
                animatix_syntax::schema::PrimitiveSpec {
                    type_name: primitive.type_name(),
                    display_name: primitive.display_name(),
                    category: match primitive.category() {
                        super::ActorCategory::Shape => {
                            animatix_syntax::schema::PrimitiveCategory::Shape
                        },
                        super::ActorCategory::Text => {
                            animatix_syntax::schema::PrimitiveCategory::Text
                        },
                        super::ActorCategory::Media => {
                            animatix_syntax::schema::PrimitiveCategory::Media
                        },
                        super::ActorCategory::Plot => {
                            animatix_syntax::schema::PrimitiveCategory::Plot
                        },
                        super::ActorCategory::Container => {
                            animatix_syntax::schema::PrimitiveCategory::Container
                        },
                        super::ActorCategory::Annotation => {
                            animatix_syntax::schema::PrimitiveCategory::Annotation
                        },
                    },
                    icon_id: primitive.icon_id(),
                    advanced: primitive.is_advanced(),
                    capabilities: animatix_syntax::schema::PrimitiveCapabilities {
                        text_paths: capabilities.text_paths,
                        vector_paths: capabilities.vector_paths,
                        image_payload: capabilities.image_payload,
                        layout_container: capabilities.layout_container,
                        morphable_paths: capabilities.morphable_paths,
                        vector_reveal_target: capabilities.vector_reveal_target,
                        plot_geometry: capabilities.plot_geometry,
                        is_container: primitive.is_container(),
                        is_shape: primitive.is_shape(),
                    },
                }
            })
            .collect()
    }
}

/// Adapter that lets static built-ins live in the same `Arc<dyn Primitive>`
/// storage as extension primitives.
struct BuiltinPrimitive(&'static dyn Primitive);

impl Primitive for BuiltinPrimitive {
    fn type_name(&self) -> &'static str {
        self.0.type_name()
    }

    fn display_name(&self) -> &'static str {
        self.0.display_name()
    }

    fn category(&self) -> ActorCategory {
        self.0.category()
    }

    fn icon_id(&self) -> &'static str {
        self.0.icon_id()
    }

    fn is_advanced(&self) -> bool {
        self.0.is_advanced()
    }

    fn is_container(&self) -> bool {
        self.0.is_container()
    }

    fn is_shape(&self) -> bool {
        self.0.is_shape()
    }

    fn capabilities(&self) -> animatix_syntax::schema::PrimitiveCapabilities {
        self.0.capabilities()
    }

    fn child_processing(&self) -> ChildProcessing {
        self.0.child_processing()
    }

    fn kind_id(&self) -> ActorKindId {
        self.0.kind_id()
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        self.0.build(ctx, label, props, modifiers, children)
    }

    fn render(&self, ctx: &RenderCtx) -> Option<Vec<VelloPath>> {
        self.0.render(ctx)
    }

    fn apply_defaults(&self, state: &mut VectorShapeState) {
        self.0.apply_defaults(state);
    }

    fn apply_property(
        &self,
        name: &str,
        value: &Expr,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
        state: &mut VectorShapeState,
    ) -> bool {
        self.0.apply_property(name, value, env, diagnostics, subject, state)
    }

    fn finalize_state(&self, state: &mut VectorShapeState) {
        self.0.finalize_state(state);
    }

    fn uses_custom_path(&self) -> bool {
        self.0.uses_custom_path()
    }

    fn exposes_tip_size(&self) -> bool {
        self.0.exposes_tip_size()
    }

    fn supports_fill(&self) -> bool {
        self.0.supports_fill()
    }

    fn default_color_key(&self, property: &str) -> Option<&'static str> {
        self.0.default_color_key(property)
    }

    fn resize_mode(&self) -> crate::timeline::ResizeMode {
        self.0.resize_mode()
    }

    fn default_props(&self, scene_dimensions: &SceneDimensions) -> Vec<Property> {
        self.0.default_props(scene_dimensions)
    }

    fn handle_assignment(
        &self,
        track: &mut AnimationTrack,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<Diagnostic>,
        subject: &str,
    ) -> bool {
        self.0.handle_assignment(track, property, value, ctx, env, diagnostics, subject)
    }

    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[Property],
    ) -> Result<(), Vec<Diagnostic>> {
        self.0.finalize_container_build(ctx, label, props)
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        text_ctx: Option<&mut TextCompileCtx>,
    ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
        self.0.evaluate(ctx, text_ctx)
    }
}

/// Error returned when a primitive cannot be registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimitiveRegistrationError {
    /// A primitive with this type name already exists.
    Duplicate(String),
}

impl std::fmt::Display for PrimitiveRegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Duplicate(name) => write!(f, "primitive '{name}' is already registered"),
        }
    }
}

impl std::error::Error for PrimitiveRegistrationError {}

#[cfg(test)]
mod tests {
    use super::PrimitiveRegistry;
    use crate::ast::{InlineItem, Modifier, Property};
    use crate::diagnostics::Diagnostic;
    use crate::primitives::{
        ActorCategory, ActorKindId, BuildCtx, EvaluateCtx, Primitive, RenderCommand, TextCompileCtx,
    };
    use crate::renderer::error::RenderError;
    use std::collections::HashMap;
    use std::sync::Arc;

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
            _props: &[Property],
            _modifiers: &[Modifier],
            _children: &[InlineItem],
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
            _ctx: &EvaluateCtx,
            _text_ctx: Option<&mut TextCompileCtx>,
        ) -> Result<Option<Vec<RenderCommand>>, RenderError> {
            Ok(Some(vec![RenderCommand::Paths { paths: Vec::new() }]))
        }
    }

    #[test]
    fn registry_layers_custom_primitive_over_builtins() {
        let mut registry = PrimitiveRegistry::new();
        assert!(registry.find("Rect").is_some());
        assert!(registry.find("Gauge").is_none());
        assert!(registry.register(Arc::new(Gauge)).is_ok());
        assert!(registry.find("Gauge").is_some());
        assert_eq!(registry.len(), super::PRIMITIVES.len() + 1);

        let specs = registry.specs();
        assert!(specs.iter().any(|spec| spec.type_name == "Rect"));
        assert!(specs.iter().any(|spec| spec.type_name == "Gauge"));
    }

    #[test]
    fn builtins_and_extensions_share_one_registration_storage() {
        let mut registry = PrimitiveRegistry::new();
        assert_eq!(registry.primitives.len(), super::PRIMITIVES.len());
        assert!(!registry.remove("Rect"), "built-ins must stay registered");
        assert!(registry.register(Arc::new(Gauge)).is_ok());
        assert_eq!(registry.primitives.len(), super::PRIMITIVES.len() + 1);
        assert!(registry.remove("Gauge"));
        assert_eq!(registry.primitives.len(), super::PRIMITIVES.len());
    }

    #[test]
    fn registry_specs_match_shared_schema_for_builtins() {
        let registry_specs = PrimitiveRegistry::new().specs();
        let schema_specs = animatix_syntax::schema::builtin_primitive_specs();
        assert_eq!(
            registry_specs.len(),
            schema_specs.len(),
            "runtime and shared-schema primitive counts drifted"
        );
        for schema in &schema_specs {
            let runtime = registry_specs
                .iter()
                .find(|spec| spec.type_name == schema.type_name)
                .unwrap_or_else(|| panic!("runtime registry is missing {}", schema.type_name));
            assert_eq!(runtime, schema, "shared schema drifted for {}", schema.type_name);
        }
    }

    #[test]
    fn duplicate_primitive_is_rejected() {
        let mut registry = PrimitiveRegistry::new();
        assert_eq!(registry.register(Arc::new(Gauge)), Ok(()));
        assert!(registry.register(Arc::new(Gauge)).is_err());
    }

    #[test]
    fn custom_primitive_builds_through_timeline() {
        let (ast, errors) = animatix_syntax::parser::parse_source("g: Gauge");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut registry = PrimitiveRegistry::new();
        registry.register(Arc::new(Gauge)).expect("register Gauge");
        let report = crate::timeline::Timeline::build_with_primitive_registry(
            &ast,
            &HashMap::new(),
            Arc::new(registry),
        );
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let track = report.output.tracks.get("g").expect("custom actor track");
        assert_eq!(track.kind, ActorKindId::Text);
        assert_eq!(track.actor_type.as_deref(), Some("Gauge"));

        let _scene = report.output.evaluate(0.0, crate::timeline::SceneDimensions::default());
    }
}
