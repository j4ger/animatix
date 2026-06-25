//! Spatial tokens — spacing, row heights, stroke widths, radii,
//! and domain-specific layout dimensions.
//!
//! Generic spatial constants (`SPACE_*`, `ROW_*`, `RADIUS_*`, `STROKE_*`,
//! legacy aliases, `component`) are individually re-exported from
//! `eparts::tokens::spatial`. App-specific submodules (`preview`, `timeline`,
//! `inspector`, `toolbar`, `welcome`, `dialog`) are defined inline
//! below.
//!
//! Every item — generic and app-specific — is at the same module depth so
//! that existing paths like `design_tokens::spatial::SPACE_4` and
//! `design_tokens::spatial::preview::HANDLE_SIZE` continue to resolve.

// ── Generic tokens — individually re-exported from eparts ─────────────────────
// Individual re-exports (rather than `pub use …::spatial;`) are required
// so that local submodules like `dialog` can resolve `super::SPACE_4`.

// Spacing scale
pub use eparts::tokens::spatial::SPACE_0;
pub use eparts::tokens::spatial::SPACE_1;
pub use eparts::tokens::spatial::SPACE_2;
pub use eparts::tokens::spatial::SPACE_3;
pub use eparts::tokens::spatial::SPACE_4;
pub use eparts::tokens::spatial::SPACE_5;
pub use eparts::tokens::spatial::SPACE_6;
pub use eparts::tokens::spatial::SPACE_7;
pub use eparts::tokens::spatial::SPACE_8;

// Legacy aliases
pub use eparts::tokens::spatial::PAD_XXL;
pub use eparts::tokens::spatial::PAD_L;
pub use eparts::tokens::spatial::PAD_M;
pub use eparts::tokens::spatial::PAD_S;
pub use eparts::tokens::spatial::PAD_XL;
pub use eparts::tokens::spatial::PAD_XS;
pub use eparts::tokens::spatial::SPACE_XL;
pub use eparts::tokens::spatial::SPACE_XS;
pub use eparts::tokens::spatial::SPACE_L;
pub use eparts::tokens::spatial::SPACE_M;
pub use eparts::tokens::spatial::SPACE_S;

// Row heights
pub use eparts::tokens::spatial::ROW_L;
pub use eparts::tokens::spatial::ROW_M;
pub use eparts::tokens::spatial::ROW_S;
pub use eparts::tokens::spatial::ROW_XS;

// Stroke widths
pub use eparts::tokens::spatial::STROKE_WIDTH;
pub use eparts::tokens::spatial::STROKE_WIDTH_THICK;
pub use eparts::tokens::spatial::STROKE_WIDTH_THIN;

// Corner radii
pub use eparts::tokens::spatial::RADIUS_L;
pub use eparts::tokens::spatial::RADIUS_M;
pub use eparts::tokens::spatial::RADIUS_S;
pub use eparts::tokens::spatial::RADIUS_XL;

// Reusable component submodule
pub use eparts::tokens::spatial::component;

// ── App-specific submodules — defined locally ────────────────────────────────

// ── Preview canvas ──
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
    pub const HEIGHT: f32 = 28.0;
}

// ── Timeline ──
pub mod timeline {
    pub const LABEL_COL_WIDTH: f32 = 120.0;
    pub const TRACK_ROW_HEIGHT: f32 = 24.0;
    pub const RULER_HEIGHT: f32 = 22.0;
    pub const RANGE_HEIGHT: f32 = 20.0;
    pub const KF_HALF: f32 = 4.0;
    pub const PLAYBACK_STRIP_HEIGHT: f32 = 28.0;
}

// ── Welcome screen ──
pub mod welcome {
    pub const BTN_HEIGHT: f32 = 36.0;
    pub const TOP_OFFSET_FRAC: f32 = 0.22;
}

// ── Dialog layout constants ──
pub mod dialog {
    // Generic dialog metrics live in eparts (used by the dialog widget);
    // re-export them so existing paths still resolve.
    pub use eparts::tokens::spatial::dialog::{INNER_MARGIN, SCREEN_MARGIN, MAX_VIEWPORT_FRAC, SLIDE_PX};

    use eparts::tokens::spatial::SPACE_4;

    /// Gap between columns in multi-column layouts (8px).
    pub const COL_GAP: f32 = SPACE_4;
    /// Fraction of column width reserved for the key label in shortcut rows.
    pub const KEY_COL_FRAC: f32 = 0.42;
    /// Maximum pixel width of the key label in shortcut rows.
    pub const KEY_COL_MAX: f32 = 150.0;
    /// Below this available width (px), shortcuts collapse to 1 column.
    pub const SINGLE_COL_THRESHOLD: f32 = 440.0;
}

// ── Inspector layout ──
pub mod inspector {
    // ROW_M is re-exported at the `spatial` module level from eparts;
    // Rust use-super resolution does not follow re-export chains, so import
    // from eparts directly.
    use eparts::tokens::spatial::ROW_M;

    pub const KF_COL_WIDTH: f32 = 18.0;
    pub const LABEL_MIN_WIDTH: f32 = 90.0;
    pub const LABEL_MAX_WIDTH: f32 = 160.0;
    pub const COL_GAP: f32 = 8.0;
    pub const INPUT_WIDTH_FLOAT: f32 = 72.0;
    pub const INPUT_COL_WIDTH: f32 = 120.0;
    pub const INPUT_WIDTH_VEC2: f32 = 110.0;
    pub const INPUT_WIDTH_SLIDER: f32 = 110.0;
    pub const INPUT_WIDTH_COLOR: f32 = 88.0;
    pub const ROW_HEIGHT: f32 = ROW_M;
    pub const KF_BTN_WIDTH: f32 = 18.0;
    pub const LABEL_WIDTH_FRAC: f32 = 0.42;
}
