//! Example native plugin using the stable `animatix-plugin-api` ABI.
//!
//! Build with `cargo build -p animatix-plugin-demo`, then pass the resulting
//! shared library to `animatix check --plugin <library>`.

use std::ffi::{c_char, c_void};

use animatix_plugin_api::{
    NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC, NATIVE_PROPERTY_F32,
    NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_VALUE_NUM, NativePluginApiV1,
    NativePluginApiV2, NativePrimitiveV2, NativeValueV1,
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

/// Install plugin capabilities through the v1 host API table.
///
/// # Safety
///
/// `api` and `host` must be the values passed by the Animatix loader to the
/// install entry point. `host` must remain valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn animatix_plugin_install_v1(
    api: *const NativePluginApiV1,
    host: *mut c_void,
) -> i32 {
    let api = unsafe { &*api };
    let property_status = unsafe {
        (api.register_property)(host, c"Rect".as_ptr(), c"glow".as_ptr(), NATIVE_PROPERTY_F32, true)
    };
    if property_status != NATIVE_STATUS_OK {
        return property_status;
    }
    unsafe { (api.register_function)(host, c"double".as_ptr(), double) }
}

/// Install plugin capabilities through the unified v2 host API table.
///
/// # Safety
///
/// `api` and `host` must be the values passed by the Animatix loader to the
/// install entry point. `host` must remain valid for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn animatix_plugin_install_v2(
    api: *const NativePluginApiV2,
    host: *mut c_void,
) -> i32 {
    let api = unsafe { &*api };
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

    let primitive = NativePrimitiveV2 {
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

    unsafe {
        (api.provide_service)(host, c"demo.pulse".as_ptr(), pulse_action as *const () as usize)
    }
}

unsafe extern "C" fn double(
    args: *const NativeValueV1,
    arg_len: usize,
    _env: *const c_void,
    out: *mut NativeValueV1,
) -> i32 {
    if arg_len != 1 {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let arg = unsafe { &*args };
    if arg.tag != NATIVE_VALUE_NUM {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let result = NativeValueV1 {
        tag: NATIVE_VALUE_NUM,
        num: arg.num * 2.0,
        ..NativeValueV1::default()
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
/// `host` must be the opaque callback context passed by the Animatix host.
unsafe extern "C" fn pulse_evaluate(_host: *mut c_void) -> i32 {
    NATIVE_STATUS_OK
}

/// Execute callback for the `pulse` demo action.
///
/// # Safety
///
/// `host` must be the opaque callback context passed by the Animatix host.
unsafe extern "C" fn pulse_action(_host: *mut c_void) -> i32 {
    NATIVE_STATUS_OK
}
