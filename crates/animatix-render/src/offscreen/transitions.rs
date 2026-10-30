use animatix::timeline::DebugRenderOptions;
use animatix::timeline::{SceneDimensions, Timeline};

use super::{OffscreenRenderer, PendingFrame, RenderedFrame, render_timeline_into_view};
use crate::transition::{
    TransitionCompositor, collect_flip_pairs, composite_flip_overlay, encode_flip_overlay,
    plan_shared_actors,
};

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

        // Wave 4.4: match `shared_id` across the two local transition times.
        // Shared actors are suppressed during each scene render and re-drawn
        // once in the floating FLIP overlay below.
        let shared = plan_shared_actors(from_timeline, from_time, to_timeline, to_time);
        let flip_items = collect_flip_pairs(
            &shared,
            from_timeline,
            from_time,
            to_timeline,
            to_time,
            dimensions,
            debug_options,
        );

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
                &shared.suppressed_from,
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
                &shared.suppressed_to,
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

        // Render floating FLIP overlays above the blended transition.
        if !flip_items.is_empty() {
            let overlay = encode_flip_overlay(&flip_items, progress, easing);
            let Self {
                core,
                device,
                queue,
                view_a,
                output_view,
                ..
            } = self;
            let view_a = view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
            let output_view = output_view
                .as_ref()
                .ok_or_else(|| "Missing offscreen output view".to_string())?;
            composite_flip_overlay(core, device, queue, view_a, output_view, dimensions, &overlay)?;
        }

        Ok(())
    }
}
