//! Frame evaluation and program/frame-cache entry points.

use super::*;

impl Timeline {
    /// Adapter used by the [`crate::primitives::Primitive::render_children`]
    /// Filter strategy: children render into an offscreen scene, the Filter's
    /// post-processing is applied, and the result is composited back. Reached
    /// only through `FilterPrimitive::render_children`.
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

    /// Whether a supplied filter backend could actually be consulted while
    /// building a frame.
    ///
    /// The backend is only reached by effect-scope actors, so its mere presence
    /// is not a reason to skip the frame cache — a runtime that always supplies
    /// one (the web player passes it on every frame) would otherwise never
    /// cache anything. Debug options are a separate matter: they change what is
    /// drawn, so they always block.
    fn backend_can_run(
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

        let time_ms = (time_s * 1000.0) as u64;
        let needs_frame_env = self.needs_frame_env();
        let has_child_orders = self.world.has_child_orders();

        // Clear stale runtime diagnostics from previous frame.
        self.clear_runtime_diagnostics();

        // PF-4: Reuse a previously encoded scene buffer instead of allocating.
        // On a plain miss the replaced frame-cache entry is stale by
        // definition (restore_frame_cache just rejected it), so its encoded
        // scene is moved out — no clone — and becomes this frame's encode
        // buffer. Filter/debug frames bypass the frame cache (its entry stays
        // valid), so they recycle via scene_buffer instead.
        let debug_or_filter = self.backend_can_run(filter_backend)
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
        // PF-6 slot-id table: sparse clear of the previous frame's slots, then
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

        // P2.16: Skip frame environment creation when no modifiers or procedural plots exist.
        // For static scenes, this eliminates ~95% of evaluation overhead.
        let mut frame_env = if needs_frame_env {
            Some(self.build_frame_env_internal(time_ms, scene_dimensions, &overrides))
        } else {
            None
        };

        // Execute lowered modifier IR only. Lowering is total for all modifier
        // statements; if lowering failed at build time, the build already
        // emitted a ModifierCompilationError diagnostic.
        let mut modifier_errors: Vec<EvalError> = Vec::new();
        if !self.modifier_programs.is_empty() {
            if let Some(ref mut env) = frame_env {
                for program in &self.modifier_programs {
                    if let Err(e) = self.apply_modifier_program(
                        program,
                        time_ms,
                        scene_dimensions,
                        env,
                        &mut overrides,
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
        if !transient_overrides.is_empty() {
            for (label, props) in transient_overrides {
                let entry = overrides.entry(label.clone()).or_default();
                for (prop, val) in props {
                    entry.insert(prop.clone(), val.clone());
                }
            }
        }

        // Collect modifier evaluation errors as runtime diagnostics.
        for err in &modifier_errors {
            tracing::warn!("Modifier evaluation error at t={time_ms}ms: {err}");
            self.eval_caches.runtime_diagnostics.borrow_mut().push(
                crate::diagnostics::Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::ModifierRuntimeError,
                    crate::diagnostics::DiagnosticPhase::Render,
                    format!("Modifier evaluation error at t={time_ms}ms: {err}"),
                ),
            );
        }

        let bg = vello::peniko::Color::new([bg_color[0], bg_color[1], bg_color[2], bg_color[3]]);
        scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            bg,
            None,
            &kurbo::Rect::new(
                0.0,
                0.0,
                scene_dimensions.width as f64,
                scene_dimensions.height as f64,
            ),
        );

        let mut program_items = if collect_items {
            Some(Vec::new())
        } else {
            None
        };
        // The frame-invariant bundle and the mutable outputs are built once and
        // threaded through the whole recursion (see `RenderFrame`).
        let frame = crate::primitives::RenderFrame {
            time_ms,
            scene_dimensions,
            debug_options,
            overrides: &overrides,
            frame_env: frame_env.as_ref(),
        };
        // `sample` covers per-frame property sampling and node evaluation; the
        // vello encoding is currently interleaved inside it (see `stage::ENCODE_SCENE`).
        let _sample_stage = crate::perf::ScopedStage::new(crate::perf::stage::SAMPLE);
        // Scoped so the output borrows end with the loop: the code below reuses
        // `scene` / `hit_regions` / `program_items` directly.
        {
            let mut out = crate::primitives::RenderOutputs {
                scene: &mut scene,
                hit_regions: &mut hit_regions,
                program_items: &mut program_items,
                filter_backend: &mut *filter_backend,
            };
            // The scene camera wraps every root subtree (see `timeline::camera`).
            // It is read *after* the modifier pass above, so an
            // `always { camera.at = … }` write is already in the override map.
            // The background fill deliberately stays un-camerad.
            let camera_affine = if self.camera_used.get() {
                self.camera.affine(
                    time_ms,
                    scene_dimensions,
                    overrides.get(crate::timeline::camera::CAMERA_TARGET),
                )
            } else {
                kurbo::Affine::IDENTITY
            };
            let camera_exempt = self.camera_used.get() && self.camera_exempt_present.get();
            let allow_static = !self.backend_can_run(out.filter_backend)
                && !self.camera_used.get()
                && suppressed_actors.is_empty()
                && transient_overrides.is_empty();
            for &root_id in self.world.root_entities() {
                let Some(root_track) = self.world.tracks.get_by_id(root_id) else {
                    continue;
                };
                // `camera_follow: false` keeps a root subtree out of the camera
                // entirely — the HUD that must not move while the scene pushes
                // in. Only roots are consulted: the camera is applied here, once,
                // and a nested actor inherits its container's transform, so an
                // opt-out deeper in the tree would have nothing to undo (and is
                // reported at build). Short-circuited on `camera_exempt` so a
                // scene that never uses the flag does no track lookup per root.
                let root_affine = if camera_exempt {
                    let root_meta = self.world.meta(root_track.id);
                    if !root_meta.camera_follow || root_meta.parallax == 0.0 {
                        kurbo::Affine::IDENTITY
                    } else if (root_meta.parallax - 1.0).abs() < f32::EPSILON {
                        camera_affine
                    } else {
                        let p = root_meta.parallax as f64;
                        let cam = camera_affine.as_coeffs();
                        kurbo::Affine::new([
                            1.0 + p * (cam[0] - 1.0),
                            p * cam[1],
                            p * cam[2],
                            1.0 + p * (cam[3] - 1.0),
                            p * cam[4],
                            p * cam[5],
                        ])
                    }
                } else {
                    camera_affine
                };
                // P2.17: Static subtree cache — fully-static subtrees are evaluated once
                // and their vello encoding is reused on subsequent frames. Dimensions
                // and item collection are part of the key so different canvas sizes or
                // observable-program requests cannot reuse an incompatible entry.
                // PF-11: hit regions no longer opt out — the entry captures the
                // subtree's (label, rect) pairs once and restores them on hits, so
                // the GUI (which always requests `compute_hit_regions` for picking)
                // keeps its static-subtree reuse.
                // A camera move invalidates it: the cached encoding is appended
                // verbatim and cannot be re-transformed, so the frame must
                // re-evaluate the subtree instead.
                let is_static = allow_static && self.is_static_subtree_id(root_id);
                if is_static {
                    let cache_key = (root_id, scene_dimensions, collect_items, debug_options);
                    let cache = self.eval_caches.static_subtree_cache.borrow_mut();
                    if let Some((cached_scene, cached_bounds, cached_items, cached_hit_regions)) =
                        cache.get(&cache_key)
                    {
                        // Fast path: append cached encoding directly and restore the
                        // precise bounds that were computed for this subtree.
                        out.scene.encoding_mut().append(cached_scene.encoding(), &None);
                        if !cached_bounds.is_empty() {
                            let mut table = self.eval_caches.precise_bounds.borrow_mut();
                            for &(slot, rect) in cached_bounds {
                                table.write(slot, rect);
                            }
                        }
                        if debug_options.compute_hit_regions && !cached_hit_regions.is_empty() {
                            out.hit_regions.extend(cached_hit_regions.iter().cloned());
                        }
                        if let Some(items) = out.program_items.as_mut() {
                            if !cached_items.is_empty() {
                                items.reserve(cached_items.len());
                                items.extend(cached_items.iter().cloned());
                            }
                        }
                    } else {
                        drop(cache);
                        let mut temp_scene = vello::Scene::new();
                        // Snapshot offset into the frame's `written` list: entries
                        // appended during this subtree's evaluation are exactly its
                        // bounds (deterministic, unlike the old count-based skip
                        // over unordered HashMap iteration).
                        let subtree_written_before =
                            self.eval_caches.precise_bounds.borrow().written.len();
                        let mut subtree_items_slot = if collect_items {
                            Some(Vec::new())
                        } else {
                            None
                        };
                        let subtree_hits_before = out.hit_regions.len();
                        // The scratch subtree swaps both the draw target and the
                        // item sink, so it builds its own outputs rather than
                        // reusing the frame's.
                        // The scratch subtree swaps both the draw target and
                        // the item sink, so it runs through its own outputs.
                        out.with_scene_and_items(
                            &mut temp_scene,
                            &mut subtree_items_slot,
                            |temp_out| {
                                self.evaluate_node_by_id(
                                    root_id,
                                    root_affine,
                                    1.0,
                                    &crate::timeline::layout::LayoutPositions::new(),
                                    true,
                                    &frame,
                                    temp_out,
                                );
                            },
                        );
                        let subtree_items = subtree_items_slot.take().unwrap_or_default();
                        if let Some(items) = out.program_items.as_mut() {
                            items.extend(subtree_items.iter().cloned());
                        }
                        // Capture the subtree's hit regions once (only collected
                        // when requested; see the cache-key's debug_options).
                        let new_hit_regions: Vec<(EntityId, kurbo::Rect)> =
                            if debug_options.compute_hit_regions {
                                out.hit_regions[subtree_hits_before..].to_vec()
                            } else {
                                Vec::new()
                            };
                        // Append to main out.scene and cache for next time.
                        out.scene.encoding_mut().append(temp_scene.encoding(), &None);
                        let new_bounds: Vec<(u32, kurbo::Rect)> = self
                            .eval_caches
                            .precise_bounds
                            .borrow()
                            .pairs_from(subtree_written_before);
                        self.eval_caches.static_subtree_cache.borrow_mut().insert(
                            cache_key,
                            (temp_scene, new_bounds, subtree_items, new_hit_regions),
                        );
                    }
                } else {
                    // A filter scope may only take the zero-readback pending path
                    // when nothing renders after it (`can_post_composite_filter`);
                    // otherwise its post-render blit would cover later siblings.
                    // Mid-out.scene filters fall back to the inline readback path.
                    let allow_pending = self.can_post_composite_filter(root_id);
                    self.evaluate_node_by_id(
                        root_id,
                        root_affine,
                        1.0,
                        &crate::timeline::layout::LayoutPositions::new(), // empty for roots
                        allow_pending,
                        &frame,
                        &mut out,
                    );
                }
            }
        }

        // P2.24: Only store hit regions when explicitly requested.
        // Saves bounding-box computation for frames where click-to-select is not needed.
        if debug_options.compute_hit_regions {
            *self.eval_caches.hit_regions.borrow_mut() = hit_regions;
        } else {
            self.eval_caches.hit_regions.borrow_mut().clear();
        }

        // The program owns the encoded scene; the frame cache keeps a clone
        // for restore, and filter/debug frames park their copy in scene_buffer.
        // The slot table builds `program.precise_bounds` (label-keyed, for the
        // public `SceneProgram` surface) only on the observable path — it has
        // no production reader outside tooling/tests (checked 2026-09-05), so
        // the scene-only path skips the label materialization entirely. On the
        // scene-only path the frame cache gets the flat `(slot, rect)` pairs;
        // on the observable path the pairs stay empty (the cloned program
        // carries the label map, and its rare hits re-derive slots from it —
        // avoids allocating both representations per observable miss).
        let (program_bounds, cached_bounds) = if collect_items {
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
            dimensions: scene_dimensions,
            background: bg_color,
            scene,
            items: program_items.take().unwrap_or_default(),
            suppressed_items: Vec::new(),
            precise_bounds: program_bounds,
            diagnostics: self.eval_caches.runtime_diagnostics.borrow().clone(),
        };
        if !self.backend_can_run(filter_backend)
            && debug_options == DebugRenderOptions::default()
            && transient_overrides.is_empty()
        {
            *self.eval_caches.frame_cache.borrow_mut() = Some(crate::timeline::FrameCacheEntry {
                time_ms,
                dimensions: scene_dimensions,
                has_modifiers: needs_frame_env,
                has_dynamic_layout: self.dynamic_layout,
                has_child_orders,
                program: program.clone(),
                precise_bounds: cached_bounds,
                collect_items,
            });
        } else {
            // Filter/debug frames bypass the frame cache (its entry stays
            // valid for later default-option frames); hand the encoded scene
            // back to the buffer so the next such frame can reuse it.
            *self.eval_caches.scene_buffer.borrow_mut() = Some(program.scene.clone());
        }

        // PF-6: hand the finished frame environment back to the pool — its
        // overrides map keeps the table capacity and every key `String` alive
        // for the next frame's in-place rebuild. Only the plain evaluate path
        // pools; callers that received a cloned env (`local_env` in the plot
        // path) or built one on demand return nothing here.
        if let Some(env) = frame_env {
            *self.env_pool.borrow_mut() = Some(env);
        }
        *self.eval_caches.overrides_buffer.borrow_mut() = overrides;

        program
    }
}
