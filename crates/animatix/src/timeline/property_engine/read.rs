//! Environment injection and override-aware effective property reads.

use crate::extension_context::ExtensionContext;
use crate::timeline::AnimationTrack;
use crate::timeline::env::{Environment, Value};

use super::plan::read_property_plan_slot;
use super::value::PropertyValue;

// ─────────────────────────────────────────────────────────────
// Environment injection
// ─────────────────────────────────────────────────────────────

/// Inject a property's value into the runtime Environment.
///
/// Iterates over the property registry and injects every INJECTABLE property
/// that has a direct track storage field.  Derived values (width, height,
/// radius, at alias, shift, line_cap, line_join) are handled afterwards.
///
/// This is called from `inject_runtime_lookup_values()` for every actor,
/// every frame.
pub(crate) fn inject_property_into_env(
    env: &mut Environment,
    key: &mut String,
    label: &str,
    track: &AnimationTrack,
    world: Option<&crate::timeline::World>,
    time_ms: u64,
    allowed_props: Option<&std::collections::HashSet<String>>,
    inject_animating_flags: bool,
) {
    use crate::timeline::property_registry::{PROPERTY_REGISTRY, PropertyFlags, ReadSource};

    // Reusable key buffer threaded in from the injection caller (PF-6: one
    // `String::with_capacity` per track per frame was 60 allocations on a
    // 60-actor scene); the buffer is cleared and reshaped in place.
    key.clear();
    key.push_str(label);
    key.push('.');
    let prefix_len = key.len();

    // ── Inject every registered INJECTABLE property via its read_source ──
    for schema in PROPERTY_REGISTRY.iter() {
        if !schema.flags.contains(PropertyFlags::INJECTABLE) {
            continue;
        }

        if let Some(allowed) = allowed_props {
            if !allowed.contains(schema.name) {
                continue;
            }
        }

        // Read via the schema's read_source (handles Field, Alias, Component).
        let pv = match schema.read_source.read(track, world, time_ms) {
            Some(pv) => pv,
            None => {
                // No track value — fall back to schema default.
                // For Component sources, extract the indexed component.
                let write_default = (schema.default_value)(&track.caps);
                match schema.read_source {
                    ReadSource::Field(_) | ReadSource::Alias(_) => write_default,
                    ReadSource::Component { index, scale, .. } => {
                        if let PropertyValue::Vec2(v) = write_default {
                            PropertyValue::F32(v[index] * scale as f32)
                        } else {
                            write_default
                        }
                    },
                    ReadSource::None_ => continue,
                }
            },
        };

        // Inject the value and typed sub-keys (x, y, r, g, b, a).
        key.truncate(prefix_len);
        key.push_str(schema.name);
        inject_value(env, key, prefix_len, schema.name, &pv);

        // Inject the animation-state flag under the internal animating_flag
        // key (read by `is_animating(&label.prop)`).
        if inject_animating_flags {
            if let Some(storage) = schema.read_source.storage_field() {
                let flag_key = crate::timeline::env_keys::animating_flag(label, schema.name);
                let animating = track.is_field_currently_animating(storage, time_ms);
                env.set(&flag_key, Value::Bool(animating));
            }
        }
    }
}

/// Inject external property values into a frame environment.
pub(crate) fn inject_extension_properties_into_env(
    env: &mut Environment,
    key: &mut String,
    label: &str,
    track: &AnimationTrack,
    plan: &crate::timeline::plan::PropertyPlan,
    time_ms: u64,
    ctx: Option<&ExtensionContext>,
    allowed_props: Option<&std::collections::HashSet<String>>,
    inject_animating_flags: bool,
) {
    let Some(ctx) = ctx else {
        return;
    };
    let actor_type = track.actor_type.as_str();

    // Same shared-buffer discipline as `inject_property_into_env` (PF-6).
    key.clear();
    key.push_str(label);
    key.push('.');
    let prefix_len = key.len();

    for spec in ctx.property_specs() {
        if !spec.injectable || spec.actor_type != actor_type {
            continue;
        }
        if let Some(allowed) = allowed_props {
            if !allowed.contains(&spec.name) {
                continue;
            }
        }
        let Some(pv) = read_property_plan_slot(plan, spec.id, time_ms) else {
            continue;
        };
        key.truncate(prefix_len);
        key.push_str(&spec.name);
        inject_value(env, key, prefix_len, &spec.name, &pv);

        if inject_animating_flags {
            let animating = plan.get(spec.id).is_some_and(|slot| slot.track.has_any_keyframes());
            let flag_key = crate::timeline::env_keys::animating_flag(label, &spec.name);
            env.set(&flag_key, Value::Bool(animating));
        }
    }
}

/// Inject a `PropertyValue` into the environment, with typed sub-keys
/// (`.x`, `.y` for Vec2; `.r`, `.g`, `.b`, `.a` for Color/Vec4).
///
/// `key` must be positioned at the end of the property name (the full
/// `{label}.{property}` prefix).  It is restored to `prefix_len + name.len()`
/// after injection so that callers can continue using the buffer.
fn inject_value(
    env: &mut Environment,
    key: &mut String,
    prefix_len: usize,
    name: &str,
    value: &PropertyValue,
) {
    let restore = prefix_len + name.len();
    match value {
        PropertyValue::F32(v) => {
            env.set(&*key, Value::Num(*v as f64));
        },
        PropertyValue::U32(v) => {
            env.set(&*key, Value::Num(*v as f64));
        },
        PropertyValue::Vec2(v) => {
            env.set(&*key, Value::Vec2([v[0] as f64, v[1] as f64]));
            key.push_str(".x");
            env.set(&*key, Value::Num(v[0] as f64));
            key.truncate(restore);
            key.push_str(".y");
            env.set(&*key, Value::Num(v[1] as f64));
        },
        PropertyValue::Vec4(v) | PropertyValue::Color(v) => {
            env.set(&*key, Value::Color([v[0] as f64, v[1] as f64, v[2] as f64, v[3] as f64]));
            key.push_str(".r");
            env.set(&*key, Value::Num(v[0] as f64));
            key.truncate(restore);
            key.push_str(".g");
            env.set(&*key, Value::Num(v[1] as f64));
            key.truncate(restore);
            key.push_str(".b");
            env.set(&*key, Value::Num(v[2] as f64));
            key.truncate(restore);
            key.push_str(".a");
            env.set(&*key, Value::Num(v[3] as f64));
        },
        PropertyValue::Transform(v) => {
            env.set(
                &*key,
                Value::List(std::sync::Arc::from(vec![
                    Value::Num(v[0] as f64),
                    Value::Num(v[1] as f64),
                    Value::Num(v[2] as f64),
                    Value::Num(v[3] as f64),
                    Value::Num(v[4] as f64),
                    Value::Num(v[5] as f64),
                ])),
            );
        },
        PropertyValue::String(v) => {
            env.set(&*key, Value::Str(v.clone()));
        },
        PropertyValue::Enum(v) => {
            env.set(&*key, Value::Str(v.clone()));
        },
        PropertyValue::Bool(v) => {
            env.set(&*key, Value::Bool(*v));
        },
        PropertyValue::Variant { value, .. } => {
            inject_value(env, key, prefix_len, name, value);
            return;
        },
        _ => {},
    }
    key.truncate(restore);
}

// ─────────────────────────────────────────────────────────────
// Override-aware property reads
// ─────────────────────────────────────────────────────────────

/// Read a property value using a pre-resolved schema / plan-slot pair.
///
/// Same semantics as `dispatch::read_property_value`, including the fallback to
/// the track's own field, but skips the name → id lookup that
/// `property_registry::resolve_property` has already done for this call.
pub(crate) fn read_property_value_resolved(
    track: &AnimationTrack,
    plan: &crate::timeline::plan::PropertyPlan,
    schema: &crate::timeline::property_registry::PropertySchema,
    slot: Option<animatix_syntax::schema::PropertyId>,
    time_ms: u64,
) -> Option<PropertyValue> {
    if let Some(id) = slot
        && let Some(value) = read_property_plan_slot(plan, id, time_ms)
    {
        return Some(value);
    }
    track.field_ref(schema.field).and_then(|f| f.evaluate_value(time_ms))
}

/// Read an effective f32 property value, preferring modifier overrides over
/// track keyframes, with the registry entry pre-resolved by the caller.
///
/// The registry lookup is a process-constant string hash; the transform path
/// reads the same five properties on every node every frame and resolves them
/// once instead (PF-4). Semantics are identical: the modifier override wins
/// first — so an override still applies even if the property were somehow
/// unregistered — then the schema/slot read, then `default`.
pub(crate) fn effective_f32_resolved(
    track: &AnimationTrack,
    plan: &crate::timeline::plan::PropertyPlan,
    overrides: Option<&std::collections::HashMap<String, Value>>,
    time_ms: u64,
    name: &str,
    resolved: crate::timeline::property_registry::ResolvedPropertyRead,
    default: f32,
) -> f32 {
    if let Some(Value::Num(v)) = overrides.and_then(|ov| ov.get(name)) {
        return *v as f32;
    }
    if let Some((schema, slot)) = resolved
        && let Some(pv) = read_property_value_resolved(track, plan, schema, slot, time_ms)
        && let PropertyValue::F32(v) = pv
    {
        return v;
    }
    default
}

/// Read a node's half-size (its radius) for this frame, delegating the unit
/// reconciliation to [`crate::primitives::resolve_half_size`] so the node
/// transform and the primitives cannot disagree about what an override means.
/// See that helper for why the three sources need different handling.
pub(crate) fn effective_half_size_resolved(
    track: &AnimationTrack,
    plan: &crate::timeline::plan::PropertyPlan,
    overrides: Option<&std::collections::HashMap<String, Value>>,
    time_ms: u64,
    resolved: crate::timeline::property_registry::ResolvedPropertyRead,
    default: [f32; 2],
) -> [f32; 2] {
    let track_half = if let Some((schema, slot)) = resolved
        && let Some(pv) = read_property_value_resolved(track, plan, schema, slot, time_ms)
        && let PropertyValue::Vec2(v) = pv
    {
        v
    } else {
        default
    };
    crate::primitives::resolve_half_size(track_half, overrides)
}

/// Read an effective Transform property value ([f32; 6]), preferring modifier
/// overrides over track keyframes.
pub(crate) fn effective_transform(
    track: &AnimationTrack,
    plan: &crate::timeline::plan::PropertyPlan,
    overrides: Option<&std::collections::HashMap<String, Value>>,
    time_ms: u64,
    name: &str,
    resolved: crate::timeline::property_registry::ResolvedPropertyRead,
    default: [f32; 6],
) -> [f32; 6] {
    if let Some(Value::List(items)) = overrides.and_then(|ov| ov.get(name)) {
        if items.len() == 6 {
            let mut t = default;
            for (i, item) in items.iter().enumerate() {
                t[i] = item.as_num() as f32;
            }
            return t;
        }
    }
    if let Some((schema, slot)) = resolved
        && let Some(pv) = read_property_value_resolved(track, plan, schema, slot, time_ms)
        && let PropertyValue::Transform(v) = pv
    {
        return v;
    }
    default
}
