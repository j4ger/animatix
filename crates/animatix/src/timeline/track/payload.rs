//! Primitive-payload accessors on `AnimationTrack`.

use std::collections::HashMap;

use super::AnimationTrack;
use crate::renderer::types::VelloPath;
use crate::timeline::BarChartLayout;
use crate::timeline::image::SceneImage;
use crate::timeline::plot::{BarDataTransition, FuncTransition, ProceduralPlot};
use crate::timeline::property_track::PropertyTrack;

impl AnimationTrack {
    /// Return a reference to the plot payload if present.
    pub fn plot_payload(&self) -> Option<&crate::primitives::PlotPayload> {
        self.payload.as_plot()
    }

    /// Return a mutable reference to the plot payload if present.
    pub fn plot_payload_mut(&mut self) -> Option<&mut crate::primitives::PlotPayload> {
        self.payload.as_plot_mut()
    }

    /// Ensure the track has an initialized plot payload and return a mutable reference.
    pub fn ensure_plot_payload(&mut self) -> &mut crate::primitives::PlotPayload {
        self.payload.ensure_plot()
    }

    /// Return a reference to the SVG payload if present.
    pub fn svg_payload(&self) -> Option<&crate::primitives::SvgPayload> {
        self.payload.as_svg()
    }

    /// Return a mutable reference to the SVG payload if present.
    pub fn svg_payload_mut(&mut self) -> Option<&mut crate::primitives::SvgPayload> {
        self.payload.as_svg_mut()
    }

    /// Ensure the track has an initialized SVG payload and return a mutable reference.
    pub fn ensure_svg_payload(&mut self) -> &mut crate::primitives::SvgPayload {
        self.payload.ensure_svg()
    }

    /// Return a reference to the image payload if present.
    pub fn image_payload(&self) -> Option<&crate::primitives::ImagePayload> {
        self.payload.as_image()
    }

    /// Return a mutable reference to the image payload if present.
    pub fn image_payload_mut(&mut self) -> Option<&mut crate::primitives::ImagePayload> {
        self.payload.as_image_mut()
    }

    /// Ensure the track has an initialized image payload and return a mutable reference.
    pub fn ensure_image_payload(&mut self) -> &mut crate::primitives::ImagePayload {
        self.payload.ensure_image()
    }

    // ── Plot payload readers ──
    //
    // These are `Option`-returning on the read side: an actor that never built a
    // plot payload has no plot state at all, which is exactly what the old
    // `None` / empty-`Vec` flat fields meant.

    /// Return the procedural plot generator, if this actor is one.
    pub fn procedural_plot(&self) -> Option<&ProceduralPlot> {
        self.plot_payload().and_then(|p| p.procedural_plot.as_ref())
    }

    /// Return the per-parameter keyframe tracks, if this actor has a plot payload.
    pub fn plot_param_tracks(&self) -> Option<&HashMap<String, PropertyTrack<f64>>> {
        self.plot_payload().map(|p| &p.plot_param_tracks)
    }

    /// Return one plot parameter's keyframe track, if it exists.
    pub fn plot_param_track(&self, name: &str) -> Option<&PropertyTrack<f64>> {
        self.plot_payload().and_then(|p| p.plot_param_tracks.get(name))
    }

    /// Return the `func` transitions in stamp order; empty when no plot payload.
    pub fn func_transitions(&self) -> &[FuncTransition] {
        self.plot_payload().map_or(&[], |p| &p.func_transitions)
    }

    /// Return the `func` transitions for writing, initializing the plot payload.
    pub fn func_transitions_mut(&mut self) -> &mut Vec<FuncTransition> {
        &mut self.ensure_plot_payload().func_transitions
    }

    /// Return the declaration's bar dataset; empty when no plot payload.
    pub fn bar_data(&self) -> &[(String, f32)] {
        self.plot_payload().map_or(&[], |p| &p.bar_data)
    }

    /// Return the declaration's bar dataset for writing, initializing the plot
    /// payload.
    pub fn bar_data_mut(&mut self) -> &mut Vec<(String, f32)> {
        &mut self.ensure_plot_payload().bar_data
    }

    /// Return the `data = {…}` transitions in stamp order; empty when no plot
    /// payload.
    pub fn bar_data_transitions(&self) -> &[BarDataTransition] {
        self.plot_payload().map_or(&[], |p| &p.bar_data_transitions)
    }

    /// Return the `data = {…}` transitions for writing, initializing the plot
    /// payload.
    pub fn bar_data_transitions_mut(&mut self) -> &mut Vec<BarDataTransition> {
        &mut self.ensure_plot_payload().bar_data_transitions
    }

    /// Return the bar layout resolved at build time, if this actor is a bar chart.
    ///
    /// `pub(crate)` because [`BarChartLayout`] is a build-internal type; the
    /// flat field this replaces was `pub(crate)` for the same reason. Read-only
    /// on purpose: the layout is resolved once at build and never created or
    /// replaced at frame time, so a caller that gets `None` has the wrong actor.
    pub(crate) fn bar_layout(&self) -> Option<&BarChartLayout> {
        self.plot_payload().and_then(|p| p.bar_layout.as_ref())
    }

    // ── SVG payload readers ──

    /// Return the declaration-level SVG paths; empty when this actor has no SVG
    /// payload.
    pub fn svg_paths(&self) -> &[VelloPath] {
        self.svg_payload().map_or(&[], |p| &p.svg_paths)
    }

    // ── Image payload readers ──

    /// Return the raster image data track, if this actor carries one.
    pub fn image_track(&self) -> Option<&PropertyTrack<Option<SceneImage>>> {
        self.image_payload().and_then(|p| p.image.as_ref())
    }
}
