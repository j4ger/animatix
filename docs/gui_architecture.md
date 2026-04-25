# Animatix GUI Architecture

## Overview

`animatix-gui` is a separate desktop application crate that wraps the existing Animatix runtime with an egui-based desktop shell. The GUI is intentionally editor-first, not node-editor-first: the source of truth remains `.amx` text, while the app provides live preview and timeline control.

## Current Status

The first GUI MVP is now shipped in `crates/animatix-gui`.

This document is the current authoritative GUI reference. Older phase-by-phase rollout tracking has been consolidated away so the repo only keeps one GUI architecture/status doc.

What exists today:

- an egui desktop app built on `egui`, `egui-winit`, `egui_dock`, `egui_wgpu_backend`, `winit`, and `wgpu`
- startup loading of an `.amx` file
- multiline source editing with a code-editor widget
- save and reload actions
- debounced rebuilds through the real Animatix loader/parser path
- timeline scrubbing and play/pause
- a visual timeline scrubber with click-and-drag seeking, timeline ticks, and keyframe markers
- cross-platform live preview rendering via a persistent offscreen GPU renderer with in-memory egui texture presentation
- preview sizing that stays fixed to the document scene dimensions rather than following tile size
- a denser explorer and a simplified top bar for the current editor-first shell
- file watching with auto-reload when the loaded .amx file changes on disk
- a unified bottom transport bar that anchors transport controls and the timeline to the bottom of the preview tile
- a split internal architecture between document state and preview state
- a preview surface seam that supports the current offscreen live-preview path and leaves room for future preview delivery changes
- a docked workspace shell built with egui panels and tabs rather than a separate node-editor environment

What is intentionally not shipped yet:

- embedded live native GPU surface composition inside the current egui shell
- richer diagnostic UX inside the editor shell, such as stronger parse/build feedback surfaces
- Tree-sitter-backed syntax highlighting / autocomplete / code intelligence
- visual timeline lanes or scene inspectors

## Product Shape

The initial window is split into three functional regions:

1. **Editor pane** — multiline `.amx` source editing
2. **Preview pane** — live render of the current timeline time with a fixed-aspect preview surface derived from scene dimensions
3. **Explorer / transport shell** — docked file navigation plus a bottom-anchored transport bar inside the preview tile

This keeps the first GUI aligned with the current language/runtime maturity. The app is an interactive companion to the DSL, not yet a full scene authoring environment.

## Relationship to the Core Runtime

The existing `animatix` crate remains the source of truth for:

- module loading
- parsing
- timeline construction
- scene evaluation
- Vello scene generation

The GUI crate should not fork parsing or evaluation logic. It should call into the core runtime and surface the results.

## UI Composition Strategy

The current GUI uses egui for both shell-level and domain-specific composition:

- `egui_dock` manages the docked workspace regions
- `egui::TextEdit` provides the multiline editor surface, with custom layout/highlighting glue from `crates/animatix-gui/src/editor.rs`
- custom app/session code coordinates document rebuilds, preview playback, file navigation, and preview rendering

This keeps the implementation close to the runtime while still leaving room for future UI refinement.

## Main State Model

The GUI now revolves around two cooperating state domains:

### `DocumentSession`

Owns:

- current file path
- current source text
- dirty/clean state
- latest compiled AST
- latest compiled timeline
- derived timeline duration
- derived scene dimensions from document config/defaults

This layer handles file loading, save/reload, and rebuilds through the real Animatix loader/parser path.

### `PreviewPaneState`

Owns:

- current preview time
- playback state
- preview dimensions
- preview status/error state

This layer controls playback state and the preview-pane contract seen by the UI.

The editor updates `DocumentSession`. Rebuild operations refresh the compiled document and duration. `PreviewPaneState` owns current time, playback state, status text, errors, and preview dimensions.

## Hot Reload

The GUI automatically watches the currently loaded .amx file for changes. When the file is modified externally (e.g., via an external editor or save action), the GUI:

1. Detects the file change via the OS file watcher
2. Debounces rapid changes (300ms) to avoid reload storms
3. Automatically reloads the file from disk
4. Rebuilds the timeline
5. Updates the editor buffer to reflect the new content

This enables a smooth workflow where users can edit files in their preferred editor while still seeing live preview updates.

### How it works

- Files are watched using the `notify` crate's system-specific watcher
- Changes trigger the same code path as manual "Reload from Disk"
- The `prepare_frame()` method calls `check_hot_reload()` each frame
- Reload state is tracked to avoid duplicate reloads

## Rebuild Flow

The rebuild pipeline is:

1. read the current source text
2. load/parse the file graph through the core runtime where applicable
3. build a `Timeline`
4. store parse/runtime errors if the rebuild fails
5. preserve the last successful preview state when possible

This avoids blanking the app on every transient typing mistake.

## Preview Architecture

The preview subsystem now has a dedicated rendering surface object.

### `PreviewSurface`

The GUI does not assume that preview output is a file on disk. `PreviewSurface` owns the offscreen textures and the bridge from the core runtime renderer into an egui texture.

Current responsibilities:

- allocate and resize offscreen render/sample textures
- evaluate the current timeline time through the core runtime
- render into the offscreen texture via `RendererCore`
- copy the result into an egui-visible texture

### Current implementation

The shipped implementation is an offscreen live-preview path. `PreviewSurface` owns a persistent renderer, renders the current timeline time into an offscreen texture, and keeps that result synchronized with an egui texture for presentation in the preview pane.

### Future direction

A future embedded-surface path could render more directly into the windowing stack, but that is a later rendering-architecture decision rather than something already abstracted behind a shipped backend trait.

The critical architectural rule remains the same: the egui app shell owns window/event/UI flow, while the core Animatix library owns parsing, evaluation, and render-data generation.

## Timeline Architecture

The timeline transport is intentionally minimal in the MVP:

- visual scrubber track
- keyframe markers and timeline ticks
- current time label
- play/pause and rebuild controls
- duration and resolution metadata inside the transport bar

The shipped scrubber is still a navigation control rather than an editable keyframe lane editor. It now provides a visual transport surface, but it does not yet expose draggable keyframe blocks or per-track lanes.

## Editor Architecture

The editor pane is text-based and currently uses `egui::TextEdit` for the multiline editing surface.

Today the shipped editor uses a local highlighting path from `crates/animatix-gui/src/editor.rs`: `egui::TextEdit` plus a Syntect-backed layouter that includes a repo-local `animatix.sublime-syntax`. That path is intentionally pragmatic; it is not the long-term source of syntax truth.

Desired properties:

- dependable multiline editing
- fast rebuild after edits
- visible error state
- save command
- reload from disk

### Syntax Metadata Integration

A dedicated Tree-sitter grammar for `.amx` now lives in `tree-sitter-animatix/`. It is intended to be consumed as shared language metadata by external editors/tools and, later, by the GUI itself.

Architectural rules for that integration:

- `crates/animatix/src/parser.rs` remains the executable source of truth for accepted syntax
- Tree-sitter grammar/query assets are derived editor-facing metadata that must stay synchronized with the parser and `docs/spec.md`
- GUI syntax integration should not outrun diagnostic UX or the current runtime contract surface
- Tree-sitter-backed highlighting remains a possible later integration, not the default next GUI milestone
- before consuming Tree-sitter in the GUI, identify a concrete authoring-feedback gap that simpler diagnostics, examples, or lighter editor feedback cannot solve at lower maintenance cost
- the initial grammar corpus should come from curated runnable examples plus parser tests, not from deprecated or removed syntax sketches

Current status:

- the grammar package exists and passes its local generate/test workflow
- the GUI still uses the local Syntect-based highlighting path in `crates/animatix-gui/src/editor.rs`
- GUI integration remains deferred work; shipping the grammar package does not yet mean the GUI should consume it before the diagnostic UX justifies the extra maintenance cost

## Preview Delivery Strategy

The current GUI preview is intentionally pragmatic but no longer file-based: it uses a persistent offscreen GPU renderer from the core runtime to render the current time, synchronizes that frame into an egui texture, and displays it inside the preview pane.

That means the preview is still backed by the real Animatix runtime and scene evaluation path, but it is still **not** a separately embedded native preview surface. The transport is an offscreen renderer plus egui texture synchronization rather than PNG temp files.

The important architectural change is that preview delivery is isolated in `PreviewSurface` rather than smeared through the document/session layer. A more direct surface path remains a future improvement if the window/render stack grows a cleaner cross-platform integration point.

## Error Model

The app should distinguish between:

1. file loading/import errors
2. parse errors
3. timeline/runtime build errors
4. preview/render errors

The UI should show these clearly without crashing or destroying the last good state.

Near-term GUI work should make these existing error classes clearer and more actionable before adding a richer syntax integration layer.

## Deferred Features

These are deliberately out of scope for the first GUI crate:

- native embedded surface composition inside the egui shell
- a more direct preview-surface integration strategy if the render stack warrants it later
- Tree-sitter-backed GUI highlighting or code intelligence until its authoring value justifies the extra parser/query synchronization cost
- visual scene inspector
- property editor
- editable keyframe lane editor
- export dialogs
- collaborative/project workflows
- syntax-aware code intelligence beyond grammar-backed highlighting
