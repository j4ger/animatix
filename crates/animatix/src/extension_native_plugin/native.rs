//! The loaded plugin handle and its `ExtensionPlugin` installation path.

use std::any::Any;
use std::collections::HashMap;
use std::ffi::{CStr, c_char};
use std::path::Path;
use std::sync::Arc;

use animatix_plugin_api::{
    NATIVE_STATUS_OK, NativeInstallFn, NativePluginApi, UNSTABLE_ABI_VERSION,
};
use libloading::Library;

use super::{
    NativeHost, native_provide_service, native_register_action, native_register_effect,
    native_register_function, native_register_primitive, native_register_property,
};
use crate::extension_context::ExtensionContext;
use crate::extension_plugin::{ExtensionPlugin, PluginDisposer, PluginError};

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
