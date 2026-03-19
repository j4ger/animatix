# Animatix Language Specification v1.0

---

## 1. File Types

**Scene Files (.amx)**  
Main animation scripts. Contain keyframes, actors, and timeline definitions.

**Component Files (.actor.amx)**  
Reusable actor definitions. Contain parameters, internal structure, and custom actions.

**Library Files (.lib.amx)**  
Collections of utility functions, math helpers, and constants.

---

## 3. Core Syntax Symbols

**Let**  
Used for declaring objects, variables and actors (non-rendered values).  
Example: let x = 0

**Colon ( : )**  
Shorthand for binding actors to a label, and put it to scene.
Example: btn: Button, text: "OK"

**Hash ( # )**  
Marks a keyframe in the timeline.  
Example: #0s, #2.5s, #@10s (absolute timestamp)

**Curly Brackets ( { } )**  
Used for container children, arrays, and block scopes.  
Example: Row { Item1, Item2 }

**Square Brackets ( [ ] )**  
Used for action modifiers (duration, easing).  
Example: [2s, ease: bounce]

**Equals ( = )**  
Used for property assignment (instant change) or variable binding.  
Example: btn.color = red

**Comma ( , )**  
Separates object properties.  
Example: Type, prop: val, prop: val

**Space (   )**  
Separates action verbs from arguments.  
Example: move btn to (100, 100)

**Dot ( . )**  
Used for nested access (namespacing).  
Example: container.child

---

## 4. Declarations

**Actor Declaration**  
Actors are rendered objects. They must be declared with a label and a type.

    label: Type, property: value, at: (x, y)

**Variable Declaration**  
Variables are computed values. They are not rendered directly.

    let name = expression

**Re-Declaration (Morph Trigger)**  
If an existing label is declared again at a later keyframe, the engine morphs the existing actor to the new definition.

    #0s
    btn: Button, text: "OK"

    #2s
    btn: Button, text: "Submit" [2s]

    #10s
    btn: another_button // morph into another pre-defined object

---

## 5. Timeline & Keyframes

**Absolute Keyframes**  
Marks a specific time in seconds or milliseconds.

    #@0s
    #@2.5s
    #@500ms

**Relative Keyframes**  
Marks a time relative to the previous keyframe.

    #1s

**Parallel Actions**  
Actions listed under the same keyframe execute simultaneously.

    #2s
    move A to (100, 100) [2s]
    color B to red [2s]

---

## 6. Actions & Modifiers

**Action Invocation**  
Actions use space-separated verbs.

    fade-in actor [1s]
    fade-out actor
    move actor (x, y)
    pulse actor

**Modifiers**  
Modifiers are enclosed in square brackets immediately following the action.

    [duration]
    [ease: function]
    [delay: time]

**Common Modifiers**

    [2s]                      Duration
    [ease: ease-in-out]       Easing curve
    [path: arc]               Morph interpolation path

---

## 7. Morphing System

**Automatic Morph**  
Re-declaring an actor at a new keyframe triggers a morph transition.

    #0s
    circle: Circle, at: (0, 0)

    #2s
    circle: Circle, at: (100, 100) [2s]

**Morph Strategies**  
Users can hint how the engine should handle topology changes.

    [strategy: auto]          Engine decides (default)
    [strategy: match]         Force point alignment
    [strategy: fade]          Cross-fade (ReplacementTransform)

**Instant Change**  
Use zero duration or property assignment for instant updates.

    btn: Button, text: "New" [0s]
    btn.text = "New"

---

## 8. Containers & Layout

**Container Types**

    Row                     Horizontal layout
    Col                     Vertical layout
    Grid                    2D grid layout
    Stack                   Overlapping layout

**Children Declaration**  
Children are declared inline within curly brackets. They do not require labels unless accessed externally.

    row: Row, gap: 10 {
      Button, text: "A"
      Button, text: "B"
    }

---

## 9. Reactive System

**Always Blocks**  
Code inside always blocks evaluates every frame. Useful for physics, live data, and continuous motion.

    always {
      let x = slider_value
      ball.position = (x, x^2)
      label.text = format("y = {x}", x)
    }

**Conditionals**  
Inline conditionals work within expressions.

    color = if value > 0 { green } else { red }

**Loops**  
Loops can be infinite, time-bounded, or count-bounded.

    loop { ... }
    loop 5s { ... }
    loop 3 times { ... }

**Loop Control**  
Labeled loops can be stopped.

    job: loop 5 times { ... }
    stop job

---

## 10. Components

**Definition**  
Components define reusable actors with parameters.

    Button(text: "Click", color: Color) {
      pub bg: Rect, color: color
      pub label: Text, text: text
    }

**Usage**  
Import and instantiate like standard actors.

    use Button from "button.actor"
    btn: Button, text: "Submit"

**Lifecycle Hooks**  
Components can define automatic behaviors.

    on appear { ... }
    on disappear { ... }

**Custom Actions**  
Components can define callable actions.

    action collapse(param1: Number) { ... }
    collapse btn1

---

## 11. Math & Graphs

**2D Graph Actor**  
Container for plots with coordinate systems.

    graph: 2dGraph, x_range: {-5, 5}, y_range: {-10, 30}

**Plots**  
Functions bound to a graph actor.

    plot: graph.plot, func: "x^2 + 3", color: red

**Math Functions**  
Built-in support for standard math.

    sin, cos, tan
    sqrt, abs, log, exp
    lerp(a, b, t)

**Text Formatting**  
Dynamic text using format strings.

    format("Value: {val:.2f}")

---

## 12. Namespacing & Access

**Internal Scope**  
Inside a container, children can be accessed by bare name.

    container: Group {
      child: Type
      appear child
    }

**External Scope**  
Outside a container, use dot notation.

    morph container.child into target [2s]

**Import to Scope**  
Bring specific children into local scope.

    use container.{child1, child2}
    morph child1 into target [2s]

**Query Access**  
Access children by index or type.

    container.children[0]
    container.children[Type: Tex]


