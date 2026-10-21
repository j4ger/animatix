use animatix::timeline::DebugRenderOptions;
use animatix::timeline::{SceneDimensions, Timeline};

use super::{OffscreenRenderer, PendingFrame, RenderedFrame, render_timeline_into_view};
use crate::transition::TransitionCompositor;

impl OffscreenRenderer {
    /// Pipelined variant of [`Self::render_transition`] (see
    /// [`Self::begin_frame_with_debug`]).
    pub fn begin_transition(
        &mut self,
        from_timeline: &Timeline,
        from_time: f64,
        to_timeline: &Timeline,
        to_time: f64,
        progress: f32,
        transition_id: String,
        easing: animatix::easing::Easing,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<PendingFrame, String> {
        self.render_transition_to_output(
            from_timeline,
            from_time,
            to_timeline,
            to_time,
            progress,
            transition_id,
            easing,
            dimensions,
            debug_options,
        )?;
        self.begin_readback(dimensions)
    }

    /// Render a transition between two timelines by compositing them with the
    /// given progress and transition type. Returns a CPU-readback frame.
    pub fn render_transition(
        &mut self,
        from_timeline: &Timeline,
        from_time: f64,
        to_timeline: &Timeline,
        to_time: f64,
        progress: f32,
        transition_id: String,
        easing: animatix::easing::Easing,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<RenderedFrame, String> {
        self.render_transition_to_output(
            from_timeline,
            from_time,
            to_timeline,
            to_time,
            progress,
            transition_id,
            easing,
            dimensions,
            debug_options,
        )?;
        self.readback_output(dimensions)
    }

    /// Shared tail of `render_transition` / `begin_transition`: render both
    /// scenes to the intermediate textures, composite onto the output texture.
    /// Leaves the GPU work queued (no readback).
    #[allow(clippy::too_many_arguments)]
    fn render_transition_to_output(
        &mut self,
        from_timeline: &Timeline,
        from_time: f64,
        to_timeline: &Timeline,
        to_time: f64,
        progress: f32,
        transition_id: String,
        easing: animatix::easing::Easing,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        if dimensions.width == 0 || dimensions.height == 0 {
            return Err("Preview dimensions must be greater than zero".to_string());
        }

        self.ensure_targets(dimensions);

        // The two scenes share one dimension key: a resize drops both backends
        // and each is lazily rebuilt in `render_timeline_into_view`.
        if self.filter_backend_dimensions != Some(dimensions) {
            self.filter_backend = None;
            self.filter_backend_b = None;
            self.filter_backend_dimensions = Some(dimensions);
        }

        // Lazy-init compositor
        if self.compositor.is_none() {
            self.compositor =
                Some(TransitionCompositor::new(&self.device).map_err(|e| e.to_string())?);
        }
        let compositor =
            self.compositor.as_ref().ok_or_else(|| "Missing compositor".to_string())?;

        // Wave 4.4: Detect matching shared_id across from_timeline and to_timeline for FLIP
        // transitions
        let from_time_ms = (from_time * 1000.0) as u64;
        let to_time_ms = (to_time * 1000.0) as u64;
        let from_shared = from_timeline.shared_id_actors(from_time_ms);
        let to_shared = to_timeline.shared_id_actors(to_time_ms);

        let mut suppressed_from = Vec::new();
        let mut suppressed_to = Vec::new();
        let mut shared_pairs = Vec::new();

        for (shared_id, from_label) in &from_shared {
            if let Some(to_label) = to_shared.get(shared_id) {
                suppressed_from.push(from_label.clone());
                suppressed_to.push(to_label.clone());
                shared_pairs.push((from_label.clone(), to_label.clone()));
            }
        }

        let mut flip_items: Vec<(
            animatix::timeline::scene_program::SceneItem,
            animatix::timeline::scene_program::SceneItem,
        )> = Vec::new();
        if !shared_pairs.is_empty() {
            let from_prog = from_timeline.evaluate_program_with_debug(
                from_time,
                dimensions,
                debug_options,
                &mut None,
            );
            let to_prog = to_timeline.evaluate_program_with_debug(
                to_time,
                dimensions,
                debug_options,
                &mut None,
            );
            for (from_lbl, to_lbl) in &shared_pairs {
                let from_item = from_prog.items.iter().find(|it| it.label == *from_lbl).cloned();
                let to_item = to_prog.items.iter().find(|it| it.label == *to_lbl).cloned();
                if let (Some(f), Some(t)) = (from_item, to_item) {
                    flip_items.push((f, t));
                }
            }
        }

        // Render the outgoing scene to texture_a, then drop scene_a before
        // creating scene_b to avoid holding both large vello::Scene objects
        // simultaneously. Each scene keeps its own filter backend, so a
        // `Filter` scope shows up in the transition frame exactly as it does
        // in the single-scene path (its pending composites are blitted onto
        // the scene's own texture before the blend reads it).
        {
            let Self {
                core,
                device,
                queue,
                filter_backend,
                view_a,
                texture_a,
                ..
            } = self;
            render_timeline_into_view(
                core,
                device,
                queue,
                from_timeline,
                from_time,
                dimensions,
                debug_options,
                filter_backend,
                view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?,
                texture_a.as_ref(),
                &suppressed_from,
            )?;
        }

        // Render to scene to texture_b
        {
            let Self {
                core,
                device,
                queue,
                filter_backend_b,
                view_b,
                texture_b,
                ..
            } = self;
            render_timeline_into_view(
                core,
                device,
                queue,
                to_timeline,
                to_time,
                dimensions,
                debug_options,
                filter_backend_b,
                view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?,
                texture_b.as_ref(),
                &suppressed_to,
            )?;
        }

        // Composite to output_texture
        let output_view = self
            .output_view
            .as_ref()
            .ok_or_else(|| "Missing offscreen output view".to_string())?;
        let view_a = self.view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
        let view_b = self.view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?;
        compositor
            .render(
                &self.device,
                &self.queue,
                view_a,
                view_b,
                output_view,
                dimensions.width,
                dimensions.height,
                progress,
                &transition_id,
                easing,
            )
            .map_err(|e| e.to_string())?;

        // Render floating FLIP overlays
        if !flip_items.is_empty() {
            let mut overlay_scene = vello::Scene::new();
            let p_eased = animatix::easing::apply_easing(progress, easing);
            let p_f64 = p_eased as f64;
            for (from_item, to_item) in &flip_items {
                let c_from = from_item.transform.as_coeffs();
                let c_to = to_item.transform.as_coeffs();
                let mut c_interp = [0.0; 6];
                for i in 0..6 {
                    c_interp[i] = (1.0 - p_f64) * c_from[i] + p_f64 * c_to[i];
                }
                let interp_affine = kurbo::Affine::new(c_interp);

                let alpha_from = (1.0 - p_eased) * from_item.opacity;
                let alpha_to = p_eased * to_item.opacity;
                if alpha_from > 0.001 {
                    for cmd in &from_item.commands {
                        cmd.execute(&mut overlay_scene, &interp_affine, alpha_from);
                    }
                }
                if alpha_to > 0.001 {
                    for cmd in &to_item.commands {
                        cmd.execute(&mut overlay_scene, &interp_affine, alpha_to);
                    }
                }
            }

            let view_a =
                self.view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
            self.core
                .render_vello_scene_with_background(
                    &self.device,
                    &self.queue,
                    view_a,
                    dimensions.width,
                    dimensions.height,
                    &overlay_scene,
                    vello::peniko::Color::TRANSPARENT,
                )
                .map_err(|e| e.to_string())?;

            self.core.blit_texture(
                &self.device,
                &self.queue,
                view_a,
                output_view,
                dimensions.width,
                dimensions.height,
                1.0,
            );
        }

        Ok(())
    }
}
