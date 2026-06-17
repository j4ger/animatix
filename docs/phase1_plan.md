# Phase 1 Plan: Token Refoundation

## Goal
Replace the flat GUI token file with a layered `app/design_tokens/` module tree while preserving buildability after each migration checkpoint.

## Assumptions
- Do not create `crates/animatix-gui/src/app/design_tokens/mod.rs` while `crates/animatix-gui/src/app/design_tokens.rs` still exists; Rust treats them as the same module and will fail with duplicate module-file candidates.
- Start by keeping `design_tokens.rs` as the temporary compatibility facade, with submodules loaded from `design_tokens/*.rs`; create `design_tokens/mod.rs` only in the final checkpoint when `design_tokens.rs` is removed.
- In egui 0.34, the current file documents that `Color32::from_rgba_unmultiplied` is not const, so alpha-tinted tokens should remain functions in Phase 1 unless `cargo check -p animatix-gui` proves consts compile.
- Phase 1 should not introduce component widgets or typography call-site refactors beyond import/path migration; `TextRole` is introduced now so Phase 2 can consume it.

## Current Usage Findings
- Hot import style: almost every GUI file imports `use crate::app::design_tokens::*;`; `cell_editor/render.rs` imports it as `dt`; `preview/grid.rs`, `preview/mod.rs`, and `timeline_panel.rs` also use narrow token imports.
- Hot color consumers: `runtime.rs`, `app/mod.rs`, `components/*`, `shell/toolbar.rs`, `panels/timeline_panel.rs`, `panels/inspector/*`, `panels/preview_panel.rs`, and `preview/*` use `BG_*`, `TEXT_*`, `ACCENT_BLUE`, `AMBER`, `GREEN`, `RED`, `BORDER`, and alpha helper functions heavily.
- Hot spatial consumers: `SPACE_*`, `ROW_*`, `RADIUS_*`, `STROKE_WIDTH`, `FONT_SIZE_*`, `ICON_SLOT_WIDTH`, timeline dimensions, inspector dimensions, and preview handle constants are actively used.
- Cold or currently unused by search: `BORDER_FOCUS`, `PLAYING_TEXT`, `STROKE_WIDTH_THICK`, `STROKE_WIDTH_THIN`, `PAD_XS`, `PAD_M`, `PAD_XL`, `multiply_alpha`, `guide_line`, `hatch_line`, `transition_stripe_1..6`, `floating_card_bg`, `text_subtle`, `text_hover`, `green_faint`, `green_ultra_faint`, `red_faint`, and `red_ultra_faint`; preserve them through the facade until the final cleanup confirms no call sites remain.

## Target Files and Exact Contents

### `crates/animatix-gui/src/app/design_tokens/primitive.rs`
- Purpose: raw color palette and non-semantic color atoms only; no UI code imports this file.
- Visibility: prefer `mod primitive;` plus `pub(super)` constants for strict token-only access; use `pub(crate)` only if needed to match the draft spec.
- Core palette:
  - `BLUE_600 = Color32::from_rgb(60, 84, 220)`
  - `BLUE_500 = Color32::from_rgb(84, 110, 255)`
  - `BLUE_400 = Color32::from_rgb(120, 145, 255)`
  - `CYAN_500 = Color32::from_rgb(137, 200, 235)`
  - `GREEN_500 = Color32::from_rgb(80, 200, 140)`
  - `AMBER_500 = Color32::from_rgb(255, 196, 92)`
  - `RED_500 = Color32::from_rgb(255, 100, 100)`
  - `PURPLE_500 = Color32::from_rgb(156, 39, 176)`
- Surface/text palette from `docs/gui_design_language.md`:
  - `GRAY_950 = Color32::from_rgb(10, 12, 16)`
  - `GRAY_900 = Color32::from_rgb(16, 18, 23)`
  - `GRAY_850 = Color32::from_rgb(22, 25, 31)`
  - `GRAY_800 = Color32::from_rgb(30, 34, 42)`
  - `GRAY_700 = Color32::from_rgb(42, 47, 57)`
  - `GRAY_600 = Color32::from_rgb(60, 66, 78)`
  - `GRAY_500 = Color32::from_rgb(90, 97, 112)`
  - `GRAY_400 = Color32::from_rgb(130, 138, 153)`
  - `GRAY_300 = Color32::from_rgb(170, 178, 192)`
  - `GRAY_200 = Color32::from_rgb(210, 216, 228)`
  - `GRAY_100 = Color32::from_rgb(232, 236, 245)`
- Domain raw colors not in the core palette:
  - `CANVAS_BG = Color32::from_rgb(8, 8, 12)`
  - `DIAG_PARSE = Color32::from_rgb(137, 180, 250)`
  - `DIAG_RESOLVE = Color32::from_rgb(180, 190, 254)`
  - `DIAG_COMPILE = Color32::from_rgb(203, 166, 126)`
  - `PLAYING_TEXT_RAW = Color32::from_rgb(216, 249, 235)`
  - `DIAGNOSTIC_RED_RAW = Color32::from_rgb(255, 136, 136)`
  - `DIAGNOSTIC_AMBER_RAW = Color32::from_rgb(255, 214, 102)`
  - `CURVE_GREEN_RAW = Color32::from_rgb(100, 255, 100)`
  - `CURVE_BLUE_RAW = Color32::from_rgb(80, 140, 255)`
  - `CURVE_GRAY_RAW = Color32::from_rgb(200, 200, 200)`
  - `KF_FLASH_RAW = Color32::from_rgb(255, 200, 50)`
  - `SNIPPET_BLUE_RAW = Color32::from_rgb(108, 153, 187)`

### `crates/animatix-gui/src/app/design_tokens/semantic.rs`
- Purpose: public semantic color API consumed by GUI code.
- Import shape: `use crate::app::design_tokens::semantic::{accent, border, canvas, category, diagnostic, editor, overlay, status, surface, text, timeline};`
- `surface` module:
  - `BASE` replaces `BG_BASE`
  - `PANEL` replaces `BG_PANEL`
  - `SURFACE` replaces `BG_SURFACE`
  - `WIDGET` replaces `BG_WIDGET`
  - `HOVER` replaces `BG_HOVER`
  - `ACTIVE` replaces `BG_ACTIVE`
  - `floating_card_bg()` replaces `floating_card_bg()`
- `text` module:
  - `PRIMARY` replaces `TEXT_PRIMARY`
  - `SECONDARY` replaces `TEXT_SECONDARY`
  - `MUTED` replaces `TEXT_MUTED`
  - `DISABLED` replaces `TEXT_DISABLED`
  - `ON_ACCENT = surface::BASE` for text/icons on accent or warning fills that currently use `BG_BASE`
  - `faint()` replaces `text_faint()`
  - `subtle()` replaces `text_subtle()`
  - `hover()` replaces `text_hover()`
  - `dim()` replaces `text_dim()`
- `accent` module:
  - `PRIMARY` replaces `ACCENT_BLUE`
  - `CYAN` replaces `ACCENT_CYAN`
  - `PRIMARY_HOVER` new spec token
  - `PRIMARY_ACTIVE` new spec token
  - `faint()` replaces `accent_faint()`
  - `ghost()` replaces `accent_ghost()`
  - `subtle()` replaces `accent_subtle()`
  - `hover()` replaces `accent_hover()`
  - `strong()` replaces `accent_strong()`
  - `selection()` replaces `accent_selection()`
- `status` module:
  - `SUCCESS` is the status replacement for success uses of `GREEN`
  - `WARNING` is the status replacement for warning uses of `AMBER`
  - `ERROR` is the status replacement for error uses of `RED`
  - `INFO` is the status replacement for informational uses of `ACCENT_BLUE`
  - `PLAYING_TEXT` preserves `PLAYING_TEXT`
  - `DIAGNOSTIC_ERROR` replaces `DIAGNOSTIC_RED`
  - `DIAGNOSTIC_WARNING` replaces `DIAGNOSTIC_AMBER`
  - `success_faint()` replaces status uses of `green_faint()`
  - `success_ultra_faint()` replaces status uses of `green_ultra_faint()`
  - `warning_subtle()` replaces `amber_subtle()` when the meaning is warning state
  - `error_faint()` replaces status uses of `red_faint()`
  - `error_ultra_faint()` replaces status uses of `red_ultra_faint()`
- `category` module:
  - `TRANSFORM` replaces category/property-group uses of `ACCENT_BLUE`
  - `STYLE` replaces category/property-group uses of `GREEN`
  - `SHAPE` replaces category/property-group uses of `AMBER`
  - `TEXT` replaces category/property-group uses of `ACCENT_CYAN`
  - `ACTION` replaces action/category uses of `PURPLE`
  - `FILTER` replaces `PropertyGroup::Filter => PURPLE`
  - `MEDIA` replaces insertion-palette media category uses of `PURPLE`
- `border` module:
  - `DEFAULT` replaces `BORDER`
  - `HOVER` replaces `BORDER_HOVER`
  - `FOCUS` replaces `BORDER_FOCUS`
- `canvas` module:
  - `BG = p::CANVAS_BG` for preview canvas background; use this for canvas-only fills instead of `surface::BASE` where the spec requires canvas separation
  - `grid_line()` replaces `grid_line()`
  - `guide_line()` replaces `guide_line()`
  - `hatch_line()` replaces `hatch_line()`
  - `ghost_prev()` replaces `ghost_prev()`
  - `ghost_next()` replaces `ghost_next()`
  - `snap_guide_line()` replaces `snap_guide_line()`
  - `snap_guide_label_bg()` replaces `snap_guide_label_bg()`
  - `measurement_guide()` should use the existing guide/snap colors instead of raw `AMBER`, `GREEN`, or `ACCENT_CYAN`
- `timeline` module:
  - `track_block_1()` replaces `track_block_1()`
  - `track_block_2()` replaces `track_block_2()`
  - `track_block_3()` replaces `track_block_3()`
  - `track_block_4()` replaces `track_block_4()`
  - `track_block_5()` replaces `track_block_5()`
  - `loop_region()` replaces `loop_region()`
  - `transition_stripe_1()` replaces `transition_stripe_1()`
  - `transition_stripe_2()` replaces `transition_stripe_2()`
  - `transition_stripe_3()` replaces `transition_stripe_3()`
  - `transition_stripe_4()` replaces `transition_stripe_4()`
  - `transition_stripe_5()` replaces `transition_stripe_5()`
  - `transition_stripe_6()` replaces `transition_stripe_6()`
  - `KF_FLASH` replaces `KF_FLASH`
  - `row_alt()` replaces `row_alt()` for alternating timeline/inspector rows
- `diagnostic` module:
  - `PHASE_PARSE` replaces `DIAG_PHASE_PARSE`
  - `PHASE_RESOLVE` replaces `DIAG_PHASE_RESOLVE`
  - `PHASE_COMPILE` replaces `DIAG_PHASE_COMPILE`
- `overlay` module:
  - `backdrop()` replaces `overlay_backdrop()`
  - `badge_bg()` replaces `badge_bg()`
  - `tooltip_bg()` replaces `tooltip_bg()`
  - `shadow_ambient()` replaces `shadow_ambient()`
  - `shadow_direct()` replaces `shadow_direct()`
- `curve` module:
  - `GREEN` replaces `CURVE_GREEN`
  - `BLUE` replaces `CURVE_BLUE`
  - `GRAY` replaces `CURVE_GRAY`
- `editor` module:
  - `SNIPPET_BLUE` replaces `SNIPPET_BLUE`

### `crates/animatix-gui/src/app/design_tokens/spatial.rs`
- Purpose: public spacing, size, radius, stroke, and legacy domain layout constants.
- Unified spacing scale:
  - `SPACE_0 = 0.0`
  - `SPACE_1 = 2.0` replaces `SPACE_XS` and `PAD_XS`
  - `SPACE_2 = 4.0` replaces `SPACE_S` and `PAD_S`
  - `SPACE_3 = 6.0` replaces `SPACE_M` and `PAD_M`
  - `SPACE_4 = 8.0` replaces `SPACE_L` and `PAD_L`
  - `SPACE_5 = 12.0` replaces `SPACE_XL` and `PAD_XL`
  - `SPACE_6 = 16.0` replaces `PAD_XXL`
  - `SPACE_7 = 24.0`
  - `SPACE_8 = 32.0`
- Row heights:
  - `ROW_XS = 18.0`
  - `ROW_S = 20.0`
  - `ROW_M = 24.0`
  - `ROW_L = 28.0`
- Stroke widths:
  - `STROKE_WIDTH = 1.0`
  - `STROKE_WIDTH_THICK = 1.5`
  - `STROKE_WIDTH_THIN = 0.5`
- Radii:
  - `RADIUS_S = 2.0`
  - `RADIUS_M = 4.0`
  - `RADIUS_L = 6.0`
  - `RADIUS_XL = 8.0`
- `preview` module:
  - `ROTATION_OFFSET = 20.0` replaces `PREVIEW_ROTATION_OFFSET`
  - `ROTATION_RADIUS = 4.0` replaces `PREVIEW_ROTATION_RADIUS`
  - `HANDLE_SIZE = 6.0` replaces `PREVIEW_HANDLE_SIZE`
  - `HANDLE_HIT_RADIUS = 10.0` replaces `PREVIEW_HANDLE_HIT_RADIUS`
  - `MIN_ACTOR_SIZE = 10.0` replaces `PREVIEW_MIN_ACTOR_SIZE`
  - `MIN_SCALE = 0.01` replaces `PREVIEW_MIN_SCALE`
  - `MIN_ZOOM = 0.01` replaces `PREVIEW_MIN_ZOOM`
  - `DASH_LEN = 6.0` replaces `PREVIEW_DASH_LEN`
  - `GAP_LEN = 4.0` replaces `PREVIEW_GAP_LEN`
  - `CROSS_SIZE = 6.0` replaces `PREVIEW_CROSS_SIZE`
  - `VERTEX_HIT_BUFFER = 2.0` replaces `PREVIEW_VERTEX_HIT_BUFFER`
  - `ROTATION_HIT_BUFFER = 4.0` replaces `PREVIEW_ROTATION_HIT_BUFFER`
- `toolbar` module:
  - `HEIGHT = 28.0` replaces `TOOLBAR_HEIGHT`
- `timeline` module:
  - `LABEL_COL_WIDTH = 120.0` replaces `TIMELINE_LABEL_COL_WIDTH`
  - `TRACK_ROW_HEIGHT = 24.0` replaces `TIMELINE_TRACK_ROW_HEIGHT`
  - `RULER_HEIGHT = 22.0` replaces `TIMELINE_RULER_HEIGHT`
  - `RANGE_HEIGHT = 20.0` replaces `TIMELINE_RANGE_HEIGHT`
  - `KF_HALF = 4.0` replaces `TIMELINE_KF_HALF`
  - `PLAYBACK_STRIP_HEIGHT = 28.0` replaces `TIMELINE_PLAYBACK_STRIP_HEIGHT`
- `menu` module:
  - `MIN_WIDTH = 140.0` replaces `MENU_MIN_WIDTH`
  - `ICON_WIDTH = 16.0` replaces `MENU_ICON_WIDTH`
  - `CHECK_WIDTH = 14.0` replaces `MENU_CHECK_WIDTH`
  - `SHADOW_OFFSET_Y = 4` replaces `MENU_SHADOW_OFFSET_Y`
  - `SHADOW_BLUR = 12` replaces `MENU_SHADOW_BLUR`
- `welcome` module:
  - `BTN_HEIGHT = 36.0` replaces `WELCOME_BTN_HEIGHT`
  - `TOP_OFFSET_FRAC = 0.22` replaces `WELCOME_TOP_OFFSET_FRAC`
- `component` module:
  - `PILL_TAB_HEIGHT = 26.0` replaces `PILL_TAB_HEIGHT`
  - `PILL_TAB_GAP = 2.0` replaces `PILL_TAB_GAP`
  - `TOAST_WIDTH = 280.0` replaces `TOAST_WIDTH`
  - `TOAST_HEIGHT = 40.0` replaces `TOAST_HEIGHT`
  - `TOAST_SPACING = 8.0` replaces `TOAST_SPACING`
  - `TOAST_MARGIN = 16.0` replaces `TOAST_MARGIN`
  - `ICON_SLOT_WIDTH = 14.0` replaces `ICON_SLOT_WIDTH`
- `inspector` module:
  - `KF_COL_WIDTH = 18.0` replaces `INSPECTOR_KF_COL_WIDTH`
  - `LABEL_MIN_WIDTH = 90.0` replaces `INSPECTOR_LABEL_MIN_WIDTH`
  - `LABEL_MAX_WIDTH = 160.0` replaces `INSPECTOR_LABEL_MAX_WIDTH`
  - `COL_GAP = 8.0` replaces `INSPECTOR_COL_GAP`
  - `INPUT_WIDTH_FLOAT = 72.0` replaces `INSPECTOR_INPUT_WIDTH_FLOAT`
  - `INPUT_COL_WIDTH = 120.0` replaces `INSPECTOR_INPUT_COL_WIDTH`
  - `INPUT_WIDTH_VEC2 = 110.0` replaces `INSPECTOR_INPUT_WIDTH_VEC2`
  - `INPUT_WIDTH_SLIDER = 110.0` replaces `INSPECTOR_INPUT_WIDTH_SLIDER`
  - `INPUT_WIDTH_COLOR = 88.0` replaces `INSPECTOR_INPUT_WIDTH_COLOR`
  - `ROW_HEIGHT = ROW_M` replaces `INSPECTOR_ROW_HEIGHT`
  - `KF_BTN_WIDTH = 18.0` replaces `INSPECTOR_KF_BTN_WIDTH`
  - `LABEL_WIDTH_FRAC = 0.42` replaces `INSPECTOR_LABEL_WIDTH_FRAC`

### `crates/animatix-gui/src/app/design_tokens/typography.rs`
- Purpose: public type roles and temporary font-size compatibility values.
- Role enum:
  - `TextRole::Display` => `20.0`, proportional
  - `TextRole::Heading` => `18.0`, proportional
  - `TextRole::Title` => `15.0`, proportional
  - `TextRole::Body` => `13.0`, proportional
  - `TextRole::BodyS` => `12.0`, proportional
  - `TextRole::Caption` => `11.0`, proportional
  - `TextRole::Mono` => `12.0`, monospace
  - `TextRole::Micro` => `10.0`, proportional
- Methods:
  - `TextRole::font_id(&self) -> egui::FontId`
  - `TextRole::size(&self) -> f32` for RichText `.size(...)` migration
- Temporary size constants for the facade:
  - `FONT_SIZE_XS = TextRole::Micro.size()` equivalent, `10.0`
  - `FONT_SIZE_S = TextRole::BodyS.size()` equivalent, `12.0`
  - `FONT_SIZE_M = TextRole::Body.size()` equivalent, `13.0`
  - `FONT_SIZE_L = TextRole::Title.size()` equivalent, `15.0`
  - `FONT_SIZE_XL = TextRole::Heading.size()` equivalent, `18.0`

### `crates/animatix-gui/src/app/design_tokens/motion.rs`
- Purpose: introduce stable motion tokens now, with call-site migration deferred unless touched by token imports.
- Duration constants in seconds for egui animation APIs:
  - `INSTANT = 0.0`
  - `FAST = 0.10`
  - `NORMAL = 0.20`
  - `SLOW = 0.40`
- Easing representation:
  - `pub struct CubicBezier { pub x1: f32, pub y1: f32, pub x2: f32, pub y2: f32 }`
  - `STANDARD = CubicBezier { x1: 0.4, y1: 0.0, x2: 0.2, y2: 1.0 }`
  - `DECELERATE = CubicBezier { x1: 0.0, y1: 0.0, x2: 0.2, y2: 1.0 }`
  - `ACCELERATE = CubicBezier { x1: 0.4, y1: 0.0, x2: 1.0, y2: 1.0 }`
  - `SPRING_OVERSHOOT = CubicBezier { x1: 0.34, y1: 1.56, x2: 0.64, y2: 1.0 }`

### `crates/animatix-gui/src/app/design_tokens/util.rs`
- Purpose: temporary utility compatibility; keep until all callers can use semantic precomputed tokens or local animation helpers.
- Functions:
  - `lerp_color(a: Color32, b: Color32, t: f32) -> Color32` moved unchanged from `design_tokens.rs`
  - `multiply_alpha(c: Color32, factor: f32) -> Color32` moved unchanged from `design_tokens.rs`

### Temporary `crates/animatix-gui/src/app/design_tokens.rs`
- Purpose: compatibility facade while call sites migrate.
- Module declarations:
  - `mod primitive;`
  - `pub mod semantic;`
  - `pub mod spatial;`
  - `pub mod typography;`
  - `pub mod motion;`
  - `pub mod util;`
- Re-export utility functions:
  - `pub use util::{lerp_color, multiply_alpha};`
- Flat color aliases:
  - `pub use semantic::surface::BASE as BG_BASE;`
  - `pub use semantic::surface::PANEL as BG_PANEL;`
  - `pub use semantic::surface::SURFACE as BG_SURFACE;`
  - `pub use semantic::surface::WIDGET as BG_WIDGET;`
  - `pub use semantic::surface::HOVER as BG_HOVER;`
  - `pub use semantic::surface::ACTIVE as BG_ACTIVE;`
  - `pub use semantic::text::PRIMARY as TEXT_PRIMARY;`
  - `pub use semantic::text::SECONDARY as TEXT_SECONDARY;`
  - `pub use semantic::text::MUTED as TEXT_MUTED;`
  - `pub use semantic::text::DISABLED as TEXT_DISABLED;`
  - `pub use semantic::accent::PRIMARY as ACCENT_BLUE;`
  - `pub use semantic::accent::CYAN as ACCENT_CYAN;`
  - `pub use semantic::status::WARNING as AMBER;`
  - `pub use semantic::status::ERROR as RED;`
  - `pub use semantic::status::SUCCESS as GREEN;`
  - `pub use semantic::category::ACTION as PURPLE;`
  - `pub use semantic::border::DEFAULT as BORDER;`
  - `pub use semantic::border::HOVER as BORDER_HOVER;`
  - `pub use semantic::border::FOCUS as BORDER_FOCUS;`
  - `pub use semantic::diagnostic::PHASE_PARSE as DIAG_PHASE_PARSE;`
  - `pub use semantic::diagnostic::PHASE_RESOLVE as DIAG_PHASE_RESOLVE;`
  - `pub use semantic::diagnostic::PHASE_COMPILE as DIAG_PHASE_COMPILE;`
  - `pub use semantic::status::PLAYING_TEXT;`
  - `pub use semantic::status::DIAGNOSTIC_ERROR as DIAGNOSTIC_RED;`
  - `pub use semantic::status::DIAGNOSTIC_WARNING as DIAGNOSTIC_AMBER;`
  - `pub use semantic::curve::GREEN as CURVE_GREEN;`
  - `pub use semantic::curve::BLUE as CURVE_BLUE;`
  - `pub use semantic::curve::GRAY as CURVE_GRAY;`
  - `pub use semantic::timeline::KF_FLASH;`
  - `pub use semantic::editor::SNIPPET_BLUE;`
- Flat color-function wrappers:
  - `ghost_prev() -> semantic::canvas::ghost_prev()`
  - `ghost_next() -> semantic::canvas::ghost_next()`
  - `grid_line() -> semantic::canvas::grid_line()`
  - `guide_line() -> semantic::canvas::guide_line()`
  - `hatch_line() -> semantic::canvas::hatch_line()`
  - `snap_guide_line() -> semantic::canvas::snap_guide_line()`
  - `snap_guide_label_bg() -> semantic::canvas::snap_guide_label_bg()`
  - `track_block_1() -> semantic::timeline::track_block_1()`
  - `track_block_2() -> semantic::timeline::track_block_2()`
  - `track_block_3() -> semantic::timeline::track_block_3()`
  - `track_block_4() -> semantic::timeline::track_block_4()`
  - `track_block_5() -> semantic::timeline::track_block_5()`
  - `loop_region() -> semantic::timeline::loop_region()`
  - `transition_stripe_1() -> semantic::timeline::transition_stripe_1()`
  - `transition_stripe_2() -> semantic::timeline::transition_stripe_2()`
  - `transition_stripe_3() -> semantic::timeline::transition_stripe_3()`
  - `transition_stripe_4() -> semantic::timeline::transition_stripe_4()`
  - `transition_stripe_5() -> semantic::timeline::transition_stripe_5()`
  - `transition_stripe_6() -> semantic::timeline::transition_stripe_6()`
  - `overlay_backdrop() -> semantic::overlay::backdrop()`
  - `floating_card_bg() -> semantic::surface::floating_card_bg()`
  - `accent_faint() -> semantic::accent::faint()`
  - `accent_ghost() -> semantic::accent::ghost()`
  - `accent_subtle() -> semantic::accent::subtle()`
  - `accent_hover() -> semantic::accent::hover()`
  - `accent_strong() -> semantic::accent::strong()`
  - `accent_selection() -> semantic::accent::selection()`
  - `text_faint() -> semantic::text::faint()`
  - `text_subtle() -> semantic::text::subtle()`
  - `text_hover() -> semantic::text::hover()`
  - `text_dim() -> semantic::text::dim()`
  - `amber_subtle() -> semantic::status::warning_subtle()`
  - `green_faint() -> semantic::status::success_faint()`
  - `green_ultra_faint() -> semantic::status::success_ultra_faint()`
  - `red_faint() -> semantic::status::error_faint()`
  - `red_ultra_faint() -> semantic::status::error_ultra_faint()`
  - `badge_bg() -> semantic::overlay::badge_bg()`
  - `tooltip_bg() -> semantic::overlay::tooltip_bg()`
  - `row_alt() -> semantic::timeline::row_alt()`
  - `shadow_ambient() -> semantic::overlay::shadow_ambient()`
  - `shadow_direct() -> semantic::overlay::shadow_direct()`
- Flat spatial aliases:
  - `pub use spatial::SPACE_1 as SPACE_XS;`
  - `pub use spatial::SPACE_2 as SPACE_S;`
  - `pub use spatial::SPACE_3 as SPACE_M;`
  - `pub use spatial::SPACE_4 as SPACE_L;`
  - `pub use spatial::SPACE_5 as SPACE_XL;`
  - `pub use spatial::SPACE_1 as PAD_XS;`
  - `pub use spatial::SPACE_2 as PAD_S;`
  - `pub use spatial::SPACE_3 as PAD_M;`
  - `pub use spatial::SPACE_4 as PAD_L;`
  - `pub use spatial::SPACE_5 as PAD_XL;`
  - `pub use spatial::SPACE_6 as PAD_XXL;`
  - `pub use spatial::{ROW_XS, ROW_S, ROW_M, ROW_L};`
  - `pub use spatial::{STROKE_WIDTH, STROKE_WIDTH_THICK, STROKE_WIDTH_THIN};`
  - `pub use spatial::{RADIUS_S, RADIUS_M, RADIUS_L, RADIUS_XL};`
- Flat domain size aliases:
  - `pub use spatial::preview::ROTATION_OFFSET as PREVIEW_ROTATION_OFFSET;`
  - `pub use spatial::preview::ROTATION_RADIUS as PREVIEW_ROTATION_RADIUS;`
  - `pub use spatial::preview::HANDLE_SIZE as PREVIEW_HANDLE_SIZE;`
  - `pub use spatial::preview::HANDLE_HIT_RADIUS as PREVIEW_HANDLE_HIT_RADIUS;`
  - `pub use spatial::preview::MIN_ACTOR_SIZE as PREVIEW_MIN_ACTOR_SIZE;`
  - `pub use spatial::preview::MIN_SCALE as PREVIEW_MIN_SCALE;`
  - `pub use spatial::preview::MIN_ZOOM as PREVIEW_MIN_ZOOM;`
  - `pub use spatial::preview::DASH_LEN as PREVIEW_DASH_LEN;`
  - `pub use spatial::preview::GAP_LEN as PREVIEW_GAP_LEN;`
  - `pub use spatial::preview::CROSS_SIZE as PREVIEW_CROSS_SIZE;`
  - `pub use spatial::preview::VERTEX_HIT_BUFFER as PREVIEW_VERTEX_HIT_BUFFER;`
  - `pub use spatial::preview::ROTATION_HIT_BUFFER as PREVIEW_ROTATION_HIT_BUFFER;`
  - `pub use spatial::toolbar::HEIGHT as TOOLBAR_HEIGHT;`
  - `pub use spatial::timeline::LABEL_COL_WIDTH as TIMELINE_LABEL_COL_WIDTH;`
  - `pub use spatial::timeline::TRACK_ROW_HEIGHT as TIMELINE_TRACK_ROW_HEIGHT;`
  - `pub use spatial::timeline::RULER_HEIGHT as TIMELINE_RULER_HEIGHT;`
  - `pub use spatial::timeline::RANGE_HEIGHT as TIMELINE_RANGE_HEIGHT;`
  - `pub use spatial::timeline::KF_HALF as TIMELINE_KF_HALF;`
  - `pub use spatial::timeline::PLAYBACK_STRIP_HEIGHT as TIMELINE_PLAYBACK_STRIP_HEIGHT;`
  - `pub use spatial::menu::MIN_WIDTH as MENU_MIN_WIDTH;`
  - `pub use spatial::menu::ICON_WIDTH as MENU_ICON_WIDTH;`
  - `pub use spatial::menu::CHECK_WIDTH as MENU_CHECK_WIDTH;`
  - `pub use spatial::menu::SHADOW_OFFSET_Y as MENU_SHADOW_OFFSET_Y;`
  - `pub use spatial::menu::SHADOW_BLUR as MENU_SHADOW_BLUR;`
  - `pub use spatial::welcome::BTN_HEIGHT as WELCOME_BTN_HEIGHT;`
  - `pub use spatial::welcome::TOP_OFFSET_FRAC as WELCOME_TOP_OFFSET_FRAC;`
  - `pub use spatial::component::PILL_TAB_HEIGHT;`
  - `pub use spatial::component::PILL_TAB_GAP;`
  - `pub use spatial::component::TOAST_WIDTH;`
  - `pub use spatial::component::TOAST_HEIGHT;`
  - `pub use spatial::component::TOAST_SPACING;`
  - `pub use spatial::component::TOAST_MARGIN;`
  - `pub use spatial::component::ICON_SLOT_WIDTH;`
  - `pub use spatial::inspector::KF_COL_WIDTH as INSPECTOR_KF_COL_WIDTH;`
  - `pub use spatial::inspector::LABEL_MIN_WIDTH as INSPECTOR_LABEL_MIN_WIDTH;`
  - `pub use spatial::inspector::LABEL_MAX_WIDTH as INSPECTOR_LABEL_MAX_WIDTH;`
  - `pub use spatial::inspector::COL_GAP as INSPECTOR_COL_GAP;`
  - `pub use spatial::inspector::INPUT_WIDTH_FLOAT as INSPECTOR_INPUT_WIDTH_FLOAT;`
  - `pub use spatial::inspector::INPUT_COL_WIDTH as INSPECTOR_INPUT_COL_WIDTH;`
  - `pub use spatial::inspector::INPUT_WIDTH_VEC2 as INSPECTOR_INPUT_WIDTH_VEC2;`
  - `pub use spatial::inspector::INPUT_WIDTH_SLIDER as INSPECTOR_INPUT_WIDTH_SLIDER;`
  - `pub use spatial::inspector::INPUT_WIDTH_COLOR as INSPECTOR_INPUT_WIDTH_COLOR;`
  - `pub use spatial::inspector::ROW_HEIGHT as INSPECTOR_ROW_HEIGHT;`
  - `pub use spatial::inspector::KF_BTN_WIDTH as INSPECTOR_KF_BTN_WIDTH;`
  - `pub use spatial::inspector::LABEL_WIDTH_FRAC as INSPECTOR_LABEL_WIDTH_FRAC;`
- Flat typography aliases:
  - `pub use typography::{FONT_SIZE_XS, FONT_SIZE_S, FONT_SIZE_M, FONT_SIZE_L, FONT_SIZE_XL};`

### Final `crates/animatix-gui/src/app/design_tokens/mod.rs`
- Create only after all old flat imports and aliases are gone.
- Contents:
  - `mod primitive;`
  - `pub mod semantic;`
  - `pub mod spatial;`
  - `pub mod typography;`
  - `pub mod motion;`
  - `pub mod util;`
- Do not re-export legacy flat names from `mod.rs` after Phase 1 completes.

## Exact `runtime.rs::install_theme` Changes
1. Replace `use crate::app::design_tokens::*;` in `crates/animatix-gui/src/app/runtime.rs` with narrow imports:
   - `use crate::app::design_tokens::semantic::{accent, border, surface, text};`
   - `use crate::app::design_tokens::spatial::{self, component::ICON_SLOT_WIDTH};`
2. Update `AnimatixApp::clear_color`:
   - Replace `BG_BASE.to_normalized_gamma_f32()` with `surface::BASE.to_normalized_gamma_f32()`.
3. Remove `const WIDGET_HOVER: Color32 = BG_HOVER;`.
4. Replace spacing values:
   - `Vec2::new(PAD_S, PAD_S)` -> `Vec2::new(spatial::SPACE_2, spatial::SPACE_2)`
   - `Vec2::new(PAD_L, PAD_S)` -> `Vec2::new(spatial::SPACE_4, spatial::SPACE_2)`
   - `egui::Margin::same(PAD_L as i8)` -> `egui::Margin::same(spatial::SPACE_4 as i8)`
   - `style.spacing.indent = ICON_SLOT_WIDTH` -> `style.spacing.indent = ICON_SLOT_WIDTH`
5. Replace global fills:
   - `panel_fill = BG_PANEL` -> `surface::PANEL`
   - `window_fill = BG_PANEL` -> `surface::PANEL`
   - `extreme_bg_color = BG_BASE` -> `surface::BASE`
   - `faint_bg_color = BG_SURFACE` -> `surface::SURFACE`
6. Replace widget state tokens:
   - Noninteractive `bg_fill` and `weak_bg_fill`: `surface::SURFACE`
   - Noninteractive `bg_stroke`: `Stroke::new(spatial::STROKE_WIDTH, border::DEFAULT)`
   - Noninteractive `fg_stroke`: `Stroke::new(spatial::STROKE_WIDTH, text::SECONDARY)`
   - Inactive `bg_fill` and `weak_bg_fill`: `surface::WIDGET`
   - Inactive `bg_stroke`: `Stroke::new(spatial::STROKE_WIDTH, border::DEFAULT)`
   - Inactive `fg_stroke`: `Stroke::new(spatial::STROKE_WIDTH, text::PRIMARY)`
   - Hovered `bg_fill` and `weak_bg_fill`: `surface::HOVER`
   - Hovered `bg_stroke`: `Stroke::new(spatial::STROKE_WIDTH, border::FOCUS)`
   - Hovered `fg_stroke`: `Stroke::new(spatial::STROKE_WIDTH, text::PRIMARY)`
   - Active `bg_fill` and `weak_bg_fill`: `surface::ACTIVE`
   - Active `bg_stroke`: `Stroke::new(spatial::STROKE_WIDTH, border::FOCUS)`
   - Active `fg_stroke`: `Stroke::new(spatial::STROKE_WIDTH, text::PRIMARY)`
7. Replace all hardcoded theme corner radius `egui::CornerRadius::same(4)` with `egui::CornerRadius::same(spatial::RADIUS_M as u8)`.
8. Replace selection:
   - `selection.bg_fill = accent_selection()` -> `accent::selection()`
   - `selection.stroke = Stroke::new(STROKE_WIDTH, ACCENT_BLUE)` -> `Stroke::new(spatial::STROKE_WIDTH, accent::PRIMARY)`
9. Replace text override:
   - `override_text_color = Some(TEXT_PRIMARY)` -> `Some(text::PRIMARY)`
10. Remove the final overwrite `style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(STROKE_WIDTH, BG_WIDGET)` unless a visual smoke test proves it is required; if required, use `border::DEFAULT` or add a separate `separator` semantic token instead of mutating noninteractive border twice.
11. Verification: run `cargo check -p animatix-gui`, then visually smoke-test panel fill, window fill, button hover/active, text color, selection fill, and focus strokes.

## Phase 1 Migration Order

1. Create token submodules behind the existing facade: add `design_tokens/{primitive.rs,semantic.rs}` and replace only the top of `design_tokens.rs` with module declarations plus color aliases/wrappers; expected outcome is no call-site edits yet and flat imports still compile; verify with `cargo check -p animatix-gui`.
2. Add non-color token modules: add `design_tokens/{spatial.rs,typography.rs,motion.rs,util.rs}` and move spacing, dimensions, font sizes, and utility functions behind facade aliases; expected outcome is `design_tokens.rs` contains no raw token values except wrappers; verify with `cargo check -p animatix-gui` and `rg "Color32::from_rgb|pub const SPACE_|pub const PAD_|pub fn lerp_color" crates/animatix-gui/src/app/design_tokens.rs`.
3. Migrate global theme only: update `runtime.rs::install_theme` and `AnimatixApp::clear_color` to import `semantic::{accent,border,surface,text}` and `spatial`; expected outcome is the egui global style consumes the new token API while the rest of the GUI still uses facade aliases; verify with `cargo check -p animatix-gui`.
4. Migrate shared components in small batches: update `components/layout.rs`, `components/button.rs`, `components/context_menu.rs`; then `components/{row.rs,timeline.rs,diagnostics.rs}`; then `components/{easing_curve_editor.rs,toast.rs}` to use semantic/spatial/typography paths; expected outcome is reusable chrome no longer depends on flat token names; verify with `cargo check -p animatix-gui` after each batch and `rg "design_tokens::\*" crates/animatix-gui/src/app/components`.
5. Migrate shell chrome and app-level overlays in small batches: update `shell/{toolbar.rs,settings.rs,export_dialog.rs}`; then `shell/{command_palette.rs,find_replace.rs,insertion_palette.rs,shortcut_cheat_sheet.rs}`; then `app/mod.rs` and `app/utils.rs`; expected outcome is dialogs, toolbar, status/welcome overlays, badges, and utility drawing use semantic/spatial paths; verify with `cargo check -p animatix-gui` after each batch.
6. Migrate panels and canvas visuals in small batches: update `panels/{behavior.rs,preview_panel.rs,timeline_panel.rs,sidebar.rs}`; then `panels/inspector/{mod.rs,property_groups.rs,keyframe_table.rs}`; then `panels/inspector/{graph_editor.rs,spreadsheet.rs}`; then `preview/{mod.rs,context.rs,drag_handler.rs}`; then `preview/{grid.rs,overlay.rs,property_popup.rs,selection.rs,time_lens.rs}`; expected outcome is preview-only colors come from `semantic::canvas`, timeline colors from `semantic::timeline`, category colors from `semantic::category`, and warning/error colors from `semantic::status`; verify with `cargo check -p animatix-gui` after each batch.
7. Migrate editor-adjacent token users: update `completion_popup.rs` and `cell_editor/render.rs`; expected outcome is editor UI chrome uses `semantic::surface/text/accent/status/editor` while syntax-specific colors remain local or are explicitly mapped to `semantic::editor`; verify with `cargo check -p animatix-gui` and `rg "design_tokens" crates/animatix-gui/src/completion_popup.rs crates/animatix-gui/src/cell_editor/render.rs`.
8. Remove facade imports and flat names: replace remaining `use crate::app::design_tokens::*;`, `use crate::app::design_tokens::{...}`, `crate::app::design_tokens::...`, and `dt::...` with narrow module imports; expected outcome is no production call site depends on legacy aliases; verify with `rg "use crate::app::design_tokens::\*|design_tokens::\{|design_tokens as dt|BG_|TEXT_|ACCENT_BLUE|ACCENT_CYAN|GREEN|AMBER|RED|PURPLE|PAD_|FONT_SIZE_" crates/animatix-gui/src` and `cargo check -p animatix-gui`.
9. Finalize module layout: move the surviving module declarations from `design_tokens.rs` into `design_tokens/mod.rs`, delete `design_tokens.rs`, and delete all legacy alias/wrapper exports; expected outcome is the target `app/design_tokens/` directory is the only token module; verify with `find crates/animatix-gui/src/app -maxdepth 2 -name 'design_tokens*' -print`, `rg "BG_|TEXT_|ACCENT_BLUE|PAD_|FONT_SIZE_" crates/animatix-gui/src`, and `cargo check -p animatix-gui`.
10. Final Phase 1 validation: run `cargo test -p animatix-gui` if time allows; manually smoke-test the shell, welcome screen, settings, export dialog, command palette, insertion palette, find/replace, sidebar, inspector, preview canvas selection/handles/guides, timeline scrubbing, diagnostics, completion popup, and cell editor hover states.

## Files to Touch
- `crates/animatix-gui/src/app/design_tokens.rs` — temporary compatibility facade, then removed in final checkpoint.
- `crates/animatix-gui/src/app/design_tokens/mod.rs` — final token module entry point created only after removing `design_tokens.rs`.
- `crates/animatix-gui/src/app/design_tokens/primitive.rs` — raw palette and domain raw colors.
- `crates/animatix-gui/src/app/design_tokens/semantic.rs` — public semantic color modules.
- `crates/animatix-gui/src/app/design_tokens/spatial.rs` — spacing, row heights, radii, strokes, preview/timeline/menu/welcome/component/inspector dimensions.
- `crates/animatix-gui/src/app/design_tokens/typography.rs` — `TextRole` and temporary font-size constants.
- `crates/animatix-gui/src/app/design_tokens/motion.rs` — duration and easing tokens.
- `crates/animatix-gui/src/app/design_tokens/util.rs` — `lerp_color` and `multiply_alpha`.
- `crates/animatix-gui/src/app/runtime.rs` — global egui style installer and clear color.
- `crates/animatix-gui/src/app/components/{layout.rs,button.rs,context_menu.rs,row.rs,timeline.rs,diagnostics.rs,easing_curve_editor.rs,toast.rs}` — shared component token imports and semantic path migration.
- `crates/animatix-gui/src/app/shell/{toolbar.rs,settings.rs,export_dialog.rs,command_palette.rs,find_replace.rs,insertion_palette.rs,shortcut_cheat_sheet.rs}` — shell chrome, overlays, and dialogs token imports.
- `crates/animatix-gui/src/app/{mod.rs,utils.rs,tests.rs}` — app-level overlays, welcome/status UI, utility drawing, and token tests.
- `crates/animatix-gui/src/app/panels/{behavior.rs,preview_panel.rs,timeline_panel.rs,sidebar.rs}` — panel token imports and semantic role migration.
- `crates/animatix-gui/src/app/panels/inspector/{mod.rs,property_groups.rs,keyframe_table.rs,graph_editor.rs,spreadsheet.rs}` — inspector layout and color token migration.
- `crates/animatix-gui/src/app/preview/{mod.rs,context.rs,drag_handler.rs,grid.rs,overlay.rs,property_popup.rs,selection.rs,time_lens.rs}` — canvas-specific token migration.
- `crates/animatix-gui/src/completion_popup.rs` — completion popup token migration.
- `crates/animatix-gui/src/cell_editor/render.rs` — cell editor token migration.

## Risks
- Rust module conflict: `design_tokens.rs` and `design_tokens/mod.rs` cannot coexist; sequence the facade-to-directory move last.
- Alpha const risk: attempting `pub const` for alpha-tinted `Color32::from_rgba_unmultiplied` may fail in egui 0.34; keep alpha tokens as functions unless verified.
- Semantic ambiguity: old `GREEN`, `AMBER`, `RED`, and `PURPLE` mix status, category, timeline, and canvas meanings; migrate by meaning, not by one global find-replace.
- Visual drift: facade aliases immediately pointing at spec surface/text values will darken/lighten the entire GUI; verify contrast and screenshots after `runtime.rs` migration.
- Contrast regressions: spec `text::MUTED`/`text::DISABLED` must be checked on `surface::BASE`, `surface::SURFACE`, and `surface::WIDGET`; disabled text may intentionally fail normal-text AA only for disabled controls.
- Name collisions: `category::TEXT` may be confused with module `text`; import modules by path (`semantic::{category, text}`) rather than globbing both into local scope.
- Circular dependencies: `semantic.rs` may use `primitive`, but `primitive.rs` must not import `semantic`; `spatial.rs`, `typography.rs`, and `motion.rs` should not import `semantic`.
- Final alias removal: `app/tests.rs` imports `DIAGNOSTIC_RED` directly, so tests must be migrated with production code or final cleanup will fail.
- Behavior risk in theme cleanup: removing the final `noninteractive.bg_stroke` overwrite in `install_theme` may alter separators; visually verify menus, cards, panels, and disabled widgets.
- Component-token scope: `MENU_*`, `TOAST_*`, `PILL_TAB_*`, and inspector dimensions are component-level by the spec, but Phase 1 keeps them in `spatial.rs` for compatibility; move them beside components only in a later phase if desired.
