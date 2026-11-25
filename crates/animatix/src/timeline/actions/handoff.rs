//! Lower a same-scene handoff into ordinary transform and opacity tracks.

use super::{ActionTiming, BuiltinInvocation, resolve_action_target};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::easing::Easing;
use crate::primitives::RenderFrame;
use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_registry::ActorField;
use crate::timeline::{DebugRenderOptions, EntityId, SceneDimensions, Timeline, TrackAccessor};

#[derive(Clone)]
pub(crate) struct PendingHandoff {
    proxy: EntityId,
    source: EntityId,
    destination: EntityId,
    timing: ActionTiming,
}

pub(crate) fn queue(
    invocation: &BuiltinInvocation,
    time_ms: f64,
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ids: Option<Vec<_>> = invocation
        .targets
        .iter()
        .map(|label| {
            let resolved = resolve_action_target(timeline, label)?;
            timeline.world.get_id(&resolved)
        })
        .collect();
    let Some([proxy, source, destination]) = ids.as_deref() else {
        diagnostics.push(
            error("expected three declared actors: target, from, and to")
                .with_byte_span(invocation.byte_span),
        );
        return;
    };
    let timing = ActionTiming::parse(invocation, time_ms, diagnostics);
    if timing.t_end_ms <= timing.t_start_ms
        || proxy == source
        || proxy == destination
        || source == destination
    {
        diagnostics.push(
            error("requires a positive duration and three distinct actors")
                .with_byte_span(invocation.byte_span),
        );
        return;
    }
    timeline.pending_handoffs.push(PendingHandoff {
        proxy: *proxy,
        source: *source,
        destination: *destination,
        timing,
    });
}

pub(crate) fn bake(timeline: &mut Timeline, diagnostics: &mut Vec<Diagnostic>) {
    let drafts = std::mem::take(&mut timeline.pending_handoffs);
    if drafts.is_empty() {
        return;
    }
    timeline.world_mut().finish_hierarchy();
    let mut used = std::collections::HashSet::new();
    for draft in drafts {
        let ids = [draft.proxy, draft.source, draft.destination];
        if ids.iter().any(|id| used.contains(id)) {
            diagnostics.push(error("each participating actor can belong to only one handoff"));
            continue;
        }
        match endpoints(timeline, &draft) {
            Ok((start, end)) => {
                write_tracks(timeline, &draft, start, end, diagnostics);
                used.extend(ids);
            },
            Err(message) => diagnostics.push(error(message)),
        }
    }
}

fn endpoints(
    timeline: &Timeline,
    draft: &PendingHandoff,
) -> Result<([f32; 6], [f32; 6]), &'static str> {
    let world = &timeline.world;
    let start = draft.timing.t_start_ms;
    let end = draft.timing.t_end_ms;
    if timeline.camera.is_authored() || Timeline::stmts_address_camera(&timeline.modifiers) {
        return Err("camera-relative layouts are unsupported");
    }
    if world.hierarchy(draft.proxy).parent.is_some() {
        return Err("target must be a root Group");
    }
    if world.has_any_keyframes(draft.proxy) || !world.meta(draft.proxy).visible {
        return Err("target must be a static, visible root Group with opacity 0");
    }
    for (id, opacity) in [
        (draft.proxy, 0.0),
        (draft.source, 1.0),
        (draft.destination, 0.0),
    ] {
        if world.actor_type(id) != "Group"
            || world.first_seen(id) > start
            || !regular_subtree(timeline, id)
            || !fixed_opacity(timeline, id, start, opacity)
        {
            return Err(
                "expected ordinary Group subtrees with fixed opacity: target 0, from 1, to 0",
            );
        }
        let mut parent = world.hierarchy(id).parent;
        while let Some(ancestor) = parent {
            if world.actor_type(ancestor) != "Group"
                || !regular_actor(timeline, ancestor)
                || !fixed_opacity(timeline, ancestor, start, 1.0)
                || [draft.proxy, draft.source, draft.destination].contains(&ancestor)
            {
                return Err(
                    "actors must not contain each other and require ordinary opaque Group ancestors",
                );
            }
            parent = world.hierarchy(ancestor).parent;
        }
    }

    let dimensions = timeline.canvas_dimensions();
    if sample(timeline, draft.proxy, start, dimensions) != kurbo::Affine::IDENTITY {
        return Err("target proxy must begin at the origin with identity transform");
    }
    let from = sample(timeline, draft.source, start, dimensions);
    let to = sample(timeline, draft.destination, end, dimensions);
    match (translation_scale(from), translation_scale(to)) {
        (Some(from), Some(to)) => Ok((from, to)),
        _ => Err("only translation and positive uniform scale are supported"),
    }
}

fn fixed_opacity(timeline: &Timeline, id: EntityId, at: u64, opacity: f32) -> bool {
    let track = &timeline.world.style(id).opacity;
    track.as_ref().is_none_or(|track| track.is_effectively_static())
        && track.get(at, 1.0) == opacity
}

fn regular_actor(timeline: &Timeline, id: EntityId) -> bool {
    let world = &timeline.world;
    world.meta(id).visible
        && !world.geometry(id).isolate_opacity
        && world.style(id).blend.is_none()
        && world.label(id).is_some_and(|label| !timeline.is_targeted_by_modifier(label))
}

fn regular_subtree(timeline: &Timeline, id: EntityId) -> bool {
    let world = &timeline.world;
    matches!(world.actor_type(id), "Group" | "Rect" | "Text")
        && regular_actor(timeline, id)
        && world
            .hierarchy(id)
            .children
            .iter()
            .all(|&child| regular_subtree(timeline, child))
}

fn sample(
    timeline: &Timeline,
    actor: EntityId,
    time_ms: u64,
    scene_dimensions: SceneDimensions,
) -> kurbo::Affine {
    let frame = RenderFrame {
        time_ms,
        scene_dimensions,
        debug_options: DebugRenderOptions::default(),
        overrides: &std::collections::HashMap::new(),
        frame_env: None,
    };
    timeline.actor_point_transform(actor, &frame, false).0
}

fn translation_scale(transform: kurbo::Affine) -> Option<[f32; 6]> {
    let [a, b, c, d, x, y] = transform.as_coeffs();
    let affine = [a, b, c, d, x, y].map(|v| v as f32);
    (affine.iter().all(|value| value.is_finite())
        && a > 0.0
        && b.abs() < 1.0e-8
        && c.abs() < 1.0e-8
        && (a - d).abs() < 1.0e-8)
        .then_some(affine)
}

fn write_tracks(
    timeline: &mut Timeline,
    draft: &PendingHandoff,
    from: [f32; 6],
    to: [f32; 6],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let ActionTiming {
        t_start_ms: start,
        t_end_ms: end,
        easing,
        ..
    } = draft.timing;
    let world = timeline.world_mut();
    world.write_property_field(
        draft.proxy,
        ActorField::Transform,
        PropertyValue::Transform(from),
        start,
        start,
        Easing::Linear,
        diagnostics,
    );
    world.write_property_field(
        draft.proxy,
        ActorField::Transform,
        PropertyValue::Transform(to),
        start,
        end,
        easing,
        diagnostics,
    );
    for (id, at, before, after) in [
        (draft.source, start, 1.0, 0.0),
        (draft.proxy, start, 0.0, 1.0),
        (draft.proxy, end, 1.0, 0.0),
        (draft.destination, end, 0.0, 1.0),
    ] {
        if let Some(guard) = at.checked_sub(1) {
            world.write_property_field(
                id,
                ActorField::Opacity,
                PropertyValue::F32(before),
                guard,
                guard,
                Easing::Linear,
                diagnostics,
            );
        }
        world.write_property_field(
            id,
            ActorField::Opacity,
            PropertyValue::F32(after),
            at,
            at,
            Easing::Linear,
            diagnostics,
        );
    }
}

fn error(message: &str) -> Diagnostic {
    Diagnostic::error(
        DiagnosticCode::InvalidPropertyValue,
        DiagnosticPhase::Build,
        format!("handoff {message}"),
    )
    .with_subject("handoff")
}
