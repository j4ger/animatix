use std::collections::HashMap;

use animatix_syntax::module::ModuleGraph;
use animatix_syntax::parser::parse_source;

use super::*;
use crate::timeline::camera::CAMERA_TARGET;

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

/// Parse `source` and build through the canonical composition path.
fn build_composition(source: &str) -> BuildReport<Composition> {
    let parsed = parse_source(source).0.expect("source parses");
    Composition::build_test(&parsed, &HashMap::new())
}

/// [`BuildTarget`] dispatch, for tests that care which mode was produced.
fn build_target(source: &str) -> BuildReport<BuildTarget> {
    let parsed = parse_source(source).0.expect("source parses");
    BuildTarget::build_test(&parsed, &HashMap::new())
}

fn assert_no_errors<T>(report: &BuildReport<T>) {
    let errors: Vec<_> = report.diagnostics.iter().filter(|d| d.is_error()).collect();
    assert!(errors.is_empty(), "errors: {errors:?}");
}

fn has_code<T>(report: &BuildReport<T>, code: DiagnosticCode) -> bool {
    report.diagnostics.iter().any(|d| d.code == code)
}

fn scene<'a>(comp: &'a Composition, name: &str) -> &'a CompositionScene {
    comp.scenes.get(name).unwrap_or_else(|| panic!("scene `{name}`"))
}

fn assert_track(scene: &CompositionScene, label: &str) {
    assert!(
        scene.timeline.world.tracks.contains_key(label),
        "{}: `{label}` track must exist",
        scene.name
    );
}

fn assert_root(scene: &CompositionScene, label: &str) {
    let id = scene
        .timeline
        .get_entity_id(label)
        .unwrap_or_else(|| panic!("`{label}` entity"));
    assert!(scene.timeline.world.is_root(id), "{}: `{label}` must be re-rooted", scene.name);
}

fn assert_actor_type(scene: &CompositionScene, label: &str, ty: &str) {
    assert_track(scene, label);
    let id = scene.timeline.world.tracks.get_id(label).unwrap();
    assert_eq!(
        scene.timeline.world.actor_type(id),
        ty,
        "{}: carried `{label}` must stay {ty}",
        scene.name
    );
}

/// Staging helper for a two-file composition: `scenes.amx` is imported, and
/// `main.amx` plays `play_line`. Returns the temp dir for cleanup.
fn build_cross_file(
    tag: &str,
    scenes: &str,
    play_line: &str,
) -> (std::path::PathBuf, BuildReport<Composition>) {
    let dir = std::env::temp_dir().join(format!("animatix_test_{tag}_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let scenes_file = dir.join("scenes.amx");
    let main_file = dir.join("main.amx");
    std::fs::write(&scenes_file, scenes).unwrap();
    std::fs::write(
        &main_file,
        format!(
            "import \"{}\" as scenes\n\n# Intro\n#0s\ntitle: Text, text: \"Welcome\"\n{play_line}\n",
            scenes_file.display()
        ),
    )
    .unwrap();
    let mut graph = ModuleGraph::new();
    let program = graph.load_program(&main_file).unwrap();
    (dir, Composition::build_test(&program.statements, &program.namespaces))
}

/// The 500 ms window before `intro_dur` is where a `play` transition overlaps
/// backwards into the outgoing scene.
fn blend_in_window(comp: &Composition, intro_dur: f64) -> TransitionBlend {
    (0..40)
        .find_map(|step| {
            let t = intro_dur - 0.45 + f64::from(step) * 0.01;
            comp.evaluate(t).2.filter(|b| b.progress > 0.0 && b.progress < 1.0)
        })
        .expect("a 500ms fade must open a blend window")
}

// ---------------------------------------------------------------------------
// Scene graph
// ---------------------------------------------------------------------------

#[test]
fn test_no_scenes_returns_empty_composition() {
    let report = build_composition("#0s\ntitle: Text, text: \"Hello\"\n");
    assert!(!report.output.has_scenes());
    assert_eq!(report.output.global_duration_s, 0.0);
}

#[test]
fn test_two_scenes_implicit_order() {
    let report = build_composition(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "#1s\n",
        "title.opacity = 1\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Rect, size: (400, 400)\n",
        "#2s\n",
        "graph.opacity = 1\n",
    ));
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
    let comp = &report.output;
    assert!(comp.has_scenes());
    assert_eq!(comp.scenes.len(), 2);
    assert_eq!(comp.declaration_order, vec!["Intro", "Diagram"]);
    assert!(comp.global_duration_s > 0.0);
}

#[test]
fn test_scene_with_config() {
    let report = build_composition(concat!(
        "# Intro\n",
        "config { colorscheme: \"default-dark\" }\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
    ));
    assert!(report.diagnostics.is_empty());
    let intro = scene(&report.output, "Intro");
    assert_eq!(intro.config.len(), 1);
    assert_eq!(intro.config[0].name, "colorscheme");
}

#[test]
fn test_scene_with_play() {
    let report = build_composition(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "#1s\n",
        "title.opacity = 1\n",
        "play Diagram [fade, 300ms]\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Rect, size: (400, 400)\n",
        "#2s\n",
        "graph.opacity = 1\n",
    ));
    let comp = &report.output;
    let edge = comp.edges.get("Intro").expect("Intro edge");
    assert_eq!(edge.to_scene, "Diagram");
    assert_eq!(edge.transition.id, "fade");
    assert_eq!(edge.transition.duration_ms, 300);

    // With a 300ms transition, Diagram should overlap by 0.3s.
    assert!(comp.scene_start_times["Intro"] < comp.scene_start_times["Diagram"]);
}

/// Builds that need only "this diagnostic fired" coverage. Each case keeps its
/// own source, so a failing input is still named in the assertion context.
#[test]
fn composition_reports_build_diagnostics() {
    let cases: &[(&str, &str, DiagnosticCode)] = &[
        (
            "play target missing",
            "# Intro\n#0s\ntitle: Text, text: \"Welcome\"\nplay MissingScene\n",
            DiagnosticCode::PlayTargetNotFound,
        ),
        (
            "qualified play target needs a namespace alias",
            "# Intro\n#0s\ntitle: Text, text: \"Welcome\"\nplay module.SceneName\n",
            DiagnosticCode::PlayTargetNotFound,
        ),
        (
            "duplicate scene name",
            "# Intro\n#0s\ntitle: Text, text: \"Welcome\"\n# Intro\n#0s\ntitle: Text, text: \"Again\"\n",
            DiagnosticCode::DuplicateSceneName,
        ),
        (
            "persist in a single-scene composition",
            "# OnlyScene\n#0s\nbadge: Ellipse, at: (50, 50), size: (30, 30)\n#1s\npersist badge\n",
            DiagnosticCode::PersistTargetNotCarried,
        ),
        (
            "persist in the last scene",
            "# SceneA\n#0s\ntitle: Text, text: \"Hello\", at: (100, 100)\n#1s\npersist title\n\
             play SceneB\n\n# SceneB\n#0s\n#1s\n",
            DiagnosticCode::PersistTargetNotCarried,
        ),
        (
            "persist after remove",
            "# SceneA\n#0s\nbadge: Ellipse, at: (50, 50), size: (30, 30)\n#1s\nremove badge\n\
             #2s\npersist badge\n\n# SceneB\n#0s\n",
            DiagnosticCode::PersistAfterRemove,
        ),
    ];
    for (label, source, code) in cases {
        let report = build_composition(source);
        assert!(
            has_code(&report, *code),
            "{label}: expected {code:?}, got {:?}",
            report.diagnostics
        );
    }
}

#[test]
fn test_qualified_play_target_uses_namespace_alias() {
    let parsed = parse_source(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "play module.SceneName [fade, 300ms]\n",
    ))
    .0
    .unwrap();
    let mut namespaces = HashMap::new();
    namespaces.insert("module".to_string(), Namespace::default());

    let report = Composition::build_test(&parsed, &namespaces);
    assert_eq!(report.output.edges.get("Intro").unwrap().to_scene, "module.SceneName");
    assert!(
        !has_code(&report, DiagnosticCode::PlayTargetNotFound),
        "a namespaced alias must resolve the play target"
    );
}

#[test]
fn test_global_time_mapping() {
    let report = build_composition(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "#1s\n",
        "title.opacity = 1\n",
        "play Diagram [fade, 300ms]\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Text, text: \"Graph\"\n",
        "#2s\n",
        "graph.opacity = 1\n",
    ));
    let comp = &report.output;

    // At t=0: should be in Intro at local time 0.
    let (name, local, blend) = comp.evaluate(0.0);
    assert_eq!(name, "Intro");
    assert_eq!(local, 0.0);
    assert!(blend.is_none());

    // After Intro's duration: should be in Diagram.
    let intro_dur = scene(comp, "Intro").duration_s;
    assert_eq!(comp.evaluate(intro_dur + 0.1).0, "Diagram");
}

#[test]
fn test_local_time_for_scene() {
    let comp = build_composition(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "#1s\n",
        "title.opacity = 1\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Text, text: \"Graph\"\n",
    ))
    .output;

    assert_eq!(comp.local_time_for_scene("Intro", 0.5), Some(0.5));

    // Global time beyond Intro should return None for Intro.
    let intro_dur = scene(&comp, "Intro").duration_s;
    assert_eq!(comp.local_time_for_scene("Intro", intro_dur + 0.1), None);
}

#[test]
fn test_single_scene_no_scenes_parsed() {
    // Single-scene file — no # SceneName declarations.
    let (parsed, _) = parse_source(concat!(
        "config { resolution: (1280, 720) }\n",
        "#0s\n",
        "title: Text, text: \"Hello\"\n",
        "#1s\n",
        "fade-in title [500ms]\n",
    ));
    let parsed = parsed.unwrap();
    let has_scenes = parsed.iter().any(|s| matches!(s, Stmt::Scene { .. }));
    assert!(!has_scenes, "Single-scene file should not produce Stmt::Scene");
}

#[test]
fn test_multiple_play_targets_error() {
    let report = build_composition(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "play Diagram\n",
        "play Outro\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Rect, size: (400, 400)\n",
        "\n",
        "# Outro\n",
        "#0s\n",
        "bye: Text, text: \"Bye\"\n",
    ));
    assert!(
        has_code(&report, DiagnosticCode::MultiplePlayTargets),
        "Expected MultiplePlayTargets error"
    );
    // The first play target should still be used.
    assert_eq!(report.output.edges.get("Intro").unwrap().to_scene, "Diagram");
}

#[test]
fn test_orphan_scene_warning() {
    let report = build_composition(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "play Diagram\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Rect, size: (400, 400)\n",
        "\n",
        "# Orphan\n",
        "#0s\n",
        "solo: Text, text: \"Alone\"\n",
    ));
    assert!(has_code(&report, DiagnosticCode::OrphanScene), "Expected OrphanScene warning");
    // An orphan is still appended to the walk order.
    assert!(report.output.scenes.contains_key("Orphan"));
}

#[test]
fn an_authored_transition_ease_reaches_the_blend() {
    let comp = build_composition(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "#1s\n",
        "play Diagram [fade, 500ms, ease: ease-in]\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Text, text: \"Graph\"\n",
    ))
    .output;
    let blend = blend_in_window(&comp, scene(&comp, "Intro").duration_s);

    assert_eq!(
        blend.easing,
        crate::easing::Easing::EaseIn,
        "the authored `ease:` never reached the blend — the parser dropped it"
    );
    // ease-in is t^2, so the composited progress must sit behind the raw one.
    assert!(
        (blend.eased_progress - blend.progress * blend.progress).abs() < 1e-3,
        "eased progress {} is not ease-in of {}",
        blend.eased_progress,
        blend.progress
    );
    assert!(
        blend.eased_progress < blend.progress,
        "ease-in must lag the raw ramp: {} vs {}",
        blend.eased_progress,
        blend.progress
    );
}

#[test]
fn a_transition_without_an_eased_authored_stays_linear() {
    let comp = build_composition(concat!(
        "# Intro\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "#1s\n",
        "play Diagram [fade, 500ms]\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Text, text: \"Graph\"\n",
    ))
    .output;
    let blend = blend_in_window(&comp, scene(&comp, "Intro").duration_s);

    assert_eq!(blend.easing, crate::easing::Easing::Linear);
    assert!(
        (blend.eased_progress - blend.progress).abs() < 1e-6,
        "linear must not bend progress: {} vs {}",
        blend.eased_progress,
        blend.progress
    );
}

#[test]
fn test_scene_config_resolution_warning() {
    let report = build_composition(concat!(
        "config { resolution: (1280, 720) }\n",
        "\n",
        "# Intro\n",
        "config { resolution: (1920, 1080) }\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
        "\n",
        "# Diagram\n",
        "#0s\n",
        "graph: Rect, size: (400, 400)\n",
    ));
    let has_resolution_warning = report.diagnostics.iter().any(|d| {
        d.code == DiagnosticCode::InvalidConfigValue
            && d.message.contains("resolution")
            && d.message.contains("composition-scoped")
    });
    assert!(has_resolution_warning, "Expected resolution scene-config warning");
    // The prelude resolution is used, not the scene one; the build still succeeds.
    assert!(report.output.has_scenes());
}

#[test]
fn test_scene_config_colorscheme_no_warning() {
    let report = build_composition(concat!(
        "# Intro\n",
        "config { colorscheme: \"editorial-dark\" }\n",
        "#0s\n",
        "title: Text, text: \"Welcome\"\n",
    ));
    // colorscheme is scene-scoped — no warning expected.
    let has_config_warning = report.diagnostics.iter().any(|d| {
        d.code == DiagnosticCode::InvalidConfigValue && d.message.contains("composition-scoped")
    });
    assert!(!has_config_warning, "colorscheme should not trigger composition-scoped warning");
}

#[test]
fn test_cross_file_scene_basic() {
    let (dir, report) = build_cross_file(
        "basic",
        "# FadeIn\n#0s\nlabel: Text, text: \"Fade In Scene\"\nfade-in label [500ms]\n",
        "play scenes.FadeIn [fade, 300ms]",
    );
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);
    let comp = &report.output;

    // Both the local "Intro" and cross-file "scenes.FadeIn" must exist.
    assert!(comp.scenes.contains_key("Intro"));
    assert!(
        comp.scenes.contains_key("scenes.FadeIn"),
        "Cross-file scene 'scenes.FadeIn' not found. Scenes: {:?}",
        comp.scenes.keys().collect::<Vec<_>>()
    );

    let edge = comp.edges.get("Intro").unwrap();
    assert_eq!(edge.to_scene, "scenes.FadeIn");
    assert_eq!(edge.transition.id, "fade");
    assert_eq!(edge.transition.duration_ms, 300);

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_file_scene_not_found() {
    // "scenes.NonExistent" names a scene that does not exist under that
    // namespace, so it must produce PlayTargetNotFound.
    let (dir, report) = build_cross_file(
        "notfound",
        "# ExistingScene\n#0s\nlabel: Text, text: \"Hi\"\n",
        "play scenes.NonExistent",
    );
    assert!(
        has_code(&report, DiagnosticCode::PlayTargetNotFound),
        "Expected PlayTargetNotFound diagnostic. Diagnostics: {:?}",
        report.diagnostics
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn test_cross_file_scene_duration_preserved() {
    let (dir, report) = build_cross_file(
        "duration",
        "# TimedScene\nconfig { duration: 5.0 }\n#0s\nlabel: Text, text: \"Timed\"\n",
        "play scenes.TimedScene",
    );
    assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);

    let timed_scene = scene(&report.output, "scenes.TimedScene");
    assert_eq!(timed_scene.explicit_duration_s, Some(5.0));
    assert_eq!(timed_scene.duration_s, 5.0);

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// Scene persistence carry tests
// ---------------------------------------------------------------------------

/// Basic carry: `title` persisted from SceneA → SceneB, which gets it both as
/// a track and as a root entity.
#[test]
fn test_persist_basic_carry() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "title: Text, text: \"Hello\", at: (100, 100)\n",
        "#1s\n",
        "persist title\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
    ));
    assert_no_errors(&report);

    let scene_a = scene(&report.output, "SceneA");
    assert!(
        scene_a.timeline.persistence_flag("title"),
        "SceneA persistence flag for title must be true"
    );

    let scene_b = scene(&report.output, "SceneB");
    assert_track(scene_b, "title");
    assert_root(scene_b, "title");
}

/// Carry with assignment: SceneB modifies the carried actor's property.
#[test]
fn test_persist_carry_then_assign() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "title: Text, text: \"Hello\", at: (100, 100)\n",
        "#1s\n",
        "persist title\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
        "title.opacity = 1\n",
    ));
    assert_no_errors(&report);
    assert_track(scene(&report.output, "SceneB"), "title");
    // Assignment on a carried actor must succeed.
    assert!(
        !has_code(&report, DiagnosticCode::UnsupportedActionTarget),
        "carry assignment must not emit UnsupportedActionTarget"
    );
}

/// Chain persistence: badge carried A → B → C without re-persist in B or C.
#[test]
fn test_persist_chain() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "badge: Ellipse, at: (50, 50), size: (30, 30)\n",
        "#1s\n",
        "persist badge\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
        "\n",
        "# SceneC\n",
        "#0s\n",
    ));
    assert_no_errors(&report);

    let scene_b = scene(&report.output, "SceneB");
    assert_track(scene_b, "badge");
    assert!(
        scene_b.timeline.persistence_flag("badge"),
        "chain: SceneB persistence flag for badge must remain true"
    );
    assert_track(scene(&report.output, "SceneC"), "badge");
}

/// Remove breaks the chain: badge removed in B must not appear in C.
#[test]
fn test_remove_breaks_chain() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "badge: Ellipse, at: (50, 50), size: (30, 30)\n",
        "#1s\n",
        "persist badge\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
        "remove badge\n",
        "\n",
        "# SceneC\n",
        "#0s\n",
    ));
    assert_no_errors(&report);

    // badge must be present in SceneB (it was carried, then removed).
    let scene_b = scene(&report.output, "SceneB");
    assert_track(scene_b, "badge");
    assert_eq!(
        scene_b
            .timeline
            .entity("badge")
            .and_then(|entity| entity.persistence())
            .map(|component| component.persistent),
        Some(false),
        "SceneB persistence flag for badge must be false after remove"
    );

    // badge must NOT be in SceneC (chain broken by remove in B).
    assert!(
        !scene(&report.output, "SceneC").timeline.world.tracks.contains_key("badge"),
        "SceneC must NOT have badge (chain broken by remove in SceneB)"
    );
}

/// Container + subtree carry: persisting `row` carries the container and its
/// `child`. The child stays a child of `row` (not re-rooted), and `row`'s
/// container metadata is seeded.
#[test]
fn test_persist_container_subtree() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "row: Row, anchor: scene.center {\n",
        "    child: Text, text: \"Hi\"\n",
        "}\n",
        "#1s\n",
        "persist row\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
    ));
    assert_no_errors(&report);

    let scene_b = scene(&report.output, "SceneB");
    assert_track(scene_b, "row");
    assert_root(scene_b, "row");

    // child is carried as a subtree member, not re-rooted.
    assert_track(scene_b, "child");
    let row_id = scene_b.timeline.get_entity_id("row").expect("row entity");
    let child_id = scene_b.timeline.get_entity_id("child").expect("child entity");
    assert_eq!(
        scene_b.timeline.world.hierarchy(child_id).parent,
        Some(row_id),
        "child must stay under row (not re-rooted)"
    );

    // Container layout must be carried so layout still resolves in B.
    assert!(
        scene_b.timeline.container_layout("row").is_some(),
        "SceneB: container_layout must carry `row`"
    );
}

/// Persist in a truly single-scene file (no `# SceneName` markers) must emit
/// `PersistTargetNotCarried` because there is no successor scene.
#[test]
fn test_persist_in_single_scene_file_warns() {
    let report =
        build_target("#0s\ntitle: Text, text: \"Hello\", at: (320, 180)\n#1s\npersist title\n");
    assert!(
        has_code(&report, DiagnosticCode::PersistTargetNotCarried),
        "persist in a single-scene file should emit PersistTargetNotCarried: {:?}",
        report.diagnostics
    );
}

/// Auto-color slot must be preserved when an actor is carried across scenes.
#[test]
fn test_auto_color_slot_preserved_across_carry() {
    let report = build_composition(concat!(
        "config { colorscheme: \"default-dark\" }\n",
        "\n",
        "# SceneA\n",
        "#0s\n",
        "circle: Ellipse, at: (200, 200), size: (60, 60), color: auto\n",
        "#1s\n",
        "persist circle\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
    ));
    assert_no_errors(&report);

    let comp = &report.output;
    let slot_a = scene(comp, "SceneA")
        .timeline
        .auto_color_slot("circle")
        .expect("SceneA: circle should have an auto_color slot");
    let scene_b = scene(comp, "SceneB");
    assert_track(scene_b, "circle");
    let slot_b = scene_b
        .timeline
        .auto_color_slot("circle")
        .expect("SceneB: circle auto_color slot must be carried");
    assert_eq!(slot_a, slot_b, "auto_color slot must be the same in both scenes");
}

/// `PlotCurve` carries its identity and `procedural_plot` to the next scene.
#[test]
fn test_plot_curve_carry() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "g: Graph, at: (640, 360), size: (400, 300) {\n",
        "    curve: PlotCurve, kind: \"cartesian\", func: (x) => x * x\n",
        "}\n",
        "#1s\n",
        "persist g\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
    ));
    assert_no_errors(&report);

    let scene_b = scene(&report.output, "SceneB");
    assert_actor_type(scene_b, "curve", "PlotCurve");
    let curve_track = scene_b.timeline.world.tracks.get_id("curve").unwrap();
    assert!(
        scene_b.timeline.world.procedural_plot(curve_track).is_some(),
        "carried PlotCurve track must retain procedural_plot"
    );
}

/// `Svg` carries its identity to the next scene.
#[test]
fn test_svg_actor_carry() {
    // Inline path: no file loading needed for the kind-carry assertion.
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "icon: Svg, at: (200, 200), size: (100, 100), src: \"M10 10 L20 20\"\n",
        "#1s\n",
        "persist icon\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
    ));
    assert_actor_type(scene(&report.output, "SceneB"), "icon", "Svg");
}

/// `Image` carries its identity to the next scene.
#[test]
fn test_image_actor_carry() {
    // File-loading diagnostics are expected; only the carry matters here.
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "pic: Image, at: (300, 200), size: (200, 150), src: \"placeholder.png\"\n",
        "#1s\n",
        "persist pic\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
    ));
    assert_actor_type(scene(&report.output, "SceneB"), "pic", "Image");
}

#[test]
fn sorting_visualizer_steps_scene_sorts() {
    // Guard the demo: insertion sort must leave the bars ascending at the end
    // of the Steps scene (the original version mis-targeted later swaps).
    let workspace = std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir");
    let demo = std::path::Path::new(&workspace)
        .join("../../dogfood/projects/sorting-visualizer/entry.amx");
    if !demo.exists() {
        eprintln!("skip: {demo:?} missing");
        return;
    }
    let mut graph = ModuleGraph::new();
    let program = graph.load_program(&demo).expect("loads");
    let expanded = program.expand_components(&mut Vec::new());
    let report = Composition::build_test(&expanded, &program.namespaces);
    assert!(
        report.diagnostics.is_empty(),
        "demo must build cleanly: {:?}",
        report.diagnostics
    );
    let comp = report.output;
    let steps = &comp.scenes["Steps"].timeline;
    let track = &steps.child_order("bars").expect("bars child orders").track;
    let last = *track.keyframes.keys().max().expect("last keyframe");
    let final_order = steps.get_child_order("bars", last);
    // bar3 has value 2 and bar2 has value 3, so this order is [1,2,3,4].
    assert_eq!(
        final_order,
        vec!["bars.bar__1", "bars.bar__3", "bars.bar__2", "bars.bar__0"],
        "insertion sort must leave values 1,2,3,4 left to right"
    );
    // Scenes must run long enough for their plays to fire.
    assert!(comp.scenes["Steps"].duration_s >= 4.0, "Steps scene too short");
    assert!(comp.scenes["Title"].duration_s >= 2.0, "Title scene too short");
}

#[test]
fn test_zero_duration_scene_does_not_collapse_composition() {
    // Regression: a scene with only actor declarations (zero inferred timeline
    // duration) used to receive duration 0. As the target of a play transition
    // that collapsed the global duration to the play time, clamping playback
    // and cutting off actions in the outgoing scene before they finished.
    let report = build_composition(concat!(
        "# Scene1\n",
        "#0s\n",
        "a: Rect, width: 100, height: 100\n",
        "#0.2s\n",
        "fade-in a [400ms]\n",
        "play Scene2 [fade, 400ms]\n",
        "\n",
        "# Scene2\n",
        "#0s\n",
        "b: Rect, width: 100, height: 100\n",
    ));
    assert!(
        report.diagnostics.is_empty(),
        "unexpected diagnostics: {:?}",
        report.diagnostics
    );
    let comp = &report.output;

    // Scene2 must be long enough for the 400ms incoming transition.
    let scene2 = scene(comp, "Scene2");
    assert!(
        scene2.duration_s >= 0.4,
        "Scene2 duration {} must cover the 400ms transition",
        scene2.duration_s
    );

    // Global time 0.5s must fall inside the composition, not be clamped.
    assert!(
        comp.global_duration_s >= 0.5,
        "global duration {} too short; composition clamps early",
        comp.global_duration_s
    );

    // At global 0.5s we should be in the transition, with Scene1 still running
    // at local time >= 0.5s so the fade-in can complete.
    let (active, local, blend) = comp.evaluate(0.5);
    assert_eq!(active, "Scene1");
    assert!(local >= 0.5, "Scene1 local time {} should not be clamped to play time", local);
    assert!(blend.is_some(), "expected transition blend at t=0.5s");
}

/// `Composition::summary()` reports scenes in playback order with their start
/// times, durations, and the play edges between them.
#[test]
fn test_summary_reports_playback_order_and_edges() {
    let report = build_target(concat!(
        "# A\n",
        "a: Text, text: \"A\", at: (100, 100)\n",
        "#2s\n",
        "play B [fade, 400ms]\n",
        "# B\n",
        "b: Text, text: \"B\", at: (200, 100)\n",
    ));
    let BuildTarget::MultiScene(comp) = report.output else {
        panic!("expected MultiScene");
    };
    let summary = comp.summary();

    let names: Vec<&str> = summary.scenes.iter().map(|(n, ..)| n.as_str()).collect();
    assert_eq!(names, vec!["A", "B"], "playback order must follow start times");

    let (a_name, a_start, a_dur, _) = &summary.scenes[0];
    assert_eq!(a_name, "A");
    assert!((a_start - 0.0).abs() < 1e-9);
    assert!(*a_dur >= 2.0, "scene A must cover its #2s play marker");

    assert_eq!(summary.edges.len(), 1);
    let (from, to, id, ms) = &summary.edges[0];
    assert_eq!((from.as_str(), to.as_str(), id.as_str(), *ms), ("A", "B", "fade", 400));

    assert!(summary.total_duration_s >= 2.0);
}

/// A scene's authored `duration` is the player's clock, so losing it is not
/// cosmetic: `animatix-web` wraps playback time at the scene duration, and a
/// scene whose last keyframe lands earlier than its declared length restarts
/// early. An `import` written in front of the `config` block used to demote that
/// block into the scene body, leaving `Scene.config` empty and the duration to
/// be inferred from keyframe span — which is why this assertion sits in the
/// composition layer instead of stopping at the parser.
#[test]
fn a_scene_config_written_after_an_import_still_sets_the_scene_duration() {
    let report = build_composition(concat!(
        "# Intro\n",
        "import \"./lib/tokens.amx\" as tk\n",
        "config { resolution: (400, 300), duration: 4 }\n",
        "title: Text, text: \"Welcome\"\n",
        "#0s\n",
        "title.opacity = 0\n",
        "#1s\n",
        "title.opacity = 1\n",
    ));
    assert_eq!(
        scene(&report.output, "Intro").duration_s,
        4.0,
        "the scene ran on inferred keyframe span rather than its authored duration; \
         diagnostics: {:?}",
        report.diagnostics.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

/// `persist camera` hands the scene camera's axes to the next scene, so a
/// push-in that ends at a scene boundary does not snap back to identity.
#[test]
fn persist_camera_carries_the_transform() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "a: Rect, size: (100, 100), at: (200, 200), color: (1, 0, 0, 1)\n",
        "#0.5s\n",
        "camera.zoom = 1.6\n",
        "camera.at = (40, -20) [500ms]\n",
        "persist camera\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
        "b: Rect, size: (100, 100), at: (200, 200), color: (0, 1, 0, 1)\n",
    ));
    assert_no_errors(&report);

    let scene_a = scene(&report.output, "SceneA");
    assert!(
        scene_a.timeline.persistence_flag(CAMERA_TARGET),
        "SceneA must record the camera as persistent"
    );
    // Sample the source where it actually ends: the pan is written *at* 500ms
    // with a 500ms tween, so 500ms is still identity and the boundary the bag
    // snapshots is the scene's own end.
    let boundary_ms = (scene_a.timeline.duration_seconds() * 1000.0) as u64;
    let (pan_a, zoom_a, _) = scene_a.timeline.camera.values_at(boundary_ms);
    assert_eq!(zoom_a, 1.6, "source zoom at its boundary");
    assert_eq!(pan_a, [40.0, -20.0], "source pan at its boundary");
    let bag = scene_a.timeline.compute_carry_bag(boundary_ms, true);
    assert!(bag.camera.is_some(), "the bag must carry the camera");

    let scene_b = scene(&report.output, "SceneB");
    assert!(
        scene_b.timeline.camera.is_authored(),
        "the carried camera must count as authored in the receiving scene"
    );
    let (pan, zoom, spin) = scene_b.timeline.camera.values_at(0);
    assert_eq!(zoom, zoom_a, "zoom carries");
    assert_eq!(pan, pan_a, "the pan carries as the source scene left it");
    assert_eq!(spin, 0.0, "an untouched axis stays identity");
}

/// Without `persist camera` the receiving scene starts at identity, which is
/// what every existing composition relies on.
#[test]
fn a_scene_without_persist_camera_starts_at_identity() {
    let report = build_composition("# SceneA\n#0s\ncamera.zoom = 2.0\n\n# SceneB\n#0s\n");
    let scene_b = scene(&report.output, "SceneB");
    assert!(!scene_b.timeline.camera.is_authored());
    assert_eq!(scene_b.timeline.camera.values_at(0), ([0.0, 0.0], 1.0, 0.0));
}

/// The camera carry is sticky, like every other persisted target: a scene that
/// inherits the transform keeps handing it on, so a push-in does not snap back
/// at the *second* boundary in a three-scene run.
#[test]
fn a_carried_camera_keeps_being_carried_until_it_is_removed() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "camera.zoom = 1.8\n",
        "persist camera\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
        "b: Rect, size: (100, 100), at: (200, 200), color: (0, 1, 0, 1)\n",
        "\n",
        "# SceneC\n",
        "#0s\n",
        "c: Rect, size: (100, 100), at: (200, 200), color: (0, 0, 1, 1)\n",
    ));
    let zoom_of = |name: &str| scene(&report.output, name).timeline.camera.values_at(0).1;
    assert_eq!(zoom_of("SceneB"), 1.8, "the first boundary carries");
    assert_eq!(zoom_of("SceneC"), 1.8, "the carry must survive a scene that never mentions it");
}

/// …and `remove camera` is the way to stop it: the scene still receives the
/// transform, but its own successor starts at identity again.
#[test]
fn remove_camera_stops_the_carry_without_fading_anything() {
    let report = build_composition(concat!(
        "# SceneA\n",
        "#0s\n",
        "camera.zoom = 1.8\n",
        "persist camera\n",
        "\n",
        "# SceneB\n",
        "#0s\n",
        "remove camera\n",
        "\n",
        "# SceneC\n",
        "#0s\n",
    ));
    assert_no_errors(&report);

    let scene_b = scene(&report.output, "SceneB");
    assert_eq!(
        scene_b.timeline.camera.values_at(0).1,
        1.8,
        "the receiving scene keeps the push"
    );
    assert!(
        !scene_b.timeline.persistence_flag(CAMERA_TARGET),
        "`remove camera` clears the flag"
    );
    let scene_c = scene(&report.output, "SceneC");
    assert!(!scene_c.timeline.camera.is_authored(), "the chain stops here");
    assert_eq!(scene_c.timeline.camera.values_at(0), ([0.0, 0.0], 1.0, 0.0));
}

#[test]
fn test_composition_step_pause_points() {
    let report = build_target(concat!(
        "# SceneA\n",
        "config { duration: 1.0 }\n",
        "#0s\n",
        "a: Text, text: \"A0\", at: (100, 100)\n",
        "#step 1\n",
        "#500ms\n",
        "#pause\n",
        "# SceneB\n",
        "config { duration: 1.0 }\n",
        "#0s\n",
        "b: Text, text: \"B0\", at: (200, 200)\n",
        "#step 2\n",
        "#500ms\n",
        "#pause\n",
    ));
    let BuildTarget::MultiScene(comp) = &report.output else {
        panic!("expected MultiScene");
    };

    // SceneA has duration 1s (1000ms) and starts at 0ms: pause points at 0 and
    // 500. SceneB starts at 1000ms, so its pauses land at 1000 and 1500.
    assert_eq!(comp.pause_points(), &[0, 500, 1000, 1500]);
    assert_eq!(report.output.pause_points(), &[0, 500, 1000, 1500]);

    // Navigation checks across scene boundaries.
    assert_eq!(report.output.step_next(0), Some(500));
    assert_eq!(report.output.step_next(500), Some(1000));
    assert_eq!(report.output.step_next(1000), Some(1500));
    assert_eq!(report.output.step_next(1500), None);

    assert_eq!(report.output.step_prev(1500), Some(1000));
    assert_eq!(report.output.step_prev(1000), Some(500));
    assert_eq!(report.output.step_prev(500), Some(0));
    assert_eq!(report.output.step_prev(0), None);
}
