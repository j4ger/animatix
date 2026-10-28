//! Post-walk validation passes.
//!
//! Each pass only reads the lowered timeline (except the scene-statistics bake)
//! and emits warnings/errors for states that would otherwise fail silently at
//! playback: actors nothing ever reveals, a declared duration shorter than the
//! content, a loop seam that jumps, unregistered primitives, and `always`
//! writes that cannot land where the author expects.

use super::*;

/// Warn about actors that were seeded hidden and never revealed.
///
/// Tracks still flagged `hidden_by_default` at the end of the build were seeded
/// hidden and no entrance action (fade-in, wipe-in, ...) ever lifted them —
/// they will never be visible. The flag is maintained by
/// `lift_hidden_by_default`, which every entrance action routes through.
pub(super) fn validate_reveals(timeline: &Timeline, diagnostics: &mut Vec<Diagnostic>) {
    // Generated sub-actors (graph ticks, bar labels) render through
    // their parent primitive, so an ancestor's reveal counts as their
    // own. `parent_of` is derived from the children lists rather than
    // trusting the stored `parent` back-reference.
    // An ancestor counts as revealing only when its opacity genuinely
    // ramps 0 → positive (an entrance window). A declaration-time
    // constant keyframe — visible-by-default containers carry one — is
    // not a reveal: counting it suppressed warnings for hidden Graph
    // children, whose own seeded 0 multiplies the ancestor's opacity to
    // zero regardless (probe 010).
    let parent_of = timeline.parent_of();
    let opacity_reveals = |pt: &crate::timeline::PropertyTrack<f32>| -> bool {
        let mut saw_zero_at: Option<u64> = None;
        for (t, (v, _)) in pt.keyframes.iter() {
            if *v <= 0.0 {
                saw_zero_at.get_or_insert(*t);
            } else if saw_zero_at.is_some_and(|t0| *t > t0) {
                return true;
            }
        }
        false
    };
    let revealed_via_ancestor = |label: &str| -> bool {
        let mut current = parent_of.get(label).copied();
        while let Some(parent) = current {
            if let Some(pt) = timeline
                .world
                .get_id(parent)
                .and_then(|id| timeline.world.style(id).opacity.as_ref())
            {
                if opacity_reveals(pt) {
                    return true;
                }
            }
            current = parent_of.get(parent).copied();
        }
        false
    };
    // Generated sub-actors (graph tick labels, bar labels) render through
    // their parent primitive and are not individually targetable, so they
    // follow the parent's entrance by construction and never warn.
    let is_generated_sub = |label: &str| -> bool {
        label.contains("_tick_x_") || label.contains("_tick_y_") || label.contains("_bar_label_")
    };
    for (label, id) in timeline.world.tracks.iter() {
        if !timeline.world.meta.get(id).is_some_and(|m| m.hidden_by_default) {
            continue;
        }
        if is_generated_sub(label)
            || revealed_via_ancestor(label)
            || timeline.world.style(id).opacity.as_ref().is_some_and(opacity_reveals)
        {
            continue;
        }
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::NeverRevealed,
                DiagnosticPhase::Build,
                format!(
                    "Actor `{label}` is declared before any keyframe (implicit `opacity: 0.0`) and \
                     is never revealed by an entrance action or opacity assignment, so it will \
                     remain invisible throughout playback. Add an entrance action (e.g. \
                     `fade-in {label}`), declare it inside a keyframe block, or set `opacity: 1.0` \
                     explicitly on its declaration."
                ),
            )
            .with_subject(label)
            .with_ast_span(timeline.world.decl_span(id).copied()),
        );
    }
}

/// Bake scene statistics, then check the declared duration and the loop seam.
///
/// A declared duration overrides the keyframe-inferred length, so anything
/// authored past it is never played. That truncation is the documented
/// contract (`docs/spec.md`, "overrides keyframe-inferred duration") and the
/// composition path has always done it per scene, but doing it silently would
/// strand authored animation with no feedback — say so instead.
pub(super) fn validate_duration_and_seam(
    timeline: &mut Timeline,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(declared_s) = timeline.declared_duration_s {
        let content_s = timeline.duration_seconds();
        if content_s > declared_s {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::DurationShorterThanContent,
                    DiagnosticPhase::Build,
                    format!(
                        "`config {{ duration: {declared_s}s }}` is shorter than this \
                         timeline's last keyframe at {content_s:.2}s. A declared duration \
                         overrides the inferred one, so everything after {declared_s}s is \
                         never played — raise the duration or move the animation earlier."
                    ),
                )
                .with_subject("duration"),
            );
        }
    }

    // Bake scene statistics before the loop seam check and before env_base is
    // frozen.
    if timeline.stats_used {
        crate::timeline::scene_stats::bake_scene_stats(timeline);
    }

    // Loop seam check. A scene that declares `config { seamless_loop: true }`
    // is replayed end-to-start, so any keyframed value that differs between
    // the first and last frame shows up as a jump every cycle — the single
    // most common way a "looping" scene fails to loop.
    //
    // v1 scope, deliberately: only *keyframed* values are sampled. What an
    // `always` block computes, and a plot `func` transition, are frame-time
    // functions rather than tracks, so the build cannot see them and does
    // not lint them (a sine of the frame time wraps by construction when the
    // period divides the scene; only the author can know that).
    if !timeline.seamless_loop {
        return;
    }
    let end_ms = (timeline.duration_seconds() * 1000.0).round() as u64;
    if end_ms == 0 {
        return;
    }
    let mut broken: Vec<String> = Vec::new();
    for (label, id) in timeline.world.tracks.iter() {
        for idx in crate::timeline::property_registry::allowed_property_indices(
            &timeline.world.caps(id),
            timeline.world.actor_type(id),
        ) {
            let schema = &crate::timeline::property_registry::PROPERTY_REGISTRY[idx];
            if !timeline.world.property_has_keyframes(id, schema.field) {
                continue;
            }
            let first = timeline.world.read_property_value(id, schema.field, 0);
            let last = timeline.world.read_property_value(id, schema.field, end_ms);
            let wraps = match (&first, &last) {
                (Some(a), Some(b)) => crate::timeline::values_wrap(a, b),
                // One side never sampled: the property is constant
                // across the scene, which wraps trivially.
                _ => true,
            };
            if !wraps {
                broken.push(format!("`{label}.{}`", schema.name));
            }
        }
    }
    // The camera is not a track, so the actor walk above cannot see
    // it — and a scene that ends pushed in and restarts unzoomed
    // jumps at the seam exactly like any other un-wrapped value.
    if timeline.camera.is_authored() {
        for (axis, first, last) in timeline.camera.seam_pairs(0, end_ms) {
            if !crate::timeline::values_wrap(&first, &last) {
                broken.push(format!("`{axis}`"));
            }
        }
    }
    // Baked scene statistics are data, so check that they wrap cleanly.
    if timeline.stats_used {
        for key in [
            "scene.stats.motion",
            "scene.stats.ink",
            "scene.stats.focus_x",
            "scene.stats.focus_y",
            "scene.stats.spread_x",
            "scene.stats.spread_y",
            "scene.stats.cast",
        ] {
            if let Some(val) = timeline.env.get(key) {
                let first = crate::timeline::eval_shared::eval_builtin_fn(
                    "curve_at",
                    &[val.clone(), Value::Num(0.0)],
                );
                let last = crate::timeline::eval_shared::eval_builtin_fn(
                    "curve_at",
                    &[val.clone(), Value::Num(timeline.duration_seconds())],
                );
                if let (Ok(f), Ok(l)) = (first, last) {
                    if (f.as_num() - l.as_num()).abs() > 1e-3 {
                        broken.push(format!("`{key}`"));
                    }
                }
            }
        }
    }
    if broken.is_empty() {
        return;
    }
    let shown = if broken.len() > 6 {
        format!("{} ({} in total)", broken[..6].join(", "), broken.len())
    } else {
        broken.join(", ")
    };
    diagnostics.push(
        Diagnostic::warning(
            DiagnosticCode::LoopNotSeamless,
            DiagnosticPhase::Build,
            format!(
                "`config {{ seamless_loop: true }}` but these keyframed values \
                 differ between the first frame and the last ({shown}), so the \
                 replay jumps at the seam. Give each one the same value at both \
                 ends, or drop the declaration."
            ),
        )
        .with_subject("seamless_loop"),
    );
}

/// Check primitive registration and `always` writes that cannot take effect.
///
/// Every track's primitive identity must resolve: identity is required when the
/// entity is spawned, so a miss means the declared primitive is not registered
/// (typically an extension whose plugin is not loaded). Warn once here rather
/// than silently rendering nothing every frame.
///
/// The `always` half warns for a per-frame write that the modifier runtime has
/// nowhere to put (`solo` is assignable but deliberately not animatable) and
/// for a write over a property that also has keyframes.
pub(super) fn validate_actors(timeline: &mut Timeline, diagnostics: &mut Vec<Diagnostic>) {
    for (label, id) in timeline.world.tracks.iter() {
        let actor_type = timeline.world.actor_type(id);
        if timeline.primitive_registry.find(actor_type).is_none() {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::UnknownActorType,
                    DiagnosticPhase::Build,
                    format!(
                        "Actor `{label}` is type `{actor_type}`, which is not a registered \
                         primitive; it will not render. Load the extension that provides it."
                    ),
                )
                .with_subject(label),
            );
        }
    }

    // Check for always-blocks overriding keyframed properties.
    for stmt in &timeline.modifiers {
        if let crate::ast::Stmt::Assignment {
            target,
            property,
            span,
            ..
        } = stmt
        {
            let actor_key = crate::timeline::assignment_target_key(target);
            if let Some(id) = timeline.world.get_id(&actor_key) {
                let not_animatable = crate::timeline::property_registry::lookup_property(property)
                    .is_some_and(|schema| {
                        !schema
                            .flags
                            .contains(crate::timeline::property_registry::PropertyFlags::ANIMATED)
                    });
                if not_animatable {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::AlwaysWriteNotAnimatable,
                            DiagnosticPhase::Build,
                            format!(
                                "Always block writes `{property}` on actor `{actor_key}`, which \
                                 is not animatable, so the per-frame write is ignored. Set it \
                                 in a declaration or a keyframe assignment instead."
                            ),
                        )
                        .with_subject(property),
                    );
                } else if timeline.world.is_property_animated(id, property) {
                    // Only a property that actually *moves* is worth warning
                    // about: the build seeds a constant keyframe for most
                    // declared properties, and a frame-time write over a
                    // value that never changes silently overrides nothing.
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::AlwaysOverridesKeyframes,
                            DiagnosticPhase::Build,
                            format!(
                                "Always block writes to `{}` on actor `{}`, which also has keyframe animation. The always value will silently override keyframes every frame.",
                                property, actor_key
                            ),
                        )
                        .with_subject(property)
                        .with_ast_span(*span),
                    );
                }
            }
        }
    }
}
