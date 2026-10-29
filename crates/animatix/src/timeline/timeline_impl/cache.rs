use crate::timeline::*;

/// One label's frame-env injection decision, borrowing the referenced property
/// filter; [`Timeline::env_inject_decision`] is the single source of truth.
pub(crate) enum EnvInjectDecision<'a> {
    /// Actor not referenced by any expression: no property injection.
    Skip,
    /// Referenced but no property read: only per-frame overrides apply.
    NoProps,
    /// Full injection (wildcard target, or no referenced-roots analysis).
    Wildcard(bool),
    /// Filtered injection: only these property names are read.
    Props(&'a std::collections::HashSet<String>, bool),
}

/// Build-fixed per-entity decision for frame-env property injection, derived
/// once from the referenced-roots analysis.
#[derive(Clone)]
pub(crate) enum EnvInjectEntry {
    /// Actor not referenced by any expression: no property injection.
    Skip,
    /// Referenced but no property read: only per-frame overrides apply.
    NoProps,
    /// Full injection (wildcard target, or no referenced-roots analysis).
    Wildcard { animating: bool },
    /// Filtered injection: only these property names are read.
    Props {
        allowed: std::sync::Arc<std::collections::HashSet<String>>,
        animating: bool,
    },
}

/// The precomputed frame-env injection plan: one entry per entity in
/// `World::tracks` order plus the pooled-env capacity estimate;
/// [`Timeline::needs_frame_env`] is the memoized answer both consumers share.
pub(crate) struct EnvInjectionPlan {
    pub(crate) entries: Vec<EnvInjectEntry>,
    pub(crate) estimated_prop_entries: usize,
}

impl EnvInjectionPlan {
    /// The borrowing decision for `id`, or `None` for entities spawned after
    /// the plan was computed (unreachable via the invalidation funnel); falls
    /// back to [`Timeline::env_inject_decision`].
    pub(crate) fn decision(&self, id: EntityId) -> Option<EnvInjectDecision<'_>> {
        Some(match self.entries.get(id.index())? {
            EnvInjectEntry::Skip => EnvInjectDecision::Skip,
            EnvInjectEntry::NoProps => EnvInjectDecision::NoProps,
            EnvInjectEntry::Wildcard { animating } => EnvInjectDecision::Wildcard(*animating),
            EnvInjectEntry::Props { allowed, animating } => {
                EnvInjectDecision::Props(allowed.as_ref(), *animating)
            },
        })
    }
}

impl Timeline {
    /// The per-label injection decision, recomputed on demand: the frame path
    /// serves it from [`EnvInjectionPlan`], the build path calls it directly.
    pub(crate) fn env_inject_decision(&self, label: &str) -> EnvInjectDecision<'_> {
        let Some(roots) = &self.referenced_roots else {
            // No referenced-roots analysis: every track injects fully.
            return EnvInjectDecision::Wildcard(true);
        };
        if !roots.contains(label) {
            return EnvInjectDecision::Skip;
        }
        // A wildcard actor or any `{wildcard}__`-prefixed label injects fully.
        let is_wildcard = self.wildcard_actors.contains(label)
            || self.wildcard_actors.iter().any(|w| label.starts_with(&format!("{w}__")));
        if is_wildcard {
            return EnvInjectDecision::Wildcard(self.is_animating_used);
        }
        let base = label.split_once("__").map(|(b, _)| b).unwrap_or(label);
        match self
            .referenced_properties
            .get(label)
            .or_else(|| self.referenced_properties.get(base))
        {
            Some(p) => EnvInjectDecision::Props(p, self.is_animating_used),
            None => EnvInjectDecision::NoProps,
        }
    }

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

    /// Returns true if any actor has a procedural plot requiring per-frame
    /// evaluation; static plots use cached build-time paths.
    pub(crate) fn has_procedural_plots(&self) -> bool {
        self.world.tracks.iter().any(|(_, id)| {
            self.world
                .procedural_plot(id)
                .is_some_and(|pp| pp.is_dynamic(&self.frame_written_vars))
                || !self.world.func_transitions(id).is_empty()
        })
    }

    /// Memoized [`Self::needs_frame_env`]: computing it walks every entity, so
    /// the answer is cached until `invalidate_frame_cache`.
    pub(crate) fn needs_frame_env(&self) -> bool {
        if let Some(cached) = self.eval_caches.needs_frame_env.get() {
            return cached;
        }
        let needed = !self.modifiers.is_empty()
            || !self.modifier_programs.is_empty()
            || self.has_procedural_plots();
        self.eval_caches.needs_frame_env.set(Some(needed));
        needed
    }

    /// The build-fixed frame-env injection plan (see [`EnvInjectionPlan`]):
    /// which entities inject, with which property filter, plus the pooled-env
    /// capacity estimate. Cleared by `invalidate_frame_cache`, so the per-frame
    /// env build avoids per-label `format!` wildcard checks; valid only after
    /// the build converges.
    pub(crate) fn env_injection_plan(&self) -> std::sync::Arc<EnvInjectionPlan> {
        if let Some(plan) = self.eval_caches.env_injection_plan.borrow().clone() {
            return plan;
        }
        let plan = std::sync::Arc::new(self.compute_env_injection_plan());
        *self.eval_caches.env_injection_plan.borrow_mut() = Some(std::sync::Arc::clone(&plan));
        plan
    }

    fn compute_env_injection_plan(&self) -> EnvInjectionPlan {
        let mut entries: Vec<EnvInjectEntry> = Vec::with_capacity(self.world.tracks.len());
        let mut estimated_prop_entries = 0usize;
        for (label, _) in self.world.tracks.iter() {
            let entry = match self.env_inject_decision(label) {
                EnvInjectDecision::Skip => EnvInjectEntry::Skip,
                EnvInjectDecision::NoProps => EnvInjectEntry::NoProps,
                EnvInjectDecision::Wildcard(animating) => {
                    estimated_prop_entries += 120;
                    EnvInjectEntry::Wildcard { animating }
                },
                EnvInjectDecision::Props(allowed, animating) => {
                    estimated_prop_entries += allowed.len() * 4;
                    EnvInjectEntry::Props {
                        allowed: std::sync::Arc::new(allowed.clone()),
                        animating,
                    }
                },
            };
            entries.push(entry);
        }
        debug_assert_eq!(
            entries.len(),
            self.world.tracks.len(),
            "the injection plan must cover every entity (a spawn after the plan was \
             computed is unreachable through the invalidation funnel)"
        );
        EnvInjectionPlan {
            entries,
            estimated_prop_entries,
        }
    }

    /// Stamp `World::bounds_slots` and (re)build the label registry if invalid.
    pub(crate) fn ensure_bounds_registry(&self) {
        self.eval_caches.ensure_bounds_registry(&self.world);
    }

    /// World-space bounds for `label` recorded during the current frame, if any.
    /// Point queries are rare, so a binary search over the sorted registry is plenty.
    pub(crate) fn precise_bounds_for(&self, label: &str) -> Option<kurbo::Rect> {
        self.ensure_bounds_registry();
        let registry = self.eval_caches.bounds_registry.borrow().clone()?;
        let slot = registry.labels.binary_search_by(|probe| probe.as_str().cmp(label)).ok()?;
        self.eval_caches.precise_bounds.borrow().slots.get(slot).copied().flatten()
    }

    /// Record `rect` as the world-space bounds of the node at `stamped_slot`
    /// during the current frame; `label` is consulted only on the slow path.
    pub(crate) fn record_precise_bounds(&self, stamped_slot: u32, label: &str, rect: kurbo::Rect) {
        let mut slot = stamped_slot;
        if slot == u32::MAX {
            self.ensure_bounds_registry();
            slot = self
                .world
                .tracks
                .get_id(label)
                .map_or(u32::MAX, |id| self.world.bounds_slot(id));
            if slot == u32::MAX {
                // Unreachable while the invalidation funnel holds; log rather
                // than silently dropping the bounds.
                tracing::warn!("no bounds slot for track '{label}'; bounds dropped for this frame");
                return;
            }
        }
        self.eval_caches.precise_bounds.borrow_mut().write(slot, rect);
    }

    /// One beat in milliseconds: `config { bpm: … }` when declared, else the default tempo.
    pub(crate) fn beat_duration_ms(&self) -> f64 {
        self.beat_ms
            .unwrap_or(crate::ast::beat_seconds(crate::ast::DEFAULT_BPM) * 1000.0)
    }

    /// Recompute [`Timeline::blend_used`]; called from `invalidate_frame_cache`
    /// and at the end of the build walk.
    pub(crate) fn refresh_blend_used(&self) {
        let any = self.world.style.iter().any(|(_, style)| style.blend.is_some());
        self.blend_used.set(any);
    }

    /// Recompute [`Timeline::camera_used`]. `always { camera.at = … }` only
    /// writes the frame override map, so the modifier statements must be asked.
    pub(crate) fn refresh_camera_used(&self) {
        self.camera_used
            .set(self.camera.is_authored() || Self::stmts_address_camera(&self.modifiers));
        self.camera_exempt_present.set(self.world.tracks.iter().any(|(_, id)| {
            self.world
                .meta
                .get(id)
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
        if let Some(cached) = self.eval_caches.subtree_static(id) {
            return cached;
        }
        if !self.world.exists(id) {
            return true;
        }
        let is_static = self.compute_static_subtree(id);
        self.eval_caches.set_subtree_static(id, is_static);
        is_static
    }

    /// Whether the entity's transform inputs are time-invariant (no keyframes on
    /// any track), so the cache reuses them across times. Memoized; cleared by
    /// the invalidation funnel.
    pub(crate) fn is_transform_static(&self, id: EntityId) -> bool {
        if let Some(cached) = self.eval_caches.transform_static(id) {
            return cached;
        }
        let is_static = self.world.exists(id) && !self.world.has_any_keyframes(id);
        self.eval_caches.set_transform_static(id, is_static);
        is_static
    }

    /// Returns `true` if `label` is targeted by any modifier assignment (`Assign` or `AssignIndexed`).
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

    /// Uncached static-subtree computation ([`Self::is_static_subtree_id`] wraps it in a memo).
    pub(crate) fn compute_static_subtree(&self, id: EntityId) -> bool {
        if !self.world.exists(id) {
            return true;
        }
        // Animated child orders can change layout outside keyframe detection.
        if self.world.child_order.contains(id) {
            return false;
        }
        // A modifier writes this actor at frame time, so it cannot be cached.
        if self.world.label(id).is_some_and(|label| self.is_targeted_by_modifier(label)) {
            return false;
        }
        if self.world.caps(id).time_dependent
            || self.world.transcript.contains(id)
            || self.world.effects(id).is_some_and(|chain| chain.has_any_keyframes())
            || self.world.has_any_keyframes(id)
            || self.world.procedural_plot(id).is_some()
            || self.world.shape.get(id).is_some_and(|shape| shape.from_anchor.is_some())
            || self.world.shape.get(id).is_some_and(|shape| shape.to_anchor.is_some())
        {
            return false;
        }
        self.world
            .hierarchy(id)
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
        let Some(id) = self.world.get_id(label) else {
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

impl EvalCaches {
    /// Reset every per-frame cache, keeping allocations worth recycling: the
    /// encoded scene is recycled, per-entity tables and the bounds registry are
    /// cleared, and build-fixed memos drop so they re-derive from the world.
    ///
    /// The caller advances the content epoch and drops the pooled frame
    /// environment because the injected key set may have changed.
    pub(crate) fn clear(&self) {
        // Recycle the encoded scene so the next evaluation reuses its allocation
        // (every GUI edit calls this).
        if let Some(entry) = self.frame_cache.borrow_mut().take() {
            *self.scene_buffer.borrow_mut() = Some(entry.program.scene);
        }
        self.transform_cache.borrow_mut().clear();
        self.static_flags.borrow_mut().clear();
        self.static_subtree_cache.borrow_mut().clear();
        // Hit regions describe the last evaluated frame; an edit makes them stale.
        self.hit_regions.borrow_mut().clear();
        self.overrides_buffer.borrow_mut().clear();
        // Flat-vec clear: no string keys to recycle (slot-id table).
        self.precise_bounds.borrow_mut().clear_frame();
        // A track mutation can change the label set; drop the registry, rebuild lazily.
        *self.bounds_registry.borrow_mut() = None;
        // The env-path answers are as build-fixed as the ones above.
        self.needs_frame_env.set(None);
        *self.env_injection_plan.borrow_mut() = None;
        // Mutations can add or remove `solo` (and modifiers), so the scan answer is stale.
        self.solo_scan_needed.set(None);
        self.primitive_handles.borrow_mut().clear();
    }

    /// Memoized `is_static_subtree_id` answer for `id`, when computed.
    pub(crate) fn subtree_static(&self, id: EntityId) -> Option<bool> {
        self.static_flags.borrow().get(id.index()).and_then(|flags| flags.subtree)
    }

    /// Record the `is_static_subtree_id` answer for `id`.
    pub(crate) fn set_subtree_static(&self, id: EntityId, value: bool) {
        self.update_static_flags(id, |flags| flags.subtree = Some(value));
    }

    /// Memoized `is_transform_static` answer for `id`, when computed.
    pub(crate) fn transform_static(&self, id: EntityId) -> Option<bool> {
        self.static_flags.borrow().get(id.index()).and_then(|flags| flags.transform)
    }

    /// Record the `is_transform_static` answer for `id`.
    pub(crate) fn set_transform_static(&self, id: EntityId, value: bool) {
        self.update_static_flags(id, |flags| flags.transform = Some(value));
    }

    fn update_static_flags(&self, id: EntityId, update: impl FnOnce(&mut EntityStaticFlags)) {
        let mut flags = self.static_flags.borrow_mut();
        let index = id.index();
        if index >= flags.len() {
            flags.resize(index + 1, EntityStaticFlags::default());
        }
        update(&mut flags[index]);
    }
}
