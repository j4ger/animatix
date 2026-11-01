# Dogfooding

This directory is the workshop for real Animatix content. It is intentionally
separate from `examples/` so that in-progress scenes and intentionally failing
grammar probes do not pollute the curated suite.

A dogfooding session starts with a real brief, not a feature demo. The goal is
to find where the grammar is expressive, where it forces workarounds, and where
diagnostics make the intended change obvious.

## Artifact Types

| Path | Purpose | Lifecycle |
|---|---|---|
| `projects/<name>/` | One complete real project | Polish and promote to `examples/projects/`; `notes.md` records grammar feedback |
| `probes/<NNN>-<slug>/` | Minimal repro for one parser, type, runtime, or renderer gap | Stays in dogfood as a regression fixture or design ticket |
| `arena/battles/<slug>/` | Head-to-head candidate review in Web Arena | Evaluated via browser WebGPU player; decisions logged to `feedback.jsonl` |

Use a project to ask "can the current DSL express real content?", a probe to
ask "what is the smallest failing example?", and an arena battle to ask "which
candidate provides the better visual outcome, motion polish, or DSL ergonomics?"

## Workflow

1. Pick a real brief: an explainer, dashboard, generative piece, or content
   port. Write the brief in `projects/<name>/brief.md`.
2. Author the scene in idiomatic Animatix first. Do not start with workarounds.
3. Run the standard automatic gates from the workspace root:
   ```bash
   cargo run --bin animatix -- check dogfood/projects/<name>/entry.amx
   cargo run --bin animatix -- lint dogfood/projects/<name>/entry.amx
   cargo run --bin animatix -- review dogfood/projects/<name>/entry.amx
   ```
   `animatix review` is the default automatic review gate: it needs no
   `verify.txt` and no GPU (the pixel tier is skipped when no adapter exists).
   A still render remains useful for eyeballing a single frame:
   ```bash
   cargo run --bin animatix -- image dogfood/projects/<name>/entry.amx --time 1.0 --output /tmp/dogfood.png
   ```
   Legacy projects that already carry a `verify.txt` may still run the content
   checks as an optional regression floor:
   ```bash
   cargo run --bin animatix -- verify dogfood/projects/<name>/entry.amx
   ```
4. When the grammar blocks the intent, create a minimal probe under
   `probes/`. The probe should reproduce the gap with the smallest possible
   `.amx`, not with the whole project.
5. For comparative evaluation across multiple candidate implementations, stage an
   arena battle under `arena/battles/<slug>/`, then launch the Web Arena:
   ```bash
   python3 scripts/arena.py stage <slug> --brief "..." --variants a.amx b.amx
   python3 scripts/arena.py serve
   ```
   Review the candidates with synchronized timeline scrub, slow-motion stepping,
   live `applySource` scratchpads, and rubric scoring at `http://127.0.0.1:8124/arena/?battle=<slug>`.
   Decisions are saved to `feedback.json` and aggregated into `arena/feedback.jsonl`.
6. Record workarounds and impact in `projects/<name>/notes.md`. Ingest arena outcomes
   using `python3 scripts/arena.py ingest <slug>` to drive compiler improvements,
   new primitives, and agent prompt refinements.

## Arena Battle Checklist

A new battle must satisfy:

- One focused design or aesthetic question.
- At least two `.amx` variants targeting the same brief.
- Every variant passes `animatix check`.
- `battle.json` records title, brief, tags, and candidate metadata.
- Feedback records winning variant, 4-axis rubric scores, and timeline-anchored notes.

See `arena/README.md` for the full agent workflow, Web Arena controls, and feedback ingestion schema.

## Visual Verification & Content Checks

`check`/`lint` only see the build model, and a whole-frame render smoke stays
green while a single actor silently disappears — `examples/data/07_plots.amx`
shipped with an invisible headline curve through both gates.

### 1. Primary Automatic Review (`animatix review`)

`review` is the default automatic gate for new projects and does not need a
`verify.txt`:

```bash
cargo run --bin animatix -- review dogfood/projects/<name>/entry.amx
```

It builds the target, evaluates sampled frames on the CPU, and applies the
structural rules (hidden/never-revealed actors, clipping, text overflow,
independent overlap candidates, entrance visibility, redundant keyframes).
When an offscreen GPU adapter is available, `--render auto` (the default) adds
whole-frame ink, per-actor ink, beat-change, and transition-endpoint checks.
Use `--format json` for the stable `schema_version: 1` report, `--policy site`
for the resting-composition policy, and `--storyboard` when both the automatic
report and a visual contact sheet are wanted. `review` exits 1 on an issue that
reaches `--fail-on` and 2 when the review cannot run.

### 2. Qualitative Visual Review (`animatix storyboard`)

For both AI agents and human authors, the qualitative visual inspection tool is
the storyboard contact sheet:

```bash
cargo run --bin animatix -- storyboard dogfood/projects/<name>/entry.amx
```

This generates a multi-panel grid image (`<name>_storyboard.png`) sampling $t=0$,
each keyframe transition beat, and the final resting frame in a single command.
Inspect the contact sheet to verify:
- Actors appear cleanly at their cues.
- Layouts and labels do not collide or overflow scene boundaries.
- The resting frame holds a finished, readable figure (not an empty plate).

### 3. Optional Legacy Regression Floors (`verify.txt`)

`animatix verify` remains as an internal pixel-regression escape hatch for
projects that already carry a companion `verify.txt`; it is not required for new
projects and is not the primary authoring gate:

```bash
cargo run --bin animatix -- verify dogfood/projects/<name>/entry.amx
```

It renders the frame(s) at the named times and prints one PASS/FAIL per line.
The file is line-based: one check per line, `#` comments and blank lines
ignored, times are **global** composition times (see `animatix timeline`).

`verify.txt` is an **optional legacy regression floor**, not a mandatory
prerequisite for authoring new scenes. Existing projects that already carry one
keep working unchanged. It is most useful for calibration against tricky GPU
boundaries like `effects-wave1`:
`dogfood/projects/effects-wave1/verify.txt` is the calibration example for the
hard case: a full-canvas `Filter` scope whose bounds contain every other actor.
`visible` and `reveals` on such a scope prove nothing — its region changes
whenever any child moves, and it has ink whenever anything draws. Assert a
whole-frame `ink` floor instead (that project uses 35–50% against a measured
94–96%), which collapses to roughly the card's area alone when the scope loses
its composite.

| Check | Meaning |
|---|---|
| `visible <t> <label>` | the actor put ink inside its evaluated bounds at `t` |
| `invisible <t> <label>` | it did not |
| `reveals <label> <t_before> <t_after> [min_fraction]` | the actor's bounds region changed between the two times — the robust "did it appear?" check |
| `differs <t1> <t2> [min_fraction]` | the two frames differ (the animation actually animates) |
| `ink <t> <min_fraction>` | the whole frame is not blank |

Thresholds default to 0.01 (1%) for `reveals` and 0.001 for `differs`.

**Choosing the right check.** `visible` is cheap but weak when the actor's
bounds contain other painted content: a `Graph` paints its generated axes inside
the hosted curve's bounding box, so `visible <curve>` passes even with the curve
missing. Use `reveals <curve> <hidden_t> <shown_t>` for that class — it measures
the change in the actor's region, so a curve that never appears fails. A single
actor's footprint can also be too small to threshold (a high-degree partial sum
overlaps its siblings almost everywhere); use a scene-level `differs` there.

Run `animatix review` on every project and keep the `check` warning allowlist.
The local script below covers `check` and legacy `verify` only — it does not
invoke `review` yet:

```bash
bash scripts/dogfood-verify.sh
```

This is intentionally local-only, not CI: dogfood content tracks the language as
it changes and the render checks need a GPU.

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
- Always run `bash scripts/check_examples.sh` before promoting a project to `examples/`.
