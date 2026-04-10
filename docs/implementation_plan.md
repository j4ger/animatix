# Animatix Implementation Plan

## Status Overview

| Category | Parser | Runtime | Status |
|----------|--------|---------|--------|
| **Reactive System** | Partial | Missing | **NOT IMPLEMENTED** |
| **Math/Graph** | Complete | Complete | **IMPLEMENTED** |
| **Containers (Row, Col)** | Complete | Complete | **IMPLEMENTED** |
| **Containers (Grid, Stack)** | Missing | Missing | **NOT IMPLEMENTED** |
| **Components** | Partial | Missing | **NOT IMPLEMENTED** |

---

## 1. Reactive System (`loop`, `always`, `if`)

### Status: NOT IMPLEMENTED

**AST is defined** (`ast.rs`):
- `LoopKind`: `Infinite`, `Bounded(Time)`, `Count(u32)`
- `LoopCommand`: `Stop`, `Pause`, `Resume`
- `Always`, `LabeledAlways`: Per-frame evaluation blocks
- `Conditional`: If/else expressions
- `ForLoop`: Iteration

**Parser** (`parser.rs`):
- **MISSING**: No parsing for `always`, `loop`, `if`, `for` statements

**Runtime** (`timeline/mod.rs`):
- **MISSING**: `process_body()` falls through to `_ => {}` for all reactive statements

### Target Syntax

```animatix
// Per-frame evaluation
always { ball.at = (x, sin(x)) }

// Loop constructs
job: loop 3 times { ... }
loop 5s { ... }
stop job

// Conditionals
if x > 0 { ... } else { ... }

// Iteration
for item in items { ... }
```

### Why Deferred
1. Requires per-frame evaluation model (vs keyframe model)
2. No clear use case in current examples
3. Interactive UI (slider integration) is the main driver
4. AST and data structures are already designed

---

## 2. Containers (`Grid`, `Stack`)

### Status: Row and Col IMPLEMENTED, Grid and Stack NOT IMPLEMENTED

**Implemented:**
- `Row`: Horizontal layout with `gap` and `align`
- `Col`: Vertical layout with `gap` and `align`
- `Group`: Generic container for grouping and transform inheritance

**NOT Implemented:**
- `Grid`: 2D grid layout
- `Stack`: Overlapping layout

### Target Syntax

```animatix
// Grid: 2D layout
grid: Grid, cols: 3, gap: 10 {
  Item1, Item2, Item3
  Item4, Item5, Item6
}

// Stack: Overlapping elements
stack: Stack {
  background: Rect, width: 100, height: 100
  foreground: Circle, radius: 30
}
```

---

## 3. Components

### Status: DEFINED (AST), NOT IMPLEMENTED (Runtime)

**Implemented:**
- `ComponentDef` AST node
- `component_def` parser
- `LifecycleEvent`, `ComponentAction` AST nodes

**NOT Implemented:**
- Component instantiation from `.actor.amx` files
- Parameter passing to components
- Lifecycle hooks (`on appear`, `on disappear`)
- Custom actions on components
- `@config` block

### Target Syntax

```animatix
// button.actor.amx
pub actor Button(text: "Click", color: Color) {
  bg: Rect, color: color
  label: Text, text: text
}

// scene.amx
import "button.actor.amx"
btn: Button, text: "Submit", color: blue
```

### Why Deferred
1. Depends on module import system being fully functional
2. Requires significant design work (parameter scoping, action inheritance)
3. Current `Group` with inline children provides similar functionality

---

## Stage 3 (Complete)

### User-Configurable Sampling Parameters

The plotting engine exposes `tolerance` and `max_depth` properties to the Animatix language AST:

```animatix
// High fidelity for smooth curves
smooth_curve: CartesianPlot, func: (x) => sin(x), tolerance: 0.001, max_depth: 12

// Fast rendering for previews
preview_curve: CartesianPlot, func: (x) => x^2, tolerance: 0.5, max_depth: 6
```

### Discontinuity Detection

The plotting engine detects and handles mathematical discontinuities like asymptotes. For example, plotting `y = 1/x` across `(-1, 1)` no longer draws a massive vertical line through the origin.

The engine:
1. Checks if `y` values jump by more than a threshold between adjacent samples
2. Breaks the path into separate segments at the discontinuity point
3. Injects `NAN` to break the Vello path at discontinuities

### Bounding-Box Culling

Optimizes plotting by stopping subdivision when a segment is entirely outside the graph's physical screen bounds.

## Future Improvements

### Parametric and Implicit Curve Plotting

**Parametric curves:**
```animatix
ParametricPlot, x_func: (t) => cos(t), y_func: (t) => sin(t), t_range: (0, 2π)
```

**Implicit equations:**
```animatix
ImplicitPlot, equation: (x, y) => x^2 + y^2 - 1, x_range: (-1.5, 1.5), y_range: (-1.5, 1.5)
```

### Advanced Path Effects

- Path trimming
- Dashing patterns
- Stroke animations
