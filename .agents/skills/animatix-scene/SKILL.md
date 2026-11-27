---
name: animatix-scene
description: Write, revise, or verify Animatix AMX scenes from a settled brief or approved storyboard. Use for DSL, reusable animated components, and visual QA of a scene.
metadata:
  version: "0.9.0-draft.44"
---

# Animatix scene

Preserve the approved claim, timing, format, and unrelated behavior. Match the target engine's spec and audit tools; start unfamiliar syntax from a compiling sample on that revision. Unsettled narration or full explanations belong to animatix-explainer.

## Choose what to reuse

Look up the element by its job before drawing it. Check the scene's imports, then the [shared layout components](../../../web/content/templates/layout.amx) and [native primitives](../../../docs/primitives.md). The layout module includes `MetricCard`, `CaptionPlate` and `SignalLane`. For small marks such as arrows and checks, use a `Path` with a [supported icon name](../../../docs/primitives.md#icon-names). [Public scenes](../../../web/content/scenes/) show the imports and styling in context.

The [component import example](assets/attached-status.amx) runs in this checkout. Resolve its import relative to the scene when moving it.

Use an existing component directly when its inputs cover the beat. Compose a one-off from primitives in the scene. When a new mechanism needs independently changing instances, build a small reusable module and probe its rest, action and result. Check that changing one instance leaves its neighbor stable before assembling the scene. Rerun the probe when the module changes.

## Stage the beat

Show one viewing task at a time. For a causal beat, show the cause before its result. Keep live labels near their subjects and allow time to read. When one element belongs to another, name the relationship first (beside a label, touching an edge, or following a port). Use a shared layout or bind their attachment points in the same coordinate space. Use the component’s geometry for alignment, the target layout for spacing and the delivery cue for timing. In reusable examples, explain the values a caller would need to choose again. Tie parts of the same event to the same state. Use a cut when interpolation would imply a state that never occurs.

When a material carries the design, choose a visible trace of how it was made. A dry brush can leave slivers of bare paper within a broad stroke. Check what the surface looks like in the whole frame at delivery size before inspecting a detail crop.

Consult [geometry and clearance](references/geometry-and-clearance.md) for attachments, moving ports or overlaps, [layout and data](references/layout-data-qa.md) for charts, readouts or states identified by color, and [typed colors](references/colors.md) for color syntax. [Engine essentials](references/engine-essentials.md) covers opening visibility, timing and optional imagery.

## Verify, then iterate

Check build, size, and duration; render the opening, the informative interior of a move, and the held result at delivery size. Verify the actual paint, including the cue that carries the claim, the unchanged reference, and any risky pass. Watch playback when available. Ask what a fresh viewer can infer before reading the intended answer.

When several elements compete for attention, shrink the frame that carries the beat and glance at it before reading the text. What catches your eye first? Try adjusting the competing elements' relative size, contrast or surrounding space. Check the change again at delivery size, with the context and labels still readable.

Test the inputs that can change an attachment: try the longest label and widest reading with its unit for text, or vary the dimensions for shapes. If the owner has a position input, render the same moment with that input visibly shifted and compare which elements followed and which stayed fixed. Otherwise, swap the owner where supported. Check spacing and ownership at delivery size.

For **a timing-sensitive state, a fragile hold, or an easily misread claim**, use the [acceptance workflow](references/acceptance-card.md). Confirm what failed on the original before editing: a cue on time with a wrong value needs a content fix, not a timing change. Keep the last verified revision; keep a new one only if its rendered evidence improves the identified issue without breaking a passing check.

Repeat only for a concrete remaining problem. Report failed or unverified behavior plainly. Build success, rendered timestamp samples, and real delivery playback are different evidence.
