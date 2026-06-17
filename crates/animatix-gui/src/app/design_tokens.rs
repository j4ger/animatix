//! Compatibility facade for the layered design token system.
//!
//! # Migration state
//! This file temporarily re-exports all legacy flat names (`BG_*`, `TEXT_*`,
//! `PAD_*`, `FONT_SIZE_*`, etc.) from the new `design_tokens/` module tree.
//! During Phase 1 migration, call sites switch from `glob` imports to narrow
//! `semantic::{...}` / `spatial` / `typography` imports.
//!
//! Once all call sites are migrated, this file is deleted and replaced by
//! `design_tokens/mod.rs`.
//!
//! # Naming conventions (legacy)
//! - Backgrounds: `BG_*` (BG_BASE, BG_SURFACE, BG_WIDGET, BG_HOVER, BG_ACTIVE)
//! - Text: `TEXT_*` (TEXT_PRIMARY, TEXT_SECONDARY, TEXT_MUTED, TEXT_DISABLED)
//! - Accent/semantic: `ACCENT_*`, `GREEN`, `RED`, `AMBER`, `PURPLE`
//! - Spacing: `SPACE_*` (SPACE_XS, S, M, L, XL)
//! - Padding: `PAD_*` (PAD_XS, S, M, L, XL, XXL)
//! - Font sizes: `FONT_SIZE_*` (FONT_SIZE_XS, S, M, L, XL)
//! - Radii: `RADIUS_*` (RADIUS_S, M, L, XL)
//! - Timeline: `TIMELINE_*`

mod primitive;
pub mod motion;
pub mod semantic;
pub mod spatial;
pub mod typography;
pub mod util;

pub use util::{lerp_color, multiply_alpha};

// ── Backgrounds (→ semantic::surface) ──
pub use semantic::surface::BASE as BG_BASE;
pub use semantic::surface::PANEL as BG_PANEL;
pub use semantic::surface::SURFACE as BG_SURFACE;
pub use semantic::surface::WIDGET as BG_WIDGET;
pub use semantic::surface::HOVER as BG_HOVER;
pub use semantic::surface::ACTIVE as BG_ACTIVE;

// ── Text (→ semantic::text) ──
pub use semantic::text::PRIMARY as TEXT_PRIMARY;
pub use semantic::text::SECONDARY as TEXT_SECONDARY;
pub use semantic::text::MUTED as TEXT_MUTED;
pub use semantic::text::DISABLED as TEXT_DISABLED;

// ── Accents (→ semantic::accent / semantic::status / semantic::category) ──
pub use semantic::accent::PRIMARY as ACCENT_BLUE;
pub use semantic::accent::CYAN as ACCENT_CYAN;
pub use semantic::status::WARNING as AMBER;
pub use semantic::status::ERROR as RED;
pub use semantic::status::SUCCESS as GREEN;
pub use semantic::category::ACTION as PURPLE;

// ── Borders (→ semantic::border) ──
pub use semantic::border::DEFAULT as BORDER;
pub use semantic::border::HOVER as BORDER_HOVER;
pub use semantic::border::FOCUS as BORDER_FOCUS;

// ── Diagnostic phases (→ semantic::diagnostic) ──
pub use semantic::diagnostic::PHASE_PARSE as DIAG_PHASE_PARSE;
pub use semantic::diagnostic::PHASE_RESOLVE as DIAG_PHASE_RESOLVE;
pub use semantic::diagnostic::PHASE_COMPILE as DIAG_PHASE_COMPILE;

// ── Status colors (→ semantic::status) ──
pub use semantic::status::PLAYING_TEXT;
pub use semantic::status::DIAGNOSTIC_ERROR as DIAGNOSTIC_RED;
pub use semantic::status::DIAGNOSTIC_WARNING as DIAGNOSTIC_AMBER;

// ── Curve editor (→ semantic::curve) ──
pub use semantic::curve::GREEN as CURVE_GREEN;
pub use semantic::curve::BLUE as CURVE_BLUE;
pub use semantic::curve::GRAY as CURVE_GRAY;

// ── Timeline (→ semantic::timeline) ──
pub use semantic::timeline::KF_FLASH;

// ── Editor (→ semantic::editor) ──
pub use semantic::editor::SNIPPET_BLUE;

// ── Alpha-tinted function wrappers (→ semantic::* functions) ──

pub use semantic::canvas::ghost_prev;
pub use semantic::canvas::ghost_next;
pub use semantic::canvas::grid_line;
pub use semantic::canvas::guide_line;
pub use semantic::canvas::hatch_line;
pub use semantic::canvas::snap_guide_line;
pub use semantic::canvas::snap_guide_label_bg;

pub use semantic::timeline::track_block_1;
pub use semantic::timeline::track_block_2;
pub use semantic::timeline::track_block_3;
pub use semantic::timeline::track_block_4;
pub use semantic::timeline::track_block_5;
pub use semantic::timeline::loop_region;
pub use semantic::timeline::transition_stripe_1;
pub use semantic::timeline::transition_stripe_2;
pub use semantic::timeline::transition_stripe_3;
pub use semantic::timeline::transition_stripe_4;
pub use semantic::timeline::transition_stripe_5;
pub use semantic::timeline::transition_stripe_6;
pub use semantic::timeline::row_alt;

pub use semantic::overlay::backdrop as overlay_backdrop;
pub use semantic::overlay::badge_bg;
pub use semantic::overlay::tooltip_bg;
pub use semantic::overlay::shadow_ambient;
pub use semantic::overlay::shadow_direct;

pub use semantic::surface::floating_card_bg;

pub use semantic::accent::faint as accent_faint;
pub use semantic::accent::ghost as accent_ghost;
pub use semantic::accent::subtle as accent_subtle;
pub use semantic::accent::hover as accent_hover;
pub use semantic::accent::strong as accent_strong;
pub use semantic::accent::selection as accent_selection;

pub use semantic::text::faint as text_faint;
pub use semantic::text::subtle as text_subtle;
pub use semantic::text::hover as text_hover;
pub use semantic::text::dim as text_dim;

pub use semantic::status::warning_subtle as amber_subtle;
pub use semantic::status::success_faint as green_faint;
pub use semantic::status::success_ultra_faint as green_ultra_faint;
pub use semantic::status::error_faint as red_faint;
pub use semantic::status::error_ultra_faint as red_ultra_faint;

// ── Spatial aliases (→ spatial module) ──

pub use spatial::SPACE_1 as SPACE_XS;
pub use spatial::SPACE_2 as SPACE_S;
pub use spatial::SPACE_3 as SPACE_M;
pub use spatial::SPACE_4 as SPACE_L;
pub use spatial::SPACE_5 as SPACE_XL;

pub use spatial::SPACE_1 as PAD_XS;
pub use spatial::SPACE_2 as PAD_S;
pub use spatial::SPACE_3 as PAD_M;
pub use spatial::SPACE_4 as PAD_L;
pub use spatial::SPACE_5 as PAD_XL;
pub use spatial::SPACE_6 as PAD_XXL;

pub use spatial::{ROW_XS, ROW_S, ROW_M, ROW_L};
pub use spatial::{STROKE_WIDTH, STROKE_WIDTH_THICK, STROKE_WIDTH_THIN};
pub use spatial::{RADIUS_S, RADIUS_M, RADIUS_L, RADIUS_XL};

pub use spatial::preview::ROTATION_OFFSET as PREVIEW_ROTATION_OFFSET;
pub use spatial::preview::ROTATION_RADIUS as PREVIEW_ROTATION_RADIUS;
pub use spatial::preview::HANDLE_SIZE as PREVIEW_HANDLE_SIZE;
pub use spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;
pub use spatial::preview::MIN_ACTOR_SIZE as PREVIEW_MIN_ACTOR_SIZE;
pub use spatial::preview::MIN_SCALE as PREVIEW_MIN_SCALE;
pub use spatial::preview::MIN_ZOOM as PREVIEW_MIN_ZOOM;
pub use spatial::preview::DASH_LEN as PREVIEW_DASH_LEN;
pub use spatial::preview::GAP_LEN as PREVIEW_GAP_LEN;
pub use spatial::preview::CROSS_SIZE as PREVIEW_CROSS_SIZE;
pub use spatial::preview::VERTEX_HIT_BUFFER as PREVIEW_VERTEX_HIT_BUFFER;
pub use spatial::preview::ROTATION_HIT_BUFFER as PREVIEW_ROTATION_HIT_BUFFER;

pub use spatial::toolbar::HEIGHT as TOOLBAR_HEIGHT;

pub use spatial::timeline::LABEL_COL_WIDTH as TIMELINE_LABEL_COL_WIDTH;
pub use spatial::timeline::TRACK_ROW_HEIGHT as TIMELINE_TRACK_ROW_HEIGHT;
pub use spatial::timeline::RULER_HEIGHT as TIMELINE_RULER_HEIGHT;
pub use spatial::timeline::RANGE_HEIGHT as TIMELINE_RANGE_HEIGHT;
pub use spatial::timeline::KF_HALF as TIMELINE_KF_HALF;
pub use spatial::timeline::PLAYBACK_STRIP_HEIGHT as TIMELINE_PLAYBACK_STRIP_HEIGHT;

pub use spatial::menu::MIN_WIDTH as MENU_MIN_WIDTH;
pub use spatial::menu::ICON_WIDTH as MENU_ICON_WIDTH;
pub use spatial::menu::CHECK_WIDTH as MENU_CHECK_WIDTH;
pub use spatial::menu::SHADOW_OFFSET_Y as MENU_SHADOW_OFFSET_Y;
pub use spatial::menu::SHADOW_BLUR as MENU_SHADOW_BLUR;

pub use spatial::welcome::BTN_HEIGHT as WELCOME_BTN_HEIGHT;
pub use spatial::welcome::TOP_OFFSET_FRAC as WELCOME_TOP_OFFSET_FRAC;

pub use spatial::component::{PILL_TAB_HEIGHT, PILL_TAB_GAP};
pub use spatial::component::{TOAST_WIDTH, TOAST_HEIGHT, TOAST_SPACING, TOAST_MARGIN};
pub use spatial::component::ICON_SLOT_WIDTH;

pub use spatial::inspector::KF_COL_WIDTH as INSPECTOR_KF_COL_WIDTH;
pub use spatial::inspector::LABEL_MIN_WIDTH as INSPECTOR_LABEL_MIN_WIDTH;
pub use spatial::inspector::LABEL_MAX_WIDTH as INSPECTOR_LABEL_MAX_WIDTH;
pub use spatial::inspector::COL_GAP as INSPECTOR_COL_GAP;
pub use spatial::inspector::INPUT_WIDTH_FLOAT as INSPECTOR_INPUT_WIDTH_FLOAT;
pub use spatial::inspector::INPUT_COL_WIDTH as INSPECTOR_INPUT_COL_WIDTH;
pub use spatial::inspector::INPUT_WIDTH_VEC2 as INSPECTOR_INPUT_WIDTH_VEC2;
pub use spatial::inspector::INPUT_WIDTH_SLIDER as INSPECTOR_INPUT_WIDTH_SLIDER;
pub use spatial::inspector::INPUT_WIDTH_COLOR as INSPECTOR_INPUT_WIDTH_COLOR;
pub use spatial::inspector::ROW_HEIGHT as INSPECTOR_ROW_HEIGHT;
pub use spatial::inspector::KF_BTN_WIDTH as INSPECTOR_KF_BTN_WIDTH;
pub use spatial::inspector::LABEL_WIDTH_FRAC as INSPECTOR_LABEL_WIDTH_FRAC;

// ── Typography aliases (→ typography module) ──
pub use typography::{FONT_SIZE_XS, FONT_SIZE_S, FONT_SIZE_M, FONT_SIZE_L, FONT_SIZE_XL};
