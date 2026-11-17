//! Native ordinary functions and invocation-owned inert plan writers.

use std::collections::HashMap;
use std::ffi::{c_char, c_void};
use std::sync::Arc;

use animatix_core::callable::{CallableDescriptor, CallableType};
use animatix_plugin_api::*;

use super::convert::{
    NativeValueArena, native_easing, native_to_property_value, native_to_value, read_c_string,
    value_to_native,
};
use crate::timeline::{ActorRef, Environment, EvalError, PlanDuration, TimelinePlanBuilder, Value};

pub(super) struct NativeCallableHost<'a> {
    env: &'a Environment,
    services: &'a HashMap<String, usize>,
    properties: &'a HashMap<u32, (String, animatix_syntax::schema::PropertyValueKind)>,
    actors: Vec<ActorRef>,
    handle_namespace: u64,
    arena: NativeValueArena,
    builder: TimelinePlanBuilder,
    accepts_plan: bool,
    failed: bool,
}

impl NativeCallableHost<'_> {
    fn encode(&mut self, value: &Value) -> Result<NativeValue, EvalError> {
        match value {
            Value::ActorRef(actor) => {
                let index =
                    self.actors.iter().position(|existing| existing == actor).unwrap_or_else(
                        || {
                            self.actors.push(actor.clone());
                            self.actors.len() - 1
                        },
                    );
                Ok(NativeValue {
                    tag: NATIVE_VALUE_ACTOR,
                    handle: (self.handle_namespace << 32) | (index as u64 + 1),
                    ..NativeValue::default()
                })
            },
            Value::List(items) => {
                let values =
                    items.iter().map(|item| self.encode(item)).collect::<Result<Vec<_>, _>>()?;
                let (list, list_len) = self.arena.list(values);
                Ok(NativeValue {
                    tag: NATIVE_VALUE_LIST,
                    list,
                    list_len,
                    ..NativeValue::default()
                })
            },
            _ => value_to_native(value, &mut self.arena),
        }
    }

    fn decode(&self, native: NativeValue) -> Result<Value, EvalError> {
        if native.tag == NATIVE_VALUE_ACTOR {
            return (native.handle >> 32 == self.handle_namespace)
                .then_some(native.handle & 0xffff_ffff)
                .and_then(|value| value.checked_sub(1))
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| self.actors.get(index))
                .cloned()
                .map(Value::ActorRef)
                .ok_or_else(|| EvalError::TypeMismatch("foreign native actor handle".into()));
        }
        if native.tag == NATIVE_VALUE_LIST {
            if native.list_len == 0 {
                return Ok(Value::List(Arc::from([])));
            }
            if native.list.is_null() {
                return Err(EvalError::TypeMismatch("null native list".into()));
            }
            // The plugin owns this buffer for the duration of its callback.
            let values = unsafe { std::slice::from_raw_parts(native.list, native.list_len) };
            return values
                .iter()
                .map(|item| self.decode(*item))
                .collect::<Result<Vec<_>, _>>()
                .map(|values| Value::List(values.into()));
        }
        native_to_value(native)
    }
}

unsafe extern "C" fn get_env(host: *mut c_void, name: *const c_char, out: *mut NativeValue) -> i32 {
    let Some(host) = (unsafe { host.cast::<NativeCallableHost>().as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let Some(value) = host.env.get(&name) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    match host.encode(&value) {
        Ok(value) => {
            unsafe {
                *out = value;
            }
            NATIVE_STATUS_OK
        },
        Err(_) => NATIVE_STATUS_TYPE_ERROR,
    }
}

unsafe extern "C" fn get_service(host: *mut c_void, name: *const c_char, out: *mut usize) -> i32 {
    let Some(host) = (unsafe { host.cast::<NativeCallableHost>().as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(value) = host.services.get(&name) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    unsafe {
        *out = *value;
    }
    NATIVE_STATUS_OK
}

unsafe extern "C" fn get_property_id(host: *mut c_void, name: *const c_char, out: *mut u32) -> i32 {
    let Some(host) = (unsafe { host.cast::<NativeCallableHost>().as_ref() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let Some(name) = (unsafe { read_c_string(name) }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    if out.is_null() {
        return NATIVE_STATUS_TYPE_ERROR;
    }
    let Some((id, _)) = host.properties.iter().find(|(_, (property, _))| property == &name) else {
        return NATIVE_STATUS_UNSUPPORTED;
    };
    unsafe {
        *out = *id;
    }
    NATIVE_STATUS_OK
}

pub(crate) fn decode_native_easing(value: NativeEasing) -> Option<crate::easing::Easing> {
    use crate::easing::Easing;
    if !value
        .params
        .iter()
        .all(|value| value.is_finite() && (*value as f32).is_finite())
    {
        return None;
    }
    match (value.code, value.param_len) {
        (NATIVE_EASING_SPRING, 2) => Some(Easing::Spring {
            damping: value.params[0] as f32,
            frequency: value.params[1] as f32,
        }),
        (NATIVE_EASING_SPRING_V0, 3) => Some(Easing::SpringV0 {
            damping: value.params[0] as f32,
            frequency: value.params[1] as f32,
            v0: value.params[2] as f32,
        }),
        (NATIVE_EASING_CUBIC_BEZIER, 4) => {
            Some(Easing::CubicBezier(value.params.map(|value| value as f32)))
        },
        (code, 0) => native_easing(code),
        _ => None,
    }
}

pub(crate) fn encode_native_easing(value: crate::easing::Easing) -> NativeEasing {
    use crate::easing::Easing;
    let (code, param_len, params) = match value {
        Easing::Spring { damping, frequency } => {
            (NATIVE_EASING_SPRING, 2, [damping as f64, frequency as f64, 0.0, 0.0])
        },
        Easing::SpringV0 {
            damping,
            frequency,
            v0,
        } => (NATIVE_EASING_SPRING_V0, 3, [damping as f64, frequency as f64, v0 as f64, 0.0]),
        Easing::CubicBezier(points) => (NATIVE_EASING_CUBIC_BEZIER, 4, points.map(f64::from)),
        other => (super::convert::easing_code(other), 0, [0.0; 4]),
    };
    NativeEasing {
        code,
        param_len,
        params,
    }
}

unsafe extern "C" fn append_property_op(
    host: *mut c_void,
    actor: u64,
    property: u32,
    value: NativeValue,
    start_s: f64,
    end_s: f64,
    easing: NativeEasing,
) -> i32 {
    let Some(host) = (unsafe { host.cast::<NativeCallableHost>().as_mut() }) else {
        return NATIVE_STATUS_TYPE_ERROR;
    };
    let result = append_checked(host, actor, property, value, start_s, end_s, easing);
    if result.is_err() {
        host.failed = true;
    }
    result.map_or(NATIVE_STATUS_TYPE_ERROR, |()| NATIVE_STATUS_OK)
}

fn append_checked(
    host: &mut NativeCallableHost<'_>,
    actor: u64,
    property: u32,
    value: NativeValue,
    start_s: f64,
    end_s: f64,
    easing: NativeEasing,
) -> Result<(), EvalError> {
    let invalid = || EvalError::TypeMismatch("invalid native plan operation".into());
    if !host.accepts_plan || !end_s.is_finite() || end_s < start_s {
        return Err(invalid());
    }
    let actor = (actor >> 32 == host.handle_namespace)
        .then_some(actor & 0xffff_ffff)
        .and_then(|value| value.checked_sub(1))
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| host.actors.get(index))
        .cloned()
        .ok_or_else(invalid)?;
    let (name, kind) = host.properties.get(&property).ok_or_else(invalid)?;
    if native_to_property_value(value, *kind).is_none() {
        return Err(invalid());
    }
    let value = host.decode(value)?;
    let start = PlanDuration::from_seconds(start_s).map_err(|_| invalid())?;
    let duration = PlanDuration::from_seconds(end_s - start_s).map_err(|_| invalid())?;
    let easing = decode_native_easing(easing).ok_or_else(invalid)?;
    host.builder
        .append_property_op(actor, name.clone(), value, start, duration, Some(easing));
    Ok(())
}

/// Invoke a plugin without exposing a Timeline or partially modifying a World.
pub(super) fn invoke_native_callable(
    descriptor: &CallableDescriptor,
    callback: NativeCallableFn,
    args: &[Value],
    env: &Environment,
    properties: &HashMap<u32, (String, animatix_syntax::schema::PropertyValueKind)>,
    services: &HashMap<String, usize>,
) -> Result<Value, EvalError> {
    let mut host = NativeCallableHost {
        env,
        services,
        properties,
        actors: Vec::new(),
        handle_namespace: next_handle_namespace(),
        arena: NativeValueArena::default(),
        builder: TimelinePlanBuilder::default(),
        accepts_plan: !env.is_frame_phase() && allows_timeline(&descriptor.return_type),
        failed: false,
    };
    let native_args = args.iter().map(|arg| host.encode(arg)).collect::<Result<Vec<_>, _>>()?;
    let mut context = NativeCallableContext {
        size: std::mem::size_of::<NativeCallableContext>(),
        host: (&mut host as *mut NativeCallableHost).cast(),
        get_env,
        get_service,
        get_property_id,
        append_property_op,
    };
    let mut out = NativeValue {
        tag: NATIVE_VALUE_NULL,
        ..NativeValue::default()
    };
    let status =
        unsafe { callback(&mut context, native_args.as_ptr(), native_args.len(), &mut out) };
    if status != NATIVE_STATUS_OK || host.failed {
        return Err(EvalError::TypeMismatch(format!(
            "native function '{}' failed (status {status}); its plan was discarded",
            descriptor.qualified_name()
        )));
    }
    let value = if out.tag == NATIVE_VALUE_TIMELINE {
        if !host.accepts_plan {
            return Err(EvalError::TypeMismatch(
                "native function cannot return a timeline in this context".into(),
            ));
        }
        Value::TimelinePlan(Arc::new(host.builder.finish()))
    } else {
        host.decode(out)?
    };
    crate::timeline::callable::validate_return_value(descriptor, &value)?;
    Ok(value)
}

fn allows_timeline(ty: &CallableType) -> bool {
    match ty {
        CallableType::Timeline | CallableType::Any => true,
        CallableType::Union(types) => types.iter().any(allows_timeline),
        _ => false,
    }
}

fn next_handle_namespace() -> u64 {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);
    u64::from(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
}
