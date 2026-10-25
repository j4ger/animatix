//! # Field Dispatch Module
//!
//! Central dispatch for reading/writing property tracks on the per-entity
//! tier bundles.
//!
//! ## Key types
//!
//! - [`TrackFieldMut`] / [`TrackFieldRef`] — type-erased references to property tracks, enabling
//!   generic dispatch over the property registry.

use super::animation_track::{CalloutPlace, PlacementMode, PositionBinding};
use crate::renderer::types::{GradientSpec, TextPath, VelloPath};
use crate::timeline::morph::MorphOptions;
use crate::timeline::property_engine::EnumPropertyValue;
pub use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_track::PropertyTrack;

pub use super::animation_track::apply_split_text_animation;

mod access;
mod field;

pub use field::{TrackFieldMut, TrackFieldRef};
