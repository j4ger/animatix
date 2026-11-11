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
   New projects can also run the strict zero-config verifier, which adds
   draw/ink/reveal/rest checks and needs no sidecar:
   ```bash
   cargo run --bin animatix -- verify dogfood/projects/<name>/entry.amx --render never
   ```
   Legacy projects that already carry a `verify.txt` still run the content
   checks; use `--auto` to force the new mode when a sidecar should be ignored.
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

Animatix tooling provides a four-step cycle for authoring robust content: `check` → `audit` → `storyboard` → `baseline`.

### 1. Iterative Assistance (`animatix check` / `lint`)
Run `check` and `lint` to ensure the scene parses, types correctly, and conforms to static semantics. These are iterative helpers and do not gate final output.

```bash
cargo run --bin animatix -- check dogfood/projects/<name>/entry.amx
cargo run --bin animatix -- lint dogfood/projects/<name>/entry.amx
```

### 2. Automatic Review (`animatix audit`)
`audit` is the primary automatic gate, replacing the legacy `review` and `verify` commands. It checks layout structure, actor visibility, typography, and timing.

```bash
cargo run --bin animatix -- audit dogfood/projects/<name>/entry.amx
```

It builds the target, evaluates sampled frames, and reports findings. Use `--brief` for compact JSON and `--policy site` for embedded scene composition. Findings include `fix_hint` to simplify remediation. See [`docs/audit.md`](../docs/audit.md) for the JSON schema and rule details.

### 3. Visual Confirmation (`animatix storyboard`)
After `audit` results are clean, inspect the scene qualitatively:

```bash
cargo run --bin animatix -- storyboard dogfood/projects/<name>/entry.amx
```

This generates a contact sheet showing cues and the final resting frame. Use it to confirm that overlaps, clippings, and compositions are intentionally designed. If visual issues exist, fix the `.amx` or add an exemption marker; do not guess.

### 4. Regression Testing (`animatix baseline`)
Use `baseline` to establish a regression floor for visual output:

```bash
cargo run --bin animatix -- baseline dogfood/projects/<name>/entry.amx
```

This generates PNG snapshots. Only update snapshots (`--update`) after human review confirms that diffs are intentional design changes.

The following script runs checks across dogfood projects, including the `baseline` regression floor:

```bash
bash scripts/dogfood-verify.sh
```

This script is local-only as it requires GPU-accelerated rendering.


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
