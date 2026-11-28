use super::*;
use animatix::easing::Easing;

#[test]
fn audit_omits_paint_without_rewriting_authored_opacity_or_dirtying_the_cache() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let timeline = build_checked(
        r#"
config { canvas_size: (160, 90) }
#0s
scene.background_color = hex_color("101820")
dot: Ellipse, at: (40,45), size: (28,28), color: hex_color("ffbe30"), opacity: 0
marker: Rect, at: (120,45), size: (30,30), color: hex_color("70ffff")
always {
  dot.opacity = 0.7
  marker.opacity = dot.opacity
}
"#,
    );
    let dims = timeline.canvas_dimensions();
    let full = render(&mut renderer, &timeline, 0.0, dims);
    let variant = renderer.render_canvas_omitting_paint(&timeline, 0.0, dims, &["dot"]).unwrap();
    assert_ne!(
        full.rgba, variant.rgba,
        "always assigning opacity must not restore omitted paint"
    );
    let mut changed = 0;
    for (i, (a, b)) in full
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(variant.rgba.as_chunks::<4>().0)
        .enumerate()
    {
        if a != b {
            assert!(i % (dims.width as usize) < 80, "disjoint marker or background changed");
            changed += 1;
        }
    }
    assert!(changed > 500);
    let after = render(&mut renderer, &timeline, 0.0, dims);
    assert_eq!(full.rgba, after.rgba, "counterfactual must not affect a later ordinary render");
}

#[test]
fn audit_omitting_either_shared_endpoint_omits_exactly_the_one_overlay() {
    let Some(mut renderer) = new_renderer() else {
        return;
    };
    let from = build_checked(
        r#"
config { canvas_size: (160, 90) }
#0s
scene.background_color = hex_color("101820")
from_label: Text, at: (66,45), text: "HUB", font_size: 25, shared_id: "hub"
marker: Rect, at: (140,45), size: (20,20), color: hex_color("70ffff")
"#,
    );
    let to = build_checked(
        r#"
config { canvas_size: (160, 90) }
#0s
scene.background_color = hex_color("101820")
to_label: Text, at: (86,45), text: "HUB", font_size: 25, shared_id: "hub"
marker: Rect, at: (140,45), size: (20,20), color: hex_color("70ffff")
"#,
    );
    let dims = from.canvas_dimensions();
    let mut variant = |omit| {
        renderer
            .render_canvas_transition_omitting_paint(
                &from,
                0.0,
                &to,
                0.0,
                0.45,
                "fade".into(),
                Easing::Linear,
                dims,
                omit,
            )
            .unwrap()
    };
    let full = variant((&[], &[]));
    let omit_from = variant((&["from_label"], &[]));
    let omit_to = variant((&[], &["to_label"]));
    assert_eq!(
        omit_from.rgba, omit_to.rgba,
        "the two names identify one logical shared overlay"
    );
    let mut changed = 0;
    for (i, (a, b)) in full
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .zip(omit_to.rgba.as_chunks::<4>().0)
        .enumerate()
    {
        if a != b {
            assert!(i % (dims.width as usize) < 120, "nonshared marker changed");
            changed += 1;
        }
    }
    assert!(changed > 30, "the label should have been present, then omitted");
}
