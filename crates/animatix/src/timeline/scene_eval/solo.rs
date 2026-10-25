//! Authored solo-flag resolution.

use super::*;

impl Timeline {
    /// Resolve the authored `solo` flags for this frame.
    ///
    /// While any actor declares `solo: true`, only soloed actors draw: their
    /// ancestors stay traversable (so a soloed descendant of a non-soloed
    /// container still renders) while every other subtree is pruned whole.
    /// `visible: false` still wins over solo — an explicitly hidden actor stays
    /// hidden. Nothing soloed is the common case and allocates nothing.
    pub(crate) fn resolve_solo_state(&self, time_ms: u64) -> crate::timeline::SoloState {
        // Scenes that cannot be soloed (the overwhelming majority) skip the
        // per-track walk entirely; see `EvalCaches::solo_scan_needed`.
        if !self.solo_scan_needed() {
            return crate::timeline::SoloState::default();
        }
        let mut soloed: Option<std::collections::HashSet<EntityId>> = None;
        for (_, id) in self.world.tracks.iter() {
            // `visible: false` wins: an explicitly hidden actor is not soloed
            // back into view, and a hidden actor must not keep the rest of
            // the scene suppressed.
            if !self.world.meta(id).visible {
                continue;
            }
            if !self.track_declares_solo(id, time_ms) {
                continue;
            }
            soloed.get_or_insert_with(std::collections::HashSet::new).insert(id);
        }
        let Some(soloed) = soloed else {
            return crate::timeline::SoloState::default();
        };
        // Ancestors stay traversable (so a soloed descendant of a non-soloed
        // container still renders). The parent chain *is* the ancestor set —
        // walked by id, no label allocation or tree search.
        let mut visible = soloed.clone();
        for &id in &soloed {
            let mut ancestor = self.world.hierarchy(id).parent;
            while let Some(parent) = ancestor {
                if !visible.insert(parent) {
                    break;
                }
                ancestor = self.world.hierarchy(parent).parent;
            }
        }
        crate::timeline::SoloState {
            visible: Some(visible),
            soloed: Some(soloed),
        }
    }

    /// Derive (once per mutation) whether the solo gate needs a per-frame walk.
    ///
    /// Deriving it costs the same walk as the gate itself, so the answer is
    /// cached and reset by `invalidate_frame_cache`. Two things can make the
    /// walk necessary:
    ///
    /// - some actor declares `solo: true` (the flag is non-animatable, so this only changes with
    ///   the timeline); or
    /// - an `always` block writes `solo` on some actor, which happens per frame *without* a
    ///   mutation and would otherwise go unnoticed.
    ///
    /// The second test is exact rather than a blanket "has modifiers" flag:
    /// timelines that merely animate other properties (the common case, and
    /// what the actor-count benchmarks measure) still take the fast path.
    pub(crate) fn solo_scan_needed(&self) -> bool {
        if let Some(needed) = self.eval_caches.solo_scan_needed.get() {
            return needed;
        }
        let always_writes_solo = self.modifiers.iter().any(|stmt| {
            matches!(
                stmt,
                crate::ast::Stmt::Assignment { property, .. } if property == "solo"
            )
        });
        let needed = always_writes_solo
            || self.world.tracks.iter().any(|(_, id)| self.track_declares_solo(id, 0));
        self.eval_caches.solo_scan_needed.set(Some(needed));
        needed
    }

    /// The raw flag read, without the visibility rule: the scan asks whether
    /// the flag is declared at all, independent of `visible`.
    pub(crate) fn track_declares_solo(&self, id: EntityId, time_ms: u64) -> bool {
        matches!(
            self.world.read_property_value(
                id,
                crate::timeline::property_registry::ActorField::Tagged("solo"),
                time_ms
            ),
            Some(crate::timeline::PropertyValue::Bool(true))
        )
    }
}
