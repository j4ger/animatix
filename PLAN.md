# GUI Design Language Migration Plan

## Goal
Migrate `crates/animatix-gui` from flat/ad-hoc egui styling to the design language in `docs/gui_design_language.md` while preserving source-first editing, preview behavior, and existing tests.

## Current State
- `docs/roadmap.md` already defines the migration as four phases: token refoundation, component unification, command split, and interaction upgrade; `docs/gui_design_language.md` is the authoritative spec.
- There are no code references to "design language"; references are documentation-only: `docs/gui_design_language.md`, `docs/roadmap.md`, `docs/README.md`, and `Readme.md`.
- `crates/animatix-gui/Cargo.toml` uses `eframe = 0.34`, `egui = 0.34`, `egui_extras = 0.34`, `egui_tiles = 0.15`, `egui-phosphor = 0.12`, `egui_code_editor = 0.2`, and `wgpu = 29.0.0`; this is not primarily an egui-version migration.
- The current theme is a flat `crates/animatix-gui/src/app/design_tokens.rs` imported by 30+ files; it mixes raw palette colors, semantic-ish names, spacing, padding duplicates, typography sizes, timeline constants, inspector constants, preview constants, and color utility functions.
- `runtime::install_theme` in `crates/animatix-gui/src/app/runtime.rs` directly maps flat tokens into `egui::Style` and uses `PAD_*`; the GUI is dark-only today.
- Styling is partially centralized but still inline in `app/mod.rs`, `app/shell/*`, `app/panels/*`, `app/preview/*`, `cell_editor/render.rs`, `completion_popup.rs`, and `highlighting.rs`.
- Primitive UI components exist but are function-based: `components/button.rs` has `icon_button`, `icon_button_colored`, `toolbar_toggle_button`, `toolbar_action_button`; `components/layout.rs` has `card`, `section_header`, `field_sized`, `labeled_row`, and `pill_tab_bar`; `components/context_menu.rs` has a reusable context menu.
- Typography uses `FONT_SIZE_*`, raw `FontId::new`, `FontId::monospace`, `FontId::proportional`, and `.size(FONT_SIZE_*)`; `section_header` and context menu headers use `to_uppercase()`.
- Command architecture is partly split: `commands.rs` still contains a large mixed `Command` enum, while `handlers/{actor,file,keyframe,playback,property,scene,ui}.rs` already contain domain handler functions and `shell/mod.rs` owns the large dispatcher.
- Interaction architecture is still centralized around `preview/drag_handler.rs` and `runtime::handle_keyboard_shortcuts`; `CommandBus` exists as future infrastructure but most panels still emit directly into `ActionQueue`.
- Motion is scattered but small: `animate_value_with_time` appears in `cell_editor/render.rs`, `shell/toolbar.rs`, and `panels/sidebar.rs`.

## Target State
- `app/design_tokens/` contains layered modules: `primitive` (`pub(crate)` raw values), `semantic` (public role tokens), `typography` (`TextRole`), `spatial` (`SPACE_*`, row heights, radii), `motion` (durations/easing helpers), and `util` (temporary compatibility helpers).
- UI code imports semantic roles instead of flat constants, with canvas-specific colors only from `semantic::canvas`; `PAD_*` and legacy flat color names are gone after migration.
- `runtime::install_theme` consumes semantic/spatial tokens and remains the single egui global style installer.
- Buttons and common controls are primitive widgets implementing `egui::Widget`; panels compose widgets and patterns instead of painting standard controls ad hoc.
- `TextRole` replaces raw font construction for UI chrome; syntax highlighting and code-editor token colors are either isolated as syntax/editor tokens or explicitly documented exceptions.
- Commands are split into domain modules and separate undoable document mutations from non-undoable view/playback actions.
- Canvas interactions route through typed gestures that emit commands; keyboard navigation and shortcut handling share one registry/focus guard.
- Motion uses a shared helper instead of direct `animate_value_with_time` calls.

## Dependencies
- Phase 1 must land before Phase 2 because `Button`, `TextRole`, and panel cleanup depend on semantic tokens.
- Phase 2 should land before Phase 3/4 in visible UI areas so later command/gesture work does not churn obsolete button and typography call sites.
- Phase 3 should land before Phase 4 because gesture handlers should emit the new domain command types instead of the flat `Command` enum.
- `completion_popup.rs`, `highlighting.rs`, and `cell_editor/render.rs` should be migrated after core chrome tokens are stable because they contain syntax/editor-specific colors that may need separate semantic namespaces.

## Plan
1. Create token modules behind a compatibility facade: add `crates/animatix-gui/src/app/design_tokens/{primitive.rs,semantic.rs}` and update `crates/animatix-gui/src/app/design_tokens.rs` to re-export old names from semantic tokens; outcome is no call-site churn yet; verify with `cargo check -p animatix-gui`.
2. Add spatial, typography, motion, and util tokens: add `design_tokens/{spatial.rs,typography.rs,motion.rs,util.rs}` and move `SPACE_*`, `ROW_*`, `RADIUS_*`, `lerp_color`, and `multiply_alpha`; outcome is a complete token API with legacy aliases still available; verify with `cargo check -p animatix-gui` and `rg "PAD_|FONT_SIZE_|BG_|ACCENT_BLUE" crates/animatix-gui/src/app/design_tokens.rs`.
3. Migrate global egui styling: update `runtime::install_theme` and `AnimatixApp::clear_color` in `crates/animatix-gui/src/app/runtime.rs` to use `semantic::{surface,text,accent,border}` and `spatial`; outcome is the new surface depth and WCAG-fixed text colors applied globally; verify with `cargo check -p animatix-gui` and a visual smoke test of the shell background, widgets, selection, and focus states.
4. Migrate core reusable components: update `components/{layout.rs,row.rs,timeline.rs,diagnostics.rs,easing_curve_editor.rs,toast.rs,context_menu.rs}` to semantic/spatial/typography tokens, remove `to_uppercase()` from `layout::section_header` and `context_menu::render_menu_header`; outcome is consistent cards, rows, tabs, menus, toasts, timeline dots, and diagnostics; verify with `cargo check -p animatix-gui` and `rg "to_uppercase\(" crates/animatix-gui/src/app/components`.
5. Migrate shell chrome and modals: update `app/mod.rs` plus `shell/{toolbar.rs,settings.rs,export_dialog.rs,insertion_palette.rs,command_palette.rs,find_replace.rs,shortcut_cheat_sheet.rs}` to semantic/spatial/typography tokens; outcome is toolbar, status bar, welcome screen, dialogs, palettes, and settings using the same roles; verify with `cargo check -p animatix-gui` and manually open welcome, export, settings, command palette, insertion palette, find/replace, and shortcuts.
6. Migrate panels and canvas visuals: update `panels/{mod.rs,behavior.rs,preview_panel.rs,timeline_panel.rs,sidebar.rs}` and `panels/inspector/{mod.rs,property_groups.rs,keyframe_table.rs,graph_editor.rs,spreadsheet.rs}` plus `preview/{mod.rs,context.rs,grid.rs,overlay.rs,property_popup.rs,selection.rs,time_lens.rs}` to semantic tokens, keeping `semantic::canvas` for preview-only overlays; outcome is consistent sidebar, inspector, preview, overlays, and timeline category colors; verify with `cargo check -p animatix-gui` and smoke test actor selection, drag handles, rulers, guides, property popup, inspector tabs, and timeline lanes.
7. Migrate editor-specific UI tokens: update `cell_editor/render.rs`, `completion_popup.rs`, and `highlighting.rs` to use `semantic::editor`/`semantic::syntax` tokens or documented local exceptions for language highlighting; outcome is editor and completion styling isolated from chrome tokens; verify with `cargo test -p animatix-gui highlighting` and manual completion-popup/cell-editor smoke tests.
8. Remove legacy token aliases and finalize module layout: move the facade from `app/design_tokens.rs` to `app/design_tokens/mod.rs`, delete compatibility re-exports, update imports to `use crate::app::design_tokens::{semantic::*, spatial::*, typography::TextRole}` or narrower paths; outcome is compiler-enforced token layering; verify with `rg "use crate::app::design_tokens::\*|BG_|TEXT_|ACCENT_BLUE|GREEN|AMBER|RED|PURPLE|PAD_|FONT_SIZE_" crates/animatix-gui/src` and `cargo check -p animatix-gui`.
9. Introduce unified button widget: replace function-only APIs in `components/button.rs` with `Button`, `ButtonVariant`, and `ButtonSize` implementing `egui::Widget`, keeping temporary wrappers for `icon_button`, `toolbar_toggle_button`, and `toolbar_action_button`; outcome is one button state machine and paint path; verify with `cargo check -p animatix-gui` and toolbar/timeline transport smoke tests.
10. Migrate button call sites and delete wrappers: update `shell/toolbar.rs`, `panels/timeline_panel.rs`, `panels/sidebar.rs`, `preview/property_popup.rs`, `components/diagnostics.rs`, `shell/{command_palette.rs,find_replace.rs,insertion_palette.rs,export_dialog.rs}`, and `app/mod.rs` to the new `Button` API; outcome is no ad-hoc button helpers or raw `egui::Button::new` for standard UI buttons; verify with `rg "icon_button\(|icon_button_colored\(|toolbar_toggle_button\(|toolbar_action_button\(|egui::Button::new" crates/animatix-gui/src/app` and `cargo test -p animatix-gui`.
11. Migrate typography: add `TextRole` helpers in `design_tokens/typography.rs`, update high-density files (`components/*`, `shell/*`, `panels/*`, `preview/*`, `app/mod.rs`) from raw `FontId`/`FONT_SIZE_*` to roles, then handle `cell_editor`, `completion_popup`, and `highlighting`; outcome is consistent display/heading/title/body/caption/mono/micro usage; verify with `rg "FontId::new|FontId::monospace|FontId::proportional|FONT_SIZE_" crates/animatix-gui/src` and visual checks for timecodes, coordinates, labels, and headings.
12. Split command modules: replace `commands.rs` with `commands/mod.rs` plus `commands/{document.rs,actor.rs,keyframe.rs,scene.rs,view.rs,playback.rs}` and compatibility `From<DomainCommand>` conversions; outcome is domain command types without touching every call site at once; verify with `cargo check -p animatix-gui`.
13. Split undoable and non-undoable dispatch: update `shell/mod.rs`, `command_bus.rs`, `handlers/{actor,file,keyframe,playback,property,scene,ui}.rs`, and `document/history.rs` usage so undo stack accepts only undoable document commands while view/playback actions bypass snapshots; outcome is clearer undo semantics; verify with `cargo test -p animatix-gui command_handlers` and manual undo/redo for property edit, actor create/delete, playback, and panel toggles.
14. Migrate panel emission to domain commands: update `runtime.rs`, `app/mod.rs`, `shell/*`, `panels/*`, and `preview/*` call sites from flat `Command::` variants to domain command constructors through `ShellAction`; outcome is no flat domain mixing in UI code; verify with `rg "Command::" crates/animatix-gui/src/app crates/animatix-gui/src/cell_editor` and `cargo check -p animatix-gui`.
15. Add gesture layer scaffolding: add `preview/gesture.rs` and `preview/gesture_router.rs`, wire `preview_panel::preview_panel_ui` to create gestures from egui pointer events while still delegating to existing `drag_handler.rs`; outcome is no behavior change with a testable gesture event boundary; verify with `cargo check -p animatix-gui` and manual canvas click/drag/selection smoke tests.
16. Migrate drag modes to gesture handlers incrementally: move move, scale, rotate, vertex, pivot, motion-path, reorder, marquee, and guide-drag logic from `preview/drag_handler.rs` into focused gesture handlers using `PreviewContext`; outcome is smaller handlers that emit commands consistently; verify after each mode with manual canvas tests and `cargo test -p animatix-gui`.
17. Add keyboard navigation framework: add `app/interaction/{mod.rs,keyboard.rs}` and move shortcut/focus logic out of `runtime::handle_keyboard_shortcuts`; outcome is one focus-aware registry for global shortcuts, arrow nudging, play/pause, escape, and command palette; verify with manual keyboard tests in editor focus, inspector inputs, canvas focus, and modal focus.
18. Unify motion helpers: add `design_tokens/motion.rs` transition helpers or `app/components/anim.rs`, replace direct `animate_value_with_time` in `cell_editor/render.rs`, `shell/toolbar.rs`, and `panels/sidebar.rs`; outcome is consistent durations/easing and a future reduced-motion switch point; verify with `rg "animate_value_with_time" crates/animatix-gui/src` and visual checks for sidebar tab switch, build spinner, and cell divider hover.
19. Final cleanup and documentation sync: update `docs/gui_design_language.md` only for implementation deviations, remove completed roadmap bullets from `docs/roadmap.md`, and ensure every `#[allow(dead_code)]` added or retained has an inline justification comment; outcome is docs matching the migrated implementation; verify with `rg "#\[allow\(dead_code\)\]" crates/animatix-gui/src` and `cargo check`.
20. Final validation pass: run `cargo check`, `cargo test -p animatix-gui`, `cargo test -p animatix`, and before commit `cargo test --no-fail-fast`; outcome is build/test confidence across GUI and adjacent runtime behavior; manually smoke test `cargo run -p animatix-gui -- examples/20_feature_reel.amx` across toolbar, sidebar tabs, inspector, preview drag/edit, timeline scrubbing, export dialog, palettes, completion popup, and undo/redo.

## Files to touch
- `crates/animatix-gui/src/app/design_tokens.rs` — convert from flat token source to temporary compatibility facade, then remove after `design_tokens/mod.rs` exists.
- `crates/animatix-gui/src/app/design_tokens/mod.rs` — final token module entry point and public exports.
- `crates/animatix-gui/src/app/design_tokens/primitive.rs` — raw palette, raw spacing, raw radii, and raw durations with `pub(crate)` visibility.
- `crates/animatix-gui/src/app/design_tokens/semantic.rs` — public surface/text/accent/status/category/border/canvas/editor/syntax role tokens.
- `crates/animatix-gui/src/app/design_tokens/spatial.rs` — unified `SPACE_0..SPACE_8`, row heights, radii, and component dimensions.
- `crates/animatix-gui/src/app/design_tokens/typography.rs` — `TextRole` enum and font helpers.
- `crates/animatix-gui/src/app/design_tokens/motion.rs` — duration/easing tokens and transition helper.
- `crates/animatix-gui/src/app/design_tokens/util.rs` — temporary `lerp_color` and alpha utilities during migration.
- `crates/animatix-gui/src/app/runtime.rs` — global egui theme, clear color, keyboard extraction dependency.
- `crates/animatix-gui/src/app/mod.rs` — welcome screen, dialogs, status bar, token imports, button migration.
- `crates/animatix-gui/src/app/components/button.rs` — unified `Button` widget and removal of ad-hoc button helpers.
- `crates/animatix-gui/src/app/components/layout.rs` — card, section header, fields, labeled rows, pill tabs, typography cleanup.
- `crates/animatix-gui/src/app/components/context_menu.rs` — menu tokens, header typography, remove uppercase transformation.
- `crates/animatix-gui/src/app/components/{row.rs,timeline.rs,diagnostics.rs,easing_curve_editor.rs,toast.rs}` — semantic tokens and `TextRole` migration.
- `crates/animatix-gui/src/app/shell/{toolbar.rs,settings.rs,export_dialog.rs,insertion_palette.rs,command_palette.rs,find_replace.rs,shortcut_cheat_sheet.rs,mod.rs}` — shell chrome, modal/button migration, command dispatch updates.
- `crates/animatix-gui/src/app/panels/{mod.rs,behavior.rs,preview_panel.rs,timeline_panel.rs,sidebar.rs}` — panel frames, timeline colors, transport controls, sidebar tabs, command calls.
- `crates/animatix-gui/src/app/panels/inspector/{mod.rs,property_groups.rs,keyframe_table.rs,graph_editor.rs,spreadsheet.rs}` — inspector rows, controls, property group colors, typography, commands.
- `crates/animatix-gui/src/app/preview/{mod.rs,context.rs,drag_handler.rs,grid.rs,overlay.rs,property_popup.rs,selection.rs,time_lens.rs}` — canvas tokens, gesture migration, preview overlays, popup controls.
- `crates/animatix-gui/src/app/preview/{gesture.rs,gesture_router.rs}` — new gesture boundary and router.
- `crates/animatix-gui/src/app/interaction/{mod.rs,keyboard.rs}` — new focus-aware keyboard shortcut registry.
- `crates/animatix-gui/src/app/commands.rs` — temporary command facade or deletion after split.
- `crates/animatix-gui/src/app/commands/{mod.rs,document.rs,actor.rs,keyframe.rs,scene.rs,view.rs,playback.rs}` — new domain command packages.
- `crates/animatix-gui/src/app/command_bus.rs` — emit domain actions and separate undoable from non-undoable actions.
- `crates/animatix-gui/src/app/handlers/{actor.rs,file.rs,keyframe.rs,playback.rs,property.rs,scene.rs,ui.rs,mod.rs}` — adjust handler signatures to domain commands and undo semantics.
- `crates/animatix-gui/src/app/stores/{ui_store.rs,history_store.rs,preview_store.rs,document_store.rs}` — focus/keyboard state and undo snapshot typing if needed.
- `crates/animatix-gui/src/cell_editor/render.rs` — editor tokens, motion helper, typography roles.
- `crates/animatix-gui/src/completion_popup.rs` — completion-popup tokens and typography roles.
- `crates/animatix-gui/src/highlighting.rs` — isolate syntax color tokens or document as syntax-specific exception.
- `docs/gui_design_language.md` — update only if implementation intentionally deviates from the draft spec.
- `docs/roadmap.md` — remove completed GUI migration bullets as phases land.

## Risks
- Rust module transition risk: `app/design_tokens.rs` and `app/design_tokens/mod.rs` cannot coexist as the same module, so use a temporary facade first and only move to `mod.rs` after call sites are migrated.
- Visual regression risk: changing base surface/text values affects all panels; mitigate with phase-by-phase smoke tests and screenshot comparison if available.
- Token naming churn risk: migrating every `BG_*`/`TEXT_*` call in one pass is high-noise; keep compatibility aliases until each surface is migrated and verified.
- Syntax highlighting risk: code colors are semantic to the language rather than GUI chrome; avoid forcing them into status/category tokens.
- Undo regression risk: splitting commands can accidentally snapshot playback/view actions or stop snapshotting document mutations; verify undo/redo manually and with handler tests.
- Interaction regression risk: `preview/drag_handler.rs` covers many modes and edge cases including layout reorder, snapping, guides, vertices, pivot, and motion paths; migrate one gesture mode at a time.
- Accessibility risk: new disabled/muted colors must still meet intended contrast rules on their actual backgrounds; verify likely text/background pairs after Phase 1.
- Roadmap/docs drift risk: `docs/roadmap.md` explicitly says completed work should be removed, so update it only when each phase actually lands.
