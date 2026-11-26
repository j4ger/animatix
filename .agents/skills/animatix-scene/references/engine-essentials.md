# Engine essentials
Match the installed engine spec and validate these conventions with a runnable sample before using unfamiliar syntax.

- Config duration is bare seconds (`duration: 5`); markers and brackets take time literals (`#1s`, `[1.5s]`). Declare opening paint at `#0s` or use explicit visibility in setup. A timed `.at` assignment names destination and duration at the **start** of travel.
- Default top-level Rect/Ellipse placement is centered. For anchors, transformed positions, moving paint or links, see [geometry and clearance](geometry-and-clearance.md).
- For a number whose digits change width, reserve a value field and anchor the unit separately. Check the widest and narrowest poses on actual frames. For small delivery views, check the downsized render and simplify labels before shrinking them.
- If imagery materially helps, use a supplied/suitable image or generate one when a tool is available. Keep it local and relative; keep occlusion, causal state, and live readouts editable. Inspect actual contrast and compositing. If the image/tool/target format is unavailable, retain the same inference with native DSL and no broken reference.
