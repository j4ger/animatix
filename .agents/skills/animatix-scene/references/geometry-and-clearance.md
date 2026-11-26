# Geometry and clearance

The examples behind this guidance were checked at Animatix revision `69c7795715911ff098225da4d8f7c569bcab3ca2`. Confirm syntax with the target specification and a compiling example; a revision alone isn't syntax.

## Nested or transformed placement

For uncameraed screen pixels, x increases rightward and y downward. A standard centered Rect/Ellipse for bounds `[l,r) × [t,b)` uses center `((l+r)/2,(t+b)/2)` and size `(r-l,b-t)`. For a nested or custom-anchored actor, inspect final screen-space bounds using its named instance path. Do not infer them from local coordinates. Use a shared transform or progress for attached parts. Pass a typed Color to component parameters; use `hex_color(...)` for fixed hex colors or `rgb(...)` / `rgba(...)` for computed ones.

## Protected marks near motion

Keep identifying paint outside the swept footprint, including stroke, caps, joins and decoration, or show that compositing keeps it readable without obscuring other content. For a foreground mark already needed at 0, declare it in the time-zero setup after the mover if it paints above it; do not create it later just to change draw order.

For straight monotonic travel, project the mark onto the centerline, clamp to the segment and invert the easing to estimate the closest time. Check a nearby boundary if riskier. For curves, transforms or uncertain bounds, use provisional samples/diagnostics to find risk, not as proof. Check a native crop at the actual risky pass against a clear reference crop. No structural warning is not clearance, and permission to overlap one mark doesn't extend to others.

## Connectors to nested ports

Use actor-edge anchors, e.g. `from: (source, right), to: (moving.port, left)`, not bare actor handles or guessed `.position` values. Set `stroke` explicitly. Declare a rear connector before the opaque movers and a foreground mark after them. At an interior pose, check the connector bounds track the actual nested port and verify exposed wire paint; a successful build can still render none.

Run [the small self-contained example](../assets/nested-connector.amx) with the matching engine. At 2 s the visible wire reaches the back of the moving body, the endpoint resolves to the port edge, and P paints above the body. The pinned checker can warn that this tuple expects Vec2 even when native paint resolves correctly; verify the actual renderer on the target revision.
