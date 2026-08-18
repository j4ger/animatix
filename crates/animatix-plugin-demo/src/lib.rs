//! Example native plugin using the stable `animatix-plugin-api` ABI.
//!
//! Build with `cargo build -p animatix-plugin-demo`, then pass the resulting
//! shared library to `animatix check --plugin <library>`.

use std::ffi::{c_char, c_void};

use animatix_plugin_api::{
    NATIVE_PATH_ELLIPSE, NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
    NATIVE_PROPERTY_F32, NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_VALUE_NUM,
    NativePathCommand, NativePluginApi, NativePrimitive, NativePrimitiveEvaluateCtx, NativeService,
    NativeValue,
};

/// Return the ABI version implemented by this plugin.
#[unsafe(no_mangle)]
pub extern "C" fn animatix_plugin_abi_version() -> u32 {
    animatix_plugin_api::ABI_VERSION
}

/// Return the stable plugin name shown in loader diagnostics.
#[unsafe(no_mangle)]
pub extern "C" fn animatix_plugin_name() -> *const c_char {
    c"demo".as_ptr()
}

/// Install plugin capabilities through the current host API table.
///
/// # Safety
///
/// `api` and `host` must be the values passed by the Animatix loader to the
/// install entry point. `host` must remain valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn animatix_plugin_install(
    api: *const NativePluginApi,
    host: *mut c_void,
) -> i32 {
    let api = unsafe { &*api };
    if api.size != std::mem::size_of::<NativePluginApi>()
        || api.version != animatix_plugin_api::ABI_VERSION
    {
        return NATIVE_STATUS_TYPE_ERROR;
    }

    let property_status = unsafe {
        (api.register_property)(host, c"Rect".as_ptr(), c"glow".as_ptr(), NATIVE_PROPERTY_F32, true)
    };
    if property_status != NATIVE_STATUS_OK {
        return property_status;
    }

    let function_status = unsafe { (api.register_function)(host, c"double".as_ptr(), double) };
    if function_status != NATIVE_STATUS_OK {
        return function_status;
    }

    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        evaluate: Some(pulse_evaluate),
    };
    let primitive_status = unsafe { (api.register_primitive)(host, primitive) };
    if primitive_status != NATIVE_STATUS_OK {
        return primitive_status;
    }

    let action_status = unsafe { (api.register_action)(host, c"pulse".as_ptr(), pulse_action) };
    if action_status != NATIVE_STATUS_OK {
        return action_status;
    }

    let service = NativeService {
        name: c"demo.pulse".as_ptr(),
        value: pulse_action as *const () as usize,
        drop: Some(drop_service),
    };
    unsafe { (api.provide_service)(host, service) }
}

unsafe extern "C" fn double(
    args: *const NativeValue,
    arg_len: usize,
    _env: *const c_void,
    out: *mut NativeValue,
) -> i32 {
    if arg_len != 1 {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let arg = unsafe { &*args };
    if arg.tag != NATIVE_VALUE_NUM {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let result = NativeValue {
        tag: NATIVE_VALUE_NUM,
        num: arg.num * 2.0,
        ..NativeValue::default()
    };
    unsafe {
        *out = result;
    }
    NATIVE_STATUS_OK
}

/// Frame-time evaluate callback for the `Pulse` demo primitive.
///
/// # Safety
///
/// `ctx` must be the callback context passed by the Animatix host, and its
/// `append_path` function pointer must be called with the matching `host`.
unsafe extern "C" fn pulse_evaluate(ctx: *mut NativePrimitiveEvaluateCtx) -> i32 {
    let ctx = unsafe { &*ctx };
    let Some(append_path) = ctx.append_path else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let command = NativePathCommand {
        kind: NATIVE_PATH_ELLIPSE,
        x: 0.0,
        y: 0.0,
        width: 60.0,
        height: 60.0,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        points: std::ptr::null(),
        point_len: 0,
        fill: [0.2, 0.8, 1.0, 1.0],
        stroke: [1.0, 1.0, 1.0, 1.0],
        stroke_width: 2.0,
        line_cap: 0,
        line_join: 0,
    };
    unsafe { (append_path)(ctx.host, command) }
}

/// Execute callback for the `pulse` demo action.
///
/// # Safety
///
/// `host` must be the opaque callback context passed by the Animatix host.
unsafe extern "C" fn pulse_action(_host: *mut c_void) -> i32 {
    NATIVE_STATUS_OK
}

/// Destructor for the demo service value.
///
/// # Safety
///
/// `value` must be a service value previously provided by this plugin.
unsafe extern "C" fn drop_service(_value: usize) {}
