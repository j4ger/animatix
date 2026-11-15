# GUI design language

Design guidance for the native workbench and its `eparts` component library.
For user-facing behavior and limitations, read [GUI workbench](gui-workbench.md).
The implementation defines exact token values and APIs; this document defines
how to use them without duplicating the source tables.

The former visual-editor design is superseded: canvas transforms, property
editing, auto-key, object insertion and draggable keyframes are not active
workbench features. Their presence in an enum or an old design does not grant a
writing path. Future timing edits must follow the source-patch contract below.

## Principles

- **Code authors the scene.** The editor changes `.amx`; other panels help the
  author navigate, inspect and review it. Do not reconstruct source from ECS state.
- **Preview and time share priority.** Keep the canvas prominent and timing
  visible. Panel chrome should not compete with rendered content.
- **Show state.** Distinguish the current preview, a rebuild, a failed build and
  a stale last-good preview. Disabled actions need an understandable reason.
- **Reveal detail on demand.** Authored `#` markers are primary; property tracks
  and diagnostic values are secondary views, not a replacement timeline model.
- **Keep document and workspace actions separate.** Seeking, selecting,
  zooming, changing panes and setting review loops do not modify the document.

## Tokens and components

The shared library is [`crates/eparts`](../crates/eparts/src/lib.rs).
GUI-specific canvas and timeline tokens belong in the app, not the shared library.

| Concern | Source |
|---|---|
| Raw palette | [`tokens/primitive.rs`](../crates/eparts/src/tokens/primitive.rs) |
| Semantic roles | [`tokens/semantic.rs`](../crates/eparts/src/tokens/semantic.rs) |
| Runtime themes and component slots | [`tokens/theme/`](../crates/eparts/src/tokens/theme/) |
| Spacing, density, radii and dimensions | [`tokens/spatial.rs`](../crates/eparts/src/tokens/spatial.rs) |
| Font roles and fallback aliases | [`tokens/typography.rs`](../crates/eparts/src/tokens/typography.rs) |
| Animation durations and easing | [`tokens/motion.rs`](../crates/eparts/src/tokens/motion.rs) |
| Widgets | [`widget/`](../crates/eparts/src/widget/) |

Read colors through `eparts::theme(ui)` and its semantic/component slots. Do not
copy palette literals into widgets or assume a dark background. Each semantic
role must work in both light and dark themes; status colors and actor-category
colors have different meanings and should not be interchanged.

Use `TextRole` rather than constructing font IDs or setting arbitrary sizes.
The current roles include display, heading, subheading, title, body, medium body,
small body, caption, mono, tabular mono and micro. Timecodes and numeric readouts
use mono roles. Preserve font fallback when adding custom font families.
GUI font setup is centralized in
[`fonts.rs`](../crates/animatix-gui/src/fonts.rs): it installs the bundled UI faces
and discovers system fallbacks for CJK and other scripts, sharing cached discovery
across the app and screenshot harness. Egui defaults alone do not cover CJK.
Keep fallbacks on both proportional/monospace and named families.

Use shared spacing and density helpers rather than independent pixel scales.
Compose standard controls from `eparts`; custom painting is appropriate for
scene previews and specialized timeline/curve visualizations, not a replacement
for ordinary buttons or fields.

### Widget API contract

A widget has one primary entry point:

- `impl egui::Widget` for plain inputs and an `egui::Response` result, used with
  `ui.add(...)`. Examples: `Button`, `Label`, `Spinner` and `Slider`.
- `show(...)` for content closures, richer response structs or cross-frame
  coordination. Examples: `Form`, `Field`, `Dialog`, `Popover` and `Row`.

Name rich results `<Widget>Response`. Stateless layout functions such as `card`
and `section_header` are deliberate exceptions. `Row::show_in_rect` is the
supported pre-allocated variant of `Row::show`, not a competing widget API.

Widgets must handle normal, hover, pressed/active, disabled and focused states
where relevant. Read state colors from theme slots. Reuse the common focus-ring
and response handling rather than drawing a different focus treatment per panel.
Buttons and rows use the default cursor; pointing hands are reserved for links.

### Icons and text

Use the existing Phosphor icon family. Actions may be icon-only if their tooltip
and accessibility name describe the action in words. Include shortcuts in the
tooltip. Keep related groups separate: playback, marker navigation, review loops
and panel controls should not become one undifferentiated row.

Keep filenames, timecodes, track names, headings and preview validity as text.
Selected view modes need visible active states. Status must not rely on color or
an unlabeled icon alone.

## Workbench surfaces

| Surface | Responsibility |
|---|---|
| Source editor | Edit, save, undo/redo and navigate `.amx` source |
| Preview | Playback, selection and view navigation; no object transforms |
| Timeline | Scene ranges, authored markers, audio/transcript review and loops |
| Inspector | Read sampled values and locate eligible declarations |
| Outline, assets and library | Browse and inspect; no implicit insertion or reordering |
| Curves | Inspect derived tracks; no property or keyframe writes |

The visible presets are **Write**, **Review**, **Inspect** and **Focus**.
Inspector and Code share the detail region; Timeline and Curves share the bottom
region. Focus hides surrounding panels. The docking implementation uses
`egui_tiles`; presets adjust the existing layout rather than discard it.

Region allocation uses `clamp(ratio × available, min, max)`, with a tile-size
floor so panels cannot collapse into slivers. User-resized dimensions should
survive within those bounds; Reset layout restores defaults. Compact windows use
overlay drawers; widening must restore the user's prior visibility choices. Test this
round-trip with both an open detail panel and Focus mode. Preset labels live in
[`layout_preset.rs`](../crates/animatix-gui/src/app/layout_preset.rs); sizing,
persistence and migration live in
[`persistence/`](../crates/animatix-gui/src/app/persistence/).

## Interaction boundaries

[`Command::allowed_in_workbench`](../crates/animatix-gui/src/app/commands/api.rs)
is the central write boundary. Controls and shortcuts must respect it, including
stale selections. Removing a button alone is not sufficient. Source edits remain
undoable; workspace changes must not masquerade as source edits.

Use the application's shortcut registry for hints and the shortcut sheet rather
than maintaining another key table here. Text-input focus and open overlays must
consume applicable keys before global playback or navigation shortcuts.

Overlay coordination is in
[`widget/overlay.rs`](../crates/eparts/src/widget/overlay.rs). Dialogs and popovers
use the foreground plane; tooltips use the tooltip plane. Only the topmost
eligible overlay handles Escape or outside-click dismissal. Remove closed
overlays from the coordinator so the next one can respond.

### Future timing editing

Timeline dragging is not implemented. A future implementation may move or reorder
explicit source timing only through a narrow patch:

1. Resolve a direct, unambiguous source span and check its revision.
2. Show which timing text will change; reject generated or unresolved targets.
3. Apply the edit through source undo/redo and rebuild.
4. Preserve the last-good preview on failure, visibly marked stale.

Do not enable arbitrary inspector editing, mutate runtime tracks as a substitute,
introduce a separate timing truth, or reserialize the entire AST for a small edit.

## Motion

Use the shared [`anim` helpers](../crates/eparts/src/widget/anim.rs), not scattered
`animate_value_with_time` calls. Use tokenized durations/easing: ordinary panel
transitions use `NORMAL`/`STANDARD`; durations longer than `SLOW` need an explicit
reason. Selection and playhead scrubbing update immediately, with no animation lag.

Respect `MotionPreference::Reduced`: non-essential transitions resolve to
`INSTANT`. The existing helpers apply this preference; do not bypass it with a
custom animation. Verify the Settings path as well as the helper behavior rather
than assuming OS preference detection is available on every platform.

## Accessibility and validation

These are acceptance goals, not a claim of full accessibility certification:

- Non-disabled body text meets WCAG AA contrast (4.5:1); large text and active
  UI components meet 3:1. Check both themes.
- Focus is visible, icon actions have readable names, and essential operations
  have keyboard paths. Target a minimum 24×24 logical-pixel hit area in both
  dimensions; a smaller visual glyph or compact row must not silently shrink its
  usable hit target. This is an acceptance goal, not a claim that all existing
  compact controls already meet it.
- Motion is brief and purposeful. Never delay input behind a decorative animation.
- Test disabled actions, repeated clicks, Escape, focus changes, and window-size
  round-trips, not just the default screenshot.

For component screenshots, use `scripts/gui-screenshots.sh` in the development
shell (with a display or `xvfb-run`). It writes to `target/gui-screenshots/` and
bounds each capture. Component screenshots do not replace workbench interaction
tests. See [contributing](contributing.md) for the test gates and
[workbench validation limits](gui-workbench.md#validation-and-remaining-platform-checks)
for the last recorded platform coverage. Remaining work belongs in
[the roadmap](roadmap.md), not a second feature-status table here.
