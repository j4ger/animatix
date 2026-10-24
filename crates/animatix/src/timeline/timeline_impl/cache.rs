use crate::timeline::*;

impl Timeline {
    pub(crate) fn collect_frame_written_vars(
        modifiers: &[Stmt],
    ) -> std::collections::HashSet<String> {
        fn walk(stmts: &[Stmt], out: &mut std::collections::HashSet<String>) {
            for stmt in stmts {
                match stmt {
                    Stmt::Assignment {
                        target, property, ..
                    } if target.is_empty() => {
                        out.insert(property.clone());
                    },
                    Stmt::LetDecl { name, .. } => {
                        out.insert(name.clone());
                    },
                    Stmt::Conditional {
                        then_branch,
                        else_branch,
                        ..
                    } => {
                        walk(then_branch, out);
                        if let Some(else_branch) = else_branch {
                            walk(else_branch, out);
                        }
                    },
                    Stmt::ForLoop { body, .. } => walk(body, out),
                    _ => {}, // Only variable writes matter for the gate.
                }
            }
        }
        let mut out = std::collections::HashSet::new();
        walk(modifiers, &mut out);
        out
    }

    /// Returns true if any actor has a procedural plot that requires per-frame
    /// evaluation.  Static plots (no `t` reference, no animated params, no func
    /// transitions) always use their cached build-time paths and do not force a
    /// frame environment to be constructed.
    pub(crate) fn has_procedural_plots(&self) -> bool {
        self.world.tracks.values().any(|t| {
            self.world
                .procedural_plot(t.id)
                .is_some_and(|pp| pp.is_dynamic(&self.frame_written_vars))
                || !self.world.func_transitions(t.id).is_empty()
        })
    }

    /// Returns true if frame environment is needed for evaluation.
    /// Frame environment is needed when modifiers or procedural plots exist.
    pub(crate) fn needs_frame_env(&self) -> bool {
        !self.modifiers.is_empty()
            || !self.modifier_programs.is_empty()
            || self.has_procedural_plots()
    }

    /// Returns true if the actor and all its descendants have no keyframes
    /// and the timeline has no modifiers that could affect them.
    /// Fully-static subtrees can have their rendered output cached (P2.17).
    /// Stamp [`AnimationTrack::bounds_slot`] values and (re)build the label
    /// registry if invalid. See [`EvalCaches::ensure_bounds_registry`].
    pub(crate) fn ensure_bounds_registry(&self) {
        self.eval_caches.ensure_bounds_registry(&self.world.tracks);
    }

    /// Precise world-space bounds for `label` recorded during the current
    /// frame, if any. Lookup path for [`TargetResolver::target_bounds`];
    /// point queries are rare (a few targeted callouts/lines/arrows per
    /// frame), so a binary search over the sorted registry is plenty.
    pub(crate) fn precise_bounds_for(&self, label: &str) -> Option<kurbo::Rect> {
        self.ensure_bounds_registry();
        let registry = self.eval_caches.bounds_registry.borrow().clone()?;
        let slot = registry.labels.binary_search_by(|probe| probe.as_str().cmp(label)).ok()?;
        self.eval_caches.precise_bounds.borrow().slots.get(slot).copied().flatten()
    }

    /// Record `rect` as the world-space bounds of the node at `stamped_slot`
    /// (from [`AnimationTrack::bounds_slot`]) during the current frame.
    ///
    /// `label` is only consulted on the slow path — when the slot was never
    /// stamped (track added mid-frame without the invalidation funnel) — to
    /// rebuild the registry once and retry.
    pub(crate) fn record_precise_bounds(&self, stamped_slot: u32, label: &str, rect: kurbo::Rect) {
        let mut slot = stamped_slot;
        if slot == u32::MAX {
            self.ensure_bounds_registry();
            slot = self.world.tracks.get(label).map(|t| t.bounds_slot.get()).unwrap_or(u32::MAX);
            if slot == u32::MAX {
                // Unreachable while the invalidation funnel holds; log rather
                // than silently dropping the bounds.
                tracing::warn!("no bounds slot for track '{label}'; bounds dropped for this frame");
                return;
            }
        }
        self.eval_caches.precise_bounds.borrow_mut().write(slot, rect);
    }

    /// Recompute [`Timeline::blend_used`] from the tracks. Called from
    /// `invalidate_frame_cache` (every public mutation funnels through it)
    /// and once at the end of the build walk.
    /// Length of one beat in milliseconds: the scene's `config { bpm: … }` when
    /// declared, the documented default tempo otherwise.
    pub(crate) fn beat_duration_ms(&self) -> f64 {
        self.beat_ms
            .unwrap_or(crate::ast::beat_seconds(crate::ast::DEFAULT_BPM) * 1000.0)
    }

    pub(crate) fn refresh_blend_used(&self) {
        let any = self.world.tracks.values().any(|t| t.style.blend.is_some());
        self.blend_used.set(any);
    }

    /// Recompute [`Timeline::camera_used`].
    ///
    /// A keyframed `camera.zoom = …` shows up on the camera tracks, but an
    /// `always { camera.at = … }` never touches them — it writes the frame
    /// override map — so the modifier statements have to be asked as well.
    pub(crate) fn refresh_camera_used(&self) {
        self.camera_used
            .set(self.camera.is_authored() || Self::stmts_address_camera(&self.modifiers));
        self.camera_exempt_present.set(self.world.tracks.values().any(|track| {
            self.world
                .meta
                .get(track.id)
                .is_some_and(|meta| !meta.camera_follow || meta.parallax != 1.0)
        }));
    }

    pub(crate) fn stmts_address_camera(stmts: &[crate::ast::Stmt]) -> bool {
        use crate::ast::Stmt;
        stmts.iter().any(|stmt| match stmt {
            Stmt::Assignment { target, .. } => target
                .first()
                .is_some_and(|segment| segment.label_str() == camera::CAMERA_TARGET),
            Stmt::Always { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Block { body, .. } => Self::stmts_address_camera(body),
            _ => false, // Every other statement form is a leaf for camera purposes.
        })
    }

    pub(crate) fn is_static_subtree_id(&self, id: EntityId) -> bool {
        let idx = id.index();
        if let Some(&Some(cached)) = self.eval_caches.static_subtree_flags.borrow().get(idx) {
            return cached;
        }
        let Some(track) = self.world.tracks.get_by_id(id) else {
            return true;
        };
        let is_static = self.compute_static_subtree(&track.label);
        let mut flags = self.eval_caches.static_subtree_flags.borrow_mut();
        if idx >= flags.len() {
            flags.resize(idx + 1, None);
        }
        flags[idx] = Some(is_static);
        is_static
    }

    /// Returns `true` if `label` is targeted by any modifier assignment (`Assign` or
    /// `AssignIndexed`).
    pub(crate) fn is_targeted_by_modifier(&self, label: &str) -> bool {
        if self.modifier_written_targets.contains(label) {
            return true;
        }
        for target in &self.modifier_written_targets {
            if let Some(rest) = label.strip_prefix(target.as_str()) {
                if rest.starts_with("__") || rest.starts_with('.') {
                    return true;
                }
            }
        }
        false
    }

    /// Uncached static-subtree computation ([`Self::is_static_subtree`] wraps
    /// this in a memo).
    pub(crate) fn compute_static_subtree(&self, label: &str) -> bool {
        // Animated child orders live in a component, outside `AnimationTrack`
        // keyframe detection, and can change layout output, so they disable
        // static caching.
        if self.world.get_id(label).is_some_and(|id| self.world.child_order.contains(id)) {
            return false;
        }
        // If this actor is written to by any modifier (always block), it changes
        // at frame time and cannot be statically cached.
        if self.is_targeted_by_modifier(label) {
            return false;
        }
        let Some(track) = self.world.tracks.get(label) else {
            return true;
        };
        if track.caps.time_dependent
            || self.world.transcript.contains(track.id)
            || self.world.effects(track.id).is_some_and(|chain| chain.has_any_keyframes())
            || track.has_any_keyframes()
            || self.world.procedural_plot(track.id).is_some()
            || track.shape.from_anchor.is_some()
            || track.shape.to_anchor.is_some()
        {
            return false;
        }
        self.world
            .hierarchy(track.id)
            .children
            .iter()
            .all(|child_id| self.is_static_subtree_id(*child_id))
    }

    #[cfg(test)]
    pub(crate) fn has_static_subtree_cached(
        &self,
        label: &str,
        dimensions: SceneDimensions,
        collect_items: bool,
        debug_options: DebugRenderOptions,
    ) -> bool {
        let Some(id) = self.world.tracks.get_entity_id(label) else {
            return false;
        };
        self.eval_caches.static_subtree_cache.borrow().contains_key(&(
            id,
            dimensions,
            collect_items,
            debug_options,
        ))
    }
}
