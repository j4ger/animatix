//! Registry-driven property-plan slots and extension-property writes.

use animatix_syntax::schema::PropertyValueKind;

use crate::ast::Expr;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::extension_context::ExtensionContext;
use crate::timeline::AnimationTrack;
use crate::timeline::env::{Environment, Value};

use super::value::{PropertyValue, property_value_from_value};

// ─────────────────────────────────────────────────────────────
// Plan-backed property access
// ─────────────────────────────────────────────────────────────

/// Write a value through a plan, without an actor track.
pub fn write_property_plan_slot_on(
    plan: &mut crate::timeline::plan::PropertyPlan,
    id: animatix_syntax::schema::PropertyId,
    kind: crate::timeline::PropertyKind,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) -> bool {
    let has_duration = t_end_ms > t_start_ms;
    let slot = plan.ensure_slot(id, kind);
    // The track model has no step keyframe: two writes at one timestamp collapse
    // to the later one, and any two *consecutive* keyframes read as an
    // interpolation segment. So an undated change has to be fenced as well, or
    // the previous keyframe eases all the way into it and `r.opacity = 0.0` at
    // `#2s` starts fading out at frame 0 — `docs/spec.md` calls a bare property
    // assignment an "Instant Change", and this is where that is true.
    // `preserve_instant_delayed_value` in `timeline/position.rs` is the same
    // fence for the delayed case; the slot track here is a `DynTrack` rather
    // than an `Option<PropertyTrack<T>>`, so it cannot be reused directly.
    let seed_ms = if has_duration {
        t_start_ms
    } else {
        t_start_ms.saturating_sub(1)
    };
    if seed_ms > 0 {
        // `sample` reads what the slot holds *before* the target below is
        // written, so re-seeding a stamp that already carries a keyframe writes
        // that same value again rather than clobbering it.
        if let Some(start) = slot.track.sample(seed_ms) {
            slot.track.add_keyframe(seed_ms, start);
        }
    }
    slot.track.add_keyframe_eased(t_end_ms, value, easing).is_some()
}

/// Write a property value through the actor’s registry-driven `PropertyPlan`.
pub fn write_property_plan_slot(
    track: &mut AnimationTrack,
    id: animatix_syntax::schema::PropertyId,
    kind: crate::timeline::PropertyKind,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) -> bool {
    write_property_plan_slot_on(
        &mut track.property_plan,
        id,
        kind,
        value,
        t_start_ms,
        t_end_ms,
        easing,
    )
}

/// Read a property value through the actor's registry-driven `PropertyPlan`.
pub fn read_property_plan_slot(
    track: &AnimationTrack,
    id: animatix_syntax::schema::PropertyId,
    time_ms: u64,
) -> Option<PropertyValue> {
    track.property_plan.get(id).and_then(|slot| slot.track.sample(time_ms))
}

/// Parse an expression into the finite value kind declared by an extension.
///
/// When the extension property declares a precise `Enum` tooling type, bare
/// variant identifiers (`mode: ring`) and string literals (`mode: "ring"`) are
/// both accepted and validated against the declared variants, mirroring the
/// built-in `ValueType::Enum` path. Without this, a bare `mode: ring` would be
/// mis-read as a variable lookup, fail, and silently drop the property.
pub(crate) fn parse_extension_property_value(
    kind: PropertyValueKind,
    ty: Option<&animatix_syntax::typing::Type>,
    expr: &Expr,
    env: &Environment,
    diagnostics: &mut Vec<Diagnostic>,
    subject: &str,
) -> Option<PropertyValue> {
    if let Some(animatix_syntax::typing::Type::Enum(variants)) = ty {
        let text = match expr {
            Expr::Ident(name) | Expr::Str(name) => name.clone(),
            _ => crate::timeline::evaluate_expr_with_lookup_diagnostic(
                expr,
                env,
                diagnostics,
                subject,
            )?
            .as_str(),
        };
        if variants.iter().any(|variant| variant == &text) {
            return Some(PropertyValue::Enum(text));
        }
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!("'{}' expects one of {}, got '{}'", subject, variants.join(" | "), text),
            )
            .with_subject(subject),
        );
        return None;
    }
    let value =
        crate::timeline::evaluate_expr_with_lookup_diagnostic(expr, env, diagnostics, subject)?;
    let parsed = match kind {
        PropertyValueKind::F32 => match value {
            Value::Num(n) => Some(PropertyValue::F32(n as f32)),
            _ => None,
        },
        PropertyValueKind::U32 => match value {
            Value::Num(n) if n >= 0.0 && n <= u32::MAX as f64 => Some(PropertyValue::U32(n as u32)),
            _ => None,
        },
        PropertyValueKind::Bool => match value {
            Value::Bool(b) => Some(PropertyValue::Bool(b)),
            _ => None,
        },
        PropertyValueKind::Vec2 => match value {
            Value::Vec2(v) => Some(PropertyValue::Vec2([v[0] as f32, v[1] as f32])),
            _ => None,
        },
        PropertyValueKind::Vec4 => match value {
            Value::Vec4(v) | Value::Color(v) => {
                Some(PropertyValue::Vec4([v[0] as f32, v[1] as f32, v[2] as f32, v[3] as f32]))
            },
            _ => None,
        },
        PropertyValueKind::String => match value {
            Value::Str(s) => Some(PropertyValue::String(s)),
            _ => None,
        },
        PropertyValueKind::PointList => match value {
            Value::List(items) => {
                let mut points = Vec::with_capacity(items.len());
                for item in items.iter() {
                    match item {
                        Value::Vec2(v) => points.push([v[0] as f32, v[1] as f32]),
                        _ => return None,
                    }
                }
                Some(PropertyValue::PointList(points))
            },
            _ => None,
        },
        PropertyValueKind::Generic => property_value_from_value(value),
    };
    if parsed.is_none() {
        tracing::warn!(
            "{subject}: extension property expects {:?}, got a value of another kind",
            kind
        );
    }
    parsed
}

/// Write an extension property into the actor plan.
pub(crate) fn write_extension_property_slot(
    track: &mut AnimationTrack,
    ctx: &ExtensionContext,
    actor_type: &str,
    property: &str,
    value: PropertyValue,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) -> bool {
    let Some(spec) = ctx.property_spec(actor_type, property) else {
        return false;
    };
    let value = match (spec.kind, &value) {
        (PropertyValueKind::Vec4, PropertyValue::Color(v)) => PropertyValue::Vec4(*v),
        (PropertyValueKind::F32, PropertyValue::U32(v)) => PropertyValue::F32(*v as f32),
        (PropertyValueKind::U32, PropertyValue::F32(v)) => PropertyValue::U32(v.max(0.0) as u32),
        _ => value,
    };
    write_property_plan_slot(track, spec.id, spec.kind.into(), value, t_start_ms, t_end_ms, easing)
}
