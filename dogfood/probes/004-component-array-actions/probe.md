# Probe: component-generated array actors and indexed custom actions

Status: runtime findings fixed; analyzer follow-up open.

## Intent

A reusable `Bars` component should generate `bar[i]` actors, expose custom
actions that target `bar[index]`, and still participate in `swap` from the
scene.

## Minimal Repro

See `repro.amx`.

```animatix
pub component Bars(values: List<Num>, colors: List<Color>) {
  row: Row {
    for v, i in values {
      bar[i]: Rect, size: (100, v * 2), color: colors[i]
    }
  }

  action pulseAt(index: Num) {
    bar[index].scale = 1.1 [120ms]
    bar[index].scale = 1.0 [180ms]
  }
}
```

## Fixed Findings

- Component labels inside inline `for` loops were not namespaced, so `bar[i]`
  expanded to global `bar__N` tracks instead of `deck.bar__N`.
- Custom component actions did not substitute parameters into indexed target
  segments, so `bar[index]` was not rewritten to the instance namespace or to
  the concrete `deck.bar__1` target.
- External `swap deck.bar[0]` now resolves because the component generates
  `deck.bar__N` tracks.
- The analyzer now recognizes local component types, component action names,
  and component-generated array labels.

## Remaining Findings

- Named color lists such as `{red, green, blue}` are inferred as `List<Color>`
  by the shared symbol-aware type layer; that finding is tracked separately as
  probe 005.

## Regression coverage

- `load_program_expands_component_for_loop_array_actors` verifies component
  `for` actors create `deck.bar__N` tracks.
- `load_program_custom_component_action_indexed_array_target` verifies custom
  indexed actions inline to concrete `deck.bar__1` targets.
