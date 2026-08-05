# eparts Design Audit — Consolidated Reconciliation & Improvement Plan

> Two tracks: **Track 1** reconciles `docs/gui_design_language.md` with the shipped
> `crates/eparts` code (make doc and code agree). **Track 2** proposes opinionated
> design-language improvements adapted to immediate-mode egui + a desktop animation IDE.
> No code is edited by this plan; it is a task breakdown. Plans historically lived
> in `.picopi/plans/`; they now live under `docs/plans/`.

---

## 0. Grounding: what I verified in the code

Decisions below are grounded in direct reads, not the audit summaries alone.

| Claim checked | Finding | File:line |
|---|---|---|
| Button variants in code | Only `Primary`, `Ghost`, `Icon`; one size `Medium`. No `Secondary`/`Danger`, no `Small`/`Large`. | `widget/button.rs:14-32` |
| Button `Secondary`/`Small`/`Large` are dead stubs? | **No** — they were *removed entirely* (M3/E3), not left as `#[allow(dead_code)]`. The enums are genuinely 3-variant / 1-size. | `widget/button.rs`, roadmap §6 M3 |
| eparts `Button::secondary/danger/small/large` used anywhere in GUI? | **Never.** Zero call sites. | `rg` across `crates/animatix-gui` |
| `.small()` at export_dialog.rs:479 | It is **egui's native `Button::small()`**, on `egui::Button::new(...)`, not eparts. | `export_dialog.rs:471-480` |
| Delete/destructive buttons in GUI | Done with raw `egui::Button` / `ui.button("… Delete")`, not an eparts danger variant. | `property_groups.rs:492`, `timeline_panel.rs:1805` |
| `theme.button` slots | `primary, secondary, ghost, icon, danger` ALL exist as full state slots (normal/hover/active/selected/disabled). `secondary` + `danger` are wired in the Theme but **no Button variant consumes them.** | `tokens/theme.rs:68-74, 556-558, 654-658` |
| primitive.rs visibility | Entries are `pub`. GUI re-exports them: `pub use eparts::tokens::primitive;` and `use super::primitive as p;` in GUI semantic submodules. | `primitive.rs:16`, `design_tokens/mod.rs:32`, `design_tokens/semantic.rs:20` |
| §5.1 ACTIVE color | Doc says `#3C423A`. Code `GRAY_600 = rgb(60,66,78) = #3C424E`. **Doc is wrong.** | `primitive.rs` GRAY_600, doc §5.1 |
| §5.1 surface "5 layers" | Code defines **6** entries (BASE/PANEL/SURFACE/WIDGET/HOVER/ACTIVE). eparts `semantic.rs:14` *also* mislabels "5 depth layers" in a comment. | `semantic.rs:14-31`, doc §5.1 |
| §5.1 "current values #0C0E12→#121418" | Stale parenthetical; code already uses corrected values. | doc §5.1 |
| Command split (Phase 3) | **Done.** `commands/{actor,document,keyframe,playback,scene,view,mod}.rs` exist. | `crates/animatix-gui/src/app/commands/` |
| `animate_toward_eased` dead? | **Used** at `anim.rs:82` inside `animate_bool_eased`. `#[allow(dead_code)]` is wrong/misleading. | `anim.rs:82,102` |
| Select trigger cursor | `select.rs:163` uses `ui.button(...)` (egui native) → egui shows `PointingHand`. Violates cursor convention. | `select.rs:163` |
| `SPACE_M` legacy aliases | `SPACE_M=SPACE_3` etc. are explicitly marked legacy in `spatial.rs:17-23`; ~30+ uses across widgets. | `spatial.rs:17-23` |
| Row dual entry point | `show()` + `show_in_rect()` both `pub`. But AGENTS.md Tier-2 says one *primary* entry; `show_in_rect` is the rect-mode internal used by Tree/List. | `row.rs:114,129` |

### Source-of-truth principle used throughout

**The shipped eparts code + `crates/eparts/AGENTS.md` are the source of truth for the
component library.** `docs/gui_design_language.md` is the older, pre-extraction design
spec written when tokens lived inside `animatix-gui`. The roadmap (M1–M7 complete)
deliberately diverged from that doc (lean variants, 3-layer crate boundary, runtime
`Theme`). So **most Track-1 conflicts resolve by updating the doc**, with a few
genuine code fixes where the code violates its own stated conventions.

---

## TRACK 1 — CONSISTENCY FIXES

Goal: doc and code agree. Each task is fixer-sized (~1–3 files). Ordered so doc rewrites
(low risk) precede code changes (need build/test).

### T1.1 — Resolve Button-variant conflict → update the DOC to the lean impl
**Source of truth: code.** Three independent signals say the lean impl is correct, not a regression:
1. Roadmap M3/E3 explicitly removed `Secondary`/`Small`/`Large` per principle 6 ("don't pre-build unused variants").
2. Zero GUI call sites use any removed variant/size.
3. AGENTS.md principle 6 codifies "4-tier size vocabulary, wire what you use."

**Change:** Rewrite `docs/gui_design_language.md` §6.2 "Unified Button API" to match
`widget/button.rs`:
- Variants: `Primary`, `Ghost`, `Icon` (drop `Secondary` from the doc enum).
- Size: document `ButtonSize::Medium` only, and state the policy: "additional sizes are
  added when a call site needs them (principle 6), via the shared `Sizable`/`Size` trait —
  not pre-built."
- Constructors: `primary()`, `ghost()`, `icon()`. Builder methods that actually exist:
  `with_icon`, `with_tooltip`, `active`, `icon_color`, `hover_icon_color`, `loading`, `on_hover`.
- Remove the doc's `secondary()`, `small()`, `disabled()` (no `disabled()` builder exists;
  disabled is handled via the field + `loading`/state — note this).
- Add a one-line note: "A `danger`/destructive variant is *seeded in the Theme*
  (`theme.button.danger`) but not yet exposed as a `ButtonVariant`. See Track 2 T2.10."

**Files:** `docs/gui_design_language.md` (§6.2 + §6.3 constraint #2 "all 5 states").
**Risk:** doc-only. Low.
**Verify:** manual read; ensure §6.2 enum matches `button.rs:14-32` exactly.

---

### T1.2 — primitive-visibility conflict → update the DOC to allow `pub` for the library boundary
**Source of truth: code.** The doc's `pub(crate)` rule (§2.1, §2.3) was written when tokens
lived *inside* `animatix-gui`. Now `primitive` lives in the **`eparts` crate** and the GUI's
app-specific semantic submodules (category, diagnostic, curve, editor, timeline, canvas)
reference raw entries via `pub use eparts::tokens::primitive;` + `use super::primitive as p;`.
`pub(crate)` would make this re-export impossible without forcing every app primitive into eparts.

**Decision:** Keep primitives `pub` (code unchanged). The intended invariant ("UI code consumes
semantic, not primitive") is now a **convention enforced by re-export discipline**, not by
`pub(crate)`. Update the doc to say so.

**Change:** In `docs/gui_design_language.md` §2.1 and §2.3:
- §2.1 visibility line: change "Primitive … Visibility: pub(crate)" → "Visibility: `pub`
  (crate-public). eparts is a library; the consuming app's semantic submodules reference
  primitives through `eparts::tokens::primitive`. The layering invariant — *UI/widget code
  imports semantic roles, never primitives directly* — is enforced by convention + code review,
  not by `pub(crate)`."
- §2.3 constraint row "UI code must not import primitive | pub(crate) visibility — compiler error":
  change enforcement to "Convention + review; widgets read `theme(ui)` slots or `semantic::*`,
  never `primitive::*`."
- Drop the inline `pub(crate) const …` example block (or relabel it as the *historic
  single-crate* layout) since `primitive.rs:16` is `pub`.

Also fix the matching mislabel inside the code's own doc comment if touching: optional —
`primitive.rs` header already explains the `pub` rationale, so no code change needed.

**Files:** `docs/gui_design_language.md` (§2.1, §2.2 code sample, §2.3 table).
**Risk:** doc-only. Low.
**Verify:** manual read.

---

### T1.3 — §5.1 Surface depth: fix layer count + ACTIVE color + stale parenthetical
**Source of truth: code.**
**Changes in `docs/gui_design_language.md` §5.1:**
- Heading "Surface Depth (5 layers)" → "Surface Depth (6 levels)". (BASE/PANEL/SURFACE/WIDGET/HOVER/ACTIVE.)
- `ACTIVE #3C423A` → `ACTIVE #3C424E` (= `GRAY_600 = rgb(60,66,78)`).
- Delete the parenthetical "The current values (#0C0E12 → #121418) differ by only ~2.3% — the new values above correct this." (already corrected in code).
- While here, reconcile §2.2 comment "Surface (5 depth layers)" → "(6 levels)" if that block is retained.

**Files:** `docs/gui_design_language.md` (§5.1, §2.2 comment).
**Risk:** doc-only. Low.
**Verify:** cross-check each hex against `primitive.rs` GRAY_* values.

---

### T1.4 — Fix eparts code comment mislabel "5 depth layers" (code, trivial)
`crates/eparts/src/tokens/semantic.rs:14` comment says "Surface (5 depth layers)" but defines 6.
**Change:** comment "5 depth layers" → "6 levels". Code-only, comment.
**Files:** `crates/eparts/src/tokens/semantic.rs`.
**Risk:** none (comment). Low.
**Verify:** `cargo check -p eparts`.

---

### T1.5 — Rewrite §11 Migration Plan as a delta of remaining work
**Source of truth: roadmap + code.** AGENTS.md mandates roadmap/migration sections list ONLY
remaining work. Verified status:
- Phase 1 (token refoundation): **DONE** (3-layer + crate extraction).
- Phase 2 (component unification — Button + TextRole): **DONE** (M3, M5/M6).
- Phase 3 (command split): **DONE** (`commands/` dir exists with 6 domain modules).
- Phase 4 (interaction): motion + keyboard + gesture types **DONE** (M4, `preview/gesture.rs`
  exists). `gesture_router` full replacement of `drag_handler.rs` — **verify residual**: the
  enum/handler types exist; confirm whether `drag_handler.rs` is fully retired before claiming done.

**Change:** Replace the entire §11 (Phases 1–4 + summary table) with a short "Status &
Remaining Work" section:
- One paragraph: "Phases 1–3 complete; see `docs/plans/eparts-refinement-roadmap.md` (M1–M7)
  for the eparts component work and `crates/animatix-gui/src/app/commands/` for the command split."
- A short bullet list of genuinely remaining items only (e.g. "migrate inspector/dialog fields
  onto `Form`", "retire `drag_handler.rs` once gesture router covers all drag modes", "drive
  toolbar tooltips/cheat-sheet from `ShortcutRegistry`"). Pull these from roadmap follow-up notes.

**Files:** `docs/gui_design_language.md` (§11 entire).
**Risk:** doc-only, but must not over-claim — leave the one unverified item (drag_handler retirement) as "verify/remaining".
**Verify:** confirm `commands/` modules exist (done); grep `drag_handler` usage before finalizing the remaining list.

---

### T1.6 — Remove the misleading `#[allow(dead_code)]` on `animate_toward_eased`
**Source of truth: code.** `anim.rs:102` carries `#[allow(dead_code)]` but the fn is called
at `anim.rs:82`. Per AGENTS.md, `#[allow(dead_code)]` must be justified *and* genuinely unused.
**Change:** delete the attribute (and its comment) at `anim.rs:102`. The fn is reachable.
**Files:** `crates/eparts/src/widget/anim.rs`.
**Risk:** if clippy now flags something else, none expected — it's used. Low.
**Verify:** `cargo check -p eparts` + `cargo clippy -p eparts` (no dead_code warning expected).

---

### T1.7 — Select trigger cursor: honor cursor convention
**Source of truth: AGENTS.md principle 3 (code convention).** `select.rs:163` uses
`ui.button(current_label)` → egui paints `PointingHand`. Buttons must show the default arrow.
**Change:** append `.on_hover_cursor(egui::CursorIcon::Default)` to the `button_response`
(or build the trigger via eparts `Button` which already overrides the cursor). Minimal fix:
`let button_response = ui.button(current_label).on_hover_cursor(egui::CursorIcon::Default);`
**Files:** `crates/eparts/src/widget/select.rs`.
**Risk:** low; cosmetic. Verify the popover still toggles on the same response.
**Verify:** `cargo test -p eparts`; manual hover check on the Select in the GUI.

---

### T1.8 — Document Row's two entry points (doc the sanctioned exception, don't break callers)
**Source of truth: code + AGENTS.md Tier-2 rule.** `Row::show()` is the primary entry point;
`show_in_rect()` is the rect-mode path used by `Tree`/`List` (they pre-allocate the rect).
Renaming `show_in_rect` to a builder `.in_rect(rect)` would not fit — it needs the caller's
`row_response` and `painter`, i.e. it's a *different invocation contract*, not a builder option.

**Decision:** Keep both; document `show_in_rect` as a **sanctioned rect-mode variant** of the
single logical entry point (both render the same Row; `show` is the convenience wrapper that
allocates then calls `show_in_rect`). This matches the actual code (`show` calls `show_in_rect`).

**Change:**
- Add a doc comment on `Row` (or on `show_in_rect`) stating it is the rect-mode entry used by
  container widgets that own allocation; `show()` is the standard path. (Code comment already
  half-says this — make it explicit and reference the AGENTS.md Tier-2 exception.)
- Add the exception to `crates/eparts/AGENTS.md` "Widget API contract" section: "Container-driven
  widgets (`Row`) may expose a second rect-mode entry (`show_in_rect`) that takes a pre-allocated
  rect + response + painter; it is the same logical entry, not a competing API."

**Files:** `crates/eparts/src/widget/row.rs` (doc comment), `crates/eparts/AGENTS.md`.
**Risk:** doc-only. Low.
**Verify:** read; `cargo check -p eparts`.

---

### T1.9 — Add missing builder tests
**Source of truth: AGENTS.md principle 2** ("every builder field gets a unit test").
Audit A flags gaps: `DialogSpec`, `ContextMenu`, `EasingCurveEditor`, `Row` (most fields),
`Tree` (`row_height`/`indent_step`). Add `#[cfg(test)]` builder assertions per widget.
Split into per-widget sub-tasks if large:
- T1.9a `dialog.rs` — DialogSpec fields.
- T1.9b `context_menu.rs` — MenuEntry/builder fields.
- T1.9c `easing_curve_editor.rs` — builder fields.
- T1.9d `row.rs` — height/indent/icon/label_color/has_children/right/sense.
- T1.9e `tree.rs` — row_height/indent_step.

**Files:** the five widget files (test modules only).
**Risk:** none (test-only); may surface a builder that doesn't actually set a field — fix if so.
**Verify:** `cargo test -p eparts`.

---

### T1.10 — Replace remaining legacy `SPACE_M`/`SPACE_S`/`SPACE_L` aliases with `SPACE_3/2/4`
**Source of truth: code comment** (`spatial.rs:17` "will be removed once all call sites use the
new scale") + doc §4.4 #2 ("PAD_* deleted; SPACE_* the only system"). ~30+ uses remain.
**Change:** mechanical rename across `crates/eparts/src/widget/*.rs`:
`SPACE_M→SPACE_3`, `SPACE_S→SPACE_2`, `SPACE_L→SPACE_4`, `SPACE_XS→SPACE_1`, `SPACE_XL→SPACE_5`.
Then delete the alias block in `spatial.rs:17-23`.
**Files:** many widget files + `spatial.rs`. Do as one mechanical sweep, but it touches >3
files — split per the fixer rule: do it widget-by-widget in a couple of passes, or as one
clearly-mechanical commit (note in commit message it's pure rename).
**Risk:** mechanical; values identical so no visual change. Compile catches misses.
**Verify:** `cargo check --workspace` (GUI may also reference these aliases — grep first);
`cargo test -p eparts`. **Precondition:** grep `SPACE_M\b|SPACE_S\b|SPACE_L\b` across
`crates/animatix-gui` too before deleting aliases, or keep aliases until GUI is swept.

---

### T1.11 — Low-priority code consistency cleanups (batch, optional)
Group these small items; each is independent. Do only if touching the file for another reason,
or as one "consistency" pass:
- **`show()` self-consumption**: standardize `&self` vs `self` across Popover/Collapsible/
  ResizeHandle/Toast. Decide: `show(self, …)` (consuming) is the AGENTS.md Tier-2 norm — make
  the outliers consuming. (Check each; ResizeHandle returning `f32` is separate, below.)
- **Theme access style**: pick one — `crate::tokens::theme::theme(ui)` is the canonical path;
  alias `use crate::tokens::theme;` then `theme::theme(ui)` is what `button.rs` uses. Standardize
  on the short `theme(ui)` import form across widgets.
- **`ResizeHandle` return type**: returns `f32`; AGENTS.md Tier-2 says rich returns should be a
  named `*Response` struct. Either wrap in `ResizeHandleResponse { delta: f32, … }` or document
  the `f32` as the deliberate minimal return. (Lower value; document is fine.)

**Files:** `popover.rs`, `collapsible.rs`, `resize.rs`, `toast.rs`, assorted.
**Risk:** API-shape changes (self-consumption, return type) can ripple to GUI call sites — grep
first. Keep this task last; it's polish, not correctness.
**Verify:** `cargo check --workspace`, `cargo test -p eparts`, `cargo test -p animatix-gui`.

---

### Track 1 ordering & global verification
1. Doc-only first (no build risk): **T1.1, T1.2, T1.3, T1.5**.
2. Trivial code/comment: **T1.4, T1.6**.
3. Behavior-touching code: **T1.7**, then **T1.8** (doc), **T1.9** (tests).
4. Mechanical sweep: **T1.10** (grep GUI first).
5. Optional polish: **T1.11**.

After each code task and at the end:
```bash
cargo check --workspace
cargo test -p eparts
cargo test -p animatix-gui
cargo test --no-fail-fast
```
(`-p eparts` is the eparts test target; the workspace AGENTS.md list also applies.)

---

## TRACK 2 — DESIGN-LANGUAGE IMPROVEMENTS

Opinionated additions for an **immediate-mode egui desktop animation IDE**. Each item: what,
why, concrete values WE adopt, doc-only vs code, effort, and quick-win vs initiative. Research
(Audit C) cited where it informs a choice; we deviate from web norms where egui/IDE context warrants.

### Quick wins (doc-only or tiny code) — do these first

#### T2.1 — Document existing component-scoped Theme slots (doc-only) — quick win
**What:** §6 of the design doc never documents `theme.button/list/tab/menu/input/scrollbar`
slots that M2 actually built (`tokens/theme.rs:68-74`, etc.). Add a "§6.4 Component Theme Slots"
subsection enumerating the slot taxonomy from the roadmap §3a and pointing at `tokens/theme.rs`
as the source of truth.
**Why:** the doc claims to be "authoritative" but omits the live theming API; consumers can't
discover `button.danger`, zebra `list.even/odd`, `tab.active.indicator`, `input.invalid`.
**Values/naming:** mirror the shipped struct field names exactly (no renaming).
**Effort:** S. **Doc-only.**

#### T2.2 — Document the Tier-2 `show()` API contract in the design doc (doc-only) — quick win
**What:** §6.3 says "Primitive components implement `egui::Widget` — no free functions," which
contradicts the AGENTS.md two-tier reality (`Form`, `Dialog`, `Popover`, `Tree`, `Row` use
`show()`). Add the Tier-1/Tier-2 distinction to §6.3, referencing `crates/eparts/AGENTS.md`.
**Why:** removes a direct doc/code contradiction; the doc currently forbids a pattern the lib relies on.
**Effort:** S. **Doc-only.**

#### T2.3 — Document the cursor convention in the design doc (doc-only) — quick win
**What:** the cursor convention (arrow for buttons/rows, pointer only for links) lives only in
`crates/eparts/AGENTS.md` principle 3. Add it to §7 (Interaction Language) as a constraint.
**Why:** it's a core "native desktop, not web" decision; the design doc is where designers look.
**Values:** "Default arrow cursor for all interactive chrome; `PointingHand` reserved for `Link`
(genuine hyperlinks) only."
**Effort:** S. **Doc-only.**

#### T2.4 — Document `lines` and `overlay` semantic roles (doc-only) — quick win
**What:** §5.2 lists Accent/Status/Category/Border but omits `semantic::lines` and
`semantic::overlay` (both `pub mod` in `semantic.rs:139,154`). Add them.
**Why:** completeness; these are live roles (separators/dividers, scrims/backdrops).
**Effort:** S. **Doc-only.** Inspect `semantic.rs:139-160` for exact constants to list.

#### T2.5 — Concrete SPRING easing params (doc-only, maybe code const) — quick win
**What:** §8.2 SPRING is "slight overshoot" — underspecified. egui has no native spring; our
motion layer is cubic-bezier (`tokens/motion.rs`). A cubic-bezier cannot truly overshoot
(output stays in [0,1] for standard control points), so be honest about this.
**Decision:** Either (a) define SPRING as a perceptual approximation
`cubic-bezier(0.34, 1.56, 0.64, 1.0)` **and note that egui clamps**, or (b) document SPRING as a
two-phase ease (overshoot then settle) implemented in `anim.rs` if a real spring is wanted.
**Recommendation:** ship (a) as the documented value now; flag a real spring as a small code
follow-up only if drag-release snap-back needs it. Material-style standard easings already exist
(`STANDARD/DECELERATE/ACCELERATE` cubic-beziers) — keep those.
**Effort:** S (doc) / S (optional const in `motion.rs`).

#### T2.6 — `prefers-reduced-motion` as a real a11y constraint, not a deferred footnote — quick win (doc) + S (code)
**What:** §8.3 #6 and §10 both orphan reduced-motion. Promote it into §10 Accessibility as a
first-class constraint and define the mechanism.
**Why:** it's an accessibility requirement, not a Phase-2 nicety. Research: reduced-motion → 0ms.
**Decision/values:** add an app-owned `MotionPreference { Full, Reduced }` (mirrors the existing
`AppThemeChoice` pattern from B11). When `Reduced`, `anim::transition`/`Transition` resolves
duration to `INSTANT` (0ms). egui/winit does not expose OS reduced-motion on all platforms, so
default to a Settings toggle (like the theme toggle), optionally OS-detected on platforms that
support it.
**Effort:** doc S; code S (gate duration in the motion helper). **Doc + small code.**

---

### Medium initiatives (doc + meaningful code)

#### T2.7 — Elevation / shadow tokens — medium initiative
**What:** the doc has no elevation/shadow system; surfaces are distinguished only by the 6-level
flat color depth. Add a small shadow token set for *floating* surfaces (popover, dialog, menu,
toast, dropdown).
**Why:** floating overlays currently rely on color alone; a subtle shadow reads as "above" and is
the one place an IDE benefits from depth. Research: mature systems use 3–5 elevation levels.
**Opinionated scope (deviate from web):** a flat IDE does **not** want Material's 24-level dp
system. Adopt **3 elevation levels only**, shadow-as-token:
```
elevation.flat     — no shadow (in-panel chrome)
elevation.raised   — popover/menu/dropdown: soft 1–2px blur, ~25% black
elevation.overlay  — dialog/modal: larger 8–16px blur, ~40% black + existing backdrop scrim
```
egui shadows are `egui::epaint::Shadow { offset, blur, spread, color }` — express each level as a
`Shadow` const in a new `tokens` location (or as `theme.elevation.{raised,overlay}` slots so
light/dark can differ). Apply via `Frame::shadow` / overlay layers.
**Effort:** M. **Doc + code** (`tokens/theme.rs` or `tokens/spatial`/new `elevation` block; apply
in `popover.rs`, `dialog.rs`, `context_menu.rs`, `toast.rs`).

#### T2.8 — Density modes — medium initiative
**What:** doc has fixed row heights; no density toggle. IDEs benefit from a Comfortable/Compact switch.
**Why:** research: dense default + optional density modes for pro/IDE tools. animatix already has
`Size {Xs,Sm,Md,Lg}` — density is the *global* multiplier layer above per-widget size.
**Opinionated decision:** add an app-owned `Density { Compact, Default }` (skip "Comfortable" — a
dense IDE doesn't need three). It scales `ROW_*` and `SPACE_*` by a factor (Compact ≈ 0.875×,
Default = 1×) resolved through the same Memory pattern as Theme. Document in §4 (Spatial) + §9 (Layout).
**Effort:** M. **Doc + code** (spatial resolution via a density-aware accessor; touches the
spatial token reads — non-trivial because current tokens are `const`. Cheapest path: a
`density()` accessor returning a multiplier, applied at row/space *use* sites in eparts widgets,
or a runtime spatial struct mirroring the Theme migration). Flag as the larger of the "medium" items.

#### T2.9 — Expanded per-component state rules: focus / disabled / loading / empty / error — medium
**What:** §6.3 #2 asserts "every Primitive supports all 5 interaction states" but the doc never
*specifies per-component* what focus/disabled/loading/empty/error look like. The code already has
`input.invalid`, `loading`, focus rings, `EmptyState`, `Skeleton`.
**Why:** the contract is stated but unspecified; document the live behavior so it's authoritative.
**Values:** per primitive, a small table: focus = `border.focus` ring (see T2.11); disabled =
`text.disabled` fg + reduced bg; loading = spinner + non-interactive (Button already); empty =
`EmptyState` pattern; error = `input.invalid` border + status.error message.
**Effort:** M (mostly doc, audits each widget). **Doc-only** (documents existing code).

#### T2.10 — Wire a `danger`/destructive Button variant — medium, NOW-relevant
**What:** `theme.button.danger` slots exist and are fully seeded, but **no `ButtonVariant::Danger`
consumes them**; GUI Delete actions use raw `egui::Button`. Add the variant + `Button::danger(label)`.
**Why:** destructive actions (Delete actor, Delete keyframe, Clear) deserve consistent
destructive styling; the Theme is already ready. This is the highest-value Track-2 *code* item and
it resolves the Track-1 T1.1 deferral.
**Values:** `ButtonVariant::Danger` reading `t.button.danger.<state>` (same state machine as
Primary). Constructor `Button::danger(impl Into<String>)`. Then migrate the few GUI Delete sites
(`property_groups.rs:492`, `timeline_panel.rs:1805`) to it.
**Effort:** M. **Code + doc** (`button.rs` variant + paint arm; doc §6.2; GUI call-site migration).
Add to §6.2 once shipped.

#### T2.11 — Focus-ring spec — quick-to-medium
**What:** §7.4 #5 says "2px accent::PRIMARY outline" but offset/inset and per-shape behavior are
unspecified; the code hand-paints focus per Button arm with `t.focus_ring()` and `STROKE_WIDTH`
(`button.rs`), and a `border.focus` slot exists (B5).
**Why:** consolidate into one documented spec; research: 2px accent ring + offset is standard.
**Values:** "Focus ring = `STROKE_WIDTH` (2px) stroke in `theme.border.focus` / `focus_ring()`,
painted **inset by 1px** (`rect.shrink(1.0)`, `StrokeKind::Inside`) to avoid clipping — matches
the current Button implementation. Every focusable primitive uses this identical treatment."
Document in §7.4 and as a §6.4 cross-reference.
**Effort:** S doc; the code already does this consistently (audit confirms Button arms match).
**Doc-only** unless a widget is found not honoring it.

#### T2.12 — Z-index / overlay-layer documentation — medium (doc)
**What:** M4 built a managed overlay coordination layer (`widget/overlay.rs`) with priority
(Dialog < Popover < Tooltip) but it's **undocumented** in the design doc.
**Why:** layering order is a design decision designers/devs must know; it's live code today.
**Values:** document the priority ladder from `overlay.rs` (verify exact order in code) +
`egui::Order` mapping (menus/popovers `Order::Foreground`, tooltips topmost). Add a §7.5
"Overlay Layering" subsection. No new code.
**Effort:** S–M. **Doc-only** (read `overlay.rs` to transcribe the real priority + dismissal rules).

#### T2.13 — Full contrast matrix — medium (doc + verification)
**What:** §10.1 lists only 2 violations; no full text/bg contrast matrix.
**Why:** AA (4.5:1) is claimed but unproven across the palette. Build the matrix for the key
pairs (each `text.*` over each `surface.*`, status colors over their faint bgs, on_accent over
accent).
**Decision:** stay on **WCAG 2 (4.5:1 body, 3:1 large/UI)** for now; APCA is a WCAG3 candidate —
**defer** (see Skip list). Compute ratios for dark *and* light themes.
**Effort:** M. **Doc** (matrix) + possibly small **code** token tweaks if a pair fails. Could be
scripted as a test that asserts contrast for critical pairs (nice-to-have).

#### T2.14 — Light-theme concrete values in the doc — medium (doc)
**What:** `Theme::light()` ships (M2) but the doc only shows dark values and §2.3 still lists
"every color must exist in both themes" as future Phase-2 work.
**Why:** light mode is done; the doc must document it as authoritative. Transcribe `Theme::light()`
surface/text/accent values into §5 alongside dark.
**Effort:** M. **Doc-only** (read `theme.rs` light palette ~lines 319-420).

#### T2.15 — Iconography rules — medium (doc)
**What:** no iconography section. The lib standardizes on `egui-phosphor`.
**Why:** icon sizing/weight/usage should be a documented rule (icon font = `TextRole::Body` size,
phosphor `regular` weight, semantic pairing color = text color of context).
**Values:** "Icons: egui-phosphor `regular`. Default icon size = `TextRole::Body` (13px) font;
toolbar/icon-button icons centered in `ROW_M` slot. Icon color follows the slot `fg`, never a
hardcoded color. Status icons always pair with text (triple-encoding, §10.3)."
**Effort:** S–M. **Doc-only.**

#### T2.16 — IDE / command-palette tokens — medium
**What:** research flags IDE-specific tokens (grid/ruler/guide/selection) — animatix already has
`semantic::canvas` (grid_line, guide_line, selection_marquee, snap_guide, handle). The gap is
**documenting** them in the design doc as the IDE token set, plus a small command-palette token
note (the palette exists at `command_palette.rs`).
**Why:** these are the IDE-distinct tokens; doc them as a first-class group, not buried in §2.2 canvas.
**Effort:** S–M. **Doc-only** (canvas tokens already exist in code; document + add a
"command-palette uses `surface.overlay` + `elevation.overlay`" line once T2.7 lands).

---

### Code-hygiene items folded from Track 1 audit (small, Track-2-flavored)

#### T2.17 — Replace hardcoded colors with tokens — quick wins
Audit A flagged literals violating principle 1 ("never hardcode colors"):
- `color_picker.rs:131` `from_gray(180)` checkerboard → a `semantic`/theme neutral.
- `theme.rs:559`/`336` danger-active `(200,40,40)` → already a named local `danger_active`; acceptable
  as a *theme-internal* literal (theme.rs is where literals are allowed) — **document this exception**
  in AGENTS.md rather than "fixing." Light-palette inline literals (`theme.rs:319-401`) are likewise
  legitimate inside `Theme::light()`.
**Decision:** Only `color_picker.rs:131` is a real violation (a *widget* hardcoding a color).
theme.rs literals are the *definition* layer and are fine — clarify in AGENTS.md that color literals
are permitted exclusively inside `tokens/theme.rs` and `tokens/primitive.rs`.
**Effort:** S. **Code** (color_picker) + **doc** (AGENTS.md note).

#### T2.18 — Toggle: use spatial tokens for dimensions — quick win
`toggle.rs:55,257,401` hardcode dimensions. Replace with spatial tokens / `Size` mapping where a
token fits; where a switch needs a bespoke pixel size, add a named `const` in `tokens/spatial::component`
with a comment, rather than an inline literal.
**Effort:** S. **Code.**

#### T2.19 — Button/ProgressBar adopt shared `Size`/`Sizable` — medium
`button.rs:37` (`ButtonSize::Medium` local enum) and `feedback.rs` ProgressBar (`height = 16.0`
hardcoded) bypass the shared `Size`/`Sizable` trait that exists in `traits.rs`. Migrate Button to
`Size` (this also naturally enables Sm/Lg per principle 6 when wired — ties to T1.1). ProgressBar
height → a spatial token or `Size`-derived value.
**Why:** principle 4 ("shared trait contracts over per-widget enums"). Button's local `ButtonSize`
is exactly the duplication the trait was built to remove.
**Effort:** M. **Code** (`button.rs`, `feedback.rs`). Coordinate with T1.1 (don't pre-build unused
sizes; just route Medium through `Size::Md`).

#### T2.20 — ToastQueue cross-frame state location — medium (design clarification)
`toast.rs:108` keeps cross-frame state in the widget struct, blurring the state contract (AGENTS.md:
cross-frame state → `ctx.data` or app struct). Either (a) move queue state into `ctx.data`/app-owned,
or (b) explicitly document `ToastQueue` as an *app-owned state manager* (not a per-frame widget),
which is the more honest framing.
**Decision:** document it as an app-owned manager (option b) — it already lives in `ui_store`-style
ownership; add the clarification to AGENTS.md state-management contract.
**Effort:** S (doc) or M (refactor). Prefer doc unless reuse demands the refactor.

---

### Deliberately SKIP / defer (with rationale)

- **RTL / i18n (K4):** defer. animatix-gui is single-locale; egui RTL support is limited and the
  IDE has no localization requirement. Revisit only when a shipping general-app consumer needs it
  (roadmap already tags i18n [FRAMEWORK]).
- **APCA contrast (WCAG3):** defer. It's a candidate spec, not adopted; tooling is immature. Stay on
  WCAG 2 (4.5:1) which we can compute reliably now (T2.13). Revisit when APCA stabilizes.
- **OKLCH primitive palette (Tailwind v4):** skip for now. Our primitives are fixed `Color32` sRGB;
  OKLCH only pays off when *generating* scales/computed hover shades — relevant to JSON themes (B9,
  [FRAMEWORK]), not to the hand-authored dark/light palettes.
- **Radix 12-step scales:** skip wholesale. Our 6-surface + named role model is sufficient for a
  dark-first IDE; a 12-step-per-hue scale is overkill and would bloat the token surface. Borrow only
  the *idea* (semantic per-step meaning) where useful, not the full system.
- **Gradient tokens (B10):** skip. Flat IDE surfaces don't need gradients; egui gradient fills are
  manual mesh work. Already [FRAMEWORK]-deferred.
- **Screen-reader / AccessKit / ARIA full pass (K5):** keep only the **baseline** (don't break
  focus order/labels). Full AccessKit is a shipped-product concern; egui's AccessKit support is
  partial. Note the baseline in §10, defer the full pass.
- **24px vs 44px target sizing by modality:** document the rule (24px AA minimum for desktop/mouse,
  note 44px is the AAA touch target) but **do not** redesign for touch — animatix is a desktop
  mouse/keyboard IDE. One sentence in §10.2, no code.

---

## Files to touch (consolidated)

Track 1 (doc):
- `docs/gui_design_language.md` — §2.1/2.2/2.3 (T1.2), §5.1/§2.2 comment (T1.3), §6.2/6.3 (T1.1), §11 (T1.5).

Track 1 (code):
- `crates/eparts/src/tokens/semantic.rs` — comment fix (T1.4).
- `crates/eparts/src/widget/anim.rs` — remove dead_code attr (T1.6).
- `crates/eparts/src/widget/select.rs` — cursor override (T1.7).
- `crates/eparts/src/widget/row.rs` + `crates/eparts/AGENTS.md` — entry-point doc (T1.8).
- `crates/eparts/src/widget/{dialog,context_menu,easing_curve_editor,row,tree}.rs` — builder tests (T1.9).
- `crates/eparts/src/widget/*.rs` + `tokens/spatial.rs` — SPACE_* alias sweep (T1.10).
- `crates/eparts/src/widget/{popover,collapsible,resize,toast}.rs` — polish (T1.11).

Track 2 (doc-heavy):
- `docs/gui_design_language.md` — new §6.4 (slots), §6.3 Tier note, §7 cursor + §7.5 overlay layering,
  §5 light values + lines/overlay roles, §8.2 SPRING, §10 reduced-motion + contrast matrix + targets,
  iconography, IDE/command-palette tokens.
- `crates/eparts/AGENTS.md` — color-literal exception note, ToastQueue framing, Row exception.

Track 2 (code):
- `crates/eparts/src/widget/button.rs` — Danger variant (T2.10), `Size` adoption (T2.19).
- `crates/eparts/src/tokens/theme.rs` (or new elevation block) — elevation/shadow tokens (T2.7).
- spatial/density accessor — density modes (T2.8).
- `crates/eparts/src/widget/{color_picker,toggle,feedback}.rs` — token cleanups (T2.17/18/19).
- motion helper — reduced-motion gate (T2.6).
- GUI Delete sites — danger button migration (T2.10).

---

## Risks (ordered)

1. **T1.10 alias sweep crossing the crate boundary** — GUI may import `SPACE_M` etc.; deleting
   aliases breaks the GUI build. Mitigation: grep `crates/animatix-gui` first; keep aliases until
   GUI is also swept, or sweep both crates in one commit. Highest breakage risk in Track 1.
2. **T2.8 density modes** — current spatial tokens are `const`; runtime density needs an accessor
   layer touching many use sites (like the Theme migration). Largest Track-2 effort; scope creep risk.
   Mitigation: ship as accessor + apply incrementally; don't convert all consts at once.
3. **T2.10 Danger variant** — adding a `ButtonVariant` is additive (safe), but the paint arm must
   replicate the focus-ring/loading logic of the other arms or it'll drift. Mitigation: T2.19
   (shared state machine) first, so Danger is just a slot lookup.
4. **T1.11 self-consumption / ResizeHandle return change** — API-shape changes ripple to GUI call
   sites. Mitigation: grep call sites; keep as last, optional task; prefer documenting over changing.
5. **T1.7 select cursor** — verify the `.on_hover_cursor` doesn't consume/replace the response the
   popover toggles on (it returns the same response, so safe, but confirm).
6. **§11 / T1.5 over-claiming "done"** — the one unverified item (full `drag_handler.rs` retirement)
   must be left as "remaining/verify" not "done," per AGENTS.md honesty about unverified state.
7. **T2.13/T2.14 light-theme contrast** — transcribing/contrast-checking light values may surface a
   failing pair needing a token tweak (code change in `theme.rs`), expanding a "doc" task into code.

## Verification (all code tasks)
```bash
cargo check --workspace        # catches GUI/analyzer/LSP drift (GUI excluded from bare check)
cargo test -p eparts
cargo test -p animatix-syntax
cargo test -p animatix --lib
cargo test -p animatix-gui
cargo test --no-fail-fast
```
Plus a GUI visual smoke check for T1.7 (select cursor), T2.7 (shadows), T2.8 (density), T2.10
(danger buttons), T2.14 (light theme).
