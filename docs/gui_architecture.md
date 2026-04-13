# Animatix GUI Architecture

## Overview

`animatix-gui` is a separate desktop application crate that wraps the existing Animatix runtime with a GPUI shell built on top of `gpui-component` for the application chrome. The GUI is intentionally editor-first, not node-editor-first: the source of truth remains `.amx` text, while the app provides live preview and timeline control.

## Current Status

The first GUI MVP is now shipped in `crates/animatix-gui`.

What exists today:

- a GPUI desktop app
- startup loading of an `.amx` file
- multiline source editing with a code-editor widget
- save and reload actions
- debounced rebuilds through the real Animatix loader/parser path
- timeline scrubbing and play/pause
- cross-platform live preview rendering via a persistent offscreen GPU renderer with in-memory GPUI image presentation
- a split internal architecture between document state and preview state
- a preview backend seam that supports the current offscreen live-preview path and leaves room for a future true native preview surface
- a `gpui-component`-based shell using `Root`, themed panels, resizable layout, and component buttons

What is intentionally not shipped yet:

- embedded live native GPU surface composition inside GPUI
- Tree-sitter-backed syntax highlighting / autocomplete / code intelligence
- visual timeline lanes or scene inspectors

## Product Shape

The initial window is split into three functional regions:

1. **Editor pane** — multiline `.amx` source editing
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
- custom GPUI views for domain-specific elements such as the preview host and editor/preview coordination

This keeps the app visually more coherent without giving up the custom pieces that are still specific to Animatix behavior.

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

The GUI does not assume that preview output is always a file on disk. Instead, a backend produces a `PreviewArtifact`.

Current artifact shape:

- in-memory `RenderImage` frames generated from the live offscreen renderer

Reserved future shape:

- native embedded surface / shared GPU-backed artifact

### Current backend

The shipped backend is an offscreen live-preview backend. It owns a persistent WGPU/Vello renderer, renders the current timeline time into an offscreen texture, reads the frame back into memory, and presents that frame inside GPUI as an in-memory image.

### Future backend

A future `EmbeddedSurfaceBackend` can render into a true native preview surface without changing document editing, playback, or timeline logic. The migration point is the backend layer, though the preview pane/UI plumbing will still need an additional rendering branch for non-image artifacts.

The critical architectural rule remains the same: GPUI owns the application shell and event flow, while the core Animatix library owns evaluation and render data generation.

## Timeline Architecture

The timeline pane is intentionally minimal in the MVP:

- scrubber/slider
- current time label
- play/pause
- optional duration label

The scrubber does not expose editable keyframe blocks in the first release. It is a playback/navigation control over the existing runtime timeline.

## Editor Architecture

The editor pane is text-based and now uses a multiline code-editor surface from `gpui-component`, rather than the older line-oriented MVP approach.

Today the shipped editor still uses a local fallback syntax configuration from `crates/animatix-gui/src/editor.rs`. That fallback is intentionally small and keyword-driven; it is not a reusable language package and should not be treated as the long-term source of syntax truth.

Desired properties:

- dependable multiline editing
- fast rebuild after edits
- visible error state
- save command
- reload from disk

### Planned Syntax Metadata Integration

The next syntax step should be a dedicated Tree-sitter grammar for `.amx`, consumed as shared language metadata by external editors/tools and, later, by the GUI itself.

Architectural rules for that integration:

- `crates/animatix/src/parser.rs` remains the executable source of truth for accepted syntax
- Tree-sitter grammar/query assets are derived editor-facing metadata that must stay synchronized with the parser and `docs/spec.md`
- the GUI should migrate away from its ad hoc keyword list toward those shared syntax assets rather than maintaining a separate grammar definition
- initial GUI adoption should focus on highlighting first; richer code intelligence can follow later
- the initial grammar corpus should come from curated runnable examples plus parser tests, not from deprecated or removed syntax sketches

## Preview Delivery Strategy

The current GUI preview is intentionally pragmatic but no longer file-based: it uses a persistent offscreen GPU renderer from the core runtime to render the current time, converts that frame into an in-memory GPUI image, and displays it inside the preview pane.

That means the preview is still backed by the real Animatix runtime and scene evaluation path, but it is still **not** a shared native GPU surface embedded directly into GPUI. The transport is now in-memory image upload rather than PNG temp files. This is the current cross-platform solution because GPUI does not yet expose a generic embedded native `wgpu` surface path.

The important architectural change is that this is implemented as a backend choice rather than a hardcoded assumption in the main session state. A true native embedded surface remains a future improvement if GPUI grows a supported cross-platform API for it.

## Error Model

The app should distinguish between:

1. file loading/import errors
2. parse errors
3. timeline/runtime build errors
4. preview/render errors

The UI should show these clearly without crashing or destroying the last good state.

## Deferred Features

These are deliberately out of scope for the first GUI crate:

- native embedded surface composition inside GPUI
- an embedded-surface preview backend implementation when GPUI supports it cross-platform
- a full migration of every custom widget to `gpui-component`
- visual scene inspector
- property editor
- keyframe lane editor
- export dialogs
- collaborative/project workflows
- syntax-aware code intelligence beyond grammar-backed highlighting
