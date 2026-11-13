//! Integration coverage for indexed component instances.

use std::path::Path;

use animatix_syntax::module::ModuleGraph;

use super::support::text_at;
use crate::timeline::{SceneDimensions, Timeline, Value};

fn indexed_actor_declarations(statements: &[crate::ast::Stmt], out: &mut Vec<String>) {
    for statement in statements {
        match statement {
            crate::ast::Stmt::ActorDecl {
                label,
                array_index: Some(index),
                ..
            } => out.push(format!("{label}[{index:?}]")),
            crate::ast::Stmt::ForLoop { body, .. } => indexed_actor_declarations(body, out),
            _ => {},
        }
    }
}

#[test]
fn nested_indexed_component_calls_keep_roots_children_and_targets_distinct() {
    let source = r#"
pub component Badge(label: Str) {
  badge_text: Text, text: label
}
pub component MetricCard(title: Str) {
  tile: Col {
    title_text: Text, text: title, color: accent.primary
    frame: Rect, size: (120, 80), color: title_text.color
    for marker, j in {"dup", "dup"} {
      markers[j]: Text, text: marker
    }
    badge: Badge, label: title
  }
  fn reveal { fade-in title_text [200ms] }
  always { title_text.opacity = 0.5 }
}

for title, i in {"same", "same"} {
  for unused, j in {0, 1} {
    cards[i * 2 + j]: MetricCard, title: title, at: (320, 180)
    copies[i * 2 + j]: MetricCard, title: title, at: (320, 180)
  }
}
featured: MetricCard, title: "same"

#0s
for unused, i in {0, 1, 2, 3} {
  fade-in cards[i] [200ms]
}

#0.1s
cards[1].title_text.text = "updated"

always {
  cards[3].opacity = 0.75
}

#0.3s
for title, i in {"same", "same"} {
  for unused, j in {0, 1} {
    reveal cards[i * 2 + j]
  }
}
"#;
    let mut modules = ModuleGraph::new();
    let mut program = modules
        .load_program_with_source(Path::new("virtual:///indexed_cards.amx"), Some(source))
        .expect("load component program");
    let type_diagnostics = program.typecheck();
    assert!(
        type_diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "component parameter types: {type_diagnostics:?}"
    );
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    assert!(expansion_errors.is_empty(), "expansion errors: {expansion_errors:?}");

    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    let timeline = report.output;
    assert!(
        timeline.build_loop_scopes.is_empty(),
        "build loop scopes must not escape the build"
    );
    let mut expanded_indices = Vec::new();
    indexed_actor_declarations(&expanded, &mut expanded_indices);
    let unexpected: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.code,
                crate::diagnostics::DiagnosticCode::DuplicateLabel
                    | crate::diagnostics::DiagnosticCode::UndefinedLabel
            )
        })
        .collect();
    assert!(unexpected.is_empty(), "component identity diagnostics: {unexpected:?}");

    let labels: Vec<_> = timeline.world.tracks.iter().map(|(label, _)| label.as_str()).collect();
    for base in ["cards", "copies"] {
        for index in 0..4 {
            let root = format!("{base}__{index}");
            assert!(
                timeline.get_entity_id(&root).is_some(),
                "missing component root {root}; runtime labels: {labels:?}; expanded indexed declarations: {expanded_indices:?}"
            );
            assert!(
                timeline.get_entity_id(&format!("{root}.title_text")).is_some(),
                "missing title child for {root}; labels: {labels:?}"
            );
            assert!(
                timeline.get_entity_id(&format!("{root}.frame")).is_some(),
                "missing frame child for {root}; labels: {labels:?}"
            );
            assert!(
                timeline.get_entity_id(&format!("{root}.badge")).is_some(),
                "nested component root was not specialized for {root}; labels: {labels:?}"
            );
            for mark in 0..2 {
                assert!(
                    timeline.get_entity_id(&format!("{root}.markers__{mark}")).is_some(),
                    "component-body loop did not specialize {root} marker {mark}: {labels:?}"
                );
            }
            assert_eq!(
                timeline.world.child_labels(timeline.get_entity_id(&root).unwrap()).count(),
                5,
                "each component root owns title, frame, two loop markers, and nested badge"
            );
            assert_eq!(
                text_at(&timeline, &format!("{root}.title_text"), 0),
                "same",
                "duplicate input values still produce separate components with their own content"
            );
        }
    }
    assert!(timeline.get_entity_id("featured").is_some());
    assert!(timeline.get_entity_id("featured.title_text").is_some());
    assert_eq!(timeline.world.root_entities().len(), 9);
    for index in 0..4 {
        let target = format!("cards__{index}.title_text");
        assert!(
            timeline
                .action_events
                .iter()
                .any(|event| { event.verb == "fade-in" && event.targets.contains(&target) }),
            "component fn action should specialize its child target to {target}"
        );
    }

    for index in 0..4 {
        let label = format!("cards__{index}");
        // Container entrances expand to their drawable leaves. The layout
        // wrapper stays at opacity 1 to avoid multiplying the fade twice.
        let id = timeline
            .get_entity_id(&format!("{label}.frame"))
            .expect("generated leaf exists");
        let opacity = timeline.world.style(id).opacity.as_ref().expect("fade-in opacity");
        assert_eq!(opacity.evaluate(0), 0.0, "{label} starts hidden");
        assert_eq!(opacity.evaluate(200), 1.0, "indexed container entrance resolves {label}");
    }
    assert_eq!(text_at(&timeline, "cards__1.title_text", 100), "updated");
    let frame_id = timeline.get_entity_id("cards__0.frame").unwrap();
    let title_id = timeline.get_entity_id("cards__0.title_text").unwrap();
    assert_eq!(
        timeline.world.style(frame_id).color.as_ref().unwrap().evaluate(0),
        timeline.world.style(title_id).color.as_ref().unwrap().evaluate(0),
        "sibling property expression should resolve under the concrete array instance"
    );

    let mut overrides = std::collections::HashMap::new();
    let mut env = timeline.build_frame_env_internal(250, SceneDimensions::default(), &overrides);
    for program in &timeline.modifier_programs {
        timeline
            .apply_modifier_program(
                program,
                250,
                SceneDimensions::default(),
                &mut env,
                &mut overrides,
            )
            .expect("always modifier must evaluate");
    }
    assert_eq!(
        overrides.get("cards__3").and_then(|properties| properties.get("opacity")),
        Some(&Value::Num(0.75)),
        "always assignment must resolve the indexed component root"
    );
    assert_eq!(
        overrides
            .get("cards__3.title_text")
            .and_then(|properties| properties.get("opacity")),
        Some(&Value::Num(0.5)),
        "component always assignment must resolve the child under the indexed root"
    );
}

#[test]
fn component_method_called_after_declaration_loop_rebinds_nested_targets_to_call_index() {
    let source = r#"
pub component Badge() {
  glyph: Text, text: "badge"
  badge_frame: Rect, size: (12, 12)
  fn mark(amount: Num) {
    badge_frame.opacity = amount + i * 0
  }
}
pub component Card() {
  card_frame: Rect, size: (40, 40)
  badge: Badge
}
#0s
for unused, i in {0, 1, 2} {
  cards[i]: Card
}
for amount, j in {0.2, 0.4, 0.6} {
  mark cards[j].badge [amount: amount]
}
"#;
    let mut modules = ModuleGraph::new();
    let program = modules
        .load_program_with_source(Path::new("virtual:///indexed_method.amx"), Some(source))
        .expect("load component program");
    let mut expansion_errors = Vec::new();
    let expanded = program.expand_components(&mut expansion_errors);
    assert!(expansion_errors.is_empty(), "expansion errors: {expansion_errors:?}");

    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    assert!(
        report.diagnostics.iter().all(|diagnostic| {
            diagnostic.code != crate::diagnostics::DiagnosticCode::InvalidPropertyValue
                && diagnostic.code != crate::diagnostics::DiagnosticCode::UndefinedLabel
                && diagnostic.code != crate::diagnostics::DiagnosticCode::UnknownAction
        }),
        "method expansion must not leave unresolved methods, indices or child paths: {:?}; expanded: {expanded:#?}",
        report.diagnostics,
    );
    let labels: Vec<_> =
        report.output.world.tracks.iter().map(|(label, _)| label.as_str()).collect();
    for (index, expected) in [0.2, 0.4, 0.6].into_iter().enumerate() {
        let label = format!("cards__{index}.badge.badge_frame");
        let id = report
            .output
            .get_entity_id(&label)
            .unwrap_or_else(|| {
                panic!(
                    "nested method target {label} was not built; actual labels: {labels:?}; diagnostics: {:?}",
                    report.diagnostics
                )
            });
        let opacity = report
            .output
            .world
            .style(id)
            .opacity
            .as_ref()
            .unwrap_or_else(|| panic!("method did not assign opacity on {label}"));
        assert_eq!(
            opacity.evaluate(0),
            expected,
            "wrong instance binding for {label}; diagnostics: {:?}; expanded: {expanded:#?}",
            report.diagnostics
        );
    }
}

#[test]
fn indexed_component_method_errors_keep_the_call_site_location() {
    let source = r#"
pub component Card() {
  glyph: Text
  fn mark(amount: Num) { glyph.opacity = amount }
}
for unused, i in {0, 1} {
  cards[i]: Card
}
mark cards[1.5] [amount: 0.5]
missing_method cards[0]
"#;
    let mut modules = ModuleGraph::new();
    let program = modules
        .load_program_with_source(Path::new("virtual:///indexed_method_errors.amx"), Some(source))
        .expect("load component program");
    let expanded = program.expand_components(&mut Vec::new());
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());

    let invalid_index = report
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code == crate::diagnostics::DiagnosticCode::InvalidPropertyValue
        })
        .expect("fractional method index must be diagnosed");
    assert_eq!(invalid_index.location.line, Some(9));
    let unknown_method = report
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code == crate::diagnostics::DiagnosticCode::UnknownAction)
        .expect("missing component method must be diagnosed as an unknown action");
    assert_eq!(unknown_method.location.line, Some(10));
}

#[test]
fn indexed_methods_keep_shadowing_and_rebind_self_for_constant_and_loop_targets() {
    let source = r#"
pub component Card() {
  glyph: Text
  fn from_param(i: Num) { self.opacity = i }
  fn from_local { let i = 0.6
    self.opacity = i
  }
  fn from_for {
    for item, i in {1, 2} { self.opacity = i / 10 }
  }
  fn from_dynamic(j: Num) { self.opacity = j }
  fn from_nested(j: Num) {
    from_dynamic self [j: 0.5]
    self.opacity = j
  }
}

#0s
for unused, i in {0, 1, 2} {
  cards[i]: Card
}
from_param cards[2] [i: 0.3]
#1s
from_local cards[2]
#2s
from_for cards[2]
#3s
for amount, j in {0.4, 0.5, 0.6} {
  from_dynamic cards[j] [j: amount]
}
#5s
let __animatix_index_snapshot_0 = 0.7
from_dynamic cards[2] [j: __animatix_index_snapshot_0]
#6s
from_nested cards[2] [j: 0.8]
"#;
    let mut modules = ModuleGraph::new();
    let program = modules
        .load_program_with_source(
            Path::new("virtual:///indexed_method_shadowing.amx"),
            Some(source),
        )
        .expect("load component program");
    let expanded = program.expand_components(&mut Vec::new());
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    let unexpected: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            matches!(
                diagnostic.code,
                crate::diagnostics::DiagnosticCode::InvalidPropertyValue
                    | crate::diagnostics::DiagnosticCode::UndefinedLabel
                    | crate::diagnostics::DiagnosticCode::UnknownAction
            )
        })
        .collect();
    assert!(
        unexpected.is_empty(),
        "indexed methods produced diagnostics: {unexpected:?}; expanded: {expanded:#?}"
    );

    let id = report
        .output
        .get_entity_id("cards__2")
        .expect("constant and dynamic calls must keep the indexed component root");
    let opacity = report
        .output
        .world
        .style(id)
        .opacity
        .as_ref()
        .expect("self.opacity is rebound to the method instance");
    assert_eq!(
        opacity.evaluate(0),
        0.3,
        "same-named parameter must shadow the declaration index"
    );
    assert_eq!(
        opacity.evaluate(1000),
        0.6,
        "same-named local let must shadow the declaration index"
    );
    assert!(
        (opacity.evaluate(2000) - 0.1).abs() < 1e-5,
        "same-named loop index must stay scoped to the loop body"
    );
    assert_eq!(
        opacity.evaluate(3000),
        0.6,
        "dynamic self target must bind to the matching array instance"
    );
    assert_eq!(
        opacity.evaluate(4000),
        0.6,
        "dynamic call must not leak its loop variable into later scope"
    );
    assert_eq!(
        opacity.evaluate(5000),
        0.7,
        "generated index snapshots must not shadow caller argument identifiers"
    );
    assert_eq!(
        opacity.evaluate(6000),
        0.8,
        "a nested call must restore the caller's shadowed local parameter"
    );
    for (index, expected) in [0.4, 0.5].into_iter().enumerate() {
        let id = report
            .output
            .get_entity_id(&format!("cards__{index}"))
            .expect("dynamic method array instance exists");
        assert_eq!(
            report.output.world.style(id).opacity.as_ref().unwrap().evaluate(3000),
            expected,
            "method parameter shadowing must not redirect the captured call index"
        );
    }
}

#[test]
fn indexed_multi_actor_component_methods_keep_each_group_subtree_distinct() {
    let source = r#"
pub component Card(title: Str, tint: Color) {
  root: Col, size: (240, auto), gap: 10 {
    heading: Text, text: title, color: tint
    body: Text, text: "A wrapped body child", color: green
    bar: Rect, size: (80, 6), color: blue
  }
  marker: Rect, size: (6, 6), color: yellow
  fn highlight {
    bar.color = heading.color
    always { marker.color = heading.color }
  }
}
#0s
for title, i in {"First", "Second"} {
  cards[i]: Card, title: title, tint: if i == 0 { red } else { green }
}
#1s
for unused, j in {0, 1} {
  highlight cards[j]
}
"#;
    let mut modules = ModuleGraph::new();
    let program = modules
        .load_program_with_source(
            Path::new("virtual:///indexed_multi_actor_method.amx"),
            Some(source),
        )
        .expect("load multi-actor component program");
    let expanded = program.expand_components(&mut Vec::new());
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    let structural_errors: Vec<_> = report
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.is_error()
                || matches!(
                    diagnostic.code,
                    crate::diagnostics::DiagnosticCode::DuplicateLabel
                        | crate::diagnostics::DiagnosticCode::UnknownTargetPath
                )
        })
        .collect();
    assert!(
        structural_errors.is_empty(),
        "each indexed Group must own independent descendants and resolve method paths: {structural_errors:?}; expanded: {expanded:#?}"
    );

    for index in 0..2 {
        let heading = report
            .output
            .get_entity_id(&format!("cards__{index}.heading"))
            .expect("each indexed component has its own heading");
        let bar = report
            .output
            .get_entity_id(&format!("cards__{index}.bar"))
            .expect("each indexed component has its own bar");
        let heading_color = report.output.world.style(heading).color.as_ref().unwrap();
        let bar_color = report.output.world.style(bar).color.as_ref().unwrap();
        assert_eq!(
            bar_color.evaluate(1000),
            heading_color.evaluate(1000),
            "method must resolve the sibling property inside indexed Group {index}"
        );
    }
    let first_bar = report.output.get_entity_id("cards__0.bar").unwrap();
    let second_bar = report.output.get_entity_id("cards__1.bar").unwrap();
    assert_ne!(first_bar, second_bar, "array instances must not alias the same bar entity");
    assert!(report.output.block_scope.is_empty());
    assert!(
        !report
            .output
            .variable_tracks
            .keys()
            .any(|name| name.starts_with("__animatix_index_snapshot_"))
    );
    let mut overrides = std::collections::HashMap::new();
    let dimensions = SceneDimensions::default();
    let mut env = report.output.build_frame_env_internal(1000, dimensions, &overrides);
    for program in &report.output.modifier_programs {
        report
            .output
            .apply_modifier_program(program, 1000, dimensions, &mut env, &mut overrides)
            .expect("method-local reactive bindings must retain their call index");
    }
    for index in 0..2 {
        let heading = report.output.get_entity_id(&format!("cards__{index}.heading")).unwrap();
        let color = report.output.world.style(heading).color.as_ref().unwrap().evaluate(1000);
        assert_eq!(
            overrides
                .get(&format!("cards__{index}.marker"))
                .and_then(|props| props.get("color")),
            Some(&Value::Color(color.map(f64::from)))
        );
    }
}

#[test]
fn indexed_method_reads_and_scope_survive_sequence_timing() {
    let source = r#"
pub component Card() {
  frame: Rect, size: (100, 50)
  heading: Rect, size: (10, 10), color: green
  bar: Rect, size: (10, 10), color: blue
  fn recolor {
    bar.color = heading.color [100ms]
    bar.opacity = 0.5 [200ms]
  }
}
#0s
for unused, i in {0, 1} { cards[i]: Card }
after: Rect, opacity: 1
sequence {
  recolor cards[1]
  after.opacity = 0.25 [50ms]
}
"#;
    let mut modules = ModuleGraph::new();
    let program = modules
        .load_program_with_source(Path::new("virtual:///indexed_sequence.amx"), Some(source))
        .expect("load indexed method sequence");
    let expanded = program.expand_components(&mut Vec::new());
    let report = Timeline::build_with_diagnostics(&expanded, &std::collections::HashMap::new());
    assert!(report.diagnostics.iter().all(|d| !d.is_error()), "{:?}", report.diagnostics);
    let timeline = report.output;
    let bar = timeline.get_entity_id("cards__1.bar").unwrap();
    let heading = timeline.get_entity_id("cards__1.heading").unwrap();
    let after = timeline.get_entity_id("after").unwrap();
    assert_eq!(
        timeline.world.style(bar).color.as_ref().unwrap().evaluate(100),
        timeline.world.style(heading).color.as_ref().unwrap().evaluate(100)
    );
    assert_eq!(timeline.world.style(bar).opacity.as_ref().unwrap().evaluate(300), 0.5);
    assert_eq!(timeline.world.style(after).opacity.as_ref().unwrap().evaluate(299), 1.0);
    assert_eq!(timeline.world.style(after).opacity.as_ref().unwrap().evaluate(350), 0.25);
    assert!(timeline.block_scope.is_empty());
    assert!(
        !timeline
            .variable_tracks
            .keys()
            .any(|name| name.starts_with("__animatix_index_snapshot_"))
    );
}
