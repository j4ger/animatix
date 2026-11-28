---
name: animatix-scene
description: Write, revise, or verify Animatix AMX scenes from a settled brief or approved storyboard. Use for scene composition, cue timing, component integration and visual QA of a scene.
metadata:
  version: "0.9.0-draft.48"
---

# Animatix scene

Preserve the approved claim, timing, format, and unrelated behavior. Match the target engine's spec and audit tools; start unfamiliar syntax from a compiling sample on that revision. Unsettled narration or full explanations belong to animatix-explainer.

## Preflight visual output

In a fresh cloud or headless environment, render one frame from the checkout's
[small playback fixture](../../../web/app/public/sdk-examples/scrub.amx) before
authoring if this task needs rendered evidence. Use the normal
`animatix image FILE --time 0 -o PREVIEW.png` command and confirm the PNG exists.
If no adapter is available, check the rendering row of `animatix doctor` and see
[headless Linux setup](../../../docs/contributing.md#headless-linux-rendering).
Keep working on source when useful, but report visual verification as blocked;
a passing `check` or structure-only audit does not prove rendered output.

## Choose what to reuse

Look up the element by its job before drawing it. If the project already has a materials index or README, check it for that job and try a matching export at its existing import path. Check the scene's imports, then the [shared layout components](../../../web/content/templates/layout.amx) and [native primitives](../../../docs/primitives.md). The layout module includes `MetricCard`, `CaptionPlate` and `SignalLane`. For small marks such as arrows and checks, use a `Path` with a [supported icon name](../../../docs/primitives.md#icon-names). [Public scenes](../../../web/content/scenes/) show the imports and styling in context.

The [component import example](assets/attached-status.amx) runs in this checkout. Resolve its import relative to the scene when moving it.

Use an existing component directly when its inputs cover the beat. Compose a one-off from primitives in the scene. In either case, continue with staging here. For a new object shared across scenes, independently actuated instances, or a mechanism that needs its own visual checks, use [animatix-component](../animatix-component/SKILL.md). Pass its planned uses, delivery sizes, cues, attachments and any existing art direction or reference frame. A separate author can own it when several scenes depend on it or the mechanism merits focused work. The same author can follow the component skill inline. Agree on one project-local import path with the other callers. If another scene already owns the module, use its handoff.

Resume scene production with the returned import, controls and state references. Schedule the action, attach through the public anchors and inspect it in the actual composition. Return an interface gap to the component owner and recheck the integrated scene after the fix.

## Stage the beat

Show one viewing task at a time. For a causal beat, show the cause before its result. Keep live labels near their subjects and allow time to read. When one element belongs to another, name the relationship first (beside a label, touching an edge, or following a port). Use a shared layout or bind their attachment points in the same coordinate space. Use the component’s geometry for alignment, the target layout for spacing and the delivery cue for timing. In reusable examples, explain the values a caller would need to choose again. Tie parts of the same event to the same state. When connecting two views, run `animatix storyboard FILE --transition-from OUTGOING_SCENE`. For a timed overlap, read the middle tile at delivery size. If two captions compete there, end the old one before revealing its replacement. For changing text in a shared title, the incoming actor can use `shared_text_transition: "fade_through"`. If a mark passes a scale value, or a part appears to release or engage, check whether the story supports that reading. A cut can preserve two distinct observations.

For a timed beat, start with the instant the viewer needs the evidence. If a spoken phrase lands on the result, place the arrival there and work backward to the start of the move. If the phrase introduces an action, start it there. Use a [transcript marker or timed word](../../../docs/spec.md#4-timeline--keyframes) when available. Translate full-track times to the scene’s clock once, accounting for its cut point and the audio placement. Keep independently timed events on their own clock. For a silent sequence, try the move at delivery size. Give the viewer time to find the subject before it moves and to make the intended comparison before the next cue. On a page, let persistent text or a reader-controlled advance carry the reading time.

For a material-led scene, use the [visual review methods](../../../docs/visual-review.md) to compare the surface and silhouette at delivery size.

Consult [geometry and clearance](references/geometry-and-clearance.md) for attachments, moving ports or overlaps, [layout and data](references/layout-data-qa.md) for charts, readouts or states identified by color, and [typed colors](references/colors.md) for color syntax. [Engine essentials](references/engine-essentials.md) covers opening visibility, timing and optional imagery.

## Verify, then iterate

Check build, size, and duration; render the opening, the informative interior of a move, and the held result at delivery size. Verify the actual paint, including the cue that carries the claim, the unchanged reference, and any risky pass. Watch playback when available. Ask what a fresh viewer can infer before reading the intended answer.

When several elements compete for attention, use the [attention and motion checks](../../../docs/visual-review.md) on the frame carrying the beat.

Test the inputs that can change an attachment: try the longest label and widest reading with its unit for text, or vary the dimensions for shapes. If the owner has a position input, render the same moment with that input visibly shifted and compare which elements followed and which stayed fixed. Otherwise, swap the owner where supported. Check spacing and ownership at delivery size.

For **a timing-sensitive state, a fragile hold, or an easily misread claim**, use the [acceptance workflow](references/acceptance-card.md). Confirm what failed on the original before editing: a cue on time with a wrong value needs a content fix, not a timing change. Keep the last verified revision; keep a new one only if its rendered evidence improves the identified issue without breaking a passing check.

Repeat only for a concrete remaining problem. Report failed or unverified behavior plainly. Build success, rendered timestamp samples, and real delivery playback are different evidence.
