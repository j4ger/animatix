//! Frame render phase: background fill and root-subtree encoding.
//!
//! The evaluation entry point dispatches the frame stages; the execution order
//! here is background → roots → caches.

use super::frame::FrameEvaluation;
use super::*;

impl Timeline {
    /// Encode the frame: background fill, then every root subtree
    /// (static-subtree fast path or full node evaluation).
    pub(super) fn render_frame_roots(
        &self,
        state: &mut FrameEvaluation<'_>,
        filter_backend: &mut Option<&mut dyn crate::timeline::effects::FilterBackend>,
    ) {
        let bg = vello::peniko::Color::new([
            state.bg_color[0],
            state.bg_color[1],
            state.bg_color[2],
            state.bg_color[3],
        ]);
        state.scene.fill(
            vello::peniko::Fill::NonZero,
            kurbo::Affine::IDENTITY,
            bg,
            None,
            &kurbo::Rect::new(
                0.0,
                0.0,
                state.scene_dimensions.width as f64,
                state.scene_dimensions.height as f64,
            ),
        );

        let mut program_items = if state.collect_items {
            Some(Vec::new())
        } else {
            None
        };
        // The frame-invariant bundle and the mutable outputs are built once and
        // threaded through the whole recursion (see `RenderFrame`).
        let frame = crate::primitives::RenderFrame {
            time_ms: state.time_ms,
            scene_dimensions: state.scene_dimensions,
            debug_options: state.debug_options,
            overrides: &state.overrides,
            frame_env: state.frame_env.as_ref(),
        };
        // `sample` covers per-frame property sampling and node evaluation; the
        // vello encoding is currently interleaved inside it (see
        // `stage::ENCODE_SCENE`).
        let _sample_stage = crate::perf::ScopedStage::new(crate::perf::stage::SAMPLE);
        // Scoped so the output borrows end with the loop: the caller reuses
        // `scene` / `hit_regions` / `program_items` directly.
        {
            let mut out = crate::primitives::RenderOutputs {
                scene: &mut state.scene,
                hit_regions: &mut state.hit_regions,
                program_items: &mut program_items,
                filter_backend: &mut *filter_backend,
            };
            // The scene camera wraps every root subtree (see `timeline::camera`).
            // It is read *after* the modifier pass above, so an
            // `always { camera.at = … }` write is already in the override map.
            // The background fill deliberately stays un-camerad.
            // Roots have no layout parent: one shared empty table per frame.
            let empty_positions = crate::timeline::layout::empty_layout_positions();
            let camera_affine = if self.camera_used.get() {
                self.camera.affine(
                    state.time_ms,
                    state.scene_dimensions,
                    state.overrides.get(crate::timeline::camera::CAMERA_TARGET),
                )
            } else {
                kurbo::Affine::IDENTITY
            };
            let camera_exempt = self.camera_used.get() && self.camera_exempt_present.get();
            // The whole-frame cache stays on the conservative `backend_can_run`
            // gate (see its doc). The *static-subtree* cache is per root, so a
            // backend only vetoes roots whose subtree actually contains an
            // effect scope; a fully-static root in a filtered scene still
            // caches. `invalidate_frame_cache` clears the cache on every
            // `content_epoch` bump, and a rebuilt web document is a fresh
            // `Timeline`, so no stale composite is replayed.
            let backend_can_run = self.backend_can_run(out.filter_backend);
            let base_allow_static = !self.camera_used.get()
                && state.suppressed_actors.is_empty()
                && state.transient_overrides.is_empty();
            for &root_id in self.world.root_entities() {
                if !self.world.exists(root_id) {
                    continue;
                }
                // `camera_follow: false` keeps a root subtree out of the camera
                // entirely — the HUD that must not move while the scene pushes
                // in. Only roots are consulted: the camera is applied here, once,
                // and a nested actor inherits its container's transform, so an
                // opt-out deeper in the tree would have nothing to undo (and is
                // reported at build). Short-circuited on `camera_exempt` so a
                // scene that never uses the flag does no track lookup per root.
                let root_affine = if camera_exempt {
                    let root_meta = self.world.meta(root_id);
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
                // Static subtree cache — fully-static subtrees are evaluated once
                // and their vello encoding reused. Dimensions and item collection
                // are part of the key; hit regions are captured once and restored.
                // A camera move invalidates it (the cached encoding cannot be
                // re-transformed). When a backend can run, bypass the cache ONLY
                // for roots whose subtree actually contains an effect scope —
                // that encoding depends on the backend's pending side channel, so
                // replaying it would drop the filter result.
                let root_has_effect_scope =
                    backend_can_run && self.subtree_has_effect_scope(root_id);
                let is_static = base_allow_static
                    && !root_has_effect_scope
                    && self.is_static_subtree_id(root_id);
                if is_static {
                    let cache_key =
                        (root_id, state.scene_dimensions, state.collect_items, state.debug_options);
                    self.render_static_root(
                        root_id,
                        root_affine,
                        cache_key,
                        state.collect_items,
                        state.debug_options,
                        &empty_positions,
                        &frame,
                        &mut out,
                    );
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
                        &empty_positions, // shared empty: roots carry no layout parent
                        allow_pending,
                        &frame,
                        &mut out,
                    );
                }
            }
        }
        state.program_items = program_items;
    }

    /// Encode one fully-static root via the static-subtree cache (append the
    /// cached encoding on a hit, or evaluate into a scratch scene, capture its
    /// bounds/items/hit regions, append, and store on a miss). Extracted from
    /// the root loop to keep `render_frame_roots` within the structure gate.
    #[allow(clippy::too_many_arguments)]
    fn render_static_root(
        &self,
        root_id: EntityId,
        root_affine: kurbo::Affine,
        cache_key: (EntityId, SceneDimensions, bool, DebugRenderOptions),
        collect_items: bool,
        debug_options: DebugRenderOptions,
        empty_positions: &crate::timeline::layout::LayoutPositions,
        frame: &crate::primitives::RenderFrame<'_>,
        out: &mut crate::primitives::RenderOutputs<'_, '_>,
    ) {
        let mut hit = false;
        {
            let cache = self.eval_caches.static_subtree_cache.borrow();
            if let Some((cached_scene, cached_bounds, cached_items, cached_hit_regions)) =
                cache.get(&cache_key)
            {
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
                if let Some(items) = out.program_items.as_mut()
                    && !cached_items.is_empty()
                {
                    items.reserve(cached_items.len());
                    items.extend(cached_items.iter().cloned());
                }
                hit = true;
            }
        }
        if hit {
            return;
        }
        let mut temp_scene = vello::Scene::new();
        // Entries appended during this subtree's evaluation are exactly its
        // bounds (deterministic; see the old count-based skip note).
        let subtree_written_before = self.eval_caches.precise_bounds.borrow().written.len();
        let mut subtree_items_slot = if collect_items {
            Some(Vec::new())
        } else {
            None
        };
        let subtree_hits_before = out.hit_regions.len();
        out.with_scene_and_items(&mut temp_scene, &mut subtree_items_slot, |temp_out| {
            self.evaluate_node_by_id(
                root_id,
                root_affine,
                1.0,
                empty_positions,
                true,
                frame,
                temp_out,
            );
        });
        let subtree_items = subtree_items_slot.take().unwrap_or_default();
        if let Some(items) = out.program_items.as_mut() {
            items.extend(subtree_items.iter().cloned());
        }
        let new_hit_regions: Vec<(EntityId, kurbo::Rect)> = if debug_options.compute_hit_regions {
            out.hit_regions[subtree_hits_before..].to_vec()
        } else {
            Vec::new()
        };
        out.scene.encoding_mut().append(temp_scene.encoding(), &None);
        let new_bounds: Vec<(u32, kurbo::Rect)> =
            self.eval_caches.precise_bounds.borrow().pairs_from(subtree_written_before);
        self.eval_caches
            .static_subtree_cache
            .borrow_mut()
            .insert(cache_key, (temp_scene, new_bounds, subtree_items, new_hit_regions));
    }

    /// Whether `id` or any descendant is an effect scope (`Filter`/`Glass`),
    /// i.e. could consult the filter backend while encoding. Walks the ECS
    /// hierarchy by `EntityId` — no label-keyed map. When a backend is active,
    /// a cached encoding for such a subtree is never self-contained (the
    /// composite lives in the backend's pending side channel), so the render
    /// loop uses this to bypass the static-subtree cache on a per-root basis.
    fn subtree_has_effect_scope(&self, id: EntityId) -> bool {
        if !self.world.exists(id) {
            return false;
        }
        if self.world.caps(id).is_effect_scope() {
            return true;
        }
        self.world
            .hierarchy(id)
            .children
            .iter()
            .any(|&child| self.subtree_has_effect_scope(child))
    }
}
