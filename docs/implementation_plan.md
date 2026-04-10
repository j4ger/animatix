# Animatix Implementation Plan

## Status Overview

| Category | Parser | Runtime | Status |
|----------|--------|---------|--------|
| **Reactive System** | Complete | Complete | **IMPLEMENTED** |
| **Math/Graph** | Complete | Complete | **IMPLEMENTED** |
| **Containers (Row, Col)** | Complete | Complete | **IMPLEMENTED** |
| **Containers (Grid, Stack)** | Missing | Missing | **NOT IMPLEMENTED** |
| **Components** | Partial | Missing | **NOT IMPLEMENTED** |

---

## 1. Reactive System (`loop`, `always`, `for`)

### Status: IMPLEMENTED (Phases 1, 2 & 3 Complete)

The reactive system uses a hybrid evaluation architecture, implemented via phased rollout.

### Phase 1: Per-Frame Evaluation Pipeline (IMPLEMENTED)

**Goal**: Establish the two-layer evaluation model (Base + Modifier).

**Status**: **COMPLETE**. The four-stage pipeline correctly passes base keyframes and layers over transient per-frame modifiers via a HashMap override system.

### Phase 2: Compile `for` Loops into Unrolled Keyframes (IMPLEMENTED)

**Goal**: Eliminate loop structures at compile time.

**Status**: **COMPLETE**. `for` loops expand statically during parse/timeline building to prevent runtime overhead.

### Phase 3: Stateful `loop` Blocks & Generators (IMPLEMENTED)

**Goal**: Stateful, per-frame expression evaluation.

**Tasks**:
1. Compile `always` blocks into AST evaluator closures that run every frame
2. Compile `loop` blocks into generator-style closures that maintain state across frames
3. Implement `yield` as a pause/resume mechanism
4. Support labeled loops (`job: loop 5s { ... }`) with `Stop`, `Pause`, `Resume` commands

**Status**: **COMPLETE**. `LoopState` manages program counters, `time_remaining`, and frame environments across evaluations in the `Timeline`. `Yield` properly defers execution to the next frame.

**`loop` Execution Model**:
- Each labeled `loop` maintains a struct with: program counter, local variables, remaining time
- On `yield`, the struct is serialized to the timeline state
- On the next frame, the struct is restored and execution resumes

**`always` Execution Model**:
- No state maintained between frames
- Full expression tree re-evaluated every frame
- Can read from base layer values for composition

### Target Syntax

```animatix
// Per-frame evaluation
always { ball.at = (mouse.x, mouse.y) }

// Loop constructs (stateful)
job: loop 5s {
  ball.at = (0, 0)
  yield
  ball.at = (100, 0)
  yield
}
stop job

// Bounded iteration (compile-time unrolling)
for i in 0..3 {
  star[i]: Circle, radius: 20
}
```

### Why Three Phases
1. Phase 1 established the evaluation infrastructure and override mechanism.
2. Phase 2 performed pure compiler transformations for bounds unrolling.
3. Phase 3 & 4 added internal generator state management to maintain contexts between yields and handle dynamic control flow.

---

## What Is Left To Do: Future Phases

To maintain a steady cadence, the remaining work is organized into distinct actionable phases.

### Phase 5: 2D Layout Containers (`Grid`, `Stack`)

**Goal**: Complete 2D geometry container types.

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

## Phase 6: Components

**Goal**: Full reusable module components mechanism.

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
