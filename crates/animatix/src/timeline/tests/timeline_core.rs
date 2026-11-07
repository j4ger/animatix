//! Core timeline build, cache-invalidation, and content-lint tests.

use super::support::build;
use super::*;

/// Content-lint warnings (`never-revealed`) fire on minimal fixtures whose
/// actors have no entrance actions — they are about demo content, not the
/// feature under test, so assertions exclude them.
pub(crate) fn without_content_lints(
    diagnostics: &[animatix_syntax::diagnostics::Diagnostic],
) -> impl Iterator<Item = &animatix_syntax::diagnostics::Diagnostic> {
    diagnostics
        .iter()
        .filter(|d| !matches!(d.code, animatix_syntax::diagnostics::DiagnosticCode::NeverRevealed))
}

#[test]
fn property_descriptors_include_shared_builtin_schema() {
    let report = build("");
    let descriptors = report.output.property_descriptors();
    assert!(
        descriptors.iter().any(|descriptor| descriptor.name == "size"),
        "built-in property descriptors should be exposed through the unified view"
    );
    assert!(
        descriptors
            .iter()
            .any(|descriptor| descriptor.actor_types.contains(&"Rect".to_string())),
        "unified descriptors should carry actor applicability"
    );
}

#[test]
fn entity_mut_invalidates_frame_cache() {
    let mut timeline = build(
        r#"
#0s
box0: Rect, size: (50, 50)
"#,
    )
    .output;
    let dims = SceneDimensions {
        width: 100,
        height: 100,
    };
    let _scene = timeline.evaluate(0.0, dims);
    assert!(timeline.eval_caches.frame_cache.borrow().is_some());
    let _entity = timeline.entity_by_id_mut(timeline.get_entity_id("box0").unwrap());
    assert!(
        timeline.eval_caches.frame_cache.borrow().is_none(),
        "entity_by_id_mut should invalidate the frame cache"
    );
}

#[test]
fn invalidation_clears_hit_regions() {
    let mut timeline = build(
        r#"
#0s
box0: Rect, size: (50, 50)
"#,
    )
    .output;
    let dims = SceneDimensions {
        width: 100,
        height: 100,
    };
    let mut backend = None;
    let _ = timeline.evaluate_with_debug(
        0.0,
        dims,
        DebugRenderOptions {
            compute_hit_regions: true,
            ..Default::default()
        },
        &mut backend,
    );
    assert!(
        !timeline.hit_regions().is_empty(),
        "fixture must produce hit regions before the edit"
    );
    let _entity = timeline.entity_by_id_mut(timeline.get_entity_id("box0").unwrap());
    assert!(timeline.hit_regions().is_empty(), "stale hit regions must not survive an edit");
}

#[test]
fn test_animated_scene_has_keyframes() {
    let report = build(
        r#"
config { colorscheme: "editorial-dark" }
#0s
box0: Rect, size: (50, 50), color: accent.primary, at: (100, 100)
#5s
box0.position = (200, 200)
box0.color = accent.success
box0.opacity = 0.5
"#,
    );
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let track = timeline.get_entity_id("box0").expect("box0 should exist");
    assert!(timeline.world().has_any_keyframes(track), "box0 should have animated keyframes");
    assert!(
        timeline
            .world()
            .geometry(track)
            .position
            .as_ref()
            .map(|t| t.keyframes.len())
            .unwrap_or(0)
            >= 2,
        "position should have at least 2 keyframes"
    );
    let style = timeline.world().style(track);
    assert!(
        style.color.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) >= 2,
        "color should have at least 2 keyframes"
    );
    assert!(
        style.opacity.as_ref().map(|t| t.keyframes.len()).unwrap_or(0) >= 2,
        "opacity should have at least 2 keyframes"
    );
}

#[test]
fn test_step_pause_points_and_navigation() {
    let source = r#"
#0s
b: Rect, at: (0, 0), size: (10, 10), color: (1, 1, 1, 1)
#step 1

#500ms
#pause

#1s
#step 2
"#;
    let report = build(source);
    let non_lints: Vec<_> = without_content_lints(&report.diagnostics).collect();
    assert!(non_lints.is_empty(), "unexpected diagnostics: {non_lints:?}");

    let timeline = report.output;
    assert_eq!(timeline.pause_points(), &[0, 500, 1000]);

    // Navigation checks
    assert_eq!(timeline.step_next(0), Some(500));
    assert_eq!(timeline.step_next(250), Some(500));
    assert_eq!(timeline.step_next(500), Some(1000));
    assert_eq!(timeline.step_next(1000), None);

    assert_eq!(timeline.step_prev(1000), Some(500));
    assert_eq!(timeline.step_prev(750), Some(500));
    assert_eq!(timeline.step_prev(500), Some(0));
    assert_eq!(timeline.step_prev(0), None);
}
