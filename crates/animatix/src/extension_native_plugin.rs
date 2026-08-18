//! Native cdylib plugin loading behind the `plugin-loading` feature.
//!
//! Plugins communicate through the stable ABI in `animatix-plugin-api`, so
//! their Rust version and internal types can differ from the host. The host
//! keeps the loaded `Library` alive for the lifetime of registered callbacks.

use std::any::Any;
use std::ffi::{CStr, c_char, c_void};
use std::path::Path;
use std::sync::Arc;

use animatix_plugin_api::{
    ABI_VERSION, NATIVE_PATH_ELLIPSE, NATIVE_PATH_LINE, NATIVE_PATH_POLYGON, NATIVE_PATH_RECT,
    NATIVE_PROPERTY_F32, NATIVE_PROPERTY_GENERIC, NATIVE_PROPERTY_POINT_LIST,
    NATIVE_PROPERTY_STRING, NATIVE_PROPERTY_U32, NATIVE_PROPERTY_VEC2, NATIVE_PROPERTY_VEC4,
    NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_STATUS_UNSUPPORTED, NATIVE_VALUE_BOOL,
    NATIVE_VALUE_COLOR, NATIVE_VALUE_NUM, NATIVE_VALUE_VEC2, NATIVE_VALUE_VEC3, NATIVE_VALUE_VEC4,
    NativeActionExecuteV2, NativeFunctionV1, NativeInstallFn, NativeInstallFnV2,
    NativePathCommandV2, NativePluginApiV1, NativePluginApiV2, NativePrimitiveEvaluateCtxV2,
    NativePrimitiveEvaluateV2, NativePrimitiveV2, NativeServiceV2, NativeValueV1,
};
use kurbo::Shape;
use libloading::Library;

use crate::extension_context::ExtensionContext;
use crate::primitives::{BuildCtx, ChildProcessing, EvaluateCtx, Primitive};
use crate::timeline::actions::registry::{ActionSignature, BuiltinAction};
use crate::timeline::{ActorCategory, ActorKindId, Environment, EvalError, Value};

use super::{ExtensionPlugin, PluginDisposer, PluginError};

/// A native plugin loaded from a `cdylib` shared library.
pub struct NativePlugin {
    name: String,
    library: Arc<Library>,
    api_v1: NativePluginApiV1,
    install_v1: Option<NativeInstallFn>,
    api_v2: NativePluginApiV2,
    install_v2: Option<NativeInstallFnV2>,
}

impl NativePlugin {
    /// Load a native plugin and verify its ABI version.
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
            if version() != ABI_VERSION {
                return Err(PluginError(format!(
                    "'{}' uses ABI version {}, host expects {}",
                    path.display(),
                    version(),
                    ABI_VERSION
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

            let install_v1 = library
                .get::<NativeInstallFn>(b"animatix_plugin_install_v1")
                .ok()
                .map(|symbol| *symbol);
            let install_v2 = library
                .get::<NativeInstallFnV2>(b"animatix_plugin_install_v2")
                .ok()
                .map(|symbol| *symbol);
            if install_v1.is_none() && install_v2.is_none() {
                return Err(PluginError(format!(
                    "'{}' has no animatix_plugin_install_v1 or _v2 symbol",
                    path.display()
                )));
            }

            let api_v1 = NativePluginApiV1 {
                size: std::mem::size_of::<NativePluginApiV1>(),
                register_property: native_register_property,
                register_function: native_register_function,
            };
            let api_v2 = NativePluginApiV2 {
                size: std::mem::size_of::<NativePluginApiV2>(),
                register_property: native_register_property,
                register_function: native_register_function,
                register_primitive: native_register_primitive,
                register_action: native_register_action,
                provide_service: native_provide_service,
            };

            Ok(Self {
                name,
                library: Arc::new(library),
                api_v1,
                install_v1,
                api_v2,
                install_v2,
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
                functions: Vec::new(),
                primitives: Vec::new(),
                actions: Vec::new(),
                services: Vec::new(),
            };
            let status = if let Some(install_v2) = self.install_v2 {
                unsafe { (install_v2)(&self.api_v2, (&mut host as *mut NativeHost).cast()) }
            } else {
                let install_v1 = self.install_v1.expect("v1 install checked during load");
                unsafe { (install_v1)(&self.api_v1, (&mut host as *mut NativeHost).cast()) }
            };
            if status != NATIVE_STATUS_OK {
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
    functions: Vec<String>,
    primitives: Vec<String>,
    actions: Vec<String>,
    services: Vec<String>,
}

unsafe extern "C" fn native_register_property(
    host: *mut c_void,
    actor_type: *const c_char,
    name: *const c_char,
    kind: u32,
    injectable: bool,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(actor_type) = (unsafe { read_c_string(actor_type) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(kind) = native_property_kind(kind) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if host
        .ctx
        .register_property(actor_type.clone(), name.clone(), kind, injectable)
        .is_err()
    {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    host.properties.push((actor_type, name));
    NATIVE_STATUS_OK
}

unsafe extern "C" fn native_register_function(
    host: *mut c_void,
    name: *const c_char,
    callback: NativeFunctionV1,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let library = host.library.clone();
    let function_name = name.clone();
    host.ctx.register_function(name.clone(), move |args, env| unsafe {
        let _keep_alive = &library;
        let native_args = args.iter().map(value_to_native).collect::<Result<Vec<_>, _>>()?;
        let mut out = NativeValueV1::default();
        let status = callback(
            if native_args.is_empty() {
                std::ptr::null()
            } else {
                native_args.as_ptr()
            },
            native_args.len(),
            env as *const Environment as *const c_void,
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
    });
    host.functions.push(name);
    NATIVE_STATUS_OK
}

unsafe extern "C" fn native_register_primitive(
    host: *mut c_void,
    primitive: NativePrimitiveV2,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(library) = host.library.clone() else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(adapter) = NativePrimitiveAdapter::new(primitive, library) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let name = adapter.type_name.to_string();
    if host.ctx.register_primitive(std::sync::Arc::new(adapter)).is_err() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    host.primitives.push(name);
    NATIVE_STATUS_OK
}

unsafe extern "C" fn native_register_action(
    host: *mut c_void,
    name: *const c_char,
    callback: NativeActionExecuteV2,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(library) = host.library.clone() else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    host.ctx.register_action(Box::new(NativeActionAdapter {
        name: name.clone(),
        callback,
        _library: library,
    }));
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

unsafe extern "C" fn native_provide_service(host: *mut c_void, service: NativeServiceV2) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(service.name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(library) = host.library.clone() else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    host.ctx.provide(
        name.clone(),
        NativeServiceHandle {
            value: service.value,
            drop: service.drop,
            _library: library,
        },
    );
    host.services.push(name);
    NATIVE_STATUS_OK
}

/// Host-side adapter that turns a native primitive descriptor into a runtime
/// [`Primitive`].
struct NativePrimitiveAdapter {
    type_name: String,
    display_name: String,
    icon_id: String,
    category: ActorCategory,
    kind: ActorKindId,
    advanced: bool,
    child_processing: ChildProcessing,
    evaluate: Option<NativePrimitiveEvaluateV2>,
    _library: Arc<dyn Any + Send + Sync>,
}

impl NativePrimitiveAdapter {
    fn new(primitive: NativePrimitiveV2, library: Arc<dyn Any + Send + Sync>) -> Option<Self> {
        let type_name = unsafe { read_c_string(primitive.type_name)? };
        let display_name =
            unsafe { read_c_string(primitive.display_name) }.unwrap_or_else(|| type_name.clone());
        let icon_id =
            unsafe { read_c_string(primitive.icon_id) }.unwrap_or_else(|| "extension".to_string());
        let category = native_primitive_category(primitive.category)?;
        let kind = native_kind(category);
        Some(Self {
            type_name,
            display_name,
            icon_id,
            category,
            kind,
            advanced: primitive.advanced,
            child_processing: native_child_processing(primitive.child_processing)
                .unwrap_or_default(),
            evaluate: primitive.evaluate,
            _library: library,
        })
    }
}

impl Primitive for NativePrimitiveAdapter {
    fn type_name(&self) -> &str {
        &self.type_name
    }

    fn display_name(&self) -> &str {
        &self.display_name
    }

    fn category(&self) -> ActorCategory {
        self.category
    }

    fn icon_id(&self) -> &str {
        &self.icon_id
    }

    fn is_advanced(&self) -> bool {
        self.advanced
    }

    fn is_container(&self) -> bool {
        matches!(self.category, ActorCategory::Container)
    }

    fn child_processing(&self) -> ChildProcessing {
        self.child_processing
    }

    fn kind_id(&self) -> ActorKindId {
        self.kind
    }

    fn build(
        &self,
        ctx: &mut BuildCtx,
        label: &str,
        _props: &[crate::ast::Property],
        _modifiers: &[crate::ast::Modifier],
        _children: &[crate::ast::InlineItem],
    ) -> Result<(), Vec<crate::diagnostics::Diagnostic>> {
        let track = ctx
            .timeline
            .tracks
            .entry(label.to_string())
            .or_insert_with(|| crate::timeline::AnimationTrack::new(label.to_string()));
        track.kind = self.kind;
        track.actor_type = Some(self.type_name.clone());
        track.rebuild_property_plan();
        Ok(())
    }

    fn evaluate(
        &self,
        ctx: &EvaluateCtx,
        _text_ctx: Option<&mut crate::primitives::TextCompileCtx>,
    ) -> Result<Option<Vec<crate::primitives::RenderCommand>>, crate::renderer::error::RenderError>
    {
        let Some(evaluate) = self.evaluate else {
            return Ok(None);
        };
        let mut host = NativePrimitiveEvaluateHost {
            commands: Vec::new(),
        };
        let mut native_ctx = NativePrimitiveEvaluateCtxV2 {
            size: std::mem::size_of::<NativePrimitiveEvaluateCtxV2>(),
            time_ms: ctx.time_ms as f64,
            host: (&mut host as *mut NativePrimitiveEvaluateHost).cast(),
            append_path: Some(native_append_path),
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
}

struct NativePrimitiveEvaluateHost {
    commands: Vec<crate::primitives::RenderCommand>,
}

unsafe extern "C" fn native_append_path(host: *mut c_void, command: NativePathCommandV2) -> i32 {
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

fn native_path_to_vello(command: &NativePathCommandV2) -> Option<crate::timeline::VelloPath> {
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
        _ => return None,
    }
    let fill = native_color(command.fill);
    let stroke = if command.stroke_width > 0.0 {
        Some((native_color(command.stroke), command.stroke_width as f32))
    } else {
        None
    };
    Some(crate::timeline::VelloPath {
        path,
        fill: Some(fill),
        stroke,
        line_cap: command.line_cap,
        line_join: command.line_join,
    })
}

fn native_color([r, g, b, a]: [f64; 4]) -> vello::peniko::Color {
    let clamp = |component: f64| (component.clamp(0.0, 1.0) * 255.0).round() as u8;
    vello::peniko::Color::from_rgba8(clamp(r), clamp(g), clamp(b), clamp(a))
}

/// Host-side adapter that turns a native action callback into a runtime action.
struct NativeActionAdapter {
    name: String,
    callback: NativeActionExecuteV2,
    _library: Arc<dyn Any + Send + Sync>,
}

impl BuiltinAction for NativeActionAdapter {
    fn signature(&self) -> ActionSignature {
        ActionSignature {
            name: self.name.clone(),
            category: "Native".to_string(),
            description: "Native plugin action".to_string(),
            params: vec![],
            modifiers: vec![],
        }
    }

    fn execute(
        &self,
        _action: &crate::ast::Action,
        _time_ms: f64,
        _timeline: &mut crate::timeline::Timeline,
        _diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    ) {
        let _status = unsafe { (self.callback)(std::ptr::null_mut()) };
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

fn native_kind(category: ActorCategory) -> ActorKindId {
    use crate::timeline::ShapeKind;
    match category {
        ActorCategory::Shape => ActorKindId::Shape(ShapeKind::Rect),
        ActorCategory::Text => ActorKindId::Text,
        ActorCategory::Media => ActorKindId::Image,
        ActorCategory::Plot => ActorKindId::PlotCurve,
        ActorCategory::Container => ActorKindId::Group,
        ActorCategory::Annotation => ActorKindId::Callout,
    }
}

unsafe fn read_c_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }.to_str().ok().map(str::to_owned)
}

fn native_property_kind(kind: u32) -> Option<animatix_syntax::schema::PropertyValueKind> {
    match kind {
        NATIVE_PROPERTY_F32 => Some(animatix_syntax::schema::PropertyValueKind::F32),
        NATIVE_PROPERTY_U32 => Some(animatix_syntax::schema::PropertyValueKind::U32),
        NATIVE_PROPERTY_VEC2 => Some(animatix_syntax::schema::PropertyValueKind::Vec2),
        NATIVE_PROPERTY_VEC4 => Some(animatix_syntax::schema::PropertyValueKind::Vec4),
        NATIVE_PROPERTY_STRING => Some(animatix_syntax::schema::PropertyValueKind::String),
        NATIVE_PROPERTY_POINT_LIST => Some(animatix_syntax::schema::PropertyValueKind::PointList),
        NATIVE_PROPERTY_GENERIC => Some(animatix_syntax::schema::PropertyValueKind::Generic),
        _ => None,
    }
}

fn value_to_native(value: &Value) -> Result<NativeValueV1, EvalError> {
    let mut native = NativeValueV1::default();
    match value {
        Value::Num(num) => {
            native.tag = NATIVE_VALUE_NUM;
            native.num = *num;
        },
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
        other => {
            return Err(EvalError::TypeMismatch(format!(
                "native plugin functions do not support {:?}",
                other
            )));
        },
    }
    Ok(native)
}

fn native_to_value(native: NativeValueV1) -> Result<Value, EvalError> {
    match native.tag {
        NATIVE_VALUE_NUM => Ok(Value::Num(native.num)),
        NATIVE_VALUE_BOOL => Ok(Value::Bool(native.boolean)),
        NATIVE_VALUE_VEC2 => Ok(Value::Vec2([native.vec[0], native.vec[1]])),
        NATIVE_VALUE_VEC3 => Ok(Value::Vec3([native.vec[0], native.vec[1], native.vec[2]])),
        NATIVE_VALUE_VEC4 => Ok(Value::Vec4(native.vec)),
        NATIVE_VALUE_COLOR => Ok(Value::Color(native.vec)),
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
    use std::ffi::c_void;

    #[test]
    fn native_values_round_trip_finite_runtime_values() {
        let cases = [
            Value::Num(3.5),
            Value::Bool(true),
            Value::Vec2([1.0, 2.0]),
            Value::Vec3([1.0, 2.0, 3.0]),
            Value::Vec4([1.0, 2.0, 3.0, 4.0]),
            Value::Color([0.1, 0.2, 0.3, 0.4]),
        ];
        for value in cases {
            assert_eq!(
                native_to_value(value_to_native(&value).expect("convert")).expect("restore"),
                value
            );
        }
    }

    #[test]
    fn property_kind_codes_cover_dynamic_track_kinds() {
        for kind in [
            PropertyKind::F32,
            PropertyKind::U32,
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

    fn property_kind_code(kind: PropertyKind) -> u32 {
        match kind {
            PropertyKind::F32 => NATIVE_PROPERTY_F32,
            PropertyKind::U32 => NATIVE_PROPERTY_U32,
            PropertyKind::Vec2 => NATIVE_PROPERTY_VEC2,
            PropertyKind::Vec4 => NATIVE_PROPERTY_VEC4,
            PropertyKind::String => NATIVE_PROPERTY_STRING,
            PropertyKind::PointList => NATIVE_PROPERTY_POINT_LIST,
            PropertyKind::Generic => NATIVE_PROPERTY_GENERIC,
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

    unsafe extern "C" fn pulse_action(_host: *mut c_void) -> i32 {
        NATIVE_STATUS_OK
    }

    unsafe extern "C" fn pulse_evaluate(ctx: *mut NativePrimitiveEvaluateCtxV2) -> i32 {
        let ctx = unsafe { &*ctx };
        let Some(append_path) = ctx.append_path else {
            return NATIVE_STATUS_TYPE_ERROR;
        };
        let command = NativePathCommandV2 {
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
            functions: Vec::new(),
            primitives: Vec::new(),
            actions: Vec::new(),
            services: Vec::new(),
        };
        assert_eq!(
            unsafe {
                native_register_property(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    c"Rect".as_ptr(),
                    c"glow".as_ptr(),
                    NATIVE_PROPERTY_F32,
                    true,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(
            unsafe {
                native_register_function(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    c"double".as_ptr(),
                    double,
                )
            },
            NATIVE_STATUS_OK
        );
        let primitive = NativePrimitiveV2 {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            evaluate: Some(pulse_evaluate),
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
                    c"pulse".as_ptr(),
                    pulse_action,
                )
            },
            NATIVE_STATUS_OK
        );
        assert_eq!(
            unsafe {
                native_provide_service(
                    (&mut host as *mut NativeHost).cast::<c_void>(),
                    NativeServiceV2 {
                        name: c"demo.pulse".as_ptr(),
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
    fn native_path_commands_become_render_commands() {
        let mut host = NativePrimitiveEvaluateHost {
            commands: Vec::new(),
        };
        let command = NativePathCommandV2 {
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
    fn native_primitive_evaluate_emits_render_commands() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
        };

        let primitive = NativePrimitiveV2 {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            evaluate: Some(pulse_evaluate),
        };
        let adapter =
            NativePrimitiveAdapter::new(primitive, Arc::new(()) as Arc<dyn Any + Send + Sync>)
                .expect("adapter");
        let track = crate::timeline::AnimationTrack::new("pulse".to_string());
        let ctx = EvaluateCtx {
            track: &track,
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
            target_resolver: None,
        };
        let commands =
            adapter.evaluate(&ctx, None).expect("native evaluate").expect("native commands");
        assert!(matches!(commands.as_slice(), [crate::primitives::RenderCommand::Paths { .. }]));
    }

    #[test]
    fn native_primitive_metadata_maps_all_abi_codes() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_ANNOTATION, NATIVE_PRIMITIVE_CATEGORY_CONTAINER,
            NATIVE_PRIMITIVE_CATEGORY_MEDIA, NATIVE_PRIMITIVE_CATEGORY_PLOT,
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CATEGORY_TEXT,
            NATIVE_PRIMITIVE_CHILD_EQUATION, NATIVE_PRIMITIVE_CHILD_FILTER,
            NATIVE_PRIMITIVE_CHILD_GENERIC, NATIVE_PRIMITIVE_CHILD_MASK,
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
    }
}
