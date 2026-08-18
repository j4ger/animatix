//! Native cdylib plugin loading behind the `plugin-loading` feature.
//!
//! Plugins communicate through the stable ABI in `animatix-plugin-api`, so
//! their Rust version and internal types can differ from the host. The host
//! keeps the loaded `Library` alive for the lifetime of registered callbacks.

use std::ffi::{CStr, c_char, c_void};
use std::path::Path;
use std::sync::Arc;

use animatix_plugin_api::{
    ABI_VERSION, NATIVE_PROPERTY_F32, NATIVE_PROPERTY_GENERIC, NATIVE_PROPERTY_POINT_LIST,
    NATIVE_PROPERTY_STRING, NATIVE_PROPERTY_U32, NATIVE_PROPERTY_VEC2, NATIVE_PROPERTY_VEC4,
    NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NATIVE_STATUS_UNSUPPORTED, NATIVE_VALUE_BOOL,
    NATIVE_VALUE_COLOR, NATIVE_VALUE_NUM, NATIVE_VALUE_VEC2, NATIVE_VALUE_VEC3, NATIVE_VALUE_VEC4,
    NativeFunctionV1, NativeInstallFn, NativePluginApiV1, NativeValueV1,
};
use libloading::Library;

use crate::extension_context::ExtensionContext;
use crate::timeline::{Environment, EvalError, Value};

use super::{ExtensionPlugin, PluginDisposer, PluginError};

/// A native plugin loaded from a `cdylib` shared library.
pub struct NativePlugin {
    name: String,
    library: Arc<Library>,
    api: NativePluginApiV1,
    install: NativeInstallFn,
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
                *library.get::<NativeInstallFn>(b"animatix_plugin_install_v1").map_err(|err| {
                    PluginError(format!(
                        "'{}' has no animatix_plugin_install_v1 symbol: {err}",
                        path.display()
                    ))
                })?;

            let api = NativePluginApiV1 {
                size: std::mem::size_of::<NativePluginApiV1>(),
                register_property: native_register_property,
                register_function: native_register_function,
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
        let (properties, functions) = {
            let mut host = NativeHost {
                ctx,
                library: Some(Arc::clone(&self.library)),
                properties: Vec::new(),
                functions: Vec::new(),
            };
            let status =
                unsafe { (self.install)(&self.api, (&mut host as *mut NativeHost).cast()) };
            if status != NATIVE_STATUS_OK {
                return Err(PluginError(format!(
                    "{} install failed with status {status}",
                    self.name
                )));
            }
            (host.properties, host.functions)
        };

        Ok(Box::new(move |ctx: &mut ExtensionContext| {
            for (actor_type, name) in properties {
                ctx.remove_property(&actor_type, &name);
            }
            for name in functions {
                ctx.remove_function(&name);
            }
        }))
    }
}

struct NativeHost<'a> {
    ctx: &'a mut ExtensionContext,
    library: Option<Arc<Library>>,
    properties: Vec<(String, String)>,
    functions: Vec<String>,
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
    let _library = library;
    host.functions.push(name);
    NATIVE_STATUS_OK
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
    use std::ffi::{c_char, c_void};

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
        let mut result = NativeValueV1::default();
        result.tag = NATIVE_VALUE_NUM;
        result.num = arg.num * 2.0;
        unsafe {
            *out = result;
        }
        NATIVE_STATUS_OK
    }

    #[test]
    fn native_host_callbacks_install_and_dispose_capabilities() {
        let mut ctx = ExtensionContext::new();
        let mut host = NativeHost {
            ctx: &mut ctx,
            library: None,
            properties: Vec::new(),
            functions: Vec::new(),
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
        let NativeHost {
            ctx,
            properties,
            functions,
            ..
        } = host;

        assert_eq!(properties, vec![("Rect".to_string(), "glow".to_string())]);
        assert_eq!(functions, vec!["double".to_string()]);

        let mut env = Environment::new();
        ctx.install_functions(&mut env);
        let Some(Value::NativeFn(call)) = env.get("double") else {
            panic!("double was not installed");
        };
        assert_eq!(
            call(&[Value::Num(21.0)], &Environment::new()).expect("call native function"),
            Value::Num(42.0)
        );

        assert!(ctx.remove_property("Rect", "glow"));
        assert!(ctx.remove_function("double"));
    }
}
