//! Service handles and the native `provide_service` entry point.

use std::any::Any;
use std::ffi::c_void;
use std::sync::Arc;

use animatix_plugin_api::{NATIVE_STATUS_OK, NATIVE_STATUS_TYPE_ERROR, NativeService};

use super::{NativeHost, read_c_string};

pub(crate) struct NativeServiceHandle {
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

pub(crate) unsafe extern "C" fn native_provide_service(
    host: *mut c_void,
    service: NativeService,
) -> i32 {
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
