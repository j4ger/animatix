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

/// `None` means "no GPU here, skip" — unless `ANIMATIX_REQUIRE_GPU` is set,
/// in which case the helper panics (see `animatix_render::testing`).
fn new_renderer() -> Option<OffscreenRenderer> {
    match OffscreenRenderer::new() {
        Ok(renderer) => Some(renderer),
        Err(_) => {
            animatix_render::testing::skip_if_no_gpu();
            None
        },
    }
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

/// Regression guard for the video backdrop loss (see `docs/roadmap.md`
/// history): vello's image-atlas residency change (upstream #1558, between
/// revs d8686d52 and 17166312) made any image-bearing vello render draw
/// nothing when a non-image render ran between two image renders. With two
/// GPU filter scopes — an image backdrop and a rect chip — every frame after
/// an invisible (opacity-0) first frame lost the backdrop. The workspace pins
/// vello to `d8686d52` (pre-residency) until upstream regains a fix; this
/// test fails if the pin is moved forward without that fix.
///
/// Five repeated renders must all keep the checker visible (bright samples
/// stay above 500; the failure mode sampled 160 — the red plate alone).
#[test]
fn two_filter_scopes_keep_backdrop_visible_across_repeated_renders() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let asset = concat!(env!("CARGO_MANIFEST_DIR"), "/../../examples/assets/checker.png");
    let source = format!(
        r#"config {{ resolution: (1280, 720) }}

backdrop: Filter, anchor: scene.center {{
  film: Vignette, amount: 0.55, radius: 0.45, softness: 0.7
  panel: Image, url: "{asset}", size: (1280, 720), anchor: scene.center
}}

chip: Filter, anchor: scene.center, offset: (200, 0) {{
  swipe: Vignette, amount: 0.1, radius: 0.9, softness: 0.1
  plate: Rect, size: (150, 44), color: (1, 0.2, 0.2, 1)
}}

#0s
fade-in backdrop [300ms]
fade-in chip [300ms]
"#
    );
    let (ast, errors) = animatix_syntax::parser::parse_source(&source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast.expect("AST"),
        &std::collections::HashMap::new(),
    );
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
    let timeline = report.output;
    let dims = animatix::timeline::SceneDimensions {
        width: 1280,
        height: 720,
    };

    // The t=0 frame renders both scopes fully transparent (fade-in hasn't
    // started) — exactly the history that used to poison the image atlas.
    let first = renderer.render_timeline(&timeline, 0.0, dims).expect("render t=0");
    let bright = first.rgba.chunks_exact(4).step_by(41).filter(|px| px[0] > 60).count();
    assert_eq!(bright, 0, "fade-in must leave the first frame empty");

    for i in 0..5 {
        let frame = renderer.render_timeline(&timeline, 1.0, dims).expect("render");
        let bright = frame.rgba.chunks_exact(4).step_by(41).filter(|px| px[0] > 60).count();
        assert!(
            bright > 500,
            "iteration {i}: checker backdrop lost (bright={bright}; the pre-fix \
             failure mode sampled only the red plate at 160)"
        );
    }
}
