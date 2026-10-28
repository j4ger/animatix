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
            let allow_static = !self.backend_can_run(out.filter_backend)
                && !self.camera_used.get()
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
                // and their vello encoding is reused on subsequent frames.
                // Dimensions and item collection are part of the key so different
                // canvas sizes or observable-program requests cannot reuse an
                // incompatible entry. Hit regions do not opt out: the entry
                // captures the subtree's (label, rect) pairs once and restores
                // them on hits, so the GUI (which always requests
                // `compute_hit_regions` for picking) keeps its static-subtree
                // reuse. A camera move invalidates it: the cached encoding is
                // appended verbatim and cannot be re-transformed, so the frame
                // must re-evaluate the subtree instead.
                let is_static = allow_static && self.is_static_subtree_id(root_id);
                if is_static {
                    let cache_key =
                        (root_id, state.scene_dimensions, state.collect_items, state.debug_options);
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
                        if state.debug_options.compute_hit_regions && !cached_hit_regions.is_empty()
                        {
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
                        let mut subtree_items_slot = if state.collect_items {
                            Some(Vec::new())
                        } else {
                            None
                        };
                        let subtree_hits_before = out.hit_regions.len();
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
                                    &empty_positions,
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
                            if state.debug_options.compute_hit_regions {
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
}
