# Final Plan — Extract `eparts` (egui widget + token crate) from `animatix-gui`

## Goal
Extract the domain-free egui widgets and the generic half of the design-token
system out of `crates/animatix-gui` into a new reusable workspace crate
`crates/eparts`, while introducing a single workspace-level source of truth for
the egui/eframe/egui-phosphor dependency stack so `eparts` and `animatix-gui`
can never drift on versions.

---

## Verified facts (re-confirmed against the tree)
- Root `Cargo.toml` has only `[workspace]` (6 members) + `resolver = "2"`. **No
  `[workspace.dependencies]` table exists today.** All versions decentralized.
- `animatix-gui/Cargo.toml` pins the egui stack directly:
  - `eframe = { version = "0.34", default-features = false, features = ["default_fonts","wgpu","x11","wayland"] }`
  - `egui = { version = "0.34", features = ["serde"] }`
  - `egui_extras = { version = "0.34", features = ["syntect"] }`
  - `egui_tiles = { version = "0.15", features = ["serde"] }`
  - `egui_code_editor = { version = "0.2", features = ["editor","egui"] }`
  - `egui-phosphor = { version = "0.12", default-features = false, features = ["regular"] }`
  - `wgpu = "29.0.0"` (also pinned independently by `animatix` at `29.0.0`)
- Token tree lives at `crates/animatix-gui/src/app/design_tokens/`:
  `mod.rs, motion.rs, primitive.rs, semantic.rs, spatial.rs, typography.rs, util.rs`.
- Widget tree lives at `crates/animatix-gui/src/app/components/`:
  `anim.rs, button.rs, context_menu.rs, diagnostics.rs, dialog.rs,
  easing_curve_editor.rs, layout.rs, row.rs, text.rs, timeline.rs, toast.rs` (mod.rs).
- **Grep-verified: zero `serde`/`Serialize`/`Deserialize`/`kurbo` usage in any
  file under `design_tokens/` or `components/`.** So `eparts` does **not** need
  egui's `serde` feature and does **not** need `kurbo`.
- **Grep-verified: only `egui_phosphor::regular::*` and
  `egui_phosphor::add_to_fonts` / `egui_phosphor::Variant::Regular` are used.**
  No `bold`/`fill`/`light`/`thin`/`duotone` variants → `regular` is the only
  phosphor feature required.

---

## A) Workspace dependency refactor — the single source of truth

### Design decision: full spec in `[workspace.dependencies]`, additive features per-crate
Cargo merges features **additively**: when a member crate writes
`egui = { workspace = true, features = ["serde"] }`, the final feature set is the
union of the workspace feature list and the per-crate list. `default-features`
is controlled by the **workspace** entry unless a member overrides it; an
override to `default-features = false` in a member is honored, but to avoid
confusion we set `default-features` once in the workspace where it matters.

Recommended split:

- Put **version + `default-features` + the common/base feature list** in
  `[workspace.dependencies]`.
- Keep **only the crate-specific extra features** in each member via
  `features = [...]` alongside `workspace = true`.
- The one feature that is app-only is `egui`'s `"serde"` (needed by
  `animatix-gui` for persisting UI state; **not** needed by `eparts`). Because
  features are additive and `eparts` never asks for `serde`, `eparts` builds
  without it while `animatix-gui` adds it locally. This is exactly the
  additive-feature model working in our favor.

> Caveat on additivity: if both crates are compiled in the same `cargo build`
> (they always are, same workspace), the resolved `egui` is built **once with
> the union of features**, i.e. `serde` will be ON in the shared compiled
> artifact. That is fine and intended — `eparts` simply doesn't reference serde
> APIs. The point of keeping `serde` out of the `eparts` manifest is so
> `eparts` is *correct and buildable standalone* (e.g. if later published or
> used in another workspace), not to strip serde from this build.

### Resulting root `Cargo.toml`
```toml
[workspace]
members = [
    "crates/animatix",
    "crates/animatix-analyzer",
    "crates/animatix-gui",
    "crates/animatix-lsp",
    "crates/animatix-syntax",
    "crates/eparts",
    "crates/tree-sitter-animatix"
]
resolver = "2"

[workspace.dependencies]
# egui stack — single source of truth, shared by eparts + animatix-gui
egui            = { version = "0.34" }
eframe          = { version = "0.34", default-features = false, features = ["default_fonts", "wgpu", "x11", "wayland"] }
egui-phosphor   = { version = "0.12", default-features = false, features = ["regular"] }
egui_extras     = { version = "0.34", features = ["syntect"] }
egui_tiles      = { version = "0.15", features = ["serde"] }
egui_code_editor = { version = "0.2", features = ["editor", "egui"] }

# Shared graphics dep already duplicated across animatix + animatix-gui
wgpu            = { version = "29.0.0" }
```

Notes:
- `egui` carries **no** features at the workspace level. `animatix-gui` adds
  `"serde"`; `eparts` adds none. This keeps `eparts` serde-free while letting the
  app opt in.
- `eframe` / `egui-phosphor` keep `default-features = false` + their feature
  lists in the workspace entry because **both potential consumers want the same
  base**. `egui-phosphor`'s only consumer split is the `regular` feature, which
  both want, so it lives in the workspace entry.
- `egui_extras`, `egui_tiles`, `egui_code_editor` are **animatix-gui-only**
  today (not used by `eparts`). They are placed in `[workspace.dependencies]`
  anyway so version control is centralized and future crates can reuse them;
  `eparts` simply won't reference them. (If you prefer strict minimalism, these
  three could stay pinned in `animatix-gui` — see scope note below. Recommended:
  centralize them too, since it's zero cost and completes the single-source-of-
  truth story for the whole egui stack.)

### Resulting `crates/eparts/Cargo.toml`
```toml
[package]
name = "eparts"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
description = "Reusable egui widgets and design tokens extracted from Animatix."

[dependencies]
egui          = { workspace = true }
egui-phosphor = { workspace = true }
```
`eparts` depends on **only** `egui` + `egui-phosphor`. No `serde`, no `kurbo`,
no `eframe` (widgets/tokens never touch eframe), no `egui_extras/tiles/code_editor`.

### Modified `crates/animatix-gui/Cargo.toml` (egui stack only; other deps unchanged)
```toml
[dependencies]
animatix          = { path = "../animatix" }
animatix-analyzer = { path = "../animatix-analyzer" }
animatix-syntax   = { path = "../animatix-syntax" }
eparts            = { path = "../eparts" }   # added in Phase 2+

eframe            = { workspace = true }
egui              = { workspace = true, features = ["serde"] }   # serde is the only per-crate extra
egui_code_editor  = { workspace = true }
egui_extras       = { workspace = true }
egui_tiles        = { workspace = true }
egui-phosphor     = { workspace = true }
wgpu              = { workspace = true }

# ...all remaining deps (kurbo, pollster, ron, serde, syntect, thiserror,
#    tracing, tracing-subscriber, notify, rfd, rodio, tree-sitter,
#    tree-sitter-animatix, tree-sitter-highlight, crossbeam-channel,
#    directories) stay exactly as they are.
```

### Why this is a single source of truth
- Every egui-stack version literal now lives in exactly one place: the root
  `[workspace.dependencies]` table. Member crates reference it with
  `workspace = true`.
- Bumping `egui` `0.34 → 0.35` is a one-line edit in root `Cargo.toml`; both
  `eparts` and `animatix-gui` recompile against the same version automatically.
- It is **structurally impossible** for `eparts` and `animatix-gui` to pin
  different egui versions — they don't carry version literals anymore. The
  lockstep risk (egui/eframe/phosphor must agree) is solved by construction.

### What still must be coordinated (honest scope of the guarantee)
- Versions can no longer silently diverge, but a **major egui bump still touches
  code in both crates** (API breakage). The table prevents *version* drift, not
  *source* churn. A bump is: edit one table line, then fix compile errors in
  both crates as flagged by `cargo check --workspace`.
- `eframe` ↔ `egui` ↔ `egui-phosphor` semver compatibility is still a human
  decision at bump time (egui-phosphor lags egui releases). The table makes the
  three versions visible side-by-side, which makes that decision easier, but
  doesn't automate it.

### Scope decision for *other* duplicated deps
- **Include now (recommended):** the full egui stack (above) **and `wgpu`**,
  because `wgpu = "29.0.0"` is already duplicated verbatim in both `animatix`
  and `animatix-gui`. Centralizing it removes an existing real drift hazard at
  zero behavioral cost. Convert `animatix/Cargo.toml` and
  `animatix-gui/Cargo.toml` `wgpu` entries to `workspace = true` in the same
  Phase 1 commit.
- **Defer (optional, out of scope):** `serde`, `tracing`, `thiserror`,
  `tree-sitter*`. These are widely shared but **not required for the eparts
  extraction** and migrating them is unrelated churn. Recommendation: leave them
  decentralized for this task; they can be centralized later in a dedicated
  housekeeping pass. Keeping scope tight reduces review surface and keeps each
  phase verifiable.

---

## B) Crate structure for `crates/eparts`

```
crates/eparts/
├── Cargo.toml                 # egui + egui-phosphor (workspace = true)
└── src/
    ├── lib.rs                 # pub mod tokens; pub mod widget; curated re-exports
    ├── tokens/
    │   ├── mod.rs             # re-exports; generic semantic + spatial constants
    │   ├── primitive.rs       # raw color/scale primitives (moved verbatim)
    │   ├── semantic.rs        # GENERIC roles only: surface, text, accent,
    │   │                      #   status, border, overlay  (+ generic grid/guide
    │   │                      #   line alpha colors promoted from canvas::)
    │   ├── spatial.rs         # GENERIC only: SPACE_*, ROW_*, RADIUS_*,
    │   │                      #   STROKE_*, component spacing
    │   ├── typography.rs      # moved verbatim
    │   ├── motion.rs          # moved verbatim (hand-rolled CubicBezier; no kurbo)
    │   └── util.rs            # moved verbatim (color/lerp helpers)
    └── widget/
        ├── mod.rs             # re-exports each widget module
        ├── anim.rs            # animated value helpers
        ├── button.rs
        ├── context_menu.rs
        ├── dialog.rs
        ├── layout.rs
        ├── row.rs
        ├── text.rs
        ├── toast.rs
        ├── timeline.rs        # generic; uses promoted grid/guide line tokens
        ├── easing_curve_editor.rs   # generic; uses promoted line tokens
        └── diagnostics.rs     # generic via `DiagnosticEntry` trait (defined here)
```

`lib.rs` skeleton:
```rust
pub mod tokens;
pub mod widget;

// Trait that decouples the diagnostics widget from animatix_syntax::Diagnostic.
pub use widget::diagnostics::DiagnosticEntry;
```

What **stays** in `animatix-gui` (layered on top of `eparts`):
- App-specific **semantic** submodules: `canvas`, `timeline`, `diagnostic`,
  `curve`, `editor`, `category` — re-homed in
  `animatix-gui/src/app/design_tokens/semantic.rs`, now referencing
  `eparts::tokens::semantic` generic roles for their base colors.
- App-specific **spatial** submodules: `preview`, `timeline`, `inspector`,
  `menu`, `toolbar`, `welcome`, `dialog` — stay in
  `animatix-gui/src/app/design_tokens/spatial.rs`.
- `completion_popup.rs` — **not extracted** (domain-bound, hardcoded colors,
  visually inconsistent with the token system).
- The `impl eparts::DiagnosticEntry for animatix_syntax::Diagnostic` —
  stays in `animatix-gui` (e.g. in `components/diagnostics.rs` shim or a small
  adapter module).

### `DiagnosticEntry` trait (defined in `eparts::widget::diagnostics`)
```rust
pub trait DiagnosticEntry {
    fn is_error(&self) -> bool;
    fn message(&self) -> &str;
    fn line(&self) -> usize;
    fn column(&self) -> usize;
    fn phase_label(&self) -> Option<&str> { None }      // optional
    fn phase_color(&self) -> Option<egui::Color32> { None }  // optional
}
```
The widget renders against `&[impl DiagnosticEntry]` (or `&dyn`). The concrete
`animatix_syntax::Diagnostic` impl lives in `animatix-gui`.

### Re-export shims (so call sites don't change)
In `animatix-gui/src/app/components/mod.rs` and
`animatix-gui/src/app/design_tokens/mod.rs`, add `pub use eparts::...` shims so
existing paths (`crate::app::components::button::...`,
`crate::app::design_tokens::semantic::surface::...`) keep resolving. Behavior
stays byte-identical because the code is moved verbatim, only its home changes.

---

## C) Migration phases (each ends green on
`cargo check --workspace` **and** `cargo test -p animatix-gui`)

### Phase 1 — Workspace dependency refactor FIRST (prove single-source-of-truth before any code moves)
1. Add `[workspace.dependencies]` table to root `Cargo.toml` with the full egui
   stack + `wgpu` (exact table in section A).
2. Convert `animatix-gui/Cargo.toml`: change egui stack + `wgpu` entries to
   `workspace = true`, keeping `features = ["serde"]` on `egui` only.
3. Convert `animatix/Cargo.toml`: change `wgpu = "29.0.0"` to
   `wgpu = { workspace = true }`.
4. **Do not create `eparts` yet** — this phase only flips existing crates onto
   workspace deps so we verify the build is byte-identical first.
   - Verify: `cargo check --workspace` clean; `cargo test -p animatix-gui` green;
     `cargo test --no-fail-fast` green. (No code changed, only manifests.)

> Rationale for ordering: doing the manifest refactor with no code movement
> isolates "did I break versioning?" from "did I break the extraction?". If the
> build is identical after Phase 1, the workspace-deps mechanism is proven.

### Phase 2 — Birth `eparts` consuming workspace deps; move generic tokens
1. Create `crates/eparts/` with `Cargo.toml` (egui + egui-phosphor,
   `workspace = true`), add `crates/eparts` to root `members`.
2. Add `eparts = { path = "../eparts" }` to `animatix-gui`.
3. Move `primitive.rs`, `typography.rs`, `motion.rs`, `util.rs` verbatim into
   `eparts/src/tokens/`. Split `semantic.rs`/`spatial.rs`: generic roles →
   `eparts`, app submodules stay.
4. Promote `canvas::grid_line` / `canvas::guide_line` neutral white-alpha colors
   to generic `eparts::tokens::semantic` entries.
5. Add `pub use eparts::tokens::...` shims in `design_tokens/mod.rs`,
   `semantic.rs`, `spatial.rs` so existing paths resolve unchanged.
   - Verify: `cargo check --workspace`; `cargo test -p animatix-gui`.

### Phase 3 — Move the 8 fully-generic widgets
1. Move `button, row, layout, dialog, context_menu, toast, anim, text` into
   `eparts/src/widget/`. Repoint their token imports to `eparts::tokens`.
2. Add `pub use eparts::widget::...` shims in `components/mod.rs`.
   - Verify: `cargo check --workspace`; `cargo test -p animatix-gui`.

### Phase 4 — Move `timeline.rs` + `easing_curve_editor.rs`
1. Repoint their line colors to the promoted generic grid/guide tokens.
2. Move into `eparts/src/widget/`, add shims.
   - Verify: `cargo check --workspace`; `cargo test -p animatix-gui`.

### Phase 5 — Move `diagnostics.rs` behind `DiagnosticEntry`
1. Define `DiagnosticEntry` trait in `eparts::widget::diagnostics`; make the
   widget generic over it. Move widget into `eparts`.
2. Add `impl DiagnosticEntry for animatix_syntax::Diagnostic` in `animatix-gui`.
3. Add shim for the widget path.
   - Verify: `cargo check --workspace`; `cargo test -p animatix-gui`.

### Phase 6 — Cleanup pass (optional, same task)
1. Remove now-dead `#[allow(dead_code)]` and stale comments introduced by moves.
2. Decide per-call-site whether to keep shims or update imports to
   `eparts::...` directly (shims may be retained indefinitely; they're cheap).
3. `completion_popup.rs` left untouched (intentionally not extracted).
   - Verify: full `AGENTS.md` pre-commit suite:
     `cargo check --workspace`, `cargo test -p animatix-syntax`,
     `cargo test -p animatix --lib`, `cargo test --no-fail-fast`.

---

## D) Risks & tradeoffs (workspace-deps approach)

1. **Additive-features gotcha.** Member `features` are unioned with workspace
   `features`; they can only *add*. You cannot *remove* a workspace feature in a
   member. Mitigation: keep the workspace base feature set minimal/common; put
   only opt-ins (egui `serde`) per-crate. We verified `eparts` needs no
   subtractions.
2. **`default-features` handling.** `default-features = false` for `eframe` and
   `egui-phosphor` is set in the **workspace** entry. A member overriding it
   back to true is possible but we don't; keep the toggle in one place. Confirm
   no member silently re-enables defaults.
3. **Shared-artifact feature unification.** In a single workspace build, `egui`
   is compiled once with the **union** of features, so `serde` is present in the
   compiled `egui` even for `eparts`'s use. This is benign (eparts never calls
   serde APIs) but means "eparts is serde-free" is a *manifest/standalone*
   property, not a *this-build* property. Documented, not a bug.
4. **One-time churn converting `animatix-gui`'s egui stack to `workspace = true`.**
   ~7 manifest lines + the `wgpu` line in `animatix`. Low risk because Phase 1
   changes no source code; any regression is a pure dependency-resolution issue
   caught immediately by `cargo check --workspace`.
5. **Major egui bump still requires source changes in both crates.** The table
   prevents version divergence but not API churn; a bump is "edit one line, fix
   both crates." Set expectations accordingly.
6. **`egui_extras/tiles/code_editor` centralized but unused by `eparts`.** Slight
   over-centralization; harmless. If a reviewer prefers strict minimalism, these
   three can stay pinned in `animatix-gui`. Recommended to centralize for a
   complete egui-stack single-source story.
7. **Ordering risk in code moves (Phases 2–5).** Moving tokens before widgets is
   mandatory (widgets import tokens). Shims must be added in the same phase as
   each move or `cargo check` breaks. Each phase is independently verifiable, so
   a broken phase is localized.
8. **Token split correctness.** Mis-classifying an app-specific color as generic
   (or vice-versa) would either bloat `eparts` or break a `pub use`. Mitigation:
   the generic role list is fixed (surface/text/accent/status/border/overlay +
   grid/guide line); everything else stays. Verify by compiling, not by judgment.
9. **`diagnostics.rs` trait surface.** If the real `Diagnostic` exposes data the
   trait omits (e.g. spans, codes), the widget may lose information. Verify the
   trait covers every field the current widget reads before moving.

### What stays per-crate (not centralized)
- `egui` `serde` feature (animatix-gui only).
- All non-egui deps except `wgpu`: `serde`, `tracing`, `thiserror`,
  `tree-sitter*`, `kurbo`, `pollster`, `ron`, `syntect`, `notify`, `rfd`,
  `rodio`, `directories`, `crossbeam-channel`, `tracing-subscriber`.

---

## E) Assumptions to verify before executing
1. **eparts needs no egui `serde`.** Verified via grep: zero
   `serde/Serialize/Deserialize` in `design_tokens/` and `components/`. Re-grep
   after the semantic/spatial split lands to ensure no serde sneaks in with the
   generic roles.
2. **`egui-phosphor` `regular` is the only variant used.** Verified: only
   `egui_phosphor::regular::*`, `add_to_fonts`, `Variant::Regular`. No
   `bold/fill/light/thin/duotone`. Confirm none of the *moved* widgets reference
   another variant (they don't, but re-grep the moved set).
3. **No `kurbo` in moved files.** Verified: zero `kurbo` matches in
   `components/`; `motion.rs` CubicBezier is hand-rolled.
4. **Token usage is reachable through shims.** Grep workspace-wide for
   `design_tokens::` and `components::` usages to enumerate every call site that
   the `pub use` shims must keep resolving:
   - `grep -rn "design_tokens::" crates/`
   - `grep -rn "components::" crates/`
   Confirm all resolve to either a moved-and-shimmed path or an app-specific path
   that stays.
5. **`DiagnosticEntry` trait completeness.** Read the current `diagnostics.rs`
   widget body and `animatix_syntax::Diagnostic` to confirm the six trait methods
   (is_error/message/line/column + optional phase_label/phase_color) cover every
   field the widget reads. Add methods if it reads more.
6. **`wgpu` versions are truly identical** (`29.0.0` in both `animatix` and
   `animatix-gui`) before collapsing into one workspace entry. Verified from
   manifests; confirm no patch-level pin elsewhere (`grep -rn "wgpu" crates/`).
7. **edition/rust-version for `eparts`** match the workspace (`edition = "2024"`,
   `rust-version = "1.85"`), consistent with `animatix-gui`.

---

## Files to touch
- `Cargo.toml` (root) — add `[workspace.dependencies]`; add `crates/eparts` member.
- `crates/animatix-gui/Cargo.toml` — egui stack + `wgpu` → `workspace = true`; add `eparts` path dep.
- `crates/animatix/Cargo.toml` — `wgpu` → `workspace = true`.
- `crates/eparts/Cargo.toml` — new; egui + egui-phosphor.
- `crates/eparts/src/lib.rs` — new.
- `crates/eparts/src/tokens/{mod,primitive,semantic,spatial,typography,motion,util}.rs` — moved/split.
- `crates/eparts/src/widget/{mod,anim,button,context_menu,dialog,layout,row,text,toast,timeline,easing_curve_editor,diagnostics}.rs` — moved.
- `crates/animatix-gui/src/app/design_tokens/{mod,semantic,spatial}.rs` — keep app submodules; add `pub use eparts::...` shims.
- `crates/animatix-gui/src/app/components/mod.rs` — add `pub use eparts::widget::...` shims; keep `completion_popup.rs`; add `DiagnosticEntry` impl/adapter.

## Definition of done
- One `[workspace.dependencies]` table is the only place egui-stack (+ wgpu)
  versions appear. No member carries an egui-stack version literal.
- `eparts` compiles standalone depending only on `egui` + `egui-phosphor`.
- All call sites resolve via shims; behavior byte-identical.
- Full `AGENTS.md` pre-commit suite green:
  `cargo check --workspace`, `cargo test -p animatix-syntax`,
  `cargo test -p animatix --lib`, `cargo test --no-fail-fast`.
