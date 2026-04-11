# Animatix GUI Architecture

## Overview

`animatix-gui` is a separate desktop application crate that wraps the existing Animatix runtime with a GPUI shell. The GUI is intentionally editor-first, not node-editor-first: the source of truth remains `.amx` text, while the app provides live preview and timeline control.

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

## Main State Model

The GUI revolves around a central session state:

- current file path
- current source text
- dirty/clean state
- latest parse result
- latest timeline build result
- latest user-facing error message(s)
- current preview time
- playback state
- preview size

The editor updates source text. Rebuild operations update the parse/timeline state. The preview pane renders from the latest successful timeline. The scrubber changes `current_time` and requests a repaint.

## Rebuild Flow

The rebuild pipeline is:

1. read the current source text
2. load/parse the file graph through the core runtime where applicable
3. build a `Timeline`
4. store parse/runtime errors if the rebuild fails
5. preserve the last successful preview state when possible

This avoids blanking the app on every transient typing mistake.

## Preview Architecture

The preview pane reuses the existing Animatix evaluation model:

1. evaluate the timeline at `current_time`
2. produce a `vello::Scene`
3. render the scene into a preview surface

The critical architectural rule is that GPUI owns the application shell and event flow, while the core Animatix library owns evaluation and render data generation.

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
- visual scene inspector
- property editor
- keyframe lane editor
- export dialogs
- collaborative/project workflows
- syntax-aware code intelligence
