# Reviewing a rendered pose or scene

Use the project's reference frame, viewing size and intended beat to choose the checks that matter.

- **Identity and material.** At the smallest intended size, can you recognize the silhouette and tell the states apart? In a close-up, look for the construction or surface detail that identifies this object. A dry brush can leave slivers of bare paper within a broad stroke. Inspect the whole image at delivery size before judging a detail crop.
- **Attention.** When several elements compete, shrink the frame and glance before reading. What catches your eye first? Adjust relative size, contrast or surrounding space and look again with the labels in place. If color carries a distinction, export the telling frame with `animatix image SCENE --time SECONDS --width PIXELS --grayscale -o gray.png` and try tracing the same subjects.
- **Motion.** Compare the start, the most revealing interior pose and the held result. Pick the interior moment from the action, such as a joint bending farthest or a moving part passing a label. Check what moves together and what stays fixed. Play at normal speed when judging pacing or reading time. Record which views or playback you actually checked.

Use the [visual QA commands](audit.md) to capture evidence. A component probe lets you inspect a joint or material in isolation. Its scene caller can then judge the same feature against the actual background, labels and camera.
