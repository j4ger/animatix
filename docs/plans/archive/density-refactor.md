# Density Mode — Token-Level Refactor Plan

Runtime Density mode (`Compact ≈ 0.875×`, `Default = 1×`) that scales spatial
tokens across `crates/eparts` and `crates/animatix-gui`, mirroring the existing
`Theme` / `MotionPreference` Memory-stored runtime token systems.

## Goal

Add a Memory-stored `Density { Compact, Default }` preference and a resolved
`Spatial` accessor so that spacing and row-height tokens shrink in Compact mode,
without regressing Default mode (must stay byte-identical to today's consts).

---

## 1. Storage & API (decided)

### Preference enum — mirror `MotionPreference` exactly

New in `crates/eparts/src/tokens/spatial.rs` (or a sibling `density.rs`; keep it
in `spatial.rs` so the resolver and its inputs live together):

```rust
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Density {
    #[default]
    Default,   // 1.0×
    Compact,   // 0.875×
}

fn density_key() -> egui::Id { egui::Id::new("eparts_density") }

pub fn density_from_ctx(ctx: &egui::Context) -> Density {
    ctx.data(|d| d.get_temp::<Density>(density_key())).unwrap_or_default()
}
pub fn density(ui: &egui::Ui) -> Density { density_from_ctx(ui.ctx()) }
pub fn set_density(ctx: &egui::Context, d: Density) {
    ctx.data_mut(|d2| d2.insert_temp(density_key(), d));
}

impl Density {
    pub fn factor(self) -> f32 { match self { Density::Default => 1.0, Density::Compact => 0.875 } }

    /// Scale a spatial pixel value. Default returns the input UNCHANGED
    /// (no multiply, no round) so Default is byte-identical to the base const.
    /// Compact multiplies then rounds to a whole pixel.
    pub fn scale(self, px: f32) -> f32 {
        match self {
            Density::Default => px,
            Density::Compact => (px * 0.875).round(),
        }
    }
}
```

This is a verbatim structural copy of `motion.rs`'s
`MotionPreference` + `motion_preference(ui)` + `set_motion_preference` +
`resolve_duration` pattern, so it is consistent with the existing codebase.

### Accessor shape — DECISION: Option (a) `Spatial` struct, scoped to scaled tokens (a hybrid in effect)

Read a resolved, `Copy` `Spatial` struct via `eparts::spatial(ui)` at the top of
each widget's `ui()`, exactly like `let t = eparts::theme(ui);`:

```rust
#[derive(Clone, Copy, Debug)]
pub struct Spatial {
    pub space_0: f32, pub space_1: f32, pub space_2: f32, pub space_3: f32,
    pub space_4: f32, pub space_5: f32, pub space_6: f32, pub space_7: f32, pub space_8: f32,
    pub row_xs: f32, pub row_s: f32, pub row_m: f32, pub row_l: f32,
    pub toggle: ToggleDims,        // scaled control sizes
    pub component: ComponentDims,  // scaled chrome dims
}

impl Spatial {
    pub fn for_density(d: Density) -> Self {
        Self {
            space_0: d.scale(SPACE_0), /* … */ space_8: d.scale(SPACE_8),
            row_xs:  d.scale(ROW_XS),  /* … */ row_l:   d.scale(ROW_L),
            toggle:  ToggleDims::for_density(d),
            component: ComponentDims::for_density(d),
        }
    }
}
pub fn spatial(ui: &egui::Ui) -> Spatial { Spatial::for_density(density_from_ctx(ui.ctx())) }
pub fn spatial_from_ctx(ctx: &egui::Context) -> Spatial { Spatial::for_density(density_from_ctx(ctx)) }
```

Why this shape over `scaled(ui, SPACE_3)` (Option b):

- **Ergonomics across ~400 sites.** Most widgets read several tokens. One
  `let s = spatial(ui);` then `s.space_3`, `s.row_m` is fewer characters and one
  Memory read per `ui()`, versus `scaled(ui, SPACE_3)` which re-reads Memory on
  every call and is noisier. It is the identical mental model widgets already use
  for `let t = theme(ui);`.
- **Performance.** One `ctx.data` HashMap lookup of a `Copy` struct per widget
  per frame — the same cost the codebase already pays for `theme(ui)`. Negligible.
- **Rounding correctness.** Rounding to whole pixels happens exactly once, inside
  `Density::scale`, at resolution time. Widgets never see fractional Compact
  values, so there are no per-site rounding divergences. egui still rounds to
  physical pixels at paint, but feeding it pre-rounded logical values avoids 1px
  seams between adjacent rects that round differently.
- **Byte-identical Default by construction.** `Density::Default` returns the input
  unchanged, so `Spatial::for_density(Default).space_n == SPACE_n` for all n. The
  guarantee is provable with a unit test (see §6).

Why **hybrid** (struct covers only the scaled tokens; non-scaled tokens stay
plain consts): `STROKE_*` and `RADIUS_*` do not scale (see §2), so there is no
reason to route them through the struct. Leaving them as consts means the ~half
of read-sites that touch only strokes/radii need **zero** migration, roughly
halving the real blast radius.

### For tokens used outside a `ui()` scope

Use `density(ui).scale(BASE_CONST)` at the `ui()`/`fn(ctx)` use-site (e.g. a
struct field that holds a base value, or a `Size` trait result). Same rounding,
same source of truth.

### Container widgets that pre-allocate rects (Row / Tree / List)

These store a base height in a field (`Row.height`, `List.row_height`,
`Tree.row_height`/`indent_step`, default = `ROW_M` / `SPACE_4`). The field keeps
the **base** value; the widget scales it at paint time inside `ui()`:
`let h = density(ui).scale(self.row_height);`. This scales both the default and
any caller-overridden height consistently (a caller asking for `ROW_L` also
compacts), and there is no stale-dimension problem because the scale is applied
every frame from live Memory.

---

## 2. Which tokens scale (classification)

| Group | Token(s) | Scales? | Why |
|---|---|---|---|
| Spacing | `SPACE_0..8` | **Yes** | The whole point — gaps/margins/padding density. |
| Row heights | `ROW_XS/S/M/L` | **Yes** | Row chrome must compact. |
| Stroke widths | `STROKE_WIDTH(_THICK/_THIN)` | **No** | Hairlines must stay crisp; `1.0→0.875` is a blurry sub-pixel border. Keep as consts. |
| Corner radii | `RADIUS_S/M/L/XL` | **No** | Cosmetic, tiny absolute values (2–8px); `4→3.5` rounds awkwardly with negligible benefit. Keeping them avoids touching every `RADIUS_*`/`as u8` site. |
| `toggle::*` | CHECKBOX/RADIO/SWITCH sizes | **Yes** | Controls are tied to `ROW_XS`; they should compact with rows. |
| `component::*` | PILL_TAB_HEIGHT, PILL_TAB_GAP, TOAST_HEIGHT, TOAST_SPACING, TOAST_MARGIN, ICON_SLOT_WIDTH, PROGRESS_BAR_HEIGHT | **Yes** | Chrome dimensions. |
| `component::TOAST_WIDTH` | 280 | **No** | Content width; shrinking clips text. |
| `menu::*` | MIN_WIDTH, ICON_WIDTH, CHECK_WIDTH | **No** | Content/icon sizing; SHADOW_OFFSET_Y/BLUR are `i8` shadow geometry — never scale. |
| `dialog::INNER_MARGIN, SCREEN_MARGIN` (derive SPACE_5/7) | **Yes** | Spacing-derived; scale at use-site. |
| `dialog::COL_GAP` (= SPACE_4) | **Yes** | Spacing. |
| `dialog::SLIDE_PX, KEY_COL_FRAC, KEY_COL_MAX, SINGLE_COL_THRESHOLD, MAX_VIEWPORT_FRAC` | **No** | Animation distance + layout thresholds/fractions, not density chrome. |
| GUI `preview::*` | HANDLE_SIZE, HIT_RADIUS, CROSS_SIZE, ROTATION_*, DASH/GAP, MIN_*, buffers | **No** | Canvas-space interaction affordances; shrinking hurts usability and is not "UI density". |
| GUI `toolbar::HEIGHT` | 28 | **Yes** | Chrome. |
| GUI `timeline::*` | TRACK_ROW_HEIGHT, RULER_HEIGHT, RANGE_HEIGHT, PLAYBACK_STRIP_HEIGHT, LABEL_COL_WIDTH | **Yes** | Row/chrome heights & label gutter. |
| GUI `timeline::KF_HALF` | 4 | **No** | Keyframe-diamond marker legibility / hit target. |
| GUI `inspector::ROW_HEIGHT` (= ROW_M), `COL_GAP` | **Yes** | Row + spacing. |
| GUI `inspector::INPUT_WIDTH_*`, `LABEL_MIN/MAX_WIDTH`, `KF_COL_WIDTH`, `KF_BTN_WIDTH`, `*_FRAC` | **No** | Content field widths / icon buttons / fractions; shrinking clips numbers. |
| GUI `welcome::BTN_HEIGHT` | 36 | **Yes** | Chrome. |
| GUI `welcome::TOP_OFFSET_FRAC` | **No** | Fraction. |

Rationale summary: scale spacing + row/chrome heights + controls; never scale
strokes, radii, content widths, fractions, thresholds, canvas affordances, or
shadow geometry. A density mode that shrinks strokes/widths/handles "looks
broken," so those are deliberately excluded.

---

## 3. Const-context handling (highest risk)

Sites that **cannot** call `spatial(ui)` because there is no `ui`/`ctx` in scope.
Strategy: **base consts remain the single source of truth for Default; every
const-context keeps using the base const; scaling is applied only at the
`ui()`-scoped use-site.**

| Category | Concrete sites | Handling |
|---|---|---|
| Const-from-const | `eparts spatial.rs` `dialog::INNER_MARGIN = SPACE_5`, `SCREEN_MARGIN = SPACE_7`; GUI `spatial.rs` `dialog::COL_GAP = SPACE_4`, `inspector::ROW_HEIGHT = ROW_M` | Keep the const as the **base**. In the dialog/inspector `ui()`, read scaled via `density(ui).scale(spatial::dialog::INNER_MARGIN)`. |
| Module consts referencing consts | `widget/context_menu.rs` `MENU_ITEM_HEIGHT = ROW_M`, `MENU_ICON_GAP = SPACE_2`, `MENU_SHORTCUT_GAP = SPACE_4` | Delete these three module consts; inline at the `ui()`-scoped use sites as `s.row_m` / `s.space_2` / `s.space_4` (the fns already build a layout inside `ui()`). |
| Trait methods without `ctx` | `widget/traits.rs` `Size::row_height()`, `pad_x()`, `pad_y()` (scale) and `radius()` (does not) | Keep the base methods (used by tests + as the Default path). Add density-aware variants `row_height_for(self, Density)`, `pad_x_for`, `pad_y_for` that delegate to `Density::scale(base)`. Migrate the widget call sites that lay out to the `_for(density(ui))` variants. `radius()` is unchanged (radii don't scale). |
| Struct default/`new()` initializers | `Row{height: ROW_M}`, `List{row_height: ROW_M}`, `Tree{row_height: ROW_M, indent_step: SPACE_4}`, `RowConfig` defaults | Default stays the base const (no ctx needed). Scale the **field value** at paint inside `ui()`: `density(ui).scale(self.row_height)`. Scales both default and overrides consistently. |
| Array/fraction consts | `dialog::MAX_VIEWPORT_FRAC: [f32;2]`, `*_FRAC`, `*_THRESHOLD` | Excluded from scaling (§2) — no change. |
| `as u8` / `as i8` casts | `RADIUS_* as u8`, `menu::SHADOW_* : i8` | Non-scaling tokens — no change. |

Grep gates used to find these (run before declaring §3 complete):
`grep -n "const [A-Z_]*.*\b(SPACE_|ROW_)" crates/*/src` and
`grep -n "= ROW_|= SPACE_" crates/*/src`.

---

## 4. Cross-crate boundary

The GUI today re-exports eparts spatial consts individually and defines
app-specific submodules inline. Mirror how `design_tokens/semantic.rs` wraps
eparts semantic roles:

1. **Keep** the individual `pub use eparts::tokens::spatial::SPACE_*` /
   `ROW_*` / `RADIUS_*` / `STROKE_*` re-exports — they remain the base consts for
   const-contexts and non-scaling sites (no churn).
2. **Add a GUI `Spatial` resolver** that composes the eparts struct with
   app-specific scaled submodules:

```rust
// crates/animatix-gui/src/app/design_tokens/spatial.rs
pub use eparts::tokens::spatial::{Density, density, density_from_ctx, set_density};

#[derive(Clone, Copy, Debug)]
pub struct Spatial {
    pub base: eparts::tokens::spatial::Spatial, // space_*, row_*, toggle, component
    pub toolbar: ToolbarSpatial,    // HEIGHT
    pub timeline: TimelineSpatial,  // TRACK_ROW_HEIGHT, RULER_HEIGHT, RANGE_HEIGHT, PLAYBACK_STRIP_HEIGHT, LABEL_COL_WIDTH
    pub inspector: InspectorSpatial,// ROW_HEIGHT, COL_GAP
    pub welcome: WelcomeSpatial,    // BTN_HEIGHT
    pub dialog: DialogSpatial,      // INNER_MARGIN, SCREEN_MARGIN, COL_GAP
}
impl Spatial { pub fn for_density(d: Density) -> Self { /* d.scale(base const) per field */ } }
pub fn spatial(ui: &egui::Ui) -> Spatial { Spatial::for_density(density(ui)) }
```

   Non-scaling app values (preview handles, input widths, KF_*, fractions,
   thresholds) stay as plain `pub const` in their existing submodules and are not
   added to the struct. App code reads `spatial(ui).base.space_4` for generic
   tokens and `spatial(ui).timeline.track_row_height` for app chrome, while still
   reading e.g. `spatial::preview::HANDLE_SIZE` as a const.

This keeps the wrapper consistent with `semantic.rs`: generic things flow from
eparts, app-specific things layer on top, all scaling through one shared
`Density::scale`.

---

## 5. Migration waves (each keeps the build GREEN)

**Partial-migration invariant (confirmed safe):** un-migrated sites keep reading
the base const, which `Density::Default` reproduces byte-for-byte. So in Default
mode the UI is pixel-identical at every intermediate step. Compact mode is only
fully correct once all scaled sites migrate, but Compact is opt-in and not the
default, so mid-migration `main` is shippable.

### Wave 1 — Core (eparts)
- Files: `crates/eparts/src/tokens/spatial.rs`, `crates/eparts/src/lib.rs`.
- Add `Density`, `density`/`density_from_ctx`/`set_density`, `Density::scale`,
  `ToggleDims`/`ComponentDims` + their `for_density`, `Spatial` + `for_density`,
  `spatial`/`spatial_from_ctx`. Re-export from `lib.rs` next to the
  `MotionPreference` line.
- Add the unit tests from §6 in `spatial.rs`.
- Verify: `cargo test -p eparts` (new tests pass), `cargo check --workspace`.
- Risk: none — purely additive, no existing site changed.

### Wave 2 — eparts widgets (in coherent groups, ~1–3 files each)
Migrate scaled-token reads to `let s = spatial(ui); s.space_n` / `s.row_m`, and
container heights to `density(ui).scale(self.row_height)`. Leave `STROKE_*` and
`RADIUS_*` untouched.
- 2a: `widget/traits.rs` — add `row_height_for`/`pad_x_for`/`pad_y_for`; keep base methods + tests.
- 2b: `widget/row.rs`, `widget/list.rs`, `widget/tree.rs` — field-scaling at paint; `SPACE_*` reads.
- 2c: `widget/context_menu.rs` — delete the 3 derived consts, inline scaled; `ROW_*`/`SPACE_*`.
- 2d: `widget/toggle.rs` — `s.toggle.*`, `ROW_XS`, `SPACE_3` via struct.
- 2e: `widget/layout.rs` (largest), `widget/feedback.rs`, `widget/toast.rs`.
- 2f: `widget/input.rs`, `widget/form.rs`, `widget/collapsible.rs`, `widget/tooltip.rs`, `widget/kbd.rs`.
- 2g: `widget/button.rs`, `widget/tabs.rs`, `widget/timeline.rs`, `widget/diagnostics.rs`, `widget/resize.rs`, `widget/dialog.rs`, `widget/easing_curve_editor.rs`, `widget/color_picker.rs`, `widget/popover.rs`.
- 2h: `tokens/spatial.rs` `dialog` submodule consumers — ensure dialog margins read scaled in the dialog widget `ui()`.
- After each sub-wave: `cargo test -p eparts` + `cargo check --workspace`.
- Risk: missing a scaled site → only affects Compact; Default stays identical.

### Wave 3 — GUI design_tokens wrapper
- File: `crates/animatix-gui/src/app/design_tokens/spatial.rs` (+ `mod.rs` if a re-export is needed).
- Add GUI `Spatial`, the app submodule `*Spatial` structs, `for_density`,
  `spatial(ui)`, and re-export `Density`/`density`/`set_density`. Keep all existing
  const re-exports and non-scaling submodule consts.
- Verify: `cargo check --workspace`, `cargo test -p animatix-gui`.
- Risk: none — additive; no call site changed yet.

### Wave 4 — GUI panels/shell (in groups)
Migrate scaled reads to the GUI `spatial(ui)`; group by panel directory so each
batch is one coherent area (~1–3 files):
- 4a: toolbar / shell chrome.
- 4b: timeline panel(s).
- 4c: inspector panel(s).
- 4d: welcome / dialogs / command palette / settings chrome.
- 4e: remaining misc panels.
- After each: `cargo check --workspace` + `cargo test -p animatix-gui`.
- Risk: as Wave 2 — Compact-only impact mid-migration.

### Wave 5 — Settings toggle + persistence (mirror `reduce_motion` exactly)
- `app/stores/ui_store.rs`: add `pub density: eparts::Density` to `ViewStore` + init in `new()` (default `Density::Default`).
- `app/persistence.rs`: add `#[serde(default)] pub density: String` (or an enum with serde default) to `SettingsPersistence` near `reduce_motion`.
- `app/mod.rs`: load (`ui_store.view.density = match s.density …`) at ~L466 and save (`density: …`) at ~L1000, mirroring the `app_theme`/`reduce_motion` lines.
- `app/shell/settings.rs`: add a Density control next to the "Snap animations" checkbox (segmented Default/Compact, bound to `ui_store.view.density`).
- `app/runtime.rs`: add `applied_density: Option<eparts::Density>` field; init in `new()`; per-frame sync block right after the motion-preference block — `if self.applied_density != Some(d) { eparts::set_density(ui.ctx(), d); self.applied_density = Some(d); }`.
- Verify: `cargo check --workspace`, `cargo test -p animatix-gui`; manual toggle smoke test.
- Risk: persistence schema — guarded by `#[serde(default)]` so old config files still load.

### Wave 6 — Final verification
- Run the full §6 strategy. Grep completeness gate (zero bare scaled-const reads
  in migrated files). Update `docs/roadmap.md` (remove the density item) and any
  user-facing settings doc.

---

## 6. Verification strategy

Per batch:
```bash
cargo check --workspace
cargo test -p eparts
cargo test -p animatix-gui
```

Byte-identical-Default unit tests (in `eparts/src/tokens/spatial.rs`):
```rust
#[test]
fn default_density_matches_base_consts() {
    let s = Spatial::for_density(Density::Default);
    assert_eq!(s.space_0, SPACE_0); /* … space_8 … */ assert_eq!(s.space_8, SPACE_8);
    assert_eq!(s.row_xs, ROW_XS); assert_eq!(s.row_m, ROW_M); assert_eq!(s.row_l, ROW_L);
    // scale() identity on Default for arbitrary values incl. non-integers
    for px in [0.0, 0.5, 1.5, 6.0, 13.0, 27.5] { assert_eq!(Density::Default.scale(px), px); }
}

#[test]
fn compact_density_shrinks_and_rounds_to_whole_px() {
    let s = Spatial::for_density(Density::Compact);
    assert!(s.space_8 < SPACE_8);
    assert_eq!(s.space_8, (SPACE_8 * 0.875).round()); // 28
    assert_eq!(s.space_8.fract(), 0.0);               // whole pixel
}

#[test]
fn density_memory_round_trip() {
    let ctx = egui::Context::default();
    set_density(&ctx, Density::Compact);
    assert_eq!(density_from_ctx(&ctx), Density::Compact);
}
```

Completeness grep (run in Wave 6; expect zero hits in migrated files):
```bash
# scaled tokens still read bare instead of via spatial(ui), in files that have a ui scope
grep -rn "\bSPACE_[0-8]\b\|\bROW_\(XS\|S\|M\|L\)\b" crates/eparts/src/widget crates/animatix-gui/src/app \
  | grep -v "spatial(ui)\|for_density\|_for(\|= SPACE_\|= ROW_\|scale(\|::SPACE_\|::ROW_"
```
Treat remaining hits as either intentional base/const-context (documented) or a
missed migration.

Visual: no screenshot harness exists, so the guarantee is the Default-identity
test above plus a manual Compact toggle pass over each migrated panel after its
wave.

---

## 7. Risks & rollback

1. **Missed scaled sites (Compact looks uneven).** Mitigation: the Wave 6 grep
   gate + per-wave manual Compact pass. Impact is Compact-only; Default is safe.
2. **Rounding inconsistency.** Mitigation: rounding lives only in
   `Density::scale`; all resolvers (eparts `Spatial`, GUI `Spatial`, `Size::*_for`,
   field-scaling) call it, so adjacent rects round identically.
3. **Memory-read perf.** One `Copy`-struct HashMap read per widget per frame —
   identical to the proven `theme(ui)` cost. Negligible.
4. **Container widgets pre-allocating with stale dims.** Avoided by scaling at
   paint from live Memory every frame (no cached resolved height stored across
   frames).
5. **GUI/eparts drift.** Both share `eparts::Density::scale`; GUI `Spatial.base`
   is the eparts `Spatial`, so the generic scale logic exists in one place.
6. **Persistence breakage.** `#[serde(default)]` on the new field keeps old
   config files loadable.
7. **Wrong tokens scaled (strokes/radii/widths/handles).** Avoided by the
   explicit §2 classification; strokes/radii/content-widths/canvas-affordances
   are deliberately excluded.

Rollback: each wave is independently revertible; reverting Wave 5 (the toggle)
leaves Density at `Default` everywhere, which is byte-identical to pre-refactor
behavior even with Waves 1–4 merged.

**Effort estimate:** Wave 1 ~0.5 day; Wave 2 ~1.5–2 days (largest, ~20 widget
files); Wave 3 ~0.5 day; Wave 4 ~1.5–2 days (GUI panels); Wave 5 ~0.5 day; Wave 6
~0.5 day. Total ≈ **5–6 focused days**, dominated by the two widget/panel waves.
