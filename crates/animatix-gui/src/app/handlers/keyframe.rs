use crate::app::commands::{Effect, UndoLabel};
use crate::app::document::timeline_diff::KeyframeId;
use crate::app::document_controller::DocumentController;
use crate::app::stores::{DocumentStore, PreviewStore, UiStore};

fn begin_snapshot(
    document_store: &mut DocumentStore,
    preview_store: &PreviewStore,
    ui_store: &UiStore,
    label: UndoLabel,
) {
    document_store.snapshot(label, ui_store.snapshot_with_preview(preview_store));
}

pub fn handle_set_keyframe_easing(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    scene: Option<String>,
    actor: String,
    property: String,
    time_s: f64,
    easing: animatix_syntax::easing::Easing,
) -> Vec<Effect> {
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        UndoLabel::SetKeyframeEasing {
            scene: scene.clone(),
            actor: actor.clone(),
            property: property.clone(),
            time_s,
            easing,
        },
    );
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_set_keyframe_easing(scene, &actor, &property, time_s, easing);
    vec![]
}

pub fn handle_delete_keyframe(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    scene: Option<String>,
    actor: String,
    property: String,
    time_s: f64,
) -> Vec<Effect> {
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        UndoLabel::DeleteKeyframe {
            scene: scene.clone(),
            actor: actor.clone(),
            property: property.clone(),
            time_s,
        },
    );
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_delete_keyframe(scene, &actor, &property, time_s);
    ctrl.prune_stale_keyframe_selections();
    vec![]
}

/// Replace a keyframe's value in place with a single undo snapshot.
pub fn handle_set_keyframe_value(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    scene: Option<String>,
    actor: String,
    property: String,
    time_s: f64,
    value: crate::app::commands::PropertyValue,
) -> Vec<Effect> {
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        UndoLabel::SetKeyframeValue {
            scene: scene.clone(),
            actor: actor.clone(),
            property: property.clone(),
            time_s,
        },
    );
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_set_keyframe_value(scene, &actor, &property, time_s, value);
    vec![]
}

pub fn handle_move_keyframe(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    scene: Option<String>,
    actor: String,
    property: String,
    old_time_s: f64,
    new_time_s: f64,
) -> Vec<Effect> {
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        UndoLabel::MoveKeyframe {
            scene: scene.clone(),
            actor: actor.clone(),
            property: property.clone(),
            old_time_s,
            new_time_s,
        },
    );
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_move_keyframes(&[crate::app::commands::MoveKeyframeSpec {
        scene,
        actor,
        property,
        old_time_s,
        new_time_s,
    }]);
    ctrl.prune_stale_keyframe_selections();
    vec![]
}

/// Move several keyframes with a single undo snapshot.
pub fn handle_move_keyframes(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    specs: Vec<crate::app::commands::MoveKeyframeSpec>,
) -> Vec<Effect> {
    if specs.is_empty() {
        return vec![];
    }
    begin_snapshot(document_store, preview_store, ui_store, UndoLabel::MoveKeyframes);
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_move_keyframes(&specs);
    ctrl.prune_stale_keyframe_selections();
    vec![]
}

/// Delete several keyframes with a single undo snapshot.
pub fn handle_delete_keyframes(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    ids: Vec<KeyframeId>,
) -> Vec<Effect> {
    if ids.is_empty() {
        return vec![];
    }
    begin_snapshot(document_store, preview_store, ui_store, UndoLabel::DeleteKeyframes);
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_delete_keyframes(&ids);
    ctrl.prune_stale_keyframe_selections();
    vec![]
}

pub fn handle_resize_action(
    document_store: &mut DocumentStore,
    preview_store: &mut PreviewStore,
    ui_store: &mut UiStore,
    verb: String,
    targets: Vec<String>,
    old_start_s: f64,
    new_start_s: f64,
    new_duration_s: f64,
) -> Vec<Effect> {
    begin_snapshot(
        document_store,
        preview_store,
        ui_store,
        UndoLabel::ResizeAction {
            verb: verb.clone(),
            targets: targets.clone(),
            old_start_s,
            new_start_s,
            new_duration_s,
        },
    );
    let mut ctrl = DocumentController {
        document_store,
        preview_store,
        ui_store,
    };
    ctrl.handle_resize_action(&verb, &targets, old_start_s, new_start_s, new_duration_s);
    vec![]
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::app::PreviewPaneState;
    use crate::app::document::timeline_diff::KeyframeId;
    use crate::app::stores::UiStore;

    fn make_document_store(source: &str) -> DocumentStore {
        let path = PathBuf::from("test.amx");
        let mut document =
            crate::document::DocumentSession::from_source(path.clone(), source.to_string())
                .expect("valid source");
        document.rebuild().expect("valid source should rebuild");
        let editor = crate::editor::EditorBuffer::new(&path, document.source_text.clone());
        DocumentStore::new(document, editor)
    }

    fn preview_store(dimensions: animatix::timeline::SceneDimensions) -> PreviewStore {
        PreviewStore::new(PreviewPaneState::new(5.0, dimensions))
    }

    #[test]
    fn set_keyframe_value_replaces_value_with_one_undo_snapshot() {
        let mut document_store =
            make_document_store("#0s\nbox: Rect, size: (100, 100)\n#1s\nbox.opacity = 0.5\n");
        let mut preview_store = preview_store(document_store.source.document.scene_dimensions);
        let mut ui_store = UiStore::new(crate::app::persistence::default_tree());

        let effects = handle_set_keyframe_value(
            &mut document_store,
            &mut preview_store,
            &mut ui_store,
            None,
            "box".to_string(),
            "opacity".to_string(),
            1.0,
            crate::app::commands::PropertyValue::F32(0.8),
        );

        assert!(effects.is_empty());
        assert_eq!(document_store.history.undo_stack.len(), 1);
        let source = document_store.source.text();
        assert!(source.contains("0.8"), "new value written to source: {source}");
        assert!(source.contains("#1s"), "keyframe time must not move: {source}");
    }

    #[test]
    fn set_keyframe_value_missing_keyframe_aborts_snapshot() {
        let mut document_store =
            make_document_store("#0s\nbox: Rect, size: (100, 100)\n#1s\nbox.opacity = 0.5\n");
        let mut preview_store = preview_store(document_store.source.document.scene_dimensions);
        let mut ui_store = UiStore::new(crate::app::persistence::default_tree());

        let _ = handle_set_keyframe_value(
            &mut document_store,
            &mut preview_store,
            &mut ui_store,
            None,
            "box".to_string(),
            "opacity".to_string(),
            // No keyframe here; the exact-time edit must not fall back to the
            // 1s block via the property merge window.
            1.02,
            crate::app::commands::PropertyValue::F32(0.8),
        );

        assert!(
            document_store.history.undo_stack.is_empty(),
            "a failed edit must not leave an undo entry"
        );
        assert!(document_store.source.text().contains("0.5"));
    }

    #[test]
    fn batch_move_uses_one_undo_snapshot_and_moves_every_spec() {
        let mut document_store = make_document_store(
            "#0s\nbox: Rect, size: (100, 100)\n#1s\nbox.opacity = 0.5\nbox.rotation = 1\n",
        );
        let mut preview_store = preview_store(document_store.source.document.scene_dimensions);
        let mut ui_store = UiStore::new(crate::app::persistence::default_tree());

        let effects = handle_move_keyframes(
            &mut document_store,
            &mut preview_store,
            &mut ui_store,
            vec![
                crate::app::commands::MoveKeyframeSpec {
                    scene: None,
                    actor: "box".to_string(),
                    property: "opacity".to_string(),
                    old_time_s: 1.0,
                    new_time_s: 2.0,
                },
                crate::app::commands::MoveKeyframeSpec {
                    scene: None,
                    actor: "box".to_string(),
                    property: "rotation".to_string(),
                    old_time_s: 1.0,
                    new_time_s: 2.0,
                },
            ],
        );

        assert!(effects.is_empty());
        // Both moves share a single undo entry.
        assert_eq!(document_store.history.undo_stack.len(), 1);
        let source = document_store.source.text();
        assert!(
            source.contains("2s") || source.contains("2000ms"),
            "moved time in source: {source}"
        );
        assert!(!source.contains("#1s"), "old keyframe block should be gone: {source}");
        assert!(source.contains("box.opacity"));
        assert!(source.contains("box.rotation"));
    }

    #[test]
    fn batch_move_moves_separate_blocks_independently() {
        let mut document_store = make_document_store(
            "#0s\nbox: Rect, size: (100, 100)\n#1s\nbox.opacity = 0.5\n#2s\nbox.rotation = 1\n",
        );
        let mut preview_store = preview_store(document_store.source.document.scene_dimensions);
        let mut ui_store = UiStore::new(crate::app::persistence::default_tree());

        let effects = handle_move_keyframes(
            &mut document_store,
            &mut preview_store,
            &mut ui_store,
            vec![
                crate::app::commands::MoveKeyframeSpec {
                    scene: None,
                    actor: "box".to_string(),
                    property: "opacity".to_string(),
                    old_time_s: 1.0,
                    new_time_s: 2.0,
                },
                crate::app::commands::MoveKeyframeSpec {
                    scene: None,
                    actor: "box".to_string(),
                    property: "rotation".to_string(),
                    old_time_s: 2.0,
                    new_time_s: 3.0,
                },
            ],
        );

        assert!(effects.is_empty());
        assert_eq!(document_store.history.undo_stack.len(), 1);
        let source = document_store.source.text();
        // Each keyframe block moved by its own delta.
        assert!(source.contains("#3s"), "rotation should land at 3s: {source}");
        assert!(source.contains("#2s"), "opacity should land at 2s: {source}");
        assert!(!source.contains("#1s"), "nothing should remain at 1s: {source}");
    }

    #[test]
    fn batch_move_with_empty_specs_does_not_snapshot() {
        let mut document_store = make_document_store("#0s\nbox: Rect, size: (100, 100)\n");
        let mut preview_store = preview_store(document_store.source.document.scene_dimensions);
        let mut ui_store = UiStore::new(crate::app::persistence::default_tree());

        let effects =
            handle_move_keyframes(&mut document_store, &mut preview_store, &mut ui_store, vec![]);

        assert!(effects.is_empty());
        assert!(document_store.history.undo_stack.is_empty());
    }

    #[test]
    fn batch_delete_uses_one_undo_snapshot() {
        let mut document_store = make_document_store(
            "#0s\nbox: Rect, size: (100, 100)\n#1s\nbox.opacity = 0.5\nbox.rotation = 1\n",
        );
        let mut preview_store = preview_store(document_store.source.document.scene_dimensions);
        let mut ui_store = UiStore::new(crate::app::persistence::default_tree());

        let ids = vec![
            KeyframeId {
                scene: None,
                actor: "box".to_string(),
                property: "opacity".to_string(),
                time_ms: 1000,
            },
            KeyframeId {
                scene: None,
                actor: "box".to_string(),
                property: "rotation".to_string(),
                time_ms: 1000,
            },
        ];
        let effects =
            handle_delete_keyframes(&mut document_store, &mut preview_store, &mut ui_store, ids);

        assert!(effects.is_empty());
        assert_eq!(document_store.history.undo_stack.len(), 1);
        let source = document_store.source.text();
        assert!(!source.contains("box.opacity"), "opacity deletion in source: {source}");
        assert!(!source.contains("box.rotation"), "rotation deletion in source: {source}");
    }

    #[test]
    fn prune_drops_wrong_property_selection() {
        let mut document_store = make_document_store(
            "# A\n#0s\nbox: Rect, size: (100, 100)\n# B\n#0s\nbox: Rect, size: (100, 100)\n#2s\nbox.color = red\n",
        );
        let mut preview_store = preview_store(document_store.source.document.scene_dimensions);
        let mut ui_store = UiStore::new(crate::app::persistence::default_tree());
        ui_store.selection.selected_keyframes.push(KeyframeId {
            scene: Some("B".to_string()),
            actor: "box".to_string(),
            property: "color".to_string(),
            time_ms: 2000,
        });

        // A color keyframe exists in B at 2s; retaining it is valid.
        let mut ctrl = DocumentController {
            document_store: &mut document_store,
            preview_store: &mut preview_store,
            ui_store: &mut ui_store,
        };
        ctrl.prune_stale_keyframe_selections();
        assert_eq!(ui_store.selection.selected_keyframes.len(), 1);

        // The same actor/time with a non-existent property must be dropped,
        // even when another property still has a keyframe at that time.
        ui_store.selection.selected_keyframes.clear();
        ui_store.selection.selected_keyframes.push(KeyframeId {
            scene: Some("B".to_string()),
            actor: "box".to_string(),
            property: "position".to_string(),
            time_ms: 2000,
        });
        let mut ctrl = DocumentController {
            document_store: &mut document_store,
            preview_store: &mut preview_store,
            ui_store: &mut ui_store,
        };
        ctrl.prune_stale_keyframe_selections();
        assert!(ui_store.selection.selected_keyframes.is_empty());
    }

    #[test]
    fn prune_drops_missing_scene_selection() {
        let mut document_store = make_document_store("# A\n#0s\nbox: Rect, size: (100, 100)\n");
        let mut preview_store = preview_store(document_store.source.document.scene_dimensions);
        let mut ui_store = UiStore::new(crate::app::persistence::default_tree());
        ui_store.selection.selected_keyframes.push(KeyframeId {
            scene: Some("B".to_string()),
            actor: "box".to_string(),
            property: "size".to_string(),
            time_ms: 0,
        });

        let mut ctrl = DocumentController {
            document_store: &mut document_store,
            preview_store: &mut preview_store,
            ui_store: &mut ui_store,
        };
        ctrl.prune_stale_keyframe_selections();
        assert!(ui_store.selection.selected_keyframes.is_empty());
    }
}
