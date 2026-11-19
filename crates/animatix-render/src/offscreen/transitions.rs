use animatix::timeline::DebugRenderOptions;
use animatix::timeline::{SceneDimensions, Timeline};

#[cfg(feature = "vello-gpu")]
use super::render_timeline_into_frame_gpu;
use super::{OffscreenRenderer, PendingFrame, RenderedFrame, render_timeline_into_view};
use crate::transition::{
    TransitionCompositor, collect_flip_pairs, composite_flip_overlay, encode_flip_overlay,
    plan_shared_actors,
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
        let from_needs_filter_backend = from_timeline.needs_filter_backend();
        let to_needs_filter_backend = to_timeline.needs_filter_backend();
        if from_needs_filter_backend && !to_needs_filter_backend && self.filter_backend.is_none() {
            self.filter_backend = self.filter_backend_b.take();
        }
        if to_needs_filter_backend && !from_needs_filter_backend && self.filter_backend_b.is_none()
        {
            self.filter_backend_b = self.filter_backend.take();
        }
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
            dimensions,
            debug_options,
        );

        // Render each side independently so its scene is dropped before the
        // next evaluation, while keeping a separate legacy filter backend.
        self.render_transition_scene(
            TransitionSceneSide::Outgoing,
            from_timeline,
            from_time,
            dimensions,
            debug_options,
            &shared.suppressed_from,
        )?;
        self.render_transition_scene(
            TransitionSceneSide::Incoming,
            to_timeline,
            to_time,
            dimensions,
            debug_options,
            &shared.suppressed_to,
        )?;

        // Composite to output_texture
        let output_view = self
            .output_view
            .as_ref()
            .ok_or_else(|| "Missing offscreen output view".to_string())?;
        let view_a = self.view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
        let view_b = self.view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?;
        let compositor =
            self.compositor.as_ref().ok_or_else(|| "Missing compositor".to_string())?;
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
            #[cfg(feature = "vello-gpu")]
            if let Some(frame_gpu) = self.frame_gpu.as_mut() {
                let overlay_view =
                    self.view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
                let output_view = self
                    .output_view
                    .as_ref()
                    .ok_or_else(|| "Missing offscreen output view".to_string())?;
                frame_gpu.render_into(&overlay, overlay_view, kurbo::Affine::IDENTITY)?;
                frame_gpu.over(output_view, overlay_view, 1.0)?;
            } else {
                let Self {
                    core,
                    device,
                    queue,
                    view_a,
                    output_view,
                    ..
                } = self;
                let view_a =
                    view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
                let output_view = output_view
                    .as_ref()
                    .ok_or_else(|| "Missing offscreen output view".to_string())?;
                composite_flip_overlay(
                    core,
                    device,
                    queue,
                    view_a,
                    output_view,
                    dimensions,
                    &overlay,
                )?;
            }
            #[cfg(not(feature = "vello-gpu"))]
            {
                let Self {
                    core,
                    device,
                    queue,
                    view_a,
                    output_view,
                    ..
                } = self;
                let view_a =
                    view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?;
                let output_view = output_view
                    .as_ref()
                    .ok_or_else(|| "Missing offscreen output view".to_string())?;
                composite_flip_overlay(
                    core,
                    device,
                    queue,
                    view_a,
                    output_view,
                    dimensions,
                    &overlay,
                )?;
            }
        }

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn render_transition_scene(
        &mut self,
        side: TransitionSceneSide,
        timeline: &Timeline,
        time: f64,
        dimensions: SceneDimensions,
        debug_options: DebugRenderOptions,
        suppressed_actors: &[String],
    ) -> Result<(), String> {
        #[cfg(feature = "vello-gpu")]
        if let Some(frame_gpu) = self.frame_gpu.as_mut() {
            let view = match side {
                TransitionSceneSide::Outgoing => self.view_a.as_ref(),
                TransitionSceneSide::Incoming => self.view_b.as_ref(),
            }
            .ok_or_else(|| match side {
                TransitionSceneSide::Outgoing => "Missing offscreen view_a".to_string(),
                TransitionSceneSide::Incoming => "Missing offscreen view_b".to_string(),
            })?;
            return render_timeline_into_frame_gpu(
                frame_gpu,
                timeline,
                time,
                dimensions,
                debug_options,
                view,
                suppressed_actors,
            );
        }

        match side {
            TransitionSceneSide::Outgoing => {
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
                    timeline,
                    time,
                    dimensions,
                    debug_options,
                    filter_backend,
                    view_a.as_ref().ok_or_else(|| "Missing offscreen view_a".to_string())?,
                    texture_a.as_ref(),
                    suppressed_actors,
                )
            },
            TransitionSceneSide::Incoming => {
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
                    timeline,
                    time,
                    dimensions,
                    debug_options,
                    filter_backend_b,
                    view_b.as_ref().ok_or_else(|| "Missing offscreen view_b".to_string())?,
                    texture_b.as_ref(),
                    suppressed_actors,
                )
            },
        }
    }
}
