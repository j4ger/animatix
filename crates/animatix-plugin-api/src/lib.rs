//! Stable C ABI shared by the Animatix host and native extension plugins.
//!
//! Plugins are `cdylib` files that export [`ABI_VERSION`],
//! `animatix_plugin_name`-equivalent symbols, and the install entry point.
//! The host and plugin never pass Rust trait objects across the library
//! boundary; they exchange only `repr(C)` structs and function pointers.

use std::ffi::{c_char, c_void};

/// ABI version negotiated by the host and plugin.
pub const ABI_VERSION: u32 = 1;

/// Numeric runtime value tag.
pub const NATIVE_VALUE_NUM: u32 = 0;
/// Boolean runtime value tag.
pub const NATIVE_VALUE_BOOL: u32 = 1;
/// Two-component runtime value tag.
pub const NATIVE_VALUE_VEC2: u32 = 2;
/// Three-component runtime value tag.
pub const NATIVE_VALUE_VEC3: u32 = 3;
/// Four-component runtime value tag.
pub const NATIVE_VALUE_VEC4: u32 = 4;
/// RGBA color runtime value tag.
pub const NATIVE_VALUE_COLOR: u32 = 5;

/// 32-bit float property kind.
pub const NATIVE_PROPERTY_F32: u32 = 0;
/// 32-bit unsigned integer property kind.
pub const NATIVE_PROPERTY_U32: u32 = 1;
/// 2D vector property kind.
pub const NATIVE_PROPERTY_VEC2: u32 = 2;
/// 4D vector/color property kind.
pub const NATIVE_PROPERTY_VEC4: u32 = 3;
/// String property kind.
pub const NATIVE_PROPERTY_STRING: u32 = 4;
/// Point-list property kind.
pub const NATIVE_PROPERTY_POINT_LIST: u32 = 5;
/// Generic finite-value property kind.
pub const NATIVE_PROPERTY_GENERIC: u32 = 6;

/// Success status for plugin install and function calls.
pub const NATIVE_STATUS_OK: i32 = 0;
/// Type mismatch or invalid plugin input.
pub const NATIVE_STATUS_TYPE_ERROR: i32 = 1;
/// Unsupported construct or operation.
pub const NATIVE_STATUS_UNSUPPORTED: i32 = 2;

/// A finite runtime value exchanged with native expression functions.
///
/// Strings, lists, and closures are intentionally not part of ABI v1. Plugins
/// that need richer values should return a type error or use a registered
/// property track instead.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct NativeValueV1 {
    /// Value tag from the `NATIVE_VALUE_*` constants.
    pub tag: u32,
    /// Numeric payload for `NATIVE_VALUE_NUM`.
    pub num: f64,
    /// Boolean payload for `NATIVE_VALUE_BOOL`.
    pub boolean: bool,
    /// Vector/color payload for vector and color tags.
    pub vec: [f64; 4],
}

impl Default for NativeValueV1 {
    fn default() -> Self {
        Self {
            tag: NATIVE_VALUE_NUM,
            num: 0.0,
            boolean: false,
            vec: [0.0; 4],
        }
    }
}

/// Native expression function callback.
///
/// Returns [`NATIVE_STATUS_OK`] on success or one of the `NATIVE_STATUS_*`
/// error codes. The output value is written into `out`.
pub type NativeFunctionV1 = unsafe extern "C" fn(
    args: *const NativeValueV1,
    arg_len: usize,
    env: *const c_void,
    out: *mut NativeValueV1,
) -> i32;

/// Host callbacks available to a native plugin during install.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativePluginApiV1 {
    /// `size_of::<NativePluginApiV1>()` so future ABI revisions can grow the
    /// table without breaking old plugins.
    pub size: usize,
    /// Register an external property for an actor type.
    pub register_property:
        unsafe extern "C" fn(*mut c_void, *const c_char, *const c_char, u32, bool) -> i32,
    /// Register a native expression function.
    pub register_function:
        unsafe extern "C" fn(*mut c_void, *const c_char, NativeFunctionV1) -> i32,
}

/// Native plugin install entry point.
pub type NativeInstallFn = unsafe extern "C" fn(*const NativePluginApiV1, *mut c_void) -> i32;
