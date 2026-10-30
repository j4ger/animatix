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
