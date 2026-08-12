use std::path::PathBuf;

use super::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode};

fn write_test_svg(name: &str, width: u32, rect_width: u32) -> PathBuf {
    let dir = std::env::temp_dir().join("animatix_svg_url_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let source = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="20">
  <rect x="0" y="0" width="{rect_width}" height="10"/>
</svg>
"#
    );
    std::fs::write(&path, source).unwrap();
    path
}

fn write_test_png(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("animatix_image_usage_tests");
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(name);
    let rgba = ::image::RgbaImage::from_raw(4, 4, vec![255; 64]).unwrap();
    rgba.save(&path).unwrap();
    path
}

fn build_svg_timeline(source: &str) -> (Timeline, Vec<Diagnostic>) {
    let (ast, parse_errors) = animatix_syntax::parser::parse_source(source);
    assert!(parse_errors.is_empty(), "Parse errors: {:?}", parse_errors);
    let ast = ast.expect("parsed AST");
    let report = Timeline::build_with_diagnostics(&ast, &std::collections::HashMap::new());
    (report.output, report.diagnostics)
}

#[test]
fn timed_svg_url_assignment_keyframes_paths_and_size() {
    let svg_a = write_test_svg("svg_url_a.svg", 40, 10);
    let svg_b = write_test_svg("svg_url_b.svg", 80, 20);
    let source = format!(
        r#"
#0s
icon: Svg {{ url: "{}" }}

#1s
icon.url = "{}" [1s]
"#,
        svg_a.display(),
        svg_b.display(),
    );

    let (timeline, diagnostics) = build_svg_timeline(&source);
    assert!(diagnostics.is_empty(), "Expected no build diagnostics, got: {:?}", diagnostics);

    let track = timeline.tracks.get("icon").expect("icon track should exist");
    assert!(track.svg_paths_at(0).is_some(), "static SVG should be available at t=0");
    assert!(track.svg_paths_at(1000).is_some(), "start keyframe should preserve paths");
    assert!(track.svg_paths_at(2000).is_some(), "assignment should provide new paths");

    let size_at_0 = track.geometry.size.get(0, DEFAULT_LAYOUT_HALF_SIZE);
    let size_at_2000 = track.geometry.size.get(2000, DEFAULT_LAYOUT_HALF_SIZE);
    assert!(
        size_at_2000[0] > size_at_0[0],
        "assigned SVG should grow the measured half-size: {} -> {}",
        size_at_0[0],
        size_at_2000[0],
    );

    assert_eq!(
        property_keyframe_times(track, ActorField::ImageData),
        vec![1000, 2000],
        "timed Svg.url assignment should expose its keyframes through the url property"
    );
    assert!(property_has_keyframe_at(track, ActorField::ImageData, 1000));
    assert_eq!(property_keyframe_count(track, ActorField::ImageData), 2);

    let collected = collect_all_keyframe_times(track);
    assert!(collected.iter().any(|t| (*t - 1.0).abs() < 0.001));
    assert!(collected.iter().any(|t| (*t - 2.0).abs() < 0.001));
}

#[test]
fn repeated_svg_url_assignments_are_visible_to_property_introspection() {
    let svg_a = write_test_svg("svg_url_repeat_a.svg", 40, 10);
    let svg_b = write_test_svg("svg_url_repeat_b.svg", 80, 20);
    let source = format!(
        r#"
#0s
icon: Svg {{ url: "{}" }}

#0.5s
icon.url = "{}"

#1s
icon.url = "{}" [500ms]
"#,
        svg_a.display(),
        svg_b.display(),
        svg_a.display(),
    );

    let (timeline, diagnostics) = build_svg_timeline(&source);
    assert!(diagnostics.is_empty(), "Expected no build diagnostics, got: {:?}", diagnostics);

    let track = timeline.tracks.get("icon").expect("icon track should exist");
    assert!(track.has_keyframe_at("url", 500));
    assert!(track.has_keyframes_for("url"));
    assert_eq!(track.list_keyframes("url"), vec![500, 1000, 1500]);
    assert_eq!(property_keyframe_times(track, ActorField::ImageData), vec![500, 1000, 1500]);
}

#[test]
fn invalid_svg_url_assignment_reports_media_load_failure() {
    let svg_a = write_test_svg("svg_url_bad_a.svg", 40, 10);
    let source = format!(
        r#"
#0s
icon: Svg {{ url: "{}" }}

#1s
icon.url = "/nonexistent/animatix-missing.svg"
"#,
        svg_a.display(),
    );

    let (timeline, diagnostics) = build_svg_timeline(&source);
    assert!(
        diagnostics.iter().any(|d| d.code == DiagnosticCode::MediaLoadFailure),
        "Expected MediaLoadFailure diagnostic, got: {:?}",
        diagnostics
    );

    let track = timeline.tracks.get("icon").expect("icon track should exist");
    assert_eq!(
        property_keyframe_count(track, ActorField::ImageData),
        0,
        "failed assignment should not add Svg.url keyframes"
    );
}

#[test]
fn asset_usage_tracks_svg_image_and_audio_actors() {
    let svg = write_test_svg("asset_usage.svg", 40, 10);
    let png = write_test_png("asset_usage.png");
    let audio = std::env::temp_dir().join("animatix_audio_usage.wav");
    let source = format!(
        r#"
#0s
icon: Svg, url: "{}"
photo: Image, url: "{}"
music: Audio, source: "{}"
"#,
        svg.display(),
        png.display(),
        audio.display(),
    );

    let (timeline, diagnostics) = build_svg_timeline(&source);
    assert!(diagnostics.is_empty(), "Expected no build diagnostics, got: {:?}", diagnostics);

    let svg_path = svg.display().to_string();
    let png_path = png.display().to_string();
    let audio_path = audio.display().to_string();

    assert_eq!(timeline.asset_cache().assets_for("icon").collect::<Vec<_>>(), vec![&svg_path]);
    assert_eq!(timeline.asset_cache().assets_for("photo").collect::<Vec<_>>(), vec![&png_path]);
    assert_eq!(
        timeline.asset_cache().assets_for("music").collect::<Vec<_>>(),
        vec![&audio_path]
    );

    let usage: Vec<_> = timeline.asset_usage().map(|(_, actors)| actors.len()).collect();
    assert!(usage.iter().all(|count| *count == 1));
}
