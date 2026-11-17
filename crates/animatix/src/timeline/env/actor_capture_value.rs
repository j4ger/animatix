//! Build-time rebinding of actor handles inside captured values.

use std::collections::HashMap;

use crate::timeline::plan_value::ActorRef;

use super::{CapturedEnv, Value};

/// Return an updated value only when one of its captured actor references has
/// a canonical World handle. Containers are copied lazily on the first change.
pub(crate) fn rebind_actor_value(
    value: &Value,
    bindings: &HashMap<String, ActorRef>,
) -> Option<Value> {
    match value {
        Value::ActorRef(actor) => bindings.get(actor.label()).cloned().map(Value::ActorRef),
        Value::List(items) => {
            let mut rebound = None;
            for (index, item) in items.iter().enumerate() {
                if let Some(updated) = rebind_actor_value(item, bindings) {
                    if rebound.is_none() {
                        rebound = Some(items.to_vec());
                    }
                    if let Some(values) = &mut rebound {
                        values[index] = updated;
                    }
                }
            }
            rebound.map(|values| Value::List(values.into()))
        },
        Value::Object(name, fields) => {
            let mut rebound = None;
            for (key, value) in fields {
                if let Some(updated) = rebind_actor_value(value, bindings) {
                    if rebound.is_none() {
                        rebound = Some(fields.clone());
                    }
                    if let Some(values) = &mut rebound {
                        values.insert(key.clone(), updated);
                    }
                }
            }
            rebound.map(|fields| Value::Object(name.clone(), fields))
        },
        Value::Closure(args, body, captures) => rebind_captured_env(captures, bindings)
            .map(|captures| Value::Closure(args.clone(), body.clone(), captures)),
        _ => None,
    }
}

/// Return a captured environment with rebound actor references, cloning its
/// map only when a contained value changes.
pub(crate) fn rebind_captured_env(
    captures: &CapturedEnv,
    bindings: &HashMap<String, ActorRef>,
) -> Option<CapturedEnv> {
    let mut rebound = None;
    for (name, value) in &captures.0 {
        if let Some(updated) = rebind_actor_value(value, bindings) {
            if rebound.is_none() {
                rebound = Some(captures.0.clone());
            }
            if let Some(values) = &mut rebound {
                values.insert(name.clone(), updated);
            }
        }
    }
    rebound.map(CapturedEnv)
}
