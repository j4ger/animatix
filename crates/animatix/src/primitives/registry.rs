//! Runtime primitive registry for built-in and extension primitives.

use std::sync::Arc;

use super::{PRIMITIVES, Primitive};
use animatix_std::{PrimitiveInfo, catalog_lookup};

/// Storage for a registered primitive: compiled-in built-ins keep their
/// `&'static` identity (metadata from the `animatix-std` catalog), extension
/// primitives own an `Arc` allocation plus the registration info.
#[derive(Clone)]
enum RegisteredPrimitive {
    /// A built-in primitive from [`PRIMITIVES`].
    Builtin(&'static dyn Primitive),
    /// A runtime-registered (extension / plugin) primitive.
    Extension(Arc<dyn Primitive>, PrimitiveInfo),
}

impl RegisteredPrimitive {
    fn as_ref(&self) -> &dyn Primitive {
        match self {
            Self::Builtin(primitive) => *primitive,
            Self::Extension(primitive, _) => primitive.as_ref(),
        }
    }

    fn info(&self) -> &PrimitiveInfo {
        match self {
            Self::Builtin(primitive) => {
                // Built-ins are pinned to the catalog by test; the expect
                // documents that invariant at the single lookup funnel.
                catalog_lookup(primitive.type_name())
                    .expect("built-in primitive missing from the animatix-std catalog")
            },
            Self::Extension(_, info) => info,
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

    /// Register a runtime (extension/plugin) primitive with its identity card.
    ///
    /// Extensions carry their own metadata (the engine trait is behaviour
    /// only), so the caller supplies the [`PrimitiveInfo`] the plugin or
    /// embedding declared at registration time.
    pub fn register(
        &mut self,
        primitive: Arc<dyn Primitive>,
        info: PrimitiveInfo,
    ) -> Result<(), PrimitiveRegistrationError> {
        let name = primitive.type_name();
        if self.find(name).is_some() {
            return Err(PrimitiveRegistrationError::Duplicate(name.to_string()));
        }
        self.primitives.push(RegisteredPrimitive::Extension(primitive, info));
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

    /// Look up the identity card for a registered primitive.
    pub fn info_of(&self, name: &str) -> Option<&PrimitiveInfo> {
        self.primitives
            .iter()
            .find(|registered| registered.as_ref().type_name() == name)
            .map(|registered| match registered {
                RegisteredPrimitive::Builtin(primitive) => catalog_lookup(primitive.type_name())
                    .expect("built-in primitive missing from the animatix-std catalog"),
                RegisteredPrimitive::Extension(_, info) => info,
            })
    }

    /// Iterate primitives together with their identity cards.
    pub fn iter_with_info(&self) -> impl Iterator<Item = (&dyn Primitive, &PrimitiveInfo)> {
        self.primitives
            .iter()
            .map(|registered| (registered.as_ref(), registered.info()))
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
        self.iter_with_info()
            .map(|(primitive, info)| animatix_syntax::schema::PrimitiveSpec {
                type_name: primitive.type_name().to_string(),
                display_name: info.display_name.to_string(),
                category: super::actor_category_to_primitive_category(info.category),
                icon_id: info.icon_id.to_string(),
                advanced: info.advanced,
                capabilities: info.capabilities,
                child_processing: info.child_processing,
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
    use crate::primitives::{BuildCtx, EvaluateCtx, Primitive, RenderCommand, TextCompileCtx};
    use crate::renderer::error::RenderError;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct Gauge;

    impl Primitive for Gauge {
        fn type_name(&self) -> &str {
            "Gauge"
        }

        fn build(
            &self,
            ctx: &mut BuildCtx,
            label: &str,
            _props: &[Property],
            _modifiers: &[Modifier],
            _children: &[InlineItem],
        ) -> Result<(), Vec<Diagnostic>> {
            let track =
                ctx.timeline.tracks.entry(label.to_string()).or_insert_with(|| {
                    crate::timeline::AnimationTrack::placeholder(label.to_string())
                });
            track.set_identity("Text");
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

    fn gauge_info() -> animatix_std::PrimitiveInfo {
        animatix_std::PrimitiveInfo {
            type_name: "Gauge",
            display_name: "Gauge",
            category: crate::timeline::ActorCategory::Plot,
            icon_id: "gauge",
            advanced: false,
            capabilities: Default::default(),
            child_processing: Default::default(),
            shape: None,
            text: None,
            stroke_path: false,
        }
    }

    #[test]
    fn registry_layers_custom_primitive_over_builtins() {
        let mut registry = PrimitiveRegistry::new();
        assert!(registry.find("Rect").is_some());
        assert!(registry.is_builtin("Rect"));
        assert!(registry.find("Gauge").is_none());
        assert!(!registry.is_builtin("Gauge"));
        assert!(registry.register(Arc::new(Gauge), gauge_info()).is_ok());
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
        assert!(registry.register(Arc::new(Gauge), gauge_info()).is_ok());
        assert_eq!(registry.primitives.len(), super::PRIMITIVES.len() + 1);
        assert!(registry.remove("Gauge"));
        assert_eq!(registry.primitives.len(), super::PRIMITIVES.len());
    }

    /// The engine behaviour registry and the `animatix-std` catalog must name
    /// the same built-in set, in the same order — the catalog is the
    /// author-visible half of every registered behaviour.
    #[test]
    fn registry_names_match_std_catalog() {
        let registry = PrimitiveRegistry::new();
        let catalog_names: Vec<&str> =
            animatix_std::CATALOG.iter().map(|info| info.type_name).collect();
        let registry_names: Vec<&str> =
            registry.iter().map(|primitive| primitive.type_name()).collect();
        assert_eq!(registry_names, catalog_names, "registry and catalog drifted");
    }

    #[test]
    fn duplicate_primitive_is_rejected() {
        let mut registry = PrimitiveRegistry::new();
        assert_eq!(registry.register(Arc::new(Gauge), gauge_info()), Ok(()));
        assert!(registry.register(Arc::new(Gauge), gauge_info()).is_err());
    }

    #[test]
    fn custom_primitive_builds_through_timeline() {
        let (ast, errors) = animatix_syntax::parser::parse_source("g: Gauge");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut registry = PrimitiveRegistry::new();
        registry
            .register(
                Arc::new(Gauge),
                animatix_std::PrimitiveInfo {
                    type_name: "Gauge",
                    display_name: "Gauge",
                    category: crate::timeline::ActorCategory::Plot,
                    icon_id: "gauge",
                    advanced: false,
                    capabilities: Default::default(),
                    child_processing: Default::default(),
                    shape: None,
                    text: None,
                    stroke_path: false,
                },
            )
            .expect("register Gauge");
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
        assert_eq!(track.actor_type, "Gauge");
        assert_eq!(track.actor_type, "Gauge");

        let _scene = report.output.evaluate(0.0, crate::timeline::SceneDimensions::default());
    }
}
