# Review, Storyboard, and Verification

`animatix review` is the author-facing automatic review command. It builds the
target, evaluates sampled frames on the CPU, and reports layout, visibility,
and timing issues as a stable text or JSON report. It needs **no `verify.txt`**
and no GPU: the structural tier always runs, and the optional pixel tier is
used when an offscreen adapter is available.

## Command roles

The four review-adjacent commands answer different questions.

| Command | Question it answers | Requirements | Role |
|---------|--------------------|--------------|------|
| `animatix check <file>` | Does the source build, type-check, and produce expected diagnostics? | None | Fast build gate |
| `animatix review <file>` | Are there structural layout, visibility, or timing defects? | No `verify.txt`; GPU optional | **Default automatic authoring gate** |
| `animatix storyboard <file>` | What does the animation look like across its beats and resting frame? | GPU | Qualitative visual review |
| `animatix verify <file>` | Does the scene satisfy the strict visibility, draw, ink, and resting-composition contract? | No `verify.txt`; GPU optional (`--render auto`) | **Strict gate for generated scenes**; legacy `verify.txt` still supported |

`check` exits 0 with warnings, and 1 when an error diagnostic exists.
`storyboard` exits 1 when it cannot render. `verify` uses a three-way auto-mode policy: 0 pass/warn, 1 verification failure, 2 cannot run; legacy checks mode keeps its historical exit behavior.
`review` has the three-way exit policy described below.

## `animatix review`

```bash
animatix review examples/projects/fft_explain.amx
animatix review examples/projects/fft_explain.amx --format json
animatix review examples/projects/fft_explain.amx --policy site --fail-on warning
animatix review examples/projects/fft_explain.amx --storyboard /tmp/board.png
```

### Flags

| Flag | Default | Meaning |
|------|---------|---------|
| `--format text\|json` | `text` | Human-readable or stable JSON report |
| `--policy default\|site` | `default` | Rule profile; `site` adds resting-composition checks mirroring `scripts/site-rest-check.py` |
| `--samples auto\|keyframes\|N` | `auto` | Storyboard-style beats, every keyframe plus start/rest, or `N` evenly spaced times (at least 2) |
| `--max-frames N` | `24` | Upper bound on the automatic/regular sample grid |
| `--render auto\|always\|never` | `auto` | `auto` skips the pixel tier when no adapter exists; `always` makes adapter/frame failure a cannot-run error; `never` is structural-only |
| `--detail summary\|full` | `summary` | Text verbosity; JSON is always complete |
| `--fail-on error\|warning\|never` | `error` | Which issue severities produce exit 1; cannot-run still exits 2 |
| `--storyboard <path>` | — | Also render a storyboard PNG after the review |
| `--frames-dir <dir>` | — | Save rendered review PNG samples as `review_NNNN_<time>s.png` |
| `--emit-verify <path>` | — | Write suggested `verify.txt` lines from measured facts; refuses to overwrite an existing file |
| `--set NAME=VALUE` | — | Build-time variable override (repeatable) |
| `--plugin <path>` | — | Load an extension manifest or native plugin (repeatable) |

`--detail` only shortens the text report; `--format json` consumers always get
every frame and issue.

### JSON schema

The report is `schema_version: 1`:

```json
{
  "schema_version": 1,
  "input": "examples/projects/fft_explain.amx",
  "target": {
    "kind": "single_scene",
    "duration_s": 24.0,
    "resolution": [1280, 720]
  },
  "render": {
    "status": "rendered",
    "reason": "rendered 12 sample(s)"
  },
  "sample_times_s": [0.0, 1.5, 3.0, 24.0],
  "frames": [
    {
      "time_s": 1.5,
      "scene": "main",
      "ink_fraction": 0.043,
      "cast": 5
    }
  ],
  "issues": [
    {
      "rule_id": "actor.clipped",
      "severity": "warning",
      "confidence": "high",
      "time_s": 1.5,
      "actor": "caption",
      "source_line": 42,
      "message": "`caption` is 92.3% on frame at 1.50s (7.7% of its bounds is clipped).",
      "evidence": {
        "bounds": [100.0, 650.0, 1300.0, 700.0],
        "canvas": [1280, 720],
        "on_frame_fraction": 0.923
      },
      "suggestion": "Move or resize the actor so its bounds stay inside the canvas."
    }
  ],
  "summary": {
    "errors": 0,
    "warnings": 1,
    "infos": 0,
    "skipped": 0
  }
}
```

Top-level keys are stable. `render.status` is `rendered`, `skipped`, or
`failed`. `frames[].ink_fraction` is `null` when the render tier did not measure
that sample. `summary.skipped` counts rule checks that could not run because a
tier was unavailable.

### Rules

Eleven rules run under `--policy default`; two more run under `--policy site`.
Rules that need pixels are skipped when the render tier is not `rendered`.

| Rule id | Severity | Confidence | Tier | Catches |
|---------|----------|------------|------|---------|
| `build.never-revealed` | warning | high | structural | A build `never-revealed` diagnostic: an actor declared before the first keyframe with nothing lifting its hidden seed |
| `actor.opacity-never-positive` | warning | high | structural | A visual actor whose effective opacity never exceeds 0.001 across sampled frames |
| `actor.never-on-frame` | warning | high | structural | An evaluated, visible actor whose bounds never intersect the canvas |
| `actor.clipped` | warning for text/plot, info otherwise | high | structural | An actor is partially outside the canvas at a sample |
| `overlap.candidates` | info | medium | structural | Independent content actors overlap outside Stack/Group/effect-scope/callout/legend relationships |
| `text.overflow` | warning for `overflow: visible`, info otherwise | high | structural | Measured text width exceeds `text_max_width` by more than 2% + 1px |
| `entrance.not-visible` | warning | high | structural | An entrance action ends and its target is still not visible |
| `keyframe.redundant` | info | high | structural | A keyframe repeats the previous value for a property |
| `frame.blank` | warning | high | render | A rendered frame has less than 0.1% whole-frame ink |
| `beat.static` | info | medium | render | Consecutive samples more than 50 ms apart change less than 0.05% of pixels (skipped when modifier programs are present) |
| `transition.endpoint` | warning | medium | render | A transition reaches a blank frame at an endpoint |
| `rest.ink-floor` | warning | high | render (`site`) | The held frame is essentially blank |
| `rest.cast-erased` | warning | high | render (`site`) | The held frame keeps less than 35% of the plateau's ink — a closing beat erases the cast |

### Exit codes

| Code | Meaning |
|------|---------|
| `0` | No issue reached the `--fail-on` threshold |
| `1` | At least one issue reached the threshold (`error` by default; `warning` when selected) |
| `2` | The review could not run: build error diagnostics, `--render always` without a working adapter, or a failed `--storyboard` / `--emit-verify` write |

`--fail-on never` suppresses exit 1 for issues only; a cannot-run review still
exits 2.

## `animatix storyboard`

Storyboard rendering is the qualitative counterpart to `review`: it produces a
single contact-sheet image of the keyframe beats and the resting frame for a
human or multimodal model to inspect.

```bash
animatix storyboard examples/projects/fft_explain.amx -o /tmp/storyboard.png
```

`--times`, `--max-frames`, `--cols`, `--tile-width`, and `--tile-height`
control the sampled times and grid. Use it for cues, overlaps, clipping, and
the resting composition; it does not produce a machine-checkable issue list.

## `animatix verify`

`verify` is the **strict contract gate** for generated scenes. It needs no
`verify.txt` and no GPU: by default it derives the contract from the built
scene and evaluates structural rules, then adds pixel rules when an offscreen
adapter is available.

```bash
animatix verify scene.amx
animatix verify scene.amx --format json --render never
animatix verify scene.amx --auto --checks legacy.verify.txt   # rejected: choose one mode
animatix verify scene.amx --checks legacy.verify.txt          # legacy pixel checks
```

Mode selection:

- No `--checks` and no `verify.txt` beside the input: zero-config **auto**.
- A `verify.txt` beside the input: legacy **checks** mode, preserved for the
  existing transformer corpus and dogfood projects.
- `--auto` forces auto mode even when an old sidecar exists.
- `--checks PATH` explicitly selects the legacy checker.

Auto mode derives the usable contract from the scene itself: actors that
should be visible, their reveal windows, draw presence, per-actor ink, resting
composition, and dead beats. It reports a compact issue list instead of a
pass/fail-only check, so an LLM can act on the result. The primary rule ids are
`build.error`, `build.never-revealed`, `actor.opacity-never-positive`,
`actor.never-on-frame`, `actor.draw-missing`, `actor.ink-missing`,
`actor.reveal-missing`, `actor.clipped`, `text.overflow`, `beat.static`,
`frame.blank`, `rest.ink-floor`, `rest.cast-erased`, `transition.endpoint`, and
`scene.duration`.

### Flags

| Flag | Default | Meaning |
|------|---------|---------|
| `--auto` | — | Force auto mode even if `verify.txt` exists |
| `--checks <path>` | `verify.txt` beside input, if present | Legacy line-based checks |
| `--render auto\|always\|never` | `auto` | Pixel tier; `never` is structural-only, `always` treats a missing adapter as cannot-run |
| `--max-frames N` | `32` | Structural/render sample cap |
| `--max-issues N` | `20` | Cap on retained issues; errors are never dropped |
| `--fail-on error\|warning\|never` | `error` | Which severities produce exit 1; cannot-run still exits 2 |
| `--detail summary\|full` | `summary` | Text verbosity; full adds fix snippets and suggestions |
| `--brief` | — | Compact JSON for an LLM repair loop (`kind: "verify.brief"`); ignored in text mode |
| `--frames-dir <dir>` | — | Save rendered issue frames |
| `--format text\|json` | `text` | Text report or stable JSON (`schema_version: 1`, `kind: "verify"`) |
| `--width`, `--height` | config resolution, else `1280x720` | Canvas override |
| `--set NAME=VALUE` | — | Build-time variable override (repeatable) |

Exit codes are three-way in auto mode: **0** pass or warnings below the
threshold, **1** verification failure (build/source error or a failed rule),
and **2** the verifier could not run (missing input, plugin failure, or
`--render always` without an adapter). Legacy checks mode keeps its historical
exit behavior: 1 for a failed check or a renderer failure.

### JSON contract

Auto mode emits one parseable document even for build errors:

```json
{
  "schema_version": 1,
  "kind": "verify",
  "input": "scene.amx",
  "mode": "auto",
  "verdict": "fail",
  "target": { "kind": "single_scene", "duration_s": 2.0, "resolution": [1280, 720] },
  "render": { "status": "skipped", "reason": "--render never" },
  "coverage": { "structural": "complete", "render": "skipped", "complete": false },
  "contract": { "scenes": 1, "actors": 4, "visual_actors": 4, "visible_windows": 4, "beats": 3 },
  "issues": [
    {
      "rule_id": "build.never-revealed",
      "severity": "error",
      "confidence": "high",
      "scene": "main",
      "time_s": null,
      "actor": "title",
      "source_line": 3,
      "message": "Actor `title` ... remains invisible throughout playback.",
      "evidence": { "diagnostic_code": "never-revealed" },
      "fix": { "kind": "add-entrance", "target": "title", "source_line": 3, "snippet": "fade-in title [600ms]" },
      "suggestion": "Add an entrance action ..."
    }
  ],
  "summary": { "errors": 1, "warnings": 0, "infos": 0, "issues_omitted": 0 }
}
```

`coverage.complete` is false whenever the render tier was skipped. The issue
list is capped by `--max-issues`, but errors bypass the cap. Legacy checks mode
keeps its original `verify: N of M checks passed` text output for
`scripts/transformer-verify.sh` and `scripts/dogfood-verify.sh`.

## Limitations

- **No per-actor isolated render.** The pixel tier measures ink inside an
  actor's bounds on the composed frame, so overlapping actors contribute to
  each other's measurement. `overlap.candidates` is therefore an informational
  candidate, not proof of a collision.
- **Component instances may lack observable bounds.** When an expanded
  component instance has no `precise_bounds` entry and no fallback, bounds-based
  rules (`actor.clipped`, `actor.never-on-frame`, `text.overflow`,
  `overlap.candidates`) skip it. Opacity rules still see it.
- **No GPU means the render tier is skipped.** Under `--render auto`, a machine
  without a working offscreen adapter reports `render.status: "skipped"` and
  skips `frame.blank`, `beat.static`, `transition.endpoint`, and the site resting
  rules; structural rules still run. Use `--render never` to make that
  structural-only mode explicit, or `--render always` to fail loudly when
  pixels are required.
- **Structural evaluation has no filter backend.** `SceneProgram`s are
  evaluated without GPU effects, so structural facts describe the unfiltered
  content. Effect appearance and filter-only defects require the render tier.
- **`review` is heuristic; `verify` is the strict gate.** `review` samples beats
  and reports evidence for authoring. `verify` applies the generated-scene
  contract and, when a project carries exact-frame `verify.txt` checks, keeps
  the legacy pixel checker as a separate strict mode.
