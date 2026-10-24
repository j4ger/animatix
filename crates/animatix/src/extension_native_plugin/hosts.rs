//! Host contexts and FFI callbacks exposed to native plugins during build, evaluate, assignment, and action execution.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};

use animatix_plugin_api::{
    NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_STATUS_UNSUPPORTED, NATIVE_TEXT_KIND_CODE,
    NATIVE_TEXT_KIND_TYST, NativeChild, NativeHighlightCommand, NativeImageCommand,
    NativeModifierValue, NativePathCommand, NativePropertyValue, NativeTextCommand, NativeValue,
};

use super::convert::{
    NativeValueArena, native_blend_from_code, native_color, native_easing, native_path_to_vello,
    native_to_property_value, native_to_value, property_value_to_native, read_c_string,
    read_c_string_len, value_to_native,
};
use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::primitives::EvaluateCtx;
use crate::timeline::property_registry::lookup_property;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{Environment, Value};

/// Host collection for a native `default_props` callback.
#[derive(Default)]
pub(crate) struct NativeDefaultPropsHost {
    pub(crate) props: Vec<(String, crate::timeline::Value)>,
}

pub(crate) unsafe extern "C" fn native_default_props_append(
    host: *mut c_void,
    name: *const c_char,
    value: NativeValue,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeDefaultPropsHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_UNSUPPORTED;
    };
    match native_to_value(value) {
        Ok(value) => {
            host.props.push((name, value));
            NATIVE_STATUS_OK
        },
        Err(_) => NATIVE_STATUS_TYPE_ERROR,
    }
}

/// Host collection for a native equation-fragment callback.
#[derive(Default)]
pub(crate) struct NativeEquationFragmentHost {
    pub(crate) content: String,
}

pub(crate) unsafe extern "C" fn native_write_content(
    host: *mut c_void,
    content: *const c_char,
    content_len: usize,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeEquationFragmentHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(text) = (unsafe { read_c_string_len(content, content_len) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    host.content = text;
    NATIVE_STATUS_OK
}

pub(crate) struct NativePrimitiveEvaluateHost<'a> {
    pub(crate) ctx: &'a EvaluateCtx<'a>,
    pub(crate) property_ids: &'a HashMap<String, animatix_syntax::schema::PropertyId>,
    pub(crate) service_values: &'a HashMap<String, usize>,
    pub(crate) text_compiler: Option<&'a mut crate::renderer::text::TextCompiler>,
    pub(crate) font_context: Option<&'a crate::renderer::text::FontContext>,
    pub(crate) commands: Vec<crate::primitives::RenderCommand>,
    pub(crate) arena: NativeValueArena,
}

pub(crate) unsafe extern "C" fn native_get_property(
    host: *mut c_void,
    name: *const c_char,
    out: *mut NativeValue,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveEvaluateHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let value = if let Some(id) = host.property_ids.get(&name) {
        let Some(plan) = host.ctx.property_plan else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        crate::timeline::property_engine::read_property_plan_slot(plan, *id, host.ctx.time_ms)
    } else if let Some(schema) = lookup_property(&name) {
        host.ctx.read_property_value(schema.field, host.ctx.time_ms)
    } else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(value) = value else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let native = property_value_to_native(&value, &mut host.arena);
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    unsafe { *out = native };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_get_service(
    host: *mut c_void,
    name: *const c_char,
    out: *mut usize,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveEvaluateHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(value) = host.service_values.get(&name) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    unsafe { *out = *value };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_append_path(
    host: *mut c_void,
    command: NativePathCommand,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveEvaluateHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(path) = native_path_to_vello(&command) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    host.commands
        .push(crate::primitives::RenderCommand::Paths { paths: vec![path] });
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_append_text(
    host: *mut c_void,
    command: NativeTextCommand,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveEvaluateHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(compiler) = host.text_compiler.as_mut() else {
        return NATIVE_STATUS_UNSUPPORTED;
    };
    let Some(font_ctx) = host.font_context else {
        return NATIVE_STATUS_UNSUPPORTED;
    };
    let Some(content) = (unsafe { read_c_string_len(command.content, command.content_len) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let font_family =
        (unsafe { read_c_string(command.font_family) }).unwrap_or_else(|| "sans-serif".to_string());
    let font_style =
        (unsafe { read_c_string(command.font_style) }).unwrap_or_else(|| "normal".to_string());
    let text_align =
        (unsafe { read_c_string(command.text_align) }).unwrap_or_else(|| "left".to_string());
    let overflow =
        (unsafe { read_c_string(command.overflow) }).unwrap_or_else(|| "visible".to_string());
    let color = [
        command.color[0].clamp(0.0, 1.0) as f32,
        command.color[1].clamp(0.0, 1.0) as f32,
        command.color[2].clamp(0.0, 1.0) as f32,
        command.color[3].clamp(0.0, 1.0) as f32,
    ];
    let text_kind = match command.kind {
        NATIVE_TEXT_KIND_CODE => crate::renderer::text::TextKind::Code,
        NATIVE_TEXT_KIND_TYST => crate::renderer::text::TextKind::Typst,
        _ => crate::renderer::text::TextKind::Text,
    };
    let paths = match compiler.compile(
        &content,
        &font_family,
        command.font_size as f32,
        command.font_weight as f32,
        &font_style,
        command.line_height as f32,
        command.letter_spacing as f32,
        command.word_spacing as f32,
        color,
        text_kind,
        "",
        font_ctx,
        command.max_width as f32,
        &text_align,
        &overflow,
    ) {
        Ok(paths) => paths,
        Err(_) => return NATIVE_STATUS_TYPE_ERROR,
    };
    let paths = crate::primitives::translate_text_paths(paths, command.x, command.y);
    host.commands.push(crate::primitives::RenderCommand::Text {
        paths,
        fill_gradient: None,
        clip_path: None,
        tint_color: None,
        hardware_layout: None,
    });
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_append_image(
    host: *mut c_void,
    command: NativeImageCommand,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveEvaluateHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let requested_url = unsafe { read_c_string(command.url) };
    let image = match requested_url.as_deref() {
        Some(url) => host.ctx.asset_cache.get_image(url),
        None => host.ctx.image_track().and_then(|t| t.evaluate(host.ctx.time_ms)),
    };
    let Some(image) = image else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let natural_size = if command.natural_size[0] > 0.0 && command.natural_size[1] > 0.0 {
        [
            command.natural_size[0] as f32,
            command.natural_size[1] as f32,
        ]
    } else {
        let half =
            host.ctx.geometry.map_or(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE, |geometry| {
                geometry.size.get(host.ctx.time_ms, crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
            });
        [half[0] * 2.0, half[1] * 2.0]
    };
    host.commands.push(crate::primitives::RenderCommand::Image {
        image,
        natural_size,
        offset: [command.x, command.y],
    });
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_append_highlight(
    host: *mut c_void,
    command: NativeHighlightCommand,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveEvaluateHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let rect = kurbo::Rect::new(command.rect[0], command.rect[1], command.rect[2], command.rect[3]);
    let blend = native_blend_from_code(command.blend);
    host.commands.push(crate::primitives::RenderCommand::HighlightLayer {
        rect,
        color: native_color(command.color),
        blend,
        alpha: command.alpha.clamp(0.0, 1.0) as f32,
        corner_radius: command.corner_radius,
    });
    NATIVE_STATUS_OK
}

pub(crate) struct NativePrimitiveBuildHost<'a> {
    pub(crate) props: Vec<NativePropertyValue>,
    pub(crate) modifiers: Vec<NativeModifierValue>,
    pub(crate) children: Vec<NativeChild>,
    pub(crate) _child_props: Vec<Vec<NativePropertyValue>>,
    pub(crate) diagnostics: &'a mut Vec<crate::diagnostics::Diagnostic>,
    pub(crate) arena: NativeValueArena,
}

pub(crate) fn native_build_value(
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    subject: &str,
    arena: &mut NativeValueArena,
) -> NativeValue {
    let value = crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
        expr,
        env,
        diagnostics,
        subject,
    )
    .unwrap_or(Value::Num(0.0));
    value_to_native(&value, arena).unwrap_or_default()
}

pub(crate) fn native_build_property(
    name: &str,
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    arena: &mut NativeValueArena,
) -> NativePropertyValue {
    let (name_ptr, _) = arena.string(name);
    NativePropertyValue {
        name: name_ptr,
        value: native_build_value(expr, env, diagnostics, name, arena),
    }
}

pub(crate) fn native_build_modifier(
    modifier: &Modifier,
    env: &Environment,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    arena: &mut NativeValueArena,
) -> NativeModifierValue {
    let name_ptr = modifier
        .name
        .as_deref()
        .map(|name| arena.string(name).0)
        .unwrap_or(std::ptr::null());
    NativeModifierValue {
        name: name_ptr,
        value: native_build_value(&modifier.value, env, diagnostics, "modifier", arena),
    }
}

pub(crate) fn native_build_child(
    item: &InlineItem,
    env: &Environment,
    diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    arena: &mut NativeValueArena,
    child_props: &mut Vec<Vec<NativePropertyValue>>,
) -> NativeChild {
    let empty_props: Vec<Property> = Vec::new();
    let (label, type_name, props) = match item {
        InlineItem::Anonymous { ty, props, .. } => ("", ty.as_str(), props),
        InlineItem::Labeled {
            label, ty, props, ..
        } => (label.as_str(), ty.as_str(), props),
        InlineItem::ForLoop { .. } => ("for", "ForLoop", &empty_props),
        InlineItem::SlotMarker => ("@slot", "Slot", &empty_props),
        InlineItem::SlotFill { slot, .. } => (slot.as_str(), "SlotFill", &empty_props),
    };
    let (label_ptr, _) = arena.string(label);
    let (type_ptr, _) = arena.string(type_name);
    let native_props = props
        .iter()
        .map(|prop| native_build_property(&prop.name, &prop.value, env, diagnostics, arena))
        .collect::<Vec<_>>();
    let properties = if native_props.is_empty() {
        std::ptr::null()
    } else {
        native_props.as_ptr()
    };
    let property_len = native_props.len();
    child_props.push(native_props);
    NativeChild {
        label: label_ptr,
        type_name: type_ptr,
        properties,
        property_len,
    }
}

impl<'a> NativePrimitiveBuildHost<'a> {
    pub(crate) fn new(
        timeline: &mut crate::timeline::Timeline,
        _time_ms: f64,
        _parent_label: Option<&'a str>,
        _label: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        diagnostics: &'a mut Vec<crate::diagnostics::Diagnostic>,
    ) -> Self {
        let mut arena = NativeValueArena::default();
        let native_props = props
            .iter()
            .map(|prop| {
                native_build_property(
                    &prop.name,
                    &prop.value,
                    &timeline.env,
                    diagnostics,
                    &mut arena,
                )
            })
            .collect();
        let native_modifiers = modifiers
            .iter()
            .map(|modifier| native_build_modifier(modifier, &timeline.env, diagnostics, &mut arena))
            .collect();
        let mut child_props = Vec::new();
        let native_children = children
            .iter()
            .map(|item| {
                native_build_child(item, &timeline.env, diagnostics, &mut arena, &mut child_props)
            })
            .collect();
        Self {
            props: native_props,
            modifiers: native_modifiers,
            children: native_children,
            _child_props: child_props,
            diagnostics,
            arena,
        }
    }
}

pub(crate) unsafe extern "C" fn native_build_get_property_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return 0;
    };
    host.props.len()
}

pub(crate) unsafe extern "C" fn native_build_get_property(
    host: *mut c_void,
    index: usize,
    out: *mut NativePropertyValue,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(value) = host.props.get(index) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    unsafe { *out = *value };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_build_get_modifier_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return 0;
    };
    host.modifiers.len()
}

pub(crate) unsafe extern "C" fn native_build_get_modifier(
    host: *mut c_void,
    index: usize,
    out: *mut NativeModifierValue,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(value) = host.modifiers.get(index) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    unsafe { *out = *value };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_build_get_child_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return 0;
    };
    host.children.len()
}

pub(crate) unsafe extern "C" fn native_build_get_child(
    host: *mut c_void,
    index: usize,
    out: *mut NativeChild,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(value) = host.children.get(index) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    unsafe { *out = *value };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_build_report_diagnostic(
    host: *mut c_void,
    _code: *const c_char,
    level: u32,
    message: *const c_char,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(message) = (unsafe { read_c_string(message) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
    let diagnostic = if level == 0 {
        Diagnostic::error(DiagnosticCode::InvalidModifierValue, DiagnosticPhase::Build, message)
    } else {
        Diagnostic::warning(DiagnosticCode::InvalidModifierValue, DiagnosticPhase::Build, message)
    };
    host.diagnostics.push(diagnostic);
    NATIVE_STATUS_OK
}

pub(crate) struct NativeAssignmentHost<'a> {
    pub(crate) plan: &'a mut crate::timeline::plan::PropertyPlan,
    pub(crate) property_ids: &'a HashMap<String, animatix_syntax::schema::PropertyId>,
    pub(crate) property_kinds: &'a HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    pub(crate) value: NativeValue,
}

pub(crate) unsafe extern "C" fn native_assignment_get_value(
    host: *mut c_void,
    out: *mut NativeValue,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeAssignmentHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    unsafe { *out = host.value };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_assignment_write_keyframe(
    host: *mut c_void,
    name: *const c_char,
    value: NativeValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: u32,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeAssignmentHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(id) = host.property_ids.get(&name) else {
        if lookup_property(&name).is_some() {
            return NATIVE_STATUS_UNSUPPORTED;
        }
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(kind) = host.property_kinds.get(&name) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(property_value) = native_to_property_value(value, *kind) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(easing) = native_easing(easing) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    crate::timeline::property_engine::write_property_plan_slot(
        host.plan,
        *id,
        crate::timeline::PropertyKind::from(*kind),
        property_value,
        t_start_ms,
        t_end_ms,
        easing,
    );
    NATIVE_STATUS_OK
}

pub(crate) struct NativeFinalizeHost;

pub(crate) struct NativeActionHost<'a> {
    pub(crate) timeline: &'a mut crate::timeline::Timeline,
    pub(crate) targets: Vec<*const c_char>,
    pub(crate) args: Vec<NativeValue>,
    pub(crate) modifiers: Vec<NativeModifierValue>,
    pub(crate) property_ids: &'a HashMap<String, animatix_syntax::schema::PropertyId>,
    pub(crate) property_kinds: &'a HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    pub(crate) service_values: &'a HashMap<String, usize>,
}

pub(crate) unsafe extern "C" fn native_action_get_target_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return 0;
    };
    host.targets.len()
}

pub(crate) unsafe extern "C" fn native_action_get_target(
    host: *mut c_void,
    index: usize,
    out: *mut *const c_char,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(target) = host.targets.get(index) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    unsafe { *out = *target };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_action_get_arg_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return 0;
    };
    host.args.len()
}

pub(crate) unsafe extern "C" fn native_action_get_arg(
    host: *mut c_void,
    index: usize,
    out: *mut NativeValue,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(arg) = host.args.get(index) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    unsafe { *out = *arg };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_action_get_modifier_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return 0;
    };
    host.modifiers.len()
}

pub(crate) unsafe extern "C" fn native_action_get_modifier(
    host: *mut c_void,
    index: usize,
    out: *mut NativeModifierValue,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(modifier) = host.modifiers.get(index) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    unsafe { *out = *modifier };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_action_write_keyframe(
    host: *mut c_void,
    target: *const c_char,
    name: *const c_char,
    value: NativeValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: u32,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(target) = (unsafe { read_c_string(target) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(track) = host.timeline.world.tracks.get_mut(&target) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(id) = host.property_ids.get(&name) else {
        if lookup_property(&name).is_some() {
            return NATIVE_STATUS_UNSUPPORTED;
        }
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(kind) = host.property_kinds.get(&name) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(property_value) = native_to_property_value(value, *kind) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(easing) = native_easing(easing) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    crate::timeline::property_engine::write_property_plan_slot(
        host.timeline.world.property_plan.get_mut(track.id),
        *id,
        crate::timeline::PropertyKind::from(*kind),
        property_value,
        t_start_ms,
        t_end_ms,
        easing,
    );
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_action_get_service(
    host: *mut c_void,
    name: *const c_char,
    out: *mut usize,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(value) = host.service_values.get(&name) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    unsafe { *out = *value };
    NATIVE_STATUS_OK
}
