use super::*;

use animatix_syntax::typing::Type;

#[test]
fn extension_value_to_kind_uses_manifest_types() {
    let enum_value = animatix::timeline::PropertyValue::String("right".to_string());
    assert_eq!(
        extension_value_to_kind(
            enum_value,
            &Type::Enum(vec!["left".to_string(), "right".to_string()])
        ),
        PropertyKind::EnumOwned {
            variants: vec!["left".to_string(), "right".to_string()],
            value: "right".to_string(),
        }
    );

    assert_eq!(
        extension_value_to_kind(animatix::timeline::PropertyValue::Bool(true), &Type::Bool),
        PropertyKind::Bool(true)
    );
    assert_eq!(
        extension_value_to_kind(
            animatix::timeline::PropertyValue::Color([1.0, 0.0, 0.0, 1.0]),
            &Type::Color
        ),
        PropertyKind::Color([1.0, 0.0, 0.0, 1.0])
    );
    assert_eq!(
        extension_value_to_kind(animatix::timeline::PropertyValue::Vec2([3.0, 4.0]), &Type::Vec2),
        PropertyKind::Vec2 { x: 3.0, y: 4.0 }
    );
}

#[test]
fn build_property_groups_for_graph_includes_plot_domain() {
    let timeline = animatix::timeline::Timeline::new();
    let mut track = AnimationTrack::new("g".into(), "Graph");
    track.caps.plot_host = true;
    let groups = build_property_groups(&timeline, &track, 0);
    let plot_group = groups.iter().find(|g| g.name == "Plot & Coordinates");
    assert!(plot_group.is_some(), "Graph should have a Plot & Coordinates group");
    let plot_group = plot_group.unwrap();
    assert!(
        plot_group.properties.iter().any(|p| p.name == "x_domain"),
        "Plot group should have x_domain"
    );
    assert!(
        plot_group.properties.iter().any(|p| p.name == "y_domain"),
        "Plot group should have y_domain"
    );
}

#[test]
fn build_property_groups_for_container_includes_layout() {
    let timeline = animatix::timeline::Timeline::new();
    let mut track = AnimationTrack::new("r".into(), "Row");
    track.caps.is_container = true;
    let groups = build_property_groups(&timeline, &track, 0);
    let layout_group = groups.iter().find(|g| g.name == "Layout");
    assert!(layout_group.is_some(), "Row should have a Layout group");
    let layout_group = layout_group.unwrap();
    assert!(
        layout_group.properties.iter().any(|p| p.name == "gap"),
        "Layout group should have gap"
    );
}

#[test]
fn build_property_groups_for_callout_includes_annotation() {
    let timeline = animatix::timeline::Timeline::new();
    let mut track = AnimationTrack::new("c".into(), "Callout");
    track.caps.callout = true;
    let groups = build_property_groups(&timeline, &track, 0);
    let annotation_group = groups.iter().find(|g| g.name == "Annotation");
    assert!(annotation_group.is_some(), "Callout should have an Annotation group");
}

#[test]
fn build_property_groups_for_captions_actor() {
    let timeline = animatix::timeline::Timeline::new();
    let track = AnimationTrack::new("subs".into(), "Captions");
    let groups = build_property_groups(&timeline, &track, 0);
    let cap_group = groups.iter().find(|g| g.name == "Captions");
    assert!(cap_group.is_some(), "Captions should have a Captions group");
    let cap_group = cap_group.unwrap();
    let caption_mode = cap_group.properties.iter().find(|p| p.name == "caption_mode");
    assert!(caption_mode.is_some(), "Captions group should have caption_mode");
    assert!(
        matches!(caption_mode.unwrap().kind, PropertyKind::Enum { .. }),
        "caption_mode should be Enum dropdown"
    );
    let backdrop = cap_group.properties.iter().find(|p| p.name == "backdrop");
    assert!(backdrop.is_some(), "Captions group should have backdrop");
    assert!(
        matches!(backdrop.unwrap().kind, PropertyKind::Bool(_)),
        "backdrop should be Bool kind"
    );
}

#[test]
fn rect_has_no_text_group() {
    let timeline = animatix::timeline::Timeline::new();
    let track = AnimationTrack::new("box".into(), "Rect");
    let groups = build_property_groups(&timeline, &track, 0);
    assert!(
        !groups.iter().any(|g| g.name == "Text"),
        "a Rect has no text properties; `solo`/`idle` must not create a Text group"
    );
}

#[test]
fn solo_lands_in_visibility() {
    let timeline = animatix::timeline::Timeline::new();
    let track = AnimationTrack::new("box".into(), "Rect");
    let groups = build_property_groups(&timeline, &track, 0);
    let visibility = groups.iter().find(|g| g.name == "Visibility");
    assert!(visibility.is_some(), "Rect should have a Visibility group for `solo`");
    assert!(
        visibility.unwrap().properties.iter().any(|p| p.name == "solo"),
        "Visibility group should contain `solo`"
    );
}

#[test]
fn idle_lands_in_geometry() {
    let timeline = animatix::timeline::Timeline::new();
    let track = AnimationTrack::new("box".into(), "Rect");
    let groups = build_property_groups(&timeline, &track, 0);
    let transform = groups.iter().find(|g| g.name == "Transform");
    assert!(transform.is_some(), "Rect should have a Transform group");
    assert!(
        transform.unwrap().properties.iter().any(|p| p.name == "idle"),
        "Transform (motion) group should contain `idle`"
    );
}
