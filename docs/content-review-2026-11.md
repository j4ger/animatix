# Content lab: first visual review

Reviewed against `aa4d8d1`, with the experimental additions in this branch.
No existing source, URL or publication list was removed or moved.

## What the renders show

Six-frame CLI storyboards were rendered for each existing candidate. These are
sampled visual observations, not proof of every frame, audio or web behavior.

| Source | Observation | Next action |
| --- | --- | --- |
| `examples/layout/02_layout.amx` | Four small, unlabeled groups sit at the canvas edges. Useful API exercise, weak standalone teaching and showcase. | Rework as one focused layout lesson; preserve current test/discovery behavior. |
| `examples/generation/28_generation_reactive.amx` | Group, Mask, generated arrays and reactive rules compete in one composition; labels are small. | Split teaching responsibilities rather than promote this as a finished work. |
| `examples/gallery/epicycles.amx` | A real explanation exists, but thin dark strokes, small annotations and excessive empty space weaken the presentation. | Keep as source material; rewrite the focused explanation, not another public gallery entry. |
| `web/demos/gradient/scene.amx` | Four acts have coherent hierarchy and a meaningful held composition. | Keep the work; replace hand-copied assembly with a checked generator. |
| `web/demos/epicycles/scene.amx` | The figure language is more polished. Sampled act transitions show superimposed headings and footers. | Preserve generated assembly. Review crossfade timing for text readability; this is not evidence of an engine defect. |

The existing sample count is deliberately small. The inventory is comprehensive;
visual review is not. In particular, registry membership is not a quality badge.

## Two complementary prototypes

- `examples/generation/array_layout.amx`: a short teaching example. Twelve array
  actors, one Grid, animated sizes and dynamic reflow. No per-tile positions.
  Four reviewed samples at 1.5, 3.8, 6.3 and 8.8 seconds show clear spacing and
  no visible text overlap or clipping. It intentionally explains one mechanism,
  rather than competing with a narrative showcase.
- `examples/gallery/phase_interference.amx`: an experimental 26-second science work. Six rendered frames (2, 7, 11,
  17, 22, 26 seconds) verify common amplitude scale, constructive/destructive
  states, readable labels and a complete resting frame. Audit reports no
  warning/error; graph-child and dial overlaps are intentional info findings.

Neither addition is automatically published through [retired work manifest](https://github.com/j4ger/animatix/blob/530b5f8843d31a77b19540ff8f42daf662fddfff/web/works.json).

## Authoring findings

`tiles.gap = ...` is rejected as an unsupported runtime assignment even with
`dynamic_layout: true`. The new example uses supported child-size animation,
which reflows the container. This deserves clearer capability documentation,
not a workaround hidden in the example. `corner_radius` is the supported Rect
property; `radius` correctly produces a diagnostic.

The old reactive example manually expands array labels in its always block,
although current DSL supports indexed targets. Concise current idioms should be
preferred in new teaching material; historical examples should not define the
recommended authoring style by accident.

## Review recipe

```sh
python3 scripts/content-catalog.py --check
python3 scripts/content-catalog.py --output /tmp/content-inventory.json
animatix check examples/generation/array_layout.amx
animatix storyboard examples/generation/array_layout.amx \
  --times 1.5,3.8,6.3,8.8 --output /tmp/array-layout.png
animatix storyboard examples/gallery/phase_interference.amx \
  --output /tmp/phase-interference.png
```

Before publication, validate the actual web player and displayed embed size,
the final hold, repeat playback/scrubbing, and any audio. CLI image output alone
must not be described as browser, GUI or audible-sync verification.

## Still to verify

A static review of the hash work flagged potentially misleading “zero search
iterations” copy around collision-chain comparison and a modulus-dependent
collision caption. These were not rendered in this pass and remain unconfirmed;
do not silently label them fixed or remove them on this evidence.

The array audit renders 24 samples: 0 errors and 4 static-beat warnings at
intentional reading holds. Do not remove all holds simply to silence a heuristic.

## Native GUI smoke test

Built the unchanged GUI from the same base and opened the phase scene on the
cloud Linux desktop. Dragging the playhead to 10.99 s displayed cancellation;
playback advanced through 17.68 s and 24.83 s and could be paused.

Two observations need a separate GUI investigation: the preview background and
low-luminance colors appear markedly darker than CLI PNG output even with the
review grid disabled; Inspector reports local duration 0.000 s while transport
correctly displays 26 s. Playback itself works. Neither issue was fixed here.
The host uses llvmpipe software Vulkan; no hardware-GPU performance claim is made.
Web and audible audio synchronization remain unverified.
