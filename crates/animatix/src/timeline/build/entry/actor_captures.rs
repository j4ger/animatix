//! Freeze actor handles inside plot captures after the build environment is
//! bound to the completed World.

use std::collections::HashMap;

use crate::timeline::Timeline;
use crate::timeline::env::{
    CapturedEnv, Environment, Value, rebind_actor_value, rebind_captured_env,
};
use crate::timeline::plan_value::ActorRef;
use crate::timeline::plot::FuncSource;

/// Rebind actor references captured by procedural plots and function
/// transitions to the canonical handles already frozen into the environment.
///
/// This runs once during build finalization. It deliberately does not resolve
/// actor names from the frame path; frame sampling receives the bound handles
/// stored in these captures.
pub(super) fn rebind_plot_actor_captures(timeline: &mut Timeline) {
    let mut bindings = HashMap::new();
    let mut affected_plots = Vec::new();
    let mut affected_variables = Vec::new();
    {
        let world = timeline.world();
        for (_, id) in world.tracks.iter() {
            let Some(payload) = world.plot_payload(id) else {
                continue;
            };
            let mut changed = false;
            if let Some(plot) = &payload.procedural_plot {
                changed |= collect_env_bindings(&plot.extra_captures, &timeline.env, &mut bindings);
            }
            for transition in &payload.func_transitions {
                changed |= collect_source_bindings(&transition.from, &timeline.env, &mut bindings);
                changed |= collect_source_bindings(&transition.to, &timeline.env, &mut bindings);
            }
            if changed {
                affected_plots.push(id);
            }
        }
        for (name, track) in &timeline.variable_tracks {
            for (&time_ms, value) in &track.keyframes {
                if collect_value_bindings(value, &timeline.env, &mut bindings) {
                    affected_variables.push((name.clone(), time_ms));
                }
            }
        }
    }

    if affected_plots.is_empty() && affected_variables.is_empty() {
        return;
    }

    let plot_write_invalidates = !affected_plots.is_empty();
    if plot_write_invalidates {
        // One write access invalidates the frame cache once for all plot payloads.
        let world = timeline.world_mut();
        for id in affected_plots {
            let Some(payload) = world.plot_payload_mut(id) else {
                continue;
            };
            if let Some(plot) = &mut payload.procedural_plot {
                rebind_env(&mut plot.extra_captures, &bindings);
            }
            for transition in &mut payload.func_transitions {
                rebind_source(&mut transition.from, &bindings);
                rebind_source(&mut transition.to, &bindings);
            }
        }
    }

    if !affected_variables.is_empty() {
        // Variable tracks are not World components, so invalidate explicitly
        // only when the plot payload write above did not already do so.
        if !plot_write_invalidates {
            timeline.invalidate_frame_cache();
        }
        for (name, time_ms) in affected_variables {
            let Some(value) = timeline
                .variable_tracks
                .get_mut(&name)
                .and_then(|track| track.keyframes.get_mut(&time_ms))
            else {
                continue;
            };
            rebind_value(value, &bindings);
        }
    }
}

fn collect_source_bindings(
    source: &FuncSource,
    env: &Environment,
    bindings: &mut HashMap<String, ActorRef>,
) -> bool {
    match source {
        FuncSource::Compiled(_, _, captures) => collect_env_bindings(captures, env, bindings),
        FuncSource::Blend { from, to, .. } => {
            let from_changed = collect_source_bindings(from, env, bindings);
            let to_changed = collect_source_bindings(to, env, bindings);
            from_changed || to_changed
        },
    }
}

fn collect_env_bindings(
    captures: &CapturedEnv,
    env: &Environment,
    bindings: &mut HashMap<String, ActorRef>,
) -> bool {
    let mut found = false;
    for value in captures.0.values() {
        found |= collect_value_bindings(value, env, bindings);
    }
    found
}

fn collect_value_bindings(
    value: &Value,
    env: &Environment,
    bindings: &mut HashMap<String, ActorRef>,
) -> bool {
    match value {
        Value::ActorRef(actor) => {
            if bindings.contains_key(actor.label()) {
                return true;
            }
            let Some(Value::ActorRef(bound)) = env.get(actor.label()) else {
                return false;
            };
            bindings.insert(actor.label().to_owned(), bound);
            true
        },
        Value::List(items) => {
            let mut found = false;
            for item in items.iter() {
                found |= collect_value_bindings(item, env, bindings);
            }
            found
        },
        Value::Object(_, fields) => {
            let mut found = false;
            for value in fields.values() {
                found |= collect_value_bindings(value, env, bindings);
            }
            found
        },
        Value::Closure(_, _, captures) => collect_env_bindings(captures, env, bindings),
        _ => false,
    }
}

fn rebind_source(source: &mut FuncSource, bindings: &HashMap<String, ActorRef>) -> bool {
    match source {
        FuncSource::Compiled(_, _, captures) => rebind_env(captures, bindings),
        FuncSource::Blend { from, to, .. } => {
            let from_changed = rebind_source(from, bindings);
            let to_changed = rebind_source(to, bindings);
            from_changed || to_changed
        },
    }
}

fn rebind_env(captures: &mut CapturedEnv, bindings: &HashMap<String, ActorRef>) -> bool {
    let Some(updated) = rebind_captured_env(captures, bindings) else {
        return false;
    };
    *captures = updated;
    true
}

fn rebind_value(value: &mut Value, bindings: &HashMap<String, ActorRef>) -> bool {
    let Some(updated) = rebind_actor_value(value, bindings) else {
        return false;
    };
    *value = updated;
    true
}
