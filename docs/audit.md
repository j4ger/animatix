# Audit and Measurement

Animatix provides a unified validation and measurement framework designed for both human authors and LLM agents. It verifies layout geometry, visual appearance, typography, and temporal continuity, replacing the legacy `review` and `verify` commands with a single deterministic pipeline.

**Core principle:** facts are derived from the scene by the tool, verdicts are produced by rules, intent is declared in the source, and regression is carried by PNG baselines. Authors and agents write `.amx` only — they never write assertion files, guess sample times, or tune ink thresholds.

## Core Roles

| Command | Purpose | Primary Flags |
|---|---|---|
| `animatix audit <file>` | Primary automatic gate: validates structure, pixels, typography, and timing | `--policy`, `--format`, `--brief`, `--render`, `--samples`, `--times`, `--max-frames`, `--fail-on`, `--detail`, `--storyboard`, `--frames-dir`, `--evidence`, `--set`, `--plugin`, `--width`, `--height` |
| `animatix measure <file>` | Pure fact query: outputs frame/actor facts as JSON with `findings: []` | `--format json` (default), `--render`, `--samples`, `--times`, `--max-frames` |
| `animatix baseline <file>` | PNG snapshot generation and regression comparison | `--update`, `--times`, `--max-frames`, `--threshold`, `--width`, `--height`, `--format`, `--set`, `--plugin` |
| `animatix storyboard <file>` | Qualitative visual evidence: keyframe beats and resting contact sheet | `-o`, `--times`, `--cols`, `--max-frames` |
| `animatix check` / `lint` | Fast syntax, typing, and static semantic checks (iterative assistance, not a final gate) | Standard compiler flags |

Exit codes across all tooling:
- `0`: Clean. No findings above the `--fail-on` threshold, or baseline matches.
- `1`: Finding detected at or above the threshold, or a baseline diff was detected.
- `2`: Cannot run (build failures, unreadable input, missing baseline, or `--render always` without an adapter).

---

## 1. `animatix audit`

The primary validation tool. It builds the target, selects deterministic sample times, and evaluates both structural properties and optional GPU-rendered pixels.

### Command-Line Flags

- `--policy default|generated|site`:
  - `default`: Relaxed tolerances for human authors.
  - `generated`: Strict gate for automated generation. Escalates `overlap.undeclared`, `actor.clipped`, `text.overflow`, and `text.occluded` to hard errors.
  - `site`: Validates embedded scenes, enforcing resting composition requirements.
- `--format text|json`: Choose between human-readable text or structured JSON (`schema_version: 2`).
- `--brief`: Emits compact JSON (`kind: "audit.brief"`) omitting raw frame dumps and full suggestion texts. Recommended for LLM loops.
- `--render auto|always|never`:
  - `auto` (default): Uses the GPU pixel tier when an offscreen adapter is available; skips it in headless/CPU environments.
  - `always`: Demands a working GPU adapter; exits 2 if unavailable.
  - `never`: Disables the pixel tier; runs structural checks only.
- `--samples auto|keyframes|N`: Time-sampling strategy. `auto` chooses structural landmarks; `keyframes` checks explicit scene keyframes; `N` evaluates a uniform grid of $N$ frames.
- `--times T1,T2,...`: Exact sample times in seconds. Overrides `--samples` when present.
- `--max-frames N`: Caps the maximum number of evaluated frames (default: 24).
- `--fail-on error|warning|never`: Threshold for returning exit code 1.
- `--detail summary|full`: Controls text output verbosity (JSON is always complete).
- `--storyboard <path>`: Also renders a storyboard contact sheet after the audit.
- `--frames-dir <dir>`: Saves individual rendered sample frames to disk.
- `--evidence <dir>`: Writes the rendered sample frames (`review_NNNN_<t>s.png`) to this directory, so each finding can be checked against the frame at its `time_s`. It does **not** produce per-finding pixel crops — the render tier does not retain RGBA buffers after measurement.
- `--set NAME=VALUE`: Overrides build-time variables (repeatable).
- `--plugin <path>`: Loads a native plugin.
- `--width <px>`, `--height <px>`: Overrides canvas resolution.

---

## 2. Schema v2 (`audit`, `measure`, `baseline`)

All structured output uses a unified format (`schema_version: 2`).

### Top-Level Object
- `schema_version`: `2` (integer).
- `kind`: `"audit"`, `"audit.brief"`, `"measure"`, or `"baseline"`.
- `input`: Source file path.
- `target`: Target description object (`kind`, `duration_s`, `resolution`).
- `render`: Render engine status (`status: "rendered"|"skipped"|"failed"`, `reason`).
- `sample_times_s`: Array of sampled timestamps in seconds.
- `frames`: Array of per-frame facts (omitted in `--brief`).
- `coverage`: Check coverage summary (`structural`, `render`, `complete`, `skipped_rules`, `checks_run`, `checks_skipped`).
- `findings`: Array of validation issues. Empty for `measure`.
- `summary`: Issue counts (`errors`, `warnings`, `infos`, `skipped`, `waived`).

### Finding Object (`findings[]`)
- `code`: Stable issue identifier string (e.g., `"overlap.undeclared"`).
- `severity`: `"error"`, `"warning"`, or `"info"`.
- `confidence`: `"high"` or `"medium"`.
- `persistence`: `"transient"` or `"held"` (settled state). Rules aggregate across sampled frames, so transient findings are **not** automatically downgraded to `info`.
- `scene`: Active scene identifier.
- `time_s`: Timestamp in seconds where the issue occurs, or `null`.
- `actor`: Target actor label, or `null`.
- `source_line`: Target source line number, or `null`.
- `message`: Human-readable description.
- `evidence`: Supporting geometric or pixel facts (see below).
- `fix_hint`: Structured suggestion for automated remediation (`kind`, `target`, `source_line`, `snippet`, `marker`), or `null`.
- `waived_by`: Exact text of the source exemption marker that matched, or `null`.
- `suggestion`: Human-readable remediation instruction.

### Evidence Details (`evidence`)
Depending on the rule, the `evidence` object provides measured values:
- `relation`: Structural relationship (`"same_stack"`, `"callout_target"`, `"legend_host"`, `"ancestor"`, `"during_transition"`, `"independent"`).
- `pair`: Array of two actor labels involved in a collision or occlusion.
- `overlap_area`: Area of bounding box overlap (pixels squared).
- `ratio_a`, `ratio_b`: Fraction of actor A or B's area that overlaps.
- `bounds_a`, `bounds_b`: Evaluated world-space bounding boxes `[x0, y0, x1, y1]`.
- `occluder`: Label of the covering actor.
- `probe_coverage`, `probed_points`, `covered_points`: Discrete sampling points for occlusion checks. Points without ink are excluded from the denominator.

### Frame Facts (`frames[]`)
- `time_s`: Timestamp of the frame.
- `scene`: Active scene name.
- `cast`: Number of active actors.
- `ink_fraction`: Whole-frame non-background pixel ratio (0..1), or `null` when the render tier was skipped.
- `changed_from_prev`: Fraction of pixels changed from the previous sample (0..1), or `null`.
- `actor_ink`: Map of `"scene:label"` to measured actor pixel coverage (0..1).
- `actors`: Array of actor states (`label`, `bounds` as `[x0,y0,x1,y1]` or `null`, `opacity`, `on_frame_fraction`, `draw_commands`).

---

## 3. Rule Catalog

### Layout and Geometry
- `overlap.undeclared`: Two independent actors overlap without an explicit structural relationship or exemption.
- `overlap.explained`: An overlap explained by DSL structure (same `Stack`, `Callout`, `Legend`, transition). Informational; never waived by a marker because it is not a defect.
- `actor.clipped`: An actor's rendered bounds extend beyond the canvas.
- `text.overflow`: Text bounds exceed the container dimensions allocated by the layout engine.
- `text.occluded`: Text is partially covered by another opaque actor.
- `text.occluded-full`: Text is entirely hidden behind another actor (non-exemptible).

### Visibility and Draw Data
- `build.never-revealed`: An actor declared before keyframe 0 lacks an entrance and remains permanently invisible.
- `build.error`: Scene fails to compile or resolve names (non-exemptible).
- `actor.opacity-never-positive`, `actor.never-on-frame`, `actor.draw-missing`, `actor.ink-missing`, `actor.reveal-missing`: Build model says an actor exists, but evaluation or pixels say otherwise.
- `frame.blank`: Entire canvas has zero ink during an active playback window (non-exemptible).

### Timing and Resting Composition
- `entrance.not-visible`, `keyframe.redundant`, `beat.static`, `transition.endpoint`, `scene.duration`.
- `rest.ink-floor`, `rest.cast-erased`: Held resting frame lost its composition (site policy).

### Marker Integrity
- `layout.marker-invalid`: Malformed exemption syntax or a marker that could not bind to an actor (non-exemptible).
- `layout.marker-abuse`: More than half of the visual actors carry waivers; blanket opt-outs hide real defects.

### Baseline
- `baseline.mismatch` is reported by `animatix baseline`, not by `audit`. It is non-exemptible: a pixel diff must be reviewed and then accepted with `--update`.

---

## 4. Source Exemptions

When an overlap or visual overflow is an intentional design choice, mark it directly in the `.amx` source. Exemptions must appear immediately before the actor declaration and must include a human-readable justification.

```animatix
// layout: allow-overlap - Badge floats over the card margin
badge: Rect, size: (40, 40)
```

### Exemption Syntax

| Marker | Exactly waives |
|---|---|
| `// layout: allow-overlap - <reason>` | `overlap.undeclared` |
| `// layout: allow-occlusion - <reason>` | `text.occluded` |
| `// layout: allow-overflow - <reason>` | `actor.clipped` and `text.overflow` |
| `// layout: allow-rest - <reason>` | `rest.ink-floor` and `rest.cast-erased` |
| `// layout: ignore - <reason>` | All layout-class findings |

### Scoping Rules
- **Subtree Propagation**: A marker applies to the declared actor and its entire descending subtree. Apply markers to the smallest containing element to limit the blast radius.
- **Structural Explanations**: Elements in the same `Stack`/`Row`/`Col`/`Grid`/`Group` container, within one `Filter`/`Glass` scope, `Callout` targets, `Legend` hosts, or elements undergoing active transitions are automatically classified as `overlap.explained` and do not require markers.
- **Compatibility**: Legacy `// rest-check: exempt - <reason>` is treated as an alias for `allow-rest`. Comments matching `// house-style: <note>` are non-binding notes and do not exempt rules.

### Non-Exemptible Codes
The following codes represent structural, compiler, or severe integrity failures and **cannot be waived** by any marker:
- `build.error`
- `build.never-revealed`
- `frame.blank`
- `text.occluded-full`
- `baseline.mismatch`
- `layout.marker-invalid`

---

## 5. Agent and Author Workflows

### Automated / LLM Workflow
1. **Write Source**: Produce `.amx` syntax. Do not author sidecar assertion files, sample-time lists, or ink thresholds.
2. **Fast Audit**: Run `animatix audit <file> --brief --format json`.
3. **Iterative Repair**: Inspect the `findings` array. Locate the code via `source_line` / `actor` and apply fixes based on `fix_hint`.
4. **Visual Inspection**: Run `animatix storyboard <file> -o board.png` and review the rendered contact sheet.
5. **Declare Intent**: Add a `// layout: ...` exemption marker *only* after confirming visually that the anomaly is intentional, then re-run `audit`.
6. **Regression Guard**: Compare with `animatix baseline <file>`. Update snapshots with `--update` only when a diff represents an intended visual update.

### Human Workflow
Use `animatix check` and `animatix lint` during active drafting. Run `animatix audit` as the definitive local gate before commits, paired with `animatix storyboard` for qualitative review. `animatix measure --format json` is the script-facing way to read frame facts without parsing text output.

---

## 6. Known Limitations

- **No Per-Actor Isolated Pass**: Ink is measured inside an actor's bounds on the composed frame, so overlapping actors contribute to each other's measurement. Occlusion uses a 27-point probe grid over the composed frame instead of a separate actor render.
- **Clip Path Approximation**: The engine does not expose effective clip paths to tooling. Probe points without ink are excluded from the denominator, which approximates clipped-to-empty regions but does not model arbitrary clip geometry.
- **Modal Colour**: The reference colour is the frame's own dominant colour; a scene where one actor covers most of the canvas can under-report that actor's ink.
- **Headless Environments**: Without a GPU/Vulkan/Metal adapter, the pixel tier is skipped under `--render auto` and `frames[].ink_fraction` is `null`. Structural rules still run. `baseline` cannot run without an adapter.
