//!
//! Returns the Phosphor icon glyph for any actor kind.
//! The `ActorKindMeta.icon_id` field already contains the concrete glyph string
//! (defined in the core crate's `icon_glyphs` module to avoid a GUI dependency).

// Re-export the language-level metadata so callers don't need a second import.
pub use animatix::primitives::actor_kind_meta;
use animatix::timeline::{ActorKindId, AnimationTrack, Timeline};

// ── Icon + Label pair ───────────────────────────────────────────────────

// ── Primary API ─────────────────────────────────────────────────────────

/// Shorthand that returns only the icon string.
///
/// `ActorKindId::Extension` has no static metadata; a generic extension glyph
/// is returned instead of an empty string (the plugin's own `icon_id()` is
/// only reachable through the live registry — see [`actor_icon_for_track`]).
pub fn actor_icon_str(kind: ActorKindId) -> &'static str {
    match kind {
        ActorKindId::Extension => egui_phosphor::regular::PUZZLE_PIECE,
        _ => actor_kind_meta(kind).map(|m| m.icon_id).unwrap_or(""),
    }
}

/// Icon for a track, resolved from the live registry via its required
/// `actor_type` (so extension primitives participate); falls back to the static
/// kind metadata only when the primitive is not registered.
pub fn actor_icon_for_track(track: &AnimationTrack, timeline: &Timeline) -> String {
    timeline
        .primitive_registry_snapshot()
        .find(&track.actor_type)
        .map(|primitive| primitive.icon_id().to_string())
        .unwrap_or_else(|| actor_icon_str(track.kind).to_string())
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
                "ActorKind {:?} has unmapped icon_id: {:?}",
                meta.kind,
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
