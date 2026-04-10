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
- User-defined functions with closures

### Planned Syntax

```animatix
// Stage 1-2: Math functions and formatting
ball.at = (x, sin(x))
label.text = format("Value: {}", result)

// Stage 3: User-defined functions
fn easeInOut(t: Float) -> Float {
  if t < 0.5 { 2 * t * t }
  else { 1 - (-2 * t + 2)^2 / 2 }
}

// Stage 4: Graph rendering
graph: 2dGraph, x_range: {-5, 5}, y_range: {-10, 30}
plot: graph.plot, func: "x^2 + 3", color: red
```

### Recommendation: **PHASED APPROACH**

Build a string-lookup-based AST evaluator (`Environment`) now, defer Bytecode VM to later.

#### Stage 1: Context-Aware AST Evaluator
**Focus:** Core evaluation infrastructure

- `Environment` struct: string-to-value registry for variable bindings
- `Value` enum: `Number(f64)`, `String(String)`, `Bool(bool)`, `Color`, `Vector`
- `Registry` pattern: Central lookup for built-in functions by name
- `Expr::Call` evaluation via registry lookup (no closure capture yet)
- Value expansion in actor properties (e.g., `ball.at = (x, sin(x))`)

**Deliverable:** Working math functions (`sin`, `cos`, `sqrt`, `format`) with variable bindings.

#### Stage 2: Standard Library & Advanced Types
**Focus:** Expanding the type system and utility functions

**Stage 1 Status:** Environment, Context-aware AST, basic functions (sin, cos, sqrt, format) are complete.

**Stage 2 Goals:**

1. **Arithmetic Operator Parsing**
   - Parser currently only handles comparison operators (`>`, `<`, `==`, etc.)
   - Add parsing for arithmetic operators: `+`, `-`, `*`, `/`, `%`, `^`
   - Extend `Expr::Binary` evaluation to support arithmetic operations

2. **Vector Types (`Vec2`, `Vec3`, `Vec4`)**
   - Add `Value::Vec2([f64; 2])`, `Value::Vec3([f64; 3])`, `Value::Vec4([f64; 4])` variants
   - Support vector literals in parser: `(1.0, 2.0)` for Vec2, `(1.0, 2.0, 3.0)` for Vec3
   - Arithmetic on vectors: `Vec2 + Vec2`, `Vec2 * Num`, `Vec2 / Num`

3. **Color Type**
   - Add `Value::Color(Color)` variant
   - `parse_color` function: convert hex strings (`"#ff0000"`) or names (`"red"`) to Color
   - Color operations: `Color * Num` (brightness scaling), `Color + Color` (blend)

4. **Native Functions**
   - `rand()`: Generate random number between 0.0 and 1.0
   - `noise(x, y)`: 2D Perlin/simplex noise for organic motion
   - `parse_color(color_str)`: Convert color names/hex to Color value

5. **Dynamic Type Evaluation**
   - Binary operations dispatch based on operand types
   - `Num op Num` -> Num
   - `Vec2 op Vec2` -> Vec2 (element-wise)
   - `Vec2 * Num` -> Vec2 (scalar multiplication)
   - `Color * Num` -> Color (brightness adjustment)
   - `Color + Color` -> Color (blend)

**Deliverable:** Full arithmetic support with rich type system for spatial math and color operations.

#### Stage 3: User-Defined Functions
**Focus:** Closures and custom behavior

- `Function` struct with captured environment (closure)
- `Expr::FuncDef` AST node with parameter bindings
- Scope chain: local scope -> closure scope -> global registry
- Recursive function support

**Deliverable:** User-defined functions with proper scoping.

#### Stage 4 (Deferred): Bytecode VM
**Focus:** Extreme performance for complex computations

- Compile AST expressions to bytecode instructions
- Stack-based VM for evaluation
- JIT compilation considerations
- Benchmark-driven: Only implement if profiling shows need

**Rationale for Deferral:**
- String lookup is fast enough for most animation use cases
- Simpler architecture enables faster iteration
- Can add later without breaking changes

**Rationale for Phased Approach:**
- Graph rendering is complex (coordinate systems, axes, plotting)
- Math rendering via Typst already works
- Math functions (sin, cos, etc.) are simpler to add and useful for animations
- Environment/Registry pattern is a clean foundation for all stages

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

### Priority 4: Math Engine Stage 1 & 2

**Files to modify:** `crates/animatix/src/timeline/mod.rs`, new `crates/animatix/src/runtime/` module

**Stage 1 Changes:**

1. Create `Environment` struct with `HashMap<String, Value>` for variable bindings
2. Create `Value` enum: `Number(f64)`, `String(String)`, `Bool(bool)`, `Color(Color)`, `Vector(Vec<Value>)`
3. Create `Registry` struct holding built-in functions: `sin`, `cos`, `tan`, `sqrt`, `abs`, `floor`, `ceil`, `round`, `pow`, `format`
4. Add `format!`-style string interpolation: `"Value: {}"` -> `"Value: 42"`
5. Modify `process_body()` to pass `&mut Environment` through expression evaluation
6. Implement `Expr::Call` evaluation: lookup function in registry, apply arguments

**Stage 2 Changes:**

1. Add `Value::Vec2`, `Value::Vec3`, `Value::Vec4` for vector types
2. Add `Value::Color` for color values
3. Add arithmetic operator parsing (`+`, `-`, `*`, `/`, `%`, `^`) to complement existing comparison operators
4. Implement dynamic type evaluation in binary operations (e.g., `Vec2 + Vec2`, `Color * Num`)
5. Add native functions: `rand`, `noise`, `parse_color`
6. Add `modulo`, `random` to registry

**Status:** IN PROGRESS

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