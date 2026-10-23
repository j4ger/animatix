use std::path::PathBuf;
use std::sync::Arc;

use animatix::composition::BuildTarget;
use animatix::renderer::text::FontContext;
use animatix::timeline::BuildQuality;

use super::*;

/// A web build that pre-seeds the asset cache must satisfy an `Image`
/// actor's build-time load from memory: the sandbox has no filesystem, so
/// without the pre-seed this same scene fails with `MediaLoadFailure`.
#[cfg(feature = "image-decode")]
#[test]
fn preseeded_asset_cache_satisfies_build_time_loads() {
    let png: &[u8] = include_bytes!("../../../../examples/assets/checker.png");
    let mut cache = animatix::timeline::assets::AssetCache::new();
    cache.insert_image_bytes("dot.png", png).expect("checker.png decodes");

    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0.1s
pic: Image, url: "dot.png", size: (400, 240), at: (320, 180)
fade-in pic [300ms]
"#;
    let built = super::build_document_with_assets(
        source,
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
        Some(Arc::new(cache)),
    );

    assert!(
        built.result.ok,
        "the pre-seeded asset must satisfy the build: {:?}",
        built.result.diagnostics
    );
    assert!(
        !built.result.diagnostics.iter().any(|d| d.code == "media-load-failure"),
        "no media-load failure may remain: {:?}",
        built.result.diagnostics
    );
}

/// Same for an `Svg` actor: the pre-registered source must satisfy the
/// build-time parse.
#[cfg(feature = "svg")]
#[test]
fn preseeded_svg_source_satisfies_build_time_parse() {
    let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" width="80" height="80"><circle cx="40" cy="40" r="30" fill="tomato"/></svg>"#;
    let mut cache = animatix::timeline::assets::AssetCache::new();
    cache.insert_svg_source("mark.svg", svg).expect("the inline svg parses");

    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
#0.1s
pic: Svg, url: "mark.svg", size: (300, 300), at: (320, 180)
fade-in pic [300ms]
"#;
    let built = super::build_document_with_assets(
        source,
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
        Some(Arc::new(cache)),
    );

    assert!(
        built.result.ok,
        "the pre-seeded svg must satisfy the build: {:?}",
        built.result.diagnostics
    );
}

const HELLO: &str = include_str!("../../../../examples/basics/00_hello.amx");
const MOTION: &str = include_str!("../../../../examples/basics/03_timing.amx");
const MULTISCENE: &str = include_str!("../../../../examples/composition/14_multiscene.amx");

#[test]
fn builds_self_contained_example_without_errors() {
    let doc = build_document(HELLO, Arc::new(FontContext::new()), BuildQuality::Draft);
    assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
    assert!(doc.result.diagnostics.is_empty());
    assert!(matches!(doc.target, Some(BuildTarget::SingleScene(_))));
    assert!(doc.result.duration_s >= MIN_DURATION_S);
    assert!(doc.result.width > 0 && doc.result.height > 0);
}

#[test]
fn builds_animation_example_with_duration() {
    let doc = build_document(MOTION, Arc::new(FontContext::new()), BuildQuality::Draft);
    assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
    assert!(
        doc.result.duration_s > MIN_DURATION_S,
        "timed example should have real duration"
    );
}

#[test]
fn builds_multiscene_example_as_composition() {
    let doc = build_document(MULTISCENE, Arc::new(FontContext::new()), BuildQuality::Draft);
    assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
    assert!(matches!(doc.target, Some(BuildTarget::MultiScene(_))));
}

#[test]
fn parse_failure_reports_diagnostics_without_target() {
    let doc = build_document(
        "actor this is not valid amx {{{",
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
    );
    assert!(!doc.result.ok);
    assert!(doc.target.is_none());
    assert!(doc.result.diagnostics.iter().any(|d| d.severity == "error"));
}

// A *build* that produced error diagnostics still parses, so it used to
// hand the shell a half-built target. The embed installs whatever target
// it is given, so a typo in the live editor blanked the figure it was told
// to keep — `ok == false` must mean "no document to install".
#[test]
fn build_failure_reports_diagnostics_without_target() {
    let doc = build_document(
        "config { colorscheme: \"editorial-dark\", resolution: (320, 180) }\n\
         ghost: DoesNotExist, size: (10, 10)\n\
         seen: Rect, size: (10, 10), color: accent.primary, at: (20, 20)\n\
         #0.1s\nfade-in seen [100ms]\n",
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
    );
    assert!(!doc.result.ok);
    assert!(
        doc.result
            .diagnostics
            .iter()
            .any(|d| d.severity == "error" && d.code == "unknown-actor-type"),
        "expected the unknown-actor-type error, got {:?}",
        doc.result.diagnostics
    );
    assert!(
        doc.target.is_none(),
        "a failed build must not hand the shell a document to install"
    );
    // The reported extent still describes what the build saw, so the shell
    // can say where it stopped.
    assert!(doc.result.width > 0 && doc.result.height > 0);
}

// The gate is on errors only. Warnings are ordinary — every shipped demo
// scene has some — and must keep the document installable.
#[test]
fn warnings_still_yield_a_document() {
    let doc = build_document(
        "config { colorscheme: \"editorial-dark\", resolution: (320, 180) }\n\
         ghost: Rect, size: (10, 10), color: accent.primary, at: (20, 20)\n\
         seen: Rect, size: (10, 10), color: accent.success, at: (60, 20)\n\
         #0.1s\nfade-in seen [100ms]\n",
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
    );
    assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
    assert!(
        doc.result.diagnostics.iter().any(|d| d.severity == "warning"),
        "the fixture should exercise the warning path: {:?}",
        doc.result.diagnostics
    );
    assert!(matches!(doc.target, Some(BuildTarget::SingleScene(_))));
}

// The transformer demo scenes (web/demos/transformer/scenes) must always
// build cleanly — they are embedded in the shipped demo page.
//
// The scenes import the site theme two directories up (`../../lib/
// theme.amx` -> web/demos/lib/theme.amx), which the browser fetches and
// registers through the missing-imports retry protocol. The test helper
// drives that same protocol: build, register every reported key from the
// theme source, rebuild until the graph closes.
fn build_demo_scene(source: &str) -> BuiltDocument {
    let mut modules: Vec<(PathBuf, String)> = Vec::new();
    for _ in 0..4 {
        let mut doc = build_document_with_modules(
            source,
            &modules,
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
            None,
        );
        if doc.result.missing_imports.is_empty() {
            return doc;
        }
        for key in std::mem::take(&mut doc.result.missing_imports) {
            let content = if key.ends_with("theme.amx") {
                include_str!("../../../../web/demos/lib/theme.amx")
            } else if key.ends_with("ambience.amx") {
                include_str!("../../../../web/demos/lib/ambience.amx")
            } else if key.ends_with("components.amx") {
                include_str!("../../../../web/demos/lib/components.amx")
            } else {
                panic!("demo scenes should only import site demo lib files, got {key}");
            };
            let path = PathBuf::from(&key);
            if !modules.iter().any(|(p, _)| *p == path) {
                modules.push((path, content.into()));
            }
        }
    }
    panic!("the demo scene's import graph never closed: {source:?}");
}

#[test]
fn transformer_demo_scenes_build_cleanly() {
    const SCENES: &[(&str, &str)] = &[
        (
            "overview",
            include_str!("../../../../web/demos/transformer/scenes/overview.amx"),
        ),
        ("tokens", include_str!("../../../../web/demos/transformer/scenes/tokens.amx")),
        (
            "positional",
            include_str!("../../../../web/demos/transformer/scenes/positional.amx"),
        ),
        (
            "attention",
            include_str!("../../../../web/demos/transformer/scenes/attention.amx"),
        ),
        (
            "multihead",
            include_str!("../../../../web/demos/transformer/scenes/multihead.amx"),
        ),
        (
            "feedforward",
            include_str!("../../../../web/demos/transformer/scenes/feedforward.amx"),
        ),
        (
            "pipeline",
            include_str!("../../../../web/demos/transformer/scenes/pipeline.amx"),
        ),
    ];
    for (name, source) in SCENES {
        let doc = build_demo_scene(source);
        assert!(
            doc.result.ok,
            "demo scene '{name}' failed to build: {:?}",
            doc.result.diagnostics
        );
        assert!(
            doc.result.diagnostics.is_empty(),
            "demo scene '{name}' has diagnostics: {:?}",
            doc.result.diagnostics
        );
        assert!(doc.result.duration_s > MIN_DURATION_S, "scene '{name}' has no duration");
    }
}

/// The scrubber's landmarks: single-scene documents report their
/// scene-level keyframe times, compositions report scene starts and
/// transition windows.
#[test]
fn load_result_markers_expose_timeline_landmarks() {
    const TOKENS: &str = include_str!("../../../../web/demos/transformer/scenes/tokens.amx");
    let doc = build_demo_scene(TOKENS);
    assert!(doc.result.ok, "tokens should build: {:?}", doc.result.diagnostics);
    let ts: Vec<f64> = doc
        .result
        .markers
        .iter()
        .filter(|m| m.kind == "keyframe")
        .map(|m| m.t)
        .collect();
    for expected in [0.1, 1.2, 2.1, 3.0, 4.4, 5.2] {
        assert!(
            ts.iter().any(|t| (t - expected).abs() < 1e-3),
            "tokens keyframes must include {expected}s: {ts:?}"
        );
    }
    assert!(
        doc.result.markers.iter().all(|m| m.dur == 0.0),
        "single-scene markers are points: {:?}",
        doc.result.markers
    );

    const MULTISCENE: &str = include_str!("../../../../examples/composition/14_multiscene.amx");
    let doc = build_document(MULTISCENE, Arc::new(FontContext::new()), BuildQuality::Draft);
    assert!(doc.result.ok);
    let kinds: Vec<&str> = doc.result.markers.iter().map(|m| m.kind.as_str()).collect();
    assert!(
        kinds.contains(&"scene"),
        "a composition must mark scene starts: {:?}",
        doc.result.markers
    );
    // Sorted, and all markers inside the document's duration.
    let ts: Vec<f64> = doc.result.markers.iter().map(|m| m.t).collect();
    let mut sorted = ts.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(ts, sorted);
    assert!(ts.iter().all(|t| *t >= 0.0 && *t <= doc.result.duration_s + 1e-6));
    let _ = kinds;
}

#[test]
fn load_result_exposes_step_pause_points_and_markers() {
    let source = r#"
#0s
t: Text, text: "Step 0", at: (10, 10)
#step 1
#500ms
#pause
#1s
#step 2
"#;
    let doc = build_document(source, Arc::new(FontContext::new()), BuildQuality::Draft);
    assert!(doc.result.ok, "build diagnostics: {:?}", doc.result.diagnostics);
    assert_eq!(doc.result.pause_points, vec![0.0, 0.5, 1.0]);

    let step_markers: Vec<f64> =
        doc.result.markers.iter().filter(|m| m.kind == "step").map(|m| m.t).collect();
    assert_eq!(step_markers, vec![0.0, 0.5, 1.0]);
}

#[test]
fn missing_import_is_reported_as_a_fetch_key_not_read_from_disk() {
    let doc = build_document(
        "import \"../lib/nonexistent.amx\" as ghost\n",
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
    );
    assert!(!doc.result.ok);
    assert!(doc.target.is_none(), "an unsupplied import builds no document");
    assert!(
        doc.result.diagnostics.iter().any(|d| d.message.contains("nonexistent.amx")),
        "the diagnostic names the missing module: {:?}",
        doc.result.diagnostics
    );
}

/// The shell-side import protocol: a load that stops on an unsupplied
/// import reports the resolved key in `missing_imports`; registering the
/// fetched text under that key and retrying eventually closes the graph —
/// including transitively (a supplied module importing another module).
#[test]
fn missing_imports_close_the_graph_through_the_retry_protocol() {
    let entry = "import \"./mods/greet.amx\" as greet\n";
    let greet = "import \"./word.amx\" as word\n\nexport component Hello { }\n";
    let word = "export component Word { }\n";

    // Round 1: nothing supplied — the entry's own import is missing.
    let doc = build_document(entry, Arc::new(FontContext::new()), BuildQuality::Draft);
    assert!(!doc.result.ok);
    assert_eq!(doc.result.missing_imports, ["mods/greet.amx".to_string()]);

    // Round 2: greet.amx supplied — its own import surfaces next.
    let modules = vec![(PathBuf::from("mods/greet.amx"), greet.to_string())];
    let doc = build_document_with_modules(
        entry,
        &modules,
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
        None,
    );
    assert!(!doc.result.ok);
    assert_eq!(doc.result.missing_imports, ["mods/word.amx".to_string()]);

    // Round 3: closure — the graph loads.
    let modules = vec![
        (PathBuf::from("mods/greet.amx"), greet.to_string()),
        (PathBuf::from("mods/word.amx"), word.to_string()),
    ];
    let doc = build_document_with_modules(
        entry,
        &modules,
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
        None,
    );
    assert!(doc.result.ok, "the closed graph must build: {:?}", doc.result.diagnostics);
    assert!(doc.result.missing_imports.is_empty());
}
