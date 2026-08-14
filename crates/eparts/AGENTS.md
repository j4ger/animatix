# Agent Guide for eparts

`eparts` is a reusable egui widget + design-token library extracted from Animatix. It targets
immediate-mode egui (pinned to one egui minor — today **0.34**). It is being grown into a general
framework reusable by future apps, so build framework-clean, domain-free APIs.

See the canonical work list at `docs/roadmap.md` for the eparts widget-adoption backlog.

## Module map

- `src/tokens/` — design tokens (the domain-free design system):
  - `primitive.rs` — raw palette (`pub`, referenced by app-specific token submodules in the GUI).
  - `semantic.rs` — semantic color roles (`surface`, `text`, `accent`, `status`, `border`, `lines`, `overlay`).
  - `spatial.rs` — spacing scale, row heights, radii, strokes, component/menu/dialog dims.
  - `typography.rs` — `TextRole` (8-level type scale).
  - `motion.rs` — `CubicBezier`, `Transition`, named durations/curves.
  - `theme.rs` — runtime `Theme` struct + `eparts::theme(ui)` / `eparts::set_theme(ctx, ..)` accessors.
  - `util.rs` — `lerp_color`, `multiply_alpha`.
- `src/widget/` — domain-agnostic widgets (button, row, layout, dialog, context_menu, toast, anim,
  text, timeline, easing_curve_editor, diagnostics). Plus shared `traits.rs`.

## Core principles

1. **Theme-driven, never hardcode colors.** Every color comes from a `tokens::semantic` role or the
   `Theme` struct; never `Color32::from_rgb(...)` inline in a widget. Whether a role resolves at compile
   time (const) or runtime (`Theme`) is an implementation detail behind the role name. Raw color literals
   (`Color32::from_rgb`, `from_gray`, etc.) are allowed only inside the token definition layer
   (`crates/eparts/src/tokens/primitive.rs`, `crates/eparts/src/tokens/theme.rs` — e.g. `Theme::light()`/`dark()`
   and `*_slots()` builders). Widgets must never contain color literals; they read semantic roles or
   `theme(ui)` slots.
2. **Builder pattern mandatory + builder tests.** Every widget is `Widget::new(...)` + chained `.with_x()`
   setters returning `Self`, and every builder field gets a unit test asserting it is set.
3. **Cursor convention: default arrow for buttons, pointer only for links.** Buttons/rows keep the default
   cursor; reserve `CursorIcon::PointingHand` for genuine hyperlinks. (Inverse of egui's default — gives a
   native-desktop feel rather than a web feel.)
4. **Shared trait contracts over per-widget enums.** Use `Sizable`/`Selectable`/`Disableable`/`Collapsible`
   from `widget/traits.rs` instead of duplicating per-widget size/state enums.
5. **Stateless render; cross-frame state in egui Memory or app structs.** Any open/closed, animation
   progress, focus, or hover-grace state lives in `ctx.data` (`Id`-keyed) or an app-owned struct, never in
   a retained widget instance.
6. **4-tier size vocabulary, wire what you use.** Keep the `xs/sm/md/lg` enum on `Sizable`, but do not
   pre-build unused variants behind `#[allow(dead_code)]`. Wire a size when a call site needs it.

## Framework principles (reuse goal)

7. **API stability / semver thinking.** While `0.x`, APIs may break freely. Mark intended-stable APIs;
   keep experimental ones behind `#[doc(hidden)]` or an `unstable` feature. Changelog discipline begins at
   the first external consumer.
8. **Feature-flag optional heavy widgets.** Consumers pay only for what they use. Use Cargo features for
   heavy/optional surfaces (`table`, `charts`, `webview`, `theme-json`, `i18n`). Core tokens + traits +
   common widgets stay default-on and dependency-light (today: only `egui` + `egui-phosphor`). The Linux
   system-theme detector (`dark-light`) is gated by `cfg(target_os = "linux")` and/or a feature.
9. **Documentation + example expectations.** Every public widget gets a doc comment with a minimal usage
   snippet (and, eventually, a gallery entry). "Undocumented widget" = not done.
10. **Accessibility baseline.** Don't regress keyboard navigability or labels. Full a11y is future work;
    the baseline (don't break focus/labels) applies now.
11. **egui-version coupling is explicit.** Pin `egui` in `Cargo.toml`, document the supported version, and
    treat an egui bump as a deliberate, tested migration — not an incidental `cargo update`.

Do **not** introduce retained-mode constructs: no retained view tree, no `RenderOnce` trait, no mandatory
root view. eparts is immediate-mode.

## State-management contract (immediate mode)

Cross-frame widget state has exactly two homes:

- **`egui::Memory` via `ctx.data_mut`**, keyed by an `egui::Id` derived from the widget's id-source. Use
  for transient per-widget UI state (open/closed, animation progress, hover-grace timers, focus tracking).
- **App-owned structs** (e.g. the GUI's `ui_store`) for state the application owns and persists.

Never store state in a retained widget instance — widgets are constructed and consumed each frame.

`ToastQueue` (`crates/eparts/src/widget/toast.rs`) is an app-owned state manager, not a per-frame
widget instance. It is the sanctioned exception to "never store cross-frame state in a widget
struct," because the application owns and persists it (like `ui_store`). Genuinely transient per-widget
state still must live in egui Memory.

The `Theme` is stored in Memory and read at the top of a widget's `ui()` via `let t = eparts::theme(ui);`.

## Widget API contract (entry-point convention)

eparts widgets follow a deliberate two-tier convention. Pick the tier that matches the widget:

**Tier 1 — `impl egui::Widget`** (invoked with `ui.add(MyWidget::new(...))`). Use this for
self-contained widgets that take only plain values/builder options and return an `egui::Response`.
No content closures, no rich return struct. Examples: `Button`, `Label`, `Spinner`, `Slider`,
`Select`, `Badge`, `Tag`, `Alert`, `ProgressBar`, `Skeleton`, `Kbd`.

**Tier 2 — `pub fn show(self, ui, ...) -> T`** (invoked as `MyWidget::new(...).show(ui, ...)`). Use
this when the widget needs any of: a content/render closure (`FnOnce(&mut Ui)`), a rich return value
(a `*Response`/action struct beyond `egui::Response`), or cross-frame state coordination. Examples:
`Form`/`Field`, `Dialog::modal`, `Popover`, `Tooltip`, `Collapsible`, `Tree`, `List`, `ColorPicker`,
`TextField`/`NumberField`, `Row`, `TabBar`, `ResizeHandle`, `Toast`.

Rules:
- A widget exposes exactly **one** primary entry point — either `impl Widget` OR `show()`, never both.
  (Builder setters like `with_size`, `show_value`, `show_percentage` are fine; they are not entry points.)
- Tier-2 `show()` returns either `egui::Response` or a documented `*Response` struct; name rich structs
  `<Widget>Response`.
- Free functions in `layout.rs` (`card`, `section_header`, `separator`, …) are a deliberate exception:
  they are stateless layout helpers, not widgets, and stay as `fn(ui, …)`.
- When unsure, prefer Tier 1; promote to Tier 2 only when a closure / rich return / state is required.

**Tier-2 rect-mode exception.** Container-driven widgets (e.g. `Row`) may expose a second entry
`show_in_rect(rect, response, painter, …)` alongside `show()`. This is the SAME logical entry, not a
competing API: `show()` allocates the widget rect then delegates to `show_in_rect`; container widgets
(`Tree`, `List`) that pre-allocate the rect call `show_in_rect` directly.

## Verification (per workspace AGENTS.md)

Before committing:

```bash
cargo check --workspace        # all crates compile (catches GUI/analyzer/LSP drift)
cargo test -p animatix-syntax  # parser tests
cargo test -p animatix --lib   # core lib tests
cargo test -p animatix-gui     # GUI (no FFmpeg needed)
cargo test --no-fail-fast      # full sweep
```

- Every `#[allow(dead_code)]` needs an inline justification comment.
- Use `tracing`, not `println!`.
- Match the existing token/widget conventions; don't introduce new color literals.
