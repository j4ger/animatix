//! `SoloState`: per-frame resolution of the authored `solo` flags.

use super::SoloState;
use crate::timeline::EntityId;

impl SoloState {
    pub(crate) fn is_active(&self) -> bool {
        self.visible.is_some()
    }

    /// `true` when `id` is a soloed actor itself.
    pub(crate) fn is_soloed(&self, id: EntityId) -> bool {
        self.soloed.as_ref().is_some_and(|set| set.contains(&id))
    }

    /// `true` when `id` is a soloed actor or an ancestor of one.
    pub(crate) fn is_reachable(&self, id: EntityId) -> bool {
        self.visible.as_ref().is_some_and(|set| set.contains(&id))
    }
}
