use crate::primitives::{ChildProcessing, Primitive};
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveFamilyDescriptor {
    pub family: PrimitiveFamily,
    pub capabilities: PrimitiveCapabilities,
    pub child_processing: ChildProcessing,
}

impl Default for PrimitiveFamilyDescriptor {
    fn default() -> Self {
        Self {
            family: PrimitiveFamily::VectorShape,
            capabilities: PrimitiveCapabilities {
                vector_paths: true,
                morphable_paths: true,
                vector_reveal_target: true,
                is_shape: true,
                ..PrimitiveCapabilities::default()
            },
            child_processing: ChildProcessing::Generic,
        }
    }
}

impl PrimitiveFamilyDescriptor {
    /// Classify a primitive from the active runtime registry.
    ///
    /// This deliberately takes `&dyn Primitive` instead of looking up a static
    /// built-in so extension primitives participate in the same classification.
    pub fn from_primitive(primitive: &dyn Primitive) -> Self {
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

        // Keep the primitive's own capability flags (shared-schema defaults
        // for built-ins, plugin ABI flags for extensions) instead of rebuilding
        // a family-inferred second set that can silently drift. The only
        // normalization is a fallback for sloppy plugins that declare a
        // category without any container capability, mirroring the previous
        // per-family default rows.
        let capabilities = if family == PrimitiveFamily::Container && !caps.layout_container {
            PrimitiveCapabilities {
                layout_container: true,
                ..caps
            }
        } else if family == PrimitiveFamily::Group && !caps.is_container {
            PrimitiveCapabilities {
                is_container: true,
                ..caps
            }
        } else {
            caps
        };
        Self {
            family,
            capabilities,
            child_processing: primitive.child_processing(),
        }
    }

    pub fn is_plot(&self) -> bool {
        self.family == PrimitiveFamily::Plot
    }

    pub fn is_graph_host(&self) -> bool {
        self.capabilities.plot_host
    }

    pub fn is_plot_curve(&self) -> bool {
        self.is_plot() && !self.is_graph_host()
    }

    pub fn is_layout_container(&self) -> bool {
        self.capabilities.layout_container
    }

    /// Container semantics used by group-target expansion: layout containers
    /// and structural groups recurse into children, while equation containers
    /// aggregate children into one renderable document.
    pub fn is_recursive_container(&self) -> bool {
        (self.capabilities.layout_container || self.capabilities.is_container)
            && self.child_processing != ChildProcessing::Equation
    }
}

#[cfg(test)]
mod tests {
    use super::{PrimitiveFamily, PrimitiveFamilyDescriptor};
    use crate::primitives::find_primitive;

    #[test]
    fn classifies_text_like_primitives() {
        let primitive = find_primitive("Text").expect("Text built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(primitive);
        assert_eq!(descriptor.family, PrimitiveFamily::TextLike);
        assert!(descriptor.capabilities.text_paths);
        assert!(descriptor.capabilities.morphable_paths);
    }

    #[test]
    fn classifies_plot_primitives() {
        let primitive = find_primitive("PlotCurve").expect("PlotCurve built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(primitive);
        assert_eq!(descriptor.family, PrimitiveFamily::Plot);
        assert!(descriptor.capabilities.plot_geometry);
    }

    #[test]
    fn classifies_layout_containers() {
        let primitive = find_primitive("Row").expect("Row built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(primitive);
        assert_eq!(descriptor.family, PrimitiveFamily::Container);
        assert!(descriptor.is_layout_container());
        assert!(descriptor.is_recursive_container());
    }

    #[test]
    fn classifies_structural_groups_as_recursive_containers() {
        let primitive = find_primitive("Group").expect("Group built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(primitive);
        assert!(!descriptor.is_layout_container());
        assert!(descriptor.is_recursive_container());
    }

    #[test]
    fn equation_containers_are_not_recursive_action_targets() {
        let primitive = find_primitive("Equation").expect("Equation built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(primitive);
        assert!(!descriptor.is_recursive_container());
    }

    #[test]
    fn classifies_graph_as_plot_host() {
        let primitive = find_primitive("Graph").expect("Graph built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(primitive);
        assert!(descriptor.is_graph_host());
        assert!(!descriptor.is_plot_curve());
    }

    #[test]
    fn treats_circle_as_vector_shape() {
        let primitive = find_primitive("Ellipse").expect("Ellipse built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(primitive);
        assert_eq!(descriptor.family, PrimitiveFamily::VectorShape);
        assert!(descriptor.capabilities.vector_paths);
    }

    #[test]
    fn capabilities_pass_through_untouched_for_builtins() {
        // The full schema capability set is preserved instead of being
        // stripped down to a family-inferred subset.
        let rect = find_primitive("Rect").expect("Rect built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(rect);
        assert!(descriptor.capabilities.vector_paths);
        assert!(descriptor.capabilities.is_shape);

        let curve = find_primitive("PlotCurve").expect("PlotCurve built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_primitive(curve);
        assert!(descriptor.capabilities.plot_geometry);
        assert!(descriptor.capabilities.morphable_paths);
        assert!(!descriptor.capabilities.is_shape);
    }

    #[test]
    fn sloppy_container_category_gets_layout_defaults() {
        use crate::ast::{InlineItem, Modifier, Property};
        use crate::primitives::BuildCtx;
        use crate::timeline::{ActorCategory, ActorKindId};

        // A plugin that declares only a Container category inherits the
        // schema's container capability defaults (layout_container +
        // is_container), so it must classify as a layout container even
        // without explicit ABI capability flags.
        struct SloppyContainer;
        impl crate::primitives::Primitive for SloppyContainer {
            fn type_name(&self) -> &str {
                "Sloppy"
            }
            fn display_name(&self) -> &str {
                "Sloppy"
            }
            fn category(&self) -> ActorCategory {
                ActorCategory::Container
            }
            fn icon_id(&self) -> &str {
                "sloppy"
            }
            fn kind_id(&self) -> ActorKindId {
                ActorKindId::Group
            }
            fn build(
                &self,
                _ctx: &mut BuildCtx,
                _label: &str,
                _props: &[Property],
                _modifiers: &[Modifier],
                _children: &[InlineItem],
            ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
                Ok(())
            }
        }

        let descriptor = PrimitiveFamilyDescriptor::from_primitive(&SloppyContainer);
        assert_eq!(descriptor.family, PrimitiveFamily::Container);
        assert!(descriptor.is_layout_container());
        assert!(descriptor.is_recursive_container());
    }
}
