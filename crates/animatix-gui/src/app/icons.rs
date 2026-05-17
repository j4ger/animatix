//! Centralized icon mapping for Animatix actor types.
//!
//! Bridges the language-level `ActorKindMeta` registry (defined in the
//! `animatix` crate) to concrete egui_phosphor icons.  All UI code
//! should call `actor_icon()` rather than maintaining its own mapping.

#![allow(dead_code)]

use animatix::timeline::{ActorCategory, ActorKindId, ActorKindMeta, ShapeType};

// Re-export the language-level metadata so callers don't need a second import.
pub use animatix::primitives::{actor_kind_meta, actor_kind_registry};

// ── Icon + Label pair ───────────────────────────────────────────────────

/// Concrete icon string + human label for use in egui widgets.
pub struct ActorIcon {
    pub icon: &'static str,
    pub label: &'static str,
}

// ── Primary API ─────────────────────────────────────────────────────────

/// Get the Phosphor icon and label for any actor kind.
pub fn actor_icon(kind: ActorKindId) -> ActorIcon {
    let meta = actor_kind_meta(kind);
    ActorIcon {
        icon: phosphor_icon(meta.icon_id),
        label: meta.display_name,
    }
}

/// Shorthand that returns only the icon string.
pub fn actor_icon_str(kind: ActorKindId) -> &'static str {
    phosphor_icon(actor_kind_meta(kind).icon_id)
}

/// Get the category label for an actor kind.
pub fn actor_category(kind: ActorKindId) -> ActorCategory {
    actor_kind_meta(kind).category
}

/// Get the type name string for an actor kind (e.g. "Rect", "Text").
pub fn actor_type_name(kind: ActorKindId) -> &'static str {
    actor_kind_meta(kind).type_name
}

// ── Legacy bridge: ShapeType ────────────────────────────────────────────

/// Get the icon for a `ShapeType` (rendering-level property track).
/// Prefer `actor_icon()` for new code.
pub fn shape_type_icon(shape: ShapeType) -> &'static str {
    match shape {
        ShapeType::Rect => egui_phosphor::regular::SQUARE,
        ShapeType::Ellipse => egui_phosphor::regular::CIRCLE,
        ShapeType::Line => egui_phosphor::regular::MINUS,
        ShapeType::Polygon => egui_phosphor::regular::POLYGON,
        ShapeType::Path => egui_phosphor::regular::PEN,
        ShapeType::Graph => egui_phosphor::regular::CHART_BAR,
        ShapeType::Plot => egui_phosphor::regular::CHART_LINE_UP,
    }
}

// ── Palette helpers ─────────────────────────────────────────────────────

/// All creatable actor types for the toolbar palette.
///
/// Derived directly from `actor_kind_registry()` — no hardcoded indices.
/// `meta.advanced == true` items appear in a submenu.
pub fn actor_palette() -> &'static [ActorKindMeta] {
    actor_kind_registry()
}

// ── Internal: icon_id → Phosphor constant ──────────────────────────────

/// Map an opaque `icon_id` string from `ActorKindMeta` to a concrete
/// egui_phosphor icon constant.
fn phosphor_icon(id: &str) -> &'static str {
    match id {
        "square" => egui_phosphor::regular::SQUARE,
        "circle" => egui_phosphor::regular::CIRCLE,
        "circle-notch" => egui_phosphor::regular::CIRCLE_NOTCH,
        "minus" => egui_phosphor::regular::MINUS,
        "arrows-clockwise" => egui_phosphor::regular::ARROWS_CLOCKWISE,
        "polygon" => egui_phosphor::regular::POLYGON,
        "hexagon" => egui_phosphor::regular::HEXAGON,
        "pen" => egui_phosphor::regular::PEN,
        "arrow-right" => egui_phosphor::regular::ARROW_RIGHT,
        "dot" => egui_phosphor::regular::DOT,
        "text-t" => egui_phosphor::regular::TEXT_T,
        "function" => egui_phosphor::regular::FUNCTION,
        "code" => egui_phosphor::regular::CODE,
        "image" => egui_phosphor::regular::IMAGE,
        "vector-three" => egui_phosphor::regular::VECTOR_THREE,
        "chart-bar" => egui_phosphor::regular::CHART_BAR,
        "chart-line-up" => egui_phosphor::regular::CHART_LINE_UP,
        "chart-polar" => egui_phosphor::regular::CHART_POLAR,
        "chart-scatter" => egui_phosphor::regular::CHART_SCATTER,
        "chart-donut" => egui_phosphor::regular::CHART_DONUT,
        "rows" => egui_phosphor::regular::ROWS,
        "columns" => egui_phosphor::regular::COLUMNS,
        "squares-four" => egui_phosphor::regular::SQUARES_FOUR,
        "stack" => egui_phosphor::regular::STACK,
        "folder" => egui_phosphor::regular::FOLDER,
        _ => egui_phosphor::regular::QUESTION,
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Every ActorKindMeta entry must have a valid icon mapping.
    /// A valid mapping is anything other than the QUESTION fallback.
    #[test]
    fn all_actor_kinds_have_icons() {
        for meta in actor_kind_registry().iter() {
            let icon = phosphor_icon(meta.icon_id);
            assert_ne!(
                icon, egui_phosphor::regular::QUESTION,
                "ActorKind {:?} has unmapped icon_id: {:?}",
                meta.kind, meta.icon_id
            );
        }
    }

    /// The palette must contain at least one basic and one advanced item.
    #[test]
    fn actor_palette_has_basic_and_advanced() {
        let basic_count = actor_kind_registry().iter().filter(|m| !m.advanced).count();
        let advanced_count = actor_kind_registry().iter().filter(|m| m.advanced).count();
        assert!(basic_count > 0, "Palette has no basic items");
        assert!(advanced_count > 0, "Palette has no advanced items");
    }

    /// ShapeType bridge: all variants must have a non-QUESTION icon.
    #[test]
    fn all_shape_types_have_icons() {
        use animatix::timeline::ShapeType;
        for variant in [
            ShapeType::Rect,
            ShapeType::Ellipse,
            ShapeType::Line,
            ShapeType::Polygon,
            ShapeType::Path,
            ShapeType::Graph,
            ShapeType::Plot,
        ] {
            let icon = shape_type_icon(variant);
            assert_ne!(
                icon, egui_phosphor::regular::QUESTION,
                "ShapeType::{:?} maps to QUESTION fallback", variant
            );
        }
    }
}
