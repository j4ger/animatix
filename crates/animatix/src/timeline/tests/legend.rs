//! Tests for the Legend primitive

use super::support::{build, build_clean};
use super::*;
use crate::primitives::{EvaluateCtx, LEGEND, Primitive, RenderCommand, TextCompileCtx};

fn legend_component<'a>(timeline: &'a Timeline, label: &str) -> &'a crate::timeline::LegendTracks {
    timeline
        .entity(label)
        .and_then(|entity| entity.legend())
        .expect("legend component")
}

/// Build a minimal timeline with a single Legend actor.
fn build_legend_timeline() -> Timeline {
    build_clean(
        r#"
config { colorscheme: "editorial-dark" }
#0s
legend: Legend
"#,
    )
}

#[test]
fn test_legend_wrapped_label_respects_max_width() {
    use kurbo::{Affine, Shape};
    let legend = crate::timeline::LegendTracks {
        entries: vec![(
            "A very long series label that should wrap".to_string(),
            [1.0, 0.0, 0.0, 1.0],
        )],
        text_max_width: 80.0,
        ..Default::default()
    };

    let asset_cache = crate::timeline::assets::AssetCache::new();
    let geometry = crate::timeline::animation_track::GeometryTracks::default();
    let style = crate::timeline::animation_track::StyleTracks::default();
    let shape = crate::timeline::animation_track::ShapeTracks::default();
    let text = crate::timeline::animation_track::TextTracks::default();
    let property_plan = crate::timeline::plan::PropertyPlan::default();
    let ctx = EvaluateCtx {
        id: crate::timeline::EntityId::INVALID,
        label: "legend",
        caps: animatix_std::caps_for_type("Legend").unwrap_or_default(),
        time_ms: 0,
        local_transform: Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 1920,
            height: 1080,
        },
        background_color: [0.04, 0.06, 0.09, 1.0],
        overrides: None,
        vector_paths: &[],
        asset_cache: &asset_cache,
        highlight: None,
        property_plan: &property_plan,
        world: None,
        shape: Some(&shape),
        text: Some(&text),
        geometry: &geometry,
        style: &style,

        payload: None,
        target_resolver: None,
        legend: Some(&legend),
    };
    let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
    let mut text_compiler = crate::renderer::text::TextCompiler::new();
    let mut text_ctx = TextCompileCtx {
        text_compiler: &mut text_compiler,
        font_context: &font_context,
    };

    let commands = LEGEND
        .evaluate(&ctx, Some(&mut text_ctx))
        .expect("evaluate should succeed")
        .unwrap();
    let text_command = commands
        .iter()
        .find_map(|cmd| match cmd {
            RenderCommand::Text { paths, .. } => Some(paths.as_ref()),
            _ => None,
        })
        .expect("legend should emit a text command");
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    for text_path in text_command {
        let bounds = text_path.path.bounding_box();
        min_x = min_x.min(bounds.x0);
        max_x = max_x.max(bounds.x1);
    }
    assert!((max_x - min_x) <= 82.0, "wrapped label exceeded max width");
}

#[test]
fn test_legend_union_property_sets_actor_mode() {
    let report = build(
        r#"
config { colorscheme: "editorial-dark" }
#0s
hidden_series: Line, legend: false
named_series: Line, legend: "Revenue"
"#,
    );
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    let hidden = timeline.get_entity_id("hidden_series").expect("hidden series track");
    assert_eq!(
        crate::timeline::legend::legend_mode_for_track(
            hidden,
            timeline.world.meta(hidden).first_seen_ms,
            &timeline.world,
        ),
        super::super::LegendMode::Hidden
    );
    let named = timeline.get_entity_id("named_series").expect("named series track");
    assert_eq!(
        crate::timeline::legend::legend_mode_for_track(
            named,
            timeline.world.meta(named).first_seen_ms,
            &timeline.world,
        ),
        super::super::LegendMode::Label("Revenue".to_string())
    );
}

#[test]
fn test_legend_basic_rendering() {
    // Test that a Legend renders color swatches and labels
    let timeline = build_legend_timeline();

    let track = timeline.get_entity_id("legend").expect("legend track should exist");
    assert_eq!(timeline.world.actor_type(track), "Legend", "track kind should be Legend");

    // A legend-only scene has no color-bearing candidates.
    assert!(
        legend_component(&timeline, "legend").entries.is_empty(),
        "legend should have no entries"
    );

    // Evaluate at time 0 — should not panic and should produce RenderCommands
    let dims = SceneDimensions {
        width: 1920,
        height: 1080,
    };
    let _scene = timeline.evaluate(0.0, dims);
}

#[test]
fn test_legend_entries() {
    // Verify that legend entries are stored correctly
    let timeline = build_legend_timeline();

    // A legend-only scene has no color-bearing candidates.
    assert!(
        legend_component(&timeline, "legend").entries.is_empty(),
        "legend should have no entries"
    );
}

#[test]
fn test_legend_empty() {
    // Verify that a Legend with no entries renders nothing.
    // Build a track manually and call evaluate directly to bypass
    // the build-time placeholder insertion.
    use kurbo::Affine;
    let legend = crate::timeline::LegendTracks::default();

    // Ensure entries are empty (default)
    assert!(legend.entries.is_empty(), "fresh legend component should have empty entries");

    let asset_cache = crate::timeline::assets::AssetCache::new();
    let geometry = crate::timeline::animation_track::GeometryTracks::default();
    let style = crate::timeline::animation_track::StyleTracks::default();
    let shape = crate::timeline::animation_track::ShapeTracks::default();
    let text = crate::timeline::animation_track::TextTracks::default();
    let property_plan = crate::timeline::plan::PropertyPlan::default();
    let ctx = EvaluateCtx {
        id: crate::timeline::EntityId::INVALID,
        label: "empty_legend",
        caps: animatix_std::caps_for_type("Legend").unwrap_or_default(),
        time_ms: 0,
        local_transform: Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 1920,
            height: 1080,
        },
        background_color: [0.04, 0.06, 0.09, 1.0],
        overrides: None,
        vector_paths: &[],
        asset_cache: &asset_cache,
        highlight: None,
        property_plan: &property_plan,
        world: None,
        shape: Some(&shape),
        text: Some(&text),
        geometry: &geometry,
        style: &style,
        payload: None,
        target_resolver: None,
        legend: Some(&legend),
    };

    let result = LEGEND.evaluate(&ctx, None).expect("evaluate should succeed");
    assert!(result.is_none(), "empty legend should return None from evaluate");
}

#[test]
fn test_legend_auto_extraction_from_colored_actors() {
    let report = build(
        r#"
config { colorscheme: "editorial-dark" }
#0s
line_a: Line, color: red, legend: "Revenue"
line_b: Line, color: green, legend: false
line_c: Line, color: blue, legend: true
legend: Legend
"#,
    );
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;

    assert_eq!(
        legend_component(&timeline, "legend").entries,
        vec![
            ("Revenue".to_string(), [1.0, 0.0, 0.0, 1.0]),
            ("Line C".to_string(), [0.0, 0.0, 1.0, 1.0]),
        ]
    );
}

#[test]
fn test_legend_style_properties_are_parsed() {
    let report = build(
        r#"
config { colorscheme: "editorial-dark" }
#0s
legend: Legend, title: "Metrics", font_size: 18, label_color: white, swatch_size: 20, gap: 12, text_max_width: 180
"#,
    );
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let track = timeline.get_entity_id("legend").expect("legend track");
    assert_eq!(legend_component(&timeline, "legend").title, "Metrics");
    assert_eq!(legend_component(&timeline, "legend").font_size, 18.0);
    assert_eq!(legend_component(&timeline, "legend").label_color, Some([1.0, 1.0, 1.0, 1.0]));
    assert_eq!(legend_component(&timeline, "legend").swatch_size, 20.0);
    assert_eq!(legend_component(&timeline, "legend").gap, 12.0);
    assert_eq!(legend_component(&timeline, "legend").text_max_width, 180.0);

    use crate::timeline::property_engine::PropertyValue;
    use crate::timeline::property_registry::ActorField;
    let read =
        |key: &'static str| timeline.world.read_property_value(track, ActorField::Tagged(key), 0);
    assert_eq!(read("legend_title"), Some(PropertyValue::String("Metrics".to_string())));
    assert_eq!(read("legend_font_size"), Some(PropertyValue::F32(18.0)));
    assert_eq!(read("legend_label_color"), Some(PropertyValue::Color([1.0, 1.0, 1.0, 1.0])));
    assert_eq!(read("legend_swatch_size"), Some(PropertyValue::F32(20.0)));
    assert_eq!(read("legend_gap"), Some(PropertyValue::F32(12.0)));
    assert_eq!(read("legend_text_max_width"), Some(PropertyValue::F32(180.0)));
}

#[test]
fn test_legend_excludes_full_viewport_background() {
    let report = build(
        r#"
config { colorscheme: "editorial-dark" }
#0s
backdrop: Rect, size: ("100%", "100%"), color: white
legend: Legend
"#,
    );
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    assert!(
        legend_component(&timeline, "legend").entries.is_empty(),
        "full-viewport background should be excluded"
    );
}

#[test]
fn test_legend_true_force_includes_background() {
    let report = build(
        r#"
config { colorscheme: "editorial-dark" }
#0s
backdrop: Rect, size: ("100%", "100%"), color: white, legend: true
legend: Legend
"#,
    );
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    assert_eq!(
        legend_component(&timeline, "legend").entries.len(),
        1,
        "legend:true should force-include a background"
    );
}

#[test]
fn test_legend_source_order_is_preserved() {
    let report = build(
        r#"
config { colorscheme: "editorial-dark" }
#0s
line_z: Line, color: red
line_a: Line, color: green
legend: Legend
"#,
    );
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    assert_eq!(
        legend_component(&timeline, "legend")
            .entries
            .iter()
            .map(|(label, _)| label.clone())
            .collect::<Vec<_>>(),
        vec!["Line Z".to_string(), "Line A".to_string()]
    );
}

#[test]
fn example_legend_entries_are_scanned() {
    let source = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../examples/projects/legend_example.amx"
    ))
    .expect("legend example should exist");
    let report = build(&source);
    assert!(
        report.diagnostics.is_empty(),
        "Expected no diagnostics, got: {:?}",
        report.diagnostics
    );
    let timeline = report.output;
    let track = timeline.get_entity_id("legend").expect("legend track should exist");
    let geometry = timeline.world.geometry(track);
    assert_eq!(
        legend_component(&timeline, "legend").entries,
        vec![
            ("Series A".to_string(), [1.0, 0.0, 0.0, 1.0]),
            ("Series B".to_string(), [0.0, 1.0, 0.0, 1.0]),
            ("Series C".to_string(), [0.0, 0.0, 1.0, 1.0]),
        ]
    );
    assert_eq!(
        geometry.position.get(0, [0.0, 0.0]),
        [320.0, 100.0],
        "Legend 'at' should resolve to a stored position"
    );
}

#[test]
fn test_legend_render_commands_produced() {
    // Create a track with entries and verify evaluate returns Some(commands)
    use kurbo::Affine;

    // Set manual entries to exercise label rendering without a scene build.
    let legend = crate::timeline::LegendTracks {
        entries: vec![
            ("Series A".to_string(), [1.0, 0.0, 0.0, 1.0]),
            ("Series B".to_string(), [0.0, 1.0, 0.0, 1.0]),
            ("Series C".to_string(), [0.0, 0.0, 1.0, 1.0]),
        ],
        ..Default::default()
    };

    let asset_cache = crate::timeline::assets::AssetCache::new();
    let geometry = crate::timeline::animation_track::GeometryTracks::default();
    let style = crate::timeline::animation_track::StyleTracks::default();
    let shape = crate::timeline::animation_track::ShapeTracks::default();
    let text = crate::timeline::animation_track::TextTracks::default();
    let property_plan = crate::timeline::plan::PropertyPlan::default();
    let ctx = EvaluateCtx {
        id: crate::timeline::EntityId::INVALID,
        label: "legend",
        caps: animatix_std::caps_for_type("Legend").unwrap_or_default(),
        time_ms: 0,
        local_transform: Affine::IDENTITY,
        opacity: 1.0,
        scene_dimensions: SceneDimensions {
            width: 1920,
            height: 1080,
        },
        background_color: [0.04, 0.06, 0.09, 1.0],
        overrides: None,
        vector_paths: &[],
        asset_cache: &asset_cache,
        highlight: None,
        property_plan: &property_plan,
        world: None,
        shape: Some(&shape),
        text: Some(&text),
        geometry: &geometry,
        style: &style,
        payload: None,
        target_resolver: None,
        legend: Some(&legend),
    };

    let font_context = std::sync::Arc::new(crate::renderer::text::FontContext::new());
    let mut text_compiler = crate::renderer::text::TextCompiler::new();
    let mut text_ctx = TextCompileCtx {
        text_compiler: &mut text_compiler,
        font_context: &font_context,
    };

    let result = LEGEND.evaluate(&ctx, Some(&mut text_ctx)).expect("evaluate should succeed");
    assert!(result.is_some(), "legend with entries should return Some(commands)");

    let commands = result.unwrap();
    assert_eq!(commands.len(), 6, "3 entries should produce 6 RenderCommands (2 per entry)");

    for (i, cmd) in commands.iter().enumerate() {
        match cmd {
            RenderCommand::Paths { paths } => {
                assert_eq!(paths.len(), 1, "each swatch command should contain 1 path");
            },
            RenderCommand::Text { paths, .. } => {
                assert!(!paths.is_empty(), "legend label should compile glyph paths");
            },
            _ => panic!("Command {} should be Paths or Text variant", i),
        }
    }
}

#[test]
fn legend_text_max_width_assignment_writes_tagged_legend_track() {
    let source = r#"
legend: Legend, at: (320, 100), title: "Series", text_max_width: 200

#1s
legend.text_max_width = 120
"#;
    let timeline = build_clean(source);
    let track = timeline.get_entity_id("legend").expect("legend track");
    let read = |time_ms| {
        timeline.world.read_property_value(
            track,
            crate::timeline::property_registry::ActorField::Tagged("legend_text_max_width"),
            time_ms,
        )
    };
    assert_eq!(
        read(0),
        Some(crate::timeline::PropertyValue::F32(200.0)),
        "Legend declaration must keep using its tagged text_max_width lane"
    );
    assert_eq!(
        read(1000),
        Some(crate::timeline::PropertyValue::F32(120.0)),
        "Legend runtime text_max_width assignment must keep using its tagged lane"
    );
}
