//! # Field Dispatch Module
//!
//! Central dispatch for reading/writing property tracks on [`AnimationTrack`].
//!
//! ## Key types
//!
//! - [`AnimationTrack`] — per-actor keyframed property container.
//! - [`TrackFieldRef`] / [`TrackFieldMut`] — type-erased references to property tracks, enabling
//!   generic dispatch over the property registry.
//!
//! ## Free functions
//!
//! - [`read_property_value`] — read a property value at a given time.
//! - [`read_property_value_or_default`] — read a property, falling back to the schema default.
//! - [`property_has_keyframes`] / [`property_has_keyframe_at`] — keyframe existence checks.
//! - [`property_keyframe_count`] / [`property_keyframe_times`] — keyframe metadata.
//! - [`property_keyframe_easing`] — easing at a specific keyframe.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::animation_track::{CalloutPlace, PlacementMode, PositionBinding};
use super::property_track::PropertyTrack;
use crate::easing::Easing;
use crate::renderer::types::{GradientSpec, TextPath, VelloPath};
use crate::timeline::morph::MorphOptions;
use crate::timeline::property_engine::EnumPropertyValue;
pub use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_registry::{ActorField, PropertySchema};

pub use super::track::{AnimationTrack, apply_split_text_animation};

// ─────────────────────────────────────────────────────────────
// AnimationTrack
// ─────────────────────────────────────────────────────────────

/// Per-actor container for all keyframed property tracks.
///
/// Every actor in the scene graph has exactly one `AnimationTrack`.  The track
/// stores typed optional property tracks (geometry, style, filter, shape, text,
/// highlight tiers), along with metadata such as the derived capability
/// projection `caps` and `visible`.
///
/// The scene graph is *not* stored here: parent/children links live in
/// [`crate::timeline::world::SceneHierarchyComponent`] (see `docs/ecs.md`), so
/// `world.hierarchy(id)` is the single source of truth for the hierarchy.
///
/// Primitive-specific state lives in exactly one place: [`Self::payload`]. An
/// actor carries at most one payload variant — a plot actor has a
/// [`crate::primitives::PlotPayload`], an `Svg` actor an
/// [`crate::primitives::SvgPayload`], an `Image` actor an
/// [`crate::primitives::ImagePayload`], and everything else
/// [`crate::primitives::PrimitivePayload::None`]. Read through the accessors
/// ([`Self::procedural_plot`], [`Self::func_transitions`],
/// [`Self::svg_paths`], [`Self::image_track`], …) rather than matching the enum,
/// and write through the `ensure_*_payload` forms — but only from the primitive
/// family that owns that variant, because `ensure_*` *replaces* the variant.
mod access;
mod field;

pub use access::{
    property_has_keyframe_at, property_has_keyframes, property_keyframe_count,
    property_keyframe_easing, property_keyframe_times, read_property_value,
    read_property_value_or_default,
};
pub use field::{TrackFieldMut, TrackFieldRef};
