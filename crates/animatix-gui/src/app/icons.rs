//!
//! Returns the Phosphor icon glyph for any actor kind.
//! The `PrimitiveInfo.icon_id` field already contains the concrete glyph string
//! (defined in the core crate's `icon_glyphs` module to avoid a GUI dependency).

use animatix::timeline::{AnimationTrack, Timeline};

// ── Icon + Label pair ───────────────────────────────────────────────────

// ── Primary API ─────────────────────────────────────────────────────────

/// Icon for a track, resolved from the live registry via its required
/// `actor_type` (so extension primitives participate); unregistered type
/// names get a generic extension glyph.
pub fn actor_icon_for_track(
    track: &AnimationTrack,
    timeline: &Timeline,
) -> std::borrow::Cow<'static, str> {
    let snapshot = timeline.primitive_registry_snapshot();
    match snapshot.info_of(&track.actor_type) {
        // Built-in rows keep their literal; extension icons are host-owned.
        Some(info) => match info.static_icon_id() {
            Some(icon) => std::borrow::Cow::Borrowed(icon),
            None => std::borrow::Cow::Owned(info.icon_id.to_string()),
        },
        None => std::borrow::Cow::Borrowed(egui_phosphor::regular::PUZZLE_PIECE),
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use animatix::primitives::primitive_catalog;

    /// Every catalog row must have a valid icon glyph.
    /// A valid glyph is anything other than the QUESTION fallback.
    #[test]
    fn all_actor_kinds_have_icons() {
        for meta in primitive_catalog().iter() {
            assert_ne!(
                meta.icon_id,
                egui_phosphor::regular::QUESTION,
                "Catalog row {} has unmapped icon_id: {:?}",
                meta.type_name,
                meta.icon_id
            );
        }
    }

    /// The registry must contain at least one basic and one advanced item.
    #[test]
    fn actor_registry_has_basic_and_advanced() {
        let basic_count = primitive_catalog().iter().filter(|m| !m.advanced).count();
        let advanced_count = primitive_catalog().iter().filter(|m| m.advanced).count();
        assert!(basic_count > 0, "Registry has no basic items");
        assert!(advanced_count > 0, "Registry has no advanced items");
    }
}
