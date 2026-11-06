//! Accessors for the flattened dropdown option rows.

use super::FlatOption;

impl FlatOption {
    pub(super) fn index(&self) -> Option<usize> {
        match self {
            Self::Item { index, .. } => Some(*index),
            _ => None,
        }
    }

    pub(super) fn label(&self) -> Option<&str> {
        match self {
            Self::Item { label, .. } => Some(label.as_str()),
            _ => None,
        }
    }
}
