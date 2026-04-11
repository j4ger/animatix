# Animatix GUI Architecture

## Overview

`animatix-gui` is a separate desktop application crate that wraps the existing Animatix runtime with a GPUI shell built on top of `gpui-component` for the application chrome. The GUI is intentionally editor-first, not node-editor-first: the source of truth remains `.amx` text, while the app provides live preview and timeline control.

## Current Status

The first GUI MVP is now shipped in `crates/animatix-gui`.

What exists today:

- a GPUI desktop app
- startup loading of an `.amx` file
- line-oriented source editing
- save and reload actions
- debounced rebuilds through the real Animatix loader/parser path
- timeline scrubbing and play/pause
- snapshot-based preview rendering using the shipped `render_image` runtime path
- a split internal architecture between document state and preview state
- a preview backend seam that preserves snapshot rendering today and leaves room for a future true preview surface
- a `gpui-component`-based shell using `Root`, themed panels, resizable layout, and component buttons

What is intentionally not shipped yet:

- a full multiline document editor inside the main pane
- embedded live Vello surface composition inside GPUI
- syntax highlighting / autocomplete / code intelligence
- visual timeline lanes or scene inspectors

## Product Shape

The initial window is split into three functional regions:

1. **Editor pane** — line-oriented `.amx` source editing
2. **Preview pane** — live render of the current timeline time
3. **Timeline pane** — scrubber, time display, and play/pause controls

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

The current GUI uses a hybrid composition model:

- `gpui-component` for shell-level UI concerns such as the root app wrapper, theme tokens, action buttons, and resizable workspace layout
- custom GPUI views for domain-specific elements such as the preview host and the line-oriented text editor

This keeps the app visually more coherent without giving up the custom pieces that are still specific to Animatix behavior.

## Main State Model

The GUI now revolves around two cooperating state domains:

### `DocumentSession`

Owns:

- current file path
- current source text
- dirty/clean state
- latest compiled AST
- derived timeline duration

This layer handles file loading, save/reload, and rebuilds through the real Animatix loader/parser path.

### `PreviewSession`

Owns:

- current preview time
- playback state
- preview dimensions
- preview artifact/status/error state
- active preview backend

This layer controls how a compiled document becomes something visible in the preview pane.

The editor updates `DocumentSession`. Rebuild operations refresh the compiled document and duration. The preview pane renders from `PreviewSession`, which asks its backend to present the current frame.

## Rebuild Flow

The rebuild pipeline is:

1. read the current source text
2. load/parse the file graph through the core runtime where applicable
3. build a `Timeline`
4. store parse/runtime errors if the rebuild fails
5. preserve the last successful preview state when possible

This avoids blanking the app on every transient typing mistake.

## Preview Architecture

The preview subsystem now has an explicit transport boundary.

### `PreviewBackend`

The GUI does not assume that preview output is always a PNG file. Instead, a backend produces a `PreviewArtifact`.

Current artifact shape:

- `Snapshot(PathBuf)`

Reserved future shape:

- embedded surface / GPU-backed artifact

### Current backend

The shipped backend is `SnapshotBackend`, which wraps the existing `render_image` path and produces snapshot artifacts.

### Future backend

A future `EmbeddedSurfaceBackend` can render into a true preview surface without changing document editing, playback, or timeline logic. The migration point is the backend layer, though the preview pane/UI plumbing will still need an additional rendering branch for non-snapshot artifacts.

The critical architectural rule remains the same: GPUI owns the application shell and event flow, while the core Animatix library owns evaluation and render data generation.

## Timeline Architecture

The timeline pane is intentionally minimal in the MVP:

- scrubber/slider
- current time label
- play/pause
- optional duration label

The scrubber does not expose editable keyframe blocks in the first release. It is a playback/navigation control over the existing runtime timeline.

## Editor Architecture

The editor pane is text-based. The first release ships a **line-oriented editor**: the source is shown as numbered lines, and the selected line is edited through a focused text field. This keeps the first GPUI integration small and dependable while still allowing real code editing, insertion, deletion, save, reload, and rebuild.

Desired properties:

- dependable line editing
- fast rebuild after edits
- visible error state
- save command
- reload from disk

## Preview Delivery Strategy

The current GUI preview is intentionally pragmatic: it uses the existing `render_image` path from the core runtime to generate a PNG snapshot for the current time and displays that image inside the GPUI window.

That means the preview is still backed by the real Animatix runtime and scene evaluation path, but it is **snapshot-based**, not a shared live GPU surface embedded directly into GPUI. This is a deliberate MVP tradeoff to keep the GUI buildable and honest while avoiding a much larger GPUI/Vello surface-integration effort.

The important architectural change is that this is now implemented as a backend choice rather than a hardcoded assumption in the main session state.

## Error Model

The app should distinguish between:

1. file loading/import errors
2. parse errors
3. timeline/runtime build errors
4. preview/render errors

The UI should show these clearly without crashing or destroying the last good state.

## Deferred Features

These are deliberately out of scope for the first GUI crate:

- full multiline document editor replacing the line-oriented MVP editor
- preview temp-file cleanup and more incremental preview updates
- direct Vello surface embedding inside GPUI
- an embedded-surface preview backend implementation
- a full migration of every custom widget to `gpui-component`
- visual scene inspector
- property editor
- keyframe lane editor
- export dialogs
- collaborative/project workflows
- syntax-aware code intelligence
