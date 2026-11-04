//! Frame evaluation and program/frame-cache entry points.

use super::*;

/// Mutable state threaded through the frame-evaluation phases
/// (`prepare_frame` → `run_frame_modifiers` → `render_frame_roots` →
/// `store_frame`).
///
/// These are the same locals the monolithic evaluation used, in the same
/// order; the split is mechanical.
pub(super) struct FrameEvaluation<'a> {
    pub(super) time_ms: u64,
    pub(super) scene_dimensions: SceneDimensions,
    pub(super) debug_options: DebugRenderOptions,
    pub(super) collect_items: bool,
    pub(super) suppressed_actors: &'a [String],
    pub(super) transient_overrides:
        &'a std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    pub(super) needs_frame_env: bool,
    pub(super) has_child_orders: bool,
    pub(super) bg_color: [f32; 4],
    pub(super) scene: vello::Scene,
    pub(super) hit_regions: Vec<(EntityId, kurbo::Rect)>,
    pub(super) program_items: Option<Vec<crate::timeline::scene_program::SceneItem>>,
    pub(super) overrides: crate::timeline::modifier_runtime::ir::ModifierOverrides,
    pub(super) frame_env: Option<crate::timeline::env::Environment>,
}

impl Timeline {
    /// Evaluate the timeline at `time_s` and build the renderable scene at
    /// the requested dimensions.
    pub fn evaluate(&self, time_s: f64, scene_dimensions: SceneDimensions) -> vello::Scene {
        let mut fb = None;
        self.evaluate_with_debug(time_s, scene_dimensions, DebugRenderOptions::default(), &mut fb)
    }

    /// Evaluate the timeline with optional debug overlays.
    pub fn evaluate_with_debug(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) -> vello::Scene {
        if let Some(program) =
            self.restore_frame_cache(time_s, scene_dimensions, debug_options, filter_backend, false)
        {
            return program.scene;
        }
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            false,
            &[],
            &std::collections::HashMap::new(),
        )
        .scene
    }

    /// Evaluate the timeline with optional debug overlays and transient property overrides.
    pub fn evaluate_with_transient_overrides(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        transient_overrides: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        >,
    ) -> vello::Scene {
        if transient_overrides.is_empty() {
            if let Some(program) = self.restore_frame_cache(
                time_s,
                scene_dimensions,
                debug_options,
                filter_backend,
                false,
            ) {
                return program.scene;
            }
        }
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            false,
            &[],
            transient_overrides,
        )
        .scene
    }

    /// Evaluate the timeline straight into a [`vello::Scene`] for presentation.
    ///
    /// This is the web player's hot path. Unlike [`Self::evaluate_with_debug`]
    /// it never writes `eval_caches.frame_cache` (which is keyed on `time_ms`
    /// and so misses on every animation frame anyway) and never clones the
    /// encoded scene: the scene is moved out to the caller. A single reusable
    /// buffer is parked back via [`Self::recycle_presentation_scene`] after
    /// rasterizing, reusing the existing `scene_buffer` / `overrides_buffer` /
    /// `env_pool` recycling so steady-state frames allocate nothing.
    ///
    /// The clone-free behaviour is opt-in by calling this method explicitly;
    /// GUI, export, CLI and test entry points keep their exact existing
    /// semantics (they still publish/restore the frame cache).
    pub fn evaluate_for_presentation(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) -> vello::Scene {
        let empty_overrides = std::collections::HashMap::new();
        let mut state = self.prepare_frame(
            time_s,
            scene_dimensions,
            DebugRenderOptions::default(),
            filter_backend,
            false,
            &[],
            &empty_overrides,
            true,
        );
        self.run_frame_modifiers(&mut state);
        self.render_frame_roots(&mut state, filter_backend);
        self.store_frame_presentation(state)
    }

    /// Park a presentation scene back for reuse on the next frame.
    ///
    /// `reset()` keeps the encoding buffer's allocation, so
    /// [`Self::evaluate_for_presentation`] picks it up from `scene_buffer` and
    /// re-encodes in place instead of allocating a fresh `vello::Scene`. Call
    /// this after the scene has been rasterized; a caller that skips it (e.g.
    /// an early error return) simply makes the next frame allocate once.
    pub fn recycle_presentation_scene(&self, mut scene: vello::Scene) {
        scene.reset();
        *self.eval_caches.scene_buffer.borrow_mut() = Some(scene);
    }

    /// Whether a supplied filter backend could actually be consulted while
    /// building a frame.
    ///
    /// The backend is only reached by effect-scope actors, so its mere presence
    /// is not a reason to skip the frame cache — a runtime that always supplies
    /// one (the web player passes it on every frame) would otherwise never
    /// cache anything. Debug options are a separate matter: they change what is
    /// drawn, so they always block.
    ///
    /// This stays the conservative *whole-frame* gate for `restore_frame_cache`
    /// and `store_frame`: a cached frame is one vello Scene, and a frame that
    /// consulted the backend carries composites in the backend's pending side
    /// channel that such a Scene cannot reproduce. Recovering those per frame
    /// is not feasible, so the frame cache is simply off for filtered scenes.
    /// The static-subtree cache is a finer, per-root decision and does refine
    /// this (see `frame_render::render_frame_roots` / `subtree_has_effect_scope`).
    pub(super) fn backend_can_run(
        &self,
        filter_backend: &Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) -> bool {
        filter_backend.is_some() && self.has_effect_scopes
    }

    /// Restore cache-derived frame state when a frame cache entry matches.
    fn restore_frame_cache(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        collect_items: bool,
    ) -> Option<crate::timeline::scene_program::SceneProgram> {
        if self.backend_can_run(filter_backend) || debug_options != DebugRenderOptions::default() {
            return None;
        }
        let time_ms = (time_s * 1000.0) as u64;
        let needs_frame_env = self.needs_frame_env();
        let has_child_orders = self.world.has_child_orders();
        let cached = self.eval_caches.frame_cache.borrow();
        let cached = cached.as_ref()?;
        if cached.time_ms != time_ms
            || cached.dimensions != scene_dimensions
            || cached.has_modifiers != needs_frame_env
            || cached.has_dynamic_layout != self.dynamic_layout
            || cached.has_child_orders != has_child_orders
            || cached.collect_items != collect_items
        {
            return None;
        }
        // Restore bounds from the frame-cache entry. Scene-only entries stash
        // flat `(slot, rect)` pairs, so the restore is a plain `Vec` copy into
        // the slot table — no string keys, no per-key allocation. Observable
        // (collect_items) entries leave the pairs empty (as before, the bounds
        // live on the cloned program); their rare hits re-derive slots from the
        // label map by binary search over the registry.
        {
            let mut table = self.eval_caches.precise_bounds.borrow_mut();
            table.clear_frame();
            if collect_items {
                drop(table);
                self.ensure_bounds_registry();
                let pairs: Vec<(u32, kurbo::Rect)> = {
                    let registry = self.eval_caches.bounds_registry.borrow();
                    cached
                        .program
                        .precise_bounds
                        .iter()
                        .filter_map(|(label, rect)| {
                            let slot = registry
                                .as_ref()?
                                .labels
                                .binary_search_by(|probe| probe.as_str().cmp(label.as_str()))
                                .ok()? as u32;
                            Some((slot, *rect))
                        })
                        .collect()
                };
                let mut table = self.eval_caches.precise_bounds.borrow_mut();
                for (slot, rect) in pairs {
                    table.write(slot, rect);
                }
            } else {
                for &(slot, rect) in &cached.precise_bounds {
                    table.write(slot, rect);
                }
            }
        }
        *self.eval_caches.runtime_diagnostics.borrow_mut() = cached.program.diagnostics.clone();
        self.eval_caches.hit_regions.borrow_mut().clear();
        // On the scene-only path (no observable item collection) hand back a thin
        // program: only the authoritative scene is deep-copied; the per-frame
        // items/bounds/diagnostics vectors are not re-cloned on every hit. The
        // bounds/diagnostics are still mirrored into the caches above for any
        // tooling, and the collect_items path below keeps the full program.
        if collect_items {
            Some(cached.program.clone())
        } else {
            Some(crate::timeline::scene_program::SceneProgram {
                dimensions: cached.dimensions,
                background: cached.program.background,
                scene: cached.program.scene.clone(),
                items: Vec::new(),
                suppressed_items: Vec::new(),
                precise_bounds: std::collections::HashMap::new(),
                diagnostics: Vec::new(),
            })
        }
    }

    /// Evaluate the timeline into an observable [`crate::timeline::scene_program::SceneProgram`].
    ///
    /// The program carries the authoritative encoded scene plus the structured
    /// frame data used by tooling/tests. Filter and mask paths remain encoded
    /// directly into the authoritative scene; ordinary primitive actors are also
    /// collected as [`SceneItem`](crate::timeline::scene_program::SceneItem)s.
    pub fn evaluate_program_with_debug(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) -> crate::timeline::scene_program::SceneProgram {
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            true,
            &[],
            &std::collections::HashMap::new(),
        )
    }

    /// Evaluates scene program while suppressing specified actor labels (e.g. for FLIP
    /// transitions).
    pub fn evaluate_program_with_suppressed(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        suppressed_actors: &[String],
    ) -> crate::timeline::scene_program::SceneProgram {
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            true,
            suppressed_actors,
            &std::collections::HashMap::new(),
        )
    }

    /// Evaluates scene program with transient property overrides (e.g. interactive inspector sliders).
    pub fn evaluate_program_with_transient_overrides(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        transient_overrides: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        >,
    ) -> crate::timeline::scene_program::SceneProgram {
        self.evaluate_program_inner(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            true,
            &[],
            transient_overrides,
        )
    }

    fn evaluate_program_inner(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        collect_items: bool,
        suppressed_actors: &[String],
        transient_overrides: &std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        >,
    ) -> crate::timeline::scene_program::SceneProgram {
        if suppressed_actors.is_empty() && transient_overrides.is_empty() {
            if let Some(program) = self.restore_frame_cache(
                time_s,
                scene_dimensions,
                debug_options,
                filter_backend,
                collect_items,
            ) {
                return program;
            }
        }
        // The four phases below are the original monolithic body in order:
        // prepare (buffers, per-frame state, env) → modifiers → render →
        // store (program + frame cache).
        let mut state = self.prepare_frame(
            time_s,
            scene_dimensions,
            debug_options,
            filter_backend,
            collect_items,
            suppressed_actors,
            transient_overrides,
            false,
        );
        self.run_frame_modifiers(&mut state);
        self.render_frame_roots(&mut state, filter_backend);
        self.store_frame(state, filter_backend)
    }

    /// Prepare the frame: reuse the encoded-scene buffers, reset per-frame
    /// state, stamp bounds slots, resolve solo/hit-region scratch, and build
    /// the frame env.
    fn prepare_frame<'a>(
        &self,
        time_s: f64,
        scene_dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
        collect_items: bool,
        suppressed_actors: &'a [String],
        transient_overrides: &'a std::collections::HashMap<
            String,
            std::collections::HashMap<String, Value>,
        >,
        presentation: bool,
    ) -> FrameEvaluation<'a> {
        let time_ms = (time_s * 1000.0) as u64;
        let needs_frame_env = self.needs_frame_env();
        let has_child_orders = self.world.has_child_orders();

        // Clear stale runtime diagnostics from previous frame.
        self.clear_runtime_diagnostics();

        // Reuse a previously encoded scene buffer instead of allocating.
        // On a plain miss the replaced frame-cache entry is stale by
        // definition (restore_frame_cache just rejected it), so its encoded
        // scene is moved out — no clone — and becomes this frame's encode
        // buffer. Filter/debug frames bypass the frame cache (its entry stays
        // valid), so they recycle via scene_buffer instead.
        // `presentation` frames never consult or publish the frame cache, so
        // they always recycle their encode target from `scene_buffer` (the
        // buffer parked by `recycle_presentation_scene`), leaving frame_cache
        // untouched for the GUI/export paths.
        let debug_or_filter = presentation
            || self.backend_can_run(filter_backend)
            || debug_options != DebugRenderOptions::default()
            || !transient_overrides.is_empty();
        let mut scene = if debug_or_filter {
            self.eval_caches.scene_buffer.borrow_mut().take().unwrap_or_default()
        } else {
            self.eval_caches
                .frame_cache
                .borrow_mut()
                .take()
                .map(|entry| entry.program.scene)
                .unwrap_or_else(|| {
                    self.eval_caches.scene_buffer.borrow_mut().take().unwrap_or_default()
                })
        };
        scene.reset();
        // Precise bounds are only valid for the frame that computed them.
        // Slot-id table: sparse clear of the previous frame's slots, then
        // stamp every track's slot for this frame (registry rebuilds lazily
        // after invalidation).
        self.eval_caches.precise_bounds.borrow_mut().clear_frame();
        self.ensure_bounds_registry();
        let bg_color = self.background_color.evaluate_copy(time_ms);
        // Share the frame's background color with the per-node primitive
        // EvaluateCtx (legend label-contrast) without re-sampling the constant
        // background track once per node.
        self.eval_caches.background_color.set(bg_color);
        let solo_state = self.resolve_solo_state(time_ms);
        self.eval_caches.solo_active.set(solo_state.is_active());
        self.eval_caches.solo.replace(solo_state);

        // Collect actor world-space bounding boxes for click-to-select
        let mut hit_regions: Vec<(EntityId, kurbo::Rect)> = if debug_options.compute_hit_regions {
            self.eval_caches.hit_regions.take()
        } else {
            Vec::new()
        };
        hit_regions.clear();

        let mut overrides = self.eval_caches.overrides_buffer.take();
        for inner in overrides.values_mut() {
            inner.clear();
        }
        for label in suppressed_actors {
            overrides
                .entry(label.clone())
                .or_default()
                .insert("opacity".to_string(), Value::Num(0.0));
        }
        for (label, props) in transient_overrides {
            let entry = overrides.entry(label.clone()).or_default();
            for (prop, val) in props {
                entry.insert(prop.clone(), val.clone());
            }
        }

        // Skip frame environment creation when no modifiers or procedural
        // plots exist. For static scenes, this eliminates ~95% of evaluation
        // overhead.
        let frame_env = if needs_frame_env {
            Some(self.build_frame_env_internal(time_ms, scene_dimensions, &overrides))
        } else {
            None
        };

        FrameEvaluation {
            time_ms,
            scene_dimensions,
            debug_options,
            collect_items,
            suppressed_actors,
            transient_overrides,
            needs_frame_env,
            has_child_orders,
            bg_color,
            scene,
            hit_regions,
            program_items: None,
            overrides,
            frame_env,
        }
    }

    /// Execute lowered modifier IR, re-apply transient overrides, and record
    /// modifier errors as runtime diagnostics.
    fn run_frame_modifiers(&self, state: &mut FrameEvaluation<'_>) {
        let mut modifier_errors: Vec<EvalError> = Vec::new();
        if !self.modifier_programs.is_empty() {
            if let Some(env) = state.frame_env.as_mut() {
                for program in &self.modifier_programs {
                    if let Err(e) = self.apply_modifier_program(
                        program,
                        state.time_ms,
                        state.scene_dimensions,
                        env,
                        &mut state.overrides,
                    ) {
                        modifier_errors.push(e);
                    }
                }
            }
        } else if !self.modifiers.is_empty() {
            tracing::warn!(
                "No lowered modifier IR is available for {} modifier statement(s); skipping modifier execution",
                self.modifiers.len()
            );
        }

        // Re-apply transient overrides so user interaction wins over modifier execution.
        if !state.transient_overrides.is_empty() {
            for (label, props) in state.transient_overrides {
                let entry = state.overrides.entry(label.clone()).or_default();
                for (prop, val) in props {
                    entry.insert(prop.clone(), val.clone());
                }
            }
        }

        // Collect modifier evaluation errors as runtime diagnostics.
        for err in &modifier_errors {
            tracing::warn!("Modifier evaluation error at t={}ms: {err}", state.time_ms);
            self.eval_caches.runtime_diagnostics.borrow_mut().push(
                crate::diagnostics::Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::ModifierRuntimeError,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!("Modifier evaluation error at t={}ms: {err}", state.time_ms),
                ),
            );
        }
    }

    /// Build the observable program, stash bounds, and publish the frame cache
    /// or scene buffer.
    fn store_frame(
        &self,
        mut state: FrameEvaluation<'_>,
        filter_backend: &Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) -> crate::timeline::scene_program::SceneProgram {
        // Only store hit regions when explicitly requested.
        if state.debug_options.compute_hit_regions {
            *self.eval_caches.hit_regions.borrow_mut() = std::mem::take(&mut state.hit_regions);
        } else {
            self.eval_caches.hit_regions.borrow_mut().clear();
        }

        // The program owns the encoded scene; the frame cache keeps a clone
        // for restore, and filter/debug frames park their copy in scene_buffer.
        // The slot table builds `program.precise_bounds` (label-keyed, for the
        // public `SceneProgram` surface) only on the observable path — it has
        // no production reader outside tooling/tests, so the scene-only path
        // skips the label materialization entirely. On the scene-only path the
        // frame cache gets the flat `(slot, rect)` pairs; on the observable
        // path the pairs stay empty (the cloned program carries the label map,
        // and its rare hits re-derive slots from it).
        let (program_bounds, cached_bounds) = if state.collect_items {
            // One pass over the written slots builds the label-keyed map; the
            // pre-sized map avoids rehash growth. Registry borrowed in place.
            let table = self.eval_caches.precise_bounds.borrow();
            let registry = self.eval_caches.bounds_registry.borrow();
            let labels = registry.as_ref().map(|r| r.labels.as_slice());
            let mut b = std::collections::HashMap::with_capacity(table.written.len());
            for &slot in &table.written {
                if let Some(rect) = table.slots[slot as usize] {
                    if let Some(label) = labels.and_then(|l| l.get(slot as usize)) {
                        b.insert(label.clone(), rect);
                    }
                }
            }
            (b, Vec::new())
        } else {
            let b = self.eval_caches.precise_bounds.borrow().pairs_from(0);
            (std::collections::HashMap::new(), b)
        };
        let program = crate::timeline::scene_program::SceneProgram {
            dimensions: state.scene_dimensions,
            background: state.bg_color,
            scene: state.scene,
            items: state.program_items.take().unwrap_or_default(),
            suppressed_items: Vec::new(),
            precise_bounds: program_bounds,
            diagnostics: self.eval_caches.runtime_diagnostics.borrow().clone(),
        };
        if !self.backend_can_run(filter_backend)
            && state.debug_options == DebugRenderOptions::default()
            && state.transient_overrides.is_empty()
        {
            *self.eval_caches.frame_cache.borrow_mut() = Some(crate::timeline::FrameCacheEntry {
                time_ms: state.time_ms,
                dimensions: state.scene_dimensions,
                has_modifiers: state.needs_frame_env,
                has_dynamic_layout: self.dynamic_layout,
                has_child_orders: state.has_child_orders,
                program: program.clone(),
                precise_bounds: cached_bounds,
                collect_items: state.collect_items,
            });
        } else {
            // Filter/debug frames bypass the frame cache (its entry stays
            // valid for later default-option frames); hand the encoded scene
            // back to the buffer so the next such frame can reuse it.
            *self.eval_caches.scene_buffer.borrow_mut() = Some(program.scene.clone());
        }

        // Hand the finished frame environment back to the pool — its
        // overrides map keeps the table capacity and every key `String` alive
        // for the next frame's in-place rebuild. Only the plain evaluate path
        // pools; callers that received a cloned env (local_env in the plot
        // path) or built one on demand return nothing here.
        if let Some(env) = state.frame_env {
            *self.env_pool.borrow_mut() = Some(env);
        }
        *self.eval_caches.overrides_buffer.borrow_mut() = state.overrides;

        program
    }

    /// Clone-free publish step for [`Self::evaluate_for_presentation`].
    ///
    /// Recycles the pooled frame environment and overrides buffer (same as
    /// `store_frame`), clears hit regions (presentation never requests them),
    /// and moves the authoritative encoded scene out. It deliberately does NOT
    /// touch `frame_cache` and does NOT clone the scene: a frame keyed on
    /// `time_ms` (the web player's animation loop) would otherwise pay a full
    /// `vello::Scene` deep copy every frame. The caller parks the returned
    /// scene back via `recycle_presentation_scene` for the next frame.
    fn store_frame_presentation(&self, mut state: FrameEvaluation<'_>) -> vello::Scene {
        self.eval_caches.hit_regions.borrow_mut().clear();
        if let Some(env) = state.frame_env.take() {
            *self.env_pool.borrow_mut() = Some(env);
        }
        *self.eval_caches.overrides_buffer.borrow_mut() = state.overrides;
        state.scene
    }
}
