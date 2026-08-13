# Dogfooding

This directory is the workshop for real Animatix content. It is intentionally
separate from `examples/` so that in-progress scenes and intentionally failing
grammar probes do not pollute the curated suite.

A dogfooding session starts with a real brief, not a feature demo. The goal is
to find where the grammar is expressive, where it forces workarounds, and where
diagnostics make the intended change obvious.

## Structure

- `projects/<name>/` - one complete real project per folder.
- `probes/<NNN>-<slug>/` - minimal repros for language/grammar gaps.
- `runs/<slug>/` - focused A/B language-design review experiments. Run
  directories are local-only and gitignored; the workflow guide and templates
  are committed.
- `templates/` - starter files for projects, probes, and runs.

Projects may be moved into `examples/projects/` after they are polished and
pass the standard checks. Probes stay here and become design tickets.

## Workflow

1. Pick a real brief: an explainer, dashboard, generative piece, or content
   port. Write the brief in `projects/<name>/brief.md`.
2. Author the scene in idiomatic Animatix first. Do not start with workarounds.
3. Run the standard checks from the workspace root:
   ```bash
   cargo run --bin animatix -- check dogfood/projects/<name>/entry.amx
   cargo run --bin animatix -- lint dogfood/projects/<name>/entry.amx
   cargo run --bin animatix -- image dogfood/projects/<name>/entry.amx --time 1.0 --output /tmp/dogfood.png
   ```
4. When the grammar blocks the intent, create a minimal probe under
   `probes/`. The probe should reproduce the gap with the smallest possible
   `.amx`, not with the whole project.
5. For A/B expression comparisons, create a focused `runs/<slug>/` with one
   brief and at least two `.amx` variants, then review in the GUI:
   ```bash
   bash scripts/dogfood-review.sh <slug>
   ```
   The full agent/human handoff loop is documented in `runs/README.md`.
6. Record the workaround and impact in `projects/<name>/notes.md`. This is
   where grammar/feature design feedback should live. Run conclusions live in
   `runs/<slug>/run.md` and `runs/<slug>/review.md`.

## Rules

- Projects must be useful content, not feature checklists.
- No hand-computed coordinates, repeated hand-named actors, or copy-paste
  scaffolding if the DSL can express it better. Record every rule you cannot
  follow; that is the signal.
- Every probe must include: intent, minimal repro, expected DSL, current
  workaround, diagnostics/behavior, impact, and recommendation.
- Known-broken probes are allowed here, but they must be named and documented.
  Do not move them into `examples/`.
- Runs must compare the same brief and success criteria across variants, and
  each run should isolate one design question.
- Always run `bash scripts/check_examples.sh` and
  `bash scripts/check-parser-sync.sh` before promoting a project to `examples/`.
