# Phase 2: Component Unification — Implementation Plan

## Goal
Replace the four ad-hoc button free functions with a single `Button` widget implementing `egui::Widget`, and migrate all raw `FontId::new(...)`/`FONT_SIZE_*` usage to the `TextRole` type scale, removing `to_uppercase()` visual-hierarchy hacks.

## Assumptions / blockers (read first)
1. **No `MonoMicro` role exists.** `timeline_panel.rs` uses `FontId::monospace(FONT_SIZE_XS)` (10px mono) ~9 times. `TextRole::Mono` is 12px only; there is no 10px monospace role. **Decision needed**: either (a) add a `TextRole` variant for 10px mono, or (b) leave 10px-mono call sites on a raw `FontId::monospace(10.0)` with an inline comment. This plan assumes (b) — keep 10px-mono raw and annotate — to avoid changing the type scale defined in `docs/gui_design_language.md §3.1`. Confirm before starting Task 8.
2. **`FONT_SIZE_XL * 1.5`** (`app/mod.rs:774`, welcome title, = 27px) has no exact role. Closest is `TextRole::Display` (20px). This is a **visible size change**. This plan keeps it as a raw `.size(27.0)` with a comment unless the design owner wants Display. Confirm.
3. **`RichText::size()` vs `FontId`.** Many sites are `RichText::new(x).size(FONT_SIZE_S)` (family stays proportional default) — for these, replacing with `TextRole` means `.size(TextRole::BodyS.size())` or a new `RichText` helper. Sites that build a full `FontId::new(size, family)` map directly to `TextRole::X.font_id()`. Task 7 introduces a `components/text.rs` helper so both forms are concise.
4. `icon_button_colored` (sidebar eye/lock) needs **custom icon + hover colors**; the `Button` API in §6.2 has no color builder. This plan adds `.icon_color(Color32)` + `.hover_icon_color(Color32)` builders.
5. `toolbar_toggle_button`'s **active state draws an accent underline**; `Button` must reproduce this for the `Ghost` variant when `active(true)`.
6. `toolbar_separator` and `play_pause_icon` are **kept** (not in the delete list); only the 4 named functions are removed. Per §6.1 the `Separator` primitive replaces `toolbar_separator` later — out of scope for Phase 2.

---

## Plan

### Task 1 — Add `Button`, `ButtonVariant`, `ButtonSize` types + `Widget` impl
- **File**: `crates/animatix-gui/src/app/components/button.rs`
- **Change**: Add (do not yet remove old functions) a `Button` struct, `ButtonVariant { Primary, Secondary, Ghost, Icon }`, `ButtonSize { Small, Medium, Large }`, builders (`primary`, `secondary`, `ghost`, `icon`, `small`, `large`, `with_icon`, `with_label`, `with_tooltip`, `disabled`, `active`, `icon_color`, `hover_icon_color`, `show_label`), and `impl egui::Widget for Button`. Port the measure + paint logic from `toolbar_toggle_button`/`toolbar_action_button`/`icon_button`/`icon_button_colored` into one state machine: bg = `surface::ACTIVE` (active/pressed) / `surface::HOVER` (hover) / transparent; icon color = `accent::PRIMARY` (active) / `text::PRIMARY` (hover) / `text::SECONDARY`; accent underline when `Ghost` + `active`; focus ring via `rect_stroke`. `ButtonSize` maps to `ROW_S`/`ROW_M`/`ROW_L` heights; `Icon` variant is square `ROW_L`. Use `TextRole::BodyS` for label font and `TextRole::Body` for icon font (replacing the current `FONT_SIZE_S`/`FONT_SIZE_M`).
- **Outcome**: New API compiles next to old functions; nothing calls it yet.
- **Verify**: `cargo check -p animatix-gui`.
- **Depends on**: none.

### Task 2 — Migrate `shell/toolbar.rs` to `Button`
- **File**: `crates/animatix-gui/src/app/shell/toolbar.rs` (lines ~335, 345, 355, 370)
- **Change**:
  - `button::icon_button(ui, COMMAND, "Keyboard shortcuts")` → `ui.add(Button::icon(COMMAND).with_tooltip("Keyboard shortcuts"))`
  - `button::icon_button(ui, GEAR, "Settings")` → `ui.add(Button::icon(GEAR).with_tooltip("Settings"))`
  - both `button::toolbar_toggle_button(ui, ICON, None, tip, active, show_label)` → `ui.add(Button::ghost("").with_icon(ICON).with_tooltip(tip).active(active))` (drop label since `None`; preserve `show_label=false`/`has_diagnostics` semantics — note `has_diagnostics` was the `show_label` arg, verify it was intended as label visibility and not enable-state; preserve existing behavior exactly).
- **Outcome**: Toolbar uses unified `Button`; visuals unchanged.
- **Verify**: `cargo check -p animatix-gui`; visual smoke test toolbar.
- **Depends on**: Task 1.

### Task 3 — Migrate `panels/timeline_panel.rs` transport buttons to `Button`
- **File**: `crates/animatix-gui/src/app/panels/timeline_panel.rs` (import line 24; calls 311, 316, 321, 326, 331, 336, 341, 345, 362, 374, 381, 392, 399)
- **Change**: Replace `use ...button::{play_pause_button, toolbar_action_button, toolbar_separator, toolbar_toggle_button};` with `use ...button::{play_pause_icon, toolbar_separator, Button};`.
  - `toolbar_action_button(ui, ICON, None, tip, false)` → `ui.add(Button::ghost("").with_icon(ICON).with_tooltip(tip))`
  - `play_pause_button(ui, is_playing)` → `ui.add(Button::icon(play_pause_icon(is_playing)).with_tooltip("Play/Pause (Space)"))`
  - `toolbar_toggle_button(ui, ICON, None, tip, active, false)` → `ui.add(Button::ghost("").with_icon(ICON).with_tooltip(tip).active(active))`
  - `toolbar_separator(ui)` → unchanged.
- **Outcome**: Timeline transport row uses `Button`.
- **Verify**: `cargo check -p animatix-gui`; smoke test transport controls.
- **Depends on**: Task 1.

### Task 4 — Migrate `panels/sidebar.rs` eye/lock buttons to `Button`
- **File**: `crates/animatix-gui/src/app/panels/sidebar.rs` (lines ~700, 710)
- **Change**: `button::icon_button_colored(ui, icon, tip, color, hover)` → `ui.add(Button::icon(icon).with_tooltip(tip).icon_color(color).hover_icon_color(semantic_text_primary))`. Keep `.clicked()` checks intact.
- **Outcome**: Layer visibility/lock toggles use `Button`.
- **Verify**: `cargo check -p animatix-gui`; smoke test sidebar eye/lock.
- **Depends on**: Task 1.

### Task 5 — Migrate `preview/property_popup.rs` close button to `Button`
- **File**: `crates/animatix-gui/src/app/preview/property_popup.rs` (line ~116)
- **Change**: `button::icon_button(ui, X, "Close")` → `ui.add(Button::icon(X).with_tooltip("Close"))`.
- **Outcome**: Popup close button uses `Button`.
- **Verify**: `cargo check -p animatix-gui`.
- **Depends on**: Task 1.

### Task 6 — Delete the four old button free functions
- **File**: `crates/animatix-gui/src/app/components/button.rs`
- **Change**: Remove `icon_button`, `icon_button_colored`, `toolbar_toggle_button`, `toolbar_action_button`. **Keep** `play_pause_icon` and `toolbar_separator`. Remove now-unused imports (`FONT_SIZE_M`, `FONT_SIZE_S`).
- **Outcome**: Only the unified `Button` (+ kept helpers) remains.
- **Verify**: `cargo check -p animatix-gui` (must be 0 errors → proves Tasks 2–5 covered every call site); `cargo test -p animatix-gui`.
- **Depends on**: Tasks 2, 3, 4, 5 (all call sites migrated).

### Task 7 — Add `components/text.rs` TextRole helpers + register module
- **Files**: `crates/animatix-gui/src/app/components/text.rs` (new); `crates/animatix-gui/src/app/components/mod.rs`
- **Change**: Create `text.rs` with helpers to keep call sites concise:
  - `pub fn rich(role: TextRole, t: impl Into<String>) -> egui::RichText` → `RichText::new(t).font(role.font_id())`.
  - An extension trait `RichTextExt { fn role(self, r: TextRole) -> Self }` impl for `egui::RichText` setting `.font(r.font_id())`, so existing `.size(FONT_SIZE_S)` sites become `.role(TextRole::BodyS)`.
  - Re-export `TextRole`. Add `pub mod text;` to `components/mod.rs`.
- **Outcome**: A single ergonomic path for typography migration.
- **Verify**: `cargo check -p animatix-gui`.
- **Depends on**: none (can run parallel to Task 1).

### Task 8 — TextRole sweep: `components/` files
- **Files**: `components/layout.rs`, `components/diagnostics.rs`, `components/toast.rs`, `components/row.rs`, `components/context_menu.rs`, `components/button.rs` (label/icon fonts from Task 1)
- **Change**: Replace `FontId::new(FONT_SIZE_XS, Proportional)` → `TextRole::Micro.font_id()`, `FONT_SIZE_S`→`BodyS`, `FONT_SIZE_M`→`Body`, `FONT_SIZE_L`→`Title`, `FONT_SIZE_XL`→`Heading`. Replace `RichText::new(..).size(FONT_SIZE_*)` via `.role(...)` from Task 7. Remove `FONT_SIZE_*` imports per file. Mono: `FontId::monospace(FONT_SIZE_S)`→`TextRole::Mono.font_id()`.
- **Outcome**: Component layer uses `TextRole`.
- **Verify**: `cargo check -p animatix-gui`; `cargo test -p animatix-gui`.
- **Depends on**: Task 7 (and Task 1/6 for `button.rs`).

### Task 9 — TextRole sweep: `shell/` files
- **Files**: `shell/toolbar.rs`, `shell/settings.rs`, `shell/export_dialog.rs`, `shell/insertion_palette.rs`, `shell/shortcut_cheat_sheet.rs`, `shell/find_replace.rs`, `shell/command_palette.rs`
- **Change**: Same mapping as Task 8 across all `FONT_SIZE_*`/`FontId::new` sites; remove imports.
- **Outcome**: Shell dialogs use `TextRole`.
- **Verify**: `cargo check -p animatix-gui`; `cargo test -p animatix-gui`.
- **Depends on**: Task 7.

### Task 10 — TextRole sweep: `panels/` (incl. `inspector/`)
- **Files**: `panels/sidebar.rs`, `panels/preview_panel.rs`, `panels/timeline_panel.rs`, `panels/inspector/mod.rs`, `panels/inspector/spreadsheet.rs`, `panels/inspector/graph_editor.rs`, `panels/inspector/keyframe_table.rs`, `panels/inspector/property_groups.rs`
- **Change**: Same mapping. **Special cases** (per assumption 1): `timeline_panel.rs` `FontId::monospace(FONT_SIZE_XS)` (10px mono) stays raw → `FontId::monospace(10.0)` with `// 10px mono: no TextRole; see phase2_plan.md` comment, OR migrate after a decision to add a role. `panels/inspector/property_groups.rs:818` multiline `FontId::new(...)` — inspect and map.
- **Outcome**: Panels use `TextRole` except documented mono-10 exceptions.
- **Verify**: `cargo check -p animatix-gui`; `cargo test -p animatix-gui`.
- **Depends on**: Task 7.

### Task 11 — TextRole sweep: `preview/` + `app/mod.rs` + `app/utils.rs`
- **Files**: `preview/context.rs`, `preview/property_popup.rs`, `preview/mod.rs`, `preview/time_lens.rs`, `preview/selection.rs`, `preview/overlay.rs`, `app/mod.rs`, `app/utils.rs`
- **Change**: Same mapping. **Special cases**: `app/mod.rs:766` `FontId::proportional(28.0)` and `app/mod.rs:774` `.size(FONT_SIZE_XL * 1.5)` (welcome title, 27px) — per assumption 2 keep raw with comment unless owner picks `Display`. `preview/overlay.rs` raw `FontId::monospace(11.0/10.0/9.0)` — no roles; keep raw with comment.
- **Outcome**: Preview + top-level UI on `TextRole` except documented exceptions.
- **Verify**: `cargo check -p animatix-gui`; `cargo test -p animatix-gui`.
- **Depends on**: Task 7.

### Task 12 — Remove `to_uppercase()` from section headers
- **Files**: `components/layout.rs` (`section_header`, line ~88 `title.to_uppercase()`), `components/context_menu.rs` (`render_menu_header`, line ~358 `text.to_uppercase()`)
- **Change**: Drop `.to_uppercase()`; render the raw `title`/`text`. To preserve hierarchy per §3.3, headers already use `text::MUTED`; switch the section-header title font from `Micro` to `TextRole::Caption` (or keep `Micro` and rely on muted color) — pick one and apply consistently to both headers. Update the `MenuEntry::Header` doc comment ("muted, uppercase, small") to drop "uppercase".
- **Outcome**: No uppercasing; hierarchy via weight/color only.
- **Verify**: `cargo check -p animatix-gui`; `cargo test -p animatix-gui`; smoke test sidebar section headers + context-menu headers.
- **Depends on**: Task 8 (these files already touched there; can be folded into Task 8 if preferred).

### Task 13 — Delete `FONT_SIZE_*` legacy constants
- **File**: `crates/animatix-gui/src/app/design_tokens/typography.rs`
- **Change**: Remove the five `pub const FONT_SIZE_*` and the doc-comment reference. (Only after Tasks 8–11 leave zero references.)
- **Outcome**: `TextRole` is the sole typography API.
- **Verify**: `grep -rn FONT_SIZE_ crates/animatix-gui/src` returns nothing; `cargo check -p animatix-gui`; `cargo test --no-fail-fast`.
- **Depends on**: Tasks 8, 9, 10, 11.

### Task 14 — Docs + roadmap
- **Files**: `docs/gui_design_language.md` (mark Phase 2 done / note mono-10 + welcome-27 exceptions), `docs/roadmap.md` (remove completed Phase 2 items)
- **Change**: Record the unified `Button` API as implemented; document the 10px-mono and 27px-title exceptions and any new `TextRole`/text helper.
- **Verify**: manual read.
- **Depends on**: Tasks 6, 13.

---

## Files to touch
- `components/button.rs` — add `Button`/variants/Widget; delete 4 free fns; keep `play_pause_icon`, `toolbar_separator`.
- `components/text.rs` (new) — `TextRole` `RichText` helpers.
- `components/mod.rs` — register `text` module.
- `components/{layout,diagnostics,toast,row,context_menu}.rs` — TextRole sweep + `to_uppercase` removal.
- `shell/{toolbar,settings,export_dialog,insertion_palette,shortcut_cheat_sheet,find_replace,command_palette}.rs` — Button migration (toolbar) + TextRole sweep.
- `panels/sidebar.rs`, `panels/timeline_panel.rs`, `panels/preview_panel.rs`, `panels/inspector/{mod,spreadsheet,graph_editor,keyframe_table,property_groups}.rs` — Button migration + TextRole sweep.
- `preview/{context,property_popup,mod,time_lens,selection,overlay}.rs` — Button (popup) + TextRole sweep.
- `app/mod.rs`, `app/utils.rs` — TextRole sweep.
- `design_tokens/typography.rs` — delete `FONT_SIZE_*` constants.
- `docs/gui_design_language.md`, `docs/roadmap.md` — status.

## Risks
1. **Behavioral parity of `Button`**: the old toggle/action/icon functions have subtly different bg/color/underline/focus logic. The single state machine must reproduce each variant exactly or toolbar visuals will shift. Mitigate by porting each branch verbatim and smoke-testing toolbar + transport + sidebar.
2. **`show_label`/`has_diagnostics` argument in `toolbar.rs`**: confirm whether the boolean passed to `toolbar_toggle_button` was label-visibility or enable-state before mapping; mis-mapping changes diagnostics-button behavior.
3. **No 10px-mono / 27px roles** (assumptions 1–2): leaving them raw means `FONT_SIZE_*` deletion (Task 13) is only safe after confirming those sites were converted to literal sizes, not the deleted constants. Grep gate in Task 13 catches this.
4. **`RichText::size()` ≠ `font()`**: `.size(x)` keeps default family; `.font(role.font_id())` sets family too. For proportional-default sites the result is identical, but for any site relying on a non-default family inherited from style, `.font()` could change family. Spot-check mono/proportional intent per site.
5. **Ordering**: Task 6 (delete fns) must follow all Button call-site migrations; Task 13 (delete constants) must follow all TextRole sweeps. Running `cargo check` after each task surfaces missed sites.
6. **Large diff surface (~30 files, 200+ font sites)**: high chance of a missed import or stray constant; rely on per-task `cargo check` + final repo-wide grep.
