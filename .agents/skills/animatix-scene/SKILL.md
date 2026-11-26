---
name: animatix-scene
description: Write, revise, or verify Animatix AMX scenes from a settled brief or approved storyboard. Use for DSL, reusable animated components, and visual QA of a scene.
metadata:
  version: "0.9.0-draft.37"
---

# Animatix scene

Preserve the approved claim, timing, format, and unrelated behavior. Match the target engine's spec and audit tools; start unfamiliar syntax from a compiling sample on that revision. Unsettled narration or full explanations belong to animatix-explainer.

## Build the visual beat

- Stage one viewing task at a time. Make the cause visible before the result and keep live labels by their subjects. Give the viewer time to read.
- Separate the true event from explanatory motion. Pace setup, travel, and hold; ease motion when it helps tracking. Use a cut if interpolation invents a state. Keep parts that represent the same event tied to the same state.
- For independently changing instances of one mechanism, first build **only** a reusable module and a small probe. Render its rest, action, and result; exercise the inputs and check that one instance doesn't move its neighbor. Assemble with that same module, rerun the probe when it changes, and verify one input-to-result path in context. One-off motion can go straight to the scene.

Read only the relevant reference: [typed colors](references/colors.md) for color syntax; [geometry and clearance](references/geometry-and-clearance.md) for nested actors, ports, and swept overlap; [layout and data](references/layout-data-qa.md) for readouts and charts. [Engine essentials](references/engine-essentials.md) covers setup syntax and optional imagery. Generated images are optional; keep changing geometry and data editable and use a native fallback when imagery is unavailable.

## Verify, then iterate

Check build, size, and duration; render the opening, the informative interior of a move, and the held result at delivery size. Verify the actual paint, including the cue that carries the claim, the unchanged reference, and any risky pass. Watch playback when available. Ask what a fresh viewer can infer before reading the intended answer.

For **a timing-sensitive state, a fragile hold, or an easily misread claim**, use the [acceptance workflow](references/acceptance-card.md). Confirm what failed on the original before editing: a cue on time with a wrong value needs a content fix, not a timing change. Keep the last verified revision; keep a new one only if its rendered evidence improves the identified issue without breaking a passing check.

Repeat only for a concrete remaining problem. Report failed or unverified behavior plainly. Build success, rendered timestamp samples, and real delivery playback are different evidence.
