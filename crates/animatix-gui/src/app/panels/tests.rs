use super::api::library_drag_id;
use super::{
    LibraryDragPayload, clear_library_drag, library_drag, library_drag_label_base,
    nice_tick_interval, set_library_drag,
};

fn url_property(url: &str) -> animatix_syntax::ast::Property {
    animatix_syntax::ast::Property {
        name: "url".into(),
        value: animatix_syntax::ast::Expr::Str(url.to_string()),
        value_span: None,
        trailing_comment: None,
    }
}

#[test]
fn library_drag_payload_roundtrips_through_context_data() {
    let ctx = egui::Context::default();
    assert!(library_drag(&ctx).is_none());

    let payload = LibraryDragPayload {
        ty: "Image".into(),
        props: vec![url_property("a.png")],
    };
    set_library_drag(&ctx, payload.clone());
    assert_eq!(library_drag(&ctx), Some(payload));

    clear_library_drag(&ctx);
    assert!(library_drag(&ctx).is_none(), "cleared payload must not linger");
}

#[test]
fn library_drag_id_is_stable() {
    // The sidebar writes and the preview panel reads this id; it must not
    // drift between the two call sites.
    assert_eq!(library_drag_id(), egui::Id::unique("library_drag_payload"));
}

#[test]
fn library_drag_label_base_maps_assets_to_lowercase_stems() {
    let image = LibraryDragPayload {
        ty: "Image".into(),
        props: vec![],
    };
    assert_eq!(library_drag_label_base(&image), "image");
    let svg = LibraryDragPayload {
        ty: "Svg".into(),
        props: vec![],
    };
    assert_eq!(library_drag_label_base(&svg), "svg");
    // Component types keep their declared name.
    let component = LibraryDragPayload {
        ty: "Badge".into(),
        props: vec![],
    };
    assert_eq!(library_drag_label_base(&component), "Badge");
}

#[test]
fn nice_tick_interval_covers_ranges_and_boundaries() {
    let cases = [
        // (visible_range, expected interval at 10 target ticks)
        (100.0, 10.0),
        (50.0, 5.0),
        (0.5, 0.05),
        (0.0, 1.0),
        (-100.0, 10.0),
        (10000.0, 1000.0),
        (0.1, 0.01),
        (1.0, 0.1),
        (10.0, 1.0),
        (1000.0, 100.0),
        // Boundaries around the 1.5 / 3.5 / 7.5 nice-multiple cutoffs.
        (14.9, 1.0),
        (15.1, 2.0),
        (34.9, 2.0),
        (35.1, 5.0),
        (74.9, 5.0),
        (75.1, 10.0),
    ];
    for (range, expected) in cases {
        let interval = nice_tick_interval(range, 10.0);
        assert!(interval > 0.0, "interval must be positive for range={range}");
        assert!(
            (interval - expected).abs() < 0.001,
            "range={range}: expected {expected}, got {interval}"
        );
    }
}
