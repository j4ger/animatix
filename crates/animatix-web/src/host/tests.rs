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
            } else if key.ends_with("stage.amx") {
                // The house-style stage furniture (web/DESIGN.md §1); every
                // rewritten walkthrough scene imports it, so the demo-scene
                // build test has to be able to close that import graph.
                include_str!("../../../../web/demos/lib/stage.amx")
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
    for expected in [0.1, 1.1, 2.1, 3.0, 4.4, 5.2] {
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

// The web player's contract predates the shared `BuildTarget` entry point: a
// persisted actor in a single-scene document simply resets between loops, and
// the player never surfaced `PersistTargetNotCarried`. Keep that quiet — the
// embed renders this diagnostic list as user-facing output, so a refactor must
// not start talking about a successor scene the player does not have.
#[test]
fn single_scene_persist_target_stays_unwarned() {
    let doc = build_document(
        "config { colorscheme: \"editorial-dark\", resolution: (320, 180) }\n\
         #0s\n\
         badge: Ellipse, size: (30, 30), at: (60, 60)\n\
         fade-in badge [100ms]\n\
         #1s\n\
         persist badge\n",
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
    );
    assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
    assert!(matches!(doc.target, Some(BuildTarget::SingleScene(_))));
    assert!(
        !doc.result.diagnostics.iter().any(|d| d.code == "persist-target-not-carried"),
        "single-scene web documents keep persisted actors quiet, got {:?}",
        doc.result.diagnostics
    );
}

// ── build-time variable overrides: the CLI's `--set`, driven from a page ─────

/// The web vars channel *is* the CLI's `--set NAME=VALUE`: an override shadows
/// the scene's own `let` default and reaches the build environment.
#[test]
fn vars_shadow_the_authored_let_defaults() {
    const SOURCE: &str = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360), duration: 1.0 }

let amp = 200
let label = "default"

#0s
dot: Ellipse, size: (amp, amp), at: (320, 180)
fade-in dot [100ms]
"#;
    let vars = vec![
        ("amp".to_string(), "320".to_string()),
        ("label".to_string(), "'overridden'".to_string()),
    ];
    let doc = build_document_with_vars(
        SOURCE,
        &[],
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
        None,
        &vars,
    );
    assert!(doc.result.ok, "the overrides must build: {:?}", doc.result.diagnostics);
    let Some(BuildTarget::SingleScene(timeline)) = &doc.target else {
        panic!("expected a single-scene document");
    };
    assert_eq!(timeline.env().get("amp"), Some(animatix::timeline::Value::Num(320.0)));
    assert_eq!(
        timeline.env().get("label"),
        Some(animatix::timeline::Value::Str("overridden".to_string())),
        "a single-quoted literal reaches the environment as its text"
    );

    // The same source with no overrides keeps the authored defaults, so an
    // empty var list is exactly the pre-existing behaviour.
    let doc = build_document(SOURCE, Arc::new(FontContext::new()), BuildQuality::Draft);
    assert!(doc.result.ok, "diagnostics: {:?}", doc.result.diagnostics);
    let Some(BuildTarget::SingleScene(timeline)) = &doc.target else {
        panic!("expected a single-scene document");
    };
    assert_eq!(timeline.env().get("amp"), Some(animatix::timeline::Value::Num(200.0)));
}

/// What [`vars::string_expr`] renders must survive the engine's parser as the
/// same text — the round-trip the quote-picking rule exists for. The DSL's
/// literals are raw (an escape pair is skipped, never interpreted), so this is
/// the assertion that keeps a page's string from reaching the scene corrupted.
#[test]
fn a_rendered_string_var_round_trips_through_the_engine_parser() {
    for input in [
        "Q3 Revenue",
        "say \"hi\"",
        "it's fine",
        "",
        "unicode ✓",
        "#ff8666",
    ] {
        let Some(literal) = vars::string_expr(input) else {
            panic!("'{input}' must render as a literal");
        };
        let doc = build_document_with_vars(
            "#0s\ndot: Ellipse, size: (30, 30), at: (60, 60)\n",
            &[],
            Arc::new(FontContext::new()),
            BuildQuality::Draft,
            None,
            &[("label".to_string(), literal)],
        );
        assert!(doc.result.ok, "'{input}' must build: {:?}", doc.result.diagnostics);
        let Some(BuildTarget::SingleScene(timeline)) = &doc.target else {
            panic!("'{input}': expected a single-scene document");
        };
        assert_eq!(
            timeline.env().get("label"),
            Some(animatix::timeline::Value::Str(input.to_string())),
            "'{input}' must reach the build environment unchanged"
        );
    }
}

/// A value the engine cannot parse comes back as a build *diagnostic*: the web
/// channel has no exit code to fail with, and a slider feeding garbage must not
/// take the player down (nor panic).
#[test]
fn an_unparseable_var_is_a_diagnostic_not_a_panic() {
    const SOURCE: &str = "config { resolution: (320, 180) }\n#0s\ndot: Ellipse, size: (30, 30)\n";
    let doc = build_document_with_vars(
        SOURCE,
        &[],
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
        None,
        &[
            ("amp".to_string(), "200 +".to_string()),
            ("9bad".to_string(), "1".to_string()),
        ],
    );
    assert!(!doc.result.ok, "a bad override must fail the build");
    assert!(doc.target.is_none(), "a failed build installs no document");
    let messages: Vec<&str> = doc.result.diagnostics.iter().map(|d| d.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("amp") && m.contains("not a valid expression")),
        "the unparseable value is reported: {messages:?}"
    );
    assert!(
        messages
            .iter()
            .any(|m| m.contains("9bad") && m.contains("not a valid identifier")),
        "the invalid name is reported by the engine's one parser: {messages:?}"
    );
}

/// Overrides reach a single scene only. The engine logs that and moves on; the
/// browser installs no `tracing` subscriber, so the host build repeats it as a
/// warning diagnostic the page can actually see — without failing the build.
#[test]
fn vars_on_a_composition_report_they_are_not_applied() {
    const MULTISCENE: &str = include_str!("../../../../examples/composition/14_multiscene.amx");
    let doc = build_document_with_vars(
        MULTISCENE,
        &[],
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
        None,
        &[("amp".to_string(), "320".to_string())],
    );
    assert!(doc.result.ok, "the note must not fail the build: {:?}", doc.result.diagnostics);
    let note = doc
        .result
        .diagnostics
        .iter()
        .find(|d| d.code == "vars-not-applied")
        .expect("a composition must report that overrides did not reach it");
    assert_eq!(note.severity, "warning");

    let doc = build_document(MULTISCENE, Arc::new(FontContext::new()), BuildQuality::Draft);
    assert!(
        !doc.result.diagnostics.iter().any(|d| d.code == "vars-not-applied"),
        "with nothing overridden there is nothing to report: {:?}",
        doc.result.diagnostics
    );
}

/// The boundary's whole rendering rule: JSON scalars become expression text, and
/// the shapes the DSL's raw literals cannot hold are refused — so the caller logs
/// the drop instead of handing the engine text that means something else.
#[test]
fn var_values_render_as_animatix_expression_text() {
    assert_eq!(vars::number_expr(320.0).as_deref(), Some("320"));
    assert_eq!(vars::number_expr(0.5).as_deref(), Some("0.5"));
    assert_eq!(vars::number_expr(-12.25).as_deref(), Some("-12.25"));
    assert_eq!(vars::number_expr(1e21).as_deref(), Some("1000000000000000000000"));
    assert_eq!(vars::number_expr(f64::NAN), None);
    assert_eq!(vars::number_expr(f64::INFINITY), None);

    assert_eq!(vars::string_expr("Q3 Revenue").as_deref(), Some("\"Q3 Revenue\""));
    assert_eq!(vars::string_expr("say \"hi\"").as_deref(), Some("'say \"hi\"'"));
    assert_eq!(vars::string_expr("").as_deref(), Some("\"\""));
    assert_eq!(vars::string_expr("both 'quotes' and \"doubles\""), None);
    assert_eq!(vars::string_expr("back\\slash"), None);
    assert_eq!(vars::string_expr("two\nlines"), None);
    assert_eq!(vars::string_expr("tab\there"), None);
}

#[test]
fn preparation_reuses_syntax_and_recomputes_variable_resources() {
    let source = r#"
let label = "before"
#0.1s
caption: Text, text: label, at: (100, 100)
"#;
    let first = prepare_document(
        source,
        &[],
        &[("label".into(), "\"first\"".into())],
        Some("https://example.test/scenes/demo.amx"),
    );
    let second = prepare_document(
        source,
        &[],
        &[("label".into(), "\"second\"".into())],
        Some("https://example.test/scenes/demo.amx"),
    );
    assert!(first.analysis.ok, "{:?}", first.analysis.diagnostics);
    assert!(second.analysis.ok, "{:?}", second.analysis.diagnostics);
    assert!(Arc::ptr_eq(&first.syntax, &second.syntax));
    assert!(first.analysis.fonts.text.contains("first"));
    assert!(second.analysis.fonts.text.contains("second"));
}

#[test]
fn preparation_discovers_imports_and_resolves_relative_assets() {
    let source = "import \"./component.amx\" as part\n#0.1s\npic: Image, url: \"./shared.png\", size: (10, 10)\n";
    let url = Some("https://example.test/scenes/demo.amx");
    let missing = prepare_document(source, &[], &[], url);
    assert!(!missing.analysis.ok);
    assert_eq!(missing.analysis.missing_imports, ["component.amx"]);
    let modules = [(PathBuf::from("component.amx"), "export component Empty { }".to_owned())];
    let prepared = prepare_document(source, &modules, &[], url);
    assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
    assert!(prepared.analysis.assets.iter().any(|asset| asset.url
        == "https://example.test/scenes/shared.png"
        && asset.kind == "image"));
}

#[test]
fn preparation_expands_exported_components_in_imported_scenes_for_resources() {
    let source = r#"
import "./scene.amx" as demo
# Main
#0.1s
play demo.Intro
"#;
    let module = r#"
pub component Card(title: Str) {
  image: Image, url: "./card.png", size: (10, 10)
  heading: Text, text: title, font_family: "Open Sans"
}

# Intro
#0.1s
card: Card, title: "Imported title"
"#;
    let url = Some("https://example.test/scenes/main.amx");
    let modules = [(PathBuf::from("scene.amx"), module.to_owned())];
    let prepared = prepare_document(source, &modules, &[], url);

    assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
    assert!(
        prepared.analysis.assets.iter().any(|asset| {
            asset.url == "https://example.test/scenes/card.png" && asset.kind == "image"
        }),
        "assets: {:?}",
        prepared.analysis.assets
    );
    assert!(
        prepared.analysis.fonts.text.contains("Imported title"),
        "fonts: {:?}",
        prepared.analysis.fonts
    );
    assert!(
        prepared.analysis.fonts.families.iter().any(|family| family == "Open Sans"),
        "fonts: {:?}",
        prepared.analysis.fonts
    );
}

#[test]
fn font_requirements_mark_rich_markup_for_emphasis_and_math_faces() {
    let plain = prepare_document("#0.1s\nlabel: Text, text: \"plain\"\n", &[], &[], None);
    assert!(!plain.analysis.fonts.rich_text);

    for actor in ["Typst", "Math", "Code"] {
        let source = format!("#0.1s\nlabel: {actor}, text: \"markup\"\n");
        let prepared = prepare_document(&source, &[], &[], None);
        assert!(prepared.analysis.fonts.rich_text, "{actor}: {:?}", prepared.analysis.fonts);
    }
}

#[test]
fn font_requirements_mark_generated_typst_text_dynamic_but_literal_text_static() {
    for actor in ["Typst", "Math"] {
        let source = format!("#0.1s\nlabel: {actor}, text: \"literal source\"\n");
        let prepared = prepare_document(&source, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert!(prepared.analysis.fonts.dynamic, "{actor}: {:?}", prepared.analysis.fonts);
    }

    for actor in ["Text", "Code"] {
        let source = format!("#0.1s\nlabel: {actor}, text: \"literal source\"\n");
        let prepared = prepare_document(&source, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert!(!prepared.analysis.fonts.dynamic, "{actor}: {:?}", prepared.analysis.fonts);
    }
}

#[test]
fn preparation_collects_nonstandard_rendered_text_declarations_and_assignments() {
    let source = r#"
#0.1s
counter: Counter, prefix: "第", suffix: "页"
note: Callout, label: "提示"
legend: Legend, title: "图例"
series: Line, from: (0, 0), to: (1, 1), legend: "收入"
series_q1: Line, from: (1, 1), to: (2, 2)
plot: Graph, tick_labels: "both"
counter.prefix = "计"
note.label = "说明"
legend.title = "指标"
series.legend = "利润"
"#;
    let prepared = prepare_document(source, &[], &[], None);

    assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
    let text = &prepared.analysis.fonts.text;
    for expected in [
        "第",
        "页",
        "提示",
        "图例",
        "收入",
        "计",
        "说明",
        "指标",
        "利润",
        "Series Q1",
    ] {
        assert!(text.contains(expected), "missing {expected:?} in {text:?}");
    }
    assert!(!text.contains("both"), "tick-label mode is not rendered text: {text:?}");
    assert!(!prepared.analysis.fonts.dynamic, "fonts: {:?}", prepared.analysis.fonts);
}

#[test]
fn preparation_does_not_fetch_unrendered_legend_labels_or_modes() {
    let source = r#"
#0.1s
shape: Rect, size: (10, 10), legend: "未展示"
secret_label: Rect, size: (10, 10)
hidden: Rect, size: (10, 10), legend: false
plot: Graph, tick_labels: "auto"
"#;
    let prepared = prepare_document(source, &[], &[], None);

    assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
    assert!(!prepared.analysis.fonts.text.contains("未展示"));
    assert!(!prepared.analysis.fonts.text.contains("Secret Label"));
    assert!(!prepared.analysis.fonts.text.contains("auto"));
    assert!(!prepared.analysis.fonts.dynamic, "fonts: {:?}", prepared.analysis.fonts);
}

#[test]
fn preparation_ignores_unplayed_imported_scenes_with_dynamic_assets() {
    let source = r#"
import "./scene.amx" as demo
# Main
#0.1s
caption: Text, text: "No imported scene is played"
"#;
    let module = r#"
# Unused
#0.1s
pic: Image, url: unresolved_asset, size: (10, 10)
"#;
    let prepared = prepare_document(
        source,
        &[(PathBuf::from("scene.amx"), module.to_owned())],
        &[],
        Some("https://example.test/scenes/main.amx"),
    );

    assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
    assert!(prepared.analysis.assets.is_empty(), "{:?}", prepared.analysis.assets);
    assert!(
        !prepared
            .analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "dynamic-asset"),
        "unplayed scene diagnostics: {:?}",
        prepared.analysis.diagnostics
    );
}

#[test]
fn preparation_follows_nested_play_targets_and_collects_reachable_assets() {
    let source = r#"
import "./scene.amx" as demo
# Main
#0.1s
play demo.Intro
"#;
    let module = r#"
import "./inner.amx" as inner
# Intro
#0.1s
play demo.inner.Detail
"#;
    let nested = r#"
# Detail
#0.1s
pic: Image, url: "./used.png", size: (10, 10)

# Unused
#0.1s
bad: Image, url: unresolved_asset, size: (10, 10)
"#;
    let modules = [
        (PathBuf::from("scene.amx"), module.to_owned()),
        (PathBuf::from("inner.amx"), nested.to_owned()),
    ];
    let prepared =
        prepare_document(source, &modules, &[], Some("https://example.test/scenes/main.amx"));

    assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
    assert!(
        prepared.analysis.assets.iter().any(|asset| {
            asset.url == "https://example.test/scenes/used.png" && asset.kind == "image"
        }),
        "assets: {:?}",
        prepared.analysis.assets
    );
    assert!(
        !prepared
            .analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == "dynamic-asset"),
        "unplayed nested scene diagnostics: {:?}",
        prepared.analysis.diagnostics
    );
}

#[test]
fn preparation_rejects_dynamic_asset_and_unknown_actor() {
    let url = Some("https://example.test/scenes/demo.amx");
    let dynamic =
        prepare_document("#0.1s\npic: Image, url: unknown_source, size: (10, 10)\n", &[], &[], url);
    assert!(!dynamic.analysis.ok);
    assert!(
        dynamic.analysis.diagnostics.iter().any(|d| d.code == "dynamic-asset"),
        "{:?}",
        dynamic.analysis.diagnostics
    );
    let unknown = prepare_document("#0.1s\nx: ImaginaryActor, at: (10, 10)\n", &[], &[], url);
    assert!(!unknown.analysis.ok);
    assert!(
        unknown.analysis.diagnostics.iter().any(|d| d.code == "unknown-actor-type"),
        "{:?}",
        unknown.analysis.diagnostics
    );
}

#[test]
fn preparation_resolves_font_weight_names_before_selecting_faces() {
    for (name, weight) in [
        ("thin", 100.0),
        ("medium", 500.0),
        ("semibold", 600.0),
        ("extrabold", 800.0),
    ] {
        let source = format!("#0.1s\nlabel: Text, text: \"Hello\", font_weight: \"{name}\"\n");
        let prepared = prepare_document(&source, &[], &[], None);
        assert!(prepared.analysis.ok, "{:?}", prepared.analysis.diagnostics);
        assert!(
            prepared.analysis.fonts.weights.contains(&weight),
            "{name}: {:?}",
            prepared.analysis.fonts.weights
        );
    }
}
