//! Primitive-specific payload definitions.
//!
//! Decouples primitive-specific storage (plots, charts, SVG vector paths, raster images)
//! from the common [`AnimationTrack`] host struct.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::timeline::image::SceneImage;
use crate::timeline::plot::{BarDataTransition, FuncTransition, ProceduralPlot};
use crate::timeline::{BarChartLayout, PropertyTrack, VelloPath};

/// Payload for procedural plots, mathematical curves, and bar charts.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PlotPayload {
    /// Procedural plot generator, re-sampled each frame.
    pub procedural_plot: Option<ProceduralPlot>,
    /// Per-parameter keyframe tracks for procedural plot actors (e.g. "freq" -> f64 track).
    pub plot_param_tracks: HashMap<String, PropertyTrack<f64>>,
    /// Function transitions (side-channel for non-interpolatable closures).
    pub func_transitions: Vec<FuncTransition>,
    /// Layout resolved from the declaration for bar charts.
    #[cfg_attr(feature = "serde", serde(skip))]
    // Reserved for full migration of BarChartLayout from AnimationTrack to modular PlotPayload
    #[allow(dead_code)]
    pub(crate) bar_layout: Option<BarChartLayout>,
    /// Base dataset for bar charts.
    pub bar_data: Vec<(String, f32)>,
    /// Timed dataset transitions for bar charts.
    pub bar_data_transitions: Vec<BarDataTransition>,
}

/// Payload for vector SVG assets and timed SVG frame sequences.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SvgPayload {
    /// Static SVG paths from declarations.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub svg_paths: Vec<VelloPath>,
    /// Keyframed SVG paths from timed `Svg.url` assignments.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub svg_paths_track: Option<PropertyTrack<Option<Vec<VelloPath>>>>,
}

/// Payload for raster images.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ImagePayload {
    /// Raster image data track.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub image: Option<PropertyTrack<Option<SceneImage>>>,
}

/// Dynamic payload interface for native or third-party extension primitives.
pub trait CustomPrimitivePayload: std::fmt::Debug + Send + Sync {
    /// Clone the boxed custom payload.
    fn clone_box(&self) -> Box<dyn CustomPrimitivePayload>;
}

impl Clone for Box<dyn CustomPrimitivePayload> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Polymorphic payload holding primitive-specific state on an actor track.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PrimitivePayload {
    /// Default empty payload (used for basic shapes, containers, text without extra payload).
    #[default]
    None,
    /// Procedural plot or bar chart payload.
    Plot(Box<PlotPayload>),
    /// Vector SVG asset payload.
    Svg(Box<SvgPayload>),
    /// Raster image payload.
    Image(Box<ImagePayload>),
    /// Custom extension payload.
    #[cfg_attr(feature = "serde", serde(skip))]
    Custom(Box<dyn CustomPrimitivePayload>),
}

impl PrimitivePayload {
    /// Return a reference to the plot payload if present.
    pub fn as_plot(&self) -> Option<&PlotPayload> {
        match self {
            Self::Plot(p) => Some(p),
            _ => None,
        }
    }

    /// Return a mutable reference to the plot payload if present.
    pub fn as_plot_mut(&mut self) -> Option<&mut PlotPayload> {
        match self {
            Self::Plot(p) => Some(p),
            _ => None,
        }
    }

    /// Ensure the payload is initialized as a [`PlotPayload`] and return a mutable reference.
    pub fn ensure_plot(&mut self) -> &mut PlotPayload {
        if !matches!(self, Self::Plot(_)) {
            *self = Self::Plot(Box::default());
        }
        match self {
            Self::Plot(p) => p,
            _ => unreachable!(),
        }
    }

    /// Return a reference to the SVG payload if present.
    pub fn as_svg(&self) -> Option<&SvgPayload> {
        match self {
            Self::Svg(p) => Some(p),
            _ => None,
        }
    }

    /// Return a mutable reference to the SVG payload if present.
    pub fn as_svg_mut(&mut self) -> Option<&mut SvgPayload> {
        match self {
            Self::Svg(p) => Some(p),
            _ => None,
        }
    }

    /// Ensure the payload is initialized as a [`SvgPayload`] and return a mutable reference.
    pub fn ensure_svg(&mut self) -> &mut SvgPayload {
        if !matches!(self, Self::Svg(_)) {
            *self = Self::Svg(Box::default());
        }
        match self {
            Self::Svg(p) => p,
            _ => unreachable!(),
        }
    }

    /// Return a reference to the image payload if present.
    pub fn as_image(&self) -> Option<&ImagePayload> {
        match self {
            Self::Image(p) => Some(p),
            _ => None,
        }
    }

    /// Return a mutable reference to the image payload if present.
    pub fn as_image_mut(&mut self) -> Option<&mut ImagePayload> {
        match self {
            Self::Image(p) => Some(p),
            _ => None,
        }
    }

    /// Ensure the payload is initialized as an [`ImagePayload`] and return a mutable reference.
    pub fn ensure_image(&mut self) -> &mut ImagePayload {
        if !matches!(self, Self::Image(_)) {
            *self = Self::Image(Box::default());
        }
        match self {
            Self::Image(p) => p,
            _ => unreachable!(),
        }
    }
}
