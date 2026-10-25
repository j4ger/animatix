use crate::timeline::*;

impl Timeline {
    /// Invalidate the cached frame, forcing the next evaluation to rebuild it.
    pub fn invalidate_frame_cache(&self) {
        // Every in-place mutation of this document funnels through here, which
        // makes this the one place a driver can learn that the pixels at a given
        // time may now differ. `FrameSignature` carries the value so a raster
        // dedup cannot serve a stale frame after an edit.
        self.eval_caches.content_epoch.set(self.eval_caches.content_epoch.get() + 1);
        self.refresh_blend_used();
        self.refresh_camera_used();
        // Recycle the invalidated entry's encoded scene into the scene buffer
        // so the next evaluation reuses its allocation instead of starting
        // from an empty encoding (PF-4: every GUI edit calls this).
        if let Some(entry) = self.eval_caches.frame_cache.borrow_mut().take() {
            *self.eval_caches.scene_buffer.borrow_mut() = Some(entry.program.scene);
        }
        // Mutations can add or remove `solo` (and add modifiers), so the
        // cached "does solo need a scan" answer is no longer trustworthy.
        self.eval_caches.solo_scan_needed.set(None);
        self.eval_caches.static_subtree_cache.borrow_mut().clear();
        self.eval_caches.static_subtree_flags.borrow_mut().clear();
        self.eval_caches.transform_cache.borrow_mut().clear();
        self.eval_caches.overrides_buffer.borrow_mut().clear();
        {
            // Flat-vec clear: no string keys to recycle (PF-6 slot-id table).
            self.eval_caches.precise_bounds.borrow_mut().clear_frame();
        }
        // Bounds slots are registry-stamped; a track mutation can change the
        // label set, so drop the registry and let it rebuild lazily.
        *self.eval_caches.bounds_registry.borrow_mut() = None;
        self.layout_children_cache.borrow_mut().clear();
        self.layout_engine.invalidate_cache();
        // Drop the pooled frame environment: timeline mutation may change the
        // injected key set (referenced roots, variable tracks, modifiers), and
        // every mutation funnels through here, so this single line restores
        // fresh-build semantics for the PF-6 env pool.
        *self.env_pool.borrow_mut() = None;
        // Per-track hot-path memos (vector-path Arc sharing) key on this
        // epoch: every mutation funnels through this invalidation, so the
        // bump is the single memo-invalidation point.
        for (_, id) in self.world.tracks.iter() {
            let Some(shape) = self.world.shape.try_get(id) else {
                continue;
            };
            shape.vector_paths_epoch.set(shape.vector_paths_epoch.get().wrapping_add(1));
            shape.shape_type_switches.set(None);
        }
    }

    /// The document's mutation epoch, current for this timeline. Two reads with
    /// no mutation between them return the same value; any edit that invalidates
    /// the scene cache also moves this, which is what makes it safe to name a
    /// rendered frame by it (`frame_signature::FrameSignature`).
    ///
    /// Note it is per-`Timeline`, not global: a newly built document restarts at
    /// 0, so a driver that can swap documents needs its own generation counter
    /// alongside it.
    pub fn content_epoch(&self) -> u64 {
        self.eval_caches.content_epoch.get()
    }

    /// Returns a reference to the audio segments collected during build.
    pub fn audio_segments(&self) -> &[AudioSegment] {
        &self.audio_segments
    }

    /// Return runtime diagnostics produced during the most recent frame
    /// evaluation (modifier errors, etc.). Empty if evaluation succeeded
    /// without issues.
    pub fn runtime_diagnostics(&self) -> Vec<crate::diagnostics::Diagnostic> {
        self.eval_caches.runtime_diagnostics.borrow().clone()
    }

    /// Clear runtime diagnostics. Called automatically at the start of each
    /// `evaluate()` call.
    pub fn clear_runtime_diagnostics(&self) {
        self.eval_caches.runtime_diagnostics.borrow_mut().clear();
    }

    /// Returns the name of the currently active colorscheme.
    pub fn colorscheme_name(&self) -> &str {
        &self.colorscheme.name
    }

    /// Returns the configured named export preset, if any.
    pub fn export_preset(&self) -> Option<&str> {
        self.export_preset.as_deref()
    }

    /// Returns the scene resolution declared via `config { resolution: (w, h) }`, if any.
    pub fn resolution(&self) -> Option<(u32, u32)> {
        self.resolution
    }

    /// Returns the appropriate default color for a primitive type and property,
    /// based on the current colorscheme.
    pub fn get_default_color(
        &self,
        primitive: &dyn crate::primitives::Primitive,
        caps: &ActorCaps,
        property: &str,
    ) -> Option<[f32; 4]> {
        self.colorscheme.default_color_for_primitive(primitive, caps, property)
    }

    /// Compute the world-space affine transform for a given actor at the given time.
    ///
    /// Walks the scene-graph from the root to the target actor, accumulating
    /// position (resolved through `resolve_bound_position`), rotation, scale,
    /// and motion offset — exactly matching the renderer's transform chain.
    pub fn actor_world_affine(
        &self,
        label: &str,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
    ) -> Option<kurbo::Affine> {
        let path = self.find_path_to_actor(label)?;

        let mut parent_transform = kurbo::Affine::IDENTITY;
        let mut current_layout_positions: Arc<layout::LayoutPositions> =
            layout::empty_layout_positions();

        for node_label in &path {
            let node_id = self.world.get_id(node_label)?;

            let placement_mode = self
                .world
                .geometry(node_id)
                .placement_mode
                .get(time_ms, PlacementMode::LayoutManaged);
            let mut base_position = self.world.geometry(node_id).position.get(time_ms, [0.0, 0.0]);

            if self.dynamic_layout {
                if let Some(layout_pos) = current_layout_positions
                    .get_by_id(node_id)
                    .or_else(|| current_layout_positions.get(node_label.as_str()))
                {
                    if placement_mode == PlacementMode::LayoutManaged {
                        base_position = *layout_pos;
                    }
                }
            }

            let binding = self
                .world
                .geometry(node_id)
                .position_binding
                .get(time_ms, PositionBinding::Absolute);
            let position =
                resolve_bound_position(binding, base_position, parent_transform, scene_dimensions);
            let motion_offset = self.world.geometry(node_id).motion_offset.get(time_ms, [0.0, 0.0]);
            let rotation = self.world.geometry(node_id).rotation.get(time_ms, 0.0) as f64;
            let scale = self.world.geometry(node_id).scale.get(time_ms, 1.0) as f64;

            parent_transform = parent_transform
                * kurbo::Affine::translate((
                    position[0] as f64 + motion_offset[0] as f64,
                    position[1] as f64 + motion_offset[1] as f64,
                ))
                * kurbo::Affine::rotate(rotation)
                * kurbo::Affine::scale(scale);

            // Compute layout positions for this node's children (needed for next iteration)
            if self.dynamic_layout {
                if let Some(metadata) = self.container_layout(node_label.as_str()) {
                    let layout_children = self.layout_children_for(node_label.as_str());
                    current_layout_positions = self.layout_engine.compute_layout_for_time(
                        node_label.as_str(),
                        metadata,
                        &layout_children,
                        time_ms,
                        &self.world,
                    );
                } else {
                    current_layout_positions = layout::empty_layout_positions();
                }
            }
        }

        Some(parent_transform)
    }

    /// Find the path of actor labels from a root node down to `target`.
    pub(crate) fn find_path_to_actor(&self, target: &str) -> Option<Vec<String>> {
        for &root in self.world.root_entities() {
            let label = self.world.label(root)?;
            if let Some(path) = self.find_path_from(label, target) {
                return Some(path);
            }
        }
        None
    }

    pub(crate) fn find_path_from(&self, current: &str, target: &str) -> Option<Vec<String>> {
        if current == target {
            return Some(vec![current.to_string()]);
        }
        let id = self.world.get_id(current)?;
        for &child in &self.world.hierarchy(id).children {
            let Some(child_label) = self.world.label(child) else {
                continue;
            };
            if let Some(mut path) = self.find_path_from(child_label, target) {
                path.insert(0, current.to_string());
                return Some(path);
            }
        }
        None
    }
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new()
    }
}
