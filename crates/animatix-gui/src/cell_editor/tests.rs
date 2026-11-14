use super::{CellAction, CellEditorState, CellType};

#[test]
fn swap_cell_indices_remaps_every_indexed_pending_action() {
    let cases = [
        (CellAction::Delete { index: 0 }, CellAction::Delete { index: 1 }),
        (CellAction::Duplicate { index: 0 }, CellAction::Duplicate { index: 1 }),
        (CellAction::InsertAfter { index: 0 }, CellAction::InsertAfter { index: 1 }),
        (
            CellAction::InsertCodeAfter { index: 0 },
            CellAction::InsertCodeAfter { index: 1 },
        ),
        (CellAction::MoveUp { index: 0 }, CellAction::MoveUp { index: 1 }),
        (CellAction::MoveDown { index: 0 }, CellAction::MoveDown { index: 1 }),
    ];

    for (action, expected) in cases {
        let mut state = CellEditorState {
            pending_action: Some(action),
            ..Default::default()
        };
        state.swap_cell_indices(0, 1);
        assert_eq!(
            state.pending_action,
            Some(expected),
            "a pending action must follow its cell across a reorder, never stay stale"
        );
    }
}

#[test]
fn swap_cell_indices_leaves_append_action_unchanged() {
    let action = CellAction::AppendAtEnd(CellType::Code);
    let mut state = CellEditorState {
        pending_action: Some(action),
        ..Default::default()
    };
    state.swap_cell_indices(2, 5);
    assert_eq!(state.pending_action, Some(action));
}

#[test]
fn swap_cell_indices_keeps_unrelated_pending_action_target() {
    let mut state = CellEditorState {
        pending_action: Some(CellAction::Delete { index: 2 }),
        ..Default::default()
    };
    state.swap_cell_indices(0, 1);
    assert_eq!(state.pending_action, Some(CellAction::Delete { index: 2 }));
}

#[test]
fn speech_anchor_headers_keep_raw_lexemes_and_are_not_zero_seconds() {
    let source = "#voice@intro:\n    box.opacity = 1\n#voice[\"DSL\"]:\n    box.scale = 2\n";
    let mut cells = super::parse_cells(source);
    assert_eq!(cells[0].display_timestamp().as_deref(), Some("voice@intro:"));
    assert_eq!(cells[1].display_timestamp().as_deref(), Some("voice[\"DSL\"]:"));
    assert_eq!(cells[0].time_s(), None);
    assert_eq!(cells[1].time_s(), None);
    cells[0].set_body("    box.opacity = 0.5".into());
    let edited = super::cells_to_source(&cells);
    assert!(edited.contains("#voice@intro:"));
    assert!(edited.contains("#voice[\"DSL\"]:"));
    assert!(!edited.contains("#0s"));
}
