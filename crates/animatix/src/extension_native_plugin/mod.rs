//! Native cdylib plugin loading behind the `plugin-loading` feature.
//!
//! Plugins communicate through the unstable in-repo ABI in
//! `animatix-plugin-api`, so their Rust version and internal types can differ
//! from the host. The host keeps the loaded `Library` alive for the lifetime
//! of registered callbacks.

mod action;
pub(crate) mod convert;
pub(crate) mod hosts;
mod native;
mod primitive;
mod registration;
mod service;

#[cfg(test)]
mod tests;

#[cfg(test)]
use std::any::Any;
#[cfg(test)]
use std::collections::HashMap;
#[cfg(test)]
use std::ffi::c_char;
#[cfg(test)]
use std::sync::Arc;

#[cfg(test)]
use crate::ast::{Expr, Property};
#[cfg(test)]
pub(crate) use crate::ast::{InlineItem, Modifier};
#[cfg(test)]
use crate::primitives::{ChildProcessing, EvaluateCtx, Primitive};
#[cfg(test)]
pub(crate) use crate::timeline::ActorCategory;
#[cfg(test)]
use crate::timeline::ResizeMode;
#[cfg(test)]
use crate::timeline::actions::registry::{ActionSignature, BuiltinAction};
#[cfg(test)]
pub(crate) use crate::timeline::property_track::TrackAccessor;
#[cfg(test)]
pub(crate) use animatix_plugin_api::*;

pub(crate) use action::*;
pub(crate) use convert::*;
pub(crate) use hosts::*;
pub use native::NativePlugin;
pub(crate) use primitive::*;
pub(crate) use registration::*;
pub(crate) use service::*;
