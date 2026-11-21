//! Resolve callbacks against their registered standard or extension signatures.

use super::{Arc, Environment, EvalError, Value};

#[allow(clippy::type_complexity)]
pub(super) fn resolve_registered_callback(
    name: &str,
    descriptor: &animatix_core::callable::CallableDescriptor,
    env: &Environment,
) -> Result<Arc<dyn Fn(&[Value], &Environment) -> Result<Value, EvalError> + Send + Sync>, EvalError>
{
    let qualified_name = descriptor.qualified_name();
    let installed = env.callable_descriptor(&qualified_name).ok_or_else(|| {
        EvalError::NotCallable(format!(
            "registered callable '{qualified_name}' is not installed in this environment"
        ))
    })?;
    if installed != descriptor {
        return Err(EvalError::TypeMismatch(format!(
            "registered callable '{qualified_name}' has a different contract in this environment"
        )));
    }

    match env.get(&qualified_name) {
        Some(Value::NativeFn(callback)) => Ok(callback),
        Some(Value::RegisteredFn {
            name: installed_name,
            descriptor: installed_descriptor,
            native: Some(callback),
        }) if installed_name == qualified_name && installed_descriptor.as_ref() == descriptor => {
            Ok(callback)
        },
        _ => Err(EvalError::NotCallable(format!(
            "registered callable '{name}' has no host callback in this environment"
        ))),
    }
}
