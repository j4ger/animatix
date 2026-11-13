use super::*;

impl Timeline {
    pub(crate) fn register_container_metadata_and_apply_layout(
        &mut self,
        label: &str,
        container_ty: &str,
        time_ms: u64,
        gap: [f32; 2],
        padding: [f32; 4],
        align: Option<&str>,
        cols: Option<usize>,
        diagnostics: &mut Vec<Diagnostic>,
        vertical_align: Option<&str>,
    ) {
        let child_labels: Vec<String> = self
            .world
            .get_id(label)
            .map(|id| self.world.child_labels(id).map(str::to_string).collect())
            .unwrap_or_default();
        // The container component stores entity ids; build-time layout
        // admission (diagnostics, extents) stays label-keyed.
        let child_order: Vec<EntityId> = child_labels
            .iter()
            .filter_map(|child_label| self.world.get_id(child_label))
            .collect();
        // layout_children is computed on demand via Timeline::layout_children_for
        let _layout_children =
            self.build_layout_children(label, container_ty, &child_labels, diagnostics);

        let metadata = ContainerMetadata {
            layout_type: LayoutType::from_container_ty(container_ty),
            gap,
            padding,
            align: align.unwrap_or("center").to_string(),
            vertical_align: vertical_align.unwrap_or("center").to_string(),
            cols,
            child_order,
        };

        self.set_container_layout(label, metadata.clone());

        // ─── Two-pass width propagation ───
        // Pass 1: Apply layout to get container size from Taffy.
        // This gives us the container's total width/height determined by children.
        self.apply_container_layout(label, time_ms as f64, diagnostics);

        // Compute container size for width propagation.
        // The container_size comes from the Taffy layout we just computed.
        let child_extents: Vec<ChildExtent> = metadata
            .child_order
            .iter()
            .filter_map(|child_id| {
                let geometry = self.world.geometry(*child_id);
                // Use layout_size_last() to get the most recently keyframed value,
                // consistent with apply_container_layout().
                let half_size = geometry.layout_size_last()?;
                Some(ChildExtent {
                    half_size,
                    placement_mode: geometry
                        .placement_mode
                        .last(crate::timeline::PlacementMode::LayoutManaged),
                })
            })
            .collect();

        let (size_specs, constraints, parent_content_size) = self
            .world
            .get_id(label)
            .map(|container_id| {
                self.collect_child_layout_specs(
                    container_id,
                    &self.layout_children_for(container_id),
                    time_ms,
                )
            })
            .unwrap_or((Vec::new(), Vec::new(), [0.0, 0.0]));

        let container_size = crate::timeline::layout::compute_container_size_with_specs(
            &child_extents,
            &metadata,
            &size_specs,
            &constraints,
            parent_content_size,
        );

        tracing::debug!(
            "Width propagation: {} '{}' container_size={:?}, {} children",
            container_ty,
            label,
            container_size,
            child_extents.len()
        );

        // A container without an explicit fixed `size` publishes the extent its
        // content actually covers (Stack: the union of its layers; Row/Col/
        // Grid: the flow size) so a nested container participates in its
        // parent's layout by the real box instead of the default 100×100.
        self.publish_measured_container_extent(label, container_size, time_ms);

        // A nested container under Col/Grid does not know its final cell width
        // yet: its parent is still being built. Leave default text widths
        // untouched so they cannot be mistaken for authored `text_max_width`
        // values during the later top-down reflow.
        let defer_width_propagation =
            self.world.get_id(label).is_some_and(|id| self.defer_nested_text_width(id));
        if !defer_width_propagation {
            // Text wrapping must use a declared fixed/relative box when one
            // exists. The intrinsic size above is child-derived and can be
            // much wider than a fixed Col (for example one glyph in a 1px
            // column). Dynamic layouts use their resolved slot instead of
            // the initial literal.
            let mut text_container_size = container_size;
            if let Some(container_id) = self.world.get_id(label) {
                let geometry = self.world.geometry(container_id);
                if let Some(spec) = geometry.size_spec {
                    let resolved_size = geometry
                        .layout_size_get(time_ms)
                        .or_else(|| geometry.layout_size_last())
                        .map(|half| [half[0] * 2.0, half[1] * 2.0]);
                    match spec.width {
                        crate::timeline::taffy_layout::SizeSpec::Fixed(width)
                            if !self.dynamic_layout =>
                        {
                            text_container_size[0] = width
                        },
                        crate::timeline::taffy_layout::SizeSpec::Percent(_)
                        | crate::timeline::taffy_layout::SizeSpec::Fill => {
                            if let Some(resolved) = resolved_size {
                                text_container_size[0] = resolved[0];
                            }
                        },
                        crate::timeline::taffy_layout::SizeSpec::Fixed(_) => {
                            if let Some(resolved) = resolved_size {
                                text_container_size[0] = resolved[0];
                            }
                        },
                        crate::timeline::taffy_layout::SizeSpec::Auto
                        | crate::timeline::taffy_layout::SizeSpec::Fit => {},
                    }
                }
            }
            self.propagate_text_child_widths(label, text_container_size, time_ms, diagnostics);
        }

        // Pass 2: Re-apply layout with updated (wrapped) child sizes
        // This ensures positions account for the new wrapped text dimensions.
        self.apply_container_layout(label, time_ms as f64, diagnostics);

        // Nested containers were initially measured before their parent had
        // resolved a Grid cell/Col width. Re-measure their text from the
        // resolved top-down constraints, then let wrapped heights flow back
        // up through the container tree.
        let has_nested_layout = self.world.get_id(label).is_some_and(|id| {
            self.world.hierarchy(id).children.iter().any(|child_id| {
                self.world
                    .label(*child_id)
                    .is_some_and(|child_label| self.container_layout(child_label).is_some())
                    || (self.world.caps(*child_id).is_group_like()
                        && self.group_needs_layout_reflow(*child_id))
            })
        });
        let has_auto_extent = self.world.get_id(label).is_some_and(|id| {
            self.world.geometry(id).size_spec.is_none_or(|spec| {
                !matches!(spec.width, crate::timeline::taffy_layout::SizeSpec::Fixed(_))
                    || !matches!(spec.height, crate::timeline::taffy_layout::SizeSpec::Fixed(_))
            })
        });
        let has_wrapping_text = matches!(metadata.layout_type, LayoutType::Col | LayoutType::Grid)
            && metadata.child_order.iter().any(|child_id| self.world.text(*child_id).is_some());
        if !defer_width_propagation && (has_nested_layout || (has_auto_extent && has_wrapping_text))
        {
            self.reflow_nested_layouts(label, time_ms, diagnostics);
        }
    }
}
