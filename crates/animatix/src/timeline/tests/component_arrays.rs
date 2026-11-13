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
