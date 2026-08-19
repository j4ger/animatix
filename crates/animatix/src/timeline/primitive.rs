use crate::primitives::find_primitive;
use crate::timeline::ActorCategory;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimitiveFamily {
    TextLike,
    VectorShape,
    Media,
    Plot,
    Container,
    Group,
}

/// Capability flags shared with schema/tooling descriptors.
pub type PrimitiveCapabilities = animatix_syntax::schema::PrimitiveCapabilities;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PrimitiveFamilyDescriptor {
    pub actor_type: &'static str,
    pub family: PrimitiveFamily,
    pub capabilities: PrimitiveCapabilities,
}

impl PrimitiveFamilyDescriptor {
    pub fn for_actor_type(actor_type: &str) -> Self {
        if let Some(primitive) = find_primitive(actor_type) {
            return Self::from_primitive(primitive);
        }

        // Fallback for unknown/unregistered types
        Self {
            actor_type: "VectorShape",
            family: PrimitiveFamily::VectorShape,
            capabilities: PrimitiveCapabilities {
                vector_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                ..PrimitiveCapabilities::default()
            },
        }
    }

    fn from_primitive(primitive: &'static dyn crate::primitives::Primitive) -> Self {
        let caps = primitive.capabilities();
        let family = if caps.text_paths {
            PrimitiveFamily::TextLike
        } else if caps.plot_geometry {
            PrimitiveFamily::Plot
        } else if caps.image_payload {
            PrimitiveFamily::Media
        } else if caps.vector_paths {
            PrimitiveFamily::VectorShape
        } else if caps.layout_container {
            PrimitiveFamily::Container
        } else if caps.is_container {
            PrimitiveFamily::Group
        } else {
            match primitive.category() {
                ActorCategory::Shape => PrimitiveFamily::VectorShape,
                ActorCategory::Text => PrimitiveFamily::TextLike,
                ActorCategory::Media => PrimitiveFamily::Media,
                ActorCategory::Plot => PrimitiveFamily::Plot,
                ActorCategory::Container => PrimitiveFamily::Group,
                ActorCategory::Annotation => PrimitiveFamily::VectorShape,
            }
        };

        let actor_type = match family {
            PrimitiveFamily::TextLike => "TextLike",
            PrimitiveFamily::VectorShape => "VectorShape",
            PrimitiveFamily::Media if caps.image_payload => "Image",
            PrimitiveFamily::Media => "Svg",
            PrimitiveFamily::Plot => primitive.type_name(),
            PrimitiveFamily::Container => "Container",
            PrimitiveFamily::Group => "Group",
        };
        let capabilities = match family {
            PrimitiveFamily::TextLike => PrimitiveCapabilities {
                text_paths: caps.text_paths,
                morphable_paths: caps.morphable_paths,
                vector_reveal_target: caps.vector_reveal_target,
                ..PrimitiveCapabilities::default()
            },
            PrimitiveFamily::VectorShape => PrimitiveCapabilities {
                vector_paths: caps.vector_paths,
                morphable_paths: caps.morphable_paths,
                vector_reveal_target: caps.vector_reveal_target,
                ..PrimitiveCapabilities::default()
            },
            PrimitiveFamily::Media if caps.image_payload => PrimitiveCapabilities {
                image_payload: true,
                ..PrimitiveCapabilities::default()
            },
            PrimitiveFamily::Media => PrimitiveCapabilities {
                vector_paths: caps.vector_paths,
                morphable_paths: caps.morphable_paths,
                vector_reveal_target: caps.vector_reveal_target,
                ..PrimitiveCapabilities::default()
            },
            PrimitiveFamily::Plot => PrimitiveCapabilities {
                vector_paths: caps.vector_paths,
                morphable_paths: caps.morphable_paths,
                vector_reveal_target: caps.vector_reveal_target,
                plot_geometry: caps.plot_geometry,
                ..PrimitiveCapabilities::default()
            },
            PrimitiveFamily::Container => PrimitiveCapabilities {
                layout_container: true,
                ..PrimitiveCapabilities::default()
            },
            PrimitiveFamily::Group => PrimitiveCapabilities::default(),
        };
        Self {
            actor_type,
            family,
            capabilities,
        }
    }

    pub fn is_plot(self) -> bool {
        self.family == PrimitiveFamily::Plot
    }

    pub fn is_graph_host(self) -> bool {
        self.actor_type == "Graph"
    }

    pub fn is_plot_curve(self) -> bool {
        self.is_plot() && !self.is_graph_host()
    }

    pub fn is_layout_container(self) -> bool {
        self.capabilities.layout_container
    }
}

#[cfg(test)]
mod tests {
    use super::{PrimitiveFamily, PrimitiveFamilyDescriptor};

    #[test]
    fn classifies_text_like_primitives() {
        let descriptor = PrimitiveFamilyDescriptor::for_actor_type("Text");
        assert_eq!(descriptor.family, PrimitiveFamily::TextLike);
        assert!(descriptor.capabilities.text_paths);
        assert!(descriptor.capabilities.morphable_paths);
    }

    #[test]
    fn classifies_plot_primitives() {
        let descriptor = PrimitiveFamilyDescriptor::for_actor_type("PlotCurve");
        assert_eq!(descriptor.family, PrimitiveFamily::Plot);
        assert!(descriptor.capabilities.plot_geometry);
    }

    #[test]
    fn classifies_layout_containers() {
        let descriptor = PrimitiveFamilyDescriptor::for_actor_type("Row");
        assert_eq!(descriptor.family, PrimitiveFamily::Container);
        assert!(descriptor.is_layout_container());
    }

    #[test]
    fn treats_circle_as_vector_shape() {
        let descriptor = PrimitiveFamilyDescriptor::for_actor_type("Ellipse");
        assert_eq!(descriptor.family, PrimitiveFamily::VectorShape);
        assert!(descriptor.capabilities.vector_paths);
    }
}
