use animatix_core::caps::{ActorCaps, ActorCategory};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveFamilyDescriptor {
    pub caps: ActorCaps,
    /// Which plot family this primitive is, carried from the catalog card so the
    /// plot builder never has to compare `ty == "VectorField"`.
    pub plot: Option<animatix_core::caps::PlotKind>,
    /// Whether the generic actor-declaration body owns this primitive's
    /// connector geometry lanes (see `PrimitiveInfo::connector`). Carried here so
    /// the build path does not have to re-run a catalog lookup.
    pub connector: bool,
}

impl Default for PrimitiveFamilyDescriptor {
    fn default() -> Self {
        Self {
            caps: ActorCaps {
                vector_paths: true,
                category: ActorCategory::Shape,
                shape: Some(animatix_core::caps::ShapeKind::Rect),
                has_visual_content: true,
                ..Default::default()
            },
            plot: None,
            connector: false,
        }
    }
}

impl PrimitiveFamilyDescriptor {
    /// Classify a primitive from its registration info card.
    ///
    /// This deliberately takes `&PrimitiveInfo` instead of looking up a static
    /// built-in so extension primitives participate in the same classification.
    pub fn from_info(info: &animatix_std::PrimitiveInfo) -> Self {
        let mut caps = animatix_std::caps_from_info(info);

        // A plugin card that declares the `Container` category without any
        // container capability flag still normalizes to a recursable group
        // container, mirroring the historical per-family default. A card that
        // marks `layout_container` already carries the flag, so the old
        // `Container` fallback was unreachable.
        if caps.category == ActorCategory::Container && !caps.layout_container && !caps.is_container
        {
            caps.is_container = true;
        }

        Self {
            caps,
            plot: info.plot,
            connector: info.connector,
        }
    }

    #[inline]
    pub fn is_plot(&self) -> bool {
        self.caps.is_plot()
    }

    #[inline]
    pub fn is_graph_host(&self) -> bool {
        self.caps.is_graph_host()
    }

    #[inline]
    pub fn is_plot_curve(&self) -> bool {
        self.caps.is_plot_curve()
    }

    #[inline]
    pub fn is_layout_container(&self) -> bool {
        self.caps.is_layout_container()
    }

    /// Container semantics used by group-target expansion: layout containers
    /// and structural groups recurse into children, while equation containers
    /// aggregate children into one renderable document.
    #[inline]
    pub fn is_recursive_container(&self) -> bool {
        self.caps.is_recursive_container()
    }
}

#[cfg(test)]
mod tests {
    use animatix_std::catalog_lookup;

    use super::PrimitiveFamilyDescriptor;

    #[test]
    fn classifies_text_like_primitives() {
        let info = catalog_lookup("Text").expect("Text built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert!(descriptor.caps.text_paths);
        assert_eq!(descriptor.caps.category, animatix_core::caps::ActorCategory::Text);
    }

    #[test]
    fn classifies_plot_primitives() {
        let info = catalog_lookup("PlotCurve").expect("PlotCurve built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert!(descriptor.caps.plot_geometry);
    }

    #[test]
    fn classifies_layout_containers() {
        let info = catalog_lookup("Row").expect("Row built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert!(descriptor.is_layout_container());
        assert!(descriptor.is_recursive_container());
    }

    #[test]
    fn classifies_structural_groups_as_recursive_containers() {
        let info = catalog_lookup("Group").expect("Group built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert!(descriptor.caps.is_container);
        assert!(!descriptor.is_layout_container());
        assert!(descriptor.is_recursive_container());
    }

    #[test]
    fn equation_containers_are_not_recursive_action_targets() {
        let info = catalog_lookup("Equation").expect("Equation built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert!(!descriptor.is_recursive_container());
    }

    #[test]
    fn classifies_graph_as_plot_host() {
        let info = catalog_lookup("Graph").expect("Graph built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert!(descriptor.is_graph_host());
        assert!(!descriptor.is_plot_curve());
    }

    #[test]
    fn treats_circle_as_vector_shape() {
        let info = catalog_lookup("Ellipse").expect("Ellipse built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert!(descriptor.caps.vector_paths);
        assert_eq!(descriptor.caps.shape, Some(animatix_core::caps::ShapeKind::Ellipse));
    }

    #[test]
    fn caps_pass_through_untouched_for_builtins() {
        // The full schema capability projection is preserved instead of being
        // rebuilt from a coarse family classification.
        let info = catalog_lookup("Rect").expect("Rect built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert_eq!(descriptor.caps, animatix_std::caps_from_info(info));
        assert!(descriptor.caps.vector_paths);
        assert!(descriptor.caps.is_shape);

        let info = catalog_lookup("PlotCurve").expect("PlotCurve built-in");
        let descriptor = PrimitiveFamilyDescriptor::from_info(info);
        assert!(descriptor.caps.plot_geometry);
        assert!(!descriptor.caps.is_shape);
    }

    #[test]
    fn sloppy_container_category_gets_layout_defaults() {
        use crate::ast::{InlineItem, Modifier, Property};
        use crate::primitives::BuildCtx;

        // A plugin that declares Container capabilities (layout_container +
        // is_container, as the schema defaults and the ABI describe) must
        // classify as a layout container with its flags left intact.
        #[allow(dead_code)] // Referenced only through the registration info card below
        struct SloppyContainer;
        impl crate::primitives::Primitive for SloppyContainer {
            fn type_name(&self) -> &str {
                "Sloppy"
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

        let mut container_info = animatix_std::PrimitiveInfo::extension(
            "SloppyContainer",
            crate::timeline::ActorCategory::Container,
        );
        container_info.capabilities.layout_container = true;
        container_info.capabilities.is_container = true;
        let descriptor = PrimitiveFamilyDescriptor::from_info(&container_info);
        assert!(descriptor.is_layout_container());
        assert!(descriptor.caps.is_container);
        assert!(descriptor.is_recursive_container());

        // A genuinely sloppy card (Container category, no capability flags)
        // still normalizes to a recursable group container.
        let bare_info = animatix_std::PrimitiveInfo::extension(
            "BareContainer",
            crate::timeline::ActorCategory::Container,
        );
        let bare = PrimitiveFamilyDescriptor::from_info(&bare_info);
        assert!(bare.caps.is_container);
        assert!(!bare.caps.layout_container);
        assert!(bare.is_recursive_container());
    }
}
