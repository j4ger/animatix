//! Registration of the ABI's unified callable contracts.

use std::ffi::c_void;

use animatix_core::callable::{CallableDescriptor, CallableParam, CallablePhase, DefaultValue};
use animatix_plugin_api::*;

use super::callable::invoke_native_callable;
use super::{NativeHost, native_to_value, parse_native_type, read_c_string};
use crate::timeline::{EvalError, Value};

pub(crate) unsafe extern "C" fn native_register_callable(
    host: *mut c_void,
    native: NativeCallableDescriptor,
) -> i32 {
    let Some(host) = (unsafe { host.cast::<NativeHost>().as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let descriptor = match unsafe { read_descriptor(native) } {
        Ok(descriptor) => descriptor,
        Err(error) => {
            tracing::warn!("native callable registration rejected: {error}");
            return NATIVE_STATUS_TYPE_ERROR;
        },
    };
    let callback = native.callback;
    let name = descriptor.qualified_name();
    let callback_descriptor = descriptor.clone();
    let services = host.service_values.clone();
    let properties = host
        .property_ids
        .iter()
        .filter_map(|(name, id)| {
            host.property_kinds.get(name).map(|kind| (id.0, (name.clone(), *kind)))
        })
        .collect();
    let library = host.library.clone();
    let result = host.ctx.register_callable(descriptor, move |args, env| {
        let _keep_alive = &library;
        invoke_native_callable(&callback_descriptor, callback, args, env, &properties, &services)
    });
    if let Err(error) = result {
        tracing::warn!("native callable registration rejected: {error}");
        return NATIVE_STATUS_TYPE_ERROR;
    }
    host.functions.push(name);
    NATIVE_STATUS_OK
}

unsafe fn read_descriptor(
    native: NativeCallableDescriptor,
) -> Result<CallableDescriptor, EvalError> {
    let invalid = |message: &str| EvalError::TypeMismatch(message.into());
    let namespace = unsafe { read_c_string(native.namespace) }
        .ok_or_else(|| invalid("missing callable namespace"))?;
    let name =
        unsafe { read_c_string(native.name) }.ok_or_else(|| invalid("missing callable name"))?;
    let return_type = unsafe { read_c_string(native.return_type) }
        .and_then(|ty| parse_native_type(&ty))
        .ok_or_else(|| invalid("invalid callable result type"))?
        .to_callable_type();
    let phase = match native.phase {
        NATIVE_CALLABLE_BUILD => CallablePhase::Build,
        NATIVE_CALLABLE_BUILD_AND_FRAME => CallablePhase::BuildAndFrame,
        _ => return Err(invalid("invalid callable execution phase")),
    };
    if native.param_len > 0 && native.params.is_null() {
        return Err(invalid("null callable parameters"));
    }
    let raw = if native.param_len == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(native.params, native.param_len) }
    };
    let params = raw
        .iter()
        .map(|param| unsafe { read_param(*param) })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CallableDescriptor {
        name,
        namespace,
        params,
        return_type,
        phase,
        help: unsafe { read_c_string(native.help) }.unwrap_or_default(),
        category: unsafe { read_c_string(native.category) }.unwrap_or_default(),
    })
}

unsafe fn read_param(native: NativeCallableParam) -> Result<CallableParam, EvalError> {
    let invalid = |message: &str| EvalError::TypeMismatch(message.into());
    let name =
        unsafe { read_c_string(native.name) }.ok_or_else(|| invalid("missing parameter name"))?;
    let ty = unsafe { read_c_string(native.type_info) }
        .and_then(|ty| parse_native_type(&ty))
        .ok_or_else(|| invalid("invalid parameter type"))?
        .to_callable_type();
    let default = if native.default_value.is_null() {
        None
    } else {
        let value = native_to_value(unsafe { *native.default_value })?;
        Some(match value {
            Value::Num(value) => DefaultValue::Num(value),
            Value::Bool(value) => DefaultValue::Bool(value),
            Value::Str(value) => DefaultValue::Str(value),
            Value::Duration(value) => DefaultValue::Duration(value),
            _ => return Err(invalid("native parameter defaults must be scalar literals")),
        })
    };
    Ok(CallableParam {
        name,
        ty,
        optional: native.optional || default.is_some(),
        default,
        variadic: native.variadic,
        help: unsafe { read_c_string(native.help) }.unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn abi_snapshot_is_the_unified_callable_contract() {
        assert_eq!(UNSTABLE_ABI_VERSION, 10);
        assert!(std::mem::size_of::<NativeCallableContext>() > 0);
        assert_ne!(NATIVE_VALUE_ACTOR, NATIVE_VALUE_NUM);
        assert_ne!(NATIVE_VALUE_DURATION, NATIVE_VALUE_NUM);
    }
}
