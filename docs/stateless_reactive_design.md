# Stateless Reactive Design

This document describes the stateless reactive model that Animatix now ships.

It also preserves the rationale and migration story for replacing the earlier coroutine-style `loop` / `yield` design.

---

## 1. Problem Statement

Animatix preview, scrubbing, image export, and video export all want the same core property:

> the rendered frame at time `t` should be derivable directly from `t`

That property is already true for keyframes, layout, sampled actor properties, and `always` evaluation.

The language also overloaded `loop` for two different jobs:

1. **structural generation** — create multiple objects
2. **repeated behavior** — make something happen again and again over time

Those jobs want different semantics and should not share one construct.

---

## 2. New Model

### Structural repetition: `for`

Structural generation should remain a compile-time concern.

Use `for` for:
- repeated actors
- repeated component instances
- repeated child blocks
- repeated keyframe templates that can be expanded during timeline build

`for` should be understood as the Animatix equivalent of an elaboration/generate step, not as a runtime loop.

### Behavioral repetition: `always`

Repeated behavior should be expressed as a pure function of time inside `always`.

Use `always` for:
- continuous motion
- oscillation
- stepped toggles
- sampled composition from the current scene state
- finite or bounded periodic behavior using explicit time windows

The key rule is:

> `always` has no hidden memory between frames.

Its result is computed from:
- the requested time `t`
- scene dimensions
- sampled actor/scene properties
- explicit variables defined in the current block

---

## 3. Design Goals

The redesign should preserve these properties:

1. **Random-access preview correctness** — jumping directly to `t = 8.2s` should produce the same frame as playback reaching `8.2s`.
2. **Simple mental model** — users should think in terms of keyframes + stateless overrides, not resumable interpreters.
3. **Good authoring ergonomics** — repeated motion should remain concise without forcing everything into raw coordinate math.
4. **Export consistency** — preview, image export, and video rendering should agree.
5. **AI-friendly determinism** — authored scenes should be easy to generate and reason about from text.

---

## 4. Recommended Public Taxonomy

Animatix should present repetition through three concepts:

### `for`
Compile-time structural expansion.

```animatix
for item in items {
  // expanded structurally at build time
}
```

### `always`
Stateless per-time evaluation.

```animatix
always {
  orbiter.at = (640 + 180 * cos(t), 360 + 120 * sin(t * 2))
}
```

### Keyframes
Declarative timed changes.

```animatix
#0s
panel.opacity = 0

#1s
panel.opacity = 1 [0.4s]
```

This yields a clean split:

- **shape the scene** with `for`, components, and containers
- **declare timed animation** with keyframes
- **add time-derived behavior** with `always`

---

## 5. What Replaces `loop`

The old `loop` use cases map as follows:

| Old intent | New expression |
|---|---|
| Make N objects | `for` |
| Repeat a smooth motion forever | `always` + `sin/cos/lerp` |
| Repeat a stepped toggle | `always` + `%` / conditionals |
| Repeat behavior for a bounded window | `always` + explicit time guards |
| Pause/resume/stop internal coroutine state | not part of the default timeline model |

### Smooth periodic motion

```animatix
always {
  pulse.radius = 20 + 6 * sin(t * 4)
}
```

### Stepped periodic motion

```animatix
always {
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
}
```

### Bounded repeated behavior

```animatix
always {
  let active = t < 4.0
  pulse.opacity = if active { 0.5 + 0.5 * sin(t * 6) } else { 0.0 }
}
```

The recommended model is to make the time window explicit rather than hiding it in coroutine state.

---

## 6. `always` Semantics

The docs should consistently describe `always` as:

> a stateless render-time override block evaluated independently for the requested time.

That means:
- it is re-evaluated from scratch for each frame request
- it can read sampled actor and scene properties
- it can compute derived values using `t`
- it does not resume from a prior program counter
- it does not preserve hidden mutable locals across frames

This should be the main public reactive model.

---

## 7. Removed Coroutine Semantics

The older coroutine surface (`loop`, `yield`, labeled `stop` / `pause` / `resume`) was removed from the shipped reactive model.

The language now presents one runtime reactive primitive:
- `always`

And one structural repetition primitive:
- `for`

---

## 8. Language Guarantee

The target language promise should be:

> For pure authored scenes, the frame at time `t` is a random-access function of the source, the requested time, and the render dimensions.

This promise should power:
- timeline scrubbing
- exact frame export
- preview correctness
- deterministic reasoning about scene state

### Important caveat

Current `rand()` is not a deterministic function of time, so scenes depending on fresh randomness per evaluation should be excluded from strict repeatability guarantees until randomness is redesigned.

---

## 9. Migration Story

### Before

```animatix
pulse_job: loop 4 times {
  pulse.size = (120, 120)
  yield
  pulse.size = (180, 180)
  yield
}
```

### After

```animatix
always {
  pulse.size = if (t % 1.0) < 0.5 { (120, 120) } else { (180, 180) }
}
```

### Before

```animatix
loop 5s { ... }
```

### After

```animatix
always {
  let visible = t < 5.0
  actor.opacity = if visible { 1.0 } else { 0.0 }
}
```

### Before

```animatix
loop 3 times { create stuff }
```

### After

```animatix
for item in items {
  // expanded structurally at build time
}
```

---

## 10. Result

Animatix now uses:

1. `for` as the only repetition construct for structure
2. `always` as the only runtime reactive primitive
3. random-access preview/export correctness as the top evaluation contract

This is the design that best matches Animatix’s preview-first architecture, current timeline system, and authoring goals.
