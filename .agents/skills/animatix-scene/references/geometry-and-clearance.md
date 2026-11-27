# Geometry and clearance

## Attachments

Choose the two points that should stay together, such as a port and a wire endpoint. Express both in the same coordinate space. A shared layout can reserve a gap. For moving geometry, bind to a named point or derive both positions from the same dimension or progress. A fixed position is appropriate for a stationary subject. Choose local clearance in scene units or scale it with the design, according to the intended spacing.

- **Label and icon.** The [runnable card example](../assets/attached-status.amx) follows the label's right edge. `card_label` is a child name in this template. Prefer a documented attachment point when available and recheck child references after a template change. The built-in stroke icon uses a [24-unit grid with a top-left origin](../../../../docs/primitives.md#icon-names). Aligning its center vertically takes half its displayed grid height.
- **Shape and terminal.** For a centered circle touching a right edge from outside, shift its center right by its radius. A wire ending at a named port can use that point directly.

Text anchors follow the layout box. On this engine a string replaced inside `always` can change its painted width while keeping its authored box. Reserve space for the widest live reading and inspect the transition at delivery size.

## Nested or transformed placement

For uncameraed screen pixels, x increases rightward and y downward. A standard centered Rect/Ellipse for bounds `[l,r) × [t,b)` uses center `((l+r)/2,(t+b)/2)` and size `(r-l,b-t)`. For a nested or custom-anchored actor, inspect final screen-space bounds using its named instance path. Do not infer them from local coordinates. Use a shared transform or progress for attached parts. Pass a typed Color to component parameters; use `hex_color(...)` for fixed hex colors or `rgb(...)` / `rgba(...)` for computed ones.

## Protected marks near motion

Keep identifying paint outside the swept footprint, including stroke, caps, joins and decoration, or show that compositing keeps it readable without obscuring other content. For a foreground mark already needed at 0, declare it in the time-zero setup after the mover if it paints above it; do not create it later just to change draw order.

For straight monotonic travel, project the mark onto the centerline, clamp to the segment and invert the easing to estimate the closest time. Check a nearby boundary if riskier. For curves, transforms or uncertain bounds, use provisional samples/diagnostics to find risk, not as proof. Check a native crop at the actual risky pass against a clear reference crop. No structural warning is not clearance, and permission to overlap one mark doesn't extend to others.

## Connectors to nested ports

Use named points for a fixed attachment, e.g. `from: source.right, to: moving.port.left`. A bare actor endpoint lets the engine choose the facing side. Set `stroke` explicitly. Declare a rear connector before the opaque movers and a foreground mark after them. At an interior pose, check the connector bounds track the actual nested port and verify exposed wire paint; a successful build can still render none.

Run [the small self-contained example](../assets/nested-connector.amx) with the matching engine. The example starts travel at 1 s and moves for 2 s. At 2 s the body crosses the fixed badge near the midpoint of its path, so this frame tests both layer order and tracking. If the path or easing changes, find the new crossing time. Here the visible wire reaches the back of the moving body, the endpoint resolves to the port edge, and P paints above the body.
