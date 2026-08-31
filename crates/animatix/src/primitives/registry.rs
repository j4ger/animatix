//! Runtime primitive registry for built-in and extension primitives.

use std::sync::Arc;

use super::{PRIMITIVES, Primitive};

/// Storage for a registered primitive: compiled-in built-ins keep their
/// `&'static` identity, extension/plugin primitives own an `Arc` allocation.
#[derive(Clone)]
enum RegisteredPrimitive {
    /// A built-in primitive from [`PRIMITIVES`].
    Builtin(&'static dyn Primitive),
    /// A runtime-registered (extension / plugin) primitive.
    Extension(Arc<dyn Primitive>),
}

impl RegisteredPrimitive {
    fn as_ref(&self) -> &dyn Primitive {
        match self {
            Self::Builtin(primitive) => *primitive,
            Self::Extension(primitive) => primitive.as_ref(),
        }
    }
}

/// A registry that stores built-in and extension primitives in one list.
#[derive(Clone, Default)]
pub struct PrimitiveRegistry {
    primitives: Vec<RegisteredPrimitive>,
}

impl PrimitiveRegistry {
    /// Create a registry seeded with all built-in primitives.
    pub fn new() -> Self {
        let mut registry = Self::default();
        for primitive in PRIMITIVES {
            registry.primitives.push(RegisteredPrimitive::Builtin(*primitive));
        }
        registry
    }

    /// Register a runtime (extension/plugin) primitive.
    pub fn register(
        &mut self,
        primitive: Arc<dyn Primitive>,
    ) -> Result<(), PrimitiveRegistrationError> {
        let name = primitive.type_name();
        if self.find(name).is_some() {
            return Err(PrimitiveRegistrationError::Duplicate(name.to_string()));
        }
        self.primitives.push(RegisteredPrimitive::Extension(primitive));
        Ok(())
    }

    /// Remove a non-built-in registered primitive by type name.
    pub fn remove(&mut self, name: &str) -> bool {
        let Some(index) = self
            .primitives
            .iter()
            .position(|registered| registered.as_ref().type_name() == name)
        else {
            return false;
        };
        if matches!(&self.primitives[index], RegisteredPrimitive::Builtin(_)) {
            return false;
        }
        self.primitives.remove(index);
        true
    }

    /// Look up a primitive by type name.
    pub fn find(&self, name: &str) -> Option<&dyn Primitive> {
        self.primitives
            .iter()
            .find(|registered| registered.as_ref().type_name() == name)
            .map(RegisteredPrimitive::as_ref)
    }

    /// Return whether `name` belongs to the built-in prefix of this registry.
    pub fn is_builtin(&self, name: &str) -> bool {
        self.primitives.iter().any(|registered| {
            matches!(registered, RegisteredPrimitive::Builtin(_))
                && registered.as_ref().type_name() == name
        })
    }

    /// Iterate all primitives in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn Primitive> {
        self.primitives.iter().map(RegisteredPrimitive::as_ref)
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
                    type_name: primitive.type_name().to_string(),
                    display_name: primitive.display_name().to_string(),
                    category: super::actor_category_to_primitive_category(primitive.category()),
                    icon_id: primitive.icon_id().to_string(),
                    advanced: primitive.is_advanced(),
                    capabilities: animatix_syntax::schema::PrimitiveCapabilities {
                        text_paths: capabilities.text_paths,
                        vector_paths: capabilities.vector_paths,
                        image_payload: capabilities.image_payload,
                        layout_container: capabilities.layout_container,
                        morphable_paths: capabilities.morphable_paths,
                        vector_reveal_target: capabilities.vector_reveal_target,
                        plot_geometry: capabilities.plot_geometry,
                        plot_host: capabilities.plot_host,
                        is_container: primitive.is_container(),
                        is_shape: primitive.is_shape(),
                    },
                    child_processing: primitive.child_processing(),
                }
            })
            .collect()
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
        fn type_name(&self) -> &str {
            "Gauge"
        }

        fn display_name(&self) -> &str {
            "Gauge"
        }

        fn category(&self) -> ActorCategory {
            ActorCategory::Plot
        }

        fn icon_id(&self) -> &str {
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
        assert!(registry.is_builtin("Rect"));
        assert!(registry.find("Gauge").is_none());
        assert!(!registry.is_builtin("Gauge"));
        assert!(registry.register(Arc::new(Gauge)).is_ok());
        assert!(registry.find("Gauge").is_some());
        assert!(!registry.is_builtin("Gauge"));
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
