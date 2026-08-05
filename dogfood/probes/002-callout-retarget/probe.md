# Probe: Callout retargeting accepts actor labels

Status: resolved.

## Intent

A visualizer wants one `Callout` that follows whichever array actor is the
current algorithm key. The natural source is to retarget with the actor label:

```animatix
note.target = bar[2] [300ms]
```

## Minimal Repro

See `repro.amx`. The bare-label form now works and rendered frames before and
after the retarget differ.

## Expected DSL

Bare actor labels should be accepted in assignment value positions for
properties whose type is an actor reference.

## Resolution

`CalloutPrimitive::handle_assignment` now accepts `Expr::Ident` and
`Expr::Path` as target labels, matching the declaration syntax, while still
allowing string labels and dynamic string expressions.

## Regression coverage

- `test_callout_target_assignment_accepts_bare_actor_label` verifies the
  timeline target changes from `box1` to `box2`.
- The repro remains as a dogfood regression fixture.
