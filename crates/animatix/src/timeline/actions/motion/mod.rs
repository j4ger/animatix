use super::registry::{BuiltinAction, CallableDescriptor};
#[cfg(test)]
use crate::ast::Modifier;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::timeline::Timeline;
use crate::timeline::actions::BuiltinInvocation;
use crate::timeline::property_track::TrackAccessor;

use motion_impl::{parse_num_modifier, parse_positive_num_modifier, parse_vec2_modifier};

/// Moves the target to an absolute local translation offset.
mod acts;
mod helpers;
mod motion_impl;
mod move_action;
#[cfg(test)]
mod tests;

pub use acts::{Rotate, Scale, Shift};
pub use move_action::Move;
