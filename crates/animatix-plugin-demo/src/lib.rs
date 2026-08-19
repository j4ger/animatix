//! Example native plugin using the stable `animatix-plugin-api` ABI.
//!
//! Build with `cargo build -p animatix-plugin-demo`, then pass the resulting
//! shared library to `animatix check --plugin <library>`.

use std::ffi::{c_char, c_void};

use animatix_plugin_api::{
    NATIVE_CAP_MORPHABLE_PATHS, NATIVE_CAP_VECTOR_PATHS, NATIVE_CAP_VECTOR_REVEAL_TARGET,
    NATIVE_PATH_ELLIPSE, NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
    NATIVE_PROPERTY_F32, NATIVE_RESIZE_MODE_SIZE, NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR,
    NATIVE_TEXT_KIND_TEXT, NATIVE_VALUE_NUM, NativeAction, NativeActionContext,
    NativeFunctionContext, NativeFunctionDescriptor, NativeHighlightCommand, NativePathCommand,
    NativePluginApi, NativePrimitive, NativePrimitiveEvaluateCtx, NativePropertyDescriptor,
    NativeService, NativeTextCommand, NativeValue,
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

    let mut glow_id = 0;
    let property_status = unsafe {
        (api.register_property)(
            host,
            NativePropertyDescriptor {
                actor_type: c"Pulse".as_ptr(),
                name: c"glow".as_ptr(),
                display_name: c"Glow".as_ptr(),
                kind: NATIVE_PROPERTY_F32,
                type_info: c"Num".as_ptr(),
                injectable: true,
                group: c"Pulse".as_ptr(),
                help: c"Pulse radius glow amount".as_ptr(),
            },
            &mut glow_id,
        )
    };
    if property_status != NATIVE_STATUS_OK || glow_id == 0 {
        return NATIVE_STATUS_TYPE_ERROR;
    }

    let function_status = unsafe {
        (api.register_function)(
            host,
            NativeFunctionDescriptor {
                name: c"double".as_ptr(),
                params: std::ptr::null(),
                param_len: 0,
                return_type: c"Num".as_ptr(),
                help: c"Doubles a number".as_ptr(),
                callback: double,
            },
        )
    };
    if function_status != NATIVE_STATUS_OK {
        return function_status;
    }

    let property_names = [c"glow".as_ptr()];
    let primitive = NativePrimitive {
        type_name: c"Pulse".as_ptr(),
        display_name: c"Pulse".as_ptr(),
        icon_id: c"extension:pulse".as_ptr(),
        category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
        capabilities: NATIVE_CAP_VECTOR_PATHS
            | NATIVE_CAP_MORPHABLE_PATHS
            | NATIVE_CAP_VECTOR_REVEAL_TARGET,
        properties: property_names.as_ptr(),
        property_len: property_names.len(),
        advanced: false,
        child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
        resize_mode: NATIVE_RESIZE_MODE_SIZE,
        build: None,
        evaluate: Some(pulse_evaluate),
        handle_assignment: None,
        finalize_container_build: None,
    };
    let primitive_status = unsafe { (api.register_primitive)(host, primitive) };
    if primitive_status != NATIVE_STATUS_OK {
        return primitive_status;
    }

    let action = NativeAction {
        name: c"pulse".as_ptr(),
        category: c"Native".as_ptr(),
        description: c"Demo native pulse action".as_ptr(),
        params: std::ptr::null(),
        param_len: 0,
        modifiers: std::ptr::null(),
        modifier_len: 0,
        execute: pulse_action,
    };
    let action_status = unsafe { (api.register_action)(host, action) };
    if action_status != NATIVE_STATUS_OK {
        return action_status;
    }

    let service = NativeService {
        name: c"demo.pulse".as_ptr(),
        type_info: c"usize".as_ptr(),
        help: c"Demo native service".as_ptr(),
        value: pulse_action as *const () as usize,
        drop: Some(drop_service),
    };
    unsafe { (api.provide_service)(host, service) }
}

unsafe extern "C" fn double(
    _ctx: *mut NativeFunctionContext,
    args: *const NativeValue,
    arg_len: usize,
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
    let mut glow = 0.25;
    if let Some(get_property) = ctx.get_property {
        let mut native = NativeValue::default();
        if unsafe { (get_property)(ctx.host, c"glow".as_ptr(), &mut native) } == NATIVE_STATUS_OK
            && native.tag == NATIVE_VALUE_NUM
        {
            glow = native.num.clamp(0.0, 1.0);
        }
    }
    let radius = 30.0 + 30.0 * glow;
    let command = NativePathCommand {
        kind: NATIVE_PATH_ELLIPSE,
        x: 0.0,
        y: 0.0,
        width: radius,
        height: radius,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        points: std::ptr::null(),
        point_len: 0,
        radius: 0.0,
        start_angle: 0.0,
        sweep_angle: 0.0,
        fill: [0.2, 0.8, 1.0, 1.0],
        stroke: [1.0, 1.0, 1.0, 1.0],
        stroke_width: 2.0,
        line_cap: 0,
        line_join: 0,
    };
    let path_status = unsafe { (append_path)(ctx.host, command) };
    if path_status != NATIVE_STATUS_OK {
        return path_status;
    }
    if let Some(append_text) = ctx.append_text {
        let text_command = NativeTextCommand {
            content: c"Pulse".as_ptr(),
            content_len: 5,
            font_family: c"".as_ptr(),
            font_size: 28.0,
            font_weight: 600.0,
            font_style: c"normal".as_ptr(),
            line_height: 1.2,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            color: [1.0, 1.0, 1.0, 1.0],
            max_width: 0.0,
            text_align: c"center".as_ptr(),
            overflow: c"visible".as_ptr(),
            kind: NATIVE_TEXT_KIND_TEXT,
        };
        let text_status = unsafe { (append_text)(ctx.host, text_command) };
        if text_status != NATIVE_STATUS_OK {
            return text_status;
        }
    }
    if let Some(append_highlight) = ctx.append_highlight {
        let highlight_command = NativeHighlightCommand {
            rect: [-46.0, -46.0, 46.0, 46.0],
            color: [0.2, 0.8, 1.0, 1.0],
            alpha: 0.12,
            corner_radius: 10.0,
            blend: 2,
        };
        let highlight_status = unsafe { (append_highlight)(ctx.host, highlight_command) };
        if highlight_status != NATIVE_STATUS_OK {
            return highlight_status;
        }
    }
    NATIVE_STATUS_OK
}

/// Execute callback for the `pulse` demo action.
///
/// # Safety
///
/// `ctx` must be the callback context passed by the Animatix host.
unsafe extern "C" fn pulse_action(_ctx: *mut NativeActionContext) -> i32 {
    NATIVE_STATUS_OK
}

/// Destructor for the demo service value.
///
/// # Safety
///
/// `value` must be a service value previously provided by this plugin.
unsafe extern "C" fn drop_service(_value: usize) {}
