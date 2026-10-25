use crate::timeline::*;

impl Timeline {
    /// Authored layout children for a container at the current frame.
    pub fn layout_children_for(&self, container_id: EntityId) -> Arc<Vec<ContainerLayoutChild>> {
        if let Some(cached) = self.layout_children_cache.borrow().get(&container_id) {
            return Arc::clone(cached);
        }
        let computed = Arc::new(self.compute_layout_children(container_id));
        self.layout_children_cache
            .borrow_mut()
            .insert(container_id, Arc::clone(&computed));
        computed
    }

    /// Uncached admission computation behind [`Timeline::layout_children_for`].
    pub(crate) fn compute_layout_children(
        &self,
        container_id: EntityId,
    ) -> Vec<ContainerLayoutChild> {
        let Some(metadata) = self.world.container_layout.get(container_id) else {
            return Vec::new();
        };
        metadata.layout_children(&self.world)
    }

    /// Computes layout positions for a container using a specific child order.
    pub(crate) fn compute_layout_positions_for_order(
        &self,
        metadata: &ContainerMetadata,
        order: &[EntityId],
        time_ms: u64,
    ) -> layout::LayoutPositions {
        use crate::timeline::layout::ChildExtent;

        // Extents drop children without a sampled `layout_size`; the parallel
        // id list keeps the result map aligned with the computed positions.
        let mut admitted_ids: Vec<EntityId> = Vec::with_capacity(order.len());
        let child_extents: Vec<ChildExtent> = order
            .iter()
            .filter_map(|child_id| {
                let geometry = self.world.geometry(*child_id);
                let half_size = geometry.layout_size_get(time_ms)?;
                admitted_ids.push(*child_id);
                Some(ChildExtent {
                    half_size,
                    placement_mode: geometry
                        .placement_mode
                        .get(time_ms, PlacementMode::LayoutManaged),
                })
            })
            .collect();

        let positions = LayoutEngine::compute_positions(metadata, &child_extents);

        let mut result = layout::LayoutPositions::new();
        for (i, extent) in child_extents.iter().enumerate() {
            if extent.placement_mode == PlacementMode::LayoutManaged {
                result.insert_id(admitted_ids[i], positions[i]);
            }
        }
        result
    }

    /// Computes layout positions with animated child-order transitions.
    /// If a child-order component is present, interpolates positions between
    /// the old and new orders. Otherwise delegates to static layout.
    pub fn compute_animated_layout(
        &self,
        container_id: EntityId,
        time_ms: u64,
    ) -> Arc<layout::LayoutPositions> {
        let Some(metadata) = self.world.container_layout.get(container_id) else {
            // Shared empty table: this early return fires once per
            // non-container node per frame (alloc_driver 2026-09-04 measured
            // ~60 fresh empty `LayoutPositions` allocations per frame on a
            // 60-actor scene), and callers only ever read through the Arc.
            return layout::empty_layout_positions();
        };

        // Check for an animated child order on the container entity
        if let Some(component) = self.world.child_order.get(container_id) {
            let track = &component.track;
            let prev = track.keyframes.prev_keyframe(time_ms);
            let next = track.keyframes.next_keyframe(time_ms);

            match (prev, next) {
                (Some((&t1, (order1, easing1))), Some((&t2, (order2, _)))) if t1 != t2 => {
                    // Between two keyframes: interpolate
                    let t = ((time_ms - t1) as f32 / (t2 - t1) as f32).clamp(0.0, 1.0);
                    let eased_t = apply_easing(t, *easing1);

                    let pos1 = self.compute_layout_positions_for_order(metadata, order1, time_ms);
                    let pos2 = self.compute_layout_positions_for_order(metadata, order2, time_ms);

                    let mut result = layout::LayoutPositions::new();
                    for id in order1.iter().chain(order2.iter()) {
                        let p1 = pos1.get_by_id(*id).copied().unwrap_or([0.0, 0.0]);
                        let p2 = pos2.get_by_id(*id).copied().unwrap_or([0.0, 0.0]);
                        let pos = p1.interpolate(&p2, eased_t);
                        result.insert_id(*id, pos);
                    }
                    return Arc::new(result);
                },
                (Some((_, (order, _))), _) => {
                    // At or after a keyframe (or at the only keyframe): use it directly
                    return Arc::new(
                        self.compute_layout_positions_for_order(metadata, order, time_ms),
                    );
                },
                (None, Some((&t, (order, easing)))) => {
                    // Before the first keyframe: interpolate from default_value to first keyframe
                    let t = (time_ms as f32 / t as f32).clamp(0.0, 1.0);
                    let eased_t = apply_easing(t, *easing);

                    let pos1 = self.compute_layout_positions_for_order(
                        metadata,
                        &track.default_value,
                        time_ms,
                    );
                    let pos2 = self.compute_layout_positions_for_order(metadata, order, time_ms);

                    let mut result = layout::LayoutPositions::new();
                    for id in track.default_value.iter().chain(order.iter()) {
                        let p1 = pos1.get_by_id(*id).copied().unwrap_or([0.0, 0.0]);
                        let p2 = pos2.get_by_id(*id).copied().unwrap_or([0.0, 0.0]);
                        let pos = p1.interpolate(&p2, eased_t);
                        result.insert_id(*id, pos);
                    }
                    return Arc::new(result);
                },
                (None, None) => {
                    // Empty track — should not happen, but fall through
                },
            }
        }

        // No child-order track — delegate to static layout
        let layout_children = self.layout_children_for(container_id);
        self.layout_engine.compute_layout_for_time(
            container_id,
            metadata,
            &layout_children,
            time_ms,
            &self.world,
        )
    }

    /// Resolve animated volume envelopes for all audio segments from their tracks.
    pub(crate) fn resolve_audio_volume_envelopes(&mut self) {
        for seg in &mut self.audio_segments {
            // Find track with matching normalized source or Audio actor type
            let matching_id = self.world.tracks.iter().find_map(|(_, id)| {
                (self.world.actor_type(id) == "Audio" && self.world.geometry(id).volume.is_some())
                    .then_some(id)
            });
            if let Some(audio_id) = matching_id {
                if let Some(vol_track) = &self.world.geometry(audio_id).volume {
                    let mut envelope = Vec::new();
                    let t_start_ms = (seg.start_time_s * 1000.0).round() as u64;
                    let dur_ms =
                        seg.duration_s.map(|d| (d * 1000.0).round() as u64).unwrap_or(30_000);
                    let t_end_ms = t_start_ms + dur_ms;

                    // Sample keyframes that fall in or near the segment span
                    for (time_ms, _) in vol_track.keyframes() {
                        let t_s = *time_ms as f64 / 1000.0;
                        let val = vol_track.evaluate(*time_ms);
                        envelope.push((t_s, val));
                    }
                    if envelope.is_empty() {
                        let v = vol_track.evaluate(t_start_ms);
                        envelope.push((seg.start_time_s, v));
                    }
                    if !envelope.iter().any(|(t, _)| (*t - seg.start_time_s).abs() < 1e-4) {
                        let v = vol_track.evaluate(t_start_ms);
                        envelope.insert(0, (seg.start_time_s, v));
                    }
                    let seg_end_s = t_end_ms as f64 / 1000.0;
                    if !envelope.iter().any(|(t, _)| (*t - seg_end_s).abs() < 1e-4) {
                        let v = vol_track.evaluate(t_end_ms);
                        envelope.push((seg_end_s, v));
                    }
                    envelope.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
                    envelope.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-4);
                    seg.volume = vol_track.evaluate(t_start_ms);
                    seg.volume_envelope = Some(envelope);
                }
            }
        }
    }
}
