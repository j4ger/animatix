---
name: animatix-scene
description: Write, revise, or verify Animatix AMX scenes from a settled brief or approved storyboard. Use for DSL, reusable animated components, and visual QA of a scene.
metadata:
  version: "0.9.0-draft.38"
---

# Animatix scene

Preserve the approved claim, timing, format, and unrelated behavior. Match the target engine's spec and audit tools; start unfamiliar syntax from a compiling sample on that revision. Unsettled narration or full explanations belong to animatix-explainer.

## Choose what to reuse

Look up the element by its job before drawing it. Check the scene's imports, then the [shared layout components](../../../web/content/templates/layout.amx) and [native primitives](../../../docs/primitives.md). The layout module includes `MetricCard`, `CaptionPlate` and `SignalLane`. For small marks such as arrows and checks, use a `Path` with a [supported icon name](../../../docs/primitives.md#icon-names). [Public scenes](../../../web/content/scenes/) show the imports and styling in context.

For a scene saved under `web/content/scenes/`, this is a complete example. Adjust the import relative to your own scene and include the module with the deliverable.

```amx
import "../templates/layout.amx"
config { style: "editorial", canvas_size: (1280, 720), duration: 4 }
#0s
card: MetricCard, label: "Speed", value: "24 m/s", detail: "Average", width: 420, height: 200, at: (640, 360)
```

Use an existing component directly when its inputs cover the beat. Compose a one-off from primitives in the scene. When a new mechanism needs independently changing instances, build a small reusable module and probe its rest, action and result. Check that changing one instance leaves its neighbor stable before assembling the scene. Rerun the probe when the module changes.

## Stage the beat

Show one viewing task at a time. Make the cause visible before its result, keep live labels near their subjects and give the viewer time to read. In adjacent panels, place a status mark beside the label or value it qualifies and check its association at delivery size. Tie parts of the same event to the same state. Use a cut when interpolation would imply a state that never occurs.

Consult [geometry and clearance](references/geometry-and-clearance.md) for moving ports or overlaps, [layout and data](references/layout-data-qa.md) for charts and readouts, and [typed colors](references/colors.md) for color syntax. [Engine essentials](references/engine-essentials.md) covers opening visibility, timing and optional imagery.

## Verify, then iterate

Check build, size, and duration; render the opening, the informative interior of a move, and the held result at delivery size. Verify the actual paint, including the cue that carries the claim, the unchanged reference, and any risky pass. Watch playback when available. Ask what a fresh viewer can infer before reading the intended answer.

For **a timing-sensitive state, a fragile hold, or an easily misread claim**, use the [acceptance workflow](references/acceptance-card.md). Confirm what failed on the original before editing: a cue on time with a wrong value needs a content fix, not a timing change. Keep the last verified revision; keep a new one only if its rendered evidence improves the identified issue without breaking a passing check.

Repeat only for a concrete remaining problem. Report failed or unverified behavior plainly. Build success, rendered timestamp samples, and real delivery playback are different evidence.
