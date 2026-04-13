# Animatix GUI Implementation Plan

This file now tracks both what has landed and what remains after the first GUI MVP and the preview-architecture refactor.

## Phase 1: Workspace and Shell

**Status:** Completed

- add `crates/animatix-gui` to the workspace
- create a GUI binary entry point
- establish shared session/app state
- open an egui window with editor, preview, and timeline regions
- use a cross-platform offscreen GPU preview path for the current release rather than direct embedded GPU-surface composition
- build the app shell on egui workspace primitives, docked panels, and custom session logic

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
- make window resizing update preview dimensions
- validate the GUI against existing examples

## Phase 5: Preview Architecture Refactor

**Status:** Completed for the offscreen live-preview stage

- split GUI state into document-oriented and preview-oriented responsibilities
- consolidate preview rendering behind `PreviewSurface`
- keep preview delivery independent from file-path-based temp artifacts
- ship a persistent offscreen GPU renderer with in-memory egui texture presentation
- leave room for future preview-delivery refactors without changing document rebuild flow

## Remaining Follow-up Work

- consider moving from the current offscreen-texture path to a more direct embedded preview surface if the render/window stack warrants it later
- decide whether additional custom widgets should stay local or be replaced as the egui shell evolves
- add more focused GUI/session tests beyond the current duration/state smoke coverage
- add keyboard transport shortcuts and richer editor ergonomics
- replace the current local `.amx` fallback syntax definition with the shared Tree-sitter-backed language metadata now shipped in `tree-sitter-animatix/`

## Phase 6: Syntax Metadata and Highlighting

**Status:** Grammar package shipped; GUI integration still pending

- the dedicated Tree-sitter grammar for `.amx` now exists in `tree-sitter-animatix/`
- `.amx` is declared through standard Tree-sitter metadata for downstream editors/tools
- highlight queries now exist for the currently shipped parser surface
- keep the GUI aligned with the parser/spec/docs by consuming those shared syntax assets instead of extending the ad hoc keyword list in isolation
- treat parser tests and runnable examples as the initial highlighting/grammar validation corpus
- exclude removed or non-parser syntax from the initial grammar scope so the GUI does not regress into documenting dead language surface

**Acceptance criteria:**

- the grammar covers the syntax actually accepted by `crates/animatix/src/parser.rs`
- parser/spec mismatches are resolved before the GUI starts depending on grammar-backed highlighting
- the GUI can map `.amx` files to shared language metadata without introducing a second independent syntax definition
- external tools can discover the language through standard Tree-sitter packaging metadata
- the initial query set and keyword captures come from the shipped language surface, not historical placeholders
- the GUI no longer needs to invent a second independent keyword list once integration work lands

## Verification Checklist

- workspace builds successfully
- the GUI launches against a sample `.amx`
- typing in the editor triggers rebuild attempts
- save persists changes to disk
- timeline scrubbing updates the preview
- play/pause advances and stops time correctly

## Deferred After MVP

- direct native Vello / `wgpu` surface embedding inside the current egui shell
- syntax highlighting before the shared Tree-sitter grammar exists
- scene graph inspector
- export controls
- multi-file project sidebar
- visual timeline/keyframe lanes
- property panels and direct manipulation
