//! BehaviorId, BehaviorFamily, BehaviorSource, and BehaviorDef.

use animatix_core::caps::{ActorCaps, ActorCategory, ChildProcessingKind};
use animatix_syntax::schema::ChildProcessingKind as SchemaChildProcessingKind;

/// Compact identifier for an actor behavior in the behavior table.
///
/// Built-in primitives occupy indices `0..36` in [`crate::primitives::BUILT_INS`] order;
/// extension primitives receive dynamically assigned indices `>= 36`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct BehaviorId(pub u16);

impl BehaviorId {
    /// Sentinel for an unresolved or unknown behavior.
    pub const UNKNOWN: Self = Self(u16::MAX);

    /// Convert to a `usize` index into the behavior registry.
    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0 as usize
    }

    /// Check if this behavior ID is valid (not `UNKNOWN`).
    #[inline(always)]
    pub const fn is_valid(self) -> bool {
        self.0 != u16::MAX
    }
}

impl From<u16> for BehaviorId {
    #[inline(always)]
    fn from(id: u16) -> Self {
        Self(id)
    }
}

impl From<usize> for BehaviorId {
    #[inline(always)]
    fn from(idx: usize) -> Self {
        Self(idx as u16)
    }
}

/// Coarse architectural category of a behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum BehaviorFamily {
    /// Vector shape geometry (Rect, Ellipse, Line, Arrow, Polygon, Path).
    Shape,
    /// Textual and typographic rendering (Text, Code, Math, Typst, Fragment, Cue, Counter).
    Text,
    /// Image, audio, or external media assets (Image, SVG, Audio).
    Media,
    /// Numerical, function, and coordinate plots (Graph, PlotCurve, VectorField, etc.).
    Plot,
    /// Layout-constrained flex containers (Row, Col, Grid, Stack).
    Container,
    /// GPU shader and backdrop compositing scopes (Filter, Glass, Mask).
    Effect,
    /// Mathematical equation layout containers.
    Equation,
    /// Speech-synchronized subtitle and caption containers.
    Captions,
    /// Targeted callouts, legends, and connectors.
    Annotation,
    /// Structural scene-graph hierarchy grouping without layout constraints.
    Group,
}

impl BehaviorFamily {
    /// Classify a behavior family from actor capabilities.
    ///
    /// Mirrors the classification logic in `timeline::primitive::PrimitiveFamilyDescriptor::from_info`,
    /// expanded to cover the full set of behavior families (including specialized effect scopes,
    /// equations, captions, and annotations).
    pub fn from_caps(caps: &ActorCaps) -> Self {
        match caps.child_processing {
            ChildProcessingKind::Equation => return Self::Equation,
            ChildProcessingKind::Captions => return Self::Captions,
            ChildProcessingKind::Filter
            | ChildProcessingKind::Glass
            | ChildProcessingKind::Mask => {
                return Self::Effect;
            },
            ChildProcessingKind::Generic => {},
        }

        match caps.category {
            ActorCategory::Annotation => return Self::Annotation,
            ActorCategory::Media => return Self::Media,
            _ => {},
        }

        if caps.text_paths {
            Self::Text
        } else if caps.plot_geometry {
            Self::Plot
        } else if caps.image_payload {
            Self::Media
        } else if caps.vector_paths {
            Self::Shape
        } else if caps.layout_container {
            Self::Container
        } else if caps.is_container {
            Self::Group
        } else {
            match caps.category {
                ActorCategory::Shape => Self::Shape,
                ActorCategory::Text => Self::Text,
                ActorCategory::Media => Self::Media,
                ActorCategory::Plot => Self::Plot,
                ActorCategory::Container => Self::Group,
                ActorCategory::Annotation => Self::Annotation,
            }
        }
    }
}

/// Storage source for a primitive behavior implementation.
#[derive(Clone)]
pub(crate) enum BehaviorSource {
    /// A static built-in primitive compiled into the binary.
    Builtin(&'static dyn crate::primitives::Primitive),
    /// A runtime-registered extension or plugin primitive.
    Extension(std::sync::Arc<dyn crate::primitives::Primitive>),
}

impl BehaviorSource {
    /// Borrow the underlying primitive trait object.
    #[inline(always)]
    pub fn as_primitive(&self) -> &dyn crate::primitives::Primitive {
        match self {
            Self::Builtin(p) => *p,
            Self::Extension(p) => p.as_ref(),
        }
    }
}

/// Complete immutable definition for an actor behavior in the engine.
#[derive(Clone)]
pub struct BehaviorDef {
    /// Compact unique identifier for this behavior.
    pub id: BehaviorId,
    /// Authored type name / registration identifier.
    pub name: Box<str>,
    /// Capability projection for actors with this behavior.
    pub caps: ActorCaps,
    /// Architectural behavior family.
    pub family: BehaviorFamily,
    /// Child-rendering strategy.
    pub child_processing: SchemaChildProcessingKind,
    pub(crate) source: BehaviorSource,
}

impl BehaviorDef {
    /// The primitive implementation backing this behavior.
    #[inline(always)]
    pub fn primitive(&self) -> &dyn crate::primitives::Primitive {
        self.source.as_primitive()
    }
}

impl std::fmt::Debug for BehaviorDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BehaviorDef")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("caps", &self.caps)
            .field("family", &self.family)
            .field("child_processing", &self.child_processing)
            .finish()
    }
}
