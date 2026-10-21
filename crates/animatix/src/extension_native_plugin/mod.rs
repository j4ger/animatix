//! Native cdylib plugin loading behind the `plugin-loading` feature.
//!
//! Plugins communicate through the unstable in-repo ABI in
//! `animatix-plugin-api`, so their Rust version and internal types can differ
//! from the host. The host keeps the loaded `Library` alive for the lifetime
//! of registered callbacks.

pub(crate) mod convert;
pub(crate) mod hosts;

#[cfg(test)]
mod tests;

#[cfg(test)]
pub(crate) use crate::ast::{InlineItem, Modifier};
#[cfg(test)]
pub(crate) use crate::timeline::ActorCategory;
#[cfg(test)]
pub(crate) use crate::timeline::property_track::TrackAccessor;
#[cfg(test)]
pub(crate) use animatix_plugin_api::*;
pub(crate) use convert::*;
pub(crate) use hosts::*;

use std::any::Any;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::path::Path;
use std::sync::Arc;

use animatix_plugin_api::{
    NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_STATUS_UNSUPPORTED, NativeAction,
    NativeActionContext, NativeActionExecuteFn, NativeActionParam, NativeAssignmentContext,
    NativeAssignmentFn, NativeClipPathCtx, NativeClipPathFn, NativeDefaultColorKeyFn,
    NativeDefaultPropsCtx, NativeDefaultPropsFn, NativeEffectDescriptor, NativeFinalizeContext,
    NativeFinalizeFn, NativeFunctionContext, NativeFunctionDescriptor, NativeInstallFn,
    NativeModifierValue, NativePluginApi, NativePrimitive, NativePrimitiveBuildCtx,
    NativePrimitiveBuildFn, NativePrimitiveEquationFragmentCtx, NativePrimitiveEquationFragmentFn,
    NativePrimitiveEvaluateCtx, NativePrimitiveEvaluateFn, NativePropertyDescriptor, NativeService,
    NativeValue, UNSTABLE_ABI_VERSION,
};
use libloading::Library;

use super::{ExtensionPlugin, PluginDisposer, PluginError};
use crate::ast::{Expr, Property};
use crate::extension_context::ExtensionContext;
use crate::primitives::{
    AssignmentCtx, BuildCtx, ChildProcessing, EquationFragment, EvaluateCtx, Primitive,
};
use crate::timeline::actions::registry::{ActionParam, ActionSignature, BuiltinAction};
use crate::timeline::{Environment, EvalError, ResizeMode};

/// A native plugin loaded from a `cdylib` shared library.
pub struct NativePlugin {
    name: String,
    library: Arc<Library>,
    api: NativePluginApi,
    install: Option<NativeInstallFn>,
}

impl NativePlugin {
    /// Load a native plugin and verify its unstable ABI snapshot.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, PluginError> {
        let path = path.as_ref();
        let library = unsafe { Library::new(path) }.map_err(|err| {
            PluginError(format!("cannot load plugin '{}': {err}", path.display()))
        })?;

        unsafe {
            let version = library
                .get::<unsafe extern "C" fn() -> u32>(b"animatix_plugin_abi_version")
                .map_err(|err| {
                    PluginError(format!("'{}' is not an Animatix plugin: {err}", path.display()))
                })?;
            if version() != UNSTABLE_ABI_VERSION {
                return Err(PluginError(format!(
                    "'{}' uses unstable ABI snapshot {}, host expects {}; rebuild the plugin from the same source tree",
                    path.display(),
                    version(),
                    UNSTABLE_ABI_VERSION
                )));
            }

            let name_fn = library
                .get::<unsafe extern "C" fn() -> *const c_char>(b"animatix_plugin_name")
                .map_err(|err| {
                    PluginError(format!("'{}' has no plugin name symbol: {err}", path.display()))
                })?;
            let name_ptr = name_fn();
            let name = if name_ptr.is_null() {
                path.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.display().to_string())
            } else {
                CStr::from_ptr(name_ptr).to_string_lossy().into_owned()
            };

            let install =
                library.get::<NativeInstallFn>(b"animatix_plugin_install").map_err(|err| {
                    PluginError(format!(
                        "'{}' has no animatix_plugin_install symbol: {err}",
                        path.display()
                    ))
                })?;
            let install = Some(*install);

            let api = NativePluginApi {
                size: std::mem::size_of::<NativePluginApi>(),
                version: UNSTABLE_ABI_VERSION,
                register_property: native_register_property,
                register_function: native_register_function,
                register_primitive: native_register_primitive,
                register_action: native_register_action,
                provide_service: native_provide_service,
                register_effect: native_register_effect,
            };

            Ok(Self {
                name,
                library: Arc::new(library),
                api,
                install,
            })
        }
    }
}

impl ExtensionPlugin for NativePlugin {
    fn name(&self) -> &str {
        &self.name
    }

    fn install(&self, ctx: &mut ExtensionContext) -> Result<PluginDisposer, PluginError> {
        let library = Arc::clone(&self.library);
        let (properties, functions, primitives, actions, services) = {
            let mut host = NativeHost {
                ctx,
                library: Some(Arc::clone(&self.library) as Arc<dyn Any + Send + Sync>),
                properties: Vec::new(),
                property_ids: HashMap::new(),
                property_kinds: HashMap::new(),
                service_values: HashMap::new(),
                functions: Vec::new(),
                primitives: Vec::new(),
                actions: Vec::new(),
                services: Vec::new(),
                effects: Vec::new(),
            };
            let install = self.install.expect("install symbol checked during load");
            let status = unsafe { (install)(&self.api, (&mut host as *mut NativeHost).cast()) };
            if status != NATIVE_STATUS_OK {
                host.rollback();
                return Err(PluginError(format!(
                    "{} install failed with status {status}",
                    self.name
                )));
            }
            (host.properties, host.functions, host.primitives, host.actions, host.services)
        };

        Ok(Box::new(move |ctx: &mut ExtensionContext| {
            let _keep_alive = library;
            for (actor_type, name) in properties {
                ctx.remove_property(&actor_type, &name);
            }
            for name in functions {
                ctx.remove_function(&name);
            }
            for name in primitives {
                ctx.remove_primitive(&name);
            }
            for name in actions {
                ctx.remove_action(&name);
            }
            for name in services {
                ctx.remove_service(&name);
            }
        }))
    }
}

struct NativeHost<'a> {
    ctx: &'a mut ExtensionContext,
    library: Option<Arc<dyn Any + Send + Sync>>,
    properties: Vec<(String, String)>,
    property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    property_kinds: HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    service_values: HashMap<String, usize>,
    functions: Vec<String>,
    primitives: Vec<String>,
    actions: Vec<String>,
    services: Vec<String>,
    /// Registered effect names (the registry is keyed by authored type name).
    effects: Vec<String>,
}

impl NativeHost<'_> {
    fn rollback(&mut self) {
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

struct NativeFunctionHost<'a> {
    env: &'a Environment,
    service_values: &'a HashMap<String, usize>,
    arena: NativeValueArena,
}

unsafe extern "C" fn native_function_get_env(
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

unsafe extern "C" fn native_function_get_service(
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

unsafe extern "C" fn native_register_property(
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

unsafe extern "C" fn native_register_function(
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

unsafe extern "C" fn native_register_primitive(
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

unsafe extern "C" fn native_register_action(host: *mut c_void, action: NativeAction) -> i32 {
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

struct NativeServiceHandle {
    value: usize,
    drop: Option<unsafe extern "C" fn(usize)>,
    _library: Arc<dyn Any + Send + Sync>,
}

impl Drop for NativeServiceHandle {
    fn drop(&mut self) {
        if let Some(drop) = self.drop {
            unsafe { drop(self.value) };
        }
    }
}

unsafe extern "C" fn native_provide_service(host: *mut c_void, service: NativeService) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(service.name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(library) = host.library.clone() else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let service_descriptor = animatix_syntax::schema::ServiceDescriptor {
        name: name.clone(),
        type_info: unsafe { read_c_string(service.type_info) },
        help: unsafe { read_c_string(service.help) },
    };
    if host
        .ctx
        .provide_with_descriptor(
            name.clone(),
            NativeServiceHandle {
                value: service.value,
                drop: service.drop,
                _library: library,
            },
            service_descriptor,
        )
        .is_err()
    {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    host.service_values.insert(name.clone(), service.value);
    host.services.push(name);
    NATIVE_STATUS_OK
}

unsafe extern "C" fn native_register_effect(
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

/// Host-side adapter that turns a native primitive descriptor into a runtime
/// [`Primitive`].
struct NativePrimitiveAdapter {
    type_name: String,
    info: animatix_std::PrimitiveInfo,
    child_processing: ChildProcessing,
    declared_properties: Vec<String>,
    resize_mode: ResizeMode,
    property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    property_kinds: HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    service_values: HashMap<String, usize>,
    build: Option<NativePrimitiveBuildFn>,
    evaluate: Option<NativePrimitiveEvaluateFn>,
    handle_assignment: Option<NativeAssignmentFn>,
    finalize_container_build: Option<NativeFinalizeFn>,
    default_props: Option<NativeDefaultPropsFn>,
    default_color_key: Option<NativeDefaultColorKeyFn>,
    clip_path: Option<NativeClipPathFn>,
    equation_fragment: Option<NativePrimitiveEquationFragmentFn>,
    _library: Arc<dyn Any + Send + Sync>,
}

impl NativePrimitiveAdapter {
    fn new(
        primitive: NativePrimitive,
        library: Arc<dyn Any + Send + Sync>,
        property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
        property_kinds: HashMap<String, animatix_syntax::schema::PropertyValueKind>,
        service_values: HashMap<String, usize>,
    ) -> Option<Self> {
        let type_name = unsafe { read_c_string(primitive.type_name)? };
        let display_name =
            unsafe { read_c_string(primitive.display_name) }.unwrap_or_else(|| type_name.clone());
        let icon_id =
            unsafe { read_c_string(primitive.icon_id) }.unwrap_or_else(|| "extension".to_string());
        let category = native_primitive_category(primitive.category)?;
        let declared_properties = if primitive.properties.is_null() || primitive.property_len == 0 {
            Vec::new()
        } else {
            let names =
                unsafe { std::slice::from_raw_parts(primitive.properties, primitive.property_len) };
            names.iter().filter_map(|ptr| unsafe { read_c_string(*ptr) }).collect()
        };
        let child_processing =
            native_child_processing(primitive.child_processing).unwrap_or_default();
        let capabilities = native_capabilities(primitive.capabilities);
        let info = animatix_std::PrimitiveInfo {
            type_name: type_name.clone().into(),
            display_name: display_name.into(),
            category,
            icon_id: icon_id.into(),
            advanced: primitive.advanced,
            capabilities,
            child_processing,
            shape: None,
            text: None,
            stroke_path: false,
            connector: false,
            plot: None,
            doc: "",
            default_stub: None,
        };
        Some(Self {
            type_name,
            info,
            child_processing,
            declared_properties,
            resize_mode: native_resize_mode(primitive.resize_mode),
            property_ids,
            property_kinds,
            service_values,
            build: primitive.build,
            evaluate: primitive.evaluate,
            handle_assignment: primitive.handle_assignment,
            finalize_container_build: primitive.finalize_container_build,
            default_props: primitive.default_props,
            default_color_key: primitive.default_color_key,
            clip_path: primitive.clip_path,
            equation_fragment: primitive.equation_fragment,
            _library: library,
        })
    }
}

impl Primitive for NativePrimitiveAdapter {
    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn declared_property_names(&self) -> Vec<&str> {
        self.declared_properties.iter().map(String::as_str).collect()
    }

    fn declares_property(&self, name: &str) -> bool {
        self.declared_properties.iter().any(|declared| declared == name)
    }

    fn resize_mode(&self, _caps: &crate::timeline::ActorCaps) -> ResizeMode {
        self.resize_mode
    }

    fn default_props(&self, scene_dimensions: &crate::timeline::SceneDimensions) -> Vec<Property> {
        let Some(callback) = self.default_props else {
            return Vec::new();
        };
        let mut host = NativeDefaultPropsHost { props: Vec::new() };
        let mut native_ctx = NativeDefaultPropsCtx {
            size: std::mem::size_of::<NativeDefaultPropsCtx>(),
            scene_width: scene_dimensions.width as f64,
            scene_height: scene_dimensions.height as f64,
            host: (&mut host as *mut NativeDefaultPropsHost).cast(),
            append_property: Some(native_default_props_append),
        };
        let status = unsafe { callback(&mut native_ctx) };
        let mut props = Vec::new();
        for (name, value) in host.props {
            match crate::timeline::value_parser::value_to_expr(&value) {
                Some(expr) => props.push(Property::new(name, expr)),
                None => {
                    tracing::warn!(
                        "native primitive '{}' default property '{name}' has a value that \
                         cannot be expressed as a source default; skipping",
                        self.type_name
                    );
                },
            }
        }
        if status != NATIVE_STATUS_OK {
            tracing::warn!(
                "native primitive '{}' default-props callback failed with status {status}",
                self.type_name
            );
        }
        props
    }

    fn default_color_key(
        &self,
        property: &str,
        _caps: &crate::timeline::ActorCaps,
    ) -> Option<&'static str> {
        let callback = self.default_color_key?;
        let property_c = std::ffi::CString::new(property).ok()?;
        let mut out: *const c_char = std::ptr::null();
        let status = unsafe { callback(property_c.as_ptr(), &mut out) };
        if status != NATIVE_STATUS_OK || out.is_null() {
            return None;
        }
        unsafe {
            let text = std::ffi::CStr::from_ptr(out).to_str().ok()?;
            Some(std::mem::transmute::<&str, &'static str>(text))
        }
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        props: &[crate::ast::Property],
        modifiers: &[crate::ast::Modifier],
        children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        let BuildCtx {
            timeline,
            time_ms,
            parent_label,
            diagnostics,
            coordinate_context: _,
        } = ctx;
        let time_ms = *time_ms;
        let parent_label = *parent_label;
        {
            let track = timeline.tracks.entry(label.to_string()).or_insert_with(|| {
                crate::timeline::AnimationTrack::new(label.to_string(), self.type_name.clone())
            });
            let type_name = self.type_name.clone();
            track.actor_type = type_name;
            track.set_caps(animatix_std::caps_from_info(&self.info));
            track.rebuild_property_plan();
        }
        let Some(build) = self.build else {
            return Ok(());
        };
        let status = {
            let mut host = NativePrimitiveBuildHost::new(
                timeline,
                time_ms,
                parent_label,
                label,
                props,
                modifiers,
                children,
                diagnostics,
            );
            let parent_label_ptr = match parent_label {
                Some(parent_label) => host.arena.string(parent_label).0,
                None => std::ptr::null(),
            };
            let mut native_ctx = NativePrimitiveBuildCtx {
                size: std::mem::size_of::<NativePrimitiveBuildCtx>(),
                time_ms,
                host: (&mut host as *mut NativePrimitiveBuildHost).cast(),
                parent_label: parent_label_ptr,
                get_property_count: native_build_get_property_count,
                get_property: native_build_get_property,
                get_modifier_count: native_build_get_modifier_count,
                get_modifier: native_build_get_modifier,
                get_child_count: native_build_get_child_count,
                get_child: native_build_get_child,
                report_diagnostic: native_build_report_diagnostic,
            };
            unsafe { build(&mut native_ctx) }
        };
        if status != NATIVE_STATUS_OK {
            diagnostics.push(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                crate::diagnostics::DiagnosticPhase::Build,
                format!("native primitive '{}' build failed with status {status}", self.type_name),
            ));
        }
        Ok(())
    }

    fn handle_assignment(
        &self,
        track: &mut crate::timeline::AnimationTrack,
        property: &str,
        value: &Expr,
        ctx: &mut AssignmentCtx,
        env: &Environment,
        diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
        subject: &str,
    ) -> bool {
        let Some(callback) = self.handle_assignment else {
            return false;
        };
        let Some(value) = crate::timeline::lookup::evaluate_expr_with_lookup_diagnostic(
            value,
            env,
            diagnostics,
            subject,
        ) else {
            return false;
        };
        let mut arena = NativeValueArena::default();
        let Ok(native_value) = value_to_native(&value, &mut arena) else {
            return false;
        };
        let property_ptr = arena.string(property).0;
        let status = {
            let mut host = NativeAssignmentHost {
                track,
                property_ids: &self.property_ids,
                property_kinds: &self.property_kinds,
                value: native_value,
            };
            let mut native_ctx = NativeAssignmentContext {
                size: std::mem::size_of::<NativeAssignmentContext>(),
                host: (&mut host as *mut NativeAssignmentHost).cast(),
                property: property_ptr,
                t_start_ms: ctx.t_start_ms,
                t_end_ms: ctx.t_end_ms,
                easing: easing_code(ctx.easing),
                get_value: native_assignment_get_value,
                write_keyframe: native_assignment_write_keyframe,
            };
            unsafe { callback(&mut native_ctx) }
        };
        match status {
            NATIVE_STATUS_OK => true,
            NATIVE_STATUS_UNSUPPORTED => false,
            _ => {
                diagnostics.push(crate::diagnostics::Diagnostic::error(
                    crate::diagnostics::DiagnosticCode::InvalidAssignmentTarget,
                    crate::diagnostics::DiagnosticPhase::Build,
                    format!(
                        "native primitive '{}' failed assignment {property} with status {status}",
                        self.type_name
                    ),
                ));
                true
            },
        }
    }

    fn finalize_container_build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[Property],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        let Some(callback) = self.finalize_container_build else {
            return Ok(());
        };
        let child_count =
            ctx.timeline.tracks.get(label).map(|track| track.children.len()).unwrap_or(0);
        let label_c = std::ffi::CString::new(label).expect("actor label has no nul");
        let mut host = NativeFinalizeHost;
        let mut native_ctx = NativeFinalizeContext {
            size: std::mem::size_of::<NativeFinalizeContext>(),
            host: (&mut host as *mut NativeFinalizeHost).cast(),
            label: label_c.as_ptr(),
            child_count,
        };
        let status = unsafe { callback(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            ctx.diagnostics.push(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::DiagnosticCode::InvalidModifierValue,
                crate::diagnostics::DiagnosticPhase::Build,
                format!(
                    "native primitive '{}' finalize failed with status {status}",
                    self.type_name
                ),
            ));
        }
        Ok(())
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        let Some(evaluate) = self.evaluate else {
            return Ok(None);
        };
        let (text_compiler, font_context) = match text_ctx {
            Some(text_ctx) => (Some(&mut *text_ctx.text_compiler), Some(text_ctx.font_context)),
            None => (None, None),
        };
        let mut host = NativePrimitiveEvaluateHost {
            ctx,
            property_ids: &self.property_ids,
            service_values: &self.service_values,
            text_compiler,
            font_context,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let mut native_ctx = NativePrimitiveEvaluateCtx {
            size: std::mem::size_of::<NativePrimitiveEvaluateCtx>(),
            time_ms: ctx.time_ms as f64,
            host: (&mut host as *mut NativePrimitiveEvaluateHost).cast(),
            get_property: Some(native_get_property),
            get_service: Some(native_get_service),
            append_path: Some(native_append_path),
            append_text: Some(native_append_text),
            append_image: Some(native_append_image),
            append_highlight: Some(native_append_highlight),
        };
        let status = unsafe { evaluate(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            return Err(crate::renderer::error::RenderError::NativePrimitive(format!(
                "native primitive '{}' failed with status {status}",
                self.type_name
            )));
        }
        Ok(Some(host.commands))
    }

    fn clip_path(
        &self,
        ctx: &EvaluateCtx,
        _caps: &crate::timeline::ActorCaps,
    ) -> Option<kurbo::BezPath> {
        let clip_path = self.clip_path?;
        let mut host = NativePrimitiveEvaluateHost {
            ctx,
            property_ids: &self.property_ids,
            service_values: &self.service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let mut native_ctx = NativeClipPathCtx {
            size: std::mem::size_of::<NativeClipPathCtx>(),
            time_ms: ctx.time_ms as f64,
            host: (&mut host as *mut NativePrimitiveEvaluateHost).cast(),
            append_path: Some(native_append_path),
        };
        let status = unsafe { clip_path(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            return None;
        }
        crate::primitives::clip_bezpath_from_commands(&host.commands)
    }

    fn equation_fragment(&self, ctx: &EvaluateCtx) -> Option<EquationFragment> {
        let equation_fragment = self.equation_fragment?;
        let mut host = NativeEquationFragmentHost::default();
        let mut native_ctx = NativePrimitiveEquationFragmentCtx {
            size: std::mem::size_of::<NativePrimitiveEquationFragmentCtx>(),
            time_ms: ctx.time_ms as f64,
            host: (&mut host as *mut NativeEquationFragmentHost).cast(),
            write_content: Some(native_write_content),
            highlight_color: [0.0; 4],
            highlight_opacity: 0.0,
            highlight_padding: 4.0,
            highlight_radius: 3.0,
            highlight_blend: 0,
        };
        let status = unsafe { equation_fragment(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            return None;
        }
        let color = native_ctx.highlight_color;
        Some(EquationFragment {
            content: host.content,
            highlight_color: [
                color[0].clamp(0.0, 1.0) as f32,
                color[1].clamp(0.0, 1.0) as f32,
                color[2].clamp(0.0, 1.0) as f32,
                color[3].clamp(0.0, 1.0) as f32,
            ],
            highlight_opacity: native_ctx.highlight_opacity.clamp(0.0, 1.0) as f32,
            highlight_padding: native_ctx.highlight_padding.max(0.0) as f32,
            highlight_radius: native_ctx.highlight_radius.max(0.0) as f32,
            highlight_blend: native_blend_from_code(native_ctx.highlight_blend),
        })
    }

    fn render_children(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
        children: &[&str],
    ) -> Result<(), crate::renderer::error::RenderError> {
        let timeline = ctx.timeline;
        match self.child_processing {
            ChildProcessing::Filter => timeline.render_filter_children_ctx(ctx),
            ChildProcessing::Glass => timeline.render_glass_children_ctx(ctx),
            ChildProcessing::Mask => timeline.render_mask_children_ctx(ctx),
            ChildProcessing::Equation => timeline.render_equation_children_ctx(ctx),
            ChildProcessing::Captions => timeline.render_captions_children_ctx(ctx),
            ChildProcessing::Generic => ctx.render_children_default(children),
        }
        Ok(())
    }

    fn render_children_by_id(
        &self,
        ctx: &mut crate::primitives::RenderChildrenCtx<'_, '_, '_>,
        children_ids: &[crate::timeline::EntityId],
    ) -> Result<(), crate::renderer::error::RenderError> {
        let timeline = ctx.timeline;
        match self.child_processing {
            ChildProcessing::Filter => timeline.render_filter_children_ctx(ctx),
            ChildProcessing::Glass => timeline.render_glass_children_ctx(ctx),
            ChildProcessing::Mask => timeline.render_mask_children_ctx(ctx),
            ChildProcessing::Equation => timeline.render_equation_children_ctx(ctx),
            ChildProcessing::Captions => timeline.render_captions_children_ctx(ctx),
            ChildProcessing::Generic => ctx.render_children_ids(children_ids),
        }
        Ok(())
    }
}

/// Host-side adapter that turns a native action callback into a runtime action.
struct NativeActionAdapter {
    signature: ActionSignature,
    callback: NativeActionExecuteFn,
    property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    property_kinds: HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    service_values: HashMap<String, usize>,
    _library: Arc<dyn Any + Send + Sync>,
}

impl BuiltinAction for NativeActionAdapter {
    fn signature(&self) -> ActionSignature {
        self.signature.clone()
    }

    fn execute(
        &self,
        action: &crate::ast::Action,
        time_ms: f64,
        timeline: &mut crate::timeline::Timeline,
        diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    ) {
        let mut arena = NativeValueArena::default();
        let args = action
            .args
            .iter()
            .map(|arg| {
                crate::timeline::utils::evaluate_expr(arg, &timeline.env)
                    .ok()
                    .and_then(|value| value_to_native(&value, &mut arena).ok())
                    .unwrap_or_default()
            })
            .collect();
        let modifiers = action
            .modifiers
            .iter()
            .map(|modifier| {
                let name_ptr = modifier
                    .name
                    .as_deref()
                    .map(|name| arena.string(name).0)
                    .unwrap_or(std::ptr::null());
                let value = crate::timeline::utils::evaluate_expr(&modifier.value, &timeline.env)
                    .ok()
                    .and_then(|value| value_to_native(&value, &mut arena).ok())
                    .unwrap_or_default();
                NativeModifierValue {
                    name: name_ptr,
                    value,
                }
            })
            .collect();
        let targets = action.targets.iter().map(|target| arena.string(target).0).collect();
        let mut host = NativeActionHost {
            timeline,
            targets,
            args,
            modifiers,
            property_ids: &self.property_ids,
            property_kinds: &self.property_kinds,
            service_values: &self.service_values,
        };
        let mut native_ctx = NativeActionContext {
            size: std::mem::size_of::<NativeActionContext>(),
            host: (&mut host as *mut NativeActionHost).cast(),
            time_ms,
            get_target_count: native_action_get_target_count,
            get_target: native_action_get_target,
            get_arg_count: native_action_get_arg_count,
            get_arg: native_action_get_arg,
            get_modifier_count: native_action_get_modifier_count,
            get_modifier: native_action_get_modifier,
            write_keyframe: native_action_write_keyframe,
            get_service: native_action_get_service,
        };
        let status = unsafe { (self.callback)(&mut native_ctx) };
        if status != NATIVE_STATUS_OK {
            let detail = if status == NATIVE_STATUS_UNSUPPORTED {
                "unsupported by the current native plugin API (e.g. a built-in property, \
                 which native actions cannot keyframe yet)"
            } else {
                "failed"
            };
            diagnostics.push(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::DiagnosticCode::UnknownAction,
                crate::diagnostics::DiagnosticPhase::Build,
                format!("native action '{}' {detail} with status {status}", self.signature.name),
            ));
        }
    }
}
