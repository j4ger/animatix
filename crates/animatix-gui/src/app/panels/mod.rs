
pub mod behavior;
pub mod inspector;
pub mod timeline_panel;

pub mod sidebar;
pub mod editor;
pub mod inspector_panel;
pub mod timeline;
pub mod preview_panel;

pub use crate::app::commands::{PropertyEdit, PropertyValue};
use animatix::primitives;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SidebarTab {
    Explorer,
    Layers,
}

/// Returns the canonical default actor type: the first non-advanced Shape actor.
pub(crate) fn default_actor_type() -> &'static str {
    primitives::actor_kind_registry()
        .iter()
        .find(|meta| {
            meta.category == animatix::timeline::ActorCategory::Shape && !meta.advanced
        })
        .map(|meta| meta.type_name)
        .unwrap_or("Rect")
}

/// Compute a "nice" tick interval for ruler marks.
/// Produces round numbers (1, 2, 5, 10, 20, 50, 100, ...).
pub(super) fn nice_tick_interval(visible_range: f32, target_ticks: f32) -> f32 {
    let raw = (visible_range / target_ticks).abs();
    if raw <= 0.0 {
        return 1.0;
    }
    let magnitude = 10.0_f32.powf(raw.log10().floor());
    let normalized = raw / magnitude;
    let nice_mul = if normalized < 1.5 {
        1.0
    } else if normalized < 3.5 {
        2.0
    } else if normalized < 7.5 {
        5.0
    } else {
        10.0
    };
    nice_mul * magnitude
}

pub(super) const RULER_SIZE: f32 = 20.0;

/// Uniform panel frame: 8 px padding, transparent fill.
pub(super) fn panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::TRANSPARENT)
        .inner_margin(egui::Margin::same(8))
}

#[cfg(test)]
mod tests {
    use super::nice_tick_interval;

    #[test]
    fn nice_tick_interval_normal_range() {
        // visible_range=100.0, target_ticks=10 → raw=10 → magnitude=10 → normalized=1 → nice_mul=1 → 10.0
        let interval = nice_tick_interval(100.0, 10.0);
        assert!((interval - 10.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_rounds_to_two() {
        // visible_range=50.0, target_ticks=10 → raw=5 → magnitude=1 → normalized=5 → nice_mul=5 → 5.0
        let interval = nice_tick_interval(50.0, 10.0);
        assert!((interval - 5.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_small_values() {
        // visible_range=0.5, target_ticks=10 → raw=0.05 → magnitude=0.01 → normalized=5 → nice_mul=5 → 0.05
        let interval = nice_tick_interval(0.5, 10.0);
        assert!(interval > 0.0);
        assert!((interval / 0.01 - 5.0).abs() < 0.001 || (interval / 0.05 - 1.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_zero_range() {
        // raw=0.0 → early return 1.0
        assert_eq!(nice_tick_interval(0.0, 10.0), 1.0);
    }

    #[test]
    fn nice_tick_interval_negative_range() {
        // abs(visible_range) used
        assert_eq!(nice_tick_interval(-100.0, 10.0), 10.0);
    }

    #[test]
    fn nice_tick_interval_large_range_gives_round_numbers() {
        // visible_range=10000.0, target_ticks=10 → raw=1000 → magnitude=100 → normalized=10 → nice_mul=10 → 1000.0
        let interval = nice_tick_interval(10000.0, 10.0);
        assert!((interval - 1000.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_always_positive() {
        for &range in &[0.1, 1.0, 10.0, 100.0, 1000.0] {
            let interval = nice_tick_interval(range, 10.0);
            assert!(interval > 0.0, "interval must be positive for range={}", range);
        }
    }

    #[test]
    fn nice_tick_interval_boundary_near_one_point_five() {
        // raw just below 1.5 → nice_mul=1
        let interval = nice_tick_interval(14.9, 10.0);
        assert!((interval - 1.0).abs() < 0.001);

        // raw just above 1.5 → nice_mul=2
        let interval = nice_tick_interval(15.1, 10.0);
        // raw=1.51 → magnitude=1 → normalized=1.51 → nice_mul=2 → 2.0
        assert!((interval - 2.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_boundary_near_three_point_five() {
        // raw just below 3.5 → nice_mul=2
        let interval = nice_tick_interval(34.9, 10.0);
        assert!((interval - 2.0).abs() < 0.001);

        // raw just above 3.5 → nice_mul=5
        let interval = nice_tick_interval(35.1, 10.0);
        assert!((interval - 5.0).abs() < 0.001);
    }

    #[test]
    fn nice_tick_interval_boundary_near_seven_point_five() {
        // raw just below 7.5 → nice_mul=5
        let interval = nice_tick_interval(74.9, 10.0);
        assert!((interval - 5.0).abs() < 0.001);

        // raw just above 7.5 → nice_mul=10
        let interval = nice_tick_interval(75.1, 10.0);
        assert!((interval - 10.0).abs() < 0.001);
    }
}
