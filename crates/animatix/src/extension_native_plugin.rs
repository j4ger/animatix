//! Native cdylib plugin loading behind the `plugin-loading` feature.
//!
//! Plugins communicate through the stable ABI in `animatix-plugin-api`, so
//! their Rust version and internal types can differ from the host. The host
//! keeps the loaded `Library` alive for the lifetime of registered callbacks.

use std::any::Any;
use std::collections::HashMap;
use std::ffi::{CStr, c_char, c_void};
use std::path::Path;
use std::sync::Arc;

use animatix_plugin_api::{
    ABI_VERSION, NATIVE_PATH_ELLIPSE, NATIVE_PATH_LINE, NATIVE_PATH_POLYGON, NATIVE_PATH_RECT,
    NATIVE_PROPERTY_F32, NATIVE_PROPERTY_GENERIC, NATIVE_PROPERTY_POINT_LIST,
    NATIVE_PROPERTY_STRING, NATIVE_PROPERTY_U32, NATIVE_PROPERTY_VEC2, NATIVE_PROPERTY_VEC4,
    NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_STATUS_UNSUPPORTED, NATIVE_VALUE_BOOL,
    NATIVE_VALUE_COLOR, NATIVE_VALUE_COMMAND_LIST, NATIVE_VALUE_ENUM, NATIVE_VALUE_LIST,
    NATIVE_VALUE_NUM, NATIVE_VALUE_POINT_LIST, NATIVE_VALUE_STRING, NATIVE_VALUE_STRING_LIST,
    NATIVE_VALUE_TRANSFORM, NATIVE_VALUE_U32, NATIVE_VALUE_VARIANT, NATIVE_VALUE_VEC2,
    NATIVE_VALUE_VEC3, NATIVE_VALUE_VEC4, NativeActionExecuteFn, NativeFunction, NativeInstallFn,
    NativePathCommand, NativePluginApi, NativePrimitive, NativePrimitiveEvaluateCtx,
    NativePrimitiveEvaluateFn, NativePropertyDescriptor, NativeService, NativeValue,
};
use kurbo::Shape;
use libloading::Library;

use crate::extension_context::ExtensionContext;
use crate::primitives::{BuildCtx, ChildProcessing, EvaluateCtx, Primitive};
use crate::timeline::actions::registry::{ActionSignature, BuiltinAction};
use crate::timeline::property_registry::lookup_property;
use crate::timeline::{ActorCategory, ActorKindId, Environment, EvalError, PropertyValue, Value};

use super::{ExtensionPlugin, PluginDisposer, PluginError};

/// A native plugin loaded from a `cdylib` shared library.
pub struct NativePlugin {
    name: String,
    library: Arc<Library>,
    api: NativePluginApi,
    install: Option<NativeInstallFn>,
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
                version: ABI_VERSION,
                register_property: native_register_property,
                register_function: native_register_function,
                register_primitive: native_register_primitive,
                register_action: native_register_action,
                provide_service: native_provide_service,
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
                functions: Vec::new(),
                primitives: Vec::new(),
                actions: Vec::new(),
                services: Vec::new(),
            };
            let install = self.install.expect("install symbol checked during load");
            let status = unsafe { (install)(&self.api, (&mut host as *mut NativeHost).cast()) };
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
    property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    functions: Vec<String>,
    primitives: Vec<String>,
    actions: Vec<String>,
    services: Vec<String>,
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
    let display_name = unsafe { read_c_string(descriptor.display_name) };
    let group = unsafe { read_c_string(descriptor.group) };
    let help = unsafe { read_c_string(descriptor.help) };
    let id = match host.ctx.register_property_full(
        actor_type.clone(),
        name.clone(),
        kind,
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
    host.property_ids.insert(name, id);
    NATIVE_STATUS_OK
}

unsafe extern "C" fn native_register_function(
    host: *mut c_void,
    name: *const c_char,
    callback: NativeFunction,
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
        let mut arena = NativeValueArena::default();
        let native_args = args
            .iter()
            .map(|value| value_to_native(value, &mut arena))
            .collect::<Result<Vec<_>, _>>()?;
        let mut out = NativeValue::default();
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
    primitive: NativePrimitive,
) -> i32 {
    let Some(host) = (unsafe { (host as *mut NativeHost).as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(library) = host.library.clone() else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(adapter) = NativePrimitiveAdapter::new(primitive, library, host.property_ids.clone())
    else {
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
    callback: NativeActionExecuteFn,
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
    property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    evaluate: Option<NativePrimitiveEvaluateFn>,
    _library: Arc<dyn Any + Send + Sync>,
}

impl NativePrimitiveAdapter {
    fn new(
        primitive: NativePrimitive,
        library: Arc<dyn Any + Send + Sync>,
        property_ids: HashMap<String, animatix_syntax::schema::PropertyId>,
    ) -> Option<Self> {
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
            property_ids,
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
            ctx,
            property_ids: &self.property_ids,
            commands: Vec::new(),
            arena: NativeValueArena::default(),
        };
        let mut native_ctx = NativePrimitiveEvaluateCtx {
            size: std::mem::size_of::<NativePrimitiveEvaluateCtx>(),
            time_ms: ctx.time_ms as f64,
            host: (&mut host as *mut NativePrimitiveEvaluateHost).cast(),
            get_property: Some(native_get_property),
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

struct NativePrimitiveEvaluateHost<'a> {
    ctx: &'a EvaluateCtx<'a>,
    property_ids: &'a HashMap<String, animatix_syntax::schema::PropertyId>,
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
    callback: NativeActionExecuteFn,
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

#[derive(Default)]
struct NativeValueArena {
    strings: Vec<Vec<u8>>,
    lists: Vec<Vec<NativeValue>>,
    values: Vec<Vec<NativeValue>>,
}

impl NativeValueArena {
    fn string(&mut self, text: &str) -> (*const c_char, usize) {
        let bytes = text.as_bytes().to_vec();
        let len = bytes.len();
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
            Ok(Value::List(read_native_list(&native)?))
        },
        NATIVE_VALUE_TRANSFORM => {
            Ok(Value::List(native.transform.iter().map(|value| Value::Num(*value)).collect()))
        },
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
    use std::ffi::c_void;

    fn sample_evaluate_ctx(track: &crate::timeline::AnimationTrack) -> EvaluateCtx<'_> {
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
            Value::List(vec![Value::Num(1.0), Value::Str("two".to_string())]),
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

    unsafe extern "C" fn pulse_action(_host: *mut c_void) -> i32 {
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
            functions: Vec::new(),
            primitives: Vec::new(),
            actions: Vec::new(),
            services: Vec::new(),
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
                    c"double".as_ptr(),
                    double,
                )
            },
            NATIVE_STATUS_OK
        );
        let primitive = NativePrimitive {
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
                    NativeService {
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
        let track = crate::timeline::AnimationTrack::new("pulse".to_string());
        let eval_ctx = sample_evaluate_ctx(&track);
        let property_ids = HashMap::new();
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
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
        let mut track = crate::timeline::AnimationTrack::new("pulse".to_string());
        crate::timeline::property_engine::write_property_plan_slot(
            &mut track,
            id,
            PropertyKind::F32,
            crate::timeline::PropertyValue::F32(0.75),
            0,
            0,
            crate::easing::Easing::Linear,
        );
        let eval_ctx = sample_evaluate_ctx(&track);
        let property_ids = HashMap::from([("glow".to_string(), id)]);
        let mut host = NativePrimitiveEvaluateHost {
            ctx: &eval_ctx,
            property_ids: &property_ids,
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
    fn native_primitive_evaluate_emits_render_commands() {
        use animatix_plugin_api::{
            NATIVE_PRIMITIVE_CATEGORY_SHAPE, NATIVE_PRIMITIVE_CHILD_GENERIC,
        };

        let primitive = NativePrimitive {
            type_name: c"Pulse".as_ptr(),
            display_name: c"Pulse".as_ptr(),
            icon_id: c"extension:pulse".as_ptr(),
            category: NATIVE_PRIMITIVE_CATEGORY_SHAPE,
            advanced: false,
            child_processing: NATIVE_PRIMITIVE_CHILD_GENERIC,
            evaluate: Some(pulse_evaluate),
        };
        let adapter = NativePrimitiveAdapter::new(
            primitive,
            Arc::new(()) as Arc<dyn Any + Send + Sync>,
            HashMap::new(),
        )
        .expect("adapter");
        let track = crate::timeline::AnimationTrack::new("pulse".to_string());
        let ctx = sample_evaluate_ctx(&track);
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
