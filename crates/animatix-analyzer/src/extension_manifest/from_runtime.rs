//! Build an analyzer manifest from runtime extension descriptors.

use animatix_syntax::schema::{
    ActionSignature, FunctionDescriptor, PrimitiveCategory, PrimitiveDescriptor, PrimitiveSpec,
    PropertyDescriptor, ServiceDescriptor,
};

use super::manifest::{ExtensionManifest, ManifestEffect};

impl ExtensionManifest {
    /// Build a manifest from runtime descriptors, stripping runtime property ids.
    ///
    /// This is the single-source path for generating analyzer metadata from a
    /// loaded native plugin or in-process extension context.
    pub fn from_runtime(
        library: Option<String>,
        primitives: &[PrimitiveSpec],
        properties: &[PropertyDescriptor],
        actions: &[ActionSignature],
        functions: &[FunctionDescriptor],
        services: &[ServiceDescriptor],
        effects: &[ManifestEffect],
    ) -> Self {
        Self {
            library,
            effects: effects.to_vec(),
            primitives: primitives
                .iter()
                .map(|spec| {
                    let declared = properties
                        .iter()
                        .filter(|property| {
                            property.actor_types.iter().any(|ty| ty == &spec.type_name)
                        })
                        .map(|property| property.name.clone())
                        .collect();
                    let mut capabilities = spec.capabilities;
                    if !capabilities.has_visual_content && !capabilities.is_container {
                        capabilities.has_visual_content = capabilities.is_shape
                            || capabilities.text_paths
                            || capabilities.image_payload
                            || capabilities.vector_paths
                            || capabilities.plot_geometry
                            || capabilities.plot_host
                            || matches!(
                                spec.category,
                                PrimitiveCategory::Shape
                                    | PrimitiveCategory::Text
                                    | PrimitiveCategory::Plot
                                    | PrimitiveCategory::Annotation
                            );
                    }
                    PrimitiveDescriptor {
                        type_name: spec.type_name.clone(),
                        display_name: spec.display_name.clone(),
                        category: spec.category,
                        icon_id: spec.icon_id.clone(),
                        advanced: spec.advanced,
                        capabilities,
                        child_processing: spec.child_processing,
                        properties: declared,
                    }
                })
                .collect(),
            properties: properties
                .iter()
                .map(|property| PropertyDescriptor {
                    id: None,
                    name: property.name.clone(),
                    actor_types: property.actor_types.clone(),
                    ty: property.ty.clone(),
                    value_kind: property.value_kind,
                    injectable: property.injectable,
                    display_name: property.display_name.clone(),
                    group: property.group.clone(),
                    help: property.help.clone(),
                })
                .collect(),
            actions: actions.to_vec(),
            functions: functions.to_vec(),
            services: services.to_vec(),
        }
    }
}
