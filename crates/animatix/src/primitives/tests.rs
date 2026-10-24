//! Unit tests for the primitive trait, built-in registry wiring and text
//! evaluation.

use super::*;
use crate::timeline::{AnimationTrack, DEFAULT_WHITE, Value, VelloPath};

#[test]
fn all_primitives_have_unique_type_names() {
    let mut seen = std::collections::HashSet::new();
    for entry in BUILT_INS.iter() {
        let name = entry.behavior.type_name();
        assert!(seen.insert(name), "Duplicate type_name: {:?}", name);
    }
}

#[test]
fn size_override_is_authored_in_full_extents() {
    // Shape state stores half-extents, so the frame override has to halve
    // exactly like the declaration path does.
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("size".to_string(), Value::Vec2([100.0, 60.0]));
    assert_eq!(resolve_half_size([50.0, 50.0], Some(&overrides)), [50.0, 30.0]);
}

#[test]
fn radius_overrides_replace_one_axis_of_the_size_override() {
    // `radius_x`/`radius_y` are half-size components, so they apply on top
    // of a `size` box without the halving, and each touches only its axis.
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("size".to_string(), Value::Vec2([100.0, 100.0]));
    overrides.insert("radius_x".to_string(), Value::Num(20.0));
    assert_eq!(resolve_half_size([70.0, 70.0], Some(&overrides)), [20.0, 50.0]);

    // Without a `size` override the keyframed half-size survives on the
    // axis no radius touched.
    let mut only_y = std::collections::HashMap::new();
    only_y.insert("radius_y".to_string(), Value::Num(25.0));
    assert_eq!(resolve_half_size([50.0, 50.0], Some(&only_y)), [50.0, 25.0]);
}

#[test]
fn half_size_falls_back_to_the_track_without_usable_overrides() {
    // No overrides at all, and a wrongly-typed `size`, both keep the
    // keyframed value rather than collapsing the shape to zero.
    assert_eq!(resolve_half_size([40.0, 30.0], None), [40.0, 30.0]);
    let mut bad = std::collections::HashMap::new();
    bad.insert("size".to_string(), Value::Num(100.0));
    assert_eq!(resolve_half_size([40.0, 30.0], Some(&bad)), [40.0, 30.0]);
}

#[test]
fn text_color_override_reaches_shape_style() {
    // `always { l.stroke = "#30d158" }` arrives in the frame override map as a
    // `Value::Str`. Hex text has to land the same way a tuple does, and a
    // non-color under a color key has to leave the keyframed value alone.
    let track = AnimationTrack::new("l".to_string(), "Line");
    let mut overrides = std::collections::HashMap::new();
    overrides.insert("color".to_string(), Value::Str("#ff2d55".to_string()));
    overrides.insert("stroke".to_string(), Value::Num(0.5));
    let caps = animatix_std::caps_for_type(&track.actor_type).unwrap_or_default();
    let style_tracks = crate::timeline::StyleTracks::default();
    let style = sample_shape_style(&style_tracks, caps, 0, Some(&overrides));
    let near = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
    assert!(
        near(style.color, [1.0, 45.0 / 255.0, 85.0 / 255.0, 1.0]),
        "hex override: {:?}",
        style.color
    );
    assert!(
        near(style.stroke_color, DEFAULT_WHITE),
        "non-color override must not clobber: {:?}",
        style.stroke_color
    );
}

#[test]
fn image_display_transform_maps_pixels_to_display_box() {
    // Map the four corners of the texture rect (drawn by Vello at its pixel
    // size, 1 px = 1 unit) through the transform and check the bbox.
    let corners = |t: &kurbo::Affine, w: f64, h: f64| -> (f64, f64) {
        let xs = [
            (*t * kurbo::Point::new(0.0, 0.0)).x,
            (*t * kurbo::Point::new(w, 0.0)).x,
            (*t * kurbo::Point::new(0.0, h)).x,
            (*t * kurbo::Point::new(w, h)).x,
        ];
        let ys = [
            (*t * kurbo::Point::new(0.0, 0.0)).y,
            (*t * kurbo::Point::new(w, 0.0)).y,
            (*t * kurbo::Point::new(0.0, h)).y,
            (*t * kurbo::Point::new(w, h)).y,
        ];
        let c = |v: &[f64]| {
            v.iter().cloned().fold(f64::NAN, f64::max) - v.iter().cloned().fold(f64::NAN, f64::min)
        };
        (c(&xs), c(&ys))
    };
    // A 24×24 texture in a 240×160 actor box must scale by (240/24, 160/24):
    // the drawn quad then covers exactly the actor box (0..240 × 0..160).
    let t = image_display_transform([240.0, 160.0], [24.0, 24.0], [0.0, 0.0]);
    let (w, h) = corners(&t, 24.0, 24.0);
    assert!((w - 240.0).abs() < 0.001, "width {w}");
    assert!((h - 160.0).abs() < 0.001, "height {h}");

    // Plugin-style command: a 24×24 texture with an explicit 72×72 display
    // box stamps the texture over the full box (scale 3×3).
    let t = image_display_transform([72.0, 72.0], [24.0, 24.0], [0.0, 0.0]);
    let (w, h) = corners(&t, 24.0, 24.0);
    assert!((w - 72.0).abs() < 0.001);
    assert!((h - 72.0).abs() < 0.001);

    // Offset shifts the box in local space before the actor transform.
    let t = image_display_transform([100.0, 100.0], [1.0, 1.0], [30.0, 40.0]);
    let p = t * kurbo::Point::new(0.0, 0.0);
    assert!((p.x - 30.0).abs() < 0.001);
    assert!((p.y - 40.0).abs() < 0.001);

    // Zero / degenerate texture sizes degrade to a 1×1 guard instead of NaN.
    let t = image_display_transform([100.0, 100.0], [0.0, 0.0], [0.0, 0.0]);
    assert!(t.as_coeffs().iter().all(|c| c.is_finite()));
}

#[test]
fn find_primitive_roundtrips() {
    for entry in BUILT_INS.iter() {
        let ty = entry.behavior.type_name();
        let found = find_primitive(ty);
        assert!(found.is_some(), "find_primitive({:?}) returned None", ty);
        assert_eq!(found.unwrap().type_name(), ty);
    }
}

#[test]
fn child_processing_capabilities_cover_special_containers() {
    assert_eq!(
        animatix_std::catalog_lookup("Filter")
            .expect("Filter built-in")
            .child_processing,
        ChildProcessing::Filter
    );
    assert_eq!(
        animatix_std::catalog_lookup("Mask").expect("Mask built-in").child_processing,
        ChildProcessing::Mask
    );
    assert_eq!(
        animatix_std::catalog_lookup("Equation")
            .expect("Equation built-in")
            .child_processing,
        ChildProcessing::Equation
    );
    assert_eq!(
        animatix_std::catalog_lookup("Row").expect("Row built-in").child_processing,
        ChildProcessing::Generic
    );
}

#[test]
fn registry_matches_primitives() {
    // Each BUILT_INS row pairs a behaviour with the catalog card it
    // references by symbol, so positional drift is impossible; this test
    // pins the remaining failure mode — a name mismatch or a set drift
    // (card added without a behaviour, or vice versa). Ordering of
    // either list is presentation-only.
    for entry in BUILT_INS.iter() {
        assert_eq!(
            &*entry.info.type_name,
            entry.behavior.type_name(),
            "BUILT_INS row pairs a behaviour with the wrong catalog card"
        );
    }
    let catalog_names: std::collections::HashSet<&str> =
        primitive_catalog().iter().map(|info| &*info.type_name).collect();
    assert_eq!(
        catalog_names.len(),
        primitive_catalog().len(),
        "duplicate type_name in the animatix-std catalog"
    );
    let behavior_names: std::collections::HashSet<&str> =
        BUILT_INS.iter().map(|entry| entry.behavior.type_name()).collect();
    assert_eq!(catalog_names, behavior_names, "catalog and BUILT_INS drifted apart");
}

#[test]
fn clip_bezpath_concatenates_every_path_and_command() {
    use kurbo::Shape;
    // A multi-path shape must contribute all its geometry to the clip; the
    // earlier default dropped everything but the first path.
    let rect_path = |x: f64| VelloPath {
        path: std::sync::Arc::new(kurbo::Rect::new(x, 0.0, x + 10.0, 10.0).into_path(1e-3)),
        ..VelloPath::default()
    };
    let one = clip_bezpath_from_commands(&[RenderCommand::Paths {
        paths: vec![rect_path(0.0)],
    }])
    .expect("one path");
    let two_commands = clip_bezpath_from_commands(&[
        RenderCommand::Paths {
            paths: vec![rect_path(0.0)],
        },
        RenderCommand::Paths {
            paths: vec![rect_path(20.0)],
        },
    ])
    .expect("two commands");
    let two_paths_one_command = clip_bezpath_from_commands(&[RenderCommand::Paths {
        paths: vec![rect_path(0.0), rect_path(20.0)],
    }])
    .expect("two paths");

    assert_eq!(two_commands.elements().len(), one.elements().len() * 2);
    assert_eq!(two_paths_one_command.elements().len(), one.elements().len() * 2);
    assert!(clip_bezpath_from_commands(&[]).is_none());
}

#[test]
fn primitive_specs_cover_builtins_with_capabilities() {
    let specs = primitive_specs();
    assert_eq!(specs.len(), BUILT_INS.len());
    let rect = specs.iter().find(|spec| spec.type_name == "Rect").expect("Rect is a built-in");
    assert_eq!(rect.category, animatix_syntax::schema::PrimitiveCategory::Shape);
    assert!(rect.capabilities.vector_paths);
    let row = specs.iter().find(|spec| spec.type_name == "Row").expect("Row is a built-in");
    assert_eq!(row.category, animatix_syntax::schema::PrimitiveCategory::Container);
    assert!(row.capabilities.layout_container);
}

#[test]
fn every_catalog_row_derives_caps() {
    for info in animatix_std::CATALOG {
        let caps = animatix_std::caps_from_info(info);
        let _ = caps; // derivation must be total over built-ins
    }
}

#[test]
fn runtime_text_content_crossfade_compiles_both_endpoints() {
    use crate::easing::Easing;
    use crate::renderer::text::{FontContext, TextCompiler, TextKind};
    use crate::timeline::animation_track::TextTracks;
    use crate::timeline::property_track::PropertyTrack;
    use crate::timeline::{AnimationTrack, SceneDimensions};

    let track = AnimationTrack::placeholder("label".to_string());
    let style_tracks = crate::timeline::animation_track::StyleTracks::default();
    let mut content = PropertyTrack::new("Hello".to_string());
    content.add_keyframe(0, "Hello".to_string(), Easing::Linear);
    content.add_keyframe(1000, "Hello".to_string(), Easing::Linear);
    content.add_keyframe(2000, "World".to_string(), Easing::Linear);
    let text = TextTracks {
        text_content: Some(content),
        font_size: Some(PropertyTrack::new(48.0)),
        ..Default::default()
    };

    let font_ctx = FontContext::new();
    let mut text_compiler = TextCompiler::new();
    let asset_cache = crate::timeline::assets::AssetCache::new();
    let ctx = EvaluateCtx {
        track: &track,
        time_ms: 1500,
        local_transform: kurbo::Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 640,
            height: 480,
        },
        background_color: [0.0; 4],
        overrides: None,
        vector_paths: &[],
        asset_cache: &asset_cache,
        highlight: &crate::timeline::animation_track::HighlightTracks::default(),
        shape: None,
        geometry: None,
        style: Some(&style_tracks),
        text: Some(&text),
        property_plan: None,
        caps: animatix_std::caps_for_type(track.actor_type.as_str()).unwrap_or_default(),
        world: None,

        payload: None,
        target_resolver: None,
        legend: None,
    };
    let mut text_ctx = TextCompileCtx {
        text_compiler: &mut text_compiler,
        font_context: &font_ctx,
    };
    let (paths, _) =
        evaluate_text_paths(&ctx, &mut text_ctx, TextKind::Text, 48.0).expect("compile text");
    assert!(paths.len() > 5, "Expected both endpoint glyph sets, got {}", paths.len());
    assert!(
        paths.iter().all(|p| p.opacity > 0.0 && p.opacity < 1.0),
        "Expected midpoint cross-fade opacities"
    );
}
