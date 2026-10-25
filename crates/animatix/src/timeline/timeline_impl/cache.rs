use crate::timeline::*;

/// One label's frame-env injection decision, borrowing the referenced
/// property filter. The single source of truth for the decision is
/// [`Timeline::env_inject_decision`]; the cached [`EnvInjectionPlan`] and the
/// mid-build path both consume it.
pub(crate) enum EnvInjectDecision<'a> {
    /// Actor not referenced by any expression: no property injection.
    Skip,
    /// Referenced, but no property of it is read: only per-frame overrides
    /// are applied when present.
    NoProps,
    /// Full injection (wildcard target, or no referenced-roots analysis).
    Wildcard(bool),
    /// Filtered injection: only these property names are read.
    Props(&'a std::collections::HashSet<String>, bool),
}

/// Build-fixed per-entity decision for frame-env property injection,
/// derived once from the referenced-roots analysis (see
/// `Timeline::env_injection_plan`) instead of re-derived per frame.
#[derive(Clone)]
pub(crate) enum EnvInjectEntry {
    /// Actor not referenced by any expression: no property injection.
    Skip,
    /// Referenced, but no property of it is read: only per-frame overrides
    /// are applied when present.
    NoProps,
    /// Full injection (wildcard target, or no referenced-roots analysis).
    Wildcard { animating: bool },
    /// Filtered injection: only these property names are read.
    Props {
        allowed: std::sync::Arc<std::collections::HashSet<String>>,
        animating: bool,
    },
}

/// The precomputed frame-env injection plan (see `EnvInjectEntry`): one entry
/// per entity in `World::tracks` order, the pooled-env capacity estimate,
/// and whether runtime injection is needed at all.
pub(crate) struct EnvInjectionPlan {
    pub(crate) entries: Vec<EnvInjectEntry>,
    pub(crate) estimated_prop_entries: usize,
    pub(crate) runtime_injection: bool,
}

impl EnvInjectionPlan {
    /// The borrowing decision for `id`, or `None` for entities spawned after
    /// the plan was computed (unreachable through the invalidation funnel);
    /// callers fall back to a fresh [`Timeline::env_inject_decision`], whose
    /// answer for unknown labels errs toward over-injection.
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
    /// The per-label injection decision, recomputed on demand. The frame path
    /// serves this from the cached [`EnvInjectionPlan`]; the build path (where
    /// the referenced-roots analysis is still converging) calls it directly —
    /// the same cost the per-frame loop paid before the plan existed.
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

    /// Returns true if any actor has a procedural plot that requires per-frame
    /// evaluation.  Static plots (no `t` reference, no animated params, no func
    /// transitions) always use their cached build-time paths and do not force a
    /// frame environment to be constructed.
    pub(crate) fn has_procedural_plots(&self) -> bool {
        self.world.tracks.iter().any(|(_, id)| {
            self.world
                .procedural_plot(id)
                .is_some_and(|pp| pp.is_dynamic(&self.frame_written_vars))
                || !self.world.func_transitions(id).is_empty()
        })
    }

    /// Memoized [`Self::needs_frame_env`]: the underlying answer only changes
    /// with the timeline's compiled statements, and the raw computation walks
    /// every entity's payload — the frame path used to pay that walk up to
    /// three times per frame (restore probe, miss, env build). Cleared by
    /// `invalidate_frame_cache`.
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
    /// which entities participate in runtime injection, with which property
    /// filter, plus the pooled env's capacity estimate. Computed once from
    /// the referenced-roots analysis and cleared by `invalidate_frame_cache`,
    /// so the per-frame env build stops re-deriving wildcard membership
    /// (`format!` per label) and the capacity walk. Only valid once the build
    /// has converged — the build path itself calls
    /// [`Self::env_inject_decision`] per label instead.
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
        let runtime_injection = !self.modifier_programs.is_empty()
            || !self.modifiers.is_empty()
            || self.has_procedural_plots();
        EnvInjectionPlan {
            entries,
            estimated_prop_entries,
            runtime_injection,
        }
    }

    /// Returns true if the actor and all its descendants have no keyframes
    /// and the timeline has no modifiers that could affect them.
    /// Fully-static subtrees can have their rendered output cached (P2.17).
    /// Stamp `World::bounds_slots` values and (re)build the label
    /// registry if invalid. See [`EvalCaches::ensure_bounds_registry`].
    pub(crate) fn ensure_bounds_registry(&self) {
        self.eval_caches.ensure_bounds_registry(&self.world);
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
    /// (a `World::bounds_slots` slot id) during the current frame.
    ///
    /// `label` is only consulted on the slow path — when the slot was never
    /// stamped (track added mid-frame without the invalidation funnel) — to
    /// rebuild the registry once and retry.
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
        let any = self.world.style.iter().any(|(_, style)| style.blend.is_some());
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
        let idx = id.index();
        if let Some(&Some(cached)) = self.eval_caches.static_subtree_flags.borrow().get(idx) {
            return cached;
        }
        if !self.world.exists(id) {
            return true;
        }
        let is_static = self.compute_static_subtree(id);
        let mut flags = self.eval_caches.static_subtree_flags.borrow_mut();
        if idx >= flags.len() {
            flags.resize(idx + 1, None);
        }
        flags[idx] = Some(is_static);
        is_static
    }

    /// Whether the entity's transform inputs are time-invariant: no keyframes
    /// on any of its tracks (any tier). Such entities' node transforms depend
    /// only on parent state and layout position, so the transform cache can
    /// reuse them across times instead of re-sampling the spatial properties
    /// every frame. Memoized like `is_static_subtree_id`; the same
    /// invalidation funnel clears it.
    pub(crate) fn is_transform_static(&self, id: EntityId) -> bool {
        let idx = id.index();
        if let Some(&Some(cached)) = self.eval_caches.transform_static_flags.borrow().get(idx) {
            return cached;
        }
        let is_static = self.world.exists(id) && !self.world.has_any_keyframes(id);
        let mut flags = self.eval_caches.transform_static_flags.borrow_mut();
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

    /// Uncached static-subtree computation ([`Self::is_static_subtree_id`]
    /// wraps this in a memo). The label is borrowed only for the
    /// modifier-target check — no `String` is allocated per node.
    pub(crate) fn compute_static_subtree(&self, id: EntityId) -> bool {
        if !self.world.exists(id) {
            return true;
        }
        // Animated child orders live in a component, outside property keyframe
        // detection, and can change layout output, so they disable
        // static caching.
        if self.world.child_order.contains(id) {
            return false;
        }
        // If this actor is written to by any modifier (always block), it changes
        // at frame time and cannot be statically cached.
        if self.world.label(id).is_some_and(|label| self.is_targeted_by_modifier(label)) {
            return false;
        }
        if self.world.caps(id).time_dependent
            || self.world.transcript.contains(id)
            || self.world.effects(id).is_some_and(|chain| chain.has_any_keyframes())
            || self.world.has_any_keyframes(id)
            || self.world.procedural_plot(id).is_some()
            || self.world.shape.try_get(id).is_some_and(|shape| shape.from_anchor.is_some())
            || self.world.shape.try_get(id).is_some_and(|shape| shape.to_anchor.is_some())
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
