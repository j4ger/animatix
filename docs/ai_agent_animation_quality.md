# AI Agent Animation Quality

Status: design proposal and bring-up plan.

## Purpose

Animatix should eventually let an AI agent write high-quality animations with
either a cloud model or a fine-tuned on-device model. The current blocker is
not generation alone; it is review and iteration. Models struggle to evaluate
their own rendered output, and many quality signals such as layout balance,
transition timing, and readability are subjective.

This document defines a pragmatic bring-up path that does not require a small
model, a mature metric suite, or a polished demo to start. The plan is to build
a deterministic evaluator first, collect human review data second, and attach
models last.

## Design Principles

1. **Facts before verdicts.** The evaluator emits measurable scene facts. Rules
   and critics decide whether a fact is a problem.
2. **Intent before overlap rules.** Geometry such as overlap is never a
   universal violation. The evaluator must know whether elements are in the
   same Stack, belong to an annotation, or overlap during a transition.
3. **Independent reviewers.** The generator should not be its own final critic.
   A separate rule engine, human reviewer, cloud critic, or edge critic should
   produce the same structured feedback.
4. **One review protocol.** Human comments, rule issues, cloud critiques, and
   edge critiques all use the same schema so the agent loop is model-agnostic.
5. **Deterministic first.** The renderer and timeline are already deterministic.
   Every review signal must be reproducible from source, time, and policy.

## Scope

In scope:

- A scene facts exporter built on the existing timeline and renderer.
- A minimal rule checker that catches obvious layout, overlap, timing, and
  visibility defects.
- A demo and data-collection workflow using existing dogfood review tooling.
- An agent iteration loop that works with no model, then with a cloud critic,
  then with an edge critic.
- A preference dataset that can later train a reward model or critic model.

Out of scope for the first bring-up:

- A universal aesthetic score.
- Automatic generation of polished animation from a brief.
- A complete taxonomy of visual taste.
- Training a generator model before the review protocol is stable.

## Architecture

```text
.amx source
   |
   v
parse + build
   |
   +------------------+
   |                  |
   v                  v
Timeline          Composition
   |                  |
   +--------+---------+
            |
            v
   Scene Facts Extractor
            |
            +----------------------+
            |                      |
            v                      v
   Storyboard frames          facts.json
            |                      |
            +----------+-----------+
                       |
                       v
             Rule Engine (deterministic)
                       |
                       v
                 Review Report
                       |
            +----------+-----------+
            |          |           |
            v          v           v
         human      cloud      edge
         review     critic     critic
            |          |           |
            +----------+-----------+
                       |
                       v
              agent edits source
                       |
                       +--> re-render and re-review
```

## Existing Building Blocks

| Component | Current capability | Use in this plan |
|---|---|---|
| `Timeline` / `Composition` | deterministic per-frame evaluation | source of timing and actor state |
| `OffscreenRenderer` | renders exact frames | storyboard generation |
| `DebugRenderOptions` | debug bounds, layout debug, hit regions | actor geometry extraction |
| `Timeline::hit_regions` | world-space AABBs per actor | overlap and layout facts |
| `DocumentSession::timeline_index` | source line to keyframe time | issue-to-source mapping |
| `animatix-cli image` | single-frame export | scripted frame extraction |
| review GUI | A/B playback and comments | human feedback collection |
| `review.json` | variant, time, severity, note | structured review corpus |
| `dogfood/runs` | focused A/B experiments | data collection and calibration |

## Data Protocol

### Scene Facts

The evaluator must separate measured facts from judgments. A fact is a
snapshot of the animation at a given time.

```json
{
  "schema_version": 1,
  "source": "examples/projects/gradient_descent.amx",
  "scene_name": "LossSurface",
  "time_ms": 1200,
  "resolution": [1280, 720],
  "image": "storyboard/LossSurface_1200.png",
  "actors": [
    {
      "label": "step_title",
      "kind": "Text",
      "bounds": [90, 60, 450, 115],
      "opacity": 0.42,
      "visible": true,
      "parent": null,
      "container_role": "content",
      "source_line": 12,
      "active_actions": ["fade-in"]
    }
  ],
  "layout": {
    "containers": [],
    "overflow": []
  },
  "overlaps": [],
  "motion": [],
  "timing": {
    "keyframes": [],
    "active_transition": null
  }
}
```

### Overlap Events

Overlap is a fact with context, not a verdict.

```json
{
  "pair": ["title", "badge"],
  "area": 1200,
  "ratio_a": 0.08,
  "ratio_b": 0.45,
  "relation": "same_stack",
  "roles": ["content", "decoration"],
  "opacity": [1.0, 0.9],
  "during_transition": false,
  "suggested_verdict": "review"
}
```

The relation and roles come from DSL structure first:

- Same `Stack` or `Group`: allowed by default.
- `Callout` with its target: allowed annotation overlap.
- `Legend` with scene content: allowed if the legend does not hide critical data.
- Crossfade or morph transition: allowed while the transition is active.
- Background or decoration behind content: allowed, but readability is checked.
- Two independent content actors: default high-interest overlap.
- Text over text: default high-interest overlap.

### Rule Issues

```json
{
  "source": "rule",
  "rule_id": "layout.content_overflow",
  "severity": "major",
  "time_ms": 1200,
  "actor": "row",
  "category": "layout",
  "note": "Row content extends beyond the scene bounds.",
  "evidence": {
    "container_bounds": [0, 0, 1280, 720],
    "content_bounds": [-40, 100, 1320, 500]
  }
}
```

### Critic Issues

Cloud and edge critics use the same issue shape as rules, with an additional
source identifier.

```json
{
  "source": "cloud-critic",
  "critic_model": "gpt-...",
  "category": "timing",
  "severity": "minor",
  "time_ms": 2200,
  "actor": "legend",
  "note": "The transition feels abrupt because the legend appears before the eye has settled on the new scene.",
  "suggestion": "Delay legend entry by 200ms or extend the scene transition to 500ms.",
  "evidence": {
    "transition_ms": 180,
    "new_content_ratio": 0.87
  }
}
```

### Review Comments

The existing human review comment format is extended without breaking old
files:

```json
{
  "id": "1234-0",
  "source": "human",
  "variant": "a",
  "time_ms": 1200,
  "severity": "major",
  "category": "layout",
  "actor": "legend",
  "note": "Legend overlaps the second curve.",
  "suggestion": "Move legend to the lower-right or add a background panel."
}
```

## Metric Taxonomy

Metrics are split into three layers. Only the first layer is universal.

| Layer | Examples | Use |
|---|---|---|
| Universal facts | bounds, opacity, keyframe times, transition duration, overlap area | evidence and debugging |
| Deterministic rules | render failure, hidden final actor, content overflow, independent-content overlap, blank transition | first-pass agent review |
| Learned judgments | visual hierarchy, reading order, transition comfort, visual balance | cloud critic, then edge critic |

No fixed rule should claim to measure "taste". Rules detect likely defects;
critics judge intent and context; humans provide the final preference signal.

### Initial Rule Set

The first rule set should be small and high-confidence.

1. Render diagnostics or build failures.
2. Final frame has actors still invisible without an intentional exit.
3. Frame is blank or all content is hidden.
4. Content extends outside the scene or its parent container.
5. Text overflows its container.
6. Independent content actors overlap outside Stack/Group/annotation/transition.
7. An actor jumps position or opacity by more than a policy-defined threshold.
8. A transition is shorter or longer than the project policy range.
9. Too many distinct actions start in the same frame.
10. An actor appears, moves, and disappears without serving a visible purpose.

The first version may implement only rules 1-6. Rules 7-10 can start as
informational warnings.

## Policy Configuration

A project-level `review.toml` configures tolerances without changing the
animation language.

```toml
policy_version = 1

[layout]
min_scene_margin = 24
allow_overflow = false

[overlap]
allow = [
  ["badge", "title"],
  ["callout", "chart"]
]
check_text_over_text = true

[timing]
min_transition_ms = 250
max_transition_ms = 700
min_action_gap_ms = 80
max_simultaneous_actions = 4

[roles]
background = ["backdrop", "grid"]
decoration = ["accent_bar"]
```

The default policy is derived from Animatix semantics and curated examples.
Most projects should not need a policy file. The file exists for intentional
exceptions such as generative art or layered abstract animations.

## Agent Loop

The agent loop is the same regardless of whether the reviewer is a human, a
rule engine, a cloud model, or an edge model.

```text
1. Read brief.
2. Generate or select one or more candidate .amx files.
3. Run eval: render storyboard, emit facts, run rules.
4. Ask reviewers to produce structured issues.
5. Rank candidates and choose one to edit.
6. Map issues to source lines and edit the .amx file.
7. Re-run eval.
8. Compare before/after reports.
9. Stop when hard checks pass and review improvements plateau.
10. If needed, send the top candidate to a human A/B review.
```

Stop conditions are external, not model confidence. The loop must keep a
history file so the agent can detect oscillation and avoid repeating edits.

## Implementation Phases

### Phase 0: Review Protocol and Evaluator Skeleton

Goal: add a deterministic `animatix eval` command that produces frames and
facts.

Tasks:

- Add `crates/animatix-review` to the workspace.
- Define `SceneFacts`, `FrameFact`, `ActorFact`, `OverlapEvent`, `RuleIssue`,
  `ReviewReport`, and `ReviewPolicy` with serde.
- Add `eval` subcommand to `animatix-cli`.
- Reuse existing renderer and timeline APIs to extract actor bounds and layout
  information.
- Render storyboard frames at user-selected times.
- Write facts as JSON alongside the storyboard.
- Add unit tests for schema serialization and facts from a small scene.
- Add a regression test that runs `eval` on at least one curated example.

Exit criteria:

- `cargo run --bin animatix -- eval examples/animation/16_showcase.amx --times 0.2,1.0,2.2 --json report.json --storyboard frames/` works.
- The report contains actor bounds, opacity, keyframe times, and overlap events.
- No model or external service is required.

### Phase 1: Minimal Rule Engine and Calibration

Goal: make the evaluator useful as a first-pass reviewer.

Tasks:

- Implement rules 1-6 from the initial rule set.
- Implement `review.toml` policy loading and defaults.
- Build a known-good corpus from curated `examples/` and `dogfood/projects/`.
- Run the rule engine over the corpus and tune defaults to avoid obvious false
  positives.
- Add special cases for Stack, Group, Callout, Legend, background content, and
  transition overlap.
- Create an intentionally broken variant to verify the rules catch seeded
  defects.
- Emit a `ReviewReport` with both facts and issues.

Exit criteria:

- Known-good examples produce no hard errors under the default policy.
- A seeded bad variant catches at least five distinct issue categories.
- Issues include time, actor, category, severity, and evidence.

### Phase 2: Demo and Human Review Data

Goal: demonstrate the full review loop with a human in the loop and begin
collecting preference data.

Tasks:

- Create a quality bring-up run under `dogfood/runs/` with `good.amx` and
  `bad.amx`.
- Render storyboards for both variants.
- Generate eval reports for both variants.
- Use the existing review GUI for A/B comparison.
- Extend review comments with `source`, `category`, `actor`, and `suggestion`.
- Render and save frames at the exact time of each human comment.
- Write a short run summary that records which rule issues were real, which
  were false positives, and which required human taste.

Exit criteria:

- A reviewer can compare good and bad variants, see the report, and leave
  structured comments.
- The run directory contains source, frames, facts, reports, comments, and a
  decision.
- At least one human comment is tied to a specific actor and time.

### Phase 3: Agent Handoff Without a Model

Goal: prove that an agent can iterate using facts and rule issues alone.

Tasks:

- Define an agent loop script or documented workflow.
- Add a helper that converts `ReviewReport` issues into source edits or edit
  suggestions.
- Use `DocumentSession::timeline_index` and existing source-edit modules to
  map actor/time issues to source lines.
- Create one brief where the agent starts from a broken candidate and reaches a
  passing candidate.
- Record the before/after diff and re-eval report.

Exit criteria:

- The agent can consume `report.json`, make source edits, re-run eval, and show
  improvement.
- The loop history is stored as files, so it can be replayed or audited.
- No model is required for this phase.

### Phase 4: Cloud Critic Adapter

Goal: allow an external model to act as a reviewer through the same protocol.

Tasks:

- Define `CriticProvider` as a trait or CLI contract.
- Implement a cloud critic adapter that accepts brief text, frame paths,
  facts, and the current report.
- Require the adapter to return `CriticIssue[]`, not free-form prose.
- Require every issue to reference either a time, actor, or source line.
- Add a second-opinion mode where deterministic rules and the cloud critic are
  both included in the report.

Exit criteria:

- A cloud model can add structured timing, hierarchy, and layout comments to a
  review run.
- The agent loop can process cloud issues with the same code path as rule
  issues.
- Cloud issues that contradict deterministic facts can be rejected or demoted.

### Phase 5: Preference Dataset

Goal: turn review sessions into reusable training data.

Tasks:

- Export review sessions to JSONL.
- For each human comment, include source, facts at the comment time, frame
  path, category, severity, and optional suggestion.
- For A/B reviews, record pairwise preference: A better, B better, or tie.
- Keep the exact `.amx` version and render policy with each record.
- Add a validation script that checks every record can be re-rendered.

Exit criteria:

- A review run produces a reproducible dataset record.
- The dataset contains enough context to train a critic or reward model later.
- No model training is required in this phase.

### Phase 6: Edge Critic

Goal: fit an on-device critic that uses facts and storyboard frames.

Tasks:

- Start from the dataset collected in Phase 5.
- Use cloud critiques as synthetic labels only where deterministic checks or
  human review validate them.
- Train the edge model to predict issue category and severity, not to generate
  animation.
- Give the model compact inputs: low-resolution storyboard frames plus scene
  facts JSON.
- Keep deterministic rules in front so the model does not need to re-derive
  geometry.
- Route ambiguous cases to cloud critic or human review.

Exit criteria:

- The edge critic accepts the same protocol as rules and cloud critic.
- Precision and recall on the seed dataset are measurable.
- The edge critic adds value beyond deterministic rules without causing
  unreliable hallucinated geometry.

## Suggested Ordering and Dependencies

```text
Phase 0: no dependency on later phases
Phase 1: depends on Phase 0
Phase 2: can begin with Phase 1 reports; full GUI integration can follow
Phase 3: depends on Phase 1 and existing source-edit tooling
Phase 4: depends on Phase 1; can run in parallel with Phase 2
Phase 5: depends on Phase 2
Phase 6: depends on Phase 5, optionally Phase 4
```

If resources are limited, the critical path is:

```text
Phase 0 -> Phase 1 -> Phase 2 -> Phase 5
```

Phase 3 can be added immediately after Phase 1 and is valuable before any model
work.

## Open Design Questions

1. Should review policy live in a separate `review.toml` or in `.amx` config?
   The plan starts with `review.toml` to avoid changing the language until the
   policy model is proven.
2. Should actor roles be inferred or explicit? The plan infers from primitive
   types and containers, then allows optional explicit roles only when needed.
3. How should z-order be represented? Declaration order and opacity are enough
   for the first version; an explicit z-order fact can be added if needed.
4. Should the first evaluator render full frames or small thumbnails? Full
   frames for humans, small frames for model input.
5. Should the agent auto-edit source in Phase 1? No. Phase 1 detects issues;
   Phase 3 introduces source editing.

## Risks

| Risk | Mitigation |
|---|---|
| Rule false positives | Calibrate against curated examples; make exceptions explicit |
| Human comments too free-form | Structured categories and optional actor/time anchors |
| Model hallucinates geometry | Pass facts to the model and reject evidence that contradicts facts |
| Edge model too weak | Restrict it to issue classification; deterministic rules handle geometry |
| Metrics overfit | Treat metrics as detectors, not quality scores |
| Agent oscillates between edits | Keep a loop history and require before/after report comparison |
| Scope grows into an animation generator | Keep generator and critic separate; bring-up targets review first |

## Relation to Existing Work

This plan extends the existing dogfood review workflow rather than replacing
it. The current `review.json`, review GUI, and `dogfood/runs` structure already
provide the human feedback channel. The evaluator adds the missing evidence
layer that makes human comments reusable by an AI agent.

See also:

- `dogfood/README.md`
- `dogfood/runs/README.md`
- `crates/animatix-gui/src/app/review/mod.rs`
- `crates/animatix/src/renderer/offscreen.rs`
- `crates/animatix/src/timeline/scene_eval.rs`
