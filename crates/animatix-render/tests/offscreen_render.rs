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

/// `corner_radius` must reach the rasterizer: with a radius the corner pixels
/// stay empty while the edge midpoints and the interior are painted, and with
/// no radius the corner is painted.
#[test]
fn corner_radius_rounds_a_rect_end_to_end() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let source = r#"
#0s
square: Rect, at: (50, 50), size: (60, 60), color: (1, 1, 1, 1)
round: Rect, at: (150, 50), size: (60, 60), color: (1, 1, 1, 1), corner_radius: 18
"#;
    let (ast, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "parse errors: {errors:?}");
    let report = animatix::timeline::Timeline::build_with_diagnostics(
        &ast.expect("AST"),
        &std::collections::HashMap::new(),
    );
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
    let dims = SceneDimensions {
        width: 200,
        height: 100,
    };
    let frame = renderer.render_timeline(&report.output, 0.0, dims).expect("render");
    // The offscreen frame carries an opaque background, so "painted" shows up as
    // a bright white pixel and "cut away" as the dark background.
    let red_at = |x: usize, y: usize| frame.rgba[(y * 200 + x) * 4];

    // Square rect spans 20..80; its top-left corner is painted.
    assert!(red_at(22, 22) > 200, "square corner must be painted");
    // Rounded rect spans 120..180; its top-left corner is cut away, while the
    // edge midpoint and the interior remain.
    assert!(red_at(122, 22) < 40, "rounded corner must stay empty");
    assert!(red_at(150, 22) > 200, "the top edge must still be painted");
    assert!(red_at(150, 50) > 200, "the interior must be painted");
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

/// A multi-scene transition frame must be a blend of its two scenes, not a cut
/// to the incoming one. At progress 0.5 the frame sits between the two solid
/// colours and matches neither endpoint; it is also (near) the linear midpoint,
/// because the compositor applies the easing itself and `fade` + linear passes
/// the progress straight through.
#[test]
fn transition_midframe_blends_both_scenes() {
    use animatix::easing::Easing;

    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };

    // Two full-bleed single-colour scenes: whatever the compositor does, the
    // frame average lands on a mix of the two fills.
    let timeline = |color: &str| {
        let source = format!(
            "config {{ colorscheme: \"editorial-dark\", resolution: (64, 64) }}\n\
             #0s\n\
             r: Rect, size: (200, 200), color: {color}, at: (32, 32)\n\
             fade-in r [100ms]\n"
        );
        let (ast, errors) = animatix_syntax::parser::parse_source(&source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let report =
            Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        report.output
    };

    let warm = timeline("(0.9, 0.1, 0.1, 1)");
    let cool = timeline("(0.1, 0.2, 0.9, 1)");
    let debug = DebugRenderOptions::default();

    let average = |frame: &animatix_render::offscreen::RenderedFrame| -> [f32; 3] {
        let pixels = frame.rgba.len() / 4;
        let mut sum = [0f64; 3];
        for px in frame.rgba.chunks_exact(4) {
            sum[0] += px[0] as f64;
            sum[1] += px[1] as f64;
            sum[2] += px[2] as f64;
        }
        [
            (sum[0] / pixels as f64) as f32,
            (sum[1] / pixels as f64) as f32,
            (sum[2] / pixels as f64) as f32,
        ]
    };

    // Sample at 0.5s: after the fixtures' own fade-in, so each scene shows its
    // fill rather than the entrance ramp.
    let start = renderer
        .render_transition(
            &warm,
            0.5,
            &cool,
            0.5,
            0.0,
            "fade".to_string(),
            Easing::Linear,
            dims,
            debug,
        )
        .expect("progress 0 frame");
    let mid = renderer
        .render_transition(
            &warm,
            0.5,
            &cool,
            0.5,
            0.5,
            "fade".to_string(),
            Easing::Linear,
            dims,
            debug,
        )
        .expect("mid frame");
    let end = renderer
        .render_transition(
            &warm,
            0.5,
            &cool,
            0.5,
            1.0,
            "fade".to_string(),
            Easing::Linear,
            dims,
            debug,
        )
        .expect("progress 1 frame");

    let (start_avg, mid_avg, end_avg) = (average(&start), average(&mid), average(&end));
    assert!(
        mid_avg != start_avg && mid_avg != end_avg,
        "the mid frame must differ from both endpoints: {start_avg:?} {mid_avg:?} {end_avg:?}"
    );
    for channel in 0..3 {
        let midpoint = (start_avg[channel] + end_avg[channel]) / 2.0;
        assert!(
            (mid_avg[channel] - midpoint).abs() < 12.0,
            "channel {channel}: mid {mid_avg:?} should sit between {start_avg:?} and {end_avg:?}"
        );
    }
}

/// A transition frame at progress 0 must equal the single-scene render of the
/// outgoing scene *including its GPU filter scopes*. The transition path used
/// to evaluate both scenes with `filter_backend = None`, so a scene whose
/// content sat inside a `Filter` scope lost its effects in every exported
/// transition frame while the preview kept them.
#[test]
fn transition_frames_keep_gpu_filter_scopes() {
    use animatix::easing::Easing;

    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let dims = SceneDimensions {
        width: 64,
        height: 64,
    };

    // Outgoing scene: a bright rect inside a Blur scope over a dark backdrop —
    // the blur visibly softens and spreads the rect, so its absence moves the
    // frame average well beyond rounding noise.
    let build = |source: &str| {
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let report =
            Timeline::build_with_diagnostics(&ast.expect("AST"), &std::collections::HashMap::new());
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        report.output
    };
    let filtered = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (64, 64) }
#0s
backdrop: Rect, at: (32, 32), size: (200, 200), color: (0.06, 0.07, 0.1, 1)
panel: Filter {
  soft: Blur, radius: 6
  box: Rect, size: (36, 36), color: (0.95, 0.3, 0.25, 1)
}
fade-in panel [100ms]
"#,
    );
    let plain = build(
        r#"
config { colorscheme: "editorial-dark", resolution: (64, 64) }
#0s
backdrop: Rect, at: (32, 32), size: (200, 200), color: (0.2, 0.3, 0.5, 1)
"#,
    );

    let average = |frame: &animatix_render::offscreen::RenderedFrame| -> [f32; 3] {
        let pixels = frame.rgba.len() / 4;
        let mut sum = [0f64; 3];
        for px in frame.rgba.chunks_exact(4) {
            sum[0] += px[0] as f64;
            sum[1] += px[1] as f64;
            sum[2] += px[2] as f64;
        }
        [
            (sum[0] / pixels as f64) as f32,
            (sum[1] / pixels as f64) as f32,
            (sum[2] / pixels as f64) as f32,
        ]
    };

    let debug = DebugRenderOptions::default();
    // Sample after the fixture's own fade-in so the scope is fully applied.
    let direct = renderer
        .render_timeline_with_debug(&filtered, 0.5, dims, debug)
        .expect("single-scene frame");
    let progress0 = renderer
        .render_transition(
            &filtered,
            0.5,
            &plain,
            0.5,
            0.0,
            "fade".to_string(),
            Easing::Linear,
            dims,
            debug,
        )
        .expect("progress 0 frame");

    let direct_avg = average(&direct);
    let progress0_avg = average(&progress0);
    for channel in 0..3 {
        assert!(
            (direct_avg[channel] - progress0_avg[channel]).abs() < 1.0,
            "channel {channel}: the progress-0 transition frame {progress0_avg:?} must match the \
             single-scene render {direct_avg:?} (filter scopes dropped on the transition path?)"
        );
    }
}

/// `BuildQuality` is a real pixel knob on plot-family actors: the same scene
/// built at Draft (4× sampling tolerance) and Production renders measurably
/// different geometry. This is what the web player's `quality` attribute
/// switches between — the test pins that the difference exists and is visible.
#[test]
fn build_quality_changes_plot_sampling_output() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let dims = SceneDimensions {
        width: 320,
        height: 320,
    };
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (320, 320) }
g: Graph, x_domain: (-3, 3), y_domain: (-3, 3), size: (300, 300), at: (160, 160) {
  rose: PlotCurve, kind: "polar", func: (t) => 1.6 * sin(4 * t), color: (0.95, 0.3, 0.25, 1), stroke_width: 2
}
fade-in g [100ms]
"#;
    let build = |quality| {
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        let report = animatix::timeline::Timeline::build_with_diagnostics_and_font_context(
            &ast.expect("AST"),
            &std::collections::HashMap::new(),
            std::sync::Arc::new(animatix::renderer::text::FontContext::new()),
            quality,
        );
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
        report.output
    };
    let average = |frame: &animatix_render::offscreen::RenderedFrame| -> [f32; 3] {
        let pixels = frame.rgba.len() / 4;
        let mut sum = [0f64; 3];
        for px in frame.rgba.chunks_exact(4) {
            sum[0] += px[0] as f64;
            sum[1] += px[1] as f64;
            sum[2] += px[2] as f64;
        }
        [
            (sum[0] / pixels as f64) as f32,
            (sum[1] / pixels as f64) as f32,
            (sum[2] / pixels as f64) as f32,
        ]
    };

    let debug = DebugRenderOptions::default();
    let draft = renderer
        .render_timeline_with_debug(
            &build(animatix::timeline::BuildQuality::Draft),
            0.5,
            dims,
            debug,
        )
        .expect("draft frame");
    let production = renderer
        .render_timeline_with_debug(
            &build(animatix::timeline::BuildQuality::Production),
            0.5,
            dims,
            debug,
        )
        .expect("production frame");

    // The honest invariant of the quality knob is at the geometry level: the
    // sampler produces a finer polyline at Production. (The *pixel* difference
    // is content-dependent — a smooth curve can rasterize identically at both
    // tolerances — so it is not asserted here.)
    let elements = |timeline: &Timeline| -> usize {
        let track = timeline.tracks().get("rose").expect("rose track");
        let paths = track.evaluate_vector_paths(0);
        paths.iter().map(|p| p.path.elements().len()).sum()
    };
    let draft_timeline = build(animatix::timeline::BuildQuality::Draft);
    let production_timeline = build(animatix::timeline::BuildQuality::Production);
    let (draft_elements, production_elements) =
        (elements(&draft_timeline), elements(&production_timeline));
    assert!(
        draft_elements * 3 <= production_elements * 2,
        "production must sample distinctly finer than draft: \
         {draft_elements} vs {production_elements} path elements"
    );

    // Both qualities render real content (the knob never blanks the scene).
    let (draft_avg, production_avg) = (average(&draft), average(&production));
    for (name, avg) in [("draft", draft_avg), ("production", production_avg)] {
        assert!(avg[0] > 10.0, "{name} render lost its curve: {avg:?}");
    }
}

/// A blit into a `Bgra8Unorm` target must work: browser canvas surfaces
/// report their own format (Firefox's wgpu backend orders Bgra8Unorm first),
/// and WebGPU requires the blit pipeline's color-target format to match the
/// attachment exactly — the old fixed-Rgba8Unorm pipeline tripped Firefox's
/// validation on every present. The per-format variant must validate cleanly
/// and actually write the pixels (channel-swizzled on readback).
#[test]
fn blits_into_bgra8_targets() {
    use animatix_render::core::RendererCore;

    let Some((device, queue)) = pollster::block_on(headless_device()) else {
        animatix_render::testing::skip_if_no_gpu();
        return;
    };
    let core = RendererCore::new(&device, &queue).expect("core");
    // Validation errors must fail the test, not just log.
    device.on_uncaptured_error(std::sync::Arc::new(|error| {
        panic!("uncaptured wgpu error during bgra blit: {error}");
    }));

    const SIZE: u32 = 64; // 4 B/px * 64 = 256, the COPY_BYTES_PER_ROW_ALIGNMENT
    let texture = |format, usage, label| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: wgpu::Extent3d {
                width: SIZE,
                height: SIZE,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats: &[],
        })
    };
    let src = texture(
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST,
        "bgra-blit src",
    );
    let dst = texture(
        wgpu::TextureFormat::Bgra8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        "bgra-blit dst",
    );

    // Source: opaque red in the top half, transparent below — the blit must
    // land both, alpha-blended over the cleared destination.
    let mut pixels = vec![0u8; (SIZE * SIZE * 4) as usize];
    for px in pixels.chunks_exact_mut(4).take((SIZE * SIZE / 2) as usize) {
        px.copy_from_slice(&[220, 40, 40, 255]);
    }
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &src,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(SIZE * 4),
            rows_per_image: Some(SIZE),
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );

    let bytes_per_row = SIZE * 4;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bgra-blit readback"),
        size: (bytes_per_row * SIZE) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    core.blit_texture_to_format(
        &device,
        &queue,
        &src.create_view(&wgpu::TextureViewDescriptor::default()),
        &dst.create_view(&wgpu::TextureViewDescriptor::default()),
        SIZE,
        SIZE,
        1.0,
        wgpu::TextureFormat::Bgra8Unorm,
    );

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("bgra-blit copy"),
    });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &dst,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(bytes_per_row),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(std::iter::once(encoder.finish()));

    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        tx.send(r).ok();
    });
    match device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    }) {
        Ok(_) => {},
        Err(e) => panic!("poll failed: {e:?}"),
    }
    rx.recv().unwrap().expect("map failed");

    let data = slice.get_mapped_range();
    // Top half: red, swizzled to BGra byte order. Bottom half: the clear
    // color (black, opaque — an alpha-blended blit over LoadOp::Clear? the
    // pass uses Load, and the texture starts zeroed, so transparent black).
    let top = &data[((SIZE / 2) as usize - 1) * bytes_per_row as usize..][..4];
    assert_eq!(
        [top[0], top[1], top[2], top[3]],
        [40, 40, 220, 255],
        "bgra target must hold the swizzled red pixel"
    );
}

/// Headless device for the core-level tests above (fallible — callers decide
/// the skip policy via [`animatix_render::testing::skip_if_no_gpu`]).
async fn headless_device() -> Option<(wgpu::Device, wgpu::Queue)> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::default(),
            compatible_surface: None,
            force_fallback_adapter: true,
        })
        .await
        .ok()?;
    let limits = wgpu::Limits::default().using_resolution(adapter.limits());
    adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("Animatix Bgra Blit Test Device"),
            required_features: wgpu::Features::empty(),
            required_limits: limits,
            memory_hints: Default::default(),
            ..Default::default()
        })
        .await
        .ok()
}
