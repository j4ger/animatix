//! Unstable C ABI shared by the Animatix host and native extension plugins.
//!
//! Plugins are `cdylib` files that export [`UNSTABLE_ABI_VERSION`],
//! `animatix_plugin_name`-equivalent symbols, and the install entry point.
//! The host and plugin never pass Rust trait objects across the library
//! boundary; they exchange only `repr(C)` structs and function pointers.
//!
//! There is exactly one current ABI snapshot. The ABI is intentionally
//! unstable: no compatibility layer is kept for earlier in-repo snapshots, and
//! plugins must be rebuilt from the same source tree as the host.

use std::ffi::{c_char, c_void};

/// Unstable ABI snapshot id negotiated by the host and plugin.
///
/// This is not a compatibility version. Bump it whenever the `repr(C)` layout
/// or callback table changes; the host rejects any other snapshot.
pub const UNSTABLE_ABI_VERSION: u32 = 5;

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
/// Boolean property kind.
pub const NATIVE_PROPERTY_BOOL: u32 = 7;
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
/// Named enum property kind stored as a string choice.
pub const NATIVE_PROPERTY_ENUM: u32 = 8;

/// Success status for plugin install and function calls.
pub const NATIVE_STATUS_OK: i32 = 0;
/// Type mismatch or invalid plugin input.
pub const NATIVE_STATUS_TYPE_ERROR: i32 = 1;
/// Unsupported construct or operation.
pub const NATIVE_STATUS_UNSUPPORTED: i32 = 2;

/// Easing code: linear.
pub const NATIVE_EASING_LINEAR: u32 = 0;
/// Easing code: ease in.
pub const NATIVE_EASING_IN: u32 = 1;
/// Easing code: ease out.
pub const NATIVE_EASING_OUT: u32 = 2;
/// Easing code: ease in-out.
pub const NATIVE_EASING_IN_OUT: u32 = 3;

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
    /// Optional precise tooling type string, e.g. `"Color"` or `"Bool"`.
    pub type_info: *const c_char,
    /// Whether the property is injected into frame environments.
    pub injectable: bool,
    /// Inspector grouping key.
    pub group: *const c_char,
    /// Help text for tooltips and documentation.
    pub help: *const c_char,
}

/// A build-time property value passed to a native primitive.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativePropertyValue {
    /// Canonical property name.
    pub name: *const c_char,
    /// Evaluated property value.
    pub value: NativeValue,
}

/// A build-time modifier value passed to a native primitive.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeModifierValue {
    /// Optional modifier name; null for positional modifiers.
    pub name: *const c_char,
    /// Evaluated modifier value.
    pub value: NativeValue,
}

/// A child declaration exposed to a native primitive build callback.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeChild {
    /// Child actor label; empty for anonymous inline items.
    pub label: *const c_char,
    /// Child actor type name.
    pub type_name: *const c_char,
    /// Evaluated child properties.
    pub properties: *const NativePropertyValue,
    /// Number of child properties.
    pub property_len: usize,
}

/// Context passed to a native primitive build callback.
#[repr(C)]
pub struct NativePrimitiveBuildCtx {
    /// `size_of::<NativePrimitiveBuildCtx>()`.
    pub size: usize,
    /// Current build time in milliseconds.
    pub time_ms: f64,
    /// Opaque host handle passed back to host callbacks.
    pub host: *mut c_void,
    /// Optional parent actor label.
    pub parent_label: *const c_char,
    /// Number of actor properties.
    pub get_property_count: unsafe extern "C" fn(*mut c_void) -> usize,
    /// Read one actor property by index.
    pub get_property: unsafe extern "C" fn(*mut c_void, usize, *mut NativePropertyValue) -> i32,
    /// Number of actor modifiers.
    pub get_modifier_count: unsafe extern "C" fn(*mut c_void) -> usize,
    /// Read one actor modifier by index.
    pub get_modifier: unsafe extern "C" fn(*mut c_void, usize, *mut NativeModifierValue) -> i32,
    /// Number of child declarations.
    pub get_child_count: unsafe extern "C" fn(*mut c_void) -> usize,
    /// Read one child declaration by index.
    pub get_child: unsafe extern "C" fn(*mut c_void, usize, *mut NativeChild) -> i32,
    /// Report a build diagnostic.
    pub report_diagnostic:
        unsafe extern "C" fn(*mut c_void, *const c_char, u32, *const c_char) -> i32,
}

/// Native primitive build callback.
pub type NativePrimitiveBuildFn = unsafe extern "C" fn(*mut NativePrimitiveBuildCtx) -> i32;

/// Context passed to a native primitive assignment callback.
#[repr(C)]
pub struct NativeAssignmentContext {
    /// `size_of::<NativeAssignmentContext>()`.
    pub size: usize,
    /// Opaque host handle passed back to host callbacks.
    pub host: *mut c_void,
    /// Property name being assigned.
    pub property: *const c_char,
    /// Animation start time in milliseconds.
    pub t_start_ms: u64,
    /// Animation end time in milliseconds.
    pub t_end_ms: u64,
    /// Easing code (0=Linear, 1=EaseIn, 2=EaseOut, 3=EaseInOut).
    pub easing: u32,
    /// Read the assigned value.
    pub get_value: unsafe extern "C" fn(*mut c_void, *mut NativeValue) -> i32,
    /// Write one extension property keyframe.
    pub write_keyframe:
        unsafe extern "C" fn(*mut c_void, *const c_char, NativeValue, u64, u64, u32) -> i32,
}

/// Native primitive assignment callback.
pub type NativeAssignmentFn = unsafe extern "C" fn(*mut NativeAssignmentContext) -> i32;

/// Context passed to a native primitive finalize callback.
#[repr(C)]
pub struct NativeFinalizeContext {
    /// `size_of::<NativeFinalizeContext>()`.
    pub size: usize,
    /// Opaque host handle passed back to host callbacks.
    pub host: *mut c_void,
    /// Actor label.
    pub label: *const c_char,
    /// Number of children already built into the timeline.
    pub child_count: usize,
}

/// Native primitive finalize callback.
pub type NativeFinalizeFn = unsafe extern "C" fn(*mut NativeFinalizeContext) -> i32;

/// Native expression function callback.
///
/// Returns [`NATIVE_STATUS_OK`] on success or one of the `NATIVE_STATUS_*`
/// error codes. The output value is written into `out`.
pub type NativeFunction = unsafe extern "C" fn(
    *mut NativeFunctionContext,
    args: *const NativeValue,
    arg_len: usize,
    out: *mut NativeValue,
) -> i32;

/// Descriptor passed from a native plugin to register an expression function.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeFunctionDescriptor {
    /// Function name as written in source.
    pub name: *const c_char,
    /// Positional parameter descriptions.
    pub params: *const NativeActionParam,
    /// Number of positional parameters.
    pub param_len: usize,
    /// Optional return type string, e.g. `"Num"` or `"Color"`.
    pub return_type: *const c_char,
    /// Optional help text.
    pub help: *const c_char,
    /// Expression callback.
    pub callback: NativeFunction,
}

/// Context passed to a native expression function callback.
#[repr(C)]
pub struct NativeFunctionContext {
    /// `size_of::<NativeFunctionContext>()`.
    pub size: usize,
    /// Opaque host handle passed back to host callbacks.
    pub host: *mut c_void,
    /// Read a frame-environment value by name.
    pub get_env: unsafe extern "C" fn(*mut c_void, *const c_char, *mut NativeValue) -> i32,
    /// Read a native service value by name.
    pub get_service: unsafe extern "C" fn(*mut c_void, *const c_char, *mut usize) -> i32,
}

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

/// Primitive capability: emits text glyph paths.
pub const NATIVE_CAP_TEXT_PATHS: u32 = 1 << 0;
/// Primitive capability: emits vector paths.
pub const NATIVE_CAP_VECTOR_PATHS: u32 = 1 << 1;
/// Primitive capability: carries a raster image payload.
pub const NATIVE_CAP_IMAGE_PAYLOAD: u32 = 1 << 2;
/// Primitive capability: participates in layout containers.
pub const NATIVE_CAP_LAYOUT_CONTAINER: u32 = 1 << 3;
/// Primitive capability: supports path morphing.
pub const NATIVE_CAP_MORPHABLE_PATHS: u32 = 1 << 4;
/// Primitive capability: is a vector reveal target.
pub const NATIVE_CAP_VECTOR_REVEAL_TARGET: u32 = 1 << 5;
/// Primitive capability: emits plot geometry.
pub const NATIVE_CAP_PLOT_GEOMETRY: u32 = 1 << 6;
/// Primitive capability: is a container primitive.
pub const NATIVE_CAP_IS_CONTAINER: u32 = 1 << 7;
/// Primitive capability: is a vector shape.
pub const NATIVE_CAP_IS_SHAPE: u32 = 1 << 8;
/// Primitive capability: hosts plot-curve children in math coordinates.
pub const NATIVE_CAP_PLOT_HOST: u32 = 1 << 9;

/// Resize mode: editor sizes the actor bounds directly.
pub const NATIVE_RESIZE_MODE_SIZE: u32 = 0;
/// Resize mode: editor scales the actor uniformly.
pub const NATIVE_RESIZE_MODE_SCALE: u32 = 1;

/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_RECT: u32 = 0;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_ELLIPSE: u32 = 1;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_LINE: u32 = 2;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_POLYGON: u32 = 3;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_CUBIC: u32 = 4;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_QUADRATIC: u32 = 5;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_ARC: u32 = 6;
/// Primitive path command kind for evaluation.
pub const NATIVE_PATH_ROUNDED_RECT: u32 = 7;

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
    /// Corner radius for rounded rectangles / arc radius.
    pub radius: f64,
    /// Arc start angle in radians.
    pub start_angle: f64,
    /// Arc sweep angle in radians.
    pub sweep_angle: f64,
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

/// Text command kind: normal text glyphs.
pub const NATIVE_TEXT_KIND_TEXT: u32 = 0;
/// Text command kind: syntax-highlighted code glyphs.
pub const NATIVE_TEXT_KIND_CODE: u32 = 1;
/// Text command kind: Typst/math glyphs.
pub const NATIVE_TEXT_KIND_TYST: u32 = 2;

/// Text command emitted by a native primitive during evaluation.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeTextCommand {
    /// UTF-8 text content pointer and byte length.
    pub content: *const c_char,
    /// Byte length of `content`.
    pub content_len: usize,
    /// Font family name.
    pub font_family: *const c_char,
    /// Font size in scene units.
    pub font_size: f64,
    /// Font weight.
    pub font_weight: f64,
    /// Font style.
    pub font_style: *const c_char,
    /// Line height multiplier.
    pub line_height: f64,
    /// Letter spacing.
    pub letter_spacing: f64,
    /// Word spacing.
    pub word_spacing: f64,
    /// Text color.
    pub color: [f64; 4],
    /// Maximum text width in scene units; zero disables wrapping.
    pub max_width: f64,
    /// Text alignment.
    pub text_align: *const c_char,
    /// Overflow behavior.
    pub overflow: *const c_char,
    /// `NATIVE_TEXT_KIND_*` renderer choice.
    pub kind: u32,
}

/// Image command emitted by a native primitive during evaluation.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeImageCommand {
    /// Image URL; if null, the actor's currently loaded image is used.
    /// Non-null URLs are resolved from the timeline's cached image assets.
    pub url: *const c_char,
    /// Natural display width and height in scene units.
    pub natural_size: [f64; 2],
}

/// Highlight command emitted by a native primitive during evaluation.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeHighlightCommand {
    /// Rectangle as `[x0, y0, x1, y1]`.
    pub rect: [f64; 4],
    /// RGBA fill color.
    pub color: [f64; 4],
    /// Layer alpha.
    pub alpha: f64,
    /// Corner radius.
    pub corner_radius: f64,
    /// Blend mode code (0=Normal, 1=Multiply, 2=Difference, 3=Screen).
    pub blend: u32,
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
    /// Read a native service value by name.
    pub get_service: Option<unsafe extern "C" fn(*mut c_void, *const c_char, *mut usize) -> i32>,
    /// Append one vector path command to the current frame.
    pub append_path: Option<unsafe extern "C" fn(*mut c_void, NativePathCommand) -> i32>,
    /// Append compiled text glyphs to the current frame.
    pub append_text: Option<unsafe extern "C" fn(*mut c_void, NativeTextCommand) -> i32>,
    /// Append an image to the current frame.
    pub append_image: Option<unsafe extern "C" fn(*mut c_void, NativeImageCommand) -> i32>,
    /// Append a highlight layer to the current frame.
    pub append_highlight: Option<unsafe extern "C" fn(*mut c_void, NativeHighlightCommand) -> i32>,
}

/// Native primitive evaluate callback.
pub type NativePrimitiveEvaluateFn = unsafe extern "C" fn(*mut NativePrimitiveEvaluateCtx) -> i32;

/// One action parameter or modifier descriptor.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeActionParam {
    /// Parameter name as used in source.
    pub name: *const c_char,
    /// Human-readable description.
    pub description: *const c_char,
    /// Expected type string.
    pub type_info: *const c_char,
}

/// Descriptor passed from a native plugin to register an action.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeAction {
    /// Action verb as written in source.
    pub name: *const c_char,
    /// High-level grouping for UI.
    pub category: *const c_char,
    /// One-line description.
    pub description: *const c_char,
    /// Positional parameter descriptors.
    pub params: *const NativeActionParam,
    /// Number of positional parameters.
    pub param_len: usize,
    /// Named modifier descriptors.
    pub modifiers: *const NativeActionParam,
    /// Number of named modifiers.
    pub modifier_len: usize,
    /// Execute callback.
    pub execute: NativeActionExecuteFn,
}

/// Context passed to a native action execute callback.
#[repr(C)]
pub struct NativeActionContext {
    /// `size_of::<NativeActionContext>()`.
    pub size: usize,
    /// Opaque host handle passed back to host callbacks.
    pub host: *mut c_void,
    /// Current action time in milliseconds.
    pub time_ms: f64,
    /// Number of action targets.
    pub get_target_count: unsafe extern "C" fn(*mut c_void) -> usize,
    /// Read one action target.
    pub get_target: unsafe extern "C" fn(*mut c_void, usize, *mut *const c_char) -> i32,
    /// Number of positional action arguments.
    pub get_arg_count: unsafe extern "C" fn(*mut c_void) -> usize,
    /// Read one positional action argument.
    pub get_arg: unsafe extern "C" fn(*mut c_void, usize, *mut NativeValue) -> i32,
    /// Number of action modifiers.
    pub get_modifier_count: unsafe extern "C" fn(*mut c_void) -> usize,
    /// Read one action modifier.
    pub get_modifier: unsafe extern "C" fn(*mut c_void, usize, *mut NativeModifierValue) -> i32,
    /// Write an extension property keyframe on an actor.
    pub write_keyframe: unsafe extern "C" fn(
        *mut c_void,
        *const c_char,
        *const c_char,
        NativeValue,
        u64,
        u64,
        u32,
    ) -> i32,
    /// Read a native service value by name.
    pub get_service: unsafe extern "C" fn(*mut c_void, *const c_char, *mut usize) -> i32,
}

/// Native action execute callback.
pub type NativeActionExecuteFn = unsafe extern "C" fn(*mut NativeActionContext) -> i32;

/// Service value provided by a native plugin.
#[repr(C)]
pub struct NativeService {
    /// Canonical service name.
    pub name: *const c_char,
    /// Optional type information for tooling.
    pub type_info: *const c_char,
    /// Optional help text.
    pub help: *const c_char,
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
    /// `NATIVE_CAP_*` capability flags.
    pub capabilities: u32,
    /// Names of properties declared by this primitive.
    pub properties: *const *const c_char,
    /// Number of property names in `properties`.
    pub property_len: usize,
    /// Whether the primitive is hidden in the advanced menu.
    pub advanced: bool,
    /// `NATIVE_PRIMITIVE_CHILD_*` value.
    pub child_processing: u32,
    /// `NATIVE_RESIZE_MODE_*` value.
    pub resize_mode: u32,
    /// Optional build callback.
    pub build: Option<NativePrimitiveBuildFn>,
    /// Optional frame-time evaluate callback.
    pub evaluate: Option<NativePrimitiveEvaluateFn>,
    /// Optional assignment callback.
    pub handle_assignment: Option<NativeAssignmentFn>,
    /// Optional post-children finalize callback.
    pub finalize_container_build: Option<NativeFinalizeFn>,
}

/// Host callbacks available to a native plugin during install.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativePluginApi {
    /// `size_of::<NativePluginApi>()` so layout changes are detected.
    pub size: usize,
    /// Current `UNSTABLE_ABI_VERSION`.
    pub version: u32,
    /// Register an external property for an actor type and write its runtime
    /// `PropertyId` into `out_id`.
    pub register_property:
        unsafe extern "C" fn(*mut c_void, NativePropertyDescriptor, *mut u32) -> i32,
    /// Register a native expression function.
    pub register_function: unsafe extern "C" fn(*mut c_void, NativeFunctionDescriptor) -> i32,
    /// Register a native primitive.
    pub register_primitive: unsafe extern "C" fn(*mut c_void, NativePrimitive) -> i32,
    /// Register a native action.
    pub register_action: unsafe extern "C" fn(*mut c_void, NativeAction) -> i32,
    /// Provide a native service value with an optional destructor.
    pub provide_service: unsafe extern "C" fn(*mut c_void, NativeService) -> i32,
}

/// Native plugin install entry point.
pub type NativeInstallFn = unsafe extern "C" fn(*const NativePluginApi, *mut c_void) -> i32;
