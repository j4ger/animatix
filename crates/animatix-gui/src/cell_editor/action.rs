use super::CellType;

/// A structural edit requested by the cell editor UI.
///
/// Actions carry the cell index they target so the caller can apply them to the
/// backing cell list. At most one action is pending at a time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellAction {
    /// Delete the cell at this index.
    Delete { index: usize },
    /// Duplicate the cell at this index, inserting the copy after it.
    Duplicate { index: usize },
    /// Insert a new keyframe after this cell index.
    InsertAfter { index: usize },
    /// Insert a new code block after this cell index.
    InsertCodeAfter { index: usize },
    /// Append a cell at the very end of the document.
    AppendAtEnd(CellType),
    /// Move the cell at this index up by one position.
    MoveUp { index: usize },
    /// Move the cell at this index down by one position.
    MoveDown { index: usize },
}

impl CellAction {
    /// Remap the target cell index after two cell positions swap.
    ///
    /// Appending has no index and is unaffected by a reorder. Remapping rather
    /// than clearing preserves the user's intent: the action follows the cell
    /// it was requested for and can never be left pointing at a stale index.
    pub(super) fn swap_indices(&mut self, a: usize, b: usize) {
        let index = match self {
            Self::Delete { index }
            | Self::Duplicate { index }
            | Self::InsertAfter { index }
            | Self::InsertCodeAfter { index }
            | Self::MoveUp { index }
            | Self::MoveDown { index } => index,
            Self::AppendAtEnd(_) => return,
        };
        if *index == a {
            *index = b;
        } else if *index == b {
            *index = a;
        }
    }
}
