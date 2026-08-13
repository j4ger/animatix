# Dogfood Review Demo

## Scope

A first working review interface for language-design A/B experiments. It is
deliberately not an arena or survey product yet: the immediate reviewer is the
user working in the local repository.

## Deliverable

`animatix-gui --review dogfood/runs/<slug>` opens a run directory and loads
every `.amx` variant into a dedicated review window. The window provides:

- live WGPU preview for the selected variant at the shared playback head;
- read-only top status bar and a bottom review console that hosts variant
  target, Single/Compare mode, playback, time, speed, review completion, and
  comments;
- read-only syntax-highlighted source with line numbers and diagnostics;
- comments anchored to variant, time, and source line;
- `review.json` persistence for later agent consumption;
- an explicit `review.done` marker that also closes the review window.

The demo run `dogfood/runs/001-stagger-entry/` compares `stagger` with explicit
per-actor delays. Run directories are local-only and gitignored; the workflow
guide and templates are committed instead.

## Architecture

The review app is a standalone eframe app in
`crates/animatix-gui/src/app/review/`. It reuses `PreviewSurface` for rendering
and `DocumentSession` for parse/build. It intentionally does not reuse the full
editing `GuiShell`: that would couple a demo tool to undo, hot reload, file
tree, and mutation command paths.

Comment storage uses a stable JSON schema so a future arena/survey frontend can
read the same run artifacts.

## Deferred

- Static HTML questionnaire/arena frontend.
- Proposed-syntax variants that cannot be parsed by the current grammar.
- Video/WebM artifact review as an alternative to realtime rendering.
- Agent-driven run discovery and hypothesis generation.

## Status

Demo delivered 2026-08-13. The review GUI persists comments to `review.json`
and writes an explicit `review.done` marker. `scripts/dogfood-review.sh` wraps
validation, build, launch, and completion for agent workflows. Deferred items
remain out of scope until a concrete external-reviewer or blind-evaluation need
appears.
