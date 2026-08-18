//! Stable C ABI shared by the Animatix host and native extension plugins.
//!
//! Plugins are `cdylib` files that export [`ABI_VERSION`],
//! `animatix_plugin_name`-equivalent symbols, and the install entry point.
//! The host and plugin never pass Rust trait objects across the library
//! boundary; they exchange only `repr(C)` structs and function pointers.
//!
//! There is exactly one current ABI version. No compatibility layer is kept
//! for earlier in-repo ABI generations.

use std::ffi::{c_char, c_void};

/// ABI version negotiated by the host and plugin.
pub const ABI_VERSION: u32 = 3;

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
/// UTF-8 string runtime value tag.
pub const NATIVE_VALUE_STRING: u32 = 6;
/// Ordered list runtime value tag.
pub const NATIVE_VALUE_LIST: u32 = 7;
/// Unsigned integer runtime value tag.
pub const NATIVE_VALUE_U32: u32 = 8;
/// Point-list runtime value tag.
pub const NATIVE_VALUE_POINT_LIST: u32 = 9;
/// SVG command string runtime value tag.
pub const NATIVE_VALUE_COMMAND_LIST: u32 = 10;
/// String-list runtime value tag.
pub const NATIVE_VALUE_STRING_LIST: u32 = 11;
/// Affine transform runtime value tag.
pub const NATIVE_VALUE_TRANSFORM: u32 = 12;
/// Named enum runtime value tag.
pub const NATIVE_VALUE_ENUM: u32 = 13;
/// Named variant with a payload runtime value tag.
pub const NATIVE_VALUE_VARIANT: u32 = 14;

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

/// A finite runtime value exchanged with native extension callbacks.
///
/// String/list pointers are only valid for the duration of the callback that
/// delivered them. The host always copies string/list contents before the
/// callback returns; plugins must never retain these pointers.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct NativeValue {
    /// Value tag from the `NATIVE_VALUE_*` constants.
    pub tag: u32,
    /// Numeric payload for `NATIVE_VALUE_NUM`.
    pub num: f64,
    /// Boolean payload for `NATIVE_VALUE_BOOL`.
    pub boolean: bool,
    /// Vector/color payload for vector and color tags.
    pub vec: [f64; 4],
    /// UTF-8 string pointer and length for string-like tags.
    pub string: *const c_char,
    /// Byte length of `string`.
    pub string_len: usize,
    /// Element payload for list-like tags.
    pub list: *const NativeValue,
    /// Number of elements in `list`.
    pub list_len: usize,
    /// Transform payload for `NATIVE_VALUE_TRANSFORM`.
    pub transform: [f64; 6],
    /// Variant name for `NATIVE_VALUE_VARIANT`.
    pub variant: *const c_char,
    /// Variant payload for `NATIVE_VALUE_VARIANT`.
    pub payload: *const NativeValue,
}

impl Default for NativeValue {
    fn default() -> Self {
        Self {
            tag: NATIVE_VALUE_NUM,
            num: 0.0,
            boolean: false,
            vec: [0.0; 4],
            string: std::ptr::null(),
            string_len: 0,
            list: std::ptr::null(),
            list_len: 0,
            transform: [0.0; 6],
            variant: std::ptr::null(),
            payload: std::ptr::null(),
        }
    }
}

/// Descriptor passed from a native plugin to register an external property.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativePropertyDescriptor {
    /// Actor source type that owns this property.
    pub actor_type: *const c_char,
    /// Canonical source property name.
    pub name: *const c_char,
    /// Human-readable name for GUI labels.
    pub display_name: *const c_char,
    /// `NATIVE_PROPERTY_*` value kind.
    pub kind: u32,
    /// Whether the property is injected into frame environments.
    pub injectable: bool,
    /// Inspector grouping key.
    pub group: *const c_char,
    /// Help text for tooltips and documentation.
    pub help: *const c_char,
}

/// Native expression function callback.
///
/// Returns [`NATIVE_STATUS_OK`] on success or one of the `NATIVE_STATUS_*`
/// error codes. The output value is written into `out`.
pub type NativeFunction = unsafe extern "C" fn(
    args: *const NativeValue,
    arg_len: usize,
    env: *const c_void,
    out: *mut NativeValue,
) -> i32;

/// Primitive UI category.
pub const NATIVE_PRIMITIVE_CATEGORY_SHAPE: u32 = 0;
/// Primitive UI category.
pub const NATIVE_PRIMITIVE_CATEGORY_TEXT: u32 = 1;
/// Primitive UI category.
pub const NATIVE_PRIMITIVE_CATEGORY_MEDIA: u32 = 2;
/// Primitive UI category.
pub const NATIVE_PRIMITIVE_CATEGORY_PLOT: u32 = 3;
/// Primitive UI category.
pub const NATIVE_PRIMITIVE_CATEGORY_CONTAINER: u32 = 4;
/// Primitive UI category.
pub const NATIVE_PRIMITIVE_CATEGORY_ANNOTATION: u32 = 5;

/// Child-processing strategy.
pub const NATIVE_PRIMITIVE_CHILD_GENERIC: u32 = 0;
/// Child-processing strategy.
pub const NATIVE_PRIMITIVE_CHILD_FILTER: u32 = 1;
/// Child-processing strategy.
pub const NATIVE_PRIMITIVE_CHILD_MASK: u32 = 2;
/// Child-processing strategy.
pub const NATIVE_PRIMITIVE_CHILD_EQUATION: u32 = 3;

/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_RECT: u32 = 0;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_ELLIPSE: u32 = 1;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_LINE: u32 = 2;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_POLYGON: u32 = 3;

/// A 2D point used by polygon path commands.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct NativePoint {
    /// X coordinate in scene units.
    pub x: f64,
    /// Y coordinate in scene units.
    pub y: f64,
}

/// A vector path command emitted by a native primitive during evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub struct NativePathCommand {
    /// `NATIVE_PATH_*` kind.
    pub kind: u32,
    /// Rectangle origin / ellipse center X.
    pub x: f64,
    /// Rectangle origin / ellipse center Y.
    pub y: f64,
    /// Rectangle width / ellipse X radius.
    pub width: f64,
    /// Rectangle height / ellipse Y radius.
    pub height: f64,
    /// Line start X.
    pub x1: f64,
    /// Line start Y.
    pub y1: f64,
    /// Line end X.
    pub x2: f64,
    /// Line end Y.
    pub y2: f64,
    /// Polygon points; valid only for `NATIVE_PATH_POLYGON`.
    pub points: *const NativePoint,
    /// Number of polygon points.
    pub point_len: usize,
    /// RGBA fill color in 0..=1 ranges.
    pub fill: [f64; 4],
    /// RGBA stroke color in 0..=1 ranges.
    pub stroke: [f64; 4],
    /// Stroke width in scene units; zero disables the stroke.
    pub stroke_width: f64,
    /// Stroke line cap (0=Butt, 1=Round, 2=Square).
    pub line_cap: u32,
    /// Stroke line join (0=Miter, 1=Round, 2=Bevel).
    pub line_join: u32,
}

/// Context passed to a native primitive evaluate callback.
#[repr(C)]
pub struct NativePrimitiveEvaluateCtx {
    /// `size_of::<NativePrimitiveEvaluateCtx>()`.
    pub size: usize,
    /// Current evaluation time in milliseconds.
    pub time_ms: f64,
    /// Opaque host handle passed back to host callbacks.
    pub host: *mut c_void,
    /// Read a sampled actor property by name into `out`.
    pub get_property:
        Option<unsafe extern "C" fn(*mut c_void, *const c_char, *mut NativeValue) -> i32>,
    /// Append one vector path command to the current frame.
    pub append_path: Option<unsafe extern "C" fn(*mut c_void, NativePathCommand) -> i32>,
}

/// Native primitive evaluate callback.
pub type NativePrimitiveEvaluateFn = unsafe extern "C" fn(*mut NativePrimitiveEvaluateCtx) -> i32;

/// Native action execute callback.
pub type NativeActionExecuteFn = unsafe extern "C" fn(*mut c_void) -> i32;

/// Service value provided by a native plugin.
#[repr(C)]
pub struct NativeService {
    /// Canonical service name.
    pub name: *const c_char,
    /// Opaque service value understood by the plugin.
    pub value: usize,
    /// Optional destructor invoked when the host drops the service.
    pub drop: Option<unsafe extern "C" fn(usize)>,
}

/// Descriptor passed from a native plugin to register a primitive.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativePrimitive {
    /// Source text type name.
    pub type_name: *const c_char,
    /// Display name for GUI palettes.
    pub display_name: *const c_char,
    /// Opaque icon id.
    pub icon_id: *const c_char,
    /// `NATIVE_PRIMITIVE_CATEGORY_*` value.
    pub category: u32,
    /// Whether the primitive is hidden in the advanced menu.
    pub advanced: bool,
    /// `NATIVE_PRIMITIVE_CHILD_*` value.
    pub child_processing: u32,
    /// Optional frame-time evaluate callback.
    pub evaluate: Option<NativePrimitiveEvaluateFn>,
}

/// Host callbacks available to a native plugin during install.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativePluginApi {
    /// `size_of::<NativePluginApi>()` so future ABI revisions can be detected.
    pub size: usize,
    /// Current `ABI_VERSION`.
    pub version: u32,
    /// Register an external property for an actor type and write its runtime
    /// `PropertyId` into `out_id`.
    pub register_property:
        unsafe extern "C" fn(*mut c_void, NativePropertyDescriptor, *mut u32) -> i32,
    /// Register a native expression function.
    pub register_function: unsafe extern "C" fn(*mut c_void, *const c_char, NativeFunction) -> i32,
    /// Register a native primitive.
    pub register_primitive: unsafe extern "C" fn(*mut c_void, NativePrimitive) -> i32,
    /// Register a native action.
    pub register_action:
        unsafe extern "C" fn(*mut c_void, *const c_char, NativeActionExecuteFn) -> i32,
    /// Provide a native service value with an optional destructor.
    pub provide_service: unsafe extern "C" fn(*mut c_void, NativeService) -> i32,
}

/// Native plugin install entry point.
pub type NativeInstallFn = unsafe extern "C" fn(*const NativePluginApi, *mut c_void) -> i32;
