# Animatix Implementation Plan

## Executive Summary

After analyzing the codebase (AST, parser, timeline, renderer), here is the current status of each major feature area:

| Category | Parser | Runtime | Status |
|----------|--------|---------|--------|
| **Reactive System** | ❌ Missing | ❌ Missing | **PLANNED** |
| **Math/Graph** | ✅ | ✅ Math done, Graph deferred | **MATH DONE** |
| **Containers** | ✅ | ✅ Layout | **LAYOUT DONE** |
| **Components** | ✅ | ❌ Missing | **PLANNED** |

---

## 1. Reactive System (`loop`, `always`, `if`)

### Current Status: NOT IMPLEMENTED

**AST Defined** (`ast.rs`):
- `LoopKind`: `Infinite`, `Bounded(Time)`, `Count(u32)`
- `LoopCommand`: `Stop`, `Pause`, `Resume`
- `Loop`, `LoopControl`: For loop control flow
- `Always`, `LabeledAlways`: Per-frame evaluation blocks
- `Conditional`: If/else expressions
- `ForLoop`: Iteration

**Parser** (`parser.rs`):
- ❌ **MISSING**: No parsing for `always`, `loop`, `if`, `for` statements
- The `stmt` choice (line 442-453) does NOT include these reactive constructs

**Timeline** (`timeline/mod.rs`):
- ❌ **MISSING**: `process_body()` falls through to `_ => {}` for all reactive statements

### Implementation Required

```animatix
// Syntax to support:
always { ball.at = (x, sin(x)) }
job: loop 3 times { ... }
loop 5s { ... }
stop job
if x > 0 { ... } else { ... }
for item in items { ... }
```

### Recommendation: **DEFER**

**Rationale:**
1. Requires significant runtime architecture changes (per-frame evaluation model vs. keyframe model)
2. No clear use case in current examples
3. Interactive UI (slider integration) is the main driver - better to implement UI first
4. AST and data structures are already designed - can be added later

---

## 2. Math / Graph

### Current Status: PARTIAL

**Implemented:**
- `Math` primitive via Typst rendering (line 250-354 in `timeline/mod.rs`)
- Formula: `eq: Math, math: "E = mc^2", font_size: 18pt`

**NOT Implemented:**
- `2dGraph` container type
- `graph.plot` method/actor
- Math functions (`sin`, `cos`, `sqrt`, etc.) for live computation
- `format()` function

### Implementation Required

```animatix
// Planned syntax:
graph: 2dGraph, x_range: {-5, 5}, y_range: {-10, 30}
plot: graph.plot, func: "x^2 + 3", color: red
```

### Recommendation: **DEFER Graph, IMPLEMENT Math Functions**

**Rationale:**
- Graph rendering is complex (coordinate systems, axes, plotting)
- Math rendering via Typst already works
- Math functions (sin, cos, etc.) are simpler to add and useful for animations
- Consider: If someone needs graphs, they likely need reactive system too

**Practical Now:**
- Add `Expr::Call` evaluation in timeline for math functions
- Add `format()` string interpolation

---

## 3. Containers (`Row`, `Col`, `Grid`, `Group`)

### Current Status: PARTIAL (Row, Col IMPLEMENTED)

**Implemented:**
- `ActorDecl` with `children: Vec<InlineItem>` 
- `process_inline_items()` - recursively processes children
- Children added to scene graph with proper parentage
- `Group` type recognized (treated same as other actors)
- ✅ **Row**: Horizontal layout algorithm with `gap` and `align`
- ✅ **Col**: Vertical layout algorithm with `gap` and `align`

**NOT Implemented:**
- ❌ **Grid**: No 2D grid layout
- ❌ **Stack**: No overlapping layout

### Current Behavior

```animatix
row: Row, gap: 10, align: center { A, B, C }
```

Children (`A`, `B`, `C`) are positioned automatically according to the layout rules (e.g., horizontally with 10px spacing, centered vertically).

### Recommendation: **DEFER Grid and Stack**

**Rationale:**
1. Clean, self-contained change - just need to calculate child positions
2. Enables much more complex scenes
3. Layout algorithms are well-understood
4. Grid/Stack can be deferred, but Row/Col are essential

**Implementation Approach:**

In `timeline/mod.rs::process_body()`, when processing `Stmt::ActorDecl` with `ty == "Row"` or `"Col"`:

```rust
// Row: arrange children horizontally
// Col: arrange children vertically

// After processing children, compute their positions based on:
// - gap property (default 0)
// - child's own size
// - alignment properties
```

---

## 4. Components

### Current Status: DEFINED, NOT IMPLEMENTED

**Implemented:**
- `ComponentDef` AST node (line 141-146 in `ast.rs`)
- `component_def` parser (line 412-440 in `parser.rs`)
- `LifecycleEvent`, `ComponentAction` AST nodes

**NOT Implemented:**
- Component instantiation from `.actor.amx` files
- Parameter passing to components
- Lifecycle hooks (`on appear`, `on disappear`)
- Custom actions on components
- `@config` block

### Implementation Required

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

### Recommendation: **DEFER**

**Rationale:**
1. Depends on module import system being fully functional
2. Requires significant design work (parameter scoping, action inheritance)
3. Current `Group` with inline children provides similar functionality
4. Can be added incrementally - start with simple actor definitions

**When Ready:**
1. Complete module system (`module.rs` already exists)
2. Add component instantiation to timeline
3. Implement lifecycle hooks as special action triggers

---

## Implementation Priority

### Priority 1: Container Layout (Practical, High Impact) ✅

**Files modified:** `crates/animatix/src/timeline/mod.rs`

**Changes:**
1. Added `gap` and `align` property handling to `ActorDecl`
2. After `process_inline_items()`, compute layout positions via a two-pass algorithm
3. Update each child's position track with calculated values

**Status:** IMPLEMENTED

### Priority 2: Math Functions (Simple Addition) ✅

**Files to modify:** `crates/animatix/src/timeline/mod.rs`

**Changes:**
1. Add evaluation for `Expr::Call` with math functions
2. Add `format()` string interpolation

**Status:** IMPLEMENTED

### Priority 3: Parser Completeness (Low Risk) ✅

**Files to modify:** `crates/animatix/src/parser.rs`

**Changes:**
1. Add `always`, `loop`, `conditional`, `for` parsing (even if not evaluated)
2. This ensures syntax is correct for future implementation

**Status:** IMPLEMENTED

---

## Deferred Features (Marked as PLANNED)

| Feature | Reason for Deferral |
|---------|---------------------|
| Reactive System (`always`, `loop`, `if`) | Requires per-frame evaluation model; depends on interactive UI |
| Graph/Plot system | Complex coordinate system; depends on reactive system for live data |
| Components | Requires mature module system; can use Group as interim solution |
| Lifecycle hooks | Depends on component system |
| Config blocks | Not critical for core functionality |

---

## Summary

**Implemented (Priorities 1-3):**
1. ✅ Container layout (Row, Col) - high impact, clean change
2. ✅ Math functions (sin, cos, format) - simple additions
3. ✅ Parser completeness - `always`, `loop`, `conditional`, `for` parsing added

**Plan for Later:**
1. 🔲 Reactive system - needs architecture work
2. 🔲 Graph rendering - depends on reactive
3. 🔲 Components - depends on module system
4. 🔲 Advanced layout (Grid, Stack) - can build on Row/Col

The codebase is well-structured and the AST design is sound. Container layout, math functions, and parser completeness are now complete.