//! Build phase 4 — finalize derived state.
//!
//! Everything here runs after validation because it either consumes the whole
//! world (legend entries, idle tracks, pause points) or freezes state the frame
//! path reads (the base environment, hierarchy links, audio envelopes).

use super::*;

/// Freeze the build environment and populate the derived, post-build state.
///
/// Order matters: the environment is frozen before the read-only post-build
/// audits so nothing below can reintroduce env entries, legend entries are
/// collected before idle tracks could add generated actors, and hierarchy links
/// are resolved last so carry-injected tracks join the tree.
pub(super) fn finalize_assets_legend(timeline: &mut Timeline, diagnostics: &mut Vec<Diagnostic>) {
    // P2.22: Freeze the base environment into an Arc for cheap sharing.
    // After build, env is stable; build_frame_env will reference this Arc
    // instead of copying all entries.
    timeline.env_base = std::sync::Arc::new(std::mem::take(&mut timeline.env.overrides));
    // Restore env with the frozen base layer
    timeline.env.base = Some(std::sync::Arc::clone(&timeline.env_base));

    // Validate Callout `target` references after all actors are built.
    // This is a post-build pass so forward declarations are visible.
    for (label, id) in timeline.world.tracks.iter() {
        if timeline.world.caps(id).callout {
            use crate::timeline::TrackAccessor;
            let target = timeline.world.geometry(id).callout_target.get(0, String::new());
            if !target.is_empty() && !timeline.world.tracks.contains_key(&target) {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::CalloutTargetNotFound,
                        DiagnosticPhase::Build,
                        format!(
                            "Callout '{}' has target '{}', but no actor with that label exists.",
                            label, target
                        ),
                    )
                    .with_subject(label),
                );
            }
        }
    }

    // `camera_follow` is applied where the camera is applied — once, to each
    // root subtree (see the camera block in `evaluate_scene`). On a nested
    // actor it would read as a HUD that keeps moving, so say now that the
    // declaration has no effect instead of leaving it to be discovered in a
    // render.
    for (label, id) in timeline.world.tracks.iter() {
        let meta = timeline.world.meta.get(id);
        let camera_follow = meta.is_none_or(|m| m.camera_follow);
        let parallax = meta.map_or(1.0, |m| m.parallax);
        if !camera_follow && !timeline.world.is_root(id) {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InapplicableProperty,
                    DiagnosticPhase::Build,
                    format!(
                        "'{label}' declares `camera_follow: false` but is not a root actor, so \
                         the scene camera still moves it with its parent; move it to the top \
                         level of the scene to pin it to the viewport."
                    ),
                )
                .with_subject(label),
            );
        } else if parallax != 1.0 && !timeline.world.is_root(id) {
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::InapplicableProperty,
                    DiagnosticPhase::Build,
                    format!(
                        "'{label}' declares `parallax: {}` but is not a root actor, so \
                         the scene camera moves it with its parent; move it to the top \
                         level of the scene to apply parallax.",
                        parallax
                    ),
                )
                .with_subject(label),
            );
        }
    }

    // Populate Legend entries after every actor is built so generated,
    // forward-declared and carry-injected actors participate in the same
    // scan. Entries are derived data: they live on the entity's legend
    // component and are never carried across scenes.
    let legend_ids = timeline
        .world
        .tracks
        .iter()
        .filter(|&(_, id)| timeline.world.caps(id).legend_host)
        .map(|(_, id)| id)
        .collect::<Vec<_>>();
    if !legend_ids.is_empty() {
        let entries = crate::timeline::legend::scan_legend_entries(&timeline.world);
        for id in legend_ids {
            timeline.world.ensure_legend(id).entries = entries.clone();
        }
    }

    // Drop cached payloads that are no longer referenced by this build.
    let referenced: std::collections::HashSet<String> =
        timeline.asset_cache.asset_usage().map(|(path, _)| path.clone()).collect();
    std::sync::Arc::make_mut(&mut timeline.asset_cache).prune_unreferenced(&referenced);

    // Declarative idle micro-motions (Wave 3.3): synthesize seamless sinusoidal
    // tracks at build time for actors declaring `idle: "float"` or
    // `idle: "breath"`.
    synthesize_idle_tracks(timeline);

    timeline.pause_points.sort_unstable();
    timeline.pause_points.dedup();

    // Whether anything here could consult the filter backend at all. The
    // frame cache is gated on this rather than on the backend's presence, so
    // a runtime that always supplies one (the web player) is not locked out
    // of caching for scenes that contain no effects.
    timeline.has_effect_scopes = timeline
        .world
        .tracks
        .iter()
        .any(|(_, id)| timeline.world.caps(id).is_effect_scope());
    timeline.refresh_blend_used();
    timeline.refresh_camera_used();
    // Resolve any hierarchy link whose child track was created by a path
    // that bypassed `World::spawn` (carry injection, direct inserts).
    timeline.world.finish_hierarchy();
    timeline.resolve_audio_volume_envelopes();
}
