# Animatix GUI Implementation Plan

This file now tracks both what has landed and what remains after the first GUI MVP and the preview-architecture refactor.

## Phase 1: Workspace and Shell

**Status:** Completed

- add `crates/animatix-gui` to the workspace
- create a GUI binary entry point
- establish shared session/app state
- open a GPUI window with editor, preview, and timeline regions
- use a cross-platform offscreen GPU preview path for the current release rather than direct embedded GPU-surface composition
- migrate the app shell onto `gpui-component` primitives for root/window chrome, themed controls, and resizable workspace layout

## Phase 2: Core Interactivity

**Status:** Completed for the MVP scope

- load an `.amx` file at startup
- display editable source text
- debounce rebuild after edits
- support save and reload
- surface parse/runtime errors in the window

## Phase 3: Preview and Timeline

**Status:** Completed for the MVP scope

- evaluate the current timeline time from the shared state
- render a live preview
- add timeline scrubber and current-time display
- add play/pause and timer-driven time advancement

## Phase 4: Stability and Usability

**Status:** Partially completed

- keep the last successful timeline while edits are invalid
- handle missing files/import failures gracefully
- make window resizing update preview dimensions **(still open)**
- validate the GUI against existing examples

## Phase 5: Preview Architecture Refactor

**Status:** Completed for the offscreen live-preview stage

- split GUI state into document-oriented and preview-oriented responsibilities
- move preview rendering behind a `PreviewBackend` abstraction
- represent preview output as an artifact rather than baking file paths into the whole session model
- ship a persistent offscreen GPU renderer with in-memory GPUI image presentation
- leave a reserved seam for a future native embedded preview surface backend

## Remaining Follow-up Work

- consider moving from the current offscreen-image transport to direct native embedded GPU rendering if/when GPUI exposes a supported cross-platform path
- decide whether additional custom widgets should migrate to `gpui-component` or remain domain-specific
- add more focused GUI/session tests beyond the current duration/state smoke coverage
- add keyboard transport shortcuts and richer editor ergonomics
- replace the current local `.amx` fallback syntax definition with shared Tree-sitter-backed language metadata once that grammar exists

## Phase 6: Syntax Metadata and Highlighting

**Status:** Planned

- ship a dedicated Tree-sitter grammar for `.amx`
- declare `.amx` through standard Tree-sitter metadata so external editors/tools can discover it
- add highlight queries for the currently shipped parser surface
- keep the GUI aligned with the parser/spec/docs by consuming those shared syntax assets instead of extending the ad hoc keyword list in isolation
- treat parser tests and runnable examples as the initial highlighting/grammar validation corpus
- exclude removed or non-parser syntax from the initial grammar scope so the GUI does not regress into documenting dead language surface

**Acceptance criteria:**

- the grammar covers the syntax actually accepted by `crates/animatix/src/parser.rs`
- parser/spec mismatches are resolved before the GUI starts depending on grammar-backed highlighting
- the GUI can map `.amx` files to shared language metadata without introducing a second independent syntax definition
- external tools can discover the language through standard Tree-sitter packaging metadata
- the initial query set and keyword captures come from the shipped language surface, not historical placeholders

## Verification Checklist

- workspace builds successfully
- the GUI launches against a sample `.amx`
- typing in the editor triggers rebuild attempts
- save persists changes to disk
- timeline scrubbing updates the preview
- play/pause advances and stops time correctly

## Deferred After MVP

- direct native Vello / `wgpu` surface embedding inside GPUI
- syntax highlighting before the shared Tree-sitter grammar exists
- scene graph inspector
- export controls
- multi-file project sidebar
- visual timeline/keyframe lanes
- property panels and direct manipulation
