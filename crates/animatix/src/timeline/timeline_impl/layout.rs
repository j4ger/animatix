use crate::timeline::*;

impl Timeline {
    /// Authored layout children for a container at the current frame.
    pub fn layout_children_for(&self, container_label: &str) -> Arc<Vec<ContainerLayoutChild>> {
        if let Some(cached) = self.layout_children_cache.borrow().get(container_label) {
            return Arc::clone(cached);
        }
        let computed = Arc::new(self.compute_layout_children(container_label));
        self.layout_children_cache
            .borrow_mut()
            .insert(container_label.to_string(), Arc::clone(&computed));
        computed
    }

    /// Uncached admission computation behind [`Timeline::layout_children_for`].
    pub(crate) fn compute_layout_children(
        &self,
        container_label: &str,
    ) -> Vec<ContainerLayoutChild> {
        let Some(metadata) = self.container_layout(container_label) else {
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

        let child_extents: Vec<ChildExtent> = order
            .iter()
            .filter_map(|child_id| {
                let track = self.world.track(*child_id)?;
                Some(ChildExtent {
                    label: self.world.label(*child_id)?.to_string(),
                    half_size: track.layout_size_get(self.world.geometry(track.id), time_ms)?,
                    placement_mode: self
                        .world
                        .geometry(track.id)
                        .placement_mode
                        .get(time_ms, PlacementMode::LayoutManaged),
                })
            })
            .collect();

        let positions = LayoutEngine::compute_positions(metadata, &child_extents);

        let mut result = layout::LayoutPositions::new();
        for (i, child) in child_extents.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                if let Some(id) = self.world.tracks.get_id(&child.label) {
                    result.insert_with_id(id, child.label.clone(), positions[i]);
                } else {
                    result.insert(child.label.clone(), positions[i]);
                }
            }
        }
        result
    }

    /// Computes layout positions with animated child-order transitions.
    /// If a child-order component is present, interpolates positions between
    /// the old and new orders. Otherwise delegates to static layout.
    pub fn compute_animated_layout(
        &self,
        container_label: &str,
        time_ms: u64,
    ) -> Arc<layout::LayoutPositions> {
        let Some(metadata) = self.container_layout(container_label) else {
            // Shared empty table: this early return fires once per
            // non-container node per frame (alloc_driver 2026-09-04 measured
            // ~60 fresh empty `LayoutPositions` allocations per frame on a
            // 60-actor scene), and callers only ever read through the Arc.
            return layout::empty_layout_positions();
        };

        // Check for an animated child order on the container entity
        if let Some(component) = self.child_order(container_label) {
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
                        let Some(label) = self.world.label(*id) else {
                            continue;
                        };
                        let p1 = pos1.get(label).copied().unwrap_or([0.0, 0.0]);
                        let p2 = pos2.get(label).copied().unwrap_or([0.0, 0.0]);
                        let pos = p1.interpolate(&p2, eased_t);
                        result.insert_with_id(*id, label.to_string(), pos);
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
                        let Some(label) = self.world.label(*id) else {
                            continue;
                        };
                        let p1 = pos1.get(label).copied().unwrap_or([0.0, 0.0]);
                        let p2 = pos2.get(label).copied().unwrap_or([0.0, 0.0]);
                        let pos = p1.interpolate(&p2, eased_t);
                        result.insert_with_id(*id, label.to_string(), pos);
                    }
                    return Arc::new(result);
                },
                (None, None) => {
                    // Empty track — should not happen, but fall through
                },
            }
        }

        // No child-order track — delegate to static layout
        let layout_children = self.layout_children_for(container_label);
        self.layout_engine.compute_layout_for_time(
            container_label,
            metadata,
            &layout_children,
            time_ms,
            &self.world.tracks,
            &self.world,
        )
    }

    /// Resolve animated volume envelopes for all audio segments from their tracks.
    pub(crate) fn resolve_audio_volume_envelopes(&mut self) {
        for seg in &mut self.audio_segments {
            // Find track with matching normalized source or Audio actor type
            let matching_track =
                self.world.tracks.dense.iter().find(|t| {
                    t.actor_type == "Audio" && self.world.geometry(t.id).volume.is_some()
                });
            if let Some(track) = matching_track {
                if let Some(vol_track) = &self.world.geometry(track.id).volume {
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
