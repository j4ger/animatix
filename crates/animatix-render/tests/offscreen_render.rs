//! End-to-end offscreen rendering through the public `animatix-render` API.
//!
//! Fixtures are built with the full source pipeline (parse → build) so these
//! tests exercise exactly what an embedder does, and they live here rather
//! than in the engine crate: a dev-dependency from `animatix` back to
//! `animatix-render` would build the engine twice and the types would no
//! longer unify.

use animatix::timeline::{DebugRenderOptions, SceneDimensions, Timeline};
use animatix_render::offscreen::OffscreenRenderer;

/// Minimal solid-rect scene parsed from source.
fn solid_rect_timeline() -> Timeline {
    let source = r#"
#0s
r: Rect, at: (50, 50), size: (100, 100), color: (1, 1, 1, 1)
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let ast = ast.expect("AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
    report.output
}

fn new_renderer() -> Option<OffscreenRenderer> {
    OffscreenRenderer::new().ok()
}

#[test]
fn offscreen_renderer_render_timeline_produces_frame() {
    let Some(mut renderer) = new_renderer() else {
        return; // Skip if no GPU
    };

    let timeline = solid_rect_timeline();
    let dimensions = SceneDimensions {
        width: 100,
        height: 100,
    };
    let frame = renderer
        .render_timeline(&timeline, 0.0, dimensions)
        .expect("render should produce a frame");
    assert_eq!(frame.rgba.len(), (dimensions.width * dimensions.height * 4) as usize);
}

/// The parked buffer must not be handed to two callers at once: holding the
/// previous frame alive forces the next readback onto a fresh allocation
/// (never a shared mutation).
#[test]
fn held_frame_forces_fresh_allocation_not_shared_mutation() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let timeline = solid_rect_timeline();
    let dims = SceneDimensions {
        width: 160,
        height: 120,
    };

    let held = renderer.render_timeline(&timeline, 0.0, dims).expect("frame 1");
    let next = renderer.render_timeline(&timeline, 0.0, dims).expect("frame 2");
    assert!(
        !std::sync::Arc::ptr_eq(&held.rgba, &next.rgba),
        "a held frame must force a new buffer, not share the parked one"
    );
    // And the held frame's pixels are untouched by the second render.
    let mid = ((60 * held.width + 80) * 4) as usize;
    assert!(held.rgba[mid] > 200, "held frame was clobbered by the next render");
}

/// PF-7: the pipelined begin/wait pair must produce pixels identical to the
/// blocking path, across the rotating buffer pair (frame A in slot 0, frame B
/// in slot 1, frame C back in slot 0 after A was unmapped).
#[test]
fn pipelined_frames_match_blocking_path() {
    let Some(mut pipelined) = new_renderer() else {
        return;
    };
    let Some(mut blocking) = new_renderer() else {
        return;
    };
    let timeline = solid_rect_timeline();
    let dims = SceneDimensions {
        width: 200,
        height: 160,
    };

    // Three in-flight frames exercise the slot rotation.
    let pending_a = pipelined
        .begin_frame_with_debug(&timeline, 0.0, dims, DebugRenderOptions::default())
        .expect("begin a");
    let pending_b = pipelined
        .begin_frame_with_debug(&timeline, 0.0, dims, DebugRenderOptions::default())
        .expect("begin b");
    let frame_a = pipelined.wait_frame(pending_a).expect("wait a");
    let pending_c = pipelined
        .begin_frame_with_debug(&timeline, 0.0, dims, DebugRenderOptions::default())
        .expect("begin c");
    let frame_b = pipelined.wait_frame(pending_b).expect("wait b");
    let frame_c = pipelined.wait_frame(pending_c).expect("wait c");

    let reference = blocking.render_timeline(&timeline, 0.0, dims).expect("blocking");
    for (name, frame) in [("a", &frame_a), ("b", &frame_b), ("c", &frame_c)] {
        assert_eq!(frame.rgba.as_ref(), reference.rgba.as_ref(), "frame {name} diverged");
    }
}
