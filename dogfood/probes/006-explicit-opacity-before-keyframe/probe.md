# Probe: explicit opacity on pre-keyframe declarations is ignored

Status: resolved.

## Intent

A five-card cascade declares each card with `opacity: 0` so the scene starts
hidden, then fades the cards in with `stagger`. The DSL should honor the
explicit initial opacity.

## Minimal Repro

```animatix
config { colorscheme: "editorial-dark", resolution: (640, 360) }

box: Rect, size: (100, 100), color: accent.primary, opacity: 0, at: (200, 150)
```

At `--time 0.0`, the actor was rendered fully visible before the fix.

## Expected DSL

```animatix
config { colorscheme: "editorial-dark", resolution: (640, 360) }

box: Rect, size: (100, 100), color: accent.primary, opacity: 0, at: (200, 150)
```

At `--time 0.0`, the actor should be invisible.

## Resolution

The regular actor declaration pipeline now parses explicit `opacity` from
declaration properties and inserts it at the declaration time. The previous
pre-seeding path only handled the default hidden state, so explicit values
were never written to the track.

## Regression coverage

- `explicit_opacity_before_keyframe_is_honored` verifies `opacity: 0` at
  declaration time evaluates to 0.0.
- `dogfood/runs/002-array-actors-vs-named/` uses explicit `opacity: 0` in both
  variants and now starts with all cards hidden.

## Impact

Authors can write an explicit invisible start state without relying on
pre-keyframe default hiding. Entrance timing is consistent for every actor,
including actors whose fade-in starts after 0s.
