//!
//! Returns the Phosphor icon glyph for any actor kind.
//! The `ActorKindMeta.icon_id` field already contains the concrete glyph string
//! (defined in the core crate's `icon_glyphs` module to avoid a GUI dependency).

use animatix::timeline::ActorKindId;

// Re-export the language-level metadata so callers don't need a second import.
pub use animatix::primitives::actor_kind_meta;

// ── Icon + Label pair ───────────────────────────────────────────────────

// ── Primary API ─────────────────────────────────────────────────────────

/// Shorthand that returns only the icon string.
pub fn actor_icon_str(kind: ActorKindId) -> &'static str {
    actor_kind_meta(kind).map(|m| m.icon_id).unwrap_or("")
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
