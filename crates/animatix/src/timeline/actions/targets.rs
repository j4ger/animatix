//! Action target resolution and container expansion.

use super::guards::push_unsupported_action_target_diagnostic;
use super::*;

pub(crate) fn resolve_action_target(timeline: &Timeline, target: &str) -> Option<String> {
    // Fast path: direct lookup (handles plain identifiers and pre-built keys)
    if timeline.world.tracks.contains_key(target) {
        return Some(target.to_string());
    }

    // Dotted path: walk hierarchy
    let segments: Vec<&str> = target.split('.').collect();
    if segments.len() < 2 {
        return None;
    }

    let mut current = segments[0].to_string();
    for &segment in &segments[1..] {
        let current_id = timeline.world.get_id(&current)?;
        let next = timeline.world.child_labels(current_id).find(|child| *child == segment)?;
        current = next.to_string();
    }

    Some(current)
}

pub(crate) fn ensure_target_exists(
    timeline: &Timeline,
    target: &str,
    verb: &str,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) -> bool {
    if resolve_action_target(timeline, target).is_some() {
        return true;
    }

    push_unsupported_action_target_diagnostic(
        verb,
        target,
        "the target is not declared yet",
        diagnostics,
        span,
    );
    false
}

/// Expand group targets into their leaf children recursively.
/// A Group is identified by having children in its track. The group itself
/// is skipped; only non-group descendants are returned.
///
/// Container-only actions (reorder, swap) skip expansion — they need
/// to target the container itself.
pub(crate) fn expand_group_targets(
    timeline: &Timeline,
    targets: &[String],
    verb: &str,
) -> Vec<String> {
    // These actions operate on containers, not leaves (or need the root label
    // for metadata/flag purposes — expand would lose the container identity).
    let container_actions = ["reorder", "swap", "persist", "remove"];
    if container_actions.contains(&verb) {
        return targets.to_vec();
    }

    let mut result = Vec::new();
    let mut stack: Vec<String> = targets.to_vec();

    while let Some(label) = stack.pop() {
        // Resolve dotted paths (e.g. "decomp_eq.f1" → "f1") before track lookup
        let resolved = resolve_action_target(timeline, &label).unwrap_or(label.clone());
        if let Some(id) = timeline.world.get_id(&resolved) {
            let children = &timeline.world.hierarchy(id).children;
            if children.is_empty() {
                // Leaf actor — keep the resolved key
                result.push(resolved);
            } else if is_recursive_container(timeline, id) {
                // Container (layout or structural group) — recurse into children
                for child_id in children.iter().rev() {
                    if let Some(child) = timeline.world.label(*child_id) {
                        stack.push(child.to_string());
                    }
                }
            } else {
                // Non-layout container with children (e.g. Graph with tick labels)
                // — keep the container itself as the target
                result.push(resolved);
            }
        } else {
            // Target doesn't exist — pass through and let the action handler report it
            result.push(label);
        }
    }

    result
}

/// Returns true if the actor kind is a container whose children should be
/// expanded by `expand_group_targets`. Equation containers aggregate children
/// into one renderable document and stay leaf-like for action targeting.
pub(super) fn is_recursive_container(timeline: &Timeline, id: crate::timeline::EntityId) -> bool {
    timeline
        .primitive_registry
        .info_of(timeline.world.actor_type(id))
        .map(crate::timeline::PrimitiveFamilyDescriptor::from_info)
        .is_some_and(|descriptor| descriptor.is_recursive_container())
}

pub(crate) fn ensure_vector_reveal_target(
    timeline: &Timeline,
    target: &str,
    verb: &str,
    diagnostics: &mut Vec<Diagnostic>,
    span: Option<crate::ast::Span>,
) -> bool {
    let Some(id) = timeline.world.get_id(target) else {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "the target is not declared yet",
            diagnostics,
            span,
        );
        return false;
    };

    let capabilities = timeline.primitive_registry.info_of(timeline.world.actor_type(id));

    if capabilities.is_some_and(|info| info.capabilities.image_payload) {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "image targets only support opacity-based actions right now",
            diagnostics,
            span,
        );
        return false;
    }

    if timeline.world.image_track(id).and_then(|t| t.last_value()).is_some() {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "image targets only support opacity-based actions right now",
            diagnostics,
            span,
        );
        return false;
    }

    // Text/Code/Typst targets are now allowed; draw-in uses char_progress for typewriter effect.

    if capabilities
        .is_some_and(|info| info.capabilities.is_container || info.capabilities.layout_container)
        || (timeline.world.get_id(target).is_some_and(|id| {
            !timeline.world.hierarchy(id).children.is_empty()
                && timeline
                    .world
                    .shape(id)
                    .and_then(|s| s.vector_paths.as_ref())
                    .map(|t| t.default_value.is_empty() && t.keyframes.is_empty())
                    .unwrap_or(true)
                && !timeline.world.has_svg_path_content(id)
        }))
    {
        push_unsupported_action_target_diagnostic(
            verb,
            target,
            "the target resolves to a container/group node rather than a renderable leaf; vector reveal actions must target leaf actors with vector paths",
            diagnostics,
            span,
        );
        return false;
    }

    true
}

pub(super) fn lift_container_and_intermediate_containers(
    timeline: &mut Timeline,
    label: &str,
    t_start_ms: u64,
    t_end_ms: u64,
    easing: Easing,
) {
    use crate::timeline::property_track::TrackAccessor;
    let resolved = resolve_action_target(timeline, label).unwrap_or_else(|| label.to_string());
    let children: Vec<String> = timeline.child_labels(&resolved).map(str::to_string).collect();
    if !children.is_empty() {
        if let Some(id) = timeline.world.get_id(&resolved) {
            let style = timeline.world.style.get_mut(id);
            let meta = timeline
                .world
                .meta
                .get_or_insert_with(id, crate::timeline::world::EntityMeta::default);
            let was_hidden = meta.hidden_by_default;
            lift_hidden_by_default(style, meta, t_start_ms, t_end_ms, easing);
            if !was_hidden {
                let authored = style.opacity.get(t_start_ms, 1.0);
                if authored == 0.0 {
                    reveal_authored_zero_opacity(style, t_start_ms, t_end_ms, easing);
                }
            }
        }
        for child in children {
            lift_container_and_intermediate_containers(
                timeline, &child, t_start_ms, t_end_ms, easing,
            );
        }
    }
}
