use super::*;

mod readback_reuse_tests {
    use super::*;

    /// Fixture built through the source pipeline so these tests exercise only
    /// public engine API (the render crate has no access to engine internals).
    fn solid_rect_timeline() -> Timeline {
        let source = r#"
#0s
r: Rect, at: (50, 50), size: (100, 100), color: (1, 1, 1, 1)
"#;
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let ast = ast.expect("AST");
        let report = animatix::timeline::Timeline::build_with_diagnostics(
            &ast,
            &std::collections::HashMap::new(),
        );
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        report.output
    }

    /// PF-6 round 9: the readback buffer is parked and reused. The second
    /// render from the same renderer walks the reuse path — its pixels must
    /// be byte-identical to a fresh renderer's (no stale-buffer bleed, no
    /// unzeroed padding), and its `Arc` must share the renderer's parked
    /// buffer while the caller holds it (the fallback only fires when a
    /// consumer keeps the previous frame alive).
    #[test]
    fn readback_reuse_is_pixel_identical_to_fresh_renderer() {
        let mut reused = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };
        let mut fresh = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };
        let timeline = solid_rect_timeline();
        let dims = SceneDimensions {
            width: 320,
            height: 240,
        };

        // Frame 1 primes the parked buffer; frame 2 takes the reuse path.
        let _ = reused.render_timeline(&timeline, 0.0, dims).expect("frame 1");
        let second = reused.render_timeline(&timeline, 0.0, dims).expect("frame 2");
        let reference = fresh.render_timeline(&timeline, 0.0, dims).expect("reference");

        // The reuse path actually ran: frame 2's buffer IS the parked one.
        let parked = reused.readback_buffer.as_ref().expect("parked buffer");
        assert!(
            std::sync::Arc::ptr_eq(parked, &second.rgba),
            "frame 2 should share the renderer's parked readback buffer"
        );

        assert_eq!(second.rgba.len(), reference.rgba.len());
        assert!(
            second.rgba.as_ref() == reference.rgba.as_ref(),
            "reused-readback frame diverged from a fresh renderer's output"
        );
    }

    /// The parked buffer must not be handed to two callers at once: holding
    /// the previous frame alive forces the next readback onto a fresh
    /// allocation (never a shared mutation).
    #[test]
    fn held_frame_forces_fresh_allocation_not_shared_mutation() {
        let mut renderer = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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

    /// PF-7: the pipelined begin/wait pair must produce pixels identical to
    /// the blocking path, across the rotating buffer pair (frame A in slot 0,
    /// frame B in slot 1, frame C back in slot 0 after A was unmapped).
    #[test]
    fn pipelined_frames_match_blocking_path() {
        let mut pipelined = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
        };
        let mut blocking = match OffscreenRenderer::new() {
            Ok(r) => r,
            Err(_) => {
                crate::testing::skip_if_no_gpu();

                return;
            },
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
}

#[test]
fn offscreen_renderer_can_be_initialized() {
    // OffscreenRenderer::new() uses pollster::block_on internally.
    // In headless/CI environments this may succeed or fail gracefully.
    let result = OffscreenRenderer::new();
    // Should either succeed (GPU available) or fail with a GPU error (no adapter).
    // We just verify it doesn't panic.
    if let Ok(_renderer) = result {
        // Initialization succeeded — renderer is valid
    }
}

#[test]
fn offscreen_renderer_with_zero_dimensions_fails_gracefully() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    let timeline = Timeline::new();
    let result = renderer.render_timeline(
        &timeline,
        0.0,
        SceneDimensions {
            width: 0,
            height: 0,
        },
    );
    assert!(result.is_err(), "zero dimensions should error");
    assert!(result.unwrap_err().contains("greater than zero"), "should give clear error");
}

/// Derived region of interest: a `Filter` with no authored `bounds` must
/// still keep its content — the region is derived from the content bounds
/// recorded during the sub-scene evaluation and padded by the chain's
/// support. (Regression guard for the derived-ROI crop: a bug here would
/// clip or blank the filtered content.)
#[test]
fn derived_effect_region_preserves_content() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    let source = r#"
config { resolution: (400, 300) }

#0s
fx: Filter {
  soft: Blur, radius: 4
  box: Rect, size: (120, 120), color: (1, 1, 1, 1), at: (0, 0)
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;

    let frame = renderer
        .render_timeline(
            &timeline,
            0.0,
            animatix::timeline::SceneDimensions {
                width: 400,
                height: 300,
            },
        )
        .expect("render");

    let px = |x: usize, y: usize| -> [u8; 4] {
        let base = (y * frame.width as usize + x) * 4;
        [
            frame.rgba[base],
            frame.rgba[base + 1],
            frame.rgba[base + 2],
            frame.rgba[base + 3],
        ]
    };

    // The offscreen target composites over an opaque black backdrop, so
    // assertions run on luminance. Rect centre: white.
    let center = px(200, 150);
    assert!(center[0] > 200, "rect centre must stay white: {center:?}");
    // On the rect edge column: blur mixes content with the backdrop
    // (premultiplied alpha softens twice), so expect mid luminance.
    let spill = px(260, 150);
    assert!(
        spill[0] > 20 && spill[0] < 200,
        "expected blurred edge luminance, got {spill:?}"
    );
    // Well outside the content + support: untouched backdrop.
    let far = px(60, 40);
    assert!(
        far[0] + far[1] + far[2] < 20,
        "content must not leak outside the derived region: {far:?}"
    );
}

/// Authored `bounds:` on a Filter scopes effect processing and composites
/// the result back at the region origin.
#[test]
fn authored_effect_bounds_region_renders_at_origin() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    let source = r#"
config { resolution: (400, 300) }

#0s
fx: Filter, bounds: (100, 60, 200, 180) {
  soft: Blur, radius: 4
  box: Rect, size: (120, 120), color: (1, 1, 1, 1), at: (0, 0)
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;

    let frame = renderer
        .render_timeline(
            &timeline,
            0.0,
            animatix::timeline::SceneDimensions {
                width: 400,
                height: 300,
            },
        )
        .expect("render");

    let px = |x: usize, y: usize| -> [u8; 4] {
        let base = (y * frame.width as usize + x) * 4;
        [
            frame.rgba[base],
            frame.rgba[base + 1],
            frame.rgba[base + 2],
            frame.rgba[base + 3],
        ]
    };

    // The rect (140..260, 90..210) sits inside the authored region
    // (100..300, 60..240). The backdrop is opaque black: check luminance.
    let center = px(200, 150);
    assert!(center[0] > 200, "rect centre must stay white: {center:?}");
    let far = px(40, 30);
    assert!(
        far[0] + far[1] + far[2] < 20,
        "content must not leak outside the authored region: {far:?}"
    );
}

/// A Mask must clip its children to the mask's own rect AT THE MASK'S
/// POSITION. (Regression: the clip layer was pushed with the identity
/// transform, pinning the clip at the scene origin — children of any mask
/// not at the top-left corner were clipped away entirely, so Mask +
/// Image rendered nothing.)
#[test]
fn mask_clips_children_at_mask_position() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    // The oversized red child must show through the Mask's clip rect and
    // be clipped outside it. (Regression: the clip layer was pushed at the
    // scene origin, clipping away every child of any Mask not positioned
    // at the top-left corner.)
    let source = r#"
config { resolution: (400, 300) }

#0s
m: Mask, size: (200, 150), at: (300, 150) {
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;

    let frame = renderer
        .render_timeline(
            &timeline,
            0.5,
            SceneDimensions {
                width: 400,
                height: 300,
            },
        )
        .expect("render should succeed");

    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * frame.width + x) * 4) as usize;
        [
            frame.rgba[i],
            frame.rgba[i + 1],
            frame.rgba[i + 2],
            frame.rgba[i + 3],
        ]
    };

    // Inside the mask rect (mask spans 200..400 x 75..225): the oversized
    // child rect shows through, clipped to red.
    let inside = px(300, 150);
    assert!(
        inside[0] > 200 && inside[1] < 80 && inside[2] < 80,
        "mask interior should show the red child, got {inside:?}"
    );
    // Outside the mask rect: background, NOT the oversized child.
    let outside = px(60, 150);
    assert!(
        !(outside[0] > 200 && outside[1] < 80 && outside[2] < 80),
        "child must be clipped to the mask rect, found red outside at {outside:?}"
    );
}

/// A `clip_shape` child defines the clip geometry (ellipse here) and is
/// not rendered itself: an oversized red child shows only inside the
/// ellipse, and the mask-rect corners outside the ellipse stay background.
#[test]
fn mask_clip_shape_ellipse_defines_clip_region() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    let source = r#"
config { resolution: (400, 300) }

m: Mask, size: (200, 150), at: (300, 150) {
  clip_shape: Ellipse, size: (100, 100)
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}

#0s
fade-in m [1ms]
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;

    let frame = renderer
        .render_timeline(
            &timeline,
            0.5,
            SceneDimensions {
                width: 400,
                height: 300,
            },
        )
        .expect("render should succeed");

    let is_red = |x: u32, y: u32| -> bool {
        let i = ((y * frame.width + x) * 4) as usize;
        let px = [frame.rgba[i], frame.rgba[i + 1], frame.rgba[i + 2]];
        px[0] > 200 && px[1] < 80 && px[2] < 80
    };

    // Mask center: inside the ellipse → red child visible.
    assert!(is_red(300, 150), "ellipse center should show the red child");
    // Mask-rect corner (inside the mask size, outside the ellipse):
    // clipped away → background, and the clip_shape itself must NOT paint.
    assert!(!is_red(215, 85), "outside the ellipse must stay background");
    // Far outside the mask entirely.
    assert!(!is_red(60, 150), "far outside the mask must stay background");
}

/// A non-Rect/Ellipse `clip_shape` (Polygon here) must define the actual
/// clip geometry. Before the `Primitive::clip_path` capability this
/// silently fell back to a rectangular clip, so a diamond mask rendered as
/// a rectangle. The clip child must also not paint itself.
#[test]
fn mask_clip_shape_polygon_defines_clip_region() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    let source = r#"
config { resolution: (400, 300) }

m: Mask, size: (100, 75), at: (200, 150) {
  clip_shape: Polygon, points: {(0, -50), (50, 0), (0, 50), (-50, 0)}, color: accent.success
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}

#0s
fade-in m [1ms]
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;

    let frame = renderer
        .render_timeline(
            &timeline,
            0.5,
            SceneDimensions {
                width: 400,
                height: 300,
            },
        )
        .expect("render should succeed");

    let is_red = |x: u32, y: u32| -> bool {
        let i = ((y * frame.width + x) * 4) as usize;
        let px = [frame.rgba[i], frame.rgba[i + 1], frame.rgba[i + 2]];
        px[0] > 200 && px[1] < 80 && px[2] < 80
    };

    // Diamond center → red child visible.
    assert!(is_red(200, 150), "diamond center should show the red child");
    // Inside the mask box but outside the diamond (|dx| + |dy| > 50):
    // clipped away — a rectangular fallback would show red here.
    assert!(!is_red(235, 178), "outside the diamond must stay background");
    assert!(!is_red(60, 150), "far outside the mask must stay background");
}

/// A `clip_shape` whose primitive has no clip geometry warns instead of
/// silently clipping with a rectangle.
#[test]
fn mask_clip_shape_without_geometry_warns() {
    let source = r#"
config { resolution: (400, 300) }

m: Mask, size: (100, 75), at: (200, 150) {
  clip_shape: Text, text: "clip"
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}

#0s
fade-in m [1ms]
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;

    let _ = timeline.evaluate(
        0.5,
        SceneDimensions {
            width: 400,
            height: 300,
        },
    );
    let warnings = timeline.runtime_diagnostics();
    assert!(
        warnings.iter().any(|d| d.message.contains("provides no clip geometry")),
        "expected a clip-geometry warning, got {:?}",
        warnings.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
}

/// Plot geometry is not a clip provider (it is time-varying and
/// `stroke_progress`-trimmed), so a plot `clip_shape` warns and falls back
/// to a rectangular clip instead of clipping to a moving partial curve.
#[test]
fn mask_clip_shape_plot_falls_back_with_warning() {
    let source = r#"
config { resolution: (400, 300) }

m: Mask, size: (100, 75), at: (200, 150) {
  clip_shape: PlotCurve, kind: "cartesian", func: (x) => x
  big: Rect, size: (400, 300), color: (1, 0, 0, 1)
}

#0s
fade-in m [1ms]
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;

    let _ = timeline.evaluate(
        0.5,
        SceneDimensions {
            width: 400,
            height: 300,
        },
    );
    let warnings = timeline.runtime_diagnostics();
    assert!(
        warnings.iter().any(|d| d.message.contains("provides no clip geometry")),
        "expected a clip-geometry warning for a plot clip_shape, got {:?}",
        warnings.iter().map(|d| d.message.clone()).collect::<Vec<_>>()
    );
}

#[test]
fn hosted_bar_chart_paints_bars_across_the_full_graph_axis() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    // Regression: hosted plots used to occupy only the central half of the
    // Graph axis (the `{graph}_size` env key was seeded half but consumed
    // as full). Bars must visibly paint from the left third to the right
    // third of the axis box, not cluster in the middle. (The geometry-level
    // assertion `hosted_bar_chart_spans_graph_axis` only checks the env/
    // paths; this one checks the rasterized frame.)
    let source = r#"
config { resolution: (800, 400) }

#0s
g: Graph, size: (600, 300), at: (400, 200), x_domain: (0, 4), y_domain: (0, 100) {
  bars: BarChart,
    data: {("A", 80), ("B", 60), ("C", 85), ("D", 90)},
    bar_colors: {(1, 0.5, 0.2, 1), (1, 0.5, 0.2, 1), (1, 0.5, 0.2, 1), (1, 0.5, 0.2, 1)}
}
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    let timeline = report.output;

    let frame = renderer
        .render_timeline(
            &timeline,
            0.5,
            SceneDimensions {
                width: 800,
                height: 400,
            },
        )
        .expect("render should succeed");

    let px = |x: u32, y: u32| -> [u8; 4] {
        let i = ((y * frame.width + x) * 4) as usize;
        [
            frame.rgba[i],
            frame.rgba[i + 1],
            frame.rgba[i + 2],
            frame.rgba[i + 3],
        ]
    };

    // Scan a horizontal strip through the upper-middle of the bars. The
    // Graph spans ~x 100..700; with values 60-90 of a 0..100 domain every
    // bar paints at screen y=330 here. Ancestral "central half" behavior
    // would leave bars within ~250..550 only (see also the vertical-overhang
    // note in probe 008 — a separate hosted-BarChart baseline issue).
    let is_bar = |c: [u8; 4]| c[0] > 150 && c[1] < 180 && c[2] < 130;
    let mut min_x = u32::MAX;
    let mut max_x = 0u32;
    for x in 110..690u32 {
        if is_bar(px(x, 330)) {
            min_x = min_x.min(x);
            max_x = max_x.max(x);
        }
    }
    assert!(
        min_x < 280 && max_x > 520,
        "bars should span the full axis (left third to right third), got min_x={min_x} max_x={max_x}"
    );
}

#[test]
fn offscreen_renderer_ensure_targets_is_idempotent() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    let dimensions = SceneDimensions {
        width: 200,
        height: 200,
    };

    // First render may fail on some GPU configs; we just verify no panic
    let timeline = Timeline::new();
    let first = renderer.render_timeline(&timeline, 0.0, dimensions);
    let second = renderer.render_timeline(&timeline, 1.0, dimensions);
    // Both should either succeed or fail consistently
    if first.is_ok() {
        assert!(second.is_ok(), "second render should also succeed");
    }
}

/// Regression (effects-wave1 dogfood): a scope whose chain runs under a
/// *derived* region used to composite stale ping-pong texels from the
/// previous scope into the frame (an opaque garbage rectangle around the
/// moving content, found with a `MotionBlur` card flying over a backdrop).
/// The backdrop must survive intact inside another scope's region.
#[test]
fn region_composite_does_not_leak_stale_pixels() {
    let mut renderer = match OffscreenRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            crate::testing::skip_if_no_gpu();

            return;
        },
    };

    let source = r#"
config { resolution: (640, 360) }

sheet: Rect, size: (640, 360), color: (1, 1, 1, 1), anchor: scene.center

card: Filter, anchor: scene.center, offset: (-150, 0) {
  swipe: MotionBlur, length: 20, angle: 0
  plate: Rect, size: (140, 90), color: (1, 0.2, 0.2, 1)
}

#0s
fade-in sheet [100ms]
fade-in card [100ms]

#0.5s
card.offset = (0, 0) [1s, ease: linear]
card.swipe.length = 20 [0.5s, ease: linear]
"#;
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "parse errors: {:?}", parse_errors);
    let ast = ast.expect("AST");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast,
        &std::collections::HashMap::new(),
    );
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);

    // Mid-flight: the card centre is at (245, 180) (offset halfway from
    // -150 to 0) and its derived region spans (155, 115)-(335, 245).
    let frame = renderer
        .render_timeline(
            &report.output,
            1.0,
            animatix::timeline::SceneDimensions {
                width: 640,
                height: 360,
            },
        )
        .expect("render");

    let px = |x: usize, y: usize| -> [u8; 4] {
        let base = (y * frame.width as usize + x) * 4;
        [
            frame.rgba[base],
            frame.rgba[base + 1],
            frame.rgba[base + 2],
            frame.rgba[base + 3],
        ]
    };

    // Inside the card's region but outside the card and its smear: the
    // white backdrop must survive the region composite untouched. The
    // leak pasted opaque garbage here.
    let backdrop_inside_region = px(165, 125);
    assert!(
        backdrop_inside_region[0] > 200,
        "backdrop inside another scope's region must not be overwritten: {backdrop_inside_region:?}"
    );

    // The card plate itself stays red at its mid-flight position.
    let plate = px(245, 180);
    assert!(
        plate[0] > 200 && plate[1] < 120,
        "moving plate must render red at its mid-flight position: {plate:?}"
    );
}
