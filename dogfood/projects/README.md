# Dogfood Projects

One folder per real project.

Each project should contain:

- `brief.md` - the content goal, audience, scenes, and constraints.
- `entry.amx` - the runnable scene entry point.
- `notes.md` - idiomatic wins, workarounds, diagnostics, and open gaps.

Optional project-local assets live in `<name>/assets/`. Shared example assets
can be referenced from the workspace root as `examples/assets/...`.

Keep projects named by content, not feature, unless the content itself is the
feature probe. Use `examples/` naming conventions when a project graduates to
the curated suite.

## Current Projects

- `sorting-visualizer/` - insertion-sort explainer; first idiomatic pass with
  array actors, indexed targets, callout retargeting, and module tokens.
