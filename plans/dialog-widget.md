# Dialog Widget Improvement Plan

Scope: `crates/animatix-gui/src/app/components/dialog.rs` and its first consumer
`crates/animatix-gui/src/app/shell/shortcut_cheat_sheet.rs`, with supporting
changes in `components/anim.rs`, `design_tokens/motion.rs`,
`design_tokens/spatial.rs`. Prepares for Phase 5 (migrate 5 remaining dialogs).

---

## Answers to Design Questions

### 1. Responsive Sizing

**Verdict**: Fixed `default_size` + `min_size` alone is insufficient. Adopt a
hybrid: `default_size` is the *ideal* target on large screens; clamp it by both
`min_size` (floor) and a viewport-relative ceiling.

Current problems:
- `modal()` hardcodes `ui.set_min_width(spec.min_size[0] - 2.0 * SPACE_XL)` —
  fragile coupling to the frame's inner margin. If the frame margin changes,
  this silently breaks.
- No viewport-relative ceiling: on a small laptop/tablet a fixed 480–600px
  dialog can feel cramped or overflow; on a 4K display the *same* fixed size is
  fine (we do NOT want to scale up to viewport %, which would be huge).
- The export dialog already does viewport-relative sizing
  (`(width * 0.45).clamp(min, max)`) but it's hand-drawn Pattern A, not using
  `DialogSpec` — so the pattern exists but isn't reusable.

Recommended strategy: add `max_viewport_frac: Option<[f32; 2]>` to `DialogSpec`
(default `[0.85, 0.8]` — 85% width, 80% height). `modal()` computes:

```
effective_size = default_size
    .min(viewport * max_frac)        // cap by viewport on small screens
    .max(min_size)                   // never below floor
    .min(viewport - 2.0 * SCREEN_MARGIN)  // never overflow viewport
```

Large screens get `default_size`; small screens cap at 85%/80% but never below
`min_size`. For non-resizable dialogs, pass `effective_size` as
`default_size` + `min_size` + `max_size` (forces exact size every frame, so
viewport changes re-clamp). For resizable dialogs (Settings), use `min_size`
and `max_size` as bounds, `default_size` as initial hint only.

Shortcuts two-column collapse is a **content-level** decision, not a dialog-level
one. The body checks `ui.available_width()` against a threshold; below it →
1 column. This belongs in `shortcut_cheat_sheet.rs`, not `dialog.rs`.

### 2. Transitions

**Verdict**: Yes — add open/close animation (backdrop fade + window slide). Use
`motion::MODAL` (`SLOW` = 0.40s, `DECELERATE`). Integrate via internal animation
state in `modal()`, keeping the caller API unchanged.

Current state:
- `motion.rs` has `MODAL` transition (0.40s, DECELERATE) — purpose-built for
  dialogs, currently unused.
- `anim.rs` has `animate_toward`/`animate_bool`, but notes egui 0.34 limitation:
  `animate_value_with_time` is linear only, the `CubicBezier` easing field is
  metadata. **This is fixable** — sample the bezier manually.
- Toast component (`toast.rs`) already implements fade in/out via `Instant` +
  per-frame alpha — precedent for repaint-driven animation in this codebase.
- No `prefers-reduced-motion` setting exists yet (design language §8.3 defers to
  Phase 2). Flag as forward-looking; add a simple opt-out.

Integration approach:
- `modal()` tracks open/close progress via
  `anim::animate_toward_eased(ctx, id, target, motion::MODAL)` (Task B adds
  bezier sampling).
- First frame: target = 1.0 (open animation 0→1).
- Close request (Esc / backdrop click / close button / body returns `true`):
  target = 0.0 (close animation 1→0).
- `modal()` returns `true` while progress > ~0.01 (including during close
  animation); returns `false` only when progress ≈ 0 *after* a close was
  requested. The caller's `if !open { open_flag = false }` naturally delays
  the flag clear until the animation finishes — **no caller change needed**.
- Visual effects from progress `t ∈ [0,1]`:
  - Backdrop: `overlay::backdrop()` with alpha scaled by `t`.
  - Window: `anchor_offset[1] += (1.0 - t) * SLIDE_PX` (slides 12px on open).
- `ctx.request_repaint()` while `t` is mid-animation.

**Scale animation is NOT feasible** with `egui::Window` — you can't scale the
window's content region. Slide + fade is the practical, standard modal pattern
and looks good. Don't try to hack content alpha (text/widget colors are
per-widget in egui; a holistic fade requires layer-level alpha which `Window`
doesn't expose cleanly).

### 3. Children Sizing and Layout

**Verdict**: The `col_w` / `key_w` calculations work but use magic numbers.
Tokenize them; don't over-engineer a generic "dialog grid" component.

Issues:
- `col_w = (available_width - SPACE_L) / 2.0` — SPACE_L (8px) as inter-column
  gap is fine but should be a named token.
- `key_w = (col_w * 0.42).min(150.0)` — 0.42 matches
  `inspector::LABEL_WIDTH_FRAC` (same ratio, duplicated). The `.min(150.0)` cap
  is a magic number.
- `ScrollArea::vertical().max_height(ui.available_height())` — correct pattern.
  The window's `default_size` height minus non-scroll content (title + separator)
  determines the scroll viewport. Works as-is.
- A generic "dialog content grid" is not warranted for 5 dialogs — only the
  shortcuts dialog needs columns. Just tokenize its constants.

Recommendation: add a `dialog` submodule to `spatial.rs` with `COL_GAP`,
`KEY_COL_FRAC`, `KEY_COL_MAX`, `SCREEN_MARGIN`, `SINGLE_COL_THRESHOLD`,
`SLIDE_PX`, `INNER_MARGIN`. Reference these from both `dialog.rs` and
`shortcut_cheat_sheet.rs`.

### 4. API Gaps for Phase 5

**G1. Double close-point (high priority — blocks clean animation).**
`title_row()` returns `bool` (close clicked) AND `modal()` returns `bool`
(still open). The caller must handle both, and the shortcuts dialog sets
`shortcuts_open = false` in *two* places. Worse, the in-body close (from
`title_row`) fires *immediately*, which would kill any close animation.
Fix: body closure returns `bool` (request close). `modal()` is the single
close authority. `title_row()` still returns `bool` but the body *forwards* it
rather than acting on it.

**G2. No focus management (high priority for CommandPalette / FindReplace).**
`modal()` can't request initial focus on a widget. The command palette needs
its text input focused on open; FindReplace too.
Fix: pass a `DialogCtx { first_frame: bool }` to the body. Body calls
`ui.memory().request_focus(id)` when `first_frame`. `modal()` detects first
frame via a `ctx.memory` flag (set on first render, cleared on close).

**G3. Hardcoded frame margin coupling.**
`set_min_width(spec.min_size[0] - 2.0 * SPACE_XL)` assumes the frame inner
margin is `SPACE_XL`. Brittle.
Fix: define `const DIALOG_INNER_MARGIN: f32 = SPACE_XL` and use it in both the
`Frame::inner_margin` and the deduction.

**G4. No footer / action-bar pattern.**
UnsavedChanges needs Save / Discard / Cancel; Export has a custom action bar.
Low priority — each dialog can render its own footer in the body. Optionally
add a `footer_row()` helper later if 2+ dialogs share the pattern.

**G5. No `request_repaint`.**
Needed for animation. Add inside `modal()` (Task E).

**G6. `with_resizable` / `with_max_size` / `with_anchor_offset` are
`#[allow(dead_code)]`.** Correctly reserved for Phase 5 — no action until
migration. The `#[allow(dead_code)]` comments comply with the AGENTS.md rule
(every allow has an inline justification). Good.

---

## Ordered Plan

### Task A — Decouple frame margin constant (small)
**Files**: `components/dialog.rs`
- Add `const DIALOG_INNER_MARGIN: f32 = SPACE_XL;` (or move to `spatial::dialog`
  in Task D — but define locally now to unblock).
- Use it in `Frame::inner_margin(Margin::same(DIALOG_INNER_MARGIN as i8))` and
  in `window_ui.set_min_width(spec.min_size[0] - 2.0 * DIALOG_INNER_MARGIN)`.
- **Verify**: `cargo check -p animatix-gui`; visual unchanged.

### Task B — Add cubic-bezier sampling to anim.rs (small)
**Files**: `components/anim.rs`, `design_tokens/motion.rs`
- Add `impl CubicBezier { pub fn sample(self, t: f32) -> f32 }` using
  Newton-Raphson (4–5 iterations) to solve `x(s) = t` for `s`, then return
  `y(s)`. Clamp `t` to `[0,1]`.
- Add `pub fn animate_toward_eased(ctx, id, target, transition) -> f32` that
  calls `animate_toward` (linear progress `p` toward `target`), then applies
  `transition.easing.sample(p)` to get eased progress, and maps back to the
  target range.
- Remove the "easing is metadata only" caveat in `anim.rs`; update to state
  bezier is now sampled.
- **Verify**: unit test `CubicBezier::sample` identity: linear curve
  `(0,0,1,1).sample(t) == t`; `STANDARD.sample(0.5)` ≈ 0.5;
  `DECELERATE.sample(0.5)` > 0.5 (fast start). `cargo test -p animatix-gui`.

### Task C — Single close-flow + DialogCtx (small)
**Files**: `components/dialog.rs`, `shell/shortcut_cheat_sheet.rs`
- Define `pub struct DialogCtx { pub first_frame: bool }`.
- Change `modal()` signature to
  `body: impl FnOnce(&mut Ui, &DialogCtx) -> bool`.
- Collect the body's `bool` return as "body requests close"; OR it with
  Escape / backdrop-click / window-external-close. `modal()` returns `true`
  while open, `false` when should close — single authority.
- First-frame detection: `ctx.memory` flag `Id::new(spec.id).with("opened")`;
  set on first render, cleared when `modal()` returns `false`.
- `title_row()` keeps returning `bool` — body forwards it.
- Update shortcuts call site:
  ```rust
  let open = modal(ui, &spec, |ui, _dc| -> bool {
      let close = title_row(ui, "Keyboard Shortcuts");
      // ... content ...
      close
  });
  if !open { self.ui_store.view.shortcuts_open = false; }
  ```
  Remove the in-body `self.ui_store.view.shortcuts_open = false`.
- **Verify**: `cargo test -p animatix-gui`; manually close via Esc, backdrop,
  X button — all three paths work; only one assignment site remains.

### Task D — Viewport-relative sizing + spatial tokens (medium)
**Files**: `design_tokens/spatial.rs` (new `dialog` submodule),
`components/dialog.rs`
- Add to `spatial.rs`:
  ```rust
  pub mod dialog {
      pub const INNER_MARGIN: f32 = super::SPACE_5;     // 12px
      pub const SCREEN_MARGIN: f32 = super::SPACE_7;    // 24px
      pub const MAX_VIEWPORT_FRAC: [f32; 2] = [0.85, 0.8];
      pub const COL_GAP: f32 = super::SPACE_4;          // 8px
      pub const KEY_COL_FRAC: f32 = 0.42;
      pub const KEY_COL_MAX: f32 = 150.0;
      pub const SINGLE_COL_THRESHOLD: f32 = 440.0;
      pub const SLIDE_PX: f32 = 12.0;
  }
  ```
- Add `max_viewport_frac: [f32; 2]` to `DialogSpec` (default
  `dialog::MAX_VIEWPORT_FRAC`), builder `with_max_viewport_frac`.
- In `modal()`: compute `effective_size` per the formula in §1. For
  non-resizable dialogs, pass `effective_size` as `default_size`, and set
  both `min_size` and `max_size` to `effective_size` (forces exact size every
  frame, re-clamps on viewport resize). For resizable, keep caller `min`/`max`
  but clamp `default_size` hint by viewport.
- Replace the local `DIALOG_INNER_MARGIN` (Task A) with `dialog::INNER_MARGIN`.
- **Verify**: resize the GUI window from very small to very large; confirm the
  shortcuts dialog clamps at 85%/80% on small screens and stays at 480×540 on
  large screens; never overflows.

### Task E — Open/close animation (medium)
**Files**: `components/dialog.rs` (depends on B, C)
- Use `anim::animate_toward_eased(ctx, Id::new(spec.id).with("anim"), target,
  motion::MODAL)`.
- `target = 1.0` on open; `target = 0.0` once close is requested.
- Track `closing: bool` in `ctx.memory` (`Id::new(spec.id).with("closing")`).
  Set when any close source fires; clear when returning `false`.
- While progress `t > 0.01`:
  - Paint backdrop with alpha = `(t * 120.0 / 255.0)` (scale `overlay::backdrop()`
    alpha). Use `Color32::from_rgba_unmultiplied` reconstruction.
  - Window `anchor_offset[1] += (1.0 - t) * dialog::SLIDE_PX` (slide on open).
  - Call `ctx.request_repaint()`.
- When `t <= 0.01` and `closing`: clear `closing` flag, clear `opened` flag,
  return `false`.
- Edge: if close fires during open animation, just reverse `target` to 0.0 —
  `animate_toward_eased` handles the direction change smoothly.
- Forward-looking: if a `reduced_motion` flag is added to `ui_store.view`
  later, short-circuit by setting `transition.duration = INSTANT`. Note this
  in a comment; do not implement the setting itself (Phase 2 per design lang).
- **Verify**: smooth open/close; Esc mid-open reverses cleanly; rapid
  open/close doesn't leave stale state; dialog still closes if animation is
  instant (fallback).

### Task F — Shortcuts dialog responsive columns (small)
**Files**: `shell/shortcut_cheat_sheet.rs` (depends on D)
- Replace `col_w = (available_width - SPACE_L) / 2.0` with
  `(available_width - dialog::COL_GAP) / n_cols as f32`.
- Replace `key_w = (col_w * 0.42).min(150.0)` with
  `(col_w * dialog::KEY_COL_FRAC).min(dialog::KEY_COL_MAX)`.
- Add column-count decision:
  ```rust
  let n_cols = if ui.available_width() < dialog::SINGLE_COL_THRESHOLD { 1 } else { 2 };
  let chunk = SHORTCUT_GROUPS.len().div_ceil(n_cols);
  // render n_cols columns, each taking `chunk` groups
  ```
- Generalize the two-column `ui.horizontal` into an `n_cols` loop.
- **Verify**: resize dialog narrow → collapses to 1 column; wide → 2 columns;
  key column width stable.

### Task G — Migrate Settings dialog to `modal()` (medium) — Phase 5 item
**Files**: `shell/settings.rs` (depends on C, D)
- Replace the hand-drawn backdrop + `egui::Window` with:
  ```rust
  let spec = DialogSpec::new("Settings", [420.0, 520.0])
      .with_min_size([380.0, 400.0])
      .with_max_size([600.0, 700.0])
      .with_resizable(true);
  let open = modal(ui, &spec, |ui, _dc| -> bool {
      let close = title_row(ui, "Settings");
      ui.add_space(SPACE_M);
      ui.separator();
      ui.add_space(SPACE_M);
      // existing section_header + labeled_row content unchanged
      close
  });
  if !open { self.ui_store.view.settings_open = false; }
  ```
- Remove the now-duplicate backdrop / Esc / backdrop-click / external-close
  handling (all provided by `modal()`).
- **Verify**: every settings control (grid size, colorscheme, nudge, rotation
  snap, scrub step, rebuild debounce, undo limit, snap FPS, keyframe merge
  window) still mutates state; resizable; Esc/backdrop/X all close.

---

## Files to touch (summary)
- `crates/animatix-gui/src/app/components/dialog.rs` — Tasks A, C, D, E (core
  widget: margin const, signature change, viewport sizing, animation).
- `crates/animatix-gui/src/app/components/anim.rs` — Task B (bezier sampling).
- `crates/animatix-gui/src/app/design_tokens/motion.rs` — Task B (`CubicBezier`
  impl lives here or in anim.rs; add `sample` method).
- `crates/animatix-gui/src/app/design_tokens/spatial.rs` — Task D (new `dialog`
  submodule with sizing/layout tokens).
- `crates/animatix-gui/src/app/shell/shortcut_cheat_sheet.rs` — Tasks C, F
  (forward close bool; responsive columns).
- `crates/animatix-gui/src/app/shell/settings.rs` — Task G (migrate to modal).

## Risks
- **Animation state persistence**: `ctx.memory` flags must be cleared on close
  or reopening won't animate. Test rapid open/close/open sequences.
- **Window size memory vs. viewport clamp**: egui `Window` remembers size
  across frames. For non-resizable dialogs, forcing `min=max=effective_size`
  every frame is required for viewport changes to re-clamp. Verify by resizing
  the OS window while a dialog is open.
- **First-frame detection race**: if `opened` flag is set before the body runs,
  `first_frame` is correct. Must set the flag *inside* `modal()` on the first
  render pass, not before. Test focus request with a text input (CommandPalette
  will be the real exercise of this).
- **Bezier sampling correctness**: Newton-Raphson can fail to converge for
  extreme control points; add a binary-search fallback. The predefined curves
  (STANDARD, DECELERATE, ACCELERATE, SPRING_OVERSHOOT) are well-behaved, but
  SPRING_OVERSHOOT has `y1=1.56` which overshoots — verify `sample` handles
  `y > 1.0` (it should just return the overshooting value).
- **Close-flow API break**: Task C changes the body closure signature; only one
  call site exists today (shortcuts), so the blast radius is small. The 5
  Phase-5 dialogs aren't migrated yet, so they're unaffected.
- **Scale animation not possible**: confirmed — `egui::Window` can't scale
  content. Slide + fade is the agreed substitute. Don't attempt content-alpha
  hacks.
