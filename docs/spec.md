# Animatix Language Specification

---

## 1. File Types

- **Scene Files (`.amx`)**: Main animation scripts. Contain keyframes, actors, and timeline definitions.
- **Component Files (`.actor.amx`)**: Reusable actor definitions. Contain parameters, internal structure, and custom actions.
- **Library Files (`.lib.amx`)**: Collections of utility functions, math helpers, and constants.

---

## 2. Core Syntax Symbols

- **Let (`let`)**: Used for declaring objects, variables and actors (non-rendered values).  
  *Example:* `let x = 0`
- **Colon (`:`)**: Shorthand for binding actors to a label, and put it to scene.  
  *Example:* `btn: Button, text: "OK"`
- **Hash (`#`)**: Marks a keyframe in the timeline.  
  *Example:* `#0s`, `#2.5s`, `#@10s` (absolute timestamp)
- **Curly Brackets (`{ }`)**: Used for container children, arrays, and block scopes.  
  *Example:* `Row { Item1, Item2 }`
- **Square Brackets (`[ ]`)**: Used for action modifiers (duration, easing).  
  *Example:* `[2s, ease: bounce]`
- **Equals (`=`)**: Used for property assignment (instant change or animated) or variable binding.
  *Example:* `btn.color = red` or `morpher.size = (100, 100) [2s]`
- **Comma (`,`)**: Separates object properties.  
  *Example:* `Type, prop: val, prop: val`
- **Space (` `)**: Separates action verbs from arguments.  
  *Example:* `fade-in btn [1s]`
- **Dot (`.`)**: Used for nested access (namespacing).  
  *Example:* `container.child`

---

## 3. Declarations

**Actor Declaration**  
Actors are rendered objects. They must be declared with a label and a type.
```animatix
label: Type, property: value, at: (x, y)
```

**Variable Declaration**  
Variables are computed values. They are not rendered directly.
```animatix
let name = expression
```

**Re-Declaration (Morph Trigger)**  
If an existing label is declared again at a later keyframe, the engine morphs the existing actor to the new definition.
```animatix
#0s
btn: Button, text: "OK"

#2s
btn: Button, text: "Submit" [2s]

#10s
btn: another_button // morph into another pre-defined object
```

**Implicit Objects**  
The engine provides an implicit `scene` object representing the global environment. Its properties, such as `background_color` (defaulting to `black`), can be assigned and animated like any other property.
```animatix
#0s
scene.background_color = black // Default

#2s
scene.background_color = white [2s]
```

---

## 4. Timeline & Keyframes

**Timeline Structure**  
The timeline maintains a hierarchical `scene_graph` mapping parent containers to their children. During evaluation, transforms and opacities cascade down this tree via depth-first search, accumulating parent values into child values.

**Absolute Keyframes**  
Marks a specific time in seconds or milliseconds.
```animatix
#@0s
#@2.5s
#@500ms
```

**Relative Keyframes**  
Marks a time relative to the previous keyframe.
```animatix
#1s
```

**Parallel Actions**  
Actions listed under the same keyframe execute simultaneously.
```animatix
#2s
fade-in A [1s]
color B to red [2s]
```

---

## 5. Actions & Modifiers

**Action Invocation**  
Actions use verb-first syntax with space-separated arguments.
```animatix
fade-in btn [1s]
fade-out btn [1s]
```

**Modifiers**  
Modifiers are enclosed in square brackets immediately following the action.
```animatix
[duration]
[ease: function]
[delay: time]
```

**Common Modifiers**
```animatix
[2s]                      // Duration
[ease: ease-in-out]       // Easing curve
[path: arc]               // Morph interpolation path
```

**Built-in Actions Registry**  
The engine maintains a highly extensible registry of built-in actions, categorized by their primary visual effect:
- **Entrance**: Actions that introduce an actor to the scene (e.g., `wipe-in`, `fade-in`).
- **Exit**: Actions that remove an actor from the scene (e.g., `fade-out`).

This architecture exposes rich action signatures (including `name`, `category`, `description`, `params`, and `modifiers`). This allows external tooling—such as Language Server Protocols (LSP) and visual UI editors—to dynamically discover, document, validate, and provide autocompletion for all supported animations.

---

## 6. Morphing System

**Automatic Morph**  
Re-declaring an actor at a new keyframe triggers a morph transition.
```animatix
#0s
circle: Circle, at: (0, 0)

#2s
circle: Circle, at: (100, 100) [2s]
```

**Morph Strategies**  
Users can hint how the engine should handle topology changes.
```animatix
[strategy: auto]          // Engine decides (default)
[strategy: match]         // Force point alignment
[strategy: fade]          // Cross-fade (ReplacementTransform)
```

**Instant Change**
Use zero duration or property assignment for instant updates.
```animatix
btn: Button, text: "New" [0s]
btn.text = "New"
```

**Property-Level State Tracking**
Assignments can now take modifiers, allowing individual properties to be animated independently of the entire actor morph.
```animatix
morpher.size = (100, 100) [2s, ease: ease-out]
```

---

## 7. Containers & Layout

> **Status: Partial** — Row and Col are implemented. Grid, Stack, and Group are planned.

**Container Types**

- `Row`: Horizontal layout container. Supports `gap` (number, spacing between children) and `align` ("start", "center", "end" for vertical alignment). (Implemented)
- `Col`: Vertical layout container. Supports `gap` (number, spacing between children) and `align` ("start", "center", "end" for horizontal alignment). (Implemented)
- `Grid`: 2D grid layout (Planned)
- `Stack`: Overlapping layout (Planned)
- `Group`: Generic container for grouping and transform inheritance (Planned)

**Layout Properties**

The `gap` property sets uniform spacing between children. The `align` property controls perpendicular alignment:
- For `Row`: aligns children vertically ("start" = top, "center" = middle, "end" = bottom)
- For `Col`: aligns children horizontally ("start" = left, "center" = middle, "end" = right)

```animatix
row: Row, gap: 12, align: center {
  Rect, color: red
  Circle, color: blue
}
```

**Children Declaration**  
Children are declared inline within curly brackets. Children may be **labeled** (explicit name) or **anonymous** (no name).

```animatix
row: Row, gap: 10 {
  Button, text: "A"       // labeled child: accessible as row.Button
  Button, text: "B"       // another labeled child
}
```

**Anonymous Children and Auto-UID**

Children without a label receive an auto-generated UID. This enables individual keyframing without label collisions:

```animatix
col: Col {
  Rect, color: red        // anonymous: auto-UID assigned
  Circle, color: blue     // anonymous: different auto-UID
}

#1s
col[0].scale = 1.5 [1s]   // keyframe first anonymous child by index
col[1].opacity = 0.5 [1s] // keyframe second anonymous child
```

Auto-UIDs are stable within a session, allowing reliable referencing of anonymous children across keyframes.

**Nesting Containers**

Containers can nest arbitrarily deep, with transforms accumulating down the hierarchy:

```animatix
scene: Group {
  inner: Group {
    leaf: Circle, at: (50, 0)
  }
}
```

The `leaf` circle inherits transforms from both `inner` and `scene`. Rotating `scene` 90 degrees causes `leaf` to orbit accordingly.

---

## 8. Reactive System

> **Status: Planned/Deferred** — `always`, `loop`, and reactive patterns are parsed but not fully evaluated.

**Always Blocks**  
Code inside always blocks evaluates every frame. Useful for physics, live data, and continuous motion.
```animatix
always {
  let x = slider_value
  ball.position = (x, x^2)
  label.text = format("y = {x}", x)
}
```

**Conditionals**  
Inline conditionals work within expressions.
```animatix
color = if value > 0 { green } else { red }
```

**Loops**  
Loops can be infinite, time-bounded, or count-bounded.
```animatix
loop { ... }
loop 5s { ... }
loop 3 times { ... }
```

**Loop Control**  
Labeled loops can be stopped.
```animatix
job: loop 5 times { ... }
stop job
```

---

## 9. Components

> **Status: Planned/Deferred** — `ComponentDef` and related patterns are parsed but not fully evaluated.

### Definition
Reusable actors can be defined and parameterized. To make an actor available to other files, it must be exported using the `pub` keyword.

```animatix
pub actor Button(text: "Click", color: Color) {
  bg: Rect, color: color
  label: Text, text: text
}
```

### Import
Use the `import` keyword to load external files. Exported actors become available via dot notation or directly if imported.

```animatix
import "button.actor.amx"

btn: Button, text: "Submit"
```

**Lifecycle Hooks**  
Components can define automatic behaviors.
```animatix
on appear { ... }
on disappear { ... }
```

**Custom Actions**  
Components can define callable actions.
```animatix
action collapse(param1: Number) { ... }
collapse btn1
```

---

## 10. Math & Graphs

> **Status: Active Stage 3** — Graph and plot functionality uses closure-based evaluation.

### Graph Container

The `Graph` primitive is a container that maps logical mathematical domains to physical screen bounds. It does not render directly but establishes the coordinate system for its child plots.

**Properties:**
- `x_range`: Tuple (min, max) defining the logical x-domain
- `y_range`: Tuple (min, max) defining the logical y-domain
- `width`: Number (optional, physical width in pixels)
- `height`: Number (optional, physical height in pixels)

```animatix
graph: Graph, x_range: (-5, 5), y_range: (-10, 30)
```

### CartesianPlot

A child plot that renders a function in Cartesian coordinates. The function is defined as a closure using arrow syntax `(args) => expression`.

**Properties:**
- `func`: Closure `(x) => expression` defining the mathematical function to plot
- `color`: Color (optional, defaults to white)
- `width`: Number (optional, stroke width)

```animatix
parabola: CartesianPlot, func: (x) => x^2 + 3, color: red
```

### PolarPlot

A child plot that renders a function in polar coordinates `r(theta)`.

**Properties:**
- `func`: Closure `(theta) => expression` defining the radius as a function of angle
- `color`: Color (optional, defaults to white)
- `width`: Number (optional, stroke width)

```animatix
spiral: PolarPlot, func: (t) => t, color: blue
```

### Closure Syntax

Functions are defined using closure syntax that is natively parsed in the AST:

```animatix
(x) => x^2              // Single parameter
(x, y) => x + y          // Multiple parameters
(t) => sin(t) * cos(t)   // Mathematical expressions
```

Closures capture the lexical environment, enabling variable references from the surrounding scope.

### Math Functions

Built-in support for standard math (implemented):
- `sin(x)`, `cos(x)`, `tan(x)`
- `sqrt(x)`, `abs(x)`, `log(x)`, `exp(x)`
- `pow(base, exp)`, `lerp(a, b, t)`

**Text Formatting**  
Dynamic text using format strings.
```animatix
format("Value: {val:.2f}")
```

---

## 11. Namespacing & Access

**Internal Scope**  
Inside a container, children can be accessed by bare name.
```animatix
container: Group {
  child: Type
  appear child
}
```

**External Scope**  
Outside a container, use dot notation.
```animatix
morph container.child into target [2s]
```

**Query Access**  
Access children by index or type.
```animatix
container.children[0]
container.children[Type: Tex]
```

---

## 12. Editor & Hot-Reloading

File watching and hot-reload for realtime evaluation is intentionally **postponed**. The current architecture prioritizes a clean, stable evaluation model over live file watching. This decision keeps the core evaluation semantics simple and avoids the complexity of invalidation cascades that would arise from mid-evaluation file modifications. 

Future versions will introduce a robust UI/Hot-Reload system that assigns a unique `FileId` to each module in a central module graph, preventing circular dependencies and allowing safe, incremental re-evaluation.
