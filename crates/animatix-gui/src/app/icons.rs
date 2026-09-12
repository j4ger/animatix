//!
//! Returns the Phosphor icon glyph for any actor kind.
//! The `ActorKindMeta.icon_id` field already contains the concrete glyph string
//! (defined in the core crate's `icon_glyphs` module to avoid a GUI dependency).

use animatix::timeline::{AnimationTrack, Timeline};

// ── Icon + Label pair ───────────────────────────────────────────────────

// ── Primary API ─────────────────────────────────────────────────────────

/// Icon for a track, resolved from the live registry via its required
/// `actor_type` (so extension primitives participate); unregistered type
/// names get a generic extension glyph.
pub fn actor_icon_for_track(track: &AnimationTrack, timeline: &Timeline) -> &'static str {
    timeline
        .primitive_registry_snapshot()
        .find(&track.actor_type)
        .map(|primitive| primitive.icon_id())
        .unwrap_or(egui_phosphor::regular::PUZZLE_PIECE)
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use animatix::primitives::actor_kind_registry;

    /// Every ActorKindMeta entry must have a valid icon glyph.
    /// A valid glyph is anything other than the QUESTION fallback.
    #[test]
    fn all_actor_kinds_have_icons() {
        for meta in actor_kind_registry().iter() {
            assert_ne!(
                meta.icon_id,
                egui_phosphor::regular::QUESTION,
                "ActorKind {} has unmapped icon_id: {:?}",
                meta.type_name,
                meta.icon_id
            );
        }
    }

    /// The registry must contain at least one basic and one advanced item.
    #[test]
    fn actor_registry_has_basic_and_advanced() {
        let basic_count = actor_kind_registry().iter().filter(|m| !m.advanced).count();
        let advanced_count = actor_kind_registry().iter().filter(|m| m.advanced).count();
        assert!(basic_count > 0, "Registry has no basic items");
        assert!(advanced_count > 0, "Registry has no advanced items");
    }
}
