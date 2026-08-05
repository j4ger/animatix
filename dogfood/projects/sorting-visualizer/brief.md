# Sorting Visualizer Dogfood Brief

## Content Goal

A short explainer that visually walks through one pass of insertion sort on a
four-element array: unsorted values, compare-and-swap steps, and the sorted
result.

## Audience

A developer or student who understands arrays but benefits from seeing how
insertion sort moves one key into place.

## Scenes

1. Title: introduce the array as a row of bars.
2. Steps: show key selection, comparisons, and swaps.
3. Result: show the sorted array and the "one pass" summary.

## Constraints

- Resolution 1280x720, editorial-dark colorscheme.
- Bars should be generated from data with `for`, not hand-declared.
- Indexed actors should be used for action targets and reactive overrides.
- Reorder/swap should use the layout container, not manually animated positions.
- Pointer and key labels should avoid hand-computed coordinates where the
  grammar offers an actor-tracking primitive.

## Success Criteria

A reviewer can follow one insertion sort pass from source without reading a
step-by-step transcript. The source should communicate structure with `for`,
components, modules, and indexed actor targets wherever the grammar allows it.
