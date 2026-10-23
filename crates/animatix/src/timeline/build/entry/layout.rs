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
        let child_extents: Vec<ChildExtent> = child_labels
            .iter()
            .filter_map(|cl| {
                let track = self.world.tracks.get(cl)?;
                // Use layout_size_last() to get the most recently keyframed value,
                // consistent with apply_container_layout().
                let half_size = track.layout_size_last()?;
                Some(ChildExtent {
                    label: cl.clone(),
                    half_size,
                    placement_mode: track
                        .geometry
                        .placement_mode
                        .last(crate::timeline::PlacementMode::LayoutManaged),
                })
            })
            .collect();

        let (size_specs, constraints, parent_content_size) =
            self.collect_child_layout_specs(label, &self.layout_children_for(label), time_ms);

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

        // Propagate container width to text children
        self.propagate_text_child_widths(label, container_size, time_ms, diagnostics);

        // Pass 2: Re-apply layout with updated (wrapped) child sizes
        // This ensures positions account for the new wrapped text dimensions.
        self.apply_container_layout(label, time_ms as f64, diagnostics);
    }
}
