//! Pixel regressions for sampled geometry and authored full-scene filter regions.
use super::{RenderedFrame, SceneDimensions, build, new_renderer};

fn assert_same_pixels(a: &RenderedFrame, b: &RenderedFrame) {
    assert_eq!((a.width, a.height), (b.width, b.height));
    assert_eq!(a.rgba.len(), b.rgba.len());
    assert_eq!(a.rgba.len(), (a.width * a.height * 4) as usize);
    let max_delta = a.rgba.iter().zip(b.rgba.iter()).map(|(a, b)| a.abs_diff(*b)).max().unwrap();
    assert!(max_delta <= 2, "ROI changed the image: max channel delta={max_delta}");
}

#[test]
fn growing_filter_auto_region_matches_explicit_region_across_seeks() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let dims = SceneDimensions {
        width: 400,
        height: 200,
    };
    for tail in [true, false] {
        for times in [[0., 0.5, 1., 0.5, 1.], [1., 0.5, 0., 0.5, 1.]] {
            let source = |bounds: &str| {
                format!(
                    r##"
config {{ resolution: (400,200), duration: 3 }}
scope: Filter, at: (200,100), opacity: 1{bounds} {{
 fx: Blur, radius: 4
 box: Rect, size: (60,40), color: "#ff0000", opacity: 1
}}
{}
always {{ box.size = (60 + 180 * clamp(t,0,1),40) }}
"##,
                    if tail {
                        ""
                    } else {
                        "marker: Rect, size:(10,10), at:(15,15), color:\"#00ff00\", opacity:1"
                    }
                )
            };
            let auto = build(&source(""));
            let explicit = build(&source(", bounds:(40,40,320,120)"));
            for time in times {
                let actual = renderer.render_timeline(&auto, time, dims).unwrap();
                let expected = renderer.render_timeline(&explicit, time, dims).unwrap();
                assert_same_pixels(&actual, &expected);
                if time == 1.0 {
                    // An independent pixel oracle keeps two identically clipped
                    // outputs from satisfying the comparison accidentally.
                    assert!(actual.rgba[(100 * 400 + 100) * 4] > 240);
                }
            }
        }
    }
}

#[test]
fn full_canvas_filter_bounds_preserve_vignette_coordinate_extent() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let dims = SceneDimensions {
        width: 400,
        height: 200,
    };
    for tail in [true, false] {
        let source = |bounds: &str| {
            format!(
                r##"
config {{ resolution: (400,200) }}
scope: Filter, at: (200,100), opacity: 1{bounds} {{
 fx: Vignette, amount: 1, radius: 0.1, softness: 0.5
 box: Rect, size: (120,60), color: "#ffffff", opacity: 1
}}
{}
"##,
                if tail {
                    ""
                } else {
                    "marker: Rect, size:(10,10), at:(15,15), opacity:1"
                }
            )
        };
        let auto = build(&source(""));
        let full = build(&source(", bounds:(0,0,400,200)"));
        let oversized = build(&source(", bounds:(-10,-10,420,220)"));
        for time in [0., 1., 0.] {
            let derived = renderer.render_timeline(&auto, time, dims).unwrap();
            let authored = renderer.render_timeline(&full, time, dims).unwrap();
            let oversize = renderer.render_timeline(&oversized, time, dims).unwrap();
            assert_same_pixels(&authored, &oversize);
            let pixel = (100 * 400 + 150) * 4;
            assert!(derived.rgba[pixel] < 50, "content-relative vignette edge");
            assert!(authored.rgba[pixel] > 150, "full-canvas bounds must not derive content ROI");
        }
    }
}

#[test]
fn filtered_shadow_region_matches_fresh_explicit_reference_after_memo_hits() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let dims = SceneDimensions {
        width: 400,
        height: 240,
    };
    for tail in [true, false] {
        let source = |bounds: &str| {
            format!(
                r##"
config {{ resolution: (400,240), duration: 3 }}
scope: Filter, at:(200,120), opacity:1{bounds} {{
 fx: Blur, radius: 2
 box: Rect, size:(100,60), color:"#ff0000", opacity:1,
  shadow:(30,8,12,2), shadow_color:"#00ff00"
}}
{}
always {{ box.at = (20 * t,0) }}
"##,
                if tail {
                    ""
                } else {
                    "marker: Rect, size:(10,10), at:(15,15), opacity:1"
                }
            )
        };
        let timeline = build(&source(""));
        for time in [0., 0.5, 1., 0.5, 0., 1., 1.] {
            // Rebuild only the reference so a memo bug cannot affect both
            // histories identically and make the pixel comparison pass.
            let fresh = build(&source(", bounds:(20,20,360,200)"));
            let actual = renderer.render_timeline(&timeline, time, dims).unwrap();
            let expected = renderer.render_timeline(&fresh, time, dims).unwrap();
            assert_same_pixels(&actual, &expected);
            assert!(
                actual.rgba.as_chunks::<4>().0.iter().any(|p| p[1] > 50 && p[0] < 30),
                "shadow must be visible"
            );
        }
    }
}
