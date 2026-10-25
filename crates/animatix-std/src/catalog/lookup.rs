use super::*;

pub fn catalog_lookup(name: &str) -> Option<&'static PrimitiveInfo> {
    CATALOG.iter().copied().find(|info| info.type_name.as_ref() == name)
}

/// Derive the engine's capability projection from a catalog row.
pub fn caps_from_info(info: &PrimitiveInfo) -> ActorCaps {
    let stroke_primary = info.capabilities.stroke_primary
        || (info.shape.is_some_and(|k| matches!(k, ShapeKind::Line | ShapeKind::Arrow)));
    ActorCaps {
        category: info.category,
        child_processing: info.child_processing,
        shape: info.shape,
        text: info.text,
        is_shape: info.capabilities.is_shape,
        stroke_path: info.stroke_path || stroke_primary || info.capabilities.callout,
        text_paths: info.capabilities.text_paths,
        vector_paths: info.capabilities.vector_paths,
        image_payload: info.capabilities.image_payload,
        layout_container: info.capabilities.layout_container,
        is_container: info.capabilities.is_container,
        plot_geometry: info.capabilities.plot_geometry,
        plot_host: info.capabilities.plot_host,
        time_dependent: info.capabilities.time_dependent,
        transcript: info.capabilities.transcript,
        has_visual_content: info.capabilities.has_visual_content
            || (!info.capabilities.is_container
                && (info.capabilities.is_shape
                    || info.capabilities.text_paths
                    || info.capabilities.image_payload
                    || info.capabilities.vector_paths
                    || info.capabilities.plot_geometry
                    || info.capabilities.plot_host
                    || matches!(
                        info.category,
                        ActorCategory::Shape
                            | ActorCategory::Text
                            | ActorCategory::Plot
                            | ActorCategory::Annotation
                    ))),
        stroke_primary,
        callout: info.capabilities.callout,
        legend_host: info.capabilities.legend_host,
    }
}

/// Derive caps for an authored type name from the catalog. Returns `None` for
/// unregistered names — callers decide the fallback (the all-false
/// [`ActorCaps::default()`] preserves the historical extension behaviour).
pub fn caps_for_type(actor_type: &str) -> Option<ActorCaps> {
    catalog_lookup(actor_type).map(caps_from_info)
}
