//! Example native plugin using the unstable `animatix-plugin-api` ABI.
//!
//! Showcases the full extension surface with one `Pulse` primitive:
//!
//! - a keyframable `Num` property (`glow`)
//! - a manifest-driven `Enum` property (`mode: ring | dot | cross`) that
//!   renders as a dropdown in the GUI inspector
//! - `Str` (`caption`, `image_url`) and `Vec2` (`origin`) properties
//! - vector paths, text (`Text` + `Code` render kinds), a highlight layer,
//!   and a best-effort image command resolved from the asset cache
//! - a native expression function (`scale`)
//! - a native action (`throb`) that writes keyframes through the host API
//! - a typed service with an explicit destructor
//!
//! Build with `cargo build -p animatix-plugin-demo`, then pass the resulting
//! shared library or a manifest to the CLI:
//!
//! ```bash
//! animatix check demo.amx --plugin demo.amx-plugin.toml
//! animatix plugin describe target/debug/libanimatix_plugin_demo.so
//! ```

use std::ffi::{c_char, c_void};

use animatix_plugin_api::{
    NATIVE_CAP_MORPHABLE_PATHS, NATIVE_CAP_VECTOR_PATHS, NATIVE_CAP_VECTOR_REVEAL_TARGET,
    NATIVE_EASING_OUT, NATIVE_PATH_ELLIPSE, NATIVE_PATH_LINE, NATIVE_PRIMITIVE_CATEGORY_SHAPE,
    NATIVE_PRIMITIVE_CHILD_GENERIC, NATIVE_PROPERTY_ENUM, NATIVE_PROPERTY_F32,
    NATIVE_PROPERTY_STRING, NATIVE_PROPERTY_VEC2, NATIVE_RESIZE_MODE_SIZE, NATIVE_STATUS_OK,
    NATIVE_STATUS_TYPE_ERROR, NATIVE_TEXT_KIND_CODE, NATIVE_TEXT_KIND_TEXT, NATIVE_VALUE_ENUM,
    NATIVE_VALUE_NUM, NATIVE_VALUE_STRING, NATIVE_VALUE_VEC2, NativeAction, NativeActionContext,
    NativeActionParam, NativeFunctionContext, NativeFunctionDescriptor, NativeHighlightCommand,
    NativeImageCommand, NativeModifierValue, NativePathCommand, NativePluginApi, NativePrimitive,
    NativePrimitiveEvaluateCtx, NativePropertyDescriptor, NativeService, NativeTextCommand,
    NativeValue,
};

/// Return the unstable ABI snapshot implemented by this plugin.
#[unsafe(no_mangle)]
pub extern "C" fn animatix_plugin_abi_version() -> u32 {
    animatix_plugin_api::UNSTABLE_ABI_VERSION
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
        || api.version != animatix_plugin_api::UNSTABLE_ABI_VERSION
    {
        return NATIVE_STATUS_TYPE_ERROR;
    }

    // ── Extension properties with full tooling metadata ────────────────────
    let mut ids = [0u32; 5];
    let property_descriptors = [
        NativePropertyDescriptor {
            actor_type: c"Pulse".as_ptr(),
            name: c"glow".as_ptr(),
            display_name: c"Glow".as_ptr(),
            kind: NATIVE_PROPERTY_F32,
            type_info: c"Num".as_ptr(),
            injectable: true,
            group: c"Pulse".as_ptr(),
            help: c"Pulse radius glow amount (0..1)".as_ptr(),
        },
        NativePropertyDescriptor {
            actor_type: c"Pulse".as_ptr(),
            name: c"mode".as_ptr(),
            display_name: c"Mode".as_ptr(),
            kind: NATIVE_PROPERTY_ENUM,
            type_info: c"Enum(ring, dot, cross)".as_ptr(),
            injectable: true,
            group: c"Pulse".as_ptr(),
            help: c"Render variant: ring outline, filled dot, or crosshair".as_ptr(),
        },
        NativePropertyDescriptor {
            actor_type: c"Pulse".as_ptr(),
            name: c"caption".as_ptr(),
            display_name: c"Caption".as_ptr(),
            kind: NATIVE_PROPERTY_STRING,
            type_info: c"Str".as_ptr(),
            injectable: true,
            group: c"Pulse".as_ptr(),
            help: c"Label rendered below the pulse".as_ptr(),
        },
        NativePropertyDescriptor {
            actor_type: c"Pulse".as_ptr(),
            name: c"origin".as_ptr(),
            display_name: c"Origin".as_ptr(),
            kind: NATIVE_PROPERTY_VEC2,
            type_info: c"Vec2".as_ptr(),
            injectable: true,
            group: c"Pulse".as_ptr(),
            help: c"Scene position of the pulse center".as_ptr(),
        },
        NativePropertyDescriptor {
            actor_type: c"Pulse".as_ptr(),
            name: c"image_url".as_ptr(),
            display_name: c"Image URL".as_ptr(),
            kind: NATIVE_PROPERTY_STRING,
            type_info: c"Str".as_ptr(),
            injectable: false,
            group: c"Pulse".as_ptr(),
            help: c"Optional cached image URL stamped over the pulse".as_ptr(),
        },
    ];
    for (descriptor, id) in property_descriptors.iter().zip(ids.iter_mut()) {
        let status = unsafe { (api.register_property)(host, *descriptor, id) };
        if status != NATIVE_STATUS_OK || *id == 0 {
            return NATIVE_STATUS_TYPE_ERROR;
        }
    }

    // ── Native expression function ─────────────────────────────────────────
    let function_params = [
        NativeActionParam {
            name: c"value".as_ptr(),
            description: c"Number to scale".as_ptr(),
            type_info: c"Num".as_ptr(),
        },
        NativeActionParam {
            name: c"factor".as_ptr(),
            description: c"Scale factor".as_ptr(),
            type_info: c"Num".as_ptr(),
        },
    ];
    let function_status = unsafe {
        (api.register_function)(
            host,
            NativeFunctionDescriptor {
                name: c"scale".as_ptr(),
                params: function_params.as_ptr(),
                param_len: function_params.len(),
                return_type: c"Num".as_ptr(),
                help: c"Returns value * factor".as_ptr(),
                callback: scale,
            },
        )
    };
    if function_status != NATIVE_STATUS_OK {
        return function_status;
    }

    // ── Native primitive ───────────────────────────────────────────────────
    let property_names = [
        c"glow".as_ptr(),
        c"mode".as_ptr(),
        c"caption".as_ptr(),
        c"origin".as_ptr(),
        c"image_url".as_ptr(),
    ];
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

    // ── Native action with one named modifier ──────────────────────────────
    let action_modifiers = [NativeActionParam {
        name: c"strength".as_ptr(),
        description: c"Peak glow reached by the throb".as_ptr(),
        type_info: c"Num".as_ptr(),
    }];
    let action = NativeAction {
        name: c"throb".as_ptr(),
        category: c"Native".as_ptr(),
        description: c"Pulse the glow property of each target up over 500ms".as_ptr(),
        params: std::ptr::null(),
        param_len: 0,
        modifiers: action_modifiers.as_ptr(),
        modifier_len: action_modifiers.len(),
        execute: throb,
    };
    let action_status = unsafe { (api.register_action)(host, action) };
    if action_status != NATIVE_STATUS_OK {
        return action_status;
    }

    // ── Typed service with an explicit destructor ──────────────────────────
    let service = NativeService {
        name: c"demo.pulse".as_ptr(),
        type_info: c"usize".as_ptr(),
        help: c"Demo native service value".as_ptr(),
        value: throb as *const () as usize,
        drop: Some(drop_service),
    };
    unsafe { (api.provide_service)(host, service) }
}

/// `scale(value, factor) -> value * factor` native expression function.
///
/// # Safety
///
/// `ctx` and `args` must be the callback context and argument array passed by
/// the Animatix host; `out` must point to writable `NativeValue` storage.
unsafe extern "C" fn scale(
    _ctx: *mut NativeFunctionContext,
    args: *const NativeValue,
    arg_len: usize,
    out: *mut NativeValue,
) -> i32 {
    if arg_len != 2 {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let args = unsafe { std::slice::from_raw_parts(args, arg_len) };
    if args[0].tag != NATIVE_VALUE_NUM || args[1].tag != NATIVE_VALUE_NUM {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let result = NativeValue {
        tag: NATIVE_VALUE_NUM,
        num: args[0].num * args[1].num,
        ..NativeValue::default()
    };
    unsafe {
        *out = result;
    }
    NATIVE_STATUS_OK
}

/// Frame-time evaluate callback for the `Pulse` demo primitive.
///
/// Reads the keyframed `glow`, `mode`, `caption`, `origin`, and `image_url`
/// properties and emits paths, text, a highlight layer, and an optional image
/// through the host context.
///
/// # Safety
///
/// `ctx` must be the callback context passed by the Animatix host, and its
/// `append_*` function pointers must be called with the matching `host`.
unsafe extern "C" fn pulse_evaluate(ctx: *mut NativePrimitiveEvaluateCtx) -> i32 {
    let ctx = unsafe { &*ctx };

    // ── Read the actor's properties through the host API ───────────────────
    let mut glow = 0.25;
    let mut mode = String::from("ring");
    let mut caption = String::new();
    let mut origin = [0.0_f64, 0.0];
    let mut image_url = String::new();
    if let Some(get_property) = ctx.get_property {
        let mut native = NativeValue::default();
        if unsafe { (get_property)(ctx.host, c"glow".as_ptr(), &mut native) } == NATIVE_STATUS_OK
            && native.tag == NATIVE_VALUE_NUM
        {
            glow = native.num.clamp(0.0, 1.0);
        }
        if unsafe { (get_property)(ctx.host, c"mode".as_ptr(), &mut native) } == NATIVE_STATUS_OK
            && native.tag == NATIVE_VALUE_ENUM
        {
            mode = unsafe { read_c_string(native.string) }.unwrap_or_default();
        }
        if unsafe { (get_property)(ctx.host, c"caption".as_ptr(), &mut native) } == NATIVE_STATUS_OK
            && native.tag == NATIVE_VALUE_STRING
        {
            caption = unsafe { read_c_string(native.string) }.unwrap_or_default();
        }
        if unsafe { (get_property)(ctx.host, c"origin".as_ptr(), &mut native) } == NATIVE_STATUS_OK
            && native.tag == NATIVE_VALUE_VEC2
        {
            origin = [native.vec[0], native.vec[1]];
        }
        if unsafe { (get_property)(ctx.host, c"image_url".as_ptr(), &mut native) }
            == NATIVE_STATUS_OK
            && native.tag == NATIVE_VALUE_STRING
        {
            image_url = unsafe { read_c_string(native.string) }.unwrap_or_default();
        }
    }

    let radius = 30.0 + 30.0 * glow;
    let (cx, cy) = (origin[0], origin[1]);
    let cyan = [0.2, 0.8, 1.0, 1.0];
    let white = [1.0, 1.0, 1.0, 1.0];

    // ── Emit the body path for the selected mode ───────────────────────────
    let Some(append_path) = ctx.append_path else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let mode_status = match mode.as_str() {
        "ring" => unsafe {
            (append_path)(
                ctx.host,
                NativePathCommand {
                    kind: NATIVE_PATH_ELLIPSE,
                    x: cx,
                    y: cy,
                    width: radius,
                    height: radius,
                    fill: [0.0, 0.0, 0.0, 0.0],
                    stroke: cyan,
                    stroke_width: 3.0,
                    ..empty_path_command()
                },
            )
        },
        "dot" => {
            let body = unsafe {
                (append_path)(
                    ctx.host,
                    NativePathCommand {
                        kind: NATIVE_PATH_ELLIPSE,
                        x: cx,
                        y: cy,
                        width: radius,
                        height: radius,
                        fill: cyan,
                        stroke: white,
                        stroke_width: 1.5,
                        ..empty_path_command()
                    },
                )
            };
            if body != NATIVE_STATUS_OK {
                return body;
            }
            unsafe {
                (append_path)(
                    ctx.host,
                    NativePathCommand {
                        kind: NATIVE_PATH_ELLIPSE,
                        x: cx,
                        y: cy,
                        width: radius * 0.35,
                        height: radius * 0.35,
                        fill: white,
                        stroke: [0.0, 0.0, 0.0, 0.0],
                        stroke_width: 0.0,
                        ..empty_path_command()
                    },
                )
            }
        },
        // "cross": faint disc with crossing lines through the center.
        _ => {
            let body = unsafe {
                (append_path)(
                    ctx.host,
                    NativePathCommand {
                        kind: NATIVE_PATH_ELLIPSE,
                        x: cx,
                        y: cy,
                        width: radius,
                        height: radius,
                        fill: [0.2, 0.8, 1.0, 0.25],
                        stroke: [0.0, 0.0, 0.0, 0.0],
                        stroke_width: 0.0,
                        ..empty_path_command()
                    },
                )
            };
            if body != NATIVE_STATUS_OK {
                return body;
            }
            let horizontal = unsafe {
                (append_path)(
                    ctx.host,
                    NativePathCommand {
                        kind: NATIVE_PATH_LINE,
                        x1: cx - radius,
                        y1: cy,
                        x2: cx + radius,
                        y2: cy,
                        stroke: white,
                        stroke_width: 2.0,
                        ..empty_path_command()
                    },
                )
            };
            if horizontal != NATIVE_STATUS_OK {
                return horizontal;
            }
            unsafe {
                (append_path)(
                    ctx.host,
                    NativePathCommand {
                        kind: NATIVE_PATH_LINE,
                        x1: cx,
                        y1: cy - radius,
                        x2: cx,
                        y2: cy + radius,
                        stroke: white,
                        stroke_width: 2.0,
                        ..empty_path_command()
                    },
                )
            }
        },
    };
    if mode_status != NATIVE_STATUS_OK {
        return mode_status;
    }

    // ── Text: caption label plus a Code-kind status line ───────────────────
    if let Some(append_text) = ctx.append_text {
        if !caption.is_empty() {
            let caption_bytes = c_string(caption.as_str());
            let text_status = unsafe {
                (append_text)(
                    ctx.host,
                    NativeTextCommand {
                        content: caption_bytes.as_ptr().cast::<c_char>(),
                        content_len: caption_bytes.len() - 1,
                        font_family: c"".as_ptr(),
                        font_size: 16.0,
                        font_weight: 500.0,
                        font_style: c"normal".as_ptr(),
                        line_height: 1.2,
                        letter_spacing: 0.0,
                        word_spacing: 0.0,
                        color: white,
                        max_width: 0.0,
                        text_align: c"center".as_ptr(),
                        overflow: c"visible".as_ptr(),
                        kind: NATIVE_TEXT_KIND_TEXT,
                        x: origin[0],
                        y: origin[1] + radius + 14.0,
                    },
                )
            };
            if text_status != NATIVE_STATUS_OK {
                return text_status;
            }
        }
        let code = format!("glow: {glow:.2}  mode: {mode}");
        let code_bytes = c_string(code.as_str());
        let code_status = unsafe {
            (append_text)(
                ctx.host,
                NativeTextCommand {
                    content: code_bytes.as_ptr().cast::<c_char>(),
                    content_len: code_bytes.len() - 1,
                    font_family: c"".as_ptr(),
                    font_size: 12.0,
                    font_weight: 400.0,
                    font_style: c"normal".as_ptr(),
                    line_height: 1.2,
                    letter_spacing: 0.0,
                    word_spacing: 0.0,
                    color: [0.6, 0.6, 0.6, 1.0],
                    max_width: 0.0,
                    text_align: c"center".as_ptr(),
                    overflow: c"visible".as_ptr(),
                    kind: NATIVE_TEXT_KIND_CODE,
                    x: origin[0],
                    y: origin[1] + radius + 36.0,
                },
            )
        };
        if code_status != NATIVE_STATUS_OK {
            return code_status;
        }
    }

    // ── Highlight layer behind the pulse ───────────────────────────────────
    if let Some(append_highlight) = ctx.append_highlight {
        let highlight_command = NativeHighlightCommand {
            rect: [cx - 46.0, cy - 46.0, cx + 46.0, cy + 46.0],
            color: cyan,
            alpha: 0.12,
            corner_radius: 10.0,
            blend: 2,
        };
        let highlight_status = unsafe { (append_highlight)(ctx.host, highlight_command) };
        if highlight_status != NATIVE_STATUS_OK {
            return highlight_status;
        }
    }

    // ── Image: best-effort stamp resolved from the asset cache ─────────────
    if let Some(append_image) = ctx.append_image
        && !image_url.is_empty()
    {
        let url_bytes = c_string(image_url.as_str());
        let image_command = NativeImageCommand {
            url: url_bytes.as_ptr().cast::<c_char>(),
            natural_size: [radius * 2.0, radius * 2.0],
            x: origin[0],
            y: origin[1],
        };
        // A URL that is not cached returns an error; the pulse still renders
        // its paths so the failure is non-fatal for the frame.
        let _ = unsafe { (append_image)(ctx.host, image_command) };
    }

    NATIVE_STATUS_OK
}

/// Execute callback for the `throb` demo action.
///
/// Writes a `glow` keyframe on every target over a fixed 500ms window using
/// the named `strength` modifier as the peak value.
///
/// # Safety
///
/// `ctx` must be the callback context passed by the Animatix host.
unsafe extern "C" fn throb(ctx: *mut NativeActionContext) -> i32 {
    let ctx = unsafe { &*ctx };
    let mut strength: f64 = 1.0;
    let modifier_count = unsafe { (ctx.get_modifier_count)(ctx.host) };
    for index in 0..modifier_count {
        let mut modifier = NativeModifierValue {
            name: std::ptr::null(),
            value: NativeValue::default(),
        };
        if unsafe { (ctx.get_modifier)(ctx.host, index, &mut modifier) } != NATIVE_STATUS_OK {
            continue;
        }
        if modifier.name.is_null() {
            continue;
        }
        let name = unsafe { read_c_string(modifier.name) }.unwrap_or_default();
        if name == "strength" && modifier.value.tag == NATIVE_VALUE_NUM {
            strength = modifier.value.num;
        }
    }

    let duration_ms = 500u64;
    let target_count = unsafe { (ctx.get_target_count)(ctx.host) };
    for index in 0..target_count {
        let mut target: *const c_char = std::ptr::null();
        if unsafe { (ctx.get_target)(ctx.host, index, &mut target) } != NATIVE_STATUS_OK {
            continue;
        }
        let value = NativeValue {
            tag: NATIVE_VALUE_NUM,
            num: strength.clamp(0.0, 1.0),
            ..NativeValue::default()
        };
        let status = unsafe {
            (ctx.write_keyframe)(
                ctx.host,
                target,
                c"glow".as_ptr(),
                value,
                ctx.time_ms as u64,
                ctx.time_ms as u64 + duration_ms,
                NATIVE_EASING_OUT,
            )
        };
        if status != NATIVE_STATUS_OK {
            return status;
        }
    }
    NATIVE_STATUS_OK
}

/// Destructor for the demo service value.
///
/// # Safety
///
/// `value` must be a service value previously provided by this plugin.
unsafe extern "C" fn drop_service(_value: usize) {}

/// Defaults for every path-command field the demo does not customize.
fn empty_path_command() -> NativePathCommand {
    NativePathCommand {
        kind: 0,
        x: 0.0,
        y: 0.0,
        width: 0.0,
        height: 0.0,
        x1: 0.0,
        y1: 0.0,
        x2: 0.0,
        y2: 0.0,
        points: std::ptr::null(),
        point_len: 0,
        radius: 0.0,
        start_angle: 0.0,
        sweep_angle: 0.0,
        fill: [0.0, 0.0, 0.0, 0.0],
        stroke: [0.0, 0.0, 0.0, 0.0],
        stroke_width: 0.0,
        line_cap: 0,
        line_join: 0,
    }
}

/// Copy a Rust string into a NUL-terminated heap buffer that stays alive for
/// the enclosing scope. The host copies the contents before the append call
/// returns, so the boxed buffer only needs to outlive that call.
fn c_string(text: &str) -> Box<[u8]> {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    bytes.into_boxed_slice()
}

/// Read a NUL-terminated C string into a Rust `String`.
///
/// # Safety
///
/// `ptr` must be a valid NUL-terminated string for the duration of the call.
unsafe fn read_c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { std::ffi::CStr::from_ptr(ptr) }.to_str().ok().map(str::to_owned)
}
