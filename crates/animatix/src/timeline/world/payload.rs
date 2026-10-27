//! Entity-id keyed payload accessors over dedicated component stores.

use super::EntityId;
use super::World;
use crate::primitives::{ImagePayload, PlotPayload, PrimitivePayload, SvgPayload};
use crate::timeline::image::SceneImage;
use crate::timeline::plot::{BarDataTransition, FuncTransition, ProceduralPlot};
use crate::timeline::property_track::PropertyTrack;
use crate::timeline::{BarChartLayout, VelloPath};
use std::collections::HashMap;

impl World {
    /// Plot payload for an entity, when present.
    #[inline(always)]
    pub fn plot_payload(&self, id: EntityId) -> Option<&PlotPayload> {
        self.plot_payload.get(id)
    }

    /// Mutable plot payload for an entity, when present.
    #[inline(always)]
    pub fn plot_payload_mut(&mut self, id: EntityId) -> Option<&mut PlotPayload> {
        self.plot_payload.get_mut(id)
    }

    /// Ensure and borrow an entity's plot payload.
    #[inline(always)]
    pub fn ensure_plot_payload(&mut self, id: EntityId) -> &mut PlotPayload {
        self.plot_payload.get_or_insert_with(id, PlotPayload::default)
    }

    /// SVG payload for an entity, when present.
    #[inline(always)]
    pub fn svg_payload(&self, id: EntityId) -> Option<&SvgPayload> {
        self.svg_payload.get(id)
    }

    /// Mutable SVG payload for an entity, when present.
    #[inline(always)]
    pub fn svg_payload_mut(&mut self, id: EntityId) -> Option<&mut SvgPayload> {
        self.svg_payload.get_mut(id)
    }

    /// Ensure and borrow an entity's SVG payload.
    #[inline(always)]
    pub fn ensure_svg_payload(&mut self, id: EntityId) -> &mut SvgPayload {
        self.svg_payload.get_or_insert_with(id, SvgPayload::default)
    }

    /// Image payload for an entity, when present.
    #[inline(always)]
    pub fn image_payload(&self, id: EntityId) -> Option<&ImagePayload> {
        self.image_payload.get(id)
    }

    /// Mutable image payload for an entity, when present.
    #[inline(always)]
    pub fn image_payload_mut(&mut self, id: EntityId) -> Option<&mut ImagePayload> {
        self.image_payload.get_mut(id)
    }

    /// Ensure and borrow an entity's image payload.
    #[inline(always)]
    pub fn ensure_image_payload(&mut self, id: EntityId) -> &mut ImagePayload {
        self.image_payload.get_or_insert_with(id, ImagePayload::default)
    }

    /// Read the entity's primitive payload converted to a [`PrimitivePayload`] DTO for carry/snapshot compatibility.
    pub fn payload(&self, id: EntityId) -> Option<PrimitivePayload> {
        if let Some(plot) = self.plot_payload.get(id) {
            Some(PrimitivePayload::Plot(Box::new(plot.clone())))
        } else if let Some(svg) = self.svg_payload.get(id) {
            Some(PrimitivePayload::Svg(Box::new(svg.clone())))
        } else {
            self.image_payload
                .get(id)
                .map(|image| PrimitivePayload::Image(Box::new(image.clone())))
        }
    }

    /// Set an entity's primitive payload from a [`PrimitivePayload`] DTO for carry/snapshot compatibility.
    pub fn set_payload(&mut self, id: EntityId, payload: PrimitivePayload) {
        self.plot_payload.reset_at(id);
        self.svg_payload.reset_at(id);
        self.image_payload.reset_at(id);
        match payload {
            PrimitivePayload::None => {},
            PrimitivePayload::Plot(p) => {
                self.plot_payload.insert(id, *p);
            },
            PrimitivePayload::Svg(s) => {
                self.svg_payload.insert(id, *s);
            },
            PrimitivePayload::Image(i) => {
                self.image_payload.insert(id, *i);
            },
        }
    }

    /// Procedural plot generator for an entity.
    pub fn procedural_plot(&self, id: EntityId) -> Option<&ProceduralPlot> {
        self.plot_payload(id)?.procedural_plot.as_ref()
    }

    /// Per-parameter keyframe tracks for a plot entity.
    pub fn plot_param_tracks(&self, id: EntityId) -> Option<&HashMap<String, PropertyTrack<f64>>> {
        self.plot_payload(id).map(|p| &p.plot_param_tracks)
    }

    /// One plot parameter's keyframe track.
    pub fn plot_param_track(&self, id: EntityId, name: &str) -> Option<&PropertyTrack<f64>> {
        self.plot_payload(id)?.plot_param_tracks.get(name)
    }

    /// `func` transitions in stamp order.
    pub fn func_transitions(&self, id: EntityId) -> &[FuncTransition] {
        self.plot_payload(id).map_or(&[], |p| &p.func_transitions)
    }

    /// Mutable `func` transitions, initializing the plot payload.
    pub fn func_transitions_mut(&mut self, id: EntityId) -> &mut Vec<FuncTransition> {
        &mut self.ensure_plot_payload(id).func_transitions
    }

    /// Declaration bar dataset.
    pub fn bar_data(&self, id: EntityId) -> &[(String, f32)] {
        self.plot_payload(id).map_or(&[], |p| &p.bar_data)
    }

    /// Mutable bar dataset, initializing the plot payload.
    pub fn bar_data_mut(&mut self, id: EntityId) -> &mut Vec<(String, f32)> {
        &mut self.ensure_plot_payload(id).bar_data
    }

    /// `data = {…}` transitions.
    pub fn bar_data_transitions(&self, id: EntityId) -> &[BarDataTransition] {
        self.plot_payload(id).map_or(&[], |p| &p.bar_data_transitions)
    }

    /// Mutable `data = {…}` transitions.
    pub fn bar_data_transitions_mut(&mut self, id: EntityId) -> &mut Vec<BarDataTransition> {
        &mut self.ensure_plot_payload(id).bar_data_transitions
    }

    /// Bar layout resolved at build time.
    pub(crate) fn bar_layout(&self, id: EntityId) -> Option<&BarChartLayout> {
        self.plot_payload(id)?.bar_layout.as_ref()
    }

    /// Declaration-level SVG paths.
    pub fn svg_paths(&self, id: EntityId) -> &[VelloPath] {
        self.svg_payload(id).map_or(&[], |p| &p.svg_paths)
    }

    /// Raster image data track.
    pub fn image_track(&self, id: EntityId) -> Option<&PropertyTrack<Option<SceneImage>>> {
        self.image_payload(id)?.image.as_ref()
    }

    /// Evaluate SVG paths at `time_ms`, timed assignments first.
    pub fn svg_paths_at(&self, id: EntityId, time_ms: u64) -> Option<Vec<VelloPath>> {
        self.svg_payload(id)?.svg_paths_at(time_ms)
    }

    /// Whether the entity carries declaration or assignment SVG content.
    pub fn has_svg_path_content(&self, id: EntityId) -> bool {
        self.svg_payload(id).is_some_and(|payload| {
            !payload.svg_paths.is_empty()
                || payload.svg_paths_track.as_ref().is_some_and(|track| {
                    track.default_value.is_some() || !track.keyframes.is_empty()
                })
        })
    }
}
