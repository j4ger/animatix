use animatix::timeline::DebugRenderOptions;
use animatix::timeline::{SceneDimensions, Timeline};

use super::{
    CanvasMapping, OffscreenRenderer, PendingFrame, RenderedFrame, render_timeline_into_frame_gpu,
};
use crate::transition::{
    TransitionCompositor, collect_flip_pairs, encode_flip_overlay, plan_shared_actors,
};

#[derive(Clone, Copy)]
enum TransitionSceneSide {
    Outgoing,
    Incoming,
}

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
            CanvasMapping::new(dimensions, dimensions),
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
            CanvasMapping::new(dimensions, dimensions),
            debug_options,
        )?;
        self.readback_output(dimensions)
    }

    /// Queue an authored composition transition at the requested output size.
    #[allow(clippy::too_many_arguments)]
    pub fn begin_canvas_transition(
        &mut self,
        from_timeline: &Timeline,
        from_time: f64,
        to_timeline: &Timeline,
        to_time: f64,
        progress: f32,
        transition_id: String,
        easing: animatix::easing::Easing,
        output: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<PendingFrame, String> {
        let mapping = CanvasMapping::new(from_timeline.canvas_dimensions(), output);
        self.render_transition_to_output(
            from_timeline,
            from_time,
            to_timeline,
            to_time,
            progress,
            transition_id,
            easing,
            mapping,
            debug_options,
        )?;
        self.begin_readback(output)
    }

    /// Render an authored composition transition at the requested output size.
    #[allow(clippy::too_many_arguments)]
    pub fn render_canvas_transition(
        &mut self,
        from_timeline: &Timeline,
        from_time: f64,
        to_timeline: &Timeline,
        to_time: f64,
        progress: f32,
        transition_id: String,
        easing: animatix::easing::Easing,
        output: SceneDimensions,
        debug_options: DebugRenderOptions,
    ) -> Result<RenderedFrame, String> {
        let mapping = CanvasMapping::new(from_timeline.canvas_dimensions(), output);
        self.render_transition_to_output(
            from_timeline,
            from_time,
            to_timeline,
            to_time,
            progress,
            transition_id,
            easing,
            mapping,
            debug_options,
        )?;
        self.readback_output(output)
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
        mapping: CanvasMapping,
        debug_options: DebugRenderOptions,
    ) -> Result<(), String> {
        self.prepare_canvas(mapping, true)?;

        // Lazy-init compositor
        if self.compositor.is_none() {
            self.compositor =
                Some(TransitionCompositor::new(&self.device).map_err(|e| e.to_string())?);
        }
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
            mapping.logical,
            debug_options,
        );

        // Render each side independently so its scene is dropped before the
        // next evaluation.
        self.render_transition_scene(
            TransitionSceneSide::Outgoing,
            from_timeline,
            from_time,
            mapping,
            debug_options,
            &shared.suppressed_from,
        )?;
        self.render_transition_scene(
            TransitionSceneSide::Incoming,
            to_timeline,
            to_time,
            mapping,
            debug_options,
            &shared.suppressed_to,
        )?;

        // Composite to output_texture
        let views = self.canvas_views(mapping)?;
        let [view_a, view_b] =
            views.sides.ok_or("Missing offscreen transition inputs")?.map(Clone::clone);
        let output_view = views.output.clone();
        let compositor =
            self.compositor.as_ref().ok_or_else(|| "Missing compositor".to_string())?;
        compositor
            .render(
                &self.device,
                &self.queue,
                &view_a,
                &view_b,
                &output_view,
                mapping.raster.width,
                mapping.raster.height,
                progress,
                &transition_id,
                easing,
            )
            .map_err(|e| e.to_string())?;

        // Render floating FLIP overlays above the blended transition.
        if !flip_items.is_empty() {
            let overlay = encode_flip_overlay(&flip_items, progress, easing);
            self.core.frame.render_into(&overlay, &view_a, mapping.transform)?;
            self.core.frame.over(&output_view, &view_a, 1.0)?;
        }

        self.present_canvas(mapping)
    }

    #[allow(clippy::too_many_arguments)]
    fn render_transition_scene(
        &mut self,
        side: TransitionSceneSide,
        timeline: &Timeline,
        time: f64,
        mapping: CanvasMapping,
        debug_options: DebugRenderOptions,
        suppressed_actors: &[String],
    ) -> Result<(), String> {
        let views = self.canvas_views(mapping)?;
        let [outgoing, incoming] = views.sides.ok_or("Missing offscreen transition inputs")?;
        let view = match side {
            TransitionSceneSide::Outgoing => outgoing.clone(),
            TransitionSceneSide::Incoming => incoming.clone(),
        };
        render_timeline_into_frame_gpu(
            &mut self.core.frame,
            timeline,
            time,
            mapping.logical,
            debug_options,
            &view,
            mapping.transform,
            suppressed_actors,
        )
    }
}
