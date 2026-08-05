# eparts Refinement + Framework-Expansion Roadmap (FINAL)

> **PROGRESS: All scheduled milestones M1–M7 + the final checkpoint are COMPLETE** (8 commits on
> `main`, `3125147e` → `afc0c521`). eparts grew from ~11 to 33 widget modules; full test sweep 1203
> passed / 41 ignored; clippy clean. The Framework Expansion Track (§6.X) remains committed but
> unscheduled, awaiting the second app / capacity. See §6 for per-milestone status and §6.Y for the
> post-completion state and recommended next steps.

Strategy doc. No code here. Informed by gpui-component (retained-mode) lessons, adapted to
immediate-mode egui 0.34. **Scope has changed**: this is no longer only about polishing a focused
animation IDE. The team now wants `eparts` to grow into a **reusable egui UI framework** for future,
unrelated apps. A second app is now **planned**, so items previously cut as "not worth it for a focused
tool" are **committed framework-expansion deliverables** — listed with a scope tag, sequenced by
value/risk, and never dropped.

This is the comprehensive, exhaustive detailed plan for `eparts`. The canonical,
short current-work list for the whole project is `docs/roadmap.md`; this plan
feeds that roadmap and keeps the eparts-specific sequencing detail.

---

## 0. Verified Baseline (from source)

Crate: `crates/eparts/` — `version = "0.1.0"`, edition 2024, deps **only** `egui` + `egui-phosphor`
(clean, no domain deps). `lib.rs` exposes `pub mod tokens;` + `pub mod widget;` and re-exports
`DiagnosticEntry` (the one trait that decouples a widget from animatix-domain types — a good template).

Files:
- Tokens: `tokens/{mod,primitive,semantic,spatial,typography,motion,util}.rs`
- Widgets: `widget/{anim,button,context_menu,diagnostics,dialog,easing_curve_editor,layout,row,text,timeline,toast,mod}.rs`

State of the art (verified):
- `tokens/semantic.rs` — generic roles are `pub const`/`pub fn` inside `pub mod` (surface 6 layers,
  text 5 levels, accent, status, border, lines, overlay). **Dark-only. No `Theme` struct.** The GUI
  re-exports these via `crates/animatix-gui/src/app/design_tokens/semantic.rs`, so **eparts is the
  single source of truth** for generic color roles — a Theme migration in eparts propagates through
  the re-export layer automatically.
- `tokens/spatial.rs` — `SPACE_0..8`, `ROW_*`, `RADIUS_*`, `STROKE_*`, component/menu/dialog submodules.
- `tokens/typography.rs` — `TextRole` (8-level). `tokens/motion.rs` — `CubicBezier`, `Transition`,
  duration/curve consts.
- `widget/button.rs` — `ButtonVariant` {Primary/Secondary/Ghost/Icon} + `ButtonSize` {Small/Medium/Large};
  per-variant **inline** color logic (not a shared state machine). Only Medium wired; Small/Large +
  Secondary are `#[allow(dead_code)]` "reserved". Tooltip only via `on_hover_text`. No loading, no
  `on_hover` callback.
- `widget/dialog.rs` — `DialogSpec` + `modal()` free fn over `egui::Window`; animated backdrop, temp
  `ctx.data` flags + `animate_value_with_time`; Escape + backdrop-click dismiss; stacked sequentially
  by caller.
- `widget/context_menu.rs` — `MenuEntry` enum + `render_menu`/`render_floating_menu`; `Order::Foreground`;
  no z-stack manager.
- `widget/toast.rs` — `ToastQueue` (4 levels), bottom-right fade, click-to-dismiss. No dedup, no placement config.
- `widget/anim.rs` — `animate_toward` wraps egui's value animator; `animate_toward_eased`,
  `animate_bool`, `animate_channel` are **dead code**.
- `widget/text.rs` — `rich`/`RichTextExt` mostly dead code.
- `widget/row.rs` — builder; selected/hover/has_children. `widget/layout.rs` — free fns
  (card/section_header/empty_state/field/labeled_row/pill_tab_bar).
- `widget/{timeline,easing_curve_editor,diagnostics}.rs` — timeline strip, easing editor, diagnostics
  list generic over `DiagnosticEntry`.
- `crates/animatix-gui/src/app/interaction/keyboard.rs` — `ShortcutRegistry` is key-first
  (`Vec<(egui::KeyboardShortcut, Shortcut)>`) with actions identified by `KeyboardAction`; no reverse
  lookup yet, but adding one is trivial and platform-aware display is available through
  `KeyboardShortcut::format(&modifiers)`.
- No generic chart widgets in GUI chrome. Plot-like UI today is bespoke (`graph_editor.rs` F-curve editor,
  `easing_curve_editor`, timeline strip, time-lens ring, FPS sparkline) or the core animatix plot feature
  (animation content, out of eparts scope). No `egui_plot` dependency.
- **No shared traits. No standalone tooltip/popover/form/input widgets. No focus manager. No
  overlay/z-order/notification manager.** Keyboard handled app-side via `ShortcutRegistry`
  (scoped TextSafe/Canvas/Global), not in eparts.

Baseline health: tokens centralized + re-exported, widgets already consume tokens, app already owns
mutable state (`ui_store`). That makes several gpui lessons cheap to adopt and others unnecessary.

---

## 1. Intent & Dual-Track Scoping Model

### Two goals, one crate
- **Track 1 — Refine animatix-gui now.** Close the real UX/polish gaps in the IDE (forms, inputs,
  tooltips, focus rings, loading states, cursor convention, light mode, system theme following). These
  are concrete, near-term, and driven by the product.
- **Track 2 — Evolve eparts into a general reusable egui framework.** Build it so a *second, unrelated*
  app can depend on `eparts` and get a coherent component library, theming, and conventions — comparable
  in spirit (not in implementation) to gpui-component, but immediate-mode. That second app is planned,
  so the framework track is committed even though sequencing stays flexible.

### Tagging scheme (used on every catalogue item)
- **[NOW]** — needed by animatix-gui in the near term.
- **[FRAMEWORK]** — not needed by animatix-gui; valuable for general reuse. Committed, but sequenced
  flexibly outside the main [NOW]/[BOTH] milestones unless pulled forward by capacity or a concrete app need.
- **[BOTH]** — serves both tracks.
- **Effort** — S (hours), M (a day or few), L (multi-day / project-sized).
- **Transferability** (immediate-mode egui) — **clean** (maps directly), **partial** (works but with
  caveats / reduced scope vs gpui), **hard** (immediate mode fights it; needs real design).

### The governing principle
> **Build framework-general APIs even for [NOW] items, so they are reusable from day one. All
> [FRAMEWORK] items are now committed deliverables, but sequence them by value/risk: [NOW]/[BOTH]
> milestones go first because they also serve framework goals, and framework-only work is interleaved
> afterward or alongside them when it is high-reuse and low-risk.** Catalogue every [FRAMEWORK] item
> with a crisp value/why-defer note so the catalogue doubles as a sequenced backlog and nothing is lost.

Concretely: when we build the Form engine for the inspector ([NOW]), its API must not hard-code
animatix concepts — it should be a generic `Form`/`Field` any app could use. We do **not**, however,
build a DataTable, webview, or JSON theme loader before the foundation they need; we list them and then
pull them in by reuse value, implementation risk, and second-app requirements. Light mode and system
auto-theme are no longer speculative: animatix-gui wants them, so B6/B11 are promoted into scheduled
[BOTH] theming work.

---

## 2. Guiding Principles (finalized)

Codify these in a new `crates/eparts/AGENTS.md`. The first block carries over from the prior doc; the
second block is added now that framework reuse is a goal.

### 2.1 Carried-over principles
1. **Theme-driven, never hardcode colors.** Every color comes from a `tokens::semantic` role; never
   `Color32::from_rgb(...)` inline in a widget. Whether a role resolves at compile time (const) or
   runtime (`Theme`) is an implementation detail behind the role name — which is what makes the Theme
   migration tractable.
2. **Builder pattern mandatory + builder tests.** Every widget is `Widget::new(...)` + chained
   `.with_x()` returning `Self`, and every builder field gets a unit test asserting it is set. eparts
   has ~no builder tests today; add them as widgets are touched.
3. **Cursor convention: default arrow for buttons, pointer only for links.** Adopt the inverse of
   egui's default. Buttons/rows keep the default cursor; reserve `CursorIcon::PointingHand` for genuine
   hyperlinks. Small change, outsized "native desktop, not web" effect.
4. **Shared trait contracts over per-widget enums.** Replace duplicated `ButtonSize`/`RowState` with
   shared `Sizable`/`Selectable`/`Disableable`/`Collapsible`.
5. **Stateless render; cross-frame state in egui Memory or app structs.** Any open/closed, animation
   progress, focus, hover-grace state lives in `ctx.data` (`Id`-keyed) or an app struct (`ui_store`),
   never in a retained widget instance. Already how dialog/toast work — make it the documented norm.
6. **4-tier size vocabulary, wire what you use.** Keep an `xs/sm/md/lg` enum on `Sizable`, but do not
   pre-build unused variants behind `#[allow(dead_code)]`. Wire a size when a call site needs it.

### 2.2 Added framework-oriented principles
7. **API stability / semver thinking.** Once a second consumer exists, public APIs are contracts.
   Until then eparts is `0.x` and may break freely. Mark APIs intended as stable; keep experimental
   ones behind `#[doc(hidden)]` or a `unstable` feature. Changelog discipline starts at first external use.
8. **Feature-flag optional heavy widgets.** Consumers pay only for what they use. Cargo features for
   heavy/optional surfaces (`table`, `charts`, `webview`, `theme-json`, `i18n`). Core tokens + traits +
   common widgets stay default-on and dependency-light (today: only `egui` + `egui-phosphor`). The Linux
   system-theme detector (`dark-light = "2.0"`) should be gated by `cfg(target_os = "linux")` and/or an
   explicit feature so non-Linux consumers do not pay for it.
9. **Documentation + example expectations.** Every public widget gets a doc comment with a minimal
   usage snippet, and a corresponding entry in a gallery/story app (K3). "Undocumented widget" = not done.
10. **Accessibility baseline.** Widgets feed egui's AccessKit integration (labels, roles, focus order)
    where egui supports it. Don't regress keyboard navigability. Full a11y is [FRAMEWORK]; the baseline
    (don't break focus/labels) is [BOTH].
11. **egui-version coupling is explicit.** A shared crate is tightly bound to one `egui` minor (today
    0.34). Pin `egui` in `Cargo.toml`, document the supported version in the README, and treat an egui
    bump as a deliberate, tested migration — not an incidental `cargo update`.

Principles to **drop** (retained-mode only): retained view tree, `RenderOnce` as a literal trait,
mandatory root view.

---

## 3. Architecture Decisions (finalized)

### 3a. Runtime theming

**Verdict: partial now, full scheduled in layers — the product and framework goals both raise theme
priority.** A reusable framework *needs* runtime theming, and animatix-gui now explicitly wants light
mode plus automatic system theme following. So the order is: struct/accessor first, component slots,
Visuals sync, `Theme::light()`, then cross-platform auto/system detection. JSON + hot-reload remain
committed framework work behind feature flags, but sequenced after the scheduled theme foundation.

Do not rip out the const modules. Introduce a `Theme` struct whose values the const-era roles equal,
reached through egui's own `Memory`.

**Step 1 — `Theme` struct mirrors current consts (additive, zero churn).**
Add `crates/eparts/src/tokens/theme.rs`:
```
pub struct Theme { /* one field per semantic role + component-scoped slots (3a taxonomy) */ }
impl Theme { pub fn dark() -> Self {...}  pub fn light() -> Self {...} }
impl Default for Theme { fn default() -> Self { Self::dark() } }
```
Existing `pub const BASE: Color32 = p::GRAY_950;` stays and is *defined to equal*
`Theme::dark().surface.base`. A unit test asserts equality. Nothing reads the Theme yet.

**Step 2 — immediate-mode access pattern (the key question).**
Store `Theme` in `egui::Memory` via `ctx.data_mut`, with a thin accessor — the egui analogue of gpui's
`Theme::global(cx)`:
```
// app, once per frame or on switch:
eparts::set_theme(ctx, Theme::dark());
// any widget, top of ui():
let t = eparts::theme(ui);          // clones a Copy/Arc Theme out of ctx.data
let bg = t.button.primary.hover.bg; // component-scoped slot read
```
No signature changes; one line per widget. Assume the clone is cheap while `Theme` is mostly `Color32`
fields; after B2/B6/B11 settle, verify. If the slot count balloons, store `Arc<Theme>` so the clone stays
cheap. **This is the idiomatic egui answer.** Rejected alternative: threading `&Theme` through every
`Widget::ui(self, ui)` signature — kills `ui.add(widget)` ergonomics.

**Visuals sync (complement, not alternative).** egui carries `ctx.style().visuals` everywhere but it is
coarse (one widget bg, one hover, one selection, hyperlink) — it cannot express 6 surface depths, status
colors, or component slots. So: keep our roles in the `Theme` struct, and **also** sync the relevant
subset onto `egui::Visuals` once per frame so stock widgets (scrollbars, `TextEdit` selection, `Window`
chrome) match our palette. For [BOTH] light mode this gives stock widgets free light theming too.

**System auto-theme (B11).** egui 0.34 can follow OS theme on Windows/macOS natively via
`egui::Options.theme_preference = ThemePreference::System`; winit supplies `RawInput.system_theme` from
`WindowEvent::ThemeChanged`, so runtime changes are auto-detected there. Linux has no native winit 0.30
`ThemeChanged` on X11/Wayland, so use `dark-light = "2.0"` (XDG Desktop Portal D-Bus, Flatpak-friendly,
supports change-listening) behind Linux `cfg` or a feature; either run its Listener or periodically
re-detect. Because egui's resolved `system_theme` is `pub(crate)`, the app/framework glue should keep an
app-owned `AppThemeChoice { Auto, Light, Dark }` and call `ctx.set_theme()`/`eparts::set_theme()` each
frame with the resolved effective theme.

**Step 3 — migrate widgets opportunistically.** When a widget is touched for another item, switch its
reads from `surface::BASE` to `let t = eparts::theme(ui); t.surface.base`. No big-bang; const path keeps
compiling. When the last const reader is gone, consts become deprecated aliases.

**Step 4 — [FRAMEWORK] JSON + hot-reload.** JSON load (serde), `.theme-schema.json`, and `notify`-based
hot-reload are independent add-ons behind a `theme-json` feature with no further widget churn. They are
committed framework deliverables, but they wait until after the scheduled runtime/light/auto theme work
unless the second app pulls them forward.

**Component-scoped slot taxonomy to adopt** (seeded from current consts, gpui-style but trimmed):
```
theme.surface.{base, raised, overlay, sunken, hover, active}   // 6 existing depths
theme.text.{primary, secondary, tertiary, disabled, inverse}
theme.border.{default, strong, focus}        // focus = the single focus_ring slot
theme.status.{info, success, warning, error}
theme.button.{primary, secondary, ghost, icon, danger}.{normal, hover, active, selected, disabled, focus}
                                                          -> { bg, border, fg, underline }
theme.list.{even, odd, selected, hover}.{bg, fg}
theme.tab.{active, inactive, hover}.{bg, fg, indicator}
theme.menu.item.{normal, hover, active, disabled}.{bg, fg}
theme.input.{normal, hover, focus, invalid, disabled}.{bg, border, fg}
theme.scrollbar.thumb.{normal, hover}.bg
```
Note the explicit `danger`/destructive button path — animatix-gui currently lacks a destructive-action
color for Delete actions. Each slot is two `Color32` (fg/fill) — **no gradients** in the core
(gradient/Oklab tokens are [FRAMEWORK], B10).

### 3b. Crate structure for framework reuse

**Recommendation: stay single-crate with Cargo features now; split into sub-crates only at the very end,
after the scheduled milestones and committed framework track prove the seams.** Premature sub-crate
splitting adds release/versioning overhead while the second app is still planned rather than actively
blocked on independently published packages.

- **Now:** keep `eparts` one crate. Introduce a feature-flag layout so heavy/optional pieces are opt-in:
  - default features: `tokens`, `traits`, core widgets (button, row, layout, dialog, toast,
    context_menu, form, input, toggle, tooltip, popover) — deps stay `egui` + `egui-phosphor`.
  - optional features: `theme-json` (serde + notify), `table` (egui_extras), `charts`, `webview`,
    `i18n` (rust-i18n), `unstable`; Linux auto-theme can use `dark-light = "2.0"` behind platform cfg or
    an explicit feature if implemented in framework glue rather than only app glue.
- **Very end (post all milestones / once package boundaries are proven):** split along feature seams into
  `eparts-core` (tokens + traits), `eparts-widgets` (the widget set), `eparts-theme-json` (loader/schema/
  hot-reload). The feature layout above pre-shapes those seams so the split is mechanical, but K2 feature
  flags are the near-term mechanism.

**Domain-coupling discipline.** `DiagnosticEntry` (re-exported from `lib.rs`) is the model: the
diagnostics widget is generic over a trait the app implements, so eparts carries no animatix-domain
types. Apply the same to anything app-specific:

- `widget/timeline.rs` and `widget/easing_curve_editor.rs` are the most likely leakage points — audit
  them for animatix-specific assumptions (frame model, timeline data shapes). If they bake in domain
  concepts, either (a) generic-over-a-trait them like diagnostics, or (b) move them to the GUI crate and
  out of the framework surface. **These two are [FRAMEWORK]-questionable; decide per K-section audit.**
- The app `ShortcutRegistry` lives in the GUI, not eparts — correct. If keybindings are generalized
  into eparts (J1), keep them action-type-generic, no animatix commands.

### 3c. State-management contract for immediate mode

Document this canonically in AGENTS.md so every framework widget is consistent:
- **Ephemeral per-frame state** (layout, hover this frame): local, recomputed each frame. Never stored.
- **Cross-frame widget-internal state** (open/closed, animation progress, hover-grace timers, focus
  memory): stored in `ctx.data_mut` keyed by an `egui::Id` derived from the widget's id-source. Widgets
  expose a stable id (`.id_source(...)`) so state survives across frames. This is how dialog/toast/anim
  already work.
- **App-owned model state** (the actual values a widget edits — strings, numbers, selection): owned by
  the application (e.g. `ui_store`) and passed by `&mut` into the widget for the frame. The widget never
  owns the model. This is the immediate-mode analogue of gpui's "stateless RenderOnce + external model".
- **Theme**: in `Memory` (3a), set by the app once per frame; auto-theme mode resolves through
  `AppThemeChoice { Auto, Light, Dark }` before calling `ctx.set_theme()`/`eparts::set_theme()`.

Rule of thumb: *if losing it on reload is fine, recompute it; if it must persist a frame, use `ctx.data`;
if it is the user's actual data, the app owns it.*

---

## 4. The Comprehensive Improvement Catalogue

Exhaustive. Each item: id · name · what to build · file(s) · effort · scope tag · transferability ·
deps · one-line value. [FRAMEWORK] items carry a why-defer note; now that the second app is planned,
"why-defer" means sequencing rationale, not optionality.

### (A) Cross-cutting traits + conventions

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| A1 | Sizable/Selectable/Disableable/Collapsible traits | shared traits + `Size` enum {xs,sm,md,lg,custom(f32)} | new `widget/traits.rs` | S | BOTH | clean | — | composability; kills duplicated per-widget enums |
| A2 | `Size`→px mapping (StyleSized analogue) | fn mapping Size→{row height, radius, pad, font}; replaces Button inline `match` | `traits.rs` + `button.rs` | S | BOTH | clean | A1 | one consistent size scale |
| A3 | eparts AGENTS.md | codify §2 principles + state contract (3c) + builder-test rule | new `crates/eparts/AGENTS.md` | S | BOTH | clean (doc) | — | future widget work stays consistent |
| A4 | Builder-test harness | `#[cfg(test)]` builder assertions as a pattern across widgets | per-widget | S | BOTH | clean | — | regression-proofs builders |
| A5 | StyledExt-style egui helpers | extension trait on `Ui`/`Response`: `h_flex`/`v_flex` helpers, padding helpers, `focused_border`, debug-border toggle | new `widget/styled_ext.rs` | M | FRAMEWORK | partial | A1 | *why-defer:* CSS-like fluent API is a nicety; animatix-gui's layouts already work. Valuable for a 2nd app's ergonomics. egui has no flexbox so `flex` helpers are thin sugar, not real flex. |

### (B) Token / theme refinements

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| B1 | `Theme` struct + Memory accessor | §3a Steps 1–2: `Theme::dark()`, `theme(ui)`, `set_theme` | new `tokens/theme.rs`, `lib.rs` | M | BOTH | clean (adapted) | — | unlocks Button state machine + component slots + future light/auto mode |
| B2 | Component-scoped slots | nested slot structs (§3a taxonomy) seeded from consts | `tokens/theme.rs` | M | BOTH | clean | B1 | richer, designed surfaces; danger button path |
| B3 | fg/fill split per slot | two `Color32` (text/border vs background); no gradients | `tokens/theme.rs` | S | BOTH | partial (drop gradients) | B2 | correct text-vs-bg theming |
| B4 | Visuals sync | map theme subset onto `egui::Visuals` once/frame | GUI `app/mod.rs` setup | S | BOTH | clean | B1 | stock egui widgets stop clashing; enables light stock widget styling |
| B5 | `focus_ring` slot | single `border.focus` slot applied uniformly | `tokens/theme.rs` | S | BOTH | clean | B2 | consistent focus rings everywhere |
| B6 | Light mode | `Theme::light()` data variant + Visuals light sync | `tokens/theme.rs`, GUI theme setup | M | BOTH | clean | B1–B4 | animatix-gui wants light mode; framework consumers need it too |
| B7 | JSON themes + schema | serde load, `.theme-schema.json`, light+dark in one file | `tokens/theme_json.rs` (feature `theme-json`) | M | FRAMEWORK | clean | B1,B6 | *why-defer:* user-authored themes are framework-track work; needs serde dep behind a feature and should follow the stable Theme struct. |
| B8 | Hot-reload via notify | `ThemeRegistry` watches theme file, swaps live | `tokens/theme_registry.rs` (feature `theme-json`) | M | FRAMEWORK | clean | B7 | *why-defer:* DX nicety for theme authors; pure add-on, no widget churn. |
| B9 | Color parsing (hex/tailwind/oklab mixing) | parse hex, tailwind names (red-500), HSL; perceptual Oklab mix | `tokens/color.rs` (feature `theme-json`) | M | FRAMEWORK | clean | B7 | *why-defer:* only needed for JSON-authored themes; nice for computed hover shades. |
| B10 | Gradient tokens | `ThemeToken` dual type (solid for fg/border + gradient bg) | `tokens/theme.rs` | M | FRAMEWORK | hard | B3 | *why-defer:* flat IDE surfaces don't need gradients; egui gradient fills are manual mesh work. Web-like surfaces only. |
| B11 | Cross-platform auto/system theme | app-owned `AppThemeChoice {Auto,Light,Dark}` resolves each frame; Win/macOS native egui/winit system theme, Linux via `dark-light = "2.0"` Listener or periodic detect | GUI theme setup, `crates/animatix-gui/Cargo.toml`, optional eparts helper | M | BOTH | partial | B6, `dark-light` on Linux | animatix-gui wants follow-OS theme; framework pattern stays cross-platform despite egui's `system_theme` being `pub(crate)` |

### (C) Input / form widgets — the real product gap

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| C1 | `Form` + `Field` | grid layout over `egui::Grid`: label column (`label_width`), `col_span`, `required`, `visible`; state app-owned | new `widget/form.rs` | M | BOTH | clean | A1 | highest leverage; inspector/dialogs hand-roll fields today |
| C2 | `TextField` | themed `egui::TextEdit` wrapper: prefix/suffix slots, `cleanable` (x), placeholder, validation hook | new `widget/input.rs` | M | BOTH | partial | B1 | consistent themed text entry |
| C3 | `NumberField` | TextField + drag-to-change + min/max/step, parse-on-commit | `widget/input.rs` | M | BOTH | clean | C2 | core for animation params (durations, offsets) |
| C4 | Checkbox/Radio/Switch | themed, animated checkmark/thumb crossfade, `label_side`, tooltip | new `widget/toggle.rs` | M | BOTH | clean | B1, I2 | common controls + noticeable polish |
| C5 | `Slider` (single + log) | wrap egui slider, theme it, drag events, optional log scale | new `widget/slider.rs` | M | BOTH | partial | B1 | numeric ranges |
| C6 | `Select`/`Combobox` | wrap `egui::ComboBox`: searchable + clearable + groups; commit-on-close | new `widget/select.rs` | M | BOTH | partial | B1 | enum/choice props |
| C7 | Slider dual-thumb / range | second thumb + range value | `widget/slider.rs` | M | FRAMEWORK | partial | C5 | *why-defer:* no animatix call site; add when a feature needs a range. |
| C8 | InputMode multiline/autogrow | `InputMode` {SingleLine, Multiline, AutoGrow} on TextField | `widget/input.rs` | S | FRAMEWORK | partial | C2 | *why-defer:* inspector values are single-line; autogrow is a general-app need (chat, notes). |
| C9 | Code-editor input | syntax-highlight TextEdit, line numbers, gutter | `widget/code_input.rs` (feature) | L | FRAMEWORK | hard | C2 | *why-defer:* animatix edits `.amx` in its own editor; a generic code input is a big surface, immediate-mode text editing is fiddly. |
| C10 | Undo/History stack | per-field undo ring; generic `History<T>` helper | `widget/history.rs` | M | FRAMEWORK | partial | C2 | *why-defer:* app owns undo today; valuable as a reusable primitive for a 2nd app. |
| C11 | Input masking | format mask (phone, date, currency) on TextField | `widget/input.rs` | M | FRAMEWORK | partial | C2 | *why-defer:* no masked fields in animatix; classic form-app need. |
| C12 | Date/time pickers | calendar popover + time spinner | `widget/datetime.rs` | L | FRAMEWORK | hard | D5 | *why-defer:* animatix has no dates; common in business apps. Calendar grid + popover is real work. |
| C13 | Rating | star rating input | `widget/rating.rs` | S | FRAMEWORK | clean | A1 | *why-defer:* purely a general-app widget. |
| C14 | Stepper (numeric) | +/- buttons around a NumberField | `widget/input.rs` | S | FRAMEWORK | clean | C3 | *why-defer:* drag NumberField covers animatix; steppers suit quantity inputs. |
| C15 | Pagination | page list + prev/next, page-size select | `widget/pagination.rs` | M | FRAMEWORK | clean | E5 | *why-defer:* pairs with DataTable (H); no paged data in animatix. |
| C16 | ColorPicker (composed) | popover + HSV area + sliders + hex input + swatches | `widget/color_picker.rs` | M | BOTH | partial | D5, C5 | actor/style color props benefit; compose once D5+C5 exist |

### (D) Overlay / notification / focus infrastructure

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| D1 | Notification dedup + placement | `TypeId`/key dedup, "(xN)" collapse, configurable placement | `widget/toast.rs` | S | BOTH | clean | — | cheap; recompiles stop spamming toasts |
| D2 | Managed overlay layer | `ctx.data`-backed registry: active overlays w/ priority (tooltip>popover>dialog) + uniform click-outside/Escape. **Coordination layer, NOT mandatory root.** | new `widget/overlay.rs` | M | BOTH | partial | — | consistent dismissal/priority across overlays |
| D3 | Focus restore on modal close | save `Memory::focused()` Id on open, restore on close | `widget/dialog.rs` | S | BOTH | partial | — | focus returns where it was; no WeakHandle needed |
| D4 | Standalone tooltip + hover-card | managed tooltip w/ open delay (~400ms) + close grace (~150ms); per-id timer in Memory | new `widget/tooltip.rs` | M | BOTH | clean | — | replaces raw `on_hover_text`; no flicker = deliberate feel |
| D5 | Generic `Popover` | anchored floating panel (reuse `render_floating_menu`), trigger-is-open, click-outside dismiss, focus restore | new `widget/popover.rs` | M | BOTH | partial | D2, D3 | foundation for C6/C12/C16 |
| D6 | Sheet (side panel) | slide-in side panel overlay | `widget/sheet.rs` | M | FRAMEWORK | partial | D2 | *why-defer:* animatix uses docked panels; sheets suit mobile-ish/general layouts. |
| D7 | Full FocusTrap Tab-cycling | trap Tab within a modal, cycle fields | `widget/focus_trap.rs` | M | FRAMEWORK | hard | D3 | *why-defer:* D3 covers 90%; strict Tab containment only matters for complex multi-field modals (a 2nd app's forms). Immediate-mode focus order is fiddly. |
| D8 | Root-orchestrator-style layer manager | full layered root (tooltip>popover>dialog>sheet>notification) | `widget/root.rs` | L | FRAMEWORK | hard | D2 | *why-defer:* retained-mode construct; egui `Area`/`Order` already composites. D2 is the right immediate-mode subset. Only if a 2nd app wants gpui-parity layering. |
| D9 | Context/native menu system | promote `context_menu.rs` to a managed menu API + OS-native menu bar | `widget/menu.rs` | M | FRAMEWORK | partial | D2 | *why-defer:* current `render_menu` works for animatix; native menu bar is a general desktop-app feature. |

### (E) Richer existing widgets

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| E1 | Button per-state style machine | replace inline per-variant logic with `t.button.<variant>.<state>()`→{bg,border,fg,underline} | `button.rs`, `tokens/theme.rs` | M | BOTH | clean | A1, B2 | biggest internal cleanup; removes branchy match |
| E2 | Button loading + on_hover | `.loading(bool)` (spinner + disable), `.on_hover(cb)` | `button.rs` | S | BOTH | clean | G1 | export/render actions show progress |
| E3 | Wire sizes, delete dead stubs | wire Small/Large via A2; remove unused `#[allow(dead_code)]` Secondary/Small/Large | `button.rs` | S | NOW | clean | A2 | "don't pre-build unused" |
| E4 | Row/ListItem 4-way state + suffix | selected/secondary_selected/confirmed/disabled + suffix slot | `row.rs` | M | BOTH | clean | A1, B2 | scene tree / actor list selection states |
| E5 | Stateful TabBar | promote `pill_tab_bar` free fn to widget w/ `Selectable` | `widget/layout.rs`→new `widget/tabs.rs` | S | BOTH | clean | A1 | reusable tab control |
| E6 | Kbd badge + shortcut-in-tooltip | styled `Kbd` badge; auto-pull shortcuts from app registry via new `shortcut_for`; later drive cheat sheet + toolbar tooltip strings from registry | new `widget/kbd.rs` + GUI glue, `crates/animatix-gui/src/app/interaction/keyboard.rs` | M | BOTH | partial | D4 | VERIFIED-FEASIBLE: derive `Eq`/`PartialEq`/`Hash` on `KeyboardAction`, add linear `shortcut_for`, use `KeyboardShortcut::format(&modifiers)` |

### (F) Layout & navigation widgets

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| F1 | Separator | themed h/v separator with optional label | `widget/layout.rs` | S | BOTH | clean | B1 | trivial, used everywhere |
| F2 | GroupBox | titled bordered container | `widget/layout.rs` | S | BOTH | clean | B1 | groups inspector sections |
| F3 | ResizeHandle / splitter | padded hitbox + drag cursor over egui splitters | new `widget/resize.rs` | S | BOTH | partial | — | nicer panel/timeline resize affordance |
| F4 | Collapsible / Accordion | header + animated expand body; accordion = exclusive group | new `widget/collapsible.rs` | M | BOTH | clean | A1(Collapsible), I2 | collapsible inspector sections |
| F5 | StatusBar | bottom bar w/ segments | `widget/status_bar.rs` | S | BOTH | clean | B1 | IDE status line |
| F6 | Breadcrumb | path/trail nav | `widget/breadcrumb.rs` | S | FRAMEWORK | clean | E5 | *why-defer:* animatix has shallow nav; common in file/hierarchy apps. |
| F7 | TitleBar / window chrome | custom title bar + window controls | `widget/title_bar.rs` (feature) | M | FRAMEWORK | hard | D2 | *why-defer:* animatix uses native window chrome; custom title bar is OS-specific and only for frameless apps. |
| F8 | Sidebar | collapsible nav sidebar w/ items + groups | `widget/sidebar.rs` | M | FRAMEWORK | partial | F4, E4 | *why-defer:* animatix has a fixed panel layout; sidebars are a general app-shell pattern. |
| F9 | Dock / panel system | dockable, draggable, splittable panels | `widget/dock.rs` (feature) | L | FRAMEWORK | hard | F3, D2 | *why-defer:* big system; `egui_dock` exists. Only if a 2nd app needs IDE-style docking we don't already get from the GUI's own layout. |
| F10 | Stepper (wizard) | multi-step progress header | `widget/stepper.rs` | M | FRAMEWORK | clean | E5 | *why-defer:* no wizards in animatix; onboarding/checkout pattern. |

### (G) Display / feedback widgets

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| G1 | Spinner | indeterminate spinner | new `widget/spinner.rs` | S | BOTH | clean | I-anim | loading affordance; needed by E2 |
| G2 | Skeleton | shimmer placeholder block | `widget/skeleton.rs` | S | BOTH | clean | I2 | perceived responsiveness during `.amx` recompiles |
| G3 | ProgressBar | determinate bar + optional label | `widget/progress.rs` | S | BOTH | clean | B1 | export/render progress |
| G4 | Badge | small count/status badge | `widget/badge.rs` | S | BOTH | clean | B1 | counts, indicators |
| G5 | Tag / Chip | labeled chip, optional removable | `widget/tag.rs` | S | BOTH | clean | B1 | filters, labels |
| G6 | Alert / Callout | inline status banner (info/success/warn/error) | `widget/alert.rs` | S | BOTH | clean | B2(status) | inline messages |
| G7 | Label | themed label honoring TextRole + required marker | `widget/layout.rs` | S | BOTH | clean | B1 | consistent labels (Form uses it) |
| G8 | Link | hyperlink w/ pointer cursor (the one place it's allowed) | `widget/link.rs` | S | BOTH | clean | B1 | docs links, external nav |
| G9 | Indicator | small status dot (online/dirty/etc.) | `widget/indicator.rs` | S | FRAMEWORK | clean | B2 | *why-defer:* minor; general status UI. |
| G10 | Avatar | image/initials circle | `widget/avatar.rs` | S | FRAMEWORK | clean | — | *why-defer:* no users/people in animatix; social/collab apps. |
| G11 | Description list | key→value pairs layout | `widget/description_list.rs` | S | FRAMEWORK | clean | C1 | *why-defer:* Form/labeled_row covers animatix; general detail views. |

### (H) Data widgets

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| H1 | Tree-view | flat-rebuild model + arrow-key nav + expand_ancestors; **scoped to scene/actor hierarchy data, not a generic virtualized tree** | new `widget/tree.rs` | M–L | BOTH | partial | A1(Collapsible), E4 | scene/actor hierarchy genuinely needs it |
| H2 | List w/ keyboard nav + type-ahead | arrow nav, type-ahead jump, selection | `widget/list.rs` | M | BOTH | partial | E4 | actor/asset lists |
| H3 | Searchable list | filter box + filtered List | `widget/list.rs` | S | BOTH | clean | H2, C2 | quick-find in lists |
| H4 | DataTable | virtual rows + sortable/resizable/movable/pinned cols + cell/row/col select | `widget/table.rs` (feature `table`) | L | FRAMEWORK | hard | A1 | *why-defer:* no tabular surface in animatix; `egui_extras::TableBuilder` covers rare cases. Full data-table is a project. |
| H5 | VirtualList (heterogeneous) | virtualize variable-height items | `widget/virtual_list.rs` | M | FRAMEWORK | hard | — | *why-defer:* egui `show_rows`/`ScrollArea` handles uniform lists; heterogeneous virtualization unjustified until a huge variable list exists. |

### (I) Animation / motion utilities

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| I1 | Lerp trait | `Lerp` for f32/Color32/Vec2/Rect | `widget/anim.rs` | S | BOTH | clean | — | foundation for micro-anims |
| I2 | Transition combinator (micro-anim) | small combinator over `CubicBezier` for widget micro-animations (crossfades, expands) | `widget/anim.rs` | S | BOTH | clean | I1 | powers C4/F4/G2 polish |
| I3 | Resolve dead `animate_*` helpers | use `animate_toward_eased`/`animate_bool`/`animate_channel` in C4/F4 or delete them | `widget/anim.rs` | S | NOW | clean | I2 | removes dead code per AGENTS.md |
| I4 | Animated checkmark crossfade | crossfade check/uncheck instead of snap | `widget/toggle.rs` | S | BOTH | clean | I2 | small, very noticeable polish |

*Note:* the **product's** animation engine (timeline eval) is separate from **UI** micro-animation. Keep
a full keyframe combinator out of eparts — that lives in `crates/animatix`.

### (J) Keyboard / interaction system

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| J1 | Generalize scoped keybindings | move the scoped-action keymap pattern into eparts as action-type-generic (no animatix commands) | new `widget/keymap.rs` | M | BOTH | partial | — | reusable shortcut system; app supplies actions |
| J2 | Kbd rendering | (see E6) styled key badges | `widget/kbd.rs` | S | BOTH | clean | — | shortcut display |
| J3 | Platform-aware shortcut labels | render ⌘ vs Ctrl etc. per OS using `KeyboardShortcut::format(&modifiers)` | `widget/kbd.rs` | S | BOTH | clean | J2 | correct labels cross-platform |
| J4 | Global keymap / context system | layered/contextual keymaps, command palette backing | `widget/keymap.rs` | M | FRAMEWORK | partial | J1 | *why-defer:* animatix's scoped registry suffices; full contextual keymaps suit large apps w/ command palettes. |

### (K) Framework infrastructure / quality

| id | name | build | file(s) | eff | tag | transfer | deps | value |
|---|---|---|---|---|---|---|---|---|
| K1 | Domain-coupling audit | audit `timeline.rs`/`easing_curve_editor.rs` for animatix leakage; generic-over-trait or move to GUI | those files + `lib.rs` | M | BOTH | partial | — | keeps framework surface domain-free (like DiagnosticEntry) |
| K2 | Feature-flag layout | Cargo features per §3b; gate heavy widgets | `Cargo.toml`, `lib.rs` | S | BOTH | clean | — | consumers pay for what they use; keeps sub-crate split deferred |
| K3 | Gallery / story app | example bin showcasing every widget + theme switcher (their `story` crate analogue) | new `crates/eparts-gallery/` or `examples/` | M | FRAMEWORK | clean | B6 | *why-defer:* needed when external consumers must discover widgets; doubles as visual test bed. |
| K4 | i18n hooks | rust-i18n integration, en/zh strings in widgets | `lib.rs` (feature `i18n`) | M | FRAMEWORK | partial | — | *why-defer:* animatix is single-locale; i18n matters for shipped general apps. |
| K5 | Accessibility / AccessKit | feed labels/roles/focus order to egui AccessKit | per-widget | M | FRAMEWORK | partial | — | *why-defer:* baseline (don't break focus) is BOTH; full a11y is a shipped-product concern. |
| K6 | CI platform parity | mac/linux/win build+test matrix for eparts | `.github/workflows` | S | FRAMEWORK | clean | — | *why-defer:* matters once external consumers run other OSes. |
| K7 | Semver / release discipline | changelog, version policy, stable/unstable marking | docs | S | FRAMEWORK | clean | — | *why-defer:* starts at first external consumer. |
| K8 | Docs site / API docs | rustdoc + usage guide | docs | M | FRAMEWORK | clean | K3 | *why-defer:* external-consumer concern. |
| K9 | Clipboard helper | copy/paste helper widget + integration | `widget/clipboard.rs` | S | FRAMEWORK | clean | — | *why-defer:* egui has clipboard; a helper is a convenience for a 2nd app. |
| K10 | Webview | embed web content | `widget/webview.rs` (feature `webview`) | L | FRAMEWORK | hard | — | *why-defer:* egui has no native webview; options are heavy external crates (e.g. wry) with platform caveats and a separate window/compositing story. Entirely off animatix mission. |
| K11 | Charts / plot | chart + plot widgets built fresh against a clean data-binding API; do **not** extract bespoke F-curve/easing editors; do **not** confuse with core animatix plot feature | `widget/charts.rs` (feature `charts`) | L | FRAMEWORK | partial | — | VERIFIED-DEFERRED-BOTTOM: animatix-gui chrome has no generic chart consumer and no `egui_plot`; keep lowest priority in the committed framework track. |

---

## 5. Visual Design & UX Polish (finalized, ranked)

Concrete polish, ranked by impact on perceived quality of **animatix-gui**:

1. **Cursor convention (§2.3, E3-adjacent).** Stop the hand cursor on buttons/rows; reserve for links.
   Biggest "feels like a real desktop app" change. Effectively free.
2. **Hover-card grace period (D4).** ~400ms open delay + ~150ms close grace; no flicker on transit.
3. **Animated checkmark/toggle crossfade (C4/I4).** State changes crossfade rather than snap.
4. **Button loading spinner + disabled coherence (E2/G1).** Export/render show progress, not a freeze.
5. **Consistent focus rings (B5/E1).** One `border.focus` slot applied to every focusable widget;
   today the focus stroke is hand-painted per Button branch — a common polish leak.
6. **Component-scoped slots incl. destructive/danger button path (B2).** `tab.active`, zebra
   `list.even`, `menu.item.hover`, and a `button.danger.*` path for Delete actions (missing today).
7. **Light mode + system auto-theme (B6/B11).** A user-visible preference and OS-following behavior now
   wanted by animatix-gui; forces the theme foundation to be real rather than theoretical.
8. **Loading / skeleton during recompiles (G1/G2).** Spinner + shimmer in preview/canvas while compiling
   a scene — raises perceived responsiveness on the core compile→preview loop.
9. **Notification dedup (D1).** Recompiles can spam toasts; TypeId/key dedup + "(xN)" collapse.
10. **Visuals sync (B4).** Scrollbars, text-edit selection, window chrome match the active palette so
   stock egui widgets don't betray the custom theme.

Most improve animatix-gui specifically: **1, 2, 5, 7, 8** — they touch every interaction, theme
coherence, and the compile→preview loop, the product's heartbeat.

---

## 6. Scheduling: Phases & Milestones

> **STATUS — ALL SCHEDULED MILESTONES COMPLETE (M1–M7 + final checkpoint).**
> Implemented and committed to `main` across 8 feature commits (`3125147e` → `afc0c521`).
> Verification at each milestone and at the end: `cargo check --workspace` clean,
> `cargo test -p eparts` (166 tests), `cargo test -p animatix-gui` (206), full sweep
> 1203 passed / 41 ignored, `--features video` build clean, `cargo clippy -p eparts` clean.
> eparts grew from ~11 to 33 widget modules. The Framework Expansion Track (§6.X) remains
> committed-but-unscheduled. See §6.Y for the post-completion state.

[NOW]/[BOTH] items are scheduled into milestones by dependency. **All [FRAMEWORK]-only items are
committed deliverables** and are collected into the Framework Expansion Track (§6.X), which is
unscheduled but no longer optional. Theme work is now higher priority: **B1–B6 + B11 are scheduled**
because light mode and system auto-theme serve animatix-gui and the framework.

Every milestone verifies per AGENTS.md:
```
cargo check --workspace        # all crates compile (catches GUI/analyzer/LSP drift)
cargo test -p animatix-syntax  # parser tests
cargo test -p animatix --lib   # core lib tests
cargo test -p animatix-gui     # GUI (no FFmpeg needed)
cargo test --no-fail-fast      # full sweep
# + visual check (launch GUI)
```

### Milestone 1 — Safe additive foundation ✅ DONE (commit `3125147e`)
Fully additive, no breaking changes; const token path stays intact, new widgets opt-in.
- ✅ **A1 + A2** — `traits.rs` (Sizable/Selectable/Disableable/Collapsible + Size) + Size→px mapping. (S)
- ✅ **A3** — `crates/eparts/AGENTS.md` codifying §2 principles + state contract (3c). (S)
- ✅ **B1** — `tokens/theme.rs`: `Theme` struct + `Theme::dark()` + `Default` + `theme(ui)`/`set_theme`,
  seeded to current consts; unit test asserting `Theme::dark()` equals the consts. **No widget migrated.** (M)
- ✅ **Cursor convention** — flipped Button + Row to default cursor; pointer only for links. (S)
- ✅ **D1** — toast dedup + placement field. (S)

Deps: none external. Verified: workspace compiles, GUI tests green, toast dedup + cursor confirmed.

### Milestone 2 — Theme depth, light mode, and auto-theme ✅ DONE (commit `005e9ebf`)
Depends on M1. This is now a product milestone, not framework-only speculation.
- ✅ **B2** component-scoped slots · **B3** fg/fill split (`Slot{bg,fg,border}`, `Fill`, `TabSlot`) ·
  **B5** `focus_ring()` slot · **B4** `to_visuals()` + GUI `install_theme` rewrite.
- ✅ **B6** `Theme::light()` (hand-authored light palette) + Visuals light sync; light/dark switching
  in animatix-gui Settings ("Appearance" dropdown) + persistence. (M)
- ✅ **B11** cross-platform auto/system theme: app-owned `AppThemeChoice {Auto, Light, Dark}`, resolved
  each frame via `eparts::set_theme()`. `dark-light = "2.0"` for OS detection (all platforms), cached
  with ~2s periodic re-probe so `Auto` follows runtime OS changes. (M)
Verified: dark/light/auto switching works; stock widgets follow Visuals; persisted across restarts.

### Milestone 3 — Button cleanup on the real theme system ✅ DONE (commit `66489b3a`)
Depends on M1/M2.
- ✅ **E1** Button per-state style machine (reads `theme.button.<variant>.<state>` slots; follows light/dark) ·
  **E3** removed dead `Secondary`/`Small`/`Large` stubs + unused builders ·
  **E2** `.loading(bool)` (spinner + disabled) + `.on_hover(cb)`.
- ✅ **G1** Spinner (indeterminate, theme-driven).
Verified: dark mode visually unchanged (slots seeded to match); light mode coherent; clippy clean.

### Milestone 4 — Overlays, tooltips, focus ✅ DONE (commit `7c11df95`)
Depends on M1/M2.
- ✅ **D2** managed overlay coordination layer (priority Dialog<Popover<Tooltip, `escape_pressed`/`clicked_outside`/`is_topmost`) ·
  **D3** focus restore on modal close (save/restore focused `Id`) · **D4** tooltip + hover-card grace
  (~400ms open / ~150ms close) · **D5** generic Popover (reuses overlay + focus restore).
- ✅ **E6** Kbd badge + `format_shortcut` + `ShortcutRegistry::shortcut_for` (discriminant-based, avoids
  the `f32` Eq/Hash problem on `NudgeSelected`). Toolbar tooltips and the shortcut cheat sheet now
  read the registry; gesture-only rows remain static.
Verified: clippy clean (one transient tooltip lint fixed); GUI green.

### Milestone 5 — Form & inputs (the product gap) ✅ DONE (commit `b1af84b9`)
Depends on M1/M2.
- ✅ **C1** Form+Field · **G7** Label · **C2** TextField (prefix/suffix/cleanable/validate) ·
  **C3** NumberField (drag + range/step/suffix) · **C4** Checkbox/Radio/Switch (animated crossfade) ·
  **C5** Slider (single + log) · **C6** Select/Combobox (searchable + clearable + groups).
- ✅ **I1** `Lerp` trait (f32/Color32/Pos2/Vec2) · **I2** `animate_lerp`/`animate_bool_eased` combinators ·
  **I3** resolved the dead `animate_*` helpers (now used / justified) · **I4** animated checkmark.
Verified: 93 eparts tests; builder tests per A4; full sweep green.
Follow-up: inspector and Settings fields are migrated; export/palette dialogs were assessed and deliberately left on their custom inputs (see §6.Z).

### Milestone 6 — Display/feedback + remaining widgets ✅ DONE (commit `788a9eb7`)
Depends on prior.
- ✅ **G2** Skeleton · **G3** ProgressBar · **G4** Badge · **G5** Tag · **G6** Alert · **G8** Link
  (the one widget that uses `PointingHand`).
- ✅ **E4** Row/ListItem 4-way (selected/secondary_selected/confirmed/disabled) + suffix ·
  **E5** stateful TabBar.
- ✅ **F1** Separator · **F2** GroupBox · **F3** ResizeHandle · **F4** Collapsible/Accordion · **F5** StatusBar.
Verified: 125 eparts tests; GUI Row call sites unchanged; full sweep green.

### Milestone 7 — Data + composed widgets + interaction ✅ DONE (commits `3e1803eb`, `afc0c521`)
Depends on prior.
- ✅ **H1** Tree-view (generic flat-entry model + arrow nav + type-ahead) · **H2** List w/ kbd nav +
  type-ahead · **H3** SearchableList.
- ✅ **C16** ColorPicker (composed from Popover + egui color picker + TextField hex + swatches).
- ✅ **J1** generic action-agnostic `Keymap<A, S>` (scope-predicate gating + reverse lookup).
- ✅ **K1** domain-coupling audit — timeline.rs + easing_curve_editor.rs confirmed already generic
  (primitive APIs, zero animatix imports); **no decoupling needed**. · **K2** feature-flag scaffolding
  in `Cargo.toml` (reserved seams: theme-json/table/charts/webview/i18n/unstable, all empty today).
Verified: 166 eparts tests; clippy clean (tree.rs/list.rs lints fixed); full sweep green.

### Final verification checkpoint — post scheduled milestones ✅ DONE
- ✅ **Q3 / Theme clone cost** — measured: `Theme` is ~117 `Color32` fields (~468 bytes) and remains
  `Copy`. Well under 1KB, so storing by value in egui Memory is fine. **No `Arc<Theme>` needed.**
  Re-verify only if the slot count grows substantially (e.g. when JSON themes B7 land).
- ⏳ **Sub-crate split gate** — still deferred. Single-crate + K2 feature flags remain the structure;
  revisit `eparts-core`/`eparts-widgets`/`eparts-theme-json` split only when a second consumer needs
  independently published packages.
Verified: `cargo check --workspace` clean, eparts/GUI tests green, full sweep 1203 passed / 41 ignored.

### §6.X Framework Expansion Track — committed, sequencing flexible
The second app is planned, so this is no longer a conditional backlog. All [FRAMEWORK] items below are
committed deliverables, but they are reorderable and should be interleaved after or alongside the
[NOW]/[BOTH] milestones as capacity allows. Priority rule: highest reuse, lowest implementation risk,
and clearest second-app need first. Light mode and auto-theme are already promoted out via B6/B11.

- **Theming:** B7 JSON+schema · B8 hot-reload (notify) · B9 color parsing/oklab · B10 gradient tokens.
- **Inputs:** C7 dual-thumb slider · C8 multiline/autogrow · C9 code-editor input · C10 undo/History ·
  C11 masking · C12 date/time pickers · C13 rating · C14 stepper · C15 pagination.
- **Overlays:** D6 Sheet · D7 full FocusTrap · D8 root-orchestrator layer manager · D9 native menu system.
- **Layout/nav:** A5 StyledExt helpers · F6 Breadcrumb · F7 TitleBar · F8 Sidebar · F9 Dock · F10 wizard Stepper.
- **Display:** G9 Indicator · G10 Avatar · G11 Description list.
- **Data:** H4 DataTable · H5 VirtualList.
- **Interaction:** J4 global/contextual keymap.
- **Infra/quality:** K3 gallery app · K4 i18n · K5 AccessKit · K6 CI matrix · K7 semver/release ·
  K8 docs site · K9 clipboard · K10 webview.
- **Lowest priority:** K11 charts/plot — no current GUI chrome consumer; if ever built, build fresh
  against a clean data-binding API rather than extracting bespoke F-curve/easing editors or conflating
  with the core animatix plot feature.

First framework-track candidates once capacity opens (highest reuse value, lowest risk): **K3 gallery
app**, **B7 JSON themes**, **K6 CI platform parity**, **A5 StyledExt helpers**, then **B8 hot-reload**.
K11 remains at the bottom until a real chart consumer appears.

### §6.Y Post-completion state (what's done, what remains)

**Delivered (committed to `main`):** the entire scheduled track — cross-cutting traits + conventions
(A1–A4), the runtime `Theme` system with component-scoped slots, light mode, and cross-platform
auto-theme (B1–B6, B11, + the `dark-light` dependency), the theme-aware Button state machine + loading
(E1–E3, G1), overlay/tooltip/popover/focus infrastructure (D1–D5, E6), the full input/form widget set
(C1–C6, G7, I1–I4), display/feedback + layout + navigation widgets (G2–G8, E4–E5, F1–F5), data +
composed widgets (H1–H3, C16), the generic Keymap (J1), the K1 domain audit (no action needed), and
K2 feature-flag scaffolding.

**Known follow-ups inside completed milestones (small, not blocking):**
- Adopt the new widgets (Tree for the scene hierarchy, ColorPicker for color props, TabBar, etc.) at
  their natural GUI call sites as those areas are next touched.

### §6.Z GUI-adoption pass (post-completion, committed to `main`)

After the milestones, a follow-up pass addressed the architectural review findings and dogfooded the
library in the app. State as of commit `5ef1565f`:

**Done:**
- **Theme migration completed** (`b48c0b2c`): all 33 eparts widgets read the runtime `Theme` (was ~12);
  light/dark/auto now works app-wide. `Theme` gained a `lines` field; anti-drift test covers all base roles.
- **Settings dialog migrated** (`45e5f78d`) to `Form`/`NumberField`/`Select` — first real consumer.
- **Inspector property fields migrated** (`73f7d17c`) to `NumberField`/`TextField`/`Select`; commit path,
  value transforms (rotation radians, size half-extent), and undo drag-batching preserved. Verified the
  flagged size/half-extent transform is correctly reversed on commit (`actions/mod.rs:466`) — not a bug.
- **Toolbar shortcut tooltips** (`f536275f`) now derive from a shared `SHORTCUT_REGISTRY` +
  `shortcut_for` + `format_shortcut` (platform-aware), replacing hardcoded "Ctrl+S" strings.
- **Widget API contract documented** (`a2e58a0d`) in eparts AGENTS.md (Tier-1 `impl Widget` vs Tier-2
  `show()`); codebase verified to already conform.
- **Gallery example** (`4af9a790`): `cargo run -p eparts --example gallery`, dark/light switcher, every
  widget. Building it caught and fixed two real bugs: the `toggle` module (Checkbox/Radio/Switch) was
  never compiled (missing `pub mod toggle;`), and `Button::primary()` had been over-removed in M3.
- **Two runtime bugs fixed** (`5ef1565f`): dialog-open freeze (nested egui Memory/data lock deadlock in
  the focus save/restore) and icon-only button misalignment (empty-label normalization regressed in M3).
  Regression test added for the label fix.

**Assessed and deliberately NOT migrated (re-scoped from HV2/HV3, low value / high friction):**
- **Scene/actor hierarchy → `Tree`**: REJECTED. The hierarchy (`sidebar.rs::render_actor_tree`) already
  uses the eparts `Row` widget and supports multi-select (required for Align/Distribute),
  drag-to-reparent, and right-click context menus — none of which eparts `Tree` provides. Migrating would
  lose features for marginal gain. Stays Row-based.
- **Remaining dialogs → inputs**: REJECTED for now. `export_dialog`'s `DragValue`s use `.prefix("W: ")`
  (NumberField has no prefix) and are wrapped in `field_sized` (NumberField self-frames → double frame),
  under `video`-gated code. `command_palette`/`find_replace`/`insertion_palette` text fields have custom
  `request_focus` + arrow-key filtered-nav that `TextField`/`SearchableList` would disrupt. High friction,
  low gain.

**Remaining low-value GUI adoption (opportunistic, do when the area is next touched — NOT scheduled):**
- ~40 GUI files still read const `design_tokens::` directly. This is harmless: those consts resolve
  through the runtime `Theme` (the GUI re-exports eparts roles), so the app is already theme-complete.
  Converting reads to `eparts::theme(ui)` is cosmetic; do it opportunistically.
- Inline status banners → `Alert`; counts → `Badge`/`Tag`; inspector section grouping → `Collapsible`/
  `GroupBox`; raw `on_hover_text` → `Tooltip` with grace period — adopt as those files are edited.

**Not started (committed Framework Expansion Track, §6.X):** all [FRAMEWORK] items remain unscheduled
until the second app firms up or capacity opens; recommended ordering is listed above.

**Verification baseline (current, post GUI-adoption pass + bug fixes):** `cargo check --workspace` clean
(+ `--features video`), `cargo test -p eparts` 184, `cargo test -p animatix-gui` 206,
`cargo test --no-fail-fast` 1221 passed / 44 ignored, `cargo clippy --workspace --all-targets` clean.

---

## 7. Risks / Tradeoffs / Open Questions

### Risks & tradeoffs
- **egui-version coupling.** A shared crate is locked to one egui minor (0.34). A second consumer pins
  the same egui; an egui bump becomes a coordinated migration. Pin egui, document the supported version,
  treat bumps as deliberate (§2.11).
- **Breadth vs current-product pressure.** The framework track is committed because a second app is
  planned, but many items still lack a concrete immediate consumer. The risk is *scope creep* if large
  [FRAMEWORK] items interrupt [NOW]/[BOTH] milestones. Discipline: finish shared foundation first, then
  sequence framework-only work by highest reuse, lowest risk, and clearest second-app need.
- **Cross-platform auto-theme dependency.** Windows/macOS can rely on egui 0.34 + winit
  `WindowEvent::ThemeChanged`; Linux needs `dark-light = "2.0"` via XDG Desktop Portal D-Bus and either
  its Listener or periodic re-detect. Gate it by Linux cfg/feature, test Flatpak/portal behavior, and keep
  an app-owned `AppThemeChoice {Auto, Light, Dark}` because egui's resolved `system_theme` is `pub(crate)`.
- **Theme clone cost in Memory.** Reading `theme(ui)` each frame clones the `Theme`. Assume cheap while
  the struct is `Color32` fields, but verify at the final checkpoint after B2/B6/B11; if it is no longer
  trivially cheap, wrap in `Arc<Theme>` so the accessor stays cheap.
- **Feature-flag heavy widgets.** Pro: lean default build, consumers opt in (table/charts/webview/
  theme-json). Con: feature-matrix combinatorics in CI and more `#[cfg]` plumbing. Recommend flags only
  for genuinely heavy/optional deps, not for every widget.
- **Sub-crate split timing.** Splitting into eparts-core/widgets/theme-json now adds release overhead.
  Feature flags are the near-term approach (K2); defer the split to the very end, after all scheduled
  milestones and enough framework work prove the seams.
- **Domain leakage.** `timeline.rs`/`easing_curve_editor.rs` may bake in animatix concepts; if so they
  inflate the "framework" surface with non-reusable code (K1). Either trait-genericize or relocate to GUI.
- **Charts confusion.** GUI chrome has no generic chart consumer today. Keep K11 lowest priority and do
  not conflate bespoke F-curve/easing editors or core animatix plot content with a future framework chart API.

### Open questions / assumptions to confirm
Carried over from the prior doc, now resolved or explicitly deferred:
1. **RESOLVED — Light-mode + auto-theme need.** animatix-gui wants light mode and cross-platform system
   auto-theme. B6 is promoted to scheduled [BOTH] work, and B11 is added for system detection: native
   egui/winit on Windows/macOS, `dark-light = "2.0"` on Linux, app-owned `AppThemeChoice {Auto, Light, Dark}`
   resolving into `ctx.set_theme()`/`eparts::set_theme()` each frame.
2. **RESOLVED — ShortcutRegistry queryability.** Verified `ShortcutRegistry` is key-first
   (`Vec<(egui::KeyboardShortcut, Shortcut)>`) with actions identified by `KeyboardAction`; no reverse
   lookup exists, but app-side changes are acceptable and tiny: derive `Eq`/`PartialEq`/`Hash`, add
   `shortcut_for(&KeyboardAction) -> Option<&KeyboardShortcut>`, linear-scan ~35 entries, and format via
   `KeyboardShortcut::format(&modifiers)`. Toolbar tooltips and the shortcut cheat sheet now use this
   lookup; gesture-only rows remain static.
3. **DEFER-VERIFY-AT-END — Theme clone cost.** Assume cheap for now. At the final checkpoint, after B2
   component slots and light/auto theme work settle, inspect/measure clone cost and switch to `Arc<Theme>`
   if the struct balloons.
4. **RESOLVED — Second-consumer timeline.** A second app is planned. The [FRAMEWORK] backlog is now a
   committed, unscheduled track: all items eventually happen, sequencing is flexible, and [NOW]/[BOTH]
   milestones still go first because they also serve framework goals.
5. **DEFERRED TO END — Sub-crate split now vs later.** Keep feature flags now (K2). Defer
   `eparts-core`/`eparts-widgets`/`eparts-theme-json` until after all other scheduled and high-priority
   framework work is complete, unless packaging boundaries become a hard blocker.
6. **RESOLVED — Charts reuse (K11).** Verified animatix-gui has no generic chart widgets and no
   `egui_plot` dependency. Existing plot-like surfaces are bespoke curve/timeline/FPS widgets or core
   animation content. K11 stays committed but deferred to the bottom; if a framework chart is needed,
   build it fresh against a clean data-binding API, not by extracting F-curve/easing editors and not by
   duplicating/confusing the core animatix plot feature.

---

*This is the detailed eparts plan; `docs/roadmap.md` is the canonical current-work list.
Nothing from the catalogue is dropped — every item carries a scope tag and, if [FRAMEWORK], a
why-defer note that now explains sequencing rather than optionality.*
