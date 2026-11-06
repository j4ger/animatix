use std::ffi::c_void;

use animatix_plugin_api::NATIVE_RESIZE_MODE_SIZE;

use super::*;
use crate::extension_context::ExtensionContext;
use crate::timeline::{Environment, PropertyKind, Value};

use helpers::*;

mod callbacks;
mod commands;
mod helpers;
mod host_registration;
mod plugin_api;
