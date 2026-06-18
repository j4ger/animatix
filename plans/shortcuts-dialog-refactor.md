# Plan: Refactor Shortcuts Dialog to Pattern B + Fix Diagnostics Tofu

## Goal

Migrate the keyboard shortcuts dialog from Pattern A (hand-drawn overlay) to Pattern B (`egui::Window` with custom frame) so it gains proper inner margins and a robust two-column layout, extract a reusable `dialog` helper to DRY up the 6 duplicated Pattern B sites, and replace the `✓` glyph in the diagnostics "all clear" message with `egui_phosphor::regular::CHECK_CIRCLE`.

## Context (verified)

- Pattern B boilerplate is duplicated across **6 sites**, each repeating the same ~22 lines:
  1. `app/shell/settings.rs:19-47` — `settings_dialog_ui`
  2. `app/shell/find_replace.rs:18-46` — `find_replace_ui`
  3. `app/shell/command_palette.rs:24-57` — `command_palette_ui`
  4. `app/mod.rs:~1058-1085` — `workspace_switcher_ui`
  5. `app/mod.rs:~1170-1197` — `unsaved_changes_dialog_ui`
  6. `app/shell/shortcut_cheat_sheet.rs` — currently Pattern A (the refactor target)
- All Pattern B sites share identical: backdrop fill, Escape close, backdrop-click close, `egui::Window` with `title_bar(false)` + `Frame::new().fill(surface::BASE).stroke(Stroke::new(STROKE_WIDTH, border::DEFAULT)).corner_radius(RADIUS_XL).inner_margin(egui::Margin::same(SPACE_XL as i8))`.
- Differences per-site: window id/title, `default_size`/`min_size`/`max_size`, `resizable`, anchor offset (command_palette uses `[0.0, -80.0]`), `set_min_width`, and the close-flag mutator.
- `components/layout.rs` already hosts `card`, `section_header`, `labeled_row`, `pill_tab_bar` — a `dialog` helper belongs here conceptually, but `card` uses a different surface/shadow. A dedicated `components/dialog.rs` module keeps the modal-overlay concern isolated.
- `egui_phosphor::regular::CHECK_CIRCLE` is already used in `components/toast.rs:76` for success toasts — proven to render.
- `overlay::backdrop()` is `pub fn` in `semantic.rs:247`.
- `GuiShell` methods are `&mut self` and need to mutate close flags; the helper must be a free function taking `&egui::Context` + closure, not a `GuiShell` method (avoids borrow-checker fights with `&mut self` inside the window closure).

## Reusable dialog API

New module `crates/animatix-gui/src/app/components/dialog.rs`:

```rust
use egui::{Align2, Margin, Response, Stroke, Ui, Vec2};

use crate::app::design_tokens::semantic::{border, overlay, surface};
use crate::app::design_tokens::spatial::{RADIUS_XL, SPACE_XL, STROKE_WIDTH};

/// Configuration for a centered modal dialog.
pub struct DialogSpec<'a> {
    /// Used as the egui::Window id_salt — must be unique per open dialog.
    pub id: &'a str,
    /// Heading text rendered in the title row (Pattern B always has a title row).
    pub title: &'a str,
    pub default_size: [f32; 2],
    pub min_size: [f32; 2],
    pub max_size: Option<[f32; 2]>,
    pub resizable: bool,
    /// Anchor offset from CENTER_CENTER; default `[0.0, 0.0]`.
    pub anchor_offset: [f32; 2],
}

impl<'a> DialogSpec<'a> {
    pub fn new(id: &'a str, title: &'a str, default_size: [f32; 2]) -> Self {
        Self {
            id, title, default_size,
            min_size: default_size,
            max_size: None,
            resizable: false,
            anchor_offset: [0.0, 0.0],
        }
    }
}

/// Draws the modal backdrop (full-viewport dim), intercepts Escape + backdrop
/// click to request close, then shows a centered `egui::Window` with the
/// standard Pattern B frame. Returns `false` when the dialog should close.
///
/// `body` receives the window's inner `&mut Ui` and must render the title row
/// (including the X close button) plus content. The caller decides whether to
/// honor the close request based on its own close-flag state.
pub fn modal(
    ctx: &egui::Context,
    spec: &DialogSpec,
    body: impl FnOnce(&mut Ui),
) -> bool {
    let screen_rect = ctx.viewport_rect();
    ctx.painter().rect_filled(screen_rect, 0.0, overlay::backdrop());

    let mut should_close = false;
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        should_close = true;
    }
    let backdrop = ctx.interact(
        screen_rect,
        egui::Id::new(spec.id).with("backdrop"),
        egui::Sense::click(),
    );
    if backdrop.clicked() {
        should_close = true;
    }

    let mut window = egui::Window::new(spec.id) // id doubles as window title (title_bar hidden)
        .anchor(Align2::CENTER_CENTER, spec.anchor_offset)
        .default_size(spec.default_size)
        .min_size(spec.min_size)
        .resizable(spec.resizable)
        .collapsible(false)
        .title_bar(false)
        .frame(
            egui::Frame::new()
                .fill(surface::BASE)
                .stroke(Stroke::new(STROKE_WIDTH, border::DEFAULT))
                .corner_radius(RADIUS_XL)
                .inner_margin(Margin::same(SPACE_XL as i8)),
        );
    if let Some(max) = spec.max_size {
        window = window.max_size(max);
    }

    let resp = window.show(ctx, |ui| {
        ui.set_min_width(spec.min_size[0] - 2.0 * SPACE_XL);
        body(ui);
    });

    if resp.is_none() {
        // Window closed via egui chrome (e.g. lost focus / external close).
        should_close = true;
    }
    !should_close // returns `true` while open
}
```

### Title-row helper (optional but recommended)

To also DRY the duplicated title row + X button:

```rust
/// Renders the standard title row: heading on the left, X close button right.
/// Returns true if the X was clicked.
pub fn title_row(ui: &mut Ui, title: &str) -> bool {
    let mut close = false;
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(title)
                .size(TextRole::Heading.size())
                .color(text::PRIMARY),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.button(egui_phosphor::regular::X).on_hover_text("Close (Esc)").clicked() {
                close = true;
            }
        });
    });
    close
}
```

### Why free functions, not a `GuiShell` method

Every existing Pattern B site needs `&mut self` *inside* the window body (to mutate state, dispatch commands). If `modal` were a `&mut self` method, calling it would borrow `self` for the duration of `body`, blocking the inner `self` mutation. A free function taking `&egui::Context` + `FnOnce(&mut Ui)` sidesteps this — only the body closure captures `&mut self`.

### Migration note for the 6 existing sites

Each existing site's preamble (`screen_rect`, backdrop fill, Escape check, backdrop interact+click, Window builder+frame) collapses to one `modal(ctx, &spec, |ui| { ... })` call. The `should_close` return is OR'd into the site's own close flag. This migration is **out of scope** for this task (only the shortcuts dialog is required) but the helper is designed so migration is a later mechanical pass — list it in `docs/roadmap.md`.

## Files to touch

- **`crates/animatix-gui/src/app/components/mod.rs`** — add `pub mod dialog;`
- **`crates/animatix-gui/src/app/components/dialog.rs`** *(new)* — `DialogSpec`, `modal`, `title_row` as above.
- **`crates/animatix-gui/src/app/shell/shortcut_cheat_sheet.rs`** — full rewrite to Pattern B using `modal`, fix column layout.
- **`crates/animatix-gui/src/app/mod.rs`** (line ~596) — replace `✓` with `egui_phosphor::regular::CHECK_CIRCLE` (or `CHECK`).
- **`docs/roadmap.md`** — add a "Migrate remaining Pattern B dialogs to `components::dialog::modal`" item (mechanical cleanup, not blocking this fix).

## Shortcuts dialog refactor — column layout fix

The current overlap bug: `shortcut_column` does `ui.label(key)` then `ui.with_layout(right_to_left, |ui| ui.label(desc))`. The key label claims width first with no cap; long keys ("Ctrl+Shift+G", "← / →") push into/past the description.

### Fix: fixed-width key column + ellipsis, key rendered first in RTL

Two equal columns, each `col_w`. Within a shortcut row, use a **left-to-right** horizontal with a fixed key slot:

```rust
fn shortcut_row(ui: &mut Ui, key: &str, desc: &str, col_w: f32) {
    // Key slot: monospace, fixed width, ellipsized if too long.
    let key_w = (col_w * 0.42).min(150.0);
    ui.horizontal(|ui| {
        ui.add_sized(
            [key_w, ROW_S],
            egui::Label::new(
                RichText::new(key).monospace().size(TextRole::BodyS.size()).color(text::SECONDARY)
            )
            .truncate(true), // ellipsize on overflow instead of spilling
        );
        ui.label(
            RichText::new(desc).size(TextRole::BodyS.size()).color(text::PRIMARY),
        );
    });
}
```

Key points:
- `key_w` is a fixed fraction of `col_w` (≈42%), capped at 150px. The longest key in the table ("Ctrl+Z / Ctrl+Shift+Z") fits within ~150px at BodyS monospace; shorter keys leave whitespace — visually clean and aligned.
- `Label::truncate(true)` (egui ≥0.29: `.truncate()` / older: `.sense` + galley clip) ensures any overflow becomes `…` rather than overlapping. Verify the egui version's API name (see Risk below).
- Description label takes remaining width; if it ever overflows, egui wraps by default (Label wraps on whitespace when given a bounded width).

### Column split

Keep the existing group split (left = first `ceil(n/2)` groups, right = rest) but compute `col_w` from the window's **available** width inside the body rather than the hardcoded `panel_w - SPACE_L*3`:

```rust
let col_w = (ui.available_width() - SPACE_L) / 2.0;
ui.horizontal(|ui| {
    ui.spacing_mut().item_spacing = Vec2::new(SPACE_L, 0.0);
    shortcut_column(ui, left_groups, col_w);
    shortcut_column(ui, right_groups, col_w);
});
```

Each `shortcut_column` does `ui.set_min_width(col_w)` (not `set_width`, so the window can resize). Inside, each group renders `section_header`-style title (accent color) + rows.

### Wrap the two columns in a `ScrollArea`

The fixed `panel_h = 520.0` is fragile (Content panel taller than viewport on small windows clips). Use `ScrollArea::vertical().max_height(...)` inside the window body so content scrolls if it overflows. The window's `default_size` stays `[480.0, 540.0]` but content is no longer clipped.

### Full refactored `shortcut_cheat_sheet_ui`

```rust
impl GuiShell {
    pub(crate) fn shortcut_cheat_sheet_ui(&mut self, ui: &mut egui::Ui) {
        let spec = DialogSpec::new("shortcut_cheat_sheet", "Keyboard Shortcuts", [480.0, 540.0])
            // min_size slightly smaller so it shrinks on small viewports
            .with_min_size([380.0, 320.0]);

        let open = components::dialog::modal(ui.ctx(), &spec, |ui| {
            if components::dialog::title_row(ui, "Keyboard Shortcuts") {
                self.ui_store.view.shortcuts_open = false;
            }
            ui.add_space(SPACE_M);
            ui.separator();
            ui.add_space(SPACE_M);

            let col_w = (ui.available_width() - SPACE_L) / 2.0;
            egui::ScrollArea::vertical()
                .max_height(ui.available_height())
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(SPACE_L, 0.0);
                        let mid = SHORTCUT_GROUPS.len().div_ceil(2);
                        shortcut_column(ui, &SHORTCUT_GROUPS[..mid], col_w);
                        shortcut_column(ui, &SHORTCUT_GROUPS[mid..], col_w);
                    });
                });
        });

        if !open {
            self.ui_store.view.shortcuts_open = false;
        }
    }
}
```

(`DialogSpec::with_min_size` is a builder; alternatively set the field directly — pick one style and keep it consistent with the struct.)

## Tofu fix

`app/mod.rs:596` — change:

```rust
egui::RichText::new("No diagnostics — all clear ✓")
```

to:

```rust
egui::RichText::new(format!(
    "No diagnostics — all clear {}",
    egui_phosphor::regular::CHECK_CIRCLE
))
```

**Rationale:** `CHECK_CIRCLE` is already used in `components/toast.rs:76` (success toast) and renders correctly with the loaded phosphor font. Using it (rather than bare `CHECK`) matches the existing success affordance and is more visible in the muted `text::MUTED` color. `egui_phosphor::regular::CHECK` is also acceptable and used in `context_menu.rs:282` / `export_dialog.rs:708`; pick `CHECK_CIRCLE` for the affirmative "all clear" tone.

No font-loading changes needed — the phosphor font is already registered app-wide (proven by toast usage).

## Implementation order

1. **Add `components::dialog` module** (`components/mod.rs` + new `dialog.rs`). Pure addition, no behavior change. Verify with `cargo check -p animatix-gui`.
   - *Check:* `cargo check -p animatix-gui` compiles; `#[allow(dead_code)]` is **not** needed because step 3 uses it immediately.

2. **Fix tofu** (`app/mod.rs:596`). One-line change, independent of step 1.
   - *Check:* `cargo check -p animatix-gui`; visually confirm in running GUI that the diagnostics panel "all clear" state shows a circle-check, not tofu.

3. **Rewrite `shortcut_cheat_sheet.rs`** to Pattern B using `components::dialog::modal` + `title_row`, with the fixed-width key column + `truncate` + `ScrollArea`. Drop the now-unused imports (`Pos2`, `Rect`, `Vec2`, `surface`, `border`, `overlay`, `accent` if no longer used, `RADIUS_XL`, `STROKE_WIDTH`).
   - *Check:* `cargo check -p animatix-gui` (0 errors, no unused-import warnings); `cargo test -p animatix-gui`.
   - *Manual verify:* open the shortcuts dialog (`?` / menu), confirm: symmetric left/right inner margins, no text overlap when window is at min size, long keys ("Ctrl+Shift+G", "← / →") don't collide with descriptions, content scrolls if window is short, Esc/backdrop-click/X all close.

4. **Update `docs/roadmap.md`** — add the "migrate remaining 5 Pattern B sites to `components::dialog::modal`" item as remaining mechanical work.

5. **Final gate** — `cargo check` (0 errors) and `cargo test --no-fail-fast` (all passing) before committing.

6. **Commit** — two commits per Conventional Commits:
   - `cog commit refactor "use egui::Window for shortcuts dialog" gui` (steps 1+3)
   - `cog commit fix "replace tofu checkmark in diagnostics panel" gui` (step 2)
   - (Or one combined `cog commit fix "fix shortcuts dialog layout and diagnostics glyph" gui` if preferred — they're tightly related GUI polish.)

## Risks

- **egui `Label::truncate` API name.** The method exists in current egui as `Label::truncate(bool)` (returns `Self`); older versions used `.sense(egui::Sense::focusable_noninteractive())` hacks or galley clipping. **Verify the pinned egui version** in `Cargo.toml` before writing the call. If unavailable, fall back to a fixed `key_w` + `ui.add_sized([key_w, ROW_S], Label::new(...))` *without* truncate — the fixed width alone prevents overlap (text just clips at the slot boundary, which egui does by default when the label is sized). This is the safer fallback.
- **`ctx.interact` vs `ui.interact` for backdrop.** Existing sites use `ui.interact(screen_rect, ui.id().with(...), ...)`. Using `ctx.interact` with `egui::Id::new(spec.id).with("backdrop")` must produce a stable, unique id. Verify no id collision with the window itself (egui::Window derives its id from its title string — passing `spec.id` as both window title and backdrop id prefix is safe because `.with("backdrop")` differentiates them, but confirm the window's own interact id doesn't clash). If unsure, derive backdrop id from a separate literal like `("dialog_backdrop", spec.id)`.
- **Borrow checker with `&mut self` in body closure.** The free-function design avoids this, but the `title_row` helper calling `self.ui_store.view.shortcuts_open = false` on close means `title_row` must **not** be a `&mut self` method either — it takes only `&mut Ui` and returns `bool`, and the caller mutates `self`. Confirmed in the sketch above.
- **Window title as id_salt.** `egui::Window::new(spec.id)` uses the string both as (hidden) title and id seed. Two simultaneously-open dialogs with the same `spec.id` would collide — but only one modal is open at a time in current UX. Document this constraint in the `DialogSpec` doc comment.
- **`ScrollArea` inside `Window` with `available_height`.** `ui.available_height()` inside a non-resizable window with `default_size` is bounded; if the window is resizable (settings is), the scroll area grows with it. For the shortcuts dialog (`resizable: false`) this is fine. If a later resizable dialog reuses this body pattern, the `max_height` cap keeps it sane.
- **Column split at `div_ceil(2)`.** `SHORTCUT_GROUPS.len()` is 5 → left gets 3, right gets 2. Matches the current `len()/2 + 1` split. `div_ceil` is stable since Rust 1.73; confirm toolchain ≥1.73 (animatix likely is).
- **Tofu fix ordering.** Independent of the refactor; safe to land first as a confidence-builder before the larger rewrite.
