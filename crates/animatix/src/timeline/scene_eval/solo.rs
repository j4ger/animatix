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
        let mut soloed: Option<std::collections::HashSet<String>> = None;
        for label in self.world.tracks.keys() {
            if !self.track_is_soloed(label, time_ms) {
                continue;
            }
            soloed.get_or_insert_with(std::collections::HashSet::new).insert(label.clone());
        }
        let Some(soloed) = soloed else {
            return crate::timeline::SoloState::default();
        };
        let mut visible = soloed.clone();
        for label in &soloed {
            if let Some(path) = self.find_path_to_actor(label) {
                // The path ends at the actor itself; ancestors are its prefix.
                visible.extend(path.into_iter().take_while(|step| step != label));
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
            || self.world.tracks.values().any(|track| self.track_declares_solo(track, 0));
        self.eval_caches.solo_scan_needed.set(Some(needed));
        needed
    }

    /// Read one actor's `solo` flag at `time_ms`; the visibility rule applies.
    pub(crate) fn track_is_soloed(&self, label: &str, time_ms: u64) -> bool {
        let Some(track) = self.world.tracks.get(label) else {
            return false;
        };
        // `visible: false` wins: an explicitly hidden actor is not soloed back
        // into view, and a hidden actor must not keep the rest of the scene
        // suppressed.
        if !self.world.meta(track.id).visible {
            return false;
        }
        self.track_declares_solo(track, time_ms)
    }

    /// The raw flag read, without the visibility rule: the scan asks whether
    /// the flag is declared at all, independent of `visible`.
    pub(crate) fn track_declares_solo(
        &self,
        track: &crate::timeline::AnimationTrack,
        time_ms: u64,
    ) -> bool {
        matches!(
            self.world.read_property_value(
                track.id,
                crate::timeline::property_registry::ActorField::Tagged("solo"),
                time_ms
            ),
            Some(crate::timeline::PropertyValue::Bool(true))
        )
    }
}
