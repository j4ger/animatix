use std::path::{Path, PathBuf};

use animatix::composition::BuildTarget;
use animatix::timeline::{BuildOptions, DebugRenderOptions, SceneDimensions, Timeline};
use animatix_syntax::ast::{Expr, Stmt};
use animatix_syntax::module::{LoadedProgram, ModuleError, ModuleGraph, SourceAccess};

fn load(entry: &Path, graph: &mut ModuleGraph) -> LoadedProgram {
    graph.load_program(entry).unwrap()
}

/// Load the `scene.amx` of a virtual project.
fn load_scene(files: &[(&str, &str)]) -> LoadedProgram {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    for (path, contents) in files {
        graph.upsert_source(PathBuf::from(format!("virtual:///{}", path)), contents.to_string());
    }
    load(Path::new("virtual:///scene.amx"), &mut graph)
}

/// Load a project whose only file is the entry scene.
fn load_entry(source: &str) -> LoadedProgram {
    load_scene(&[("scene.amx", source)])
}

/// Component expansion, with the expander's diagnostics discarded so the
/// tests can assert on the resulting shape alone.
fn expanded(program: &LoadedProgram) -> Vec<Stmt> {
    program.expand_components(&mut Vec::new())
}

fn expanded_debug(program: &LoadedProgram) -> String {
    format!("{:#?}", expanded(program))
}

/// Assert the expanded program mentions every `present` needle and none of the
/// `absent` ones. Each needle is checked independently.
fn assert_contents(program: &LoadedProgram, present: &[&str], absent: &[&str]) {
    let debug = expanded_debug(program);
    for needle in present {
        assert!(debug.contains(needle), "expanded program should contain {needle:?}:\n{debug}");
    }
    for needle in absent {
        assert!(
            !debug.contains(needle),
            "expanded program should not contain {needle:?}:\n{debug}"
        );
    }
}

fn tuple_arity(expr: &Expr) -> usize {
    match expr {
        Expr::Tuple(vals) => vals.len(),
        other => panic!("expected a resolved tuple, got {other:?}"),
    }
}

const BUTTON_COMPONENT: &str = r#"
pub component Button(text: "OK") {
    fn pulse {
        self.scale = 1.2 [100ms]
        self.scale = 1.0 [100ms]
    }
    frame: Rect, size: (100, 40)
}
"#;

#[test]
fn load_program_collects_public_imported_components_only() {
    let program = load_scene(&[
        (
            "components.amx",
            r#"
pub component MetricCard(title: "Throughput") {
    label: Text, text: title
}

component InternalCard(title: "Private") {
    private_label: Text, text: title
}
"#,
        ),
        (
            "scene.amx",
            r#"
import "./components.amx"

dashboard: MetricCard, title: "Latency"
"#,
        ),
    ]);

    assert!(program.components.contains_key("MetricCard"));
    assert!(!program.components.contains_key("InternalCard"));
    assert_contents(&program, &["dashboard", "Latency"], &["MetricCard"]);
}

#[test]
fn load_program_rejects_duplicate_component_exports() {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        PathBuf::from("virtual:///first.amx"),
        r#"
pub component MetricCard(title: "One") {
    label: Text, text: title
}
"#,
    );
    graph.upsert_source(
        PathBuf::from("virtual:///second.amx"),
        r#"
pub component MetricCard(title: "Two") {
    label: Text, text: title
}
"#,
    );
    graph.upsert_source(
        PathBuf::from("virtual:///scene.amx"),
        r#"
import "./first.amx"
import "./second.amx"

dashboard: MetricCard
"#,
    );

    let error = graph.load_program(Path::new("virtual:///scene.amx")).unwrap_err();
    match error {
        ModuleError::DuplicateComponent { name, .. } => assert_eq!(name, "MetricCard"),
        other => panic!("expected duplicate component error, got {other:?}"),
    }
}

#[test]
fn load_program_expand_components_produces_build_input() {
    let program = load_scene(&[
        (
            "components.amx",
            r#"
pub component MetricCard(title: "Throughput") {
    title_text: Text, text: title
}
"#,
        ),
        (
            "scene.amx",
            r#"
import "./components.amx"

card: MetricCard, title: "Latency"
"#,
        ),
    ]);

    assert_contents(&program, &["card", "Latency"], &["MetricCard"]);

    let timeline = Timeline::build(&expanded(&program));
    assert!(timeline.tracks().contains_key("card"));
}

#[test]
fn load_program_collects_namespaced_pub_let_exports() {
    let program = load_scene(&[
        (
            "theme.amx",
            r#"
pub let accent = (0.38, 0.78, 1.0, 1.0)
pub let background = (0.04, 0.06, 0.09, 1.0)
let private = (1.0, 0.0, 0.0, 1.0)
"#,
        ),
        (
            "scene.amx",
            r#"
import "./theme.amx" as theme

panel: Rect, size: (200, 100), color: theme.accent
"#,
        ),
    ]);

    assert!(program.namespaces.contains_key("theme"));
    let theme_ns = program.namespaces.get("theme").unwrap();
    assert!(theme_ns.exports.contains_key("accent"));
    assert!(theme_ns.exports.contains_key("background"));
    assert!(!theme_ns.exports.contains_key("private"));

    // Theme's statements are not flattened into the entry program.
    assert_contents(&program, &["panel"], &["0.38", "background"]);
}

#[test]
fn load_program_collects_and_typechecks_namespaced_type_aliases() {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        PathBuf::from("virtual:///types.amx"),
        r#"
pub type Metric = Bool | Str
let private = 1
"#,
    );
    let entry = PathBuf::from("virtual:///scene.amx");

    graph.upsert_source(
        entry.clone(),
        r#"
import "./types.amx" as types

pub component Card(value: types::Metric) {}
card: Card, value: "Revenue"
"#,
    );
    let mut program = graph.load_program(&entry).unwrap();
    assert!(program.namespaces.get("types").unwrap().type_exports.contains_key("Metric"));
    assert!(program.typecheck().is_empty(), "imported type alias should typecheck");

    graph.upsert_source(
        entry.clone(),
        r#"
import "./types.amx" as types

pub component Card(value: types::Metric) {}
bad: Card, value: 42
"#,
    );
    let mut bad_program = graph.load_program(&entry).unwrap();
    assert!(
        !bad_program.typecheck().is_empty(),
        "namespaced alias should reject a non-matching value"
    );
}

#[test]
fn load_program_nested_aliased_namespace_depth() {
    let program = load_scene(&[
        ("inner.amx", "pub let accent = (1.0, 0.0, 0.0, 1.0)\n"),
        ("lib.amx", "import \"./inner.amx\" as inner\n"),
        ("scene.amx", "import \"./lib.amx\" as lib\n"),
    ]);

    let inner_ns = program.namespaces.get("lib").unwrap().namespaces.get("inner").unwrap();
    assert!(inner_ns.exports.contains_key("accent"));
}

#[test]
fn load_program_aliased_import_does_not_flatten() {
    let program = load_scene(&[
        (
            "helper.amx",
            r#"
pub let offset = 120
hidden: Ellipse, radius: 50
"#,
        ),
        (
            "scene.amx",
            r#"
import "./helper.amx" as helper

visible: Rect, size: (100, 100)
"#,
        ),
    ]);

    // The aliased helper's hidden circle must not leak into the entry program.
    assert_contents(&program, &["visible"], &["hidden"]);
}

#[test]
fn load_program_resolves_reexports() {
    let program = load_scene(&[
        (
            "colors.amx",
            r#"
 pub let primary = (0.38, 0.78, 1.0, 1.0)
 pub let danger = (1.0, 0.2, 0.2, 1.0)
  "#,
        ),
        (
            "theme.amx",
            r#"
 import "./colors.amx" as c
 pub let accent = c.primary
 pub let warning = c.danger
  "#,
        ),
        (
            "scene.amx",
            r#"
 import "./theme.amx" as theme

 panel: Rect, size: (200, 100), color: theme.accent
  "#,
        ),
    ]);

    let theme_ns = program.namespaces.get("theme").unwrap();
    assert!(theme_ns.exports.contains_key("accent"));
    assert!(theme_ns.exports.contains_key("warning"));

    // Re-exported values are resolved, not left as path expressions.
    assert_eq!(tuple_arity(theme_ns.exports.get("accent").unwrap()), 4);
    assert_eq!(tuple_arity(theme_ns.exports.get("warning").unwrap()), 4);
}

#[test]
fn load_program_custom_component_action_basic() {
    let program = load_scene(&[
        ("button.amx", BUTTON_COMPONENT),
        (
            "scene.amx",
            r#"
import "./button.amx"

btn: Button, text: "Click"

#0s
pulse btn [200ms]
"#,
        ),
    ]);

    // The custom action is inlined onto the instance: btn.scale assignments.
    let debug = expanded_debug(&program);
    assert!(
        debug.contains("\"btn\""),
        "custom action should inline btn assignments: {debug}"
    );
    assert!(
        debug.contains("\"scale\""),
        "custom action should inline scale property: {debug}"
    );
    assert!(
        !debug.contains("ComponentAction"),
        "ComponentAction should be removed after inlining"
    );
    assert!(
        !debug.contains("pulse"),
        "pulse invocation should be replaced with inlined body"
    );
}

#[test]
fn load_program_custom_component_action_multi_target() {
    let program = load_scene(&[
        ("button.amx", BUTTON_COMPONENT),
        (
            "scene.amx",
            r#"
import "./button.amx"

btn1: Button, text: "One"
btn2: Button, text: "Two"
rect: Rect

#0s
pulse btn1, btn2, rect [200ms]
"#,
        ),
    ]);

    let debug = expanded_debug(&program);
    let scale_count = debug.matches("\"scale\"").count();
    assert!(
        scale_count >= 4,
        "Expected both custom action bodies inlined, got {scale_count} scale references"
    );
    assert!(
        debug.contains("pulse"),
        "Builtin fallback action should remain for the non-component target"
    );
    assert!(
        debug.contains("rect"),
        "Remaining target should still be present in the fallback action"
    );
}

#[test]
fn load_program_expands_component_for_loop_array_actors() {
    let program = load_scene(&[
        (
            "bars.amx",
            r#"
pub component Bars(values: List<Num>) {
    row: Row, anchor: scene.center, gap: 24 {
        for v, i in values {
            bar[i]: Rect, size: (100, v)
        }
    }
}
"#,
        ),
        (
            "scene.amx",
            r#"
import "./bars.amx"

deck: Bars, values: {10, 20, 30}
"#,
        ),
    ]);

    let timeline = Timeline::build(&expanded(&program));
    assert!(timeline.tracks().contains_key("deck.bar__0"));
    assert!(timeline.tracks().contains_key("deck.bar__2"));
}

#[test]
fn load_program_custom_component_action_indexed_array_target() {
    let program = load_scene(&[
        (
            "bars.amx",
            r#"
pub component Bars(values: List<Num>) {
    row: Row, anchor: scene.center, gap: 24 {
        for v, i in values {
            bar[i]: Rect, size: (100, v)
        }
    }

    fn pulseAt(index: Num) {
        bar[index].scale = 1.1 [100ms]
        bar[index].scale = 1.0 [100ms]
    }
}
"#,
        ),
        (
            "scene.amx",
            r#"
import "./bars.amx"

deck: Bars, values: {10, 20, 30}

#0s
pulseAt deck [index: 1]
"#,
        ),
    ]);

    let timeline = Timeline::build(&expanded(&program));
    assert!(
        timeline.tracks().contains_key("deck.bar__1"),
        "indexed fn target should resolve to deck.bar__1 at build time"
    );
}

#[test]
fn load_program_custom_component_action_self_keyword() {
    let program = load_scene(&[
        (
            "card.amx",
            r#"
pub component Card {
    fn glow {
        self.frame.color = accent.primary [300ms]
        self.frame.color = accent.secondary [300ms]
    }
    frame: Rect, size: (200, 120)
}
"#,
        ),
        (
            "scene.amx",
            r#"
import "./card.amx"

card1: Card

#0s
glow card1 [500ms]
"#,
        ),
    ]);

    let debug = expanded_debug(&program);
    // self rewrites to the instance label; self.frame collapses to just card1
    // because frame is the root label.
    assert!(debug.contains("\"card1\""), "self should rewrite to instance label: {debug}");
    assert!(
        !debug.contains("\"frame\""),
        "root label frame should be rewritten to instance label, not preserved: {debug}"
    );
    assert!(debug.contains("\"color\""), "color property should exist: {debug}");
}

#[test]
fn load_program_custom_component_action_inside_sequence() {
    let program = load_scene(&[
        (
            "badge.amx",
            r#"
pub component Badge {
    fn bounce {
        self.scale = 1.5 [100ms]
        self.scale = 1.0 [100ms]
    }
    icon: Ellipse, radius: 12
}
"#,
        ),
        (
            "scene.amx",
            r#"
import "./badge.amx"

badge1: Badge

#0s
sequence {
    bounce badge1 [200ms]
    fade-in badge1 [300ms]
}
"#,
        ),
    ]);

    // Build a timeline to verify sequence timing works with inlined actions.
    let timeline = Timeline::build(&expanded(&program));
    assert!(
        timeline.tracks().contains_key("badge1"),
        "badge1 should exist. Tracks: {:?}",
        timeline.tracks().keys().collect::<Vec<_>>()
    );
    let badge_id = timeline.get_entity_id("badge1").expect("badge1 entity");
    // bounce: each inlined assignment gets the invocation [200ms] modifier,
    // so total bounce span is 400ms (200ms + 200ms). fade-in starts at 400ms.
    let scale = timeline.world().geometry(badge_id).scale.as_ref().expect("scale should exist");
    // At 100ms, first scale assignment is halfway through 200ms: 1.0 → 1.5 = 1.25
    assert!(
        (scale.evaluate(100) - 1.25).abs() < 0.01,
        "Scale should be 1.25 at 100ms, got {}",
        scale.evaluate(100)
    );
    // At 250ms, second scale assignment is 50ms into 200ms: 1.5 → 1.0 = 1.375
    assert!(
        (scale.evaluate(250) - 1.375).abs() < 0.01,
        "Scale should be 1.375 at 250ms, got {}",
        scale.evaluate(250)
    );
    // At 500ms, fade-in is 100ms into 300ms. An uneased entrance now defaults
    // to `expo-out` (it decelerates into place), so a third of the way through
    // the segment is already ~90% opaque rather than the ~33% a linear ramp
    // would give. See `default_easing_for`.
    let opacity = timeline.world().style(badge_id).opacity.as_ref().expect("opacity should exist");
    assert!(
        opacity.evaluate(500) > 0.8 && opacity.evaluate(500) < 1.0,
        "Opacity should have most of the way in at 500ms under expo-out, got {}",
        opacity.evaluate(500)
    );
}

#[test]
fn load_program_expands_component_with_slots() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component SlideLayout {
     header: Col {
         @slot
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

 slide: SlideLayout {
     @header {
         title: Text, text: "Hello"
     }
 }
 "#,
        ),
    ]);

    assert_contents(&program, &["slide", "\"title\"", "Hello"], &["SlideLayout"]);
}

#[test]
fn load_program_slot_defaults_fallback() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component SlideLayout {
     footer: Col {
         @slot
         Text, text: "Default Footer"
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

  slide: SlideLayout {}
 "#,
        ),
    ]);

    assert_contents(&program, &["slide", "Default Footer"], &["SlideLayout"]);
}

#[test]
fn load_program_slot_mixed_filled_and_unfilled() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component SlideLayout {
     header: Col {
         @slot
         Text, text: "Default Header"
     }
     body: Group {
         @slot
     }
     footer: Col {
         @slot
         Text, text: "Default Footer"
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

  slide: SlideLayout {
      @body {
          Ellipse, radius: 20
      }
  }
 // header and footer not filled — should use defaults
 "#,
        ),
    ]);

    assert_contents(
        &program,
        &["slide", "Default Header", "Default Footer", "Ellipse"],
        &["SlideLayout"],
    );
}

#[test]
fn load_program_slot_unfilled_required_becomes_empty() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component SlideLayout {
     body: Group {
         @slot
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

  slide: SlideLayout {}
 "#,
        ),
    ]);

    // The container still exists, just empty.
    assert_contents(&program, &["slide", "children: []"], &["SlideLayout"]);
}

#[test]
fn load_program_slot_multiple_instances_different_fills() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component Card {
     header: Col {
         @slot
     }
     body: Group {
         @slot
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

 first: Card {
     @header {
         Text, text: "First Header"
     }
     @body {
         Text, text: "First Body"
     }
 }

 second: Card {
     @header {
         Text, text: "Second Header"
     }
     @body {
         Text, text: "Second Body"
     }
 }
 "#,
        ),
    ]);

    assert_contents(
        &program,
        &[
            "first",
            "First Header",
            "First Body",
            "Second Header",
            "Second Body",
        ],
        &["Card"],
    );
}

#[test]
fn load_program_slot_empty_fill() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component Slide {
     header: Col {
         @slot
         Text, text: "Default Header"
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

 slide: Slide {
     @header {}
 }
 "#,
        ),
    ]);

    assert_contents(&program, &["slide"], &["Default Header", "Slide"]);
}

#[test]
fn load_program_slot_with_component_as_fill() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component Slide {
     header: Col {
         @slot
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

 slide: Slide {
     @header {
         title: Text, text: "My Title"
     }
 }
 "#,
        ),
    ]);

    assert_contents(&program, &["slide", "My Title"], &["Slide"]);
}

#[test]
fn load_program_slot_fill_nonexistent_slot() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component Card {
     header: Col {
         @slot
         Text, text: "Default Header"
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

 card: Card {
     @nonexistent {
         Text, text: "This should be ignored"
     }
 }
 "#,
        ),
    ]);

    assert_contents(&program, &["card", "Default Header"], &["This should be ignored", "Card"]);
}

#[test]
fn load_program_slot_all_filled_no_defaults_used() {
    let program = load_scene(&[
        (
            "slides.amx",
            r#"
 pub component Card {
     header: Col {
         @slot
         Text, text: "Default Header"
     }
     footer: Col {
         @slot
         Text, text: "Default Footer"
     }
 }
 "#,
        ),
        (
            "scene.amx",
            r#"
 import "./slides.amx"

 card: Card {
     @header {
         Text, text: "Custom Header"
     }
     @footer {
         Text, text: "Custom Footer"
     }
 }
 "#,
        ),
    ]);

    assert_contents(
        &program,
        &["card", "Custom Header", "Custom Footer"],
        &["Default Header", "Default Footer", "Card"],
    );
}

#[test]
fn load_program_three_slot_fills() {
    let program = load_entry(
        r##"
config { colorscheme: "editorial-dark", resolution: (800, 500) }

pub component Card(title: "Card") {
  frame: Rect, size: (200, 120), color: surface.primary, corner_radius: 8
  title_text: Text, text: title, font_size: 18, color: text.primary, at: (0, -30)
  content: Col, gap: 8, at: (0, 20) {
    @slot
  }
}

#0s
scene.background_color = "#1a1a2e"

card_a: Card, title: "Design", anchor: scene.center, offset: (-120, 0) {
  @content {
    icon: Ellipse, size: (16, 16), color: accent.primary
    desc: Text, text: "Visual systems", font_size: 12, color: text.secondary
  }
}

card_b: Card, title: "Engineer", anchor: scene.center, offset: (120, 0) {
  @content {
    icon: Rect, size: (16, 16), color: accent.warning
    desc: Text, text: "Build pipelines", font_size: 12, color: text.secondary
  }
}

card_c: Card, title: "Test", anchor: scene.center, offset: (0, 0) {
  @content {
    icon: Rect, size: (16, 16), color: accent.danger
    desc: Text, text: "Third card", font_size: 12, color: text.secondary
  }
}
"##,
    );

    let timeline = Timeline::build(&expanded(&program));
    for card in ["card_a", "card_b", "card_c"] {
        assert!(timeline.tracks().contains_key(card), "{card} should exist");
    }
}

#[test]
fn load_program_custom_component_action_self_nested_path() {
    let program = load_entry(
        r##"
config { colorscheme: "editorial-dark", resolution: (800, 500) }

pub component Card(title: "Card") {
  frame: Rect, size: (200, 120), color: surface.primary, corner_radius: 8
  title_text: Text, text: title, font_size: 18, color: text.primary, at: (0, -30)

  fn highlight {
    self.frame.color = accent.warning
    self.title_text.color = accent.danger
  }
}

#0s
scene.background_color = "#1a1a2e"

card_a: Card, title: "Design", anchor: scene.center, offset: (-120, 0)

#1s
highlight card_a
"##,
    );

    let timeline = Timeline::build(&expanded(&program));
    // 'frame' is the root label, so it becomes just 'card_a'.
    assert!(timeline.tracks().contains_key("card_a"), "card_a (root frame) should exist");
    assert!(
        timeline.tracks().contains_key("card_a.title_text"),
        "card_a.title_text should exist"
    );
}

#[test]
fn load_program_comments_in_always_block() {
    let program = load_entry(
        r##"
config { colorscheme: "editorial-dark", resolution: (800, 500) }

#0s
scene.background_color = "#1a1a2e"

box: Rect, size: (100, 100), color: accent.primary, anchor: scene.center

always {
  // This comment should not produce an IR warning
  box.rotation = box.rotation + 1
}
"##,
    );

    let timeline = Timeline::build(&expanded(&program));
    assert!(timeline.tracks().contains_key("box"), "box should exist");
}

#[test]
fn load_program_from_memory_sources_resolves_imports() {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        PathBuf::from("virtual:///components.amx"),
        r#"
pub component MetricCard(title: "Throughput") {
    label: Text, text: title
}
"#,
    );
    graph.upsert_source(
        PathBuf::from("virtual:///scene.amx"),
        r#"
import "./components.amx"

dashboard: MetricCard, title: "Latency"
"#,
    );

    let program = graph.load_program(Path::new("virtual:///scene.amx")).unwrap();
    assert!(program.components.contains_key("MetricCard"));
    assert!(!program.components.contains_key("InternalCard"));
    assert_contents(&program, &["dashboard", "Latency"], &["MetricCard"]);
}

#[test]
fn transform_property_builds_without_diagnostics() {
    let mut program = load_entry(
        r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }

a: Rect, size: (100, 100), transform: (1, 0.5, 0, 1, 0, 0), color: accent.primary, at: (200, 150)

#0s
fade-in a [0ms]
"#,
    );

    let entry = Path::new("virtual:///scene.amx");
    let type_diagnostics = program.typecheck();
    assert!(
        type_diagnostics.is_empty(),
        "transform declaration should typecheck: {type_diagnostics:?}"
    );
    let ast = expanded(&program);
    let options = BuildOptions::default();
    let report = BuildTarget::build_with_options(&ast, &program.namespaces, Some(entry), options);
    assert!(
        report.diagnostics.is_empty(),
        "transform declaration should build: {:?}",
        report.diagnostics
    );
}

#[test]
fn component_instance_opacity_multiplies_with_root_opacity() {
    // A component instance behaves like a group: instance `opacity: 0.5` on a
    // root actor already authored `opacity: 0.5` must render at 0.25, not let
    // the instance value replace the root's own value.
    let program = load_entry(
        r#"
pub component Box() {
    box: Stack, size: (100, 100), opacity: 0.5 {
        r: Rect, size: (100, 100), color: (1.0, 0.0, 0.0, 1.0), opacity: 1
    }
}

#0s
b: Box, at: (320, 180), opacity: 0.5
"#,
    );
    let timeline = Timeline::build(&expanded(&program));
    let mut filter_backend = None;
    let frame = timeline.evaluate_program_with_debug(
        0.0,
        SceneDimensions {
            width: 640,
            height: 360,
        },
        DebugRenderOptions::default(),
        &mut filter_backend,
    );
    let item = frame
        .items
        .iter()
        .find(|item| item.label == "b")
        .expect("the expanded root actor must render");
    assert!(
        (item.opacity - 0.25).abs() < 1e-6,
        "instance opacity must multiply into the root's authored opacity, got {}",
        item.opacity
    );
    let child = frame
        .items
        .iter()
        .find(|item| item.label == "b.r")
        .expect("the component's inner actor must render");
    assert!(
        (child.opacity - 0.25).abs() < 1e-6,
        "the composed opacity must reach the component's inner actors, got {}",
        child.opacity
    );
}

/// `load_program_with_source` installs a temporary override; the graph must not
/// keep it for the next load.
#[test]
fn load_program_with_source_override_does_not_leak_source_map() {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        PathBuf::from("virtual:///scene.amx"),
        "box: Rect, size: (100, 100)\n".to_string(),
    );
    let entry = Path::new("virtual:///scene.amx");

    let first = graph
        .load_program_with_source(entry, Some("#0s\ntitle: Text, text: \"Override\"\n"))
        .unwrap();
    assert!(contains_actor(&first.statements, "title", "Text"));

    let second = graph.load_program(entry).unwrap();
    assert!(contains_actor(&second.statements, "box", "Rect"));
    assert!(!contains_actor(&second.statements, "title", "Text"));
}

/// A failed override still restores the previous source, so the next load sees
/// the original program rather than the broken override.
#[test]
fn with_source_restores_previous_source_after_error() {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        PathBuf::from("virtual:///scene.amx"),
        "box: Rect, size: (100, 100)\n".to_string(),
    );
    let entry = Path::new("virtual:///scene.amx");

    let result =
        graph.with_source(entry, "this is not valid @@@ syntax", |graph| graph.load_program(entry));
    assert!(result.is_err(), "invalid override should fail");

    let restored = graph.load_program(entry).unwrap();
    assert!(contains_actor(&restored.statements, "box", "Rect"));
    assert!(!contains_actor(&restored.statements, "title", "Text"));
}

fn contains_actor(stmts: &[Stmt], label: &str, ty: &str) -> bool {
    stmts.iter().any(|stmt| match stmt {
        Stmt::ActorDecl {
            label: stmt_label,
            ty: stmt_ty,
            ..
        } => stmt_label == label && stmt_ty == ty,
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ForLoop { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. } => contains_actor(body, label, ty),
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            contains_actor(then_branch, label, ty)
                || else_branch.as_ref().is_some_and(|body| contains_actor(body, label, ty))
        },
        _ => false,
    })
}
