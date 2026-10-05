//! Native cdylib plugin loading behind the `plugin-loading` feature.
//!
//! Plugins communicate through the unstable in-repo ABI in
//! `animatix-plugin-api`, so their Rust version and internal types can differ
//! from the host. The host keeps the loaded `Library` alive for the lifetime
//! of registered callbacks.

use std::any::Any;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::path::Path;
use std::sync::Arc;

use animatix_plugin_api::{
    NATIVE_CAP_IMAGE_PAYLOAD, NATIVE_CAP_IS_CONTAINER, NATIVE_CAP_IS_SHAPE,
    NATIVE_CAP_LAYOUT_CONTAINER, NATIVE_CAP_MORPHABLE_PATHS, NATIVE_CAP_PLOT_GEOMETRY,
    NATIVE_CAP_PLOT_HOST, NATIVE_CAP_TEXT_PATHS, NATIVE_CAP_VECTOR_PATHS,
    NATIVE_CAP_VECTOR_REVEAL_TARGET, NATIVE_EASING_BACK, NATIVE_EASING_BOUNCE_IN,
    NATIVE_EASING_ELASTIC, NATIVE_EASING_EXPO, NATIVE_EASING_EXPO_IN_OUT, NATIVE_EASING_EXPO_OUT,
    NATIVE_EASING_IN, NATIVE_EASING_IN_OUT, NATIVE_EASING_LINEAR, NATIVE_EASING_OUT,
    NATIVE_EASING_UNSUPPORTED, NATIVE_EFFECT_PARAM_KIND_BOOL, NATIVE_EFFECT_PARAM_KIND_F32,
    NATIVE_EFFECT_PARAM_KIND_U32, NATIVE_EFFECT_PARAM_KIND_VEC2, NATIVE_EFFECT_PARAM_KIND_VEC4,
    NATIVE_PATH_ARC, NATIVE_PATH_CUBIC, NATIVE_PATH_ELLIPSE, NATIVE_PATH_LINE, NATIVE_PATH_POLYGON,
    NATIVE_PATH_QUADRATIC, NATIVE_PATH_RECT, NATIVE_PATH_ROUNDED_RECT, NATIVE_PROPERTY_BOOL,
    NATIVE_PROPERTY_ENUM, NATIVE_PROPERTY_F32, NATIVE_PROPERTY_GENERIC, NATIVE_PROPERTY_POINT_LIST,
    NATIVE_PROPERTY_STRING, NATIVE_PROPERTY_U32, NATIVE_PROPERTY_VEC2, NATIVE_PROPERTY_VEC4,
    NATIVE_RESIZE_MODE_SCALE, NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR,
    NATIVE_STATUS_UNSUPPORTED, NATIVE_TEXT_KIND_CODE, NATIVE_TEXT_KIND_TYST, NATIVE_VALUE_BOOL,
    NATIVE_VALUE_COLOR, NATIVE_VALUE_COMMAND_LIST, NATIVE_VALUE_ENUM, NATIVE_VALUE_LIST,
    NATIVE_VALUE_NUM, NATIVE_VALUE_POINT_LIST, NATIVE_VALUE_STRING, NATIVE_VALUE_STRING_LIST,
    NATIVE_VALUE_TRANSFORM, NATIVE_VALUE_U32, NATIVE_VALUE_VARIANT, NATIVE_VALUE_VEC2,
    NATIVE_VALUE_VEC3, NATIVE_VALUE_VEC4, NativeAction, NativeActionContext, NativeActionExecuteFn,
    NativeActionParam, NativeAssignmentContext, NativeAssignmentFn, NativeChild, NativeClipPathCtx,
    NativeClipPathFn, NativeDefaultColorKeyFn, NativeDefaultPropsCtx, NativeDefaultPropsFn,
    NativeEffectDescriptor, NativeFinalizeContext, NativeFinalizeFn, NativeFunctionContext,
    NativeFunctionDescriptor, NativeHighlightCommand, NativeImageCommand, NativeInstallFn,
    NativeModifierValue, NativePathCommand, NativePluginApi, NativePrimitive,
    NativePrimitiveBuildCtx, NativePrimitiveBuildFn, NativePrimitiveEquationFragmentCtx,
    NativePrimitiveEquationFragmentFn, NativePrimitiveEvaluateCtx, NativePrimitiveEvaluateFn,
    NativePropertyDescriptor, NativePropertyValue, NativeService, NativeTextCommand, NativeValue,
    UNSTABLE_ABI_VERSION,
};
use kurbo::Shape;
use libloading::Library;

use crate::ast::{Expr, InlineItem, Modifier, Property};
use crate::extension_context::ExtensionContext;
use crate::primitives::{
    AssignmentCtx, BuildCtx, ChildProcessing, EquationFragment, EvaluateCtx, Primitive,
};
use crate::timeline::actions::registry::{ActionParam, ActionSignature, BuiltinAction};
use crate::timeline::property_registry::lookup_property;
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::{ActorCategory, Environment, EvalError, PropertyValue, ResizeMode, Value};

use super::{ExtensionPlugin, PluginDisposer, PluginError};

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
    // The runtime resolves extension properties by name across the whole
    // plugin catalog (callbacks receive only the property name plus the
    // actor context), so a plugin that reuses one name on multiple actor
    // types cannot be disambiguated at the callback layer. Reject it up
    // front rather than silently overwriting the earlier `PropertyId` in
    // `property_ids` / `property_kinds`.
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

/// Register a plugin-authored post-processing effect.
///
/// The plugin ships WGSL source text and a parameter schema — never a GPU
/// handle. The host validates the declared uniform layout at registration and
/// compiles the shader with its own device at first render (`docs/effects.md`
/// §5/§7): wgpu validates syntax but not termination, so this is
/// trusted-authoring, and effects are GPU-only (no backend means
/// "skip + warn" with a diagnostic, like built-ins).
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

    // ── Parameters ──
    let kind_size = |kind: u32| -> Option<(EffectParamKind, u32, u32)> {
        // (kind, required alignment, byte size)
        match kind {
            NATIVE_EFFECT_PARAM_KIND_F32 => Some((EffectParamKind::F32, 4, 4)),
            NATIVE_EFFECT_PARAM_KIND_U32 => Some((EffectParamKind::U32, 4, 4)),
            NATIVE_EFFECT_PARAM_KIND_BOOL => Some((EffectParamKind::Bool, 4, 4)),
            NATIVE_EFFECT_PARAM_KIND_VEC2 => Some((EffectParamKind::Vec2, 8, 8)),
            NATIVE_EFFECT_PARAM_KIND_VEC4 => Some((EffectParamKind::Vec4, 16, 16)),
            _ => None,
        }
    };
    // SAFETY: the plugin passes a valid `param_len`-sized array of
    // `NativeEffectParam` for the duration of this call.
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
        // Same `[x, y, z, w]` identity convention as the built-in contract table.
        let identity = crate::timeline::effects::identity_for(kind, raw.identity);
        params.push(EffectParamSpec {
            name: name.into(),
            kind,
            identity,
            offset: raw.offset,
            size,
        });
    }
    // The uniform buffer must cover every declared parameter and stay a
    // multiple of 16 bytes.
    let mut uniform_size = 0usize;
    for spec in &params {
        uniform_size = uniform_size.max(spec.offset as usize + spec.size as usize);
    }
    let uniform_size = uniform_size.div_ceil(16) * 16;
    let uniform_size = uniform_size.max(16) as u32;

    // ── Passes ──
    // SAFETY: the plugin passes a valid `pass_len`-sized array of
    // `NativeEffectPass` for the duration of this call.
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
            // Idempotent re-registration (plugin hot reload) reuses the name;
            // track it once so rollback unregisters exactly once.
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

/// Host collection for a native `default_props` callback.
struct NativeDefaultPropsHost {
    props: Vec<(String, crate::timeline::Value)>,
}

unsafe extern "C" fn native_default_props_append(
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
        // Contract: plugins return a pointer valid for the plugin library's
        // lifetime (a `static` string), so extending the borrow to `'static`
        // matches that guarantee.
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
        } = ctx;
        let time_ms = *time_ms;
        let parent_label = *parent_label;
        {
            let track = timeline.tracks.entry(label.to_string()).or_insert_with(|| {
                crate::timeline::AnimationTrack::new(label.to_string(), self.type_name.clone())
            });
            let type_name = self.type_name.clone();
            track.actor_type = type_name;
            // Native adapters carry their own registration info; the track
            // caps stay consistent with it.
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
        // Concatenate every emitted subpath into one clip geometry.
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

    /// Native primitives cannot implement the built-in child pipelines, so they
    /// reuse the host's strategy selected by their declared `child_processing`.
    /// Without this, a plugin declaring `Filter`/`Glass`/`Mask`/`Equation` would fall
    /// back to the generic recursion and silently change behavior.
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
            ChildProcessing::Generic => ctx.render_children_default(children),
        }
        Ok(())
    }
}

/// Host collection for a native equation-fragment callback.
#[derive(Default)]
struct NativeEquationFragmentHost {
    content: String,
}

unsafe extern "C" fn native_write_content(
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

/// Map a `NativeHighlightCommand.blend` / equation-fragment blend code.
fn native_blend_from_code(code: u32) -> vello::peniko::Mix {
    match code {
        1 => vello::peniko::Mix::Multiply,
        2 => vello::peniko::Mix::Difference,
        3 => vello::peniko::Mix::Screen,
        _ => vello::peniko::Mix::Normal,
    }
}

struct NativePrimitiveEvaluateHost<'a> {
    ctx: &'a EvaluateCtx<'a>,
    property_ids: &'a HashMap<String, animatix_syntax::schema::PropertyId>,
    service_values: &'a HashMap<String, usize>,
    text_compiler: Option<&'a mut crate::renderer::text::TextCompiler>,
    font_context: Option<&'a crate::renderer::text::FontContext>,
    commands: Vec<crate::primitives::RenderCommand>,
    arena: NativeValueArena,
}

unsafe extern "C" fn native_get_property(
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
        crate::timeline::property_engine::read_property_plan_slot(
            host.ctx.track,
            *id,
            host.ctx.time_ms,
        )
    } else if let Some(schema) = lookup_property(&name) {
        crate::timeline::dispatch::read_property_value(
            host.ctx.track,
            schema.field,
            host.ctx.time_ms,
        )
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

unsafe extern "C" fn native_get_service(
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

struct NativePrimitiveBuildHost<'a> {
    props: Vec<NativePropertyValue>,
    modifiers: Vec<NativeModifierValue>,
    children: Vec<NativeChild>,
    _child_props: Vec<Vec<NativePropertyValue>>,
    diagnostics: &'a mut Vec<crate::diagnostics::Diagnostic>,
    arena: NativeValueArena,
}

fn native_build_value(
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

fn native_build_property(
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

fn native_build_modifier(
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

fn native_build_child(
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
    fn new(
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

unsafe extern "C" fn native_build_get_property_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return 0;
    };
    host.props.len()
}

unsafe extern "C" fn native_build_get_property(
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

unsafe extern "C" fn native_build_get_modifier_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return 0;
    };
    host.modifiers.len()
}

unsafe extern "C" fn native_build_get_modifier(
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

unsafe extern "C" fn native_build_get_child_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveBuildHost).as_ref() }) else {
        return 0;
    };
    host.children.len()
}

unsafe extern "C" fn native_build_get_child(
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

unsafe extern "C" fn native_build_report_diagnostic(
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

struct NativeAssignmentHost<'a> {
    track: &'a mut crate::timeline::AnimationTrack,
    property_ids: &'a HashMap<String, animatix_syntax::schema::PropertyId>,
    property_kinds: &'a HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    value: NativeValue,
}

unsafe extern "C" fn native_assignment_get_value(host: *mut c_void, out: *mut NativeValue) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeAssignmentHost).as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    unsafe { *out = host.value };
    NATIVE_STATUS_OK
}

fn native_to_property_value(
    native: NativeValue,
    kind: animatix_syntax::schema::PropertyValueKind,
) -> Option<PropertyValue> {
    use animatix_syntax::schema::PropertyValueKind;
    match kind {
        PropertyValueKind::F32 => {
            (native.tag == NATIVE_VALUE_NUM).then_some(PropertyValue::F32(native.num as f32))
        },
        PropertyValueKind::U32 => (native.tag == NATIVE_VALUE_NUM
            || native.tag == NATIVE_VALUE_U32)
            .then_some(PropertyValue::U32(native.num.max(0.0) as u32)),
        PropertyValueKind::Bool => {
            (native.tag == NATIVE_VALUE_BOOL).then_some(PropertyValue::Bool(native.boolean))
        },
        PropertyValueKind::Vec2 => (native.tag == NATIVE_VALUE_VEC2)
            .then_some(PropertyValue::Vec2([native.vec[0] as f32, native.vec[1] as f32])),
        PropertyValueKind::Vec4 => (native.tag == NATIVE_VALUE_VEC4
            || native.tag == NATIVE_VALUE_COLOR)
            .then_some(PropertyValue::Vec4([
                native.vec[0] as f32,
                native.vec[1] as f32,
                native.vec[2] as f32,
                native.vec[3] as f32,
            ])),
        PropertyValueKind::String => read_native_string(&native).ok().map(PropertyValue::String),
        PropertyValueKind::PointList => {
            if native.tag != NATIVE_VALUE_POINT_LIST || native.list.is_null() {
                return None;
            }
            let values = unsafe { std::slice::from_raw_parts(native.list, native.list_len) };
            let points = values
                .iter()
                .filter(|value| value.tag == NATIVE_VALUE_VEC2)
                .map(|value| [value.vec[0] as f32, value.vec[1] as f32])
                .collect();
            Some(PropertyValue::PointList(points))
        },
        PropertyValueKind::Generic => match native.tag {
            NATIVE_VALUE_NUM => Some(PropertyValue::F32(native.num as f32)),
            NATIVE_VALUE_BOOL => Some(PropertyValue::Bool(native.boolean)),
            NATIVE_VALUE_VEC2 => {
                Some(PropertyValue::Vec2([native.vec[0] as f32, native.vec[1] as f32]))
            },
            NATIVE_VALUE_VEC4 => Some(PropertyValue::Vec4([
                native.vec[0] as f32,
                native.vec[1] as f32,
                native.vec[2] as f32,
                native.vec[3] as f32,
            ])),
            NATIVE_VALUE_COLOR => Some(PropertyValue::Color([
                native.vec[0] as f32,
                native.vec[1] as f32,
                native.vec[2] as f32,
                native.vec[3] as f32,
            ])),
            NATIVE_VALUE_STRING => read_native_string(&native).ok().map(PropertyValue::String),
            _ => None,
        },
    }
}

/// Decode an easing code a plugin sent.
///
/// `None` means the host does not know that code, and both callers turn it into
/// `NATIVE_STATUS_TYPE_ERROR`. Falling back to [`Easing::Linear`] instead would
/// be the silent drop this table exists to prevent: a plugin that asked for
/// `bounce-in` would get a straight line and no diagnostic.
fn native_easing(code: u32) -> Option<crate::easing::Easing> {
    use crate::easing::Easing;
    match code {
        NATIVE_EASING_LINEAR => Some(Easing::Linear),
        NATIVE_EASING_IN => Some(Easing::EaseIn),
        NATIVE_EASING_OUT => Some(Easing::EaseOut),
        NATIVE_EASING_IN_OUT => Some(Easing::EaseInOut),
        NATIVE_EASING_BOUNCE_IN => Some(Easing::Bounce),
        NATIVE_EASING_ELASTIC => Some(Easing::Elastic),
        NATIVE_EASING_BACK => Some(Easing::Back),
        NATIVE_EASING_EXPO => Some(Easing::Expo),
        NATIVE_EASING_EXPO_OUT => Some(Easing::ExpoOut),
        NATIVE_EASING_EXPO_IN_OUT => Some(Easing::ExpoInOut),
        // Includes NATIVE_EASING_UNSUPPORTED (a curve with arguments) and any
        // code outside the table.
        _ => None,
    }
}

/// Encode an easing curve for a plugin.
///
/// Exhaustive by construction: adding an `Easing` variant is a compile error
/// here until it is given a code or explicitly declared un-encodable. That is
/// the guard the old `_ => 0` arm did not provide — it quietly reported every
/// curve outside the original three as linear.
fn easing_code(easing: crate::easing::Easing) -> u32 {
    use crate::easing::Easing;
    match easing {
        Easing::Linear => NATIVE_EASING_LINEAR,
        Easing::EaseIn => NATIVE_EASING_IN,
        Easing::EaseOut => NATIVE_EASING_OUT,
        Easing::EaseInOut => NATIVE_EASING_IN_OUT,
        Easing::Bounce => NATIVE_EASING_BOUNCE_IN,
        Easing::Elastic => NATIVE_EASING_ELASTIC,
        Easing::Back => NATIVE_EASING_BACK,
        Easing::Expo => NATIVE_EASING_EXPO,
        Easing::ExpoOut => NATIVE_EASING_EXPO_OUT,
        Easing::ExpoInOut => NATIVE_EASING_EXPO_IN_OUT,
        // Carries parameters; one u32 cannot name it.
        Easing::Spring { .. } | Easing::CubicBezier(_) => NATIVE_EASING_UNSUPPORTED,
    }
}

unsafe extern "C" fn native_assignment_write_keyframe(
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
    // Native plugins can only keyframe extension properties today. A built-in
    // property is recognized but intentionally not writable through this host
    // callback; report UNSUPPORTED so the caller falls through to the generic
    // engine path instead of failing the whole assignment.
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
        host.track,
        *id,
        crate::timeline::PropertyKind::from(*kind),
        property_value,
        t_start_ms,
        t_end_ms,
        easing,
    );
    NATIVE_STATUS_OK
}

struct NativeFinalizeHost;

unsafe extern "C" fn native_append_path(host: *mut c_void, command: NativePathCommand) -> i32 {
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

unsafe extern "C" fn native_append_text(host: *mut c_void, command: NativeTextCommand) -> i32 {
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
        // The native text command ABI carries no language tag (yet); plugin
        // `Code` text renders plain.
        "",
        font_ctx,
        command.max_width as f32,
        &text_align,
        &overflow,
    ) {
        Ok(paths) => paths,
        Err(_) => return NATIVE_STATUS_TYPE_ERROR,
    };
    // Place the text at the command's local offset.
    let paths = crate::primitives::translate_text_paths(paths, command.x, command.y);
    host.commands.push(crate::primitives::RenderCommand::Text { paths });
    NATIVE_STATUS_OK
}

unsafe extern "C" fn native_append_image(host: *mut c_void, command: NativeImageCommand) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativePrimitiveEvaluateHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let requested_url = unsafe { read_c_string(command.url) };
    let image = match requested_url.as_deref() {
        Some(url) => host.ctx.asset_cache.get_image(url),
        None => host.ctx.track.image.get(host.ctx.time_ms, None),
    };
    let Some(image) = image else {
        // Explicit URLs must be resolved from the document asset cache; silently
        // falling back to the actor image would hide stale plugin behavior.
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let natural_size = if command.natural_size[0] > 0.0 && command.natural_size[1] > 0.0 {
        [
            command.natural_size[0] as f32,
            command.natural_size[1] as f32,
        ]
    } else {
        let half = host
            .ctx
            .track
            .geometry
            .size
            .get(host.ctx.time_ms, crate::timeline::DEFAULT_LAYOUT_HALF_SIZE);
        [half[0] * 2.0, half[1] * 2.0]
    };
    host.commands.push(crate::primitives::RenderCommand::Image {
        image,
        natural_size,
        offset: [command.x, command.y],
    });
    NATIVE_STATUS_OK
}

unsafe extern "C" fn native_append_highlight(
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

fn native_path_to_vello(command: &NativePathCommand) -> Option<crate::timeline::VelloPath> {
    let mut path = kurbo::BezPath::new();
    match command.kind {
        NATIVE_PATH_RECT => {
            let rect = kurbo::Rect::new(
                command.x,
                command.y,
                command.x + command.width,
                command.y + command.height,
            );
            path = rect.into_path(1e-3);
        },
        NATIVE_PATH_ELLIPSE => {
            let ellipse =
                kurbo::Ellipse::new((command.x, command.y), (command.width, command.height), 0.0);
            path = ellipse.into_path(1e-3);
        },
        NATIVE_PATH_LINE => {
            path.move_to((command.x1, command.y1));
            path.line_to((command.x2, command.y2));
        },
        NATIVE_PATH_POLYGON => {
            if command.points.is_null() || command.point_len == 0 {
                return None;
            }
            // The callback owns the points buffer for the duration of the call,
            // and this conversion copies them into a host-owned BezPath.
            let points = unsafe { std::slice::from_raw_parts(command.points, command.point_len) };
            path.move_to((points[0].x, points[0].y));
            for point in points.iter().skip(1) {
                path.line_to((point.x, point.y));
            }
            path.close_path();
        },
        NATIVE_PATH_CUBIC => {
            path.move_to((command.x, command.y));
            path.curve_to(
                (command.x1, command.y1),
                (command.x2, command.y2),
                (command.width, command.height),
            );
        },
        NATIVE_PATH_QUADRATIC => {
            path.move_to((command.x, command.y));
            path.quad_to((command.x1, command.y1), (command.x2, command.y2));
        },
        NATIVE_PATH_ARC => {
            let arc = kurbo::Arc::new(
                (command.x, command.y),
                (command.width, command.height),
                command.start_angle,
                command.sweep_angle,
                0.0,
            );
            path = arc.into_path(1e-3);
        },
        NATIVE_PATH_ROUNDED_RECT => {
            let rect = kurbo::RoundedRect::new(
                command.x,
                command.y,
                command.x + command.width,
                command.y + command.height,
                command.radius,
            );
            path = rect.into_path(1e-3);
        },
        _ => return None,
    }
    let fill = native_color(command.fill);
    let stroke = if command.stroke_width > 0.0 {
        Some((native_color(command.stroke), command.stroke_width as f32))
    } else {
        None
    };
    Some(crate::timeline::VelloPath {
        path: std::sync::Arc::new(path),
        fill: Some(fill),
        stroke,
        line_cap: command.line_cap,
        line_join: command.line_join,
        dash_pattern: None,
        dash_offset: 0.0,
        fill_gradient: None,
        stroke_gradient: None,
    })
}

fn native_color([r, g, b, a]: [f64; 4]) -> vello::peniko::Color {
    let clamp = |component: f64| (component.clamp(0.0, 1.0) * 255.0).round() as u8;
    vello::peniko::Color::from_rgba8(clamp(r), clamp(g), clamp(b), clamp(a))
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

struct NativeActionHost<'a> {
    timeline: &'a mut crate::timeline::Timeline,
    targets: Vec<*const c_char>,
    args: Vec<NativeValue>,
    modifiers: Vec<NativeModifierValue>,
    property_ids: &'a HashMap<String, animatix_syntax::schema::PropertyId>,
    property_kinds: &'a HashMap<String, animatix_syntax::schema::PropertyValueKind>,
    service_values: &'a HashMap<String, usize>,
}

unsafe extern "C" fn native_action_get_target_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return 0;
    };
    host.targets.len()
}

unsafe extern "C" fn native_action_get_target(
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

unsafe extern "C" fn native_action_get_arg_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return 0;
    };
    host.args.len()
}

unsafe extern "C" fn native_action_get_arg(
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

unsafe extern "C" fn native_action_get_modifier_count(host: *mut c_void) -> usize {
    let Some(host) = (unsafe { (host as *mut NativeActionHost).as_ref() }) else {
        return 0;
    };
    host.modifiers.len()
}

unsafe extern "C" fn native_action_get_modifier(
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

unsafe extern "C" fn native_action_write_keyframe(
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
    let Some(track) = host.timeline.tracks.get_mut(&target) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    // Native actions can only write extension properties today; a built-in
    // property is recognized but not writable through this host callback.
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
        track,
        *id,
        crate::timeline::PropertyKind::from(*kind),
        property_value,
        t_start_ms,
        t_end_ms,
        easing,
    );
    NATIVE_STATUS_OK
}

unsafe extern "C" fn native_action_get_service(
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

fn native_primitive_category(category: u32) -> Option<ActorCategory> {
    use animatix_plugin_api::{
        NATIVE_PRIMITIVE_CATEGORY_ANNOTATION, NATIVE_PRIMITIVE_CATEGORY_CONTAINER,
        NATIVE_PRIMITIVE_CATEGORY_MEDIA, NATIVE_PRIMITIVE_CATEGORY_PLOT,
        NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CATEGORY_TEXT,
    };
    match category {
        NATIVE_PRIMITIVE_CATEGORY_SHAPE => Some(ActorCategory::Shape),
        NATIVE_PRIMITIVE_CATEGORY_TEXT => Some(ActorCategory::Text),
        NATIVE_PRIMITIVE_CATEGORY_MEDIA => Some(ActorCategory::Media),
        NATIVE_PRIMITIVE_CATEGORY_PLOT => Some(ActorCategory::Plot),
        NATIVE_PRIMITIVE_CATEGORY_CONTAINER => Some(ActorCategory::Container),
        NATIVE_PRIMITIVE_CATEGORY_ANNOTATION => Some(ActorCategory::Annotation),
        _ => None,
    }
}

fn native_child_processing(kind: u32) -> Option<ChildProcessing> {
    use animatix_plugin_api::{
        NATIVE_PRIMITIVE_CHILD_EQUATION, NATIVE_PRIMITIVE_CHILD_FILTER,
        NATIVE_PRIMITIVE_CHILD_GENERIC, NATIVE_PRIMITIVE_CHILD_MASK,
    };
    match kind {
        NATIVE_PRIMITIVE_CHILD_GENERIC => Some(ChildProcessing::Generic),
        NATIVE_PRIMITIVE_CHILD_FILTER => Some(ChildProcessing::Filter),
        NATIVE_PRIMITIVE_CHILD_MASK => Some(ChildProcessing::Mask),
        NATIVE_PRIMITIVE_CHILD_EQUATION => Some(ChildProcessing::Equation),
        _ => None,
    }
}

fn native_capabilities(flags: u32) -> animatix_syntax::schema::PrimitiveCapabilities {
    animatix_syntax::schema::PrimitiveCapabilities {
        text_paths: flags & NATIVE_CAP_TEXT_PATHS != 0,
        vector_paths: flags & NATIVE_CAP_VECTOR_PATHS != 0,
        image_payload: flags & NATIVE_CAP_IMAGE_PAYLOAD != 0,
        layout_container: flags & NATIVE_CAP_LAYOUT_CONTAINER != 0,
        morphable_paths: flags & NATIVE_CAP_MORPHABLE_PATHS != 0,
        vector_reveal_target: flags & NATIVE_CAP_VECTOR_REVEAL_TARGET != 0,
        plot_geometry: flags & NATIVE_CAP_PLOT_GEOMETRY != 0,
        plot_host: flags & NATIVE_CAP_PLOT_HOST != 0,
        is_container: flags & NATIVE_CAP_IS_CONTAINER != 0,
        is_shape: flags & NATIVE_CAP_IS_SHAPE != 0,
    }
}

fn native_resize_mode(mode: u32) -> ResizeMode {
    if mode == NATIVE_RESIZE_MODE_SCALE {
        ResizeMode::Scale
    } else {
        ResizeMode::Size
    }
}

fn parse_native_type(ty: &str) -> Option<animatix_syntax::typing::Type> {
    // Strict annotation parsing: `None` for unknown/unsupported type strings.
    animatix_syntax::typing::Type::parse(ty)
}

unsafe fn read_c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(str::to_owned)
}

unsafe fn read_c_string_len(ptr: *const c_char, len: usize) -> Option<String> {
    if ptr.is_null() {
        return Some(String::new());
    }
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), len) };
    std::str::from_utf8(bytes).ok().map(str::to_owned)
}

fn native_property_kind(kind: u32) -> Option<animatix_syntax::schema::PropertyValueKind> {
    match kind {
        NATIVE_PROPERTY_F32 => Some(animatix_syntax::schema::PropertyValueKind::F32),
        NATIVE_PROPERTY_U32 => Some(animatix_syntax::schema::PropertyValueKind::U32),
        NATIVE_PROPERTY_BOOL => Some(animatix_syntax::schema::PropertyValueKind::Bool),
        NATIVE_PROPERTY_VEC2 => Some(animatix_syntax::schema::PropertyValueKind::Vec2),
        NATIVE_PROPERTY_VEC4 => Some(animatix_syntax::schema::PropertyValueKind::Vec4),
        NATIVE_PROPERTY_STRING => Some(animatix_syntax::schema::PropertyValueKind::String),
        NATIVE_PROPERTY_POINT_LIST => Some(animatix_syntax::schema::PropertyValueKind::PointList),
        NATIVE_PROPERTY_GENERIC | NATIVE_PROPERTY_ENUM => {
            Some(animatix_syntax::schema::PropertyValueKind::Generic)
        },
        _ => None,
    }
}

#[derive(Default)]
struct NativeValueArena {
    strings: Vec<Vec<u8>>,
    lists: Vec<Vec<NativeValue>>,
    values: Vec<Vec<NativeValue>>,
}

impl NativeValueArena {
    fn string(&mut self, text: &str) -> (*const c_char, usize) {
        let len = text.len();
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        let ptr = bytes.as_ptr().cast::<c_char>();
        self.strings.push(bytes);
        (ptr, len)
    }

    fn list(&mut self, values: Vec<NativeValue>) -> (*const NativeValue, usize) {
        let ptr = values.as_ptr();
        let len = values.len();
        self.lists.push(values);
        (ptr, len)
    }

    fn value(&mut self, value: NativeValue) -> *const NativeValue {
        self.values.push(vec![value]);
        self.values.last().expect("payload just pushed").as_ptr()
    }
}

fn native_string(tag: u32, text: &str, arena: &mut NativeValueArena) -> NativeValue {
    let (string, string_len) = arena.string(text);
    NativeValue {
        tag,
        string,
        string_len,
        ..NativeValue::default()
    }
}

fn value_to_native(value: &Value, arena: &mut NativeValueArena) -> Result<NativeValue, EvalError> {
    let mut native = NativeValue::default();
    match value {
        Value::Num(num) => {
            native.tag = NATIVE_VALUE_NUM;
            native.num = *num;
        },
        Value::Str(text) => return Ok(native_string(NATIVE_VALUE_STRING, text, arena)),
        Value::Bool(boolean) => {
            native.tag = NATIVE_VALUE_BOOL;
            native.boolean = *boolean;
        },
        Value::Vec2(vec) => {
            native.tag = NATIVE_VALUE_VEC2;
            native.vec = [vec[0], vec[1], 0.0, 0.0];
        },
        Value::Vec3(vec) => {
            native.tag = NATIVE_VALUE_VEC3;
            native.vec = [vec[0], vec[1], vec[2], 0.0];
        },
        Value::Vec4(vec) => {
            native.tag = NATIVE_VALUE_VEC4;
            native.vec = *vec;
        },
        Value::Color(color) => {
            native.tag = NATIVE_VALUE_COLOR;
            native.vec = *color;
        },
        Value::List(items) => {
            let values = items
                .iter()
                .map(|item| value_to_native(item, arena))
                .collect::<Result<Vec<_>, _>>()?;
            let (list, list_len) = arena.list(values);
            native.tag = NATIVE_VALUE_LIST;
            native.list = list;
            native.list_len = list_len;
        },
        other => {
            return Err(EvalError::TypeMismatch(format!(
                "native plugin functions do not support {:?}",
                other
            )));
        },
    }
    Ok(native)
}

fn property_value_to_native(value: &PropertyValue, arena: &mut NativeValueArena) -> NativeValue {
    match value {
        PropertyValue::F32(value) => NativeValue {
            tag: NATIVE_VALUE_NUM,
            num: *value as f64,
            ..NativeValue::default()
        },
        PropertyValue::Bool(value) => NativeValue {
            tag: NATIVE_VALUE_BOOL,
            boolean: *value,
            ..NativeValue::default()
        },
        PropertyValue::U32(value) => NativeValue {
            tag: NATIVE_VALUE_U32,
            num: *value as f64,
            ..NativeValue::default()
        },
        PropertyValue::Vec2(value) => NativeValue {
            tag: NATIVE_VALUE_VEC2,
            vec: [value[0] as f64, value[1] as f64, 0.0, 0.0],
            ..NativeValue::default()
        },
        PropertyValue::Vec4(value) => NativeValue {
            tag: NATIVE_VALUE_VEC4,
            vec: [
                value[0] as f64,
                value[1] as f64,
                value[2] as f64,
                value[3] as f64,
            ],
            ..NativeValue::default()
        },
        PropertyValue::Color(value) => NativeValue {
            tag: NATIVE_VALUE_COLOR,
            vec: [
                value[0] as f64,
                value[1] as f64,
                value[2] as f64,
                value[3] as f64,
            ],
            ..NativeValue::default()
        },
        PropertyValue::String(value) => native_string(NATIVE_VALUE_STRING, value, arena),
        PropertyValue::PointList(points) => {
            let values = points
                .iter()
                .map(|point| NativeValue {
                    tag: NATIVE_VALUE_VEC2,
                    vec: [point[0] as f64, point[1] as f64, 0.0, 0.0],
                    ..NativeValue::default()
                })
                .collect();
            let (list, list_len) = arena.list(values);
            NativeValue {
                tag: NATIVE_VALUE_POINT_LIST,
                list,
                list_len,
                ..NativeValue::default()
            }
        },
        PropertyValue::F32List(values) => {
            let natives = values
                .iter()
                .map(|value| NativeValue {
                    tag: NATIVE_VALUE_NUM,
                    num: f64::from(*value),
                    ..NativeValue::default()
                })
                .collect();
            let (list, list_len) = arena.list(natives);
            NativeValue {
                tag: NATIVE_VALUE_LIST,
                list,
                list_len,
                ..NativeValue::default()
            }
        },
        PropertyValue::CommandList(value) => native_string(NATIVE_VALUE_COMMAND_LIST, value, arena),
        PropertyValue::StringList(values) => {
            let natives = values
                .iter()
                .map(|value| native_string(NATIVE_VALUE_STRING, value, arena))
                .collect();
            let (list, list_len) = arena.list(natives);
            NativeValue {
                tag: NATIVE_VALUE_STRING_LIST,
                list,
                list_len,
                ..NativeValue::default()
            }
        },
        PropertyValue::Transform(value) => NativeValue {
            tag: NATIVE_VALUE_TRANSFORM,
            transform: [
                value[0] as f64,
                value[1] as f64,
                value[2] as f64,
                value[3] as f64,
                value[4] as f64,
                value[5] as f64,
            ],
            ..NativeValue::default()
        },
        PropertyValue::Gradient(_) => {
            // No native brush tag exists yet; a plugin that needs to read a
            // ramp requires an ABI addition (see docs/handoff_motion_vocab.md
            // item 1). Logged rather than silently dropped.
            tracing::debug!(
                "property_value_to_native: gradient paints are not exposed over the plugin ABI yet"
            );
            NativeValue::default()
        },
        PropertyValue::Enum(value) => native_string(NATIVE_VALUE_ENUM, value, arena),
        PropertyValue::Variant { name, value } => {
            let (variant, variant_len) = arena.string(name);
            let payload = property_value_to_native(value, arena);
            let payload = arena.value(payload);
            NativeValue {
                tag: NATIVE_VALUE_VARIANT,
                string: variant,
                string_len: variant_len,
                payload,
                ..NativeValue::default()
            }
        },
    }
}

fn read_native_string(native: &NativeValue) -> Result<String, EvalError> {
    if native.string.is_null() {
        return Ok(String::new());
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(native.string.cast::<u8>(), native.string_len) };
    String::from_utf8(bytes.to_vec()).map_err(|err| {
        EvalError::TypeMismatch(format!("native plugin returned invalid UTF-8 string: {err}"))
    })
}

fn read_native_list(native: &NativeValue) -> Result<Vec<Value>, EvalError> {
    if native.list.is_null() {
        return Ok(Vec::new());
    }
    let values = unsafe { std::slice::from_raw_parts(native.list, native.list_len) };
    values.iter().map(|value| native_to_value(*value)).collect()
}

fn native_to_value(native: NativeValue) -> Result<Value, EvalError> {
    match native.tag {
        NATIVE_VALUE_NUM => Ok(Value::Num(native.num)),
        NATIVE_VALUE_BOOL => Ok(Value::Bool(native.boolean)),
        NATIVE_VALUE_U32 => Ok(Value::Num(native.num)),
        NATIVE_VALUE_VEC2 => Ok(Value::Vec2([native.vec[0], native.vec[1]])),
        NATIVE_VALUE_VEC3 => Ok(Value::Vec3([native.vec[0], native.vec[1], native.vec[2]])),
        NATIVE_VALUE_VEC4 => Ok(Value::Vec4(native.vec)),
        NATIVE_VALUE_COLOR => Ok(Value::Color(native.vec)),
        NATIVE_VALUE_STRING | NATIVE_VALUE_COMMAND_LIST | NATIVE_VALUE_ENUM => {
            Ok(Value::Str(read_native_string(&native)?))
        },
        NATIVE_VALUE_LIST | NATIVE_VALUE_POINT_LIST | NATIVE_VALUE_STRING_LIST => {
            Ok(Value::List(read_native_list(&native)?.into()))
        },
        NATIVE_VALUE_TRANSFORM => Ok(Value::List(
            native
                .transform
                .iter()
                .map(|value| Value::Num(*value))
                .collect::<Vec<_>>()
                .into(),
        )),
        NATIVE_VALUE_VARIANT => Err(EvalError::TypeMismatch(
            "native plugin returned a variant value to an expression function".to_string(),
        )),
        tag => Err(EvalError::TypeMismatch(format!(
            "native plugin returned unknown value tag {tag}"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension_context::ExtensionContext;
    use crate::timeline::{Environment, PropertyKind, Value};
    use animatix_plugin_api::NATIVE_RESIZE_MODE_SIZE;
    use std::ffi::c_void;

    /// The ABI easing table must not be able to lie. Every code the host
    /// advertises decodes to a curve and re-encodes to that same code, codes it
    /// does not know are rejected instead of reading as `Linear`, and the two
    /// parameterized curves report `UNSUPPORTED` rather than borrowing a
    /// neighbour's name. Before the table was exhaustive, `bounce`/`elastic`/
    /// `back`/`expo*` all crossed the boundary as `Linear` in both directions.
    #[test]
    fn easing_codes_round_trip_and_reject_the_unknown() {
        use crate::easing::Easing;
        let table: &[(u32, Easing)] = &[
            (NATIVE_EASING_LINEAR, Easing::Linear),
            (NATIVE_EASING_IN, Easing::EaseIn),
            (NATIVE_EASING_OUT, Easing::EaseOut),
            (NATIVE_EASING_IN_OUT, Easing::EaseInOut),
            (NATIVE_EASING_BOUNCE_IN, Easing::Bounce),
            (NATIVE_EASING_ELASTIC, Easing::Elastic),
            (NATIVE_EASING_BACK, Easing::Back),
            (NATIVE_EASING_EXPO, Easing::Expo),
            (NATIVE_EASING_EXPO_OUT, Easing::ExpoOut),
            (NATIVE_EASING_EXPO_IN_OUT, Easing::ExpoInOut),
        ];
        for (code, easing) in table {
            assert_eq!(
                native_easing(*code),
                Some(*easing),
                "code {code} did not decode to {easing:?}"
            );
            assert_eq!(easing_code(*easing), *code, "{easing:?} did not encode back to {code}");
        }
        let mut codes: Vec<u32> = table.iter().map(|(c, _)| *c).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), table.len(), "two easings share one code");
        assert_eq!(
            codes.last().copied(),
            Some(animatix_plugin_api::NATIVE_EASING_MAX),
            "NATIVE_EASING_MAX no longer names the highest tabled code"
        );

        // Unknown in both directions must be loud, never linear.
        assert_eq!(native_easing(animatix_plugin_api::NATIVE_EASING_MAX + 1), None);
        assert_eq!(native_easing(NATIVE_EASING_UNSUPPORTED), None);
        assert_eq!(native_easing(4242), None);
        assert_eq!(
            easing_code(Easing::Spring {
                damping: 6.0,
                frequency: 9.0
            }),
            NATIVE_EASING_UNSUPPORTED
        );
        assert_eq!(
            easing_code(Easing::CubicBezier([0.1, 0.2, 0.3, 0.4])),
            NATIVE_EASING_UNSUPPORTED
        );
        assert_ne!(
            easing_code(Easing::Spring {
                damping: 6.0,
                frequency: 9.0
            }),
            NATIVE_EASING_LINEAR,
            "a parameterized curve was reported as linear"
        );
    }

    static BUILD_CHILD_COUNT: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

    unsafe extern "C" fn action_write_test(ctx: *mut NativeActionContext) -> i32 {
        let ctx = unsafe { &*ctx };
        if unsafe { (ctx.get_target_count)(ctx.host) } != 1 {
            return NATIVE_STATUS_TYPE_ERROR;
        }
        let mut target = std::ptr::null();
        if unsafe { (ctx.get_target)(ctx.host, 0, &mut target) } != NATIVE_STATUS_OK {
            return NATIVE_STATUS_TYPE_ERROR;
        }
        let value = NativeValue {
            tag: NATIVE_VALUE_NUM,
            num: 7.0,
            ..NativeValue::default()
        };
        unsafe { (ctx.write_keyframe)(ctx.host, target, c"glow".as_ptr(), value, 0, 1000, 0) }
    }

    unsafe extern "C" fn record_build_children(ctx: *mut NativePrimitiveBuildCtx) -> i32 {
        let ctx = unsafe { &*ctx };
        BUILD_CHILD_COUNT
            .store(unsafe { (ctx.get_child_count)(ctx.host) }, std::sync::atomic::Ordering::SeqCst);
        NATIVE_STATUS_OK
    }

    fn sample_evaluate_ctx<'a>(
        track: &'a crate::timeline::AnimationTrack,
        asset_cache: &'a crate::timeline::assets::AssetCache,
    ) -> EvaluateCtx<'a> {
        EvaluateCtx {
            track,
            time_ms: 0,
            local_transform: kurbo::Affine::IDENTITY,
            opacity: 1.0,
            scene_dimensions: crate::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
            background_color: [0.0; 4],
            overrides: None,
            vector_paths: &[],
            asset_cache,
            target_resolver: None,
        }
    }

    #[test]
    fn native_values_round_trip_finite_runtime_values() {
        let cases = [
            Value::Num(3.5),
            Value::Bool(true),
            Value::Vec2([1.0, 2.0]),
            Value::Vec3([1.0, 2.0, 3.0]),
            Value::Vec4([1.0, 2.0, 3.0, 4.0]),
            Value::Color([0.1, 0.2, 0.3, 0.4]),
            Value::Str("hello".to_string()),
            Value::List(vec![Value::Num(1.0), Value::Str("two".to_string())].into()),
        ];
        for value in cases {
            let mut arena = NativeValueArena::default();
            assert_eq!(
                native_to_value(value_to_native(&value, &mut arena).expect("convert"))
                    .expect("restore"),
                value
            );
        }
    }

    #[test]
    fn native_function_context_reads_env_and_service() {
        let mut env = Environment::new();
        env.set("answer", Value::Num(42.0));
        let service_values = HashMap::from([("demo.pulse".to_string(), 7usize)]);
        let mut host = NativeFunctionHost {
            env: &env,
            service_values: &service_values,
            arena: NativeValueArena::default(),
        };
        let mut out = NativeValue::default();
        assert_eq!(
            unsafe {
                native_function_get_env(
                    (&mut host as *mut NativeFunctionHost).cast::<c_void>(),
                    c"answer".as_ptr(),
                    &mut out,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(out.tag, NATIVE_VALUE_NUM);
        assert_eq!(out.num, 42.0);

        let mut service = 0;
        assert_eq!(
            unsafe {
                native_function_get_service(
                    (&mut host as *mut NativeFunctionHost).cast::<c_void>(),
                    c"demo.pulse".as_ptr(),
                    &mut service,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(service, 7);
    }

    #[test]
    fn native_action_context_writes_extension_keyframe() {
        use animatix_syntax::schema::PropertyValueKind;

        let mut ctx = ExtensionContext::new();
        let id = ctx
            .register_property("Pulse", "glow", PropertyValueKind::F32, true)
            .expect("register property");
        let mut timeline = crate::timeline::Timeline::new();
        let mut track = crate::timeline::AnimationTrack::placeholder("p".to_string());
        track.actor_type = "Pulse".to_string();
        timeline.tracks.insert("p".to_string(), track);

        let adapter = NativeActionAdapter {
            signature: ActionSignature {
                name: "pulse".to_string(),
                category: "Native".to_string(),
                description: "Test action".to_string(),
                params: vec![],
                modifiers: vec![],
            },
            callback: action_write_test,
            property_ids: HashMap::from([("glow".to_string(), id)]),
            property_kinds: HashMap::from([("glow".to_string(), PropertyValueKind::F32)]),
            service_values: HashMap::new(),
            _library: Arc::new(()) as Arc<dyn Any + Send + Sync>,
        };
        let action = crate::ast::Action {
            verb: "pulse".to_string(),
            targets: vec!["p".to_string()],
            args: vec![],
            modifiers: vec![],
            byte_span: None,
            target_index: vec![],
        };
        let mut diagnostics = Vec::new();
        adapter.execute(&action, 0.0, &mut timeline, &mut diagnostics);
        assert!(diagnostics.is_empty(), "unexpected diagnostics: {diagnostics:?}");
        assert_eq!(
            crate::timeline::property_engine::read_property_plan_slot(
                timeline.tracks.get("p").expect("track"),
                id,
                500,
            ),
            Some(crate::timeline::PropertyValue::F32(7.0))
        );
    }

    #[test]
    fn property_kind_codes_cover_dynamic_track_kinds() {
        for kind in [
            PropertyKind::F32,
            PropertyKind::U32,
            PropertyKind::Bool,
            PropertyKind::Vec2,
            PropertyKind::Vec4,
            PropertyKind::String,
            PropertyKind::PointList,
            PropertyKind::Generic,
        ] {
            assert!(native_property_kind(property_kind_code(kind)).is_some());
        }
        assert!(native_property_kind(99).is_none());
    }

    #[test]
    fn native_enum_type_and_property_kind_are_supported() {
        use animatix_plugin_api::NATIVE_PROPERTY_ENUM;
        use animatix_syntax::typing::Type;

        assert_eq!(
            parse_native_type("Enum(left, right, top)"),
            Some(Type::Enum(vec!["left".to_string(), "right".to_string(), "top".to_string()]))
        );
        assert_eq!(
            native_property_kind(NATIVE_PROPERTY_ENUM),
            Some(animatix_syntax::schema::PropertyValueKind::Generic)
        );
    }

    fn property_kind_code(kind: PropertyKind) -> u32 {
        match kind {
            PropertyKind::F32 => NATIVE_PROPERTY_F32,
            PropertyKind::U32 => NATIVE_PROPERTY_U32,
            PropertyKind::Bool => NATIVE_PROPERTY_BOOL,
            PropertyKind::Vec2 => NATIVE_PROPERTY_VEC2,
            PropertyKind::Vec4 => NATIVE_PROPERTY_VEC4,
            PropertyKind::String => NATIVE_PROPERTY_STRING,
            PropertyKind::PointList => NATIVE_PROPERTY_POINT_LIST,
            PropertyKind::Generic => NATIVE_PROPERTY_GENERIC,
        }
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

    unsafe extern "C" fn pulse_action(_ctx: *mut NativeActionContext) -> i32 {
        NATIVE_STATUS_OK
    }

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
            radius: 0.0,
            start_angle: 0.0,
            sweep_angle: 0.0,
            fill: [0.2, 0.8, 1.0, 1.0],
            stroke: [1.0, 1.0, 1.0, 1.0],
            stroke_width: 2.0,
            line_cap: 0,
            line_join: 0,
        };
        unsafe { (append_path)(ctx.host, command) }
    }

    unsafe extern "C" fn drop_service(_value: usize) {
        DROPPED_SERVICES.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }

    static DROPPED_SERVICES: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

    #[test]
    fn native_host_callbacks_install_and_dispose_capabilities() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
        };

        let mut ctx = ExtensionContext::new();
        let mut host = NativeHost {
            ctx: &mut ctx,
            library: Some(Arc::new(()) as Arc<dyn Any + Send + Sync>),
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
        let mut property_id = 0;
        assert_eq!(
            unsafe {
                native_register_property(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativePropertyDescriptor {
                        actor_type: c"Rect".as_ptr(),
                        name: c"glow".as_ptr(),
                        display_name: std::ptr::null(),
                        kind: NATIVE_PROPERTY_F32,
                        type_info: std::ptr::null(),
                        injectable: true,
                        group: std::ptr::null(),
                        help: std::ptr::null(),
                    },
                    &mut property_id,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_ne!(property_id, 0);
        assert_eq!(
            unsafe {
                native_register_function(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativeFunctionDescriptor {
                        name: c"double".as_ptr(),
                        params: std::ptr::null(),
                        param_len: 0,
                        return_type: std::ptr::null(),
                        help: std::ptr::null(),
                        callback: double,
                    },
                )
            },
            NATIVE_STATUS_OK
        );
        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            capabilities: 0,
            properties: std::ptr::null(),
            property_len: 0,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            resize_mode: NATIVE_RESIZE_MODE_SIZE,
            build: None,
            evaluate: Some(pulse_evaluate),
            handle_assignment: None,
            finalize_container_build: None,
            default_props: None,
            default_color_key: None,
            clip_path: None,
            equation_fragment: None,
        };
        assert_eq!(
            unsafe {
                native_register_primitive(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    primitive,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(
            unsafe {
                native_register_action(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativeAction {
                        name: c"pulse".as_ptr(),
                        category: c"Native".as_ptr(),
                        description: c"Demo native pulse action".as_ptr(),
                        params: std::ptr::null(),
                        param_len: 0,
                        modifiers: std::ptr::null(),
                        modifier_len: 0,
                        execute: pulse_action,
                    },
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(
            unsafe {
                native_provide_service(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativeService {
                        name: c"demo.pulse".as_ptr(),
                        type_info: std::ptr::null(),
                        help: std::ptr::null(),
                        value: 7,
                        drop: Some(drop_service),
                    },
                )
            },
            NATIVE_STATUS_OK
        );
        let NativeHost {
            ctx,
            properties,
            functions,
            primitives,
            actions,
            services,
            ..
        } = host;

        assert_eq!(properties, vec![("Rect".to_string(), "glow".to_string())]);
        assert_eq!(functions, vec!["double".to_string()]);
        assert_eq!(primitives, vec!["Pulse".to_string()]);
        assert_eq!(actions, vec!["pulse".to_string()]);
        assert_eq!(services, vec!["demo.pulse".to_string()]);

        let mut env = Environment::new();
        ctx.install_functions(&mut env);
        let Some(Value::NativeFn(call)) = env.get("double") else {
            panic!("double was not installed");
        };
        assert_eq!(
            call(&[Value::Num(21.0)], &Environment::new()).expect("call native function"),
            Value::Num(42.0)
        );
        DROPPED_SERVICES.store(0, std::sync::atomic::Ordering::SeqCst);
        assert!(ctx.primitive_registry().find("Pulse").is_some());
        assert!(ctx.action("pulse").is_some());
        assert!(ctx.get::<NativeServiceHandle>("demo.pulse").is_some());

        assert!(ctx.remove_property("Rect", "glow"));
        assert!(ctx.remove_function("double"));
        assert!(ctx.remove_primitive("Pulse"));
        assert!(ctx.remove_action("pulse"));
        assert!(ctx.remove_service("demo.pulse"));
        assert_eq!(DROPPED_SERVICES.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert!(ctx.primitive_registry().find("Pulse").is_none());
        assert!(ctx.action("pulse").is_none());
        assert!(ctx.get::<NativeServiceHandle>("demo.pulse").is_none());
    }

    #[test]
    fn native_host_rollback_removes_partial_registrations() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
        };

        let mut ctx = ExtensionContext::new();
        let mut host = NativeHost {
            ctx: &mut ctx,
            library: Some(Arc::new(()) as Arc<dyn Any + Send + Sync>),
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
        let mut property_id = 0;
        assert_eq!(
            unsafe {
                native_register_property(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativePropertyDescriptor {
                        actor_type: c"Pulse".as_ptr(),
                        name: c"glow".as_ptr(),
                        display_name: std::ptr::null(),
                        kind: NATIVE_PROPERTY_F32,
                        type_info: std::ptr::null(),
                        injectable: true,
                        group: std::ptr::null(),
                        help: std::ptr::null(),
                    },
                    &mut property_id,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(
            unsafe {
                native_register_function(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativeFunctionDescriptor {
                        name: c"double".as_ptr(),
                        params: std::ptr::null(),
                        param_len: 0,
                        return_type: std::ptr::null(),
                        help: std::ptr::null(),
                        callback: double,
                    },
                )
            },
            NATIVE_STATUS_OK
        );
        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            capabilities: 0,
            properties: std::ptr::null(),
            property_len: 0,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            resize_mode: NATIVE_RESIZE_MODE_SIZE,
            build: None,
            evaluate: None,
            handle_assignment: None,
            finalize_container_build: None,
            default_props: None,
            default_color_key: None,
            clip_path: None,
            equation_fragment: None,
        };
        assert_eq!(
            unsafe {
                native_register_primitive(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    primitive,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(
            unsafe {
                native_register_action(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativeAction {
                        name: c"pulse".as_ptr(),
                        category: c"Native".as_ptr(),
                        description: c"Demo native pulse action".as_ptr(),
                        params: std::ptr::null(),
                        param_len: 0,
                        modifiers: std::ptr::null(),
                        modifier_len: 0,
                        execute: pulse_action,
                    },
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(
            unsafe {
                native_provide_service(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativeService {
                        name: c"demo.pulse".as_ptr(),
                        type_info: std::ptr::null(),
                        help: std::ptr::null(),
                        value: 7,
                        drop: Some(drop_service),
                    },
                )
            },
            NATIVE_STATUS_OK
        );

        host.rollback();

        assert!(ctx.property_spec("Pulse", "glow").is_none());
        assert!(ctx.action("pulse").is_none());
        assert!(ctx.get::<NativeServiceHandle>("demo.pulse").is_none());
        assert!(ctx.primitive_registry().find("Pulse").is_none());
        let mut env = Environment::new();
        ctx.install_functions(&mut env);
        assert!(env.get("double").is_none());
    }

    #[test]
    fn native_register_property_rejects_same_name_across_actor_types() {
        use animatix_plugin_api::NATIVE_PROPERTY_F32;

        let mut ctx = ExtensionContext::new();
        let mut host = NativeHost {
            ctx: &mut ctx,
            library: Some(Arc::new(()) as Arc<dyn Any + Send + Sync>),
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
        let mut first_id = 0;
        assert_eq!(
            unsafe {
                native_register_property(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativePropertyDescriptor {
                        actor_type: c"Pulse".as_ptr(),
                        name: c"glow".as_ptr(),
                        display_name: std::ptr::null(),
                        kind: NATIVE_PROPERTY_F32,
                        type_info: std::ptr::null(),
                        injectable: true,
                        group: std::ptr::null(),
                        help: std::ptr::null(),
                    },
                    &mut first_id,
                )
            },
            NATIVE_STATUS_OK
        );
        // Reusing the same name on a different actor type must be rejected
        // (the internal name-keyed maps could not disambiguate it).
        let mut second_id = 0;
        assert_eq!(
            unsafe {
                native_register_property(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativePropertyDescriptor {
                        actor_type: c"Rect".as_ptr(),
                        name: c"glow".as_ptr(),
                        display_name: std::ptr::null(),
                        kind: NATIVE_PROPERTY_F32,
                        type_info: std::ptr::null(),
                        injectable: true,
                        group: std::ptr::null(),
                        help: std::ptr::null(),
                    },
                    &mut second_id,
                )
            },
            NATIVE_STATUS_TYPE_ERROR
        );
        assert_eq!(host.property_ids.len(), 1);
        assert_eq!(host.property_ids["glow"], animatix_syntax::schema::PropertyId(first_id));
    }

    #[test]
    fn native_path_commands_become_render_commands() {
        let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
        let property_ids = HashMap::new();
        let service_values = HashMap::new();
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
            service_values: &service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let command = NativePathCommand {
            kind: NATIVE_PATH_RECT,
            x: 10.0,
            y: 20.0,
            width: 100.0,
            height: 50.0,
            x1: 0.0,
            y1: 0.0,
            x2: 0.0,
            y2: 0.0,
            points: std::ptr::null(),
            point_len: 0,
            radius: 0.0,
            start_angle: 0.0,
            sweep_angle: 0.0,
            fill: [0.0, 0.0, 0.0, 1.0],
            stroke: [1.0, 1.0, 1.0, 1.0],
            stroke_width: 1.0,
            line_cap: 0,
            line_join: 0,
        };
        assert_eq!(
            unsafe {
                native_append_path(
                    (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                    command,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(host.commands.len(), 1);
    }

    #[test]
    fn native_append_text_produces_text_command() {
        use animatix_plugin_api::NATIVE_TEXT_KIND_TEXT;

        let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
        let property_ids = HashMap::new();
        let service_values = HashMap::new();
        let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
        let mut text_compiler = crate::renderer::text::TextCompiler::new();
        let text_ctx = crate::primitives::TextCompileCtx {
            text_compiler: &mut text_compiler,
            font_context: &font_context,
        };
        let (text_compiler, font_context) =
            (Some(&mut *text_ctx.text_compiler), Some(text_ctx.font_context));
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
            service_values: &service_values,
            text_compiler,
            font_context,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let command = NativeTextCommand {
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
            x: 0.0,
            y: 0.0,
        };
        assert_eq!(
            unsafe {
                native_append_text(
                    (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                    command,
                )
            },
            NATIVE_STATUS_OK
        );
        assert!(matches!(
            host.commands.as_slice(),
            [crate::primitives::RenderCommand::Text { .. }]
        ));
    }

    #[test]
    fn native_append_highlight_produces_highlight_command() {
        let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
        let property_ids = HashMap::new();
        let service_values = HashMap::new();
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
            service_values: &service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let command = NativeHighlightCommand {
            rect: [-10.0, -10.0, 10.0, 10.0],
            color: [0.0, 0.0, 1.0, 1.0],
            alpha: 0.5,
            corner_radius: 2.0,
            blend: 2,
        };
        assert_eq!(
            unsafe {
                native_append_highlight(
                    (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                    command,
                )
            },
            NATIVE_STATUS_OK
        );
        assert!(matches!(
            host.commands.as_slice(),
            [crate::primitives::RenderCommand::HighlightLayer { .. }]
        ));
    }

    #[test]
    fn native_append_image_uses_actor_image() {
        let mut track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        let image = crate::timeline::image::SceneImage {
            data: vello::peniko::ImageData {
                data: vello::peniko::Blob::from(vec![0u8, 0, 0, 255]),
                format: vello::peniko::ImageFormat::Rgba8,
                alpha_type: vello::peniko::ImageAlphaType::Alpha,
                width: 1,
                height: 1,
            },
            natural_size: [1.0, 1.0],
        };
        track
            .image
            .ensure(None)
            .add_keyframe(0, Some(image), crate::easing::Easing::Linear);
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
        let property_ids = HashMap::new();
        let service_values = HashMap::new();
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
            service_values: &service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let command = NativeImageCommand {
            url: std::ptr::null(),
            natural_size: [100.0, 50.0],
            x: 0.0,
            y: 0.0,
        };
        assert_eq!(
            unsafe {
                native_append_image(
                    (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                    command,
                )
            },
            NATIVE_STATUS_OK
        );
        assert!(matches!(
            host.commands.as_slice(),
            [crate::primitives::RenderCommand::Image { .. }]
        ));
    }

    #[test]
    fn native_append_image_rejects_uncached_explicit_url() {
        let mut track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        let image = crate::timeline::image::SceneImage {
            data: vello::peniko::ImageData {
                data: vello::peniko::Blob::from(vec![0u8, 0, 0, 255]),
                format: vello::peniko::ImageFormat::Rgba8,
                alpha_type: vello::peniko::ImageAlphaType::Alpha,
                width: 1,
                height: 1,
            },
            natural_size: [1.0, 1.0],
        };
        track
            .image
            .ensure(None)
            .add_keyframe(0, Some(image), crate::easing::Easing::Linear);
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
        let property_ids = HashMap::new();
        let service_values = HashMap::new();
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
            service_values: &service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let url = std::ffi::CString::new("missing.png").expect("cstring");
        let command = NativeImageCommand {
            url: url.as_ptr(),
            natural_size: [100.0, 50.0],
            x: 0.0,
            y: 0.0,
        };
        assert_eq!(
            unsafe {
                native_append_image(
                    (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                    command,
                )
            },
            NATIVE_STATUS_TYPE_ERROR
        );
        assert!(
            host.commands.is_empty(),
            "explicit uncached URL must not silently fall back to the actor image"
        );
    }

    #[test]
    fn native_append_image_resolves_cached_url() {
        let dir = std::env::temp_dir().join("animatix_native_image_url_tests");
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let path = dir.join(format!("native_image_url_{}.png", std::process::id()));
        let rgba = ::image::RgbaImage::from_raw(2, 2, vec![255; 16]).expect("rgba pixels");
        rgba.save(&path).expect("save image");

        let mut asset_cache = crate::timeline::assets::AssetCache::new();
        asset_cache.load_image_for(&path.to_string_lossy(), "p").expect("cache image");

        let track = crate::timeline::AnimationTrack::placeholder("p".to_string());
        let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
        let property_ids = HashMap::new();
        let service_values = HashMap::new();
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
            service_values: &service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let url = std::ffi::CString::new(path.to_string_lossy().into_owned()).expect("cstring");
        let command = NativeImageCommand {
            url: url.as_ptr(),
            natural_size: [40.0, 20.0],
            x: 0.0,
            y: 0.0,
        };
        assert_eq!(
            unsafe {
                native_append_image(
                    (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                    command,
                )
            },
            NATIVE_STATUS_OK
        );
        assert!(matches!(
            host.commands.as_slice(),
            [crate::primitives::RenderCommand::Image { natural_size, .. }]
                if *natural_size == [40.0, 20.0]
        ));
    }

    #[test]
    fn native_path_commands_support_curve_and_rounded_rect() {
        let cubic = NativePathCommand {
            kind: NATIVE_PATH_CUBIC,
            x: 0.0,
            y: 0.0,
            width: 10.0,
            height: 10.0,
            x1: 2.0,
            y1: 2.0,
            x2: 8.0,
            y2: 8.0,
            points: std::ptr::null(),
            point_len: 0,
            radius: 0.0,
            start_angle: 0.0,
            sweep_angle: 0.0,
            fill: [1.0, 1.0, 1.0, 1.0],
            stroke: [0.0; 4],
            stroke_width: 0.0,
            line_cap: 0,
            line_join: 0,
        };
        assert!(native_path_to_vello(&cubic).is_some());

        let rounded = NativePathCommand {
            kind: NATIVE_PATH_ROUNDED_RECT,
            x: 0.0,
            y: 0.0,
            width: 20.0,
            height: 10.0,
            x1: 0.0,
            y1: 0.0,
            x2: 0.0,
            y2: 0.0,
            points: std::ptr::null(),
            point_len: 0,
            radius: 3.0,
            start_angle: 0.0,
            sweep_angle: 0.0,
            fill: [1.0, 1.0, 1.0, 1.0],
            stroke: [0.0; 4],
            stroke_width: 0.0,
            line_cap: 0,
            line_join: 0,
        };
        assert!(native_path_to_vello(&rounded).is_some());
    }

    #[test]
    fn native_get_property_reads_extension_slot() {
        let mut ctx = ExtensionContext::new();
        let id = ctx
            .register_property(
                "Pulse",
                "glow",
                animatix_syntax::schema::PropertyValueKind::F32,
                true,
            )
            .expect("register property");
        let mut track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        crate::timeline::property_engine::write_property_plan_slot(
            &mut track,
            id,
            PropertyKind::F32,
            crate::timeline::PropertyValue::F32(0.75),
            0,
            0,
            crate::easing::Easing::Linear,
        );
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let eval_ctx = sample_evaluate_ctx(&track, &asset_cache);
        let property_ids = HashMap::from([("glow".to_string(), id)]);
        let service_values = HashMap::new();
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
            service_values: &service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let mut out = NativeValue::default();
        let status = unsafe {
            native_get_property(
                (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                c"glow".as_ptr(),
                &mut out,
            )
        };
        assert_eq!(status, NATIVE_STATUS_OK);
        assert_eq!(out.tag, NATIVE_VALUE_NUM);
        assert!((out.num - 0.75).abs() < 1e-9);
    }

    #[test]
    fn native_build_exposes_props_modifiers_and_children() {
        let mut timeline = crate::timeline::Timeline::new();
        let mut diagnostics = Vec::new();
        let props = vec![Property::new("level", Expr::Num(42.0))];
        let modifiers = vec![Modifier {
            name: Some("ease".to_string()),
            value: Expr::Ident("bounce".to_string()),
        }];
        let children = vec![InlineItem::Labeled {
            label: "kid".to_string(),
            array_index: None,
            ty: "Rect".to_string(),
            props: vec![Property::new(
                "size",
                Expr::Tuple(vec![Expr::Num(10.0), Expr::Num(10.0)]),
            )],
            modifiers: Vec::new(),
            children: Vec::new(),
        }];
        let mut host = NativePrimitiveBuildHost::new(
            &mut timeline,
            0.0,
            None,
            "pulse",
            &props,
            &modifiers,
            &children,
            &mut diagnostics,
        );
        assert_eq!(
            unsafe {
                native_build_get_property_count(
                    (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                )
            },
            1
        );
        let mut property = NativePropertyValue {
            name: std::ptr::null(),
            value: NativeValue::default(),
        };
        assert_eq!(
            unsafe {
                native_build_get_property(
                    (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                    0,
                    &mut property,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(unsafe { read_c_string(property.name) }.as_deref(), Some("level"));
        assert_eq!(property.value.tag, NATIVE_VALUE_NUM);

        assert_eq!(
            unsafe {
                native_build_get_modifier_count(
                    (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                )
            },
            1
        );
        let mut modifier = NativeModifierValue {
            name: std::ptr::null(),
            value: NativeValue::default(),
        };
        assert_eq!(
            unsafe {
                native_build_get_modifier(
                    (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                    0,
                    &mut modifier,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(unsafe { read_c_string(modifier.name) }.as_deref(), Some("ease"));

        assert_eq!(
            unsafe {
                native_build_get_child_count(
                    (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                )
            },
            1
        );
        let mut child = NativeChild {
            label: std::ptr::null(),
            type_name: std::ptr::null(),
            properties: std::ptr::null(),
            property_len: 0,
        };
        assert_eq!(
            unsafe {
                native_build_get_child(
                    (&mut host as *mut NativePrimitiveBuildHost).cast::<c_void>(),
                    0,
                    &mut child,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(unsafe { read_c_string(child.label) }.as_deref(), Some("kid"));
        assert_eq!(unsafe { read_c_string(child.type_name) }.as_deref(), Some("Rect"));
    }

    #[test]
    fn native_assignment_write_keyframe_writes_extension_slot() {
        let mut ctx = ExtensionContext::new();
        let id = ctx
            .register_property(
                "Pulse",
                "glow",
                animatix_syntax::schema::PropertyValueKind::F32,
                true,
            )
            .expect("register property");
        let property_ids = HashMap::from([("glow".to_string(), id)]);
        let property_kinds =
            HashMap::from([("glow".to_string(), animatix_syntax::schema::PropertyValueKind::F32)]);
        let mut track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        let mut host = NativeAssignmentHost {
            track: &mut track,
            property_ids: &property_ids,
            property_kinds: &property_kinds,
            value: NativeValue {
                tag: NATIVE_VALUE_NUM,
                num: 0.5,
                ..NativeValue::default()
            },
        };
        assert_eq!(
            unsafe {
                native_assignment_write_keyframe(
                    (&mut host as *mut NativeAssignmentHost).cast::<c_void>(),
                    c"glow".as_ptr(),
                    NativeValue {
                        tag: NATIVE_VALUE_NUM,
                        num: 0.75,
                        ..NativeValue::default()
                    },
                    0,
                    1000,
                    0,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(
            crate::timeline::property_engine::read_property_plan_slot(&track, id, 500),
            Some(crate::timeline::PropertyValue::F32(0.75))
        );
    }

    #[test]
    fn native_build_through_timeline_processes_children() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_CONTAINER, NATIVE_PRIMITIVE_CHILD_GENERIC,
        };

        let (ast, errors) = animatix_syntax::parser::parse_source("#0s\np: Pulse { kid: Rect }");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_CONTAINER,
            capabilities: NATIVE_CAP_IS_CONTAINER,
            properties: std::ptr::null(),
            property_len: 0,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            resize_mode: NATIVE_RESIZE_MODE_SIZE,
            build: Some(record_build_children),
            evaluate: None,
            handle_assignment: None,
            finalize_container_build: None,
            default_props: None,
            default_color_key: None,
            clip_path: None,
            equation_fragment: None,
        };
        let adapter = NativePrimitiveAdapter::new(
            primitive,
            Arc::new(()) as Arc<dyn Any + Send + Sync>,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        )
        .expect("adapter");
        let mut ctx = ExtensionContext::new();
        let adapter_info = animatix_std::PrimitiveInfo::extension(
            adapter.type_name.clone(),
            adapter.info.category,
        );
        ctx.register_primitive(Arc::new(adapter), adapter_info)
            .expect("register primitive");
        BUILD_CHILD_COUNT.store(0, std::sync::atomic::Ordering::SeqCst);

        let report = crate::timeline::Timeline::build_with_context(
            &ast,
            &std::collections::HashMap::new(),
            Arc::new(ctx),
        );
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let track = report.output.tracks.get("p").expect("pulse track");
        assert_eq!(track.children, vec!["kid".to_string()]);
        assert_eq!(BUILD_CHILD_COUNT.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn native_primitive_evaluate_emits_render_commands() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
        };

        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            capabilities: 0,
            properties: std::ptr::null(),
            property_len: 0,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            resize_mode: NATIVE_RESIZE_MODE_SIZE,
            build: None,
            evaluate: Some(pulse_evaluate),
            handle_assignment: None,
            finalize_container_build: None,
            default_props: None,
            default_color_key: None,
            clip_path: None,
            equation_fragment: None,
        };
        let adapter = NativePrimitiveAdapter::new(
            primitive,
            Arc::new(()) as Arc<dyn Any + Send + Sync>,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        )
        .expect("adapter");
        let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let ctx = sample_evaluate_ctx(&track, &asset_cache);
        let commands =
            adapter.evaluate(&ctx, None).expect("native evaluate").expect("native commands");
        assert!(matches!(commands.as_slice(), [crate::primitives::RenderCommand::Paths { .. }]));
    }

    #[test]
    fn native_primitive_clip_path_and_equation_fragment_flow_through_adapter() {
        use crate::primitives::Primitive;
        use animatix_plugin_api::{
            NATIVE_PATH_RECT, NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
            NativeClipPathCtx, NativePrimitiveEquationFragmentCtx,
        };

        fn rect_command(x: f64) -> NativePathCommand {
            NativePathCommand {
                kind: NATIVE_PATH_RECT,
                x,
                y: 0.0,
                width: 10.0,
                height: 10.0,
                x1: 0.0,
                y1: 0.0,
                x2: 0.0,
                y2: 0.0,
                points: std::ptr::null(),
                point_len: 0,
                radius: 0.0,
                start_angle: 0.0,
                sweep_angle: 0.0,
                fill: [1.0, 1.0, 1.0, 1.0],
                stroke: [0.0; 4],
                stroke_width: 0.0,
                line_cap: 0,
                line_join: 0,
            }
        }

        unsafe extern "C" fn pulse_clip_path(ctx: *mut NativeClipPathCtx) -> i32 {
            let Some(ctx) = (unsafe { ctx.as_mut() }) else {
                return NATIVE_STATUS_TYPE_ERROR;
            };
            let Some(append) = ctx.append_path else {
                return NATIVE_STATUS_TYPE_ERROR;
            };
            // Two disjoint rects: both must survive into the clip geometry.
            for x in [0.0, 40.0] {
                let status = unsafe { append(ctx.host, rect_command(x)) };
                if status != NATIVE_STATUS_OK {
                    return status;
                }
            }
            NATIVE_STATUS_OK
        }

        unsafe extern "C" fn pulse_equation_fragment(
            ctx: *mut NativePrimitiveEquationFragmentCtx,
        ) -> i32 {
            let Some(ctx) = (unsafe { ctx.as_mut() }) else {
                return NATIVE_STATUS_TYPE_ERROR;
            };
            let Some(write) = ctx.write_content else {
                return NATIVE_STATUS_TYPE_ERROR;
            };
            let content = c"x^2";
            let status = unsafe { write(ctx.host, content.as_ptr(), content.to_bytes().len()) };
            if status != NATIVE_STATUS_OK {
                return status;
            }
            ctx.highlight_color = [1.0, 0.0, 0.0, 1.0];
            ctx.highlight_opacity = 0.5;
            NATIVE_STATUS_OK
        }

        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            capabilities: 0,
            properties: std::ptr::null(),
            property_len: 0,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            resize_mode: NATIVE_RESIZE_MODE_SIZE,
            build: None,
            evaluate: None,
            handle_assignment: None,
            finalize_container_build: None,
            default_props: None,
            default_color_key: None,
            clip_path: Some(pulse_clip_path),
            equation_fragment: Some(pulse_equation_fragment),
        };
        let adapter = NativePrimitiveAdapter::new(
            primitive,
            Arc::new(()) as Arc<dyn Any + Send + Sync>,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        )
        .expect("adapter");

        let track = crate::timeline::AnimationTrack::placeholder("pulse".to_string());
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let ctx = sample_evaluate_ctx(&track, &asset_cache);

        let clip = adapter
            .clip_path(&ctx, &crate::timeline::ActorCaps::default())
            .expect("native clip path");
        // Two rects (5 path elements each) must both be present.
        assert!(
            clip.elements().len() >= 10,
            "clip must contain both rects, got {} elements",
            clip.elements().len()
        );

        let fragment = adapter.equation_fragment(&ctx).expect("native equation fragment");
        assert_eq!(fragment.content, "x^2");
        assert_eq!(fragment.highlight_color, [1.0, 0.0, 0.0, 1.0]);
        assert!((fragment.highlight_opacity - 0.5).abs() < 1e-6);
    }

    #[test]
    fn native_primitive_default_props_and_color_key_flow_through_adapter() {
        use crate::primitives::Primitive;
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC, NativeDefaultPropsCtx,
        };

        unsafe extern "C" fn pulse_default_props(ctx: *mut NativeDefaultPropsCtx) -> i32 {
            let Some(ctx) = (unsafe { ctx.as_mut() }) else {
                return NATIVE_STATUS_TYPE_ERROR;
            };
            let Some(append) = ctx.append_property else {
                return NATIVE_STATUS_TYPE_ERROR;
            };
            let mut arena = NativeValueArena::default();
            let color_name = std::ffi::CString::new("color").unwrap();
            let color =
                value_to_native(&crate::timeline::Value::Color([1.0, 0.5, 0.25, 1.0]), &mut arena)
                    .expect("color value");
            let status = unsafe { append(ctx.host, color_name.as_ptr(), color) };
            if status != NATIVE_STATUS_OK {
                return status;
            }
            let size_name = std::ffi::CString::new("size").unwrap();
            let size = value_to_native(&crate::timeline::Value::Vec2([120.0, 80.0]), &mut arena)
                .expect("size value");
            unsafe { append(ctx.host, size_name.as_ptr(), size) }
        }

        unsafe extern "C" fn pulse_default_color_key(
            property: *const c_char,
            out: *mut *const c_char,
        ) -> i32 {
            let Some(property) = (unsafe { read_c_string(property) }) else {
                return NATIVE_STATUS_TYPE_ERROR;
            };
            if property == "color" {
                // A `'static` pointer owned by the test binary — matches the
                // callback contract (plugin-lifetime-valid strings).
                unsafe { *out = c"accent.primary".as_ptr() };
                NATIVE_STATUS_OK
            } else {
                NATIVE_STATUS_UNSUPPORTED
            }
        }

        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            capabilities: 0,
            properties: std::ptr::null(),
            property_len: 0,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            resize_mode: NATIVE_RESIZE_MODE_SIZE,
            build: None,
            evaluate: None,
            handle_assignment: None,
            finalize_container_build: None,
            default_props: Some(pulse_default_props),
            default_color_key: Some(pulse_default_color_key),
            clip_path: None,
            equation_fragment: None,
        };
        let adapter = NativePrimitiveAdapter::new(
            primitive,
            Arc::new(()) as Arc<dyn Any + Send + Sync>,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        )
        .expect("adapter");

        let dims = crate::timeline::SceneDimensions {
            width: 640,
            height: 480,
        };
        let props = adapter.default_props(&dims);
        let color = props.iter().find(|p| p.name == "color").expect("color default");
        assert!(
            matches!(&color.value, crate::ast::Expr::Tuple(items) if items.len() == 4),
            "color default should be a 4-tuple"
        );
        let size = props.iter().find(|p| p.name == "size").expect("size default");
        assert!(
            matches!(&size.value, crate::ast::Expr::Tuple(items) if items.len() == 2),
            "size default should be a 2-tuple"
        );

        assert_eq!(adapter.default_color_key("color", &Default::default()), Some("accent.primary"));
        assert_eq!(adapter.default_color_key("stroke", &Default::default()), None);
    }

    #[test]
    fn native_primitive_metadata_maps_all_abi_codes() {
        use animatix_plugin_api::{
            NATIVE_CAP_IMAGE_PAYLOAD, NATIVE_CAP_IS_CONTAINER, NATIVE_CAP_IS_SHAPE,
            NATIVE_CAP_LAYOUT_CONTAINER, NATIVE_CAP_MORPHABLE_PATHS, NATIVE_CAP_PLOT_GEOMETRY,
            NATIVE_CAP_PLOT_HOST, NATIVE_CAP_TEXT_PATHS, NATIVE_CAP_VECTOR_PATHS,
            NATIVE_CAP_VECTOR_REVEAL_TARGET, NATIVE_PRIMITIVE_CATEGORY_ANNOTATION,
            NATIVE_PRIMITIVE_CATEGORY_CONTAINER, NATIVE_PRIMITIVE_CATEGORY_MEDIA,
            NATIVE_PRIMITIVE_CATEGORY_PLOT, NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            NATIVE_PRIMITIVE_CATEGORY_TEXT, NATIVE_PRIMITIVE_CHILD_EQUATION,
            NATIVE_PRIMITIVE_CHILD_FILTER, NATIVE_PRIMITIVE_CHILD_GENERIC,
            NATIVE_PRIMITIVE_CHILD_MASK, NATIVE_RESIZE_MODE_SCALE,
        };

        assert_eq!(
            native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_SHAPE),
            Some(ActorCategory::Shape)
        );
        assert_eq!(
            native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_TEXT),
            Some(ActorCategory::Text)
        );
        assert_eq!(
            native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_MEDIA),
            Some(ActorCategory::Media)
        );
        assert_eq!(
            native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_PLOT),
            Some(ActorCategory::Plot)
        );
        assert_eq!(
            native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_CONTAINER),
            Some(ActorCategory::Container)
        );
        assert_eq!(
            native_primitive_category(NATIVE_PRIMITIVE_CATEGORY_ANNOTATION),
            Some(ActorCategory::Annotation)
        );
        assert_eq!(native_primitive_category(99), None);

        assert_eq!(
            native_child_processing(NATIVE_PRIMITIVE_CHILD_GENERIC),
            Some(ChildProcessing::Generic)
        );
        assert_eq!(
            native_child_processing(NATIVE_PRIMITIVE_CHILD_FILTER),
            Some(ChildProcessing::Filter)
        );
        assert_eq!(
            native_child_processing(NATIVE_PRIMITIVE_CHILD_MASK),
            Some(ChildProcessing::Mask)
        );
        assert_eq!(
            native_child_processing(NATIVE_PRIMITIVE_CHILD_EQUATION),
            Some(ChildProcessing::Equation)
        );
        assert_eq!(native_child_processing(99), None);

        let capabilities = native_capabilities(
            NATIVE_CAP_TEXT_PATHS
                | NATIVE_CAP_VECTOR_PATHS
                | NATIVE_CAP_IMAGE_PAYLOAD
                | NATIVE_CAP_LAYOUT_CONTAINER
                | NATIVE_CAP_MORPHABLE_PATHS
                | NATIVE_CAP_VECTOR_REVEAL_TARGET
                | NATIVE_CAP_PLOT_GEOMETRY
                | NATIVE_CAP_PLOT_HOST
                | NATIVE_CAP_IS_CONTAINER
                | NATIVE_CAP_IS_SHAPE,
        );
        assert!(capabilities.text_paths);
        assert!(capabilities.vector_paths);
        assert!(capabilities.image_payload);
        assert!(capabilities.layout_container);
        assert!(capabilities.morphable_paths);
        assert!(capabilities.vector_reveal_target);
        assert!(capabilities.plot_geometry);
        assert!(capabilities.plot_host);
        assert!(capabilities.is_container);
        assert!(capabilities.is_shape);
        assert!(!native_capabilities(0).is_container);
        assert!(!native_capabilities(0).is_shape);

        assert_eq!(native_resize_mode(NATIVE_RESIZE_MODE_SCALE), ResizeMode::Scale);
        assert_eq!(native_resize_mode(NATIVE_RESIZE_MODE_SIZE), ResizeMode::Size);
        assert_eq!(native_resize_mode(99), ResizeMode::Size);
    }

    #[test]
    fn native_declared_builtin_properties_use_generic_writer() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
        };

        let declared = [c"position".as_ptr(), c"color".as_ptr()];
        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            capabilities: 0,
            properties: declared.as_ptr(),
            property_len: declared.len(),
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            resize_mode: NATIVE_RESIZE_MODE_SIZE,
            build: None,
            evaluate: None,
            handle_assignment: None,
            finalize_container_build: None,
            default_props: None,
            default_color_key: None,
            clip_path: None,
            equation_fragment: None,
        };
        let adapter = NativePrimitiveAdapter::new(
            primitive,
            Arc::new(()) as Arc<dyn Any + Send + Sync>,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        )
        .expect("adapter");
        let mut ctx = ExtensionContext::new();
        let adapter_info = animatix_std::PrimitiveInfo::extension(
            adapter.type_name.clone(),
            adapter.info.category,
        );
        ctx.register_primitive(Arc::new(adapter), adapter_info)
            .expect("register primitive");

        let (ast, errors) =
            animatix_syntax::parser::parse_source("p: Pulse, position: (10, 20), color: red");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");
        let report = crate::timeline::Timeline::build_with_context(
            &ast,
            &std::collections::HashMap::new(),
            Arc::new(ctx),
        );
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let track = report.output.tracks.get("p").expect("pulse track");
        assert_eq!(
            track.geometry.position.get(0, [0.0, 0.0]),
            [10.0, 20.0],
            "declared position should be written by the generic property engine"
        );
        assert_eq!(
            track.style.color.get(0, [1.0, 1.0, 1.0, 1.0]),
            [1.0, 0.0, 0.0, 1.0],
            "declared color should be written by the generic property engine"
        );
    }

    #[test]
    fn extension_actor_is_visible_from_declaration_time() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
        };

        // Regression: the extension build path must set `first_seen_ms` like
        // the built-in path, otherwise render_actor_node skips the actor
        // forever (`time_ms < u64::MAX` is always true).
        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            capabilities: 0,
            properties: std::ptr::null(),
            property_len: 0,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            resize_mode: NATIVE_RESIZE_MODE_SIZE,
            build: None,
            evaluate: None,
            handle_assignment: None,
            finalize_container_build: None,
            default_props: None,
            default_color_key: None,
            clip_path: None,
            equation_fragment: None,
        };
        let adapter = NativePrimitiveAdapter::new(
            primitive,
            Arc::new(()) as Arc<dyn Any + Send + Sync>,
            HashMap::new(),
            HashMap::new(),
            HashMap::new(),
        )
        .expect("adapter");
        let mut ctx = ExtensionContext::new();
        let adapter_info = animatix_std::PrimitiveInfo::extension(
            adapter.type_name.clone(),
            adapter.info.category,
        );
        ctx.register_primitive(Arc::new(adapter), adapter_info)
            .expect("register primitive");

        let (ast, errors) = animatix_syntax::parser::parse_source("p: Pulse, opacity: 0.25");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");
        let report = crate::timeline::Timeline::build_with_context(
            &ast,
            &std::collections::HashMap::new(),
            Arc::new(ctx),
        );
        assert!(
            report.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            report.diagnostics
        );
        let track = report.output.tracks.get("p").expect("pulse track");
        assert_eq!(
            track.first_seen_ms, 0,
            "extension actors must be visible from their declaration time"
        );
    }

    #[test]
    fn enum_extension_property_bare_identifier_roundtrips() {
        use animatix_syntax::schema::{PropertyId, PropertyValueKind};
        use animatix_syntax::typing::Type;

        let (ast, errors) =
            animatix_syntax::parser::parse_source("p: Rect, mode: ring\n#1s\n p.mode = dot");
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("parsed AST");

        let mut ctx = ExtensionContext::new();
        let id = ctx
            .register_property_full_typed(
                "Rect",
                "mode",
                PropertyValueKind::Generic,
                Some(Type::Enum(vec!["ring".to_string(), "dot".to_string(), "cross".to_string()])),
                true,
                None,
                None,
                None,
            )
            .expect("register property");

        let report = crate::timeline::Timeline::build_with_context(
            &ast,
            &std::collections::HashMap::new(),
            Arc::new(ctx),
        );
        let track = report.output.tracks.get("p").expect("pulse track");
        let slot = track.property_plan.get(PropertyId(1_000_000));
        let t0 = slot.and_then(|s| s.track.sample(0));
        let t1500 = slot.and_then(|s| s.track.sample(1500));
        assert_eq!(t0, Some(crate::timeline::PropertyValue::Enum("ring".to_string())));
        assert_eq!(t1500, Some(crate::timeline::PropertyValue::Enum("dot".to_string())));

        // Read back through the native path -> must be NATIVE_VALUE_ENUM.
        let property_ids = HashMap::from([("mode".to_string(), id)]);
        let service_values = HashMap::new();
        let asset_cache = crate::timeline::assets::AssetCache::new();
        let eval_ctx = sample_evaluate_ctx(track, &asset_cache);
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
            service_values: &service_values,
            text_compiler: None,
            font_context: None,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let mut out = NativeValue::default();
        let status = unsafe {
            native_get_property(
                (&mut host as *mut NativePrimitiveEvaluateHost).cast::<c_void>(),
                c"mode".as_ptr(),
                &mut out,
            )
        };
        assert_eq!(status, NATIVE_STATUS_OK);
        assert_eq!(out.tag, NATIVE_VALUE_ENUM);
        assert_eq!(
            unsafe { read_c_string_len(out.string, out.string_len) }.as_deref(),
            Some("ring")
        );
    }

    #[test]
    fn native_register_effect_registers_and_rejects_bad_layout() {
        use animatix_plugin_api::{
            NATIVE_EFFECT_PARAM_KIND_F32, NATIVE_EFFECT_PARAM_KIND_VEC2, NativeEffectDescriptor,
            NativeEffectParam, NativeEffectPass,
        };

        let mut ctx = ExtensionContext::new();
        let mut host = NativeHost {
            ctx: &mut ctx,
            library: Some(Arc::new(()) as Arc<dyn Any + Send + Sync>),
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

        let params = [
            NativeEffectParam {
                name: c"amount".as_ptr(),
                kind: NATIVE_EFFECT_PARAM_KIND_F32,
                offset: 0,
                identity: [0.0; 4],
            },
            NativeEffectParam {
                name: c"centre".as_ptr(),
                kind: NATIVE_EFFECT_PARAM_KIND_VEC2,
                offset: 8,
                identity: [0.5; 4],
            },
        ];
        let passes = [NativeEffectPass {
            wgsl: c"@compute fn main() {}".as_ptr(),
            entry: c"main".as_ptr(),
        }];
        let descriptor = NativeEffectDescriptor {
            name: c"MockGlow".as_ptr(),
            display_name: c"Mock Glow".as_ptr(),
            params: params.as_ptr(),
            param_len: params.len(),
            passes: passes.as_ptr(),
            pass_len: passes.len(),
            support_px: 3.0,
        };

        let status = unsafe {
            native_register_effect((&mut host as *mut NativeHost).cast::<c_void>(), descriptor)
        };
        assert_eq!(status, NATIVE_STATUS_OK);
        assert_eq!(host.effects, vec!["MockGlow".to_string()]);

        let registered = crate::timeline::effects::effect_for_type("MockGlow")
            .expect("plugin effect registered");
        assert_eq!(registered.type_name(), "MockGlow");
        assert_eq!(registered.display_name(), "Mock Glow");
        assert_eq!(registered.author_uniform_size(), 16);
        assert_eq!(registered.params().len(), 2);
        assert_eq!(registered.passes().len(), 1);

        // A misaligned parameter offset is rejected.
        let bad_params = [NativeEffectParam {
            name: c"offset".as_ptr(),
            kind: NATIVE_EFFECT_PARAM_KIND_VEC2,
            offset: 3,
            identity: [0.0; 4],
        }];
        let bad = NativeEffectDescriptor {
            name: c"BadGlow".as_ptr(),
            display_name: c"Bad Glow".as_ptr(),
            params: bad_params.as_ptr(),
            param_len: bad_params.len(),
            passes: passes.as_ptr(),
            pass_len: passes.len(),
            support_px: 0.0,
        };
        let status =
            unsafe { native_register_effect((&mut host as *mut NativeHost).cast::<c_void>(), bad) };
        assert_eq!(status, NATIVE_STATUS_TYPE_ERROR);

        // Re-registering the same name is idempotent: same slot, no duplicate.
        let again = unsafe {
            native_register_effect((&mut host as *mut NativeHost).cast::<c_void>(), descriptor)
        };
        assert_eq!(again, NATIVE_STATUS_OK);
        assert_eq!(host.effects.len(), 1);
    }
}
