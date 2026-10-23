//! Registration entry points the native plugin ABI calls during `install`.

use std::any::Any;
use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::Arc;

use animatix_plugin_api::{
    NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_STATUS_UNSUPPORTED, NativeAction,
    NativeActionParam, NativeEffectDescriptor, NativeFunctionContext, NativeFunctionDescriptor,
    NativePrimitive, NativePropertyDescriptor, NativeValue,
};

use super::{
    NativeActionAdapter, NativePrimitiveAdapter, NativeValueArena, native_property_kind,
    native_to_value, parse_native_type, read_c_string, value_to_native,
};
use crate::extension_context::ExtensionContext;
use crate::timeline::actions::registry::{ActionParam, ActionSignature};
use crate::timeline::{Environment, EvalError};

pub(crate) struct NativeHost<'a> {
    pub(super) ctx: &'a mut ExtensionContext,
    pub(super) library: Option<Arc<dyn Any + Send + Sync>>,
    pub(super) properties: Vec<(String, String)>,
    pub(super) property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    pub(super) property_kinds: HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    pub(super) service_values: HashMap<String, usize>,
    pub(super) functions: Vec<String>,
    pub(super) primitives: Vec<String>,
    pub(super) actions: Vec<String>,
    pub(super) services: Vec<String>,
    /// Registered effect names (the registry is keyed by authored type name).
    pub(super) effects: Vec<String>,
}

impl NativeHost<'_> {
    pub(super) fn rollback(&mut self) {
        let ctx = &mut *self.ctx;
        for (actor_type, name) in self.properties.drain(..) {
            ctx.remove_property(&actor_type, &name);
        }
        for name in self.functions.drain(..) {
            ctx.remove_function(&name);
        }
        for name in self.primitives.drain(..) {
            ctx.remove_primitive(&name);
        }
        for name in self.actions.drain(..) {
            ctx.remove_action(&name);
        }
        for name in self.services.drain(..) {
            ctx.remove_service(&name);
        }
        for name in self.effects.drain(..) {
            crate::timeline::effects::unregister_extension_effect(&name);
        }
    }
}

pub(crate) struct NativeFunctionHost<'a> {
    pub(super) env: &'a Environment,
    pub(super) service_values: &'a HashMap<String, usize>,
    pub(super) arena: NativeValueArena,
}

pub(crate) unsafe extern "C" fn native_function_get_env(
    host: *mut c_void,
    name: *const c_char,
    out: *mut NativeValue,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeFunctionHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(value) = host.env.get(&name) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Ok(native) = value_to_native(&value, &mut host.arena) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    unsafe { *out = native };
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_function_get_service(
    host: *mut c_void,
    name: *const c_char,
    out: *mut usize,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeFunctionHost).as_ref() }) else {
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

pub(crate) unsafe extern "C" fn native_register_property(
    host: *mut c_void,
    descriptor: NativePropertyDescriptor,
    out_id: *mut u32,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(actor_type) = (unsafe { read_c_string(descriptor.actor_type) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(descriptor.name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(kind) = native_property_kind(descriptor.kind) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if host.properties.iter().any(|(existing_actor, existing_name)| {
        existing_name == &name && existing_actor != &actor_type
    }) {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let display_name = unsafe { read_c_string(descriptor.display_name) };
    let group = unsafe { read_c_string(descriptor.group) };
    let help = unsafe { read_c_string(descriptor.help) };
    let ty = unsafe { read_c_string(descriptor.type_info) }.and_then(|ty| parse_native_type(&ty));
    let id = match host.ctx.register_property_full_typed(
        actor_type.clone(),
        name.clone(),
        kind,
        ty,
        descriptor.injectable,
        display_name,
        group,
        help,
    ) {
        Ok(id) => id,
        Err(_) => return NATIVE_STATUS_TYPE_ERROR,
    };
    if out_id.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    unsafe { *out_id = id.0 };
    host.properties.push((actor_type, name.clone()));
    host.property_ids.insert(name.clone(), id);
    host.property_kinds.insert(name, kind);
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_register_function(
    host: *mut c_void,
    descriptor: NativeFunctionDescriptor,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(descriptor.name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let callback = descriptor.callback;
    let params = unsafe { read_native_action_params(descriptor.params, descriptor.param_len) };
    let return_type =
        unsafe { read_c_string(descriptor.return_type) }.and_then(|ty| parse_native_type(&ty));
    let help = unsafe { read_c_string(descriptor.help) };
    let function_descriptor = animatix_syntax::schema::FunctionDescriptor {
        name: name.clone(),
        params,
        return_type,
        help,
    };
    let library = host.library.clone();
    let service_values = host.service_values.clone();
    let function_name = name.clone();
    if host
        .ctx
        .register_function_with_descriptor(
            name.clone(),
            move |args, env| unsafe {
                let _keep_alive = &library;
                let mut arena = NativeValueArena::default();
                let native_args = args
                    .iter()
                    .map(|value| value_to_native(value, &mut arena))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut out = NativeValue::default();
                let mut function_host = NativeFunctionHost {
                    env,
                    service_values: &service_values,
                    arena,
                };
                let mut native_ctx = NativeFunctionContext {
                    size: std::mem::size_of::<NativeFunctionContext>(),
                    host: (&mut function_host as *mut NativeFunctionHost).cast(),
                    get_env: native_function_get_env,
                    get_service: native_function_get_service,
                };
                let status = callback(
                    &mut native_ctx,
                    if native_args.is_empty() {
                        std::ptr::null()
                    } else {
                        native_args.as_ptr()
                    },
                    native_args.len(),
                    &mut out,
                );
                match status {
                    NATIVE_STATUS_OK => native_to_value(out),
                    NATIVE_STATUS_TYPE_ERROR => Err(EvalError::TypeMismatch(format!(
                        "native plugin function '{function_name}' rejected its arguments"
                    ))),
                    NATIVE_STATUS_UNSUPPORTED => Err(EvalError::UnsupportedConstruct(format!(
                        "native plugin function '{function_name}' does not support this value"
                    ))),
                    _ => Err(EvalError::TypeMismatch(format!(
                        "native plugin function '{function_name}' failed with status {status}"
                    ))),
                }
            },
            function_descriptor,
        )
        .is_err()
    {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    host.functions.push(name);
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_register_primitive(
    host: *mut c_void,
    primitive: NativePrimitive,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(library) = host.library.clone() else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(adapter) = NativePrimitiveAdapter::new(
        primitive,
        library,
        host.property_ids.clone(),
        host.property_kinds.clone(),
        host.service_values.clone(),
    ) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let name = adapter.type_name.to_string();
    let info = adapter.info.clone();
    if host.ctx.register_primitive(std::sync::Arc::new(adapter), info).is_err() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    host.primitives.push(name);
    NATIVE_STATUS_OK
}

unsafe fn read_native_action_params(
    params: *const NativeActionParam,
    len: usize,
) -> Vec<ActionParam> {
    if params.is_null() || len == 0 {
        return Vec::new();
    }
    let params = unsafe { std::slice::from_raw_parts(params, len) };
    params
        .iter()
        .map(|param| ActionParam {
            name: (unsafe { read_c_string(param.name) }).unwrap_or_default(),
            description: (unsafe { read_c_string(param.description) }).unwrap_or_default(),
            type_info: (unsafe { read_c_string(param.type_info) }).unwrap_or_default(),
        })
        .collect()
}

pub(crate) unsafe extern "C" fn native_register_action(
    host: *mut c_void,
    action: NativeAction,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(action.name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(category) = (unsafe { read_c_string(action.category) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(description) = (unsafe { read_c_string(action.description) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let params = unsafe { read_native_action_params(action.params, action.param_len) };
    let modifiers = unsafe { read_native_action_params(action.modifiers, action.modifier_len) };
    let Some(library) = host.library.clone() else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if host
        .ctx
        .register_action(Box::new(NativeActionAdapter {
            signature: ActionSignature {
                name: name.clone(),
                category,
                description,
                params,
                modifiers,
            },
            callback: action.execute,
            property_ids: host.property_ids.clone(),
            property_kinds: host.property_kinds.clone(),
            service_values: host.service_values.clone(),
            _library: library,
        }))
        .is_err()
    {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    host.actions.push(name);
    NATIVE_STATUS_OK
}

pub(crate) unsafe extern "C" fn native_register_effect(
    host: *mut c_void,
    descriptor: NativeEffectDescriptor,
) -> i32 {
    use crate::timeline::effects::{
        EffectParamKind, EffectParamSpec, EffectPassSpec, PluginEffectData,
        register_extension_effect,
    };

    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };

    let effect_name = match unsafe { read_c_string(descriptor.name) } {
        Some(name) if !name.is_empty() => name,
        _ => {
            tracing::warn!("plugin effect registration rejected: missing name");
            return NATIVE_STATUS_TYPE_ERROR;
        },
    };
    let display_name = unsafe { read_c_string(descriptor.display_name) }.unwrap_or_default();
    if descriptor.pass_len == 0 || descriptor.passes.is_null() {
        tracing::warn!(
            effect = %effect_name,
            "plugin effect registration rejected: no compute passes"
        );
        return NATIVE_STATUS_TYPE_ERROR;
    }

    let kind_size = |kind: u32| -> Option<(EffectParamKind, u32, u32)> {
        use animatix_plugin_api::{
            NATIVE_EFFECT_PARAM_KIND_BOOL, NATIVE_EFFECT_PARAM_KIND_F32,
            NATIVE_EFFECT_PARAM_KIND_U32, NATIVE_EFFECT_PARAM_KIND_VEC2,
            NATIVE_EFFECT_PARAM_KIND_VEC4,
        };
        match kind {
            NATIVE_EFFECT_PARAM_KIND_F32 => Some((EffectParamKind::F32, 4, 4)),
            NATIVE_EFFECT_PARAM_KIND_U32 => Some((EffectParamKind::U32, 4, 4)),
            NATIVE_EFFECT_PARAM_KIND_BOOL => Some((EffectParamKind::Bool, 4, 4)),
            NATIVE_EFFECT_PARAM_KIND_VEC2 => Some((EffectParamKind::Vec2, 8, 8)),
            NATIVE_EFFECT_PARAM_KIND_VEC4 => Some((EffectParamKind::Vec4, 16, 16)),
            _ => None,
        }
    };
    let raw_params = unsafe { std::slice::from_raw_parts(descriptor.params, descriptor.param_len) };
    let mut params = Vec::with_capacity(descriptor.param_len);
    for (index, raw) in raw_params.iter().enumerate() {
        let Some(name) = (unsafe { read_c_string(raw.name) }) else {
            tracing::warn!(
                effect = %effect_name,
                "plugin effect registration rejected: parameter {index} has no name"
            );
            return NATIVE_STATUS_TYPE_ERROR;
        };
        let Some((kind, alignment, size)) = kind_size(raw.kind) else {
            tracing::warn!(
                effect = %effect_name,
                param = %name,
                "plugin effect registration rejected: unknown parameter kind"
            );
            return NATIVE_STATUS_TYPE_ERROR;
        };
        if raw.offset % alignment != 0 {
            tracing::warn!(
                effect = %effect_name,
                param = %name,
                "plugin effect registration rejected: offset {} is not {alignment}-byte aligned",
                raw.offset
            );
            return NATIVE_STATUS_TYPE_ERROR;
        }
        let identity = crate::timeline::effects::identity_for(kind, raw.identity);
        params.push(EffectParamSpec {
            name: name.into(),
            kind,
            identity,
            offset: raw.offset,
            size,
            unit: animatix_core::effect::EffectParamUnit::Normalized,
        });
    }
    let mut uniform_size = 0usize;
    for spec in &params {
        uniform_size = uniform_size.max(spec.offset as usize + spec.size as usize);
    }
    let uniform_size = uniform_size.div_ceil(16) * 16;
    let uniform_size = uniform_size.max(16) as u32;

    let raw_passes = unsafe { std::slice::from_raw_parts(descriptor.passes, descriptor.pass_len) };
    let mut passes = Vec::with_capacity(descriptor.pass_len);
    for (index, raw) in raw_passes.iter().enumerate() {
        let Some(wgsl) = (unsafe { read_c_string(raw.wgsl) }) else {
            tracing::warn!(
                effect = %effect_name,
                "plugin effect registration rejected: pass {index} has no WGSL source"
            );
            return NATIVE_STATUS_TYPE_ERROR;
        };
        let entry = unsafe { read_c_string(raw.entry) }.unwrap_or_else(|| "main".to_string());
        passes.push(EffectPassSpec {
            label: format!("{effect_name}.pass{index}").into(),
            wgsl: wgsl.into(),
            entry: entry.into(),
        });
    }

    let data = PluginEffectData {
        type_name: effect_name.clone().into(),
        display_name: display_name.clone().into(),
        params,
        passes,
        author_uniform_size: uniform_size,
        support_px: descriptor.support_px,
    };

    match register_extension_effect(data) {
        Some(_) => {
            tracing::debug!(
                effect = %effect_name,
                display = %display_name,
                "registered plugin effect"
            );
            if !host.effects.iter().any(|name| name == &effect_name) {
                host.effects.push(effect_name);
            }
            NATIVE_STATUS_OK
        },
        None => NATIVE_STATUS_TYPE_ERROR,
    }
}
