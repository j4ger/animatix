//! Example native plugin using the stable `animatix-plugin-api` ABI.
//!
//! Build with `cargo build -p animatix-plugin-demo`, then pass the resulting
//! shared library to `animatix check --plugin <library>`.

use std::ffi::{c_char, c_void};

use animatix_plugin_api::{
    NATIVE_PROPERTY_F32, NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_VALUE_NUM,
    NativePluginApiV1, NativeValueV1,
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

/// Install plugin capabilities through the host API table.
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
