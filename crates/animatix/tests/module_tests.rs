use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use animatix::timeline::Timeline;
use animatix_syntax::ast::Expr;
use animatix_syntax::module::{ModuleError, ModuleGraph};

fn temp_project_dir(name: &str) -> PathBuf {
    let unique = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let dir =
        std::env::temp_dir().join(format!("animatix_{}_{}_{}", name, std::process::id(), unique));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_file(path: &Path, contents: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, contents).unwrap();
}

#[test]
fn load_program_collects_public_imported_components_only() {
    let dir = temp_project_dir("public_components");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Throughput") {
    label: Text, text: title
}

component InternalCard(title: "Private") {
    private_label: Text, text: title
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

dashboard: MetricCard, title: "Latency"
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();

    assert!(program.components.contains_key("MetricCard"));
    assert!(!program.components.contains_key("InternalCard"));

    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");
    assert!(expanded_debug.contains("dashboard"));
    assert!(expanded_debug.contains("Latency"));
    assert!(!expanded_debug.contains("MetricCard"));
}

#[test]
fn load_program_rejects_duplicate_component_exports() {
    let dir = temp_project_dir("duplicate_components");
    let entry = dir.join("scene.amx");
    let first = dir.join("first.amx");
    let second = dir.join("second.amx");

    write_file(
        &first,
        r#"
pub component MetricCard(title: "One") {
    label: Text, text: title
}
"#,
    );

    write_file(
        &second,
        r#"
pub component MetricCard(title: "Two") {
    label: Text, text: title
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./first.amx"
import "./second.amx"

dashboard: MetricCard
"#,
    );

    let error = ModuleGraph::new().load_program(&entry).unwrap_err();
    match error {
        ModuleError::DuplicateComponent { name, .. } => assert_eq!(name, "MetricCard"),
        other => panic!("expected duplicate component error, got {other:?}"),
    }
}

#[test]
fn load_program_expand_components_produces_build_input() {
    let dir = temp_project_dir("compile_boundary");
    let entry = dir.join("scene.amx");
    let library = dir.join("components.amx");

    write_file(
        &library,
        r#"
pub component MetricCard(title: "Throughput") {
    title_text: Text, text: title
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./components.amx"

card: MetricCard, title: "Latency"
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(!expanded_debug.contains("MetricCard"));
    assert!(expanded_debug.contains("card"));
    assert!(expanded_debug.contains("Latency"));

    let timeline = Timeline::build(&expanded);
    assert!(timeline.tracks().contains_key("card"));
}

#[test]
fn load_program_collects_namespaced_pub_let_exports() {
    let dir = temp_project_dir("namespaced_exports");
    let entry = dir.join("scene.amx");
    let theme = dir.join("theme.amx");

    write_file(
        &theme,
        r#"
pub let accent = (0.38, 0.78, 1.0, 1.0)
pub let background = (0.04, 0.06, 0.09, 1.0)
let private = (1.0, 0.0, 0.0, 1.0)
"#,
    );

    write_file(
        &entry,
        r#"
import "./theme.amx" as theme

panel: Rect, size: (200, 100), color: theme.accent
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();

    assert!(program.namespaces.contains_key("theme"));
    let theme_ns = program.namespaces.get("theme").unwrap();
    assert!(theme_ns.exports.contains_key("accent"));
    assert!(theme_ns.exports.contains_key("background"));
    assert!(!theme_ns.exports.contains_key("private"));

    // Verify the expanded statements do NOT include theme's statements
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");
    assert!(!expanded_debug.contains("0.38")); // theme's pub let values not flattened
    assert!(!expanded_debug.contains("background")); // theme's other pub let not flattened
    assert!(expanded_debug.contains("panel"));
}

#[test]
fn load_program_aliased_import_does_not_flatten() {
    let dir = temp_project_dir("aliased_no_flatten");
    let entry = dir.join("scene.amx");
    let helper = dir.join("helper.amx");

    write_file(
        &helper,
        r#"
pub let offset = 120
hidden: Ellipse, radius: 50
"#,
    );

    write_file(
        &entry,
        r#"
import "./helper.amx" as helper

visible: Rect, size: (100, 100)
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();

    // The hidden circle from helper should NOT be in expanded statements
    let expanded_debug = format!("{expanded:#?}");
    assert!(!expanded_debug.contains("hidden"));
    assert!(expanded_debug.contains("visible"));
}

#[test]
fn load_program_resolves_reexports() {
    let dir = temp_project_dir("reexports");
    let entry = dir.join("scene.amx");
    let theme = dir.join("theme.amx");
    let colors = dir.join("colors.amx");

    write_file(
        &colors,
        r#"
 pub let primary = (0.38, 0.78, 1.0, 1.0)
 pub let danger = (1.0, 0.2, 0.2, 1.0)
 "#,
    );

    write_file(
        &theme,
        r#"
 import "./colors.amx" as c
 pub let accent = c.primary
 pub let warning = c.danger
 "#,
    );

    write_file(
        &entry,
        r#"
 import "./theme.amx" as theme

 panel: Rect, size: (200, 100), color: theme.accent
 "#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();

    assert!(program.namespaces.contains_key("theme"));
    let theme_ns = program.namespaces.get("theme").unwrap();
    assert!(theme_ns.exports.contains_key("accent"));
    assert!(theme_ns.exports.contains_key("warning"));

    // Verify re-exported values are resolved (not just path expressions)
    let accent_expr = theme_ns.exports.get("accent").unwrap();
    match accent_expr {
        Expr::Tuple(vals) => {
            assert_eq!(vals.len(), 4);
        },
        other => panic!("Expected resolved tuple for accent, got: {:?}", other),
    }

    let warning_expr = theme_ns.exports.get("warning").unwrap();
    match warning_expr {
        Expr::Tuple(vals) => {
            assert_eq!(vals.len(), 4);
        },
        other => panic!("Expected resolved tuple for warning, got: {:?}", other),
    }
}

#[test]
fn load_program_custom_component_action_basic() {
    let dir = temp_project_dir("custom_action");
    let entry = dir.join("scene.amx");
    let library = dir.join("button.amx");

    write_file(
        &library,
        r#"
pub component Button(text: "OK") {
    action pulse {
        self.scale = 1.2 [100ms]
        self.scale = 1.0 [100ms]
    }
    frame: Rect, size: (100, 40)
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./button.amx"

btn: Button, text: "Click"

#0s
pulse btn [200ms]
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();

    // The custom action should be inlined: btn.scale assignments
    let expanded_debug = format!("{expanded:#?}");
    // Custom action should inline scale assignments on btn
    assert!(
        expanded_debug.contains("\"btn\""),
        "Custom action should inline btn assignments, got: {}",
        expanded_debug
    );
    assert!(
        expanded_debug.contains("\"scale\""),
        "Custom action should inline scale property, got: {}",
        expanded_debug
    );
    // ComponentAction should NOT be in output
    assert!(
        !expanded_debug.contains("ComponentAction"),
        "ComponentAction should be removed after inlining"
    );
    // pulse action invocation should be replaced
    assert!(
        !expanded_debug.contains("pulse"),
        "pulse invocation should be replaced with inlined body"
    );
}

#[test]
fn load_program_custom_component_action_multi_target() {
    let dir = temp_project_dir("custom_action_multi_target");
    let entry = dir.join("scene.amx");
    let library = dir.join("button.amx");

    write_file(
        &library,
        r#"
pub component Button(text: "OK") {
    action pulse {
        self.scale = 1.2 [100ms]
        self.scale = 1.0 [100ms]
    }
    frame: Rect, size: (100, 40)
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./button.amx"

btn1: Button, text: "One"
btn2: Button, text: "Two"
rect: Rect

#0s
pulse btn1, btn2, rect [200ms]
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    let scale_count = expanded_debug.matches("\"scale\"").count();
    assert!(
        scale_count >= 4,
        "Expected both custom action bodies inlined, got {scale_count} scale references"
    );
    assert!(
        expanded_debug.contains("pulse"),
        "Builtin fallback action should remain for the non-component target"
    );
    assert!(
        expanded_debug.contains("rect"),
        "Remaining target should still be present in the fallback action"
    );
}

#[test]
fn load_program_expands_component_for_loop_array_actors() {
    let dir = temp_project_dir("component_for_loop_actors");
    let entry = dir.join("scene.amx");
    let library = dir.join("bars.amx");

    write_file(
        &library,
        r#"
pub component Bars(values: List<Num>) {
    row: Row, anchor: scene.center, gap: 24 {
        for v, i in values {
            bar[i]: Rect, size: (100, v)
        }
    }
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./bars.amx"

deck: Bars, values: {10, 20, 30}
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);
    assert!(timeline.tracks().contains_key("deck.bar__0"));
    assert!(timeline.tracks().contains_key("deck.bar__2"));
}

#[test]
fn load_program_custom_component_action_indexed_array_target() {
    let dir = temp_project_dir("component_indexed_action");
    let entry = dir.join("scene.amx");
    let library = dir.join("bars.amx");

    write_file(
        &library,
        r#"
pub component Bars(values: List<Num>) {
    row: Row, anchor: scene.center, gap: 24 {
        for v, i in values {
            bar[i]: Rect, size: (100, v)
        }
    }

    action pulseAt(index: Num) {
        bar[index].scale = 1.1 [100ms]
        bar[index].scale = 1.0 [100ms]
    }
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./bars.amx"

deck: Bars, values: {10, 20, 30}

#0s
pulseAt deck [index: 1]
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");
    let timeline = Timeline::build(&expanded);
    assert!(timeline.tracks().contains_key("deck.bar__1"));
    assert!(
        expanded_debug.contains("deck.bar__1"),
        "indexed custom action target should become deck.bar__1, got: {expanded_debug}"
    );
}

#[test]
fn load_program_custom_component_action_self_keyword() {
    let dir = temp_project_dir("custom_action_self");
    let entry = dir.join("scene.amx");
    let library = dir.join("card.amx");

    write_file(
        &library,
        r#"
pub component Card {
    action glow {
        self.frame.color = accent.primary [300ms]
        self.frame.color = accent.secondary [300ms]
    }
    frame: Rect, size: (200, 120)
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./card.amx"

card1: Card

#0s
glow card1 [500ms]
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();

    let expanded_debug = format!("{expanded:#?}");
    // self should rewrite to the instance label
    assert!(
        expanded_debug.contains("\"card1\""),
        "self should rewrite to instance label: {}",
        expanded_debug
    );
    // self.frame rewrites to just card1 when frame is the root label
    assert!(
        !expanded_debug.contains("\"frame\""),
        "root label frame should be rewritten to instance label, not preserved: {}",
        expanded_debug
    );
    assert!(
        expanded_debug.contains("\"color\""),
        "color property should exist: {}",
        expanded_debug
    );
}

#[test]
fn load_program_custom_component_action_inside_sequence() {
    let dir = temp_project_dir("custom_action_sequence");
    let entry = dir.join("scene.amx");
    let library = dir.join("badge.amx");

    write_file(
        &library,
        r#"
pub component Badge {
    action bounce {
        self.scale = 1.5 [100ms]
        self.scale = 1.0 [100ms]
    }
    icon: Ellipse, radius: 12
}
"#,
    );

    write_file(
        &entry,
        r#"
import "./badge.amx"

badge1: Badge

#0s
sequence {
    bounce badge1 [200ms]
    fade-in badge1 [300ms]
}
"#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();

    // Build timeline to verify sequence timing works with inlined actions
    let timeline = animatix::timeline::Timeline::build(&expanded);
    let track_names: Vec<_> = timeline.tracks().keys().collect();
    assert!(
        timeline.tracks().contains_key("badge1"),
        "badge1 should exist. Tracks: {:?}",
        track_names
    );
    let track = timeline.tracks().get("badge1").unwrap();
    // bounce: each inlined assignment gets the invocation [200ms] modifier,
    // so total bounce span is 400ms (200ms + 200ms). fade-in starts at 400ms.
    let scale = track.geometry.scale.as_ref().expect("scale should exist");
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
    // At 500ms, fade-in is 100ms into 300ms: opacity ~0.33
    let opacity = track.style.opacity.as_ref().expect("opacity should exist");
    assert!(
        opacity.evaluate(500) > 0.2 && opacity.evaluate(500) < 0.5,
        "Opacity should be fading in at 500ms, got {}",
        opacity.evaluate(500)
    );
}

#[test]
fn load_program_expands_component_with_slots() {
    let dir = temp_project_dir("slots_basic");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
        r#"
 pub component SlideLayout {
     header: Col {
         @slot
     }
 }
 "#,
    );

    write_file(
        &entry,
        r#"
 import "./slides.amx"

 slide: SlideLayout {
     @header {
         title: Text, text: "Hello"
     }
 }
 "#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    // Component should be expanded (no "SlideLayout" left)
    assert!(!expanded_debug.contains("SlideLayout"));
    // Instance should be there
    assert!(expanded_debug.contains("slide"));
    // Filled item should be present
    assert!(expanded_debug.contains("\"title\""));
    assert!(expanded_debug.contains("Hello"));
}

#[test]
fn load_program_slot_defaults_fallback() {
    let dir = temp_project_dir("slots_defaults");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
        r#"
 pub component SlideLayout {
     footer: Col {
         @slot
         Text, text: "Default Footer"
     }
 }
 "#,
    );

    write_file(
        &entry,
        r#"
 import "./slides.amx"

  slide: SlideLayout {}
 "#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(!expanded_debug.contains("SlideLayout"));
    assert!(expanded_debug.contains("slide"));
    // Default item should appear
    assert!(expanded_debug.contains("Default Footer"));
}

#[test]
fn load_program_slot_mixed_filled_and_unfilled() {
    let dir = temp_project_dir("slots_mixed");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
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
    );

    write_file(
        &entry,
        r#"
 import "./slides.amx"

  slide: SlideLayout {
      @body {
          Ellipse, radius: 20
      }
  }
 // header and footer not filled — should use defaults
 "#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(!expanded_debug.contains("SlideLayout"));
    assert!(expanded_debug.contains("slide"));
    // Default header and footer should appear
    assert!(expanded_debug.contains("Default Header"));
    assert!(expanded_debug.contains("Default Footer"));
    // Filled body should appear
    assert!(expanded_debug.contains("Ellipse"));
}

#[test]
fn load_program_slot_unfilled_required_becomes_empty() {
    let dir = temp_project_dir("slots_unfilled");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
        r#"
 pub component SlideLayout {
     body: Group {
         @slot
     }
 }
 "#,
    );

    write_file(
        &entry,
        r#"
 import "./slides.amx"

  slide: SlideLayout {}
 "#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(!expanded_debug.contains("SlideLayout"));
    // The container should still exist (just empty)
    assert!(expanded_debug.contains("slide"));
    assert!(expanded_debug.contains("children: []"));
}

#[test]
fn load_program_slot_multiple_instances_different_fills() {
    let dir = temp_project_dir("slot_multi_instance");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
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
    );

    write_file(
        &entry,
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
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(expanded_debug.contains("first"));
    assert!(expanded_debug.contains("Second Header"));
    assert!(expanded_debug.contains("Second Body"));
    assert!(expanded_debug.contains("first"));
    assert!(expanded_debug.contains("First Header"));
    assert!(expanded_debug.contains("First Body"));
    assert!(!expanded_debug.contains("Card"));
}

#[test]
fn load_program_slot_empty_fill() {
    let dir = temp_project_dir("slot_empty_fill");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
        r#"
 pub component Slide {
     header: Col {
         @slot
         Text, text: "Default Header"
     }
 }
 "#,
    );

    write_file(
        &entry,
        r#"
 import "./slides.amx"

 slide: Slide {
     @header {}
 }
 "#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(expanded_debug.contains("slide"));
    assert!(!expanded_debug.contains("Default Header"));
    assert!(!expanded_debug.contains("Slide"));
}

#[test]
fn load_program_slot_with_component_as_fill() {
    let dir = temp_project_dir("slot_component_fill");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
        r#"
 pub component Slide {
     header: Col {
         @slot
     }
 }
 "#,
    );

    write_file(
        &entry,
        r#"
 import "./slides.amx"

 slide: Slide {
     @header {
         title: Text, text: "My Title"
     }
 }
 "#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(expanded_debug.contains("slide"));
    assert!(expanded_debug.contains("My Title"));
    assert!(!expanded_debug.contains("Slide"));
}

#[test]
fn load_program_slot_fill_nonexistent_slot() {
    let dir = temp_project_dir("slot_nonexistent");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
        r#"
 pub component Card {
     header: Col {
         @slot
         Text, text: "Default Header"
     }
 }
 "#,
    );

    write_file(
        &entry,
        r#"
 import "./slides.amx"

 card: Card {
     @nonexistent {
         Text, text: "This should be ignored"
     }
 }
 "#,
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(expanded_debug.contains("card"));
    assert!(expanded_debug.contains("Default Header"));
    assert!(!expanded_debug.contains("This should be ignored"));
    assert!(!expanded_debug.contains("Card"));
}

#[test]
fn load_program_slot_all_filled_no_defaults_used() {
    let dir = temp_project_dir("slot_all_filled");
    let entry = dir.join("scene.amx");
    let library = dir.join("slides.amx");

    write_file(
        &library,
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
    );

    write_file(
        &entry,
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
    );

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let expanded_debug = format!("{expanded:#?}");

    assert!(expanded_debug.contains("card"));
    assert!(expanded_debug.contains("Custom Header"));
    assert!(expanded_debug.contains("Custom Footer"));
    assert!(!expanded_debug.contains("Default Header"));
    assert!(!expanded_debug.contains("Default Footer"));
    assert!(!expanded_debug.contains("Card"));
}

#[test]
fn load_program_three_slot_fills() {
    let dir = temp_project_dir("three_slot_fills");
    let entry = dir.join("scene.amx");

    write_file(
        &entry,
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

    let program = ModuleGraph::new().load_program(&entry).unwrap();

    // Also verify timeline builds correctly
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);

    // Check that all 3 cards exist in the timeline
    assert!(timeline.tracks().contains_key("card_a"), "card_a should exist");
    assert!(timeline.tracks().contains_key("card_b"), "card_b should exist");
    assert!(timeline.tracks().contains_key("card_c"), "card_c should exist");
}

#[test]
fn load_program_custom_component_action_self_nested_path() {
    let dir = temp_project_dir("self_nested_path");
    let entry = dir.join("scene.amx");

    write_file(
        &entry,
        r##"
config { colorscheme: "editorial-dark", resolution: (800, 500) }

pub component Card(title: "Card") {
  frame: Rect, size: (200, 120), color: surface.primary, corner_radius: 8
  title_text: Text, text: title, font_size: 18, color: text.primary, at: (0, -30)

  action highlight {
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

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);

    // Verify the card and its children exist after expansion
    // Note: 'frame' is the root label so it becomes just 'card_a', not 'card_a.frame'
    assert!(timeline.tracks().contains_key("card_a"), "card_a (root frame) should exist");
    assert!(
        timeline.tracks().contains_key("card_a.title_text"),
        "card_a.title_text should exist"
    );
}

#[test]
fn load_program_comments_in_always_block() {
    let dir = temp_project_dir("comments_always");
    let entry = dir.join("scene.amx");

    write_file(
        &entry,
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

    let program = ModuleGraph::new().load_program(&entry).unwrap();
    let expanded = program.expand_components();
    let timeline = Timeline::build(&expanded);

    // Verify the box exists
    assert!(timeline.tracks().contains_key("box"), "box should exist");
}
