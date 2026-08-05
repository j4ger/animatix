//! Spatial tokens — spacing, row heights, stroke widths, radii,
//! and domain-specific layout dimensions.
//!
//! Generic spatial constants (`SPACE_*`, `ROW_*`, `RADIUS_*`, `STROKE_*`,
//! `component`) are individually re-exported from
//! `eparts::tokens::spatial`. App-specific submodules (`preview`, `timeline`,
//! `inspector`, `toolbar`, `welcome`, `dialog`) are defined inline
//! below.
//!
//! Every item — generic and app-specific — is at the same module depth so
//! that existing paths like `design_tokens::spatial::SPACE_4` and
//! `design_tokens::spatial::preview::HANDLE_SIZE` continue to resolve.
//!
//! Wave 3 of the density refactor adds the GUI `Spatial` resolver struct,
//! which layers app-specific chrome dimensions on top of the eparts
//! `Spatial` (mirroring how `design_tokens/semantic.rs` layers app color
//! roles on top of eparts semantic roles).

// ── Density API — re-exported from eparts ────────────────────────────────────
// Corner radii
// Row heights
// ── Generic tokens — individually re-exported from eparts ─────────────────────
// Individual re-exports (rather than `pub use …::spatial;`) are required
// so that local submodules like `dialog` can resolve `super::SPACE_4`.

pub use eparts::tokens::spatial::{
    Density, RADIUS_L, RADIUS_M, RADIUS_S, RADIUS_XL, ROW_L, ROW_M, ROW_S, ROW_XS, SPACE_0,
    SPACE_1, SPACE_2, SPACE_3, SPACE_4, SPACE_5, SPACE_6, SPACE_7, SPACE_8, STROKE_WIDTH,
    STROKE_WIDTH_THICK, STROKE_WIDTH_THIN, component, density, density_from_ctx, set_density,
};

// ── GUI Spatial resolver ─────────────────────────────────────────────────────
//
// Composes the eparts `Spatial` (generic space_*/row_*/toggle/component)
// with app-specific scaling submodules. Read once per widget:
//
//     let s = design_tokens::spatial::spatial(ui);
//     s.base.space_4          // generic spacing
//     s.toolbar.height        // toolbar chrome
//     s.timeline.track_row_height  // timeline chrome

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spatial {
    /// Generic spatial tokens from eparts (space_*, row_*, toggle, component).
    pub base: eparts::tokens::spatial::Spatial,
    /// Toolbar chrome dimensions.
    pub toolbar: ToolbarSpatial,
    /// Timeline panel chrome dimensions.
    pub timeline: TimelineSpatial,
    /// Inspector panel chrome dimensions.
    pub inspector: InspectorSpatial,
    /// Welcome screen chrome dimensions.
    pub welcome: WelcomeSpatial,
    /// Dialog chrome dimensions.
    pub dialog: DialogSpatial,
}

impl Spatial {
    /// Resolve all tokens for the given [`Density`], scaling every chrome
    /// field from its base const value.
    pub fn for_density(d: Density) -> Self {
        Self {
            base: eparts::tokens::spatial::Spatial::for_density(d),
            toolbar: ToolbarSpatial::for_density(d),
            timeline: TimelineSpatial::for_density(d),
            inspector: InspectorSpatial::for_density(d),
            welcome: WelcomeSpatial::for_density(d),
            dialog: DialogSpatial::for_density(d),
        }
    }
}

/// Resolve [`Spatial`] from a [`egui::Ui`].
pub fn spatial(ui: &egui::Ui) -> Spatial {
    Spatial::for_density(density(ui))
}

/// Resolve [`Spatial`] from an [`egui::Context`].
pub fn spatial_from_ctx(ctx: &egui::Context) -> Spatial {
    Spatial::for_density(density_from_ctx(ctx))
}

// ── App-specific chrome sub-structs ──────────────────────────────────────────

/// Toolbar chrome dimensions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ToolbarSpatial {
    /// Total height of the toolbar strip (28 px base).
    pub height: f32,
}

impl ToolbarSpatial {
    pub fn for_density(d: Density) -> Self {
        Self {
            height: d.scale(toolbar::HEIGHT),
        }
    }
}

/// Timeline panel chrome dimensions.
///
/// `KF_HALF` is intentionally excluded — it is a keyframe-diamond
/// marker size, not chrome height/spacing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineSpatial {
    /// Height of a single track row (24 px base).
    pub track_row_height: f32,
    /// Height of the time ruler at the top of the timeline (22 px base).
    pub ruler_height: f32,
    /// Height of the loop-range overlay strip (20 px base).
    pub range_height: f32,
    /// Height of the playback-position strip (28 px base).
    pub playback_strip_height: f32,
    /// Width of the track-name label column (120 px base).
    pub label_col_width: f32,
}

impl TimelineSpatial {
    pub fn for_density(d: Density) -> Self {
        Self {
            track_row_height: d.scale(timeline::TRACK_ROW_HEIGHT),
            ruler_height: d.scale(timeline::RULER_HEIGHT),
            range_height: d.scale(timeline::RANGE_HEIGHT),
            playback_strip_height: d.scale(timeline::PLAYBACK_STRIP_HEIGHT),
            label_col_width: d.scale(timeline::LABEL_COL_WIDTH),
        }
    }
}

/// Inspector panel chrome dimensions.
///
/// Input field widths, label fractions, and keyframe button widths are
/// intentionally excluded — they are content widths, not spacing/row chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InspectorSpatial {
    /// Height of each inspector property row (24 px base, equals ROW_M).
    pub row_height: f32,
    /// Gap between columns in multi-column inspector layouts (8 px base).
    pub col_gap: f32,
}

impl InspectorSpatial {
    pub fn for_density(d: Density) -> Self {
        Self {
            row_height: d.scale(inspector::ROW_HEIGHT),
            col_gap: d.scale(inspector::COL_GAP),
        }
    }
}

/// Welcome screen chrome dimensions.
///
/// `TOP_OFFSET_FRAC` is intentionally excluded — it is a layout fraction,
/// not a pixel dimension.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WelcomeSpatial {
    /// Height of the primary action buttons (36 px base).
    pub btn_height: f32,
}

impl WelcomeSpatial {
    pub fn for_density(d: Density) -> Self {
        Self {
            btn_height: d.scale(welcome::BTN_HEIGHT),
        }
    }
}

/// Dialog chrome dimensions.
///
/// `SLIDE_PX`, `KEY_COL_FRAC`, `KEY_COL_MAX`, `SINGLE_COL_THRESHOLD`,
/// and `MAX_VIEWPORT_FRAC` are intentionally excluded — they are animation
/// distances and layout thresholds/fractions, not density chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DialogSpatial {
    /// Inner margin around dialog content (12 px base).
    pub inner_margin: f32,
    /// Minimum gap between dialog edge and screen edge (24 px base).
    pub screen_margin: f32,
    /// Gap between columns in multi-column dialog layouts (8 px base).
    pub col_gap: f32,
}

impl DialogSpatial {
    pub fn for_density(d: Density) -> Self {
        Self {
            inner_margin: d.scale(dialog::INNER_MARGIN),
            screen_margin: d.scale(dialog::SCREEN_MARGIN),
            col_gap: d.scale(dialog::COL_GAP),
        }
    }
}

// ── App-specific submodules — defined locally ────────────────────────────────

// ── Preview canvas ──
// All preview constants are NON-SCALING canvas-space interaction affordances
// (handles, hit radii, cross sizes, rotation offsets). They remain as plain
// consts and are intentionally excluded from the GUI `Spatial` struct.
pub mod preview {
    pub const ROTATION_OFFSET: f32 = 20.0;
    pub const ROTATION_RADIUS: f32 = 4.0;
    pub const HANDLE_SIZE: f32 = 6.0;
    pub const HANDLE_HIT_RADIUS: f32 = 10.0;
    pub const MIN_ACTOR_SIZE: f32 = 10.0;
    pub const MIN_SCALE: f32 = 0.01;
    pub const MIN_ZOOM: f32 = 0.01;
    pub const DASH_LEN: f32 = 6.0;
    pub const GAP_LEN: f32 = 4.0;
    pub const CROSS_SIZE: f32 = 6.0;
    pub const VERTEX_HIT_BUFFER: f32 = 2.0;
    pub const ROTATION_HIT_BUFFER: f32 = 4.0;
}

// ── Toolbar ──
pub mod toolbar {
    /// Total height of the toolbar strip (scaling chrome).
    pub const HEIGHT: f32 = 28.0;
}

// ── Timeline ──
pub mod timeline {
    /// Width of the track-name label column (scaling chrome).
    pub const LABEL_COL_WIDTH: f32 = 120.0;
    /// Height of a single track row (scaling chrome).
    pub const TRACK_ROW_HEIGHT: f32 = 24.0;
    /// Height of the time ruler (scaling chrome).
    pub const RULER_HEIGHT: f32 = 22.0;
    /// Height of the loop-range overlay strip (scaling chrome).
    pub const RANGE_HEIGHT: f32 = 20.0;
    /// Half-width of the keyframe diamond marker — NOT scaling (legibility/hit target).
    pub const KF_HALF: f32 = 4.0;
    /// Height of the playback-position strip (scaling chrome).
    pub const PLAYBACK_STRIP_HEIGHT: f32 = 28.0;
}

// ── Welcome screen ──
pub mod welcome {
    /// Height of the primary action buttons (scaling chrome).
    pub const BTN_HEIGHT: f32 = 36.0;
    /// Vertical offset as a fraction of viewport — NOT scaling (layout fraction).
    pub const TOP_OFFSET_FRAC: f32 = 0.22;
}

// ── Dialog layout constants ──
pub mod dialog {
    // Generic dialog metrics live in eparts (used by the dialog widget);
    // re-export them so existing paths still resolve.
    use eparts::tokens::spatial::SPACE_4;
    pub use eparts::tokens::spatial::dialog::{
        INNER_MARGIN, MAX_VIEWPORT_FRAC, SCREEN_MARGIN, SLIDE_PX,
    };

    /// Gap between columns in multi-column layouts (scaling spacing, 8px).
    pub const COL_GAP: f32 = SPACE_4;
    /// Fraction of column width reserved for the key label in shortcut rows — NOT scaling.
    pub const KEY_COL_FRAC: f32 = 0.42;
    /// Maximum pixel width of the key label in shortcut rows — NOT scaling.
    pub const KEY_COL_MAX: f32 = 150.0;
    /// Below this available width (px), shortcuts collapse to 1 column — NOT scaling.
    pub const SINGLE_COL_THRESHOLD: f32 = 440.0;
}

// ── Inspector layout ──
pub mod inspector {
    // ROW_M is re-exported at the `spatial` module level from eparts;
    // Rust use-super resolution does not follow re-export chains, so import
    // from eparts directly.
    use eparts::tokens::spatial::ROW_M;

    /// Width of the keyframe column in the inspector — NOT scaling (content width).
    pub const KF_COL_WIDTH: f32 = 18.0;
    /// Minimum width of property labels — NOT scaling (content width).
    pub const LABEL_MIN_WIDTH: f32 = 90.0;
    /// Maximum width of property labels — NOT scaling (content width).
    pub const LABEL_MAX_WIDTH: f32 = 160.0;
    /// Gap between columns (scaling spacing).
    pub const COL_GAP: f32 = 8.0;
    /// Width of a float input field — NOT scaling (content width).
    pub const INPUT_WIDTH_FLOAT: f32 = 72.0;
    /// Width of the color/gradient input column — NOT scaling (content width).
    pub const INPUT_COL_WIDTH: f32 = 120.0;
    /// Width of a vec2 input field — NOT scaling (content width).
    pub const INPUT_WIDTH_VEC2: f32 = 110.0;
    /// Width of a slider input field — NOT scaling (content width).
    pub const INPUT_WIDTH_SLIDER: f32 = 110.0;
    /// Width of a color input field — NOT scaling (content width).
    pub const INPUT_WIDTH_COLOR: f32 = 88.0;
    /// Height of each inspector property row (scaling chrome, equals ROW_M).
    pub const ROW_HEIGHT: f32 = ROW_M;
    /// Width of the keyframe button — NOT scaling (icon button).
    pub const KF_BTN_WIDTH: f32 = 18.0;
    /// Fraction of label column width — NOT scaling (layout fraction).
    pub const LABEL_WIDTH_FRAC: f32 = 0.42;
}

// ═══════════════════════════════════════════════════════════════════════════
// Unit tests — Wave 3
// ═══════════════════════════════════════════════════════════════════════════
// Verifies that Density::Default produces byte-identical results to the
// base consts for both the eparts portion (via base.space_n / base.row_n)
// and every GUI chrome dimension.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_density_matches_base_consts() {
        let s = Spatial::for_density(Density::Default);

        // eparts generic tokens via base
        assert_eq!(s.base.space_0, SPACE_0);
        assert_eq!(s.base.space_1, SPACE_1);
        assert_eq!(s.base.space_2, SPACE_2);
        assert_eq!(s.base.space_3, SPACE_3);
        assert_eq!(s.base.space_4, SPACE_4);
        assert_eq!(s.base.space_5, SPACE_5);
        assert_eq!(s.base.space_6, SPACE_6);
        assert_eq!(s.base.space_7, SPACE_7);
        assert_eq!(s.base.space_8, SPACE_8);
        assert_eq!(s.base.row_xs, ROW_XS);
        assert_eq!(s.base.row_s, ROW_S);
        assert_eq!(s.base.row_m, ROW_M);
        assert_eq!(s.base.row_l, ROW_L);

        // toolbar
        assert_eq!(s.toolbar.height, toolbar::HEIGHT);

        // timeline
        assert_eq!(s.timeline.track_row_height, timeline::TRACK_ROW_HEIGHT);
        assert_eq!(s.timeline.ruler_height, timeline::RULER_HEIGHT);
        assert_eq!(s.timeline.range_height, timeline::RANGE_HEIGHT);
        assert_eq!(s.timeline.playback_strip_height, timeline::PLAYBACK_STRIP_HEIGHT);
        assert_eq!(s.timeline.label_col_width, timeline::LABEL_COL_WIDTH);

        // inspector
        assert_eq!(s.inspector.row_height, inspector::ROW_HEIGHT);
        assert_eq!(s.inspector.col_gap, inspector::COL_GAP);

        // welcome
        assert_eq!(s.welcome.btn_height, welcome::BTN_HEIGHT);

        // dialog
        assert_eq!(s.dialog.inner_margin, dialog::INNER_MARGIN);
        assert_eq!(s.dialog.screen_margin, dialog::SCREEN_MARGIN);
        assert_eq!(s.dialog.col_gap, dialog::COL_GAP);
    }

    #[test]
    fn compact_density_shrinks_chrome_fields() {
        let s = Spatial::for_density(Density::Compact);

        // toolbar chrome must shrink
        assert!(s.toolbar.height < toolbar::HEIGHT);
        assert_eq!(s.toolbar.height, (toolbar::HEIGHT * 0.875).round());

        // timeline chrome must shrink
        assert!(s.timeline.track_row_height < timeline::TRACK_ROW_HEIGHT);
        assert_eq!(s.timeline.ruler_height, (timeline::RULER_HEIGHT * 0.875).round());
        assert_eq!(s.timeline.range_height, (timeline::RANGE_HEIGHT * 0.875).round());
        assert_eq!(
            s.timeline.playback_strip_height,
            (timeline::PLAYBACK_STRIP_HEIGHT * 0.875).round()
        );
        assert_eq!(s.timeline.label_col_width, (timeline::LABEL_COL_WIDTH * 0.875).round());

        // inspector chrome must shrink
        assert!(s.inspector.row_height < inspector::ROW_HEIGHT);
        assert_eq!(s.inspector.col_gap, (inspector::COL_GAP * 0.875).round());

        // welcome chrome must shrink
        assert!(s.welcome.btn_height < welcome::BTN_HEIGHT);
        assert_eq!(s.welcome.btn_height, (welcome::BTN_HEIGHT * 0.875).round());

        // dialog chrome must shrink
        assert!(s.dialog.inner_margin < dialog::INNER_MARGIN);
        assert!(s.dialog.screen_margin < dialog::SCREEN_MARGIN);
        assert_eq!(s.dialog.col_gap, (dialog::COL_GAP * 0.875).round());
    }

    #[test]
    fn default_density_is_byte_identical_to_base_consts() {
        // Sanity: every scaled field in Default mode must equal the base const exactly.
        let s = Spatial::for_density(Density::Default);
        assert_eq!(s.toolbar.height, 28.0);
        assert_eq!(s.timeline.track_row_height, 24.0);
        assert_eq!(s.timeline.ruler_height, 22.0);
        assert_eq!(s.timeline.range_height, 20.0);
        assert_eq!(s.timeline.playback_strip_height, 28.0);
        assert_eq!(s.timeline.label_col_width, 120.0);
        assert_eq!(s.inspector.row_height, 24.0); // == ROW_M
        assert_eq!(s.inspector.col_gap, 8.0);
        assert_eq!(s.welcome.btn_height, 36.0);
        assert_eq!(s.dialog.inner_margin, 12.0); // == SPACE_5
        assert_eq!(s.dialog.screen_margin, 24.0); // == SPACE_7
        assert_eq!(s.dialog.col_gap, 8.0); // == SPACE_4
    }
}
