//! Per-node draw phase: command execution, hit-region/precise-bounds
//! recording, and debug overlays.
//!
//! Split out of `render_actor_node_impl`; the execution order is unchanged.

use super::*;

impl Timeline {
    /// Draw the node's evaluated commands, record its hit region and precise
    /// bounds, and paint debug overlays.
    ///
    /// The hit region is command-derived when commands exist; otherwise the
    /// actor's layout half-size box keeps container shells clickable.
    #[allow(clippy::too_many_arguments)] // one call site; the bundle is the phase input
    pub(super) fn draw_node_commands(
        &self,
        node_id: EntityId,
        node_label: &str,
        time_ms: u64,
        debug_options: DebugRenderOptions,
        local_transform: kurbo::Affine,
        opacity: f32,
        half_size: [f32; 2],
        vector_paths: &[VelloPath],
        commands: Vec<crate::primitives::RenderCommand>,
        scene: &mut vello::Scene,
        hit_regions: &mut Vec<(EntityId, kurbo::Rect)>,
        program_items: &mut Option<Vec<crate::timeline::scene_program::SceneItem>>,
    ) {
        for cmd in &commands {
            cmd.execute(scene, &local_transform, opacity);
        }
        if let Some(items) = program_items.as_mut() {
            items.push(crate::timeline::scene_program::SceneItem {
                label: node_label.to_string(),
                transform: local_transform,
                opacity,
                commands: commands.clone(),
            });
        }
        // Hit region — compute from commands, not stale vector_paths.
        // Scoped item: a shape-command memo hit hands over the
        // build-time-computed bounds (image_size is irrelevant —
        // shape commands never carry images), skipping the per-node
        // bezpath `bounding_box` unions entirely.
        let caps = self.world.caps(node_id);
        let memo_bounds = if caps.is_shape {
            self.world.take_shape_command_bounds(node_id)
        } else {
            None
        };
        let local_bounds: Option<kurbo::Rect> = match memo_bounds {
            Some(bounds) => bounds,
            None => {
                // `local_bounds` is a declaration-time cache. Keep its O(1) path
                // for static text only when this command uses the exact cached
                // glyph allocation; sampled content/typography/layout/reactive
                // overrides have different paths and must derive bounds from
                // their current commands. TextPath::bounds is already cached,
                // so this union avoids recomputing glyph bezier bounds. Non-text
                // actors retain the existing cheap cached path.
                let uses_static_text_paths = caps.text.is_some()
                    && self
                        .world
                        .text(node_id)
                        .and_then(|text| text.static_paths.as_ref())
                        .is_some_and(|static_paths| match commands.as_slice() {
                            // Any extra command (notably Typst's HighlightLayer),
                            // or a clip that extends/changes the painted geometry,
                            // must be included by command-derived bounds.
                            [
                                crate::primitives::RenderCommand::Text {
                                    paths,
                                    clip_path: None,
                                    ..
                                },
                            ] => std::sync::Arc::ptr_eq(paths, static_paths),
                            _ => false,
                        });
                if (caps.text.is_none() || uses_static_text_paths)
                    && let Some(lb) = self.world.local_bounds(node_id)
                {
                    Some(lb)
                } else {
                    let image_size = if caps.image_payload {
                        self.world
                            .image_track(node_id)
                            .is_some_and(|t| t.evaluate(time_ms).is_some())
                            .then_some(half_size)
                    } else {
                        None
                    };
                    let mut bounds: Option<kurbo::Rect> = None;
                    for cmd in &commands {
                        if let Some(cmd_bounds) = cmd.local_bounds(image_size) {
                            bounds = Some(match bounds {
                                Some(existing) => existing.union(cmd_bounds),
                                None => cmd_bounds,
                            });
                        }
                    }
                    bounds
                }
            },
        };
        // Shape-primitive commands came from the track's command memo on a
        // hit — hand the buffers back so the next frame can take them again
        // (`take_shape_commands` returns `None` for non-memo actors, so a
        // recycle of a foreign `Vec` is simply dropped by the memo).
        self.world.recycle_shape_commands(node_id, commands);

        // Record the hit region for this actor (used by GUI picking and
        // precise-bounds consumers). Command-derived bounds when commands
        // exist; otherwise the actor's layout half-size box — this keeps
        // container shells (which return `Some(vec![])`) clickable at the
        // box they occupy. Actors whose `evaluate()` returned `None` have
        // no drawable content (e.g. empty text), so they intentionally
        // record nothing (semantics pinned by
        // `runtime_empty_text_override_clears_stale_glyphs`).
        let world_bounds = if let Some(lb) = local_bounds {
            transform_rect_bbox(&local_transform, lb)
        } else {
            let default_bounds = kurbo::Rect::new(
                (-half_size[0]) as f64,
                (-half_size[1]) as f64,
                half_size[0] as f64,
                half_size[1] as f64,
            );
            transform_rect_bbox(&local_transform, default_bounds)
        };
        // `hit_regions` is only read back when picking was requested
        // (`evaluate_program_inner` discards it otherwise). The id is the
        // entity handle, so no label is materialized per node per frame.
        if debug_options.compute_hit_regions {
            hit_regions.push((node_id, world_bounds));
        }
        // Slot-id bounds: the frame start stamped every track's slot, so the
        // write is a `Vec` store — no string key, no hash.
        let slot = self.world.bounds_slot(node_id);
        self.record_precise_bounds(slot, node_label, world_bounds);

        // Debug overlays
        if debug_options.draw_bounds {
            let svg_paths = self.world.svg_paths_at(node_id, time_ms).unwrap_or_default();
            let text_paths = self
                .world
                .text_or_default(node_id)
                .evaluate_paths(self.world.style(node_id), time_ms);
            let has_image = caps.image_payload
                && self.world.image_track(node_id).is_some_and(|t| t.evaluate(time_ms).is_some());
            let _ = self.add_node_debug_overlays(
                &svg_paths,
                half_size,
                &local_transform,
                scene,
                vector_paths,
                &text_paths,
                has_image,
            );
        }
    }
}
