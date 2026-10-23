use animatix_syntax::ast::{Stmt, Transition};
use animatix_syntax::parser::parse_simple;
use animatix_syntax::to_source::stmts_to_source;

use super::super::apply::{SourceEdit, apply_edit};

fn parse(source: &str) -> Vec<Stmt> {
    parse_simple(source).0.expect("failed to parse test source")
}

#[test]
fn add_scene_appends_new_scene() {
    let mut stmts = parse("import \"foo\"\n\n# Intro\nplay Outro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::AddScene {
                name: "Outro".into()
            }
        )
        .is_ok()
    );
    assert!(
        matches!(stmts.last(), Some(Stmt::Scene { name, body, .. }) if name == "Outro" && body.is_empty())
    );
}

#[test]
fn reorder_scenes_changes_scene_order() {
    let mut stmts =
        parse("import \"foo\"\n\n# Intro\nplay Middle\n\n# Middle\nplay Outro\n\n# Outro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::ReorderScenes {
                new_order: vec!["Outro".into(), "Intro".into(), "Middle".into()]
            }
        )
        .is_ok()
    );
    let scene_names: Vec<_> = stmts
        .iter()
        .filter_map(|s| match s {
            Stmt::Scene { name, .. } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(scene_names, vec!["Outro", "Intro", "Middle"]);
    // The import is kept as a standalone Stmt::Import at the top
    assert!(matches!(stmts.first(), Some(Stmt::Import { .. })));
}

#[test]
fn set_play_target_creates_and_removes_play() {
    let mut stmts = parse("# Intro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::SetPlayTarget {
                scene: "Intro".into(),
                target: Some("Outro".into())
            }
        )
        .is_ok()
    );
    assert!(stmts_to_source(&stmts).contains("play Outro"));
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::SetPlayTarget {
                scene: "Intro".into(),
                target: None
            }
        )
        .is_ok()
    );
    assert!(!stmts_to_source(&stmts).contains("play "));
}

#[test]
fn set_transition_updates_play_statement() {
    let mut stmts = parse("# Intro\nplay Outro\n\n# Outro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::SetTransition {
                from_scene: "Intro".into(),
                transition: Some(Transition {
                    id: "fade".into(),
                    duration_ms: 300,
                    easing: animatix_syntax::easing::Easing::Linear
                }),
            }
        )
        .is_ok()
    );
    assert!(stmts_to_source(&stmts).contains("play Outro [fade, 300ms]"));
}

#[test]
fn rename_scene_updates_play_references() {
    let mut stmts = parse("# Intro\nplay Outro\n\n# Outro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::RenameScene {
                old_name: "Outro".into(),
                new_name: "Finale".into()
            }
        )
        .is_ok()
    );
    let src = stmts_to_source(&stmts);
    assert!(src.contains("# Finale"));
    assert!(src.contains("play Finale"));
    assert!(!src.contains("Outro"));
}

#[test]
fn delete_scene_removes_declaration_and_play_references() {
    let mut stmts = parse("# Intro\nplay Outro\n\n# Middle\n\n# Outro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::DeleteScene {
                name: "Outro".into()
            }
        )
        .is_ok()
    );
    let src = stmts_to_source(&stmts);
    assert!(!src.contains("# Outro"));
    assert!(!src.contains("play Outro"));
    assert!(src.contains("# Intro"));
    assert!(src.contains("# Middle"));
}

#[test]
fn delete_scene_returns_false_when_scene_missing() {
    let mut stmts = parse("# Intro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::DeleteScene {
                name: "Missing".into()
            }
        )
        .is_err()
    );
}

#[test]
fn scene_edits_fail_for_missing_or_duplicate_names() {
    let mut stmts = parse("# Intro\nplay Outro\n\n# Outro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::RenameScene {
                old_name: "Missing".into(),
                new_name: "X".into()
            }
        )
        .is_err()
    );
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::AddScene {
                name: "Intro".into()
            }
        )
        .is_err()
    );
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::ReorderScenes {
                new_order: vec!["Intro".into(), "Intro".into()]
            }
        )
        .is_err()
    );
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::SetPlayTarget {
                scene: "Missing".into(),
                target: Some("X".into())
            }
        )
        .is_err()
    );
}

#[test]
fn set_scene_duration_adds_config() {
    let mut stmts = parse("# Intro\ntitle: Text, text: \"Hello\"");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::SetSceneDuration {
                scene: "Intro".into(),
                duration_s: Some(5.0)
            }
        )
        .is_ok()
    );
    let src = stmts_to_source(&stmts);
    assert!(src.contains("duration: 5"), "Expected duration in output: {}", src);
}

#[test]
fn set_scene_duration_updates_existing_config() {
    let mut stmts = parse("# Intro\nconfig { duration: 3 }\ntitle: Text, text: \"Hello\"");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::SetSceneDuration {
                scene: "Intro".into(),
                duration_s: Some(7.5)
            }
        )
        .is_ok()
    );
    let src = stmts_to_source(&stmts);
    assert!(src.contains("duration: 7.5"), "Expected duration: 7.5 in output: {}", src);
    assert!(!src.contains("duration: 3"), "Old duration should be gone: {}", src);
}

#[test]
fn set_scene_duration_removes_when_none() {
    let mut stmts = parse("# Intro\nconfig { duration: 5 }\ntitle: Text, text: \"Hello\"");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::SetSceneDuration {
                scene: "Intro".into(),
                duration_s: None
            }
        )
        .is_ok()
    );
    let src = stmts_to_source(&stmts);
    assert!(!src.contains("duration"), "Duration should be removed: {}", src);
}

#[test]
fn set_scene_duration_fails_for_missing_scene() {
    let mut stmts = parse("# Intro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::SetSceneDuration {
                scene: "Missing".into(),
                duration_s: Some(5.0)
            }
        )
        .is_err()
    );
}

#[test]
fn extract_scene_inserts_play_into_source_scene_body() {
    let mut stmts =
        parse("# Intro\ntitle: Text, text: \"Hello\"\nbox: Rect, size: (100, 100)\n\n# Outro");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::ExtractScene {
                actor_labels: vec!["box".into()],
                new_scene_name: "Diagram".into(),
            }
        )
        .is_ok()
    );
    let src = stmts_to_source(&stmts);
    // The play statement should be inside the Intro scene, not at top level
    assert!(src.contains("play Diagram"), "Expected 'play Diagram' in output: {}", src);
    // Verify the new scene exists
    assert!(src.contains("# Diagram"), "Expected '# Diagram' in output: {}", src);
    // Verify the play is inside Intro's body (serialized before the next scene)
    let intro_pos = src.find("# Intro").unwrap();
    let diagram_pos = src.find("# Diagram").unwrap();
    let play_pos = src.find("play Diagram").unwrap();
    assert!(
        play_pos > intro_pos && play_pos < diagram_pos,
        "play Diagram should be between # Intro and # Diagram, got positions: intro={}, play={}, diagram={}",
        intro_pos,
        play_pos,
        diagram_pos
    );
}

#[test]
fn duplicate_scene_renames_labels_across_all_scenes() {
    let mut stmts =
        parse("# Intro\nbox: Rect, size: (100, 100)\n\n# Diagram\ncircle: Ellipse, size: (50, 50)");
    assert!(
        apply_edit(
            &mut stmts,
            SourceEdit::DuplicateScene {
                name: "Intro".into()
            }
        )
        .is_ok()
    );
    let src = stmts_to_source(&stmts);
    // The duplicated scene should have renamed labels that don't conflict
    // with the original 'box' or 'circle' in other scenes
    assert!(src.contains("box"), "Original box should still exist: {}", src);
    assert!(src.contains("circle"), "Original circle should still exist: {}", src);
    // The duplicated scene should have a renamed label (box_0 or similar)
    assert!(src.contains("_0"), "Duplicated label should have _0 suffix: {}", src);
}
