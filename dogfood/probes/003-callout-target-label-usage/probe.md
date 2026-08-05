# Probe: Callout.target references count as label usage

Status: resolved.

## Intent

A scene should be able to annotate actors without also animating them directly.
The analyzer should understand that `target: box1` is a use of `box1`.

## Minimal Repro

See `repro.amx`.

```animatix
box1: Rect, size: (100, 100), color: accent.primary
note: Callout { target: box1, place: bottom }
```

## Expected DSL

No `unused-label` warning for actors referenced by callouts.

## Resolution

The analyzer now collects `Callout.target` references from both actor
declarations and assignments, including string labels. `animatix lint` on the
repro reports no diagnostics.

## Regression coverage

- `callout_target_references_count_as_usage` verifies declaration and
  assignment targets are marked as references.
- The repro remains as a dogfood regression fixture.
