//! ABI 10 ordinary functions and inert timeline-plan construction.

use std::ffi::{c_char, c_void};

use crate::NativeValue;

/// A duration value; `NativeValue::num` contains seconds.
pub const NATIVE_VALUE_DURATION: u32 = 15;
/// A call-scoped actor handle in `NativeValue::handle`.
pub const NATIVE_VALUE_ACTOR: u32 = 16;
/// A timeline result assembled through the host plan writer.
pub const NATIVE_VALUE_TIMELINE: u32 = 17;
/// A result with no value.
pub const NATIVE_VALUE_UNIT: u32 = 18;
/// An easing value in `NativeValue::easing`.
pub const NATIVE_VALUE_EASING: u32 = 19;
/// Missing optional argument or explicit null, distinct from a Unit result.
pub const NATIVE_VALUE_NULL: u32 = 20;
/// A function restricted to document construction.
pub const NATIVE_CALLABLE_BUILD: u32 = 0;
/// A pure function usable during construction or frame sampling.
pub const NATIVE_CALLABLE_BUILD_AND_FRAME: u32 = 1;
/// Parameterized damped spring easing.
pub const NATIVE_EASING_SPRING: u32 = 10;
/// Parameterized cubic Bezier easing.
pub const NATIVE_EASING_CUBIC_BEZIER: u32 = 11;
/// Spring easing with a normalized initial velocity.
pub const NATIVE_EASING_SPRING_V0: u32 = 12;

/// A complete easing payload; ordinary curves use zero parameters.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[repr(C)]
pub struct NativeEasing {
    /// A `NATIVE_EASING_*` code.
    pub code: u32,
    /// Parameter count: zero, two for spring, three for spring-v0, four for Bezier.
    pub param_len: usize,
    /// Curve parameters in source argument order.
    pub params: [f64; 4],
}

/// One ordinary function parameter; all names are available to named calls.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeCallableParam {
    /// Valid source identifier.
    pub name: *const c_char,
    /// Parameter type, using AMX type notation.
    pub type_info: *const c_char,
    /// Parameter documentation.
    pub help: *const c_char,
    /// A copied literal default, or null if there is none.
    pub default_value: *const NativeValue,
    /// Whether the parameter may be omitted without a default.
    /// Omitted arguments are passed as `NATIVE_VALUE_NULL`.
    pub optional: bool,
    /// Whether this final parameter collects trailing positional arguments.
    pub variadic: bool,
}

/// An ordinary native function, including one that returns a timeline plan.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct NativeCallableDescriptor {
    /// Required namespace, distinct from `std`.
    pub namespace: *const c_char,
    /// Unqualified source name.
    pub name: *const c_char,
    /// Ordered parameters.
    pub params: *const NativeCallableParam,
    /// Parameter count.
    pub param_len: usize,
    /// Required result type.
    pub return_type: *const c_char,
    /// A `NATIVE_CALLABLE_*` execution phase.
    pub phase: u32,
    /// Function documentation.
    pub help: *const c_char,
    /// Documentation grouping.
    pub category: *const c_char,
    /// Function implementation.
    pub callback: NativeCallableFn,
}

/// Host services available during one native function invocation.
#[repr(C)]
pub struct NativeCallableContext {
    /// `size_of::<NativeCallableContext>()`.
    pub size: usize,
    /// Opaque invocation-owned host handle.
    pub host: *mut c_void,
    /// Read an environment value; returned handles are valid only for this call.
    pub get_env: unsafe extern "C" fn(*mut c_void, *const c_char, *mut NativeValue) -> i32,
    /// Read a registered opaque service.
    pub get_service: unsafe extern "C" fn(*mut c_void, *const c_char, *mut usize) -> i32,
    /// Resolve a property registered by this plugin to its host-local id.
    pub get_property_id: unsafe extern "C" fn(*mut c_void, *const c_char, *mut u32) -> i32,
    /// Append an extension-property operation to an inert plan. Times are
    /// local seconds; handles must originate in this invocation. A failed
    /// append invalidates the entire returned plan, even if the plugin ignores
    /// the status. This callback never writes a World or a property track.
    pub append_property_op:
        unsafe extern "C" fn(*mut c_void, u64, u32, NativeValue, f64, f64, NativeEasing) -> i32,
}

/// Native function callback. The host has already bound arguments into
/// descriptor order. Timeline functions append operations and return a value
/// tagged `NATIVE_VALUE_TIMELINE`; other functions write their ordinary result.
pub type NativeCallableFn = unsafe extern "C" fn(
    *mut NativeCallableContext,
    *const NativeValue,
    usize,
    *mut NativeValue,
) -> i32;
