# Animatix GUI Implementation Plan

This file now tracks both what has landed and what remains after the first GUI MVP.

## Phase 1: Workspace and Shell

**Status:** Completed

- add `crates/animatix-gui` to the workspace
- create a GUI binary entry point
- establish shared session/app state
- open a GPUI window with editor, preview, and timeline regions
- use a snapshot-based preview path for the first release rather than direct embedded GPU-surface composition

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

## Remaining Follow-up Work

- replace the line-oriented editor with a fuller multiline editing surface
- clean up generated preview temp files automatically
- move from snapshot-based preview to direct embedded GPU rendering if/when the integration cost is justified
- add more focused GUI/session tests beyond the current duration/state smoke coverage
- add keyboard transport shortcuts and richer editor ergonomics

## Verification Checklist

- workspace builds successfully
- the GUI launches against a sample `.amx`
- typing in the editor triggers rebuild attempts
- save persists changes to disk
- timeline scrubbing updates the preview
- play/pause advances and stops time correctly

## Deferred After MVP

- full multiline editor widget inside the main pane
- direct Vello-in-GPUI surface embedding
- syntax highlighting
- scene graph inspector
- export controls
- multi-file project sidebar
- visual timeline/keyframe lanes
- property panels and direct manipulation
