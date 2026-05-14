# Multi-Scene Composition System — Final Design & Implementation Plan

> Status: Final Design Document  
> Last updated: 2026-05-14

---

## 1. Overview

This document specifies the **Multi-Scene Composition** feature for Animatix. It enables users to compose multi-scene concept explanatory videos without external video editors, while keeping single-scene `.amx` files fully backward-compatible.

### Core Design Decisions

| Decision | Rationale |
|---|---|
| `# SceneName` syntax | Consistent with `#0s` keyframes; no indentation penalty; parser unambiguous |
| Keep `#0s` / `#+1s` for keyframes | Zero migration cost; parser distinguishes by token after `#` |
| Flat scene bodies | Scene contents are not wrapped in braces; consistent with keyframe philosophy |
| `play` as `Stmt::Play` | Scene-level control flow, not actor-targeting; cleaner than shoehorning into `Action` |
| `play` at scene level (not inside keyframes) | Eliminates "mid-scene interrupt" ambiguity; scene plays out, then transitions |
| Per-scene independent timelines | Existing `Timeline` unchanged; composition is a thin orchestration layer |
| Explicit `play` overrides implicit order | Deterministic; explicit beats implicit |

---

## 2. Syntax Specification

### 2.1 Scene Declaration

```animatix
# SceneName
<statements...>
```

- `#` followed by an **identifier** (not a number or `+`) declares a scene.
- The scene body consists of all subsequent top-level statements until:
  - Another `#` (scene or keyframe)
  - EOF
- Scene names must be unique within a file.
- Scenes are **top-level only**. Inside containers, `sequence`, `stagger`, etc., `#` retains its keyframe meaning.

**Example:**

```animatix
config { resolution: (1280, 720) }

# Intro
#0s
title: Text, text: "Welcome", font_size: 48
#1s
fade-in title [500ms]

# Diagram
#0s
graph: CartesianPlot, func: (x) => x^2, color: red

# Outro
#0s
thanks: Text, text: "Thank you!"
#1s
fade-in thanks [400ms]
```

### 2.2 `play` Statement

```animatix
play SceneName [transition, duration]
```

- `play` is a **scene-level statement** defining the successor scene.
- It appears **inside a scene body**, typically at the end.
- The transition begins when the current scene's natural duration ends.
- If omitted, the next scene in declaration order is used with a hard cut.
- A scene without `play` and not followed by another scene simply ends.

**Modifiers:**
- First bare time literal = transition duration (default: `0ms`)
- `transition: <type>` — transition type (default: `cut`)
- Supported transitions (Phase 1): `cut`, `fade`, `wipe-left`, `wipe-right`, `wipe-up`, `wipe-down`

**Example:**

```animatix
# Intro
#0s
title: Text, text: "Welcome"
#1s
fade-in title [500ms]

play Diagram [fade, 300ms]

# Diagram
#0s
graph: CartesianPlot, func: (x) => x^2

play Outro [wipe-left, 200ms]

# Outro
#0s
thanks: Text, text: "Thank you!"
```

### 2.3 Backward Compatibility

If a file contains **no** `# SceneName` declarations, it is a single-scene file. All existing syntax, semantics, and behavior are preserved exactly. The parser produces the same AST as before; the timeline builder detects the absence of scenes and follows the existing single-timeline path.

```animatix
// Single-scene file — unchanged behavior
config { resolution: (1280, 720) }

#0s
title: Text, text: "Hello"
#1s
fade-in title [500ms]
```

### 2.4 Scene-Scoped Configuration

A scene may contain a `config` block. Settings are scoped to that scene.

```animatix
# Intro
config { colorscheme: "editorial-dark" }
#0s
title: Text, text: "Welcome"
```

Rules:
- `resolution`: if omitted inside a scene, inherits from the file-level `config`.
- `colorscheme`, `dynamic_layout`, `background_color`: scoped per scene.
- File-level `config` (before the first scene) is the default for all scenes.

### 2.5 Imports and Visibility

Top-level statements outside any scene (imports, `pub let`, `pub component`) are **shared prelude** — visible to all scenes.

```animatix
import "./theme.amx" as theme

pub let accent = theme.accent

# Intro
#0s
title: Text, text: "Welcome", color: accent
```

Imports inside a scene are scoped to that scene.

`pub component` and `pub let` at file top level are visible to all scenes. Inside a scene, `pub` has no cross-scene effect.

### 2.6 Cross-File Scene Composition (Phase 2)

Scenes can be defined in separate `.amx` files and referenced via import:

```animatix
// main.amx
import "./intro.amx" as intro
import "./diagram.amx" as diagram
import "./outro.amx" as outro

# Main
#0s
play intro.Intro [fade, 300ms]
```

When `import ... as` is used in a file that also contains scene declarations, the imported module's scenes are accessible via qualified names (`module.SceneName`).

Unaliased imports (`import "./intro.amx"`) flatten the imported file's scenes into the global namespace.

> Phase 2 is out of scope for initial implementation. The syntax is reserved.

### 2.7 Source Formatting

`# SceneName` and `play` follow existing flat formatting rules:

```animatix
# Intro
#0s
title: Text, text: "Welcome"

#1s
fade-in title [500ms]

play Diagram [fade, 300ms]
```

- `# SceneName` on its own line.
- Scene body statements each on their own line, **not indented**.
- Blank line separates consecutive scene declarations.
- `play` on its own line at scene body level.

---

## 3. Data Structures

### 3.1 New AST Variants

```rust
// In crates/animatix/src/ast.rs

#[derive(Clone, Debug, PartialEq)]
pub enum Stmt {
    // ... existing variants ...

    /// Scene declaration: # SceneName
    Scene {
        name: String,
        config: Vec<Property>,
        body: Vec<Stmt>,
        span: Option<Span>,
    },

    /// Play statement: play SceneName [transition, duration]
    Play {
        scene_name: String,
        transition: Option<Transition>,
        span: Option<Span>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub transition_type: TransitionType,
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TransitionType {
    Cut,
    Fade,
    WipeLeft,
    WipeRight,
    WipeUp,
    WipeDown,
}
```

### 3.2 Composition

```rust
// New module: crates/animatix/src/composition.rs

use std::collections::BTreeMap;

pub struct Scene {
    pub name: String,
    pub config: Vec<Property>,
    pub timeline: Timeline,
    pub duration_s: f64,
    pub source_span: Option<Span>,
}

pub struct SceneEdge {
    pub to_scene: String,
    pub transition: Transition,
}

pub struct Composition {
    pub scenes: BTreeMap<String, Scene>,
    /// Default order for scenes without explicit `play`
    pub declaration_order: Vec<String>,
    /// Explicit play edges: scene_name → edge
    pub edges: BTreeMap<String, SceneEdge>,
    /// Pre-computed global timeline
    pub global_duration_s: f64,
    /// scene_name → start time in global timeline
    pub scene_start_times: BTreeMap<String, f64>,
}

pub struct CompositionFrame {
    pub scene_name: String,
    pub local_time_s: f64,
    pub transition_blend: Option<TransitionBlend>,
}

pub struct TransitionBlend {
    pub from_scene: String,
    pub to_scene: String,
    pub progress: f64,  // 0.0 = fully from, 1.0 = fully to
    pub transition_type: TransitionType,
}
```

---

## 4. Pipeline Design

### 4.1 Parsing

**Parser rules:**
- At top level: `#` + identifier = `Stmt::Scene`
- At top level: `#` + digit/`+` = `Stmt::Keyframe` (existing)
- Inside a scene body: same rules as top level, but `scene` is not a keyword here
- `play` keyword: parses as `Stmt::Play` when followed by an identifier

The parser produces a flat list of `Stmt` where `Stmt::Scene` has `body: Vec<Stmt>` containing the scene's statements.

**Pseudo-grammar:**
```
top_level_stmt := scene_decl | keyframe | config | import | let_decl | actor_decl | assignment | sequence | stagger | always | for_loop | if_stmt | component_def | comment | play_stmt

scene_decl := "#" IDENT (config_block)? stmt*  // body until next "#" or EOF

play_stmt := "play" IDENT (modifier_list)?
```

### 4.2 Module Expansion

`ModuleGraph::load_program()` behavior:
1. Parse file into raw statements.
2. If any `Stmt::Scene` exists:
   - Top-level statements not inside a scene = shared prelude.
   - Extract `Stmt::Scene` blocks.
   - Collect `play` statements into edge map.
3. If no `Stmt::Scene`: existing behavior (single implicit scene).

### 4.3 Timeline Building

New function: `Composition::build(statements, namespaces) -> BuildReport<Composition>`

```
1. Separate shared_prelude and scene_blocks from statements.

2. For each scene block:
   a. Merge shared_prelude with scene body.
   b. Build Timeline::build_with_diagnostics(merged_body, &namespaces).
   c. Compute scene.duration_s = timeline.duration_seconds().
   d. Extract Play stmt (if any) to build edges map.
   e. Store Scene { name, config, timeline, duration_s }.

3. Build declaration_order from scene declaration sequence.

4. Resolve play edges:
   - For each scene with Play:
     edge[scene.name] = SceneEdge { to_scene: play.scene_name, transition: play.transition }
   - For scenes without Play, edge points to next scene in declaration_order.
   - Last scene has no edge (or edge to nothing).

5. Compute global timeline:
   current_time = 0.0
   for each scene in walk order (following edges):
     scene_start_times[scene] = current_time
     current_time += scene.duration_s
     if edge.transition.duration_ms > 0:
       current_time -= edge.transition.duration_ms / 1000.0  // overlap

   global_duration_s = current_time
```

**Edge cycle detection:** If `play` edges form a cycle, emit a diagnostic. The builder walks edges with cycle detection and stops at the first repeat.

### 4.4 Frame Evaluation

```rust
impl Composition {
    pub fn evaluate(&self, global_time_s: f64, dims: SceneDimensions) -> CompositionFrame {
        // Find active scene using scene_start_times
        let (scene_name, local_time_s, in_transition) = self.map_global_time(global_time_s);

        if let Some((from, to, progress, transition_type)) = in_transition {
            CompositionFrame {
                scene_name: from.clone(),
                local_time_s,
                transition_blend: Some(TransitionBlend {
                    from_scene: from,
                    to_scene: to,
                    progress,
                    transition_type,
                }),
            }
        } else {
            CompositionFrame {
                scene_name,
                local_time_s,
                transition_blend: None,
            }
        }
    }
}
```

### 4.5 Render Pipeline

#### Phase 1: Hard Cuts Only

No renderer changes. `Composition::evaluate()` returns a single scene + local time. The renderer calls `scene.timeline.evaluate(local_time_s, dims)` exactly as today.

```
render_frame(global_time_s):
  frame = composition.evaluate(global_time_s, dims)
  scene = composition.scenes[&frame.scene_name]
  vello_scene = scene.timeline.evaluate(frame.local_time_s, dims)
  rasterize(vello_scene)
```

#### Phase 2: Transitions

When a transition is active, two scenes are rendered and blended.

**Export pipeline:**
```
if transition_blend:
  texture_a = offscreen_render(scene_a.timeline, local_time_a, dims)
  texture_b = offscreen_render(scene_b.timeline, local_time_b, dims)
  final = blend_textures(texture_a, texture_b, progress, transition_type)
else:
  final = offscreen_render(scene.timeline, local_time, dims)
```

The existing offscreen renderer in `renderer/offscreen.rs` already supports this. For GUI preview, `PreviewSurface` needs a compositing pass (dual texture render + blend shader).

### 4.6 Export Changes

- Single-scene files: existing behavior, no changes.
- Multi-scene files: use `Composition::global_duration_s` for auto-duration. Render frames by sampling `Composition::evaluate()`.
- Parallel frame rendering still works (each thread clones `Composition`).

---

## 5. GUI Design

### 5.1 Document Model

```rust
pub struct DocumentSession {
    // ... existing fields ...
    pub timeline: Option<Timeline>,           // single-scene files
    pub composition: Option<Composition>,     // multi-scene files
    pub active_scene: Option<String>,         // which scene is being edited
    pub global_time_s: f64,                   // current playhead position
    // ...
}
```

`rebuild()`:
- AST has scenes → build `Composition`, set `composition = Some(...)`.
- No scenes → build `Timeline` as before.

### 5.2 Scene List Panel (Left Sidebar)

```
┌─────────────┐
│ Scenes      │
├─────────────┤
│ ▶ Intro     │  ← click to focus
│   Diagram   │
│   Outro     │
├─────────────┤
│ + Add Scene │
└─────────────┘
```

- Click to select active scene.
- Drag to reorder (updates implicit order, re-serializes source).
- Right-click: duplicate, delete, rename.

### 5.3 Composition Timeline (Bottom Panel)

Enhanced scrubber showing scene boundaries:

```
0s        2s        5s        8s       10s
|----Intro----|------Diagram------|--Outro--|
[=======]    [==================] [========]
```

- Scene blocks are color-coded.
- Transition regions shown as gradients.
- Playhead operates on **global time**.
- Click inside a scene → seek to that local time.
- Click a boundary → select transition for editing.

### 5.4 Preview

- Shows active scene at current global time.
- During playback, seamlessly switches at scene boundaries.
- Phase 2: shows blended preview during transitions.

### 5.5 Transport Bar

- Time display: `global_time / global_duration` (e.g., `3.2s / 10.5s`).
- Scene name badge next to time.
- Previous/next scene buttons.
- Previous/next keyframe buttons navigate within active scene.

### 5.6 Inspector

**Scene selected:**
- Name, duration
- Config (resolution, colorscheme, background_color)
- `play` target and transition

**Transition selected:**
- Type dropdown
- Duration
- Easing (future)

**Actor selected:**
- Same as today, scoped to active scene's timeline.

### 5.7 Source Write-Back

New `source_edit_v2` edit types:
- `ReorderScenes { new_order: Vec<String> }`
- `SetPlayTarget { scene: String, target: Option<String> }`
- `SetTransition { from: String, transition: Option<Transition> }`
- `RenameScene { old: String, new: String }`

All edits mutate the AST and re-serialize via `to_source.rs`.

---

## 6. Analyzer / LSP

### Grammar (`tree-sitter-animatix`)

Add rules for:
- `scene_declaration`: `#` `identifier` (`config_block`)? `statement*`
- `play_statement`: `play` `identifier` `modifier_list`?

### Diagnostics

- Duplicate scene names within a file
- `play` references non-existent scene
- Scene edges forming a cycle
- `play` inside keyframes (rejected — scene-level only)
- Invalid transition types

### Completions

- At top level after `#`: suggest existing scene names (for navigation)
- After `play`: suggest scene names
- Inside scene body: same completions as today

### Symbols

Scenes appear as document symbols for outline view.

---

## 7. Implementation Plan

### Phase 1: Core Parser & AST (Week 1)

**Goal:** Parse scenes and `play`, build per-scene timelines.

| Task | Files | Notes |
|---|---|---|
| Add AST variants | `ast.rs` | `Stmt::Scene`, `Stmt::Play`, `Transition`, `TransitionType` |
| Update parser | `parser.rs` | Top-level `# ident` = scene; `play ident [mods]` = play stmt |
| Update serializer | `to_source.rs` | Serialize `Scene` and `Play` |
| Update source formatter spec | `source-format-spec.md` | Document flat scene formatting |
| Tree-sitter grammar | `tree-sitter-animatix/` | Add scene_declaration, play_statement rules |

### Phase 2: Composition Engine (Week 1-2)

| Task | Files | Notes |
|---|---|---|
| Create composition module | `src/composition.rs` (new) | `Scene`, `Composition`, `CompositionFrame` structs |
| Build composition from AST | `composition.rs` | Extract scenes, build timelines, resolve edges |
| Time mapping | `composition.rs` | `map_global_time()` — global → (scene, local, transition) |
| Edge cycle detection | `composition.rs` | Diagnostic on cycles |
| Integration with module system | `module.rs` | Handle scenes during module expansion |
| Update Timeline::build path | `timeline/build/mod.rs` | Detect scenes → use Composition path |

### Phase 3: CLI Export (Week 2)

| Task | Files | Notes |
|---|---|---|
| Multi-scene export | `renderer/video.rs`, `renderer/gif.rs` | Use Composition::evaluate for frame sampling |
| Auto-duration for compositions | `export.rs` | Use Composition::global_duration_s |
| Parallel rendering | `renderer/video.rs` | Clone Composition per thread (scenes are Clone) |

### Phase 4: GUI — Scene List & Active Scene (Week 2-3)

| Task | Files | Notes |
|---|---|---|
| Scene list panel | `app/panels/scene_list.rs` (new) | List, select, reorder, add, delete, rename |
| DocumentSession changes | `document.rs` | Hold `Option<Composition>`, `active_scene` |
| Rebuild path | `document.rs` | Build composition when scenes detected |
| Preview active scene | `preview_surface.rs` | Evaluate active scene's timeline at global time |
| Transport bar updates | `shell/transport_bar.rs` | Show global time, scene name, prev/next scene |

### Phase 5: GUI — Composition Timeline (Week 3)

| Task | Files | Notes |
|---|---|---|
| Scene blocks on scrubber | `app/panels/timeline.rs` | Draw colored scene blocks |
| Scene boundary interactions | `timeline.rs` | Click to seek, click boundary to select transition |
| Transition editing | `inspector/` | Type dropdown, duration input |

### Phase 6: GUI — Source Write-Back (Week 3-4)

| Task | Files | Notes |
|---|---|---|
| Source edit types | `source_edit_v2.rs` | ReorderScenes, SetPlayTarget, SetTransition, RenameScene |
| Edit application | `source_edit.rs` | Apply edits to AST |
| Scene list actions | `scene_list.rs` | Wire drag-to-reorder, rename, delete to source edits |

### Phase 7: Transitions (Phase 2 Feature) (Week 4-5)

| Task | Files | Notes |
|---|---|---|
| Dual offscreen render | `renderer/offscreen.rs` | Render two scenes to textures |
| Texture blending | `renderer/composite.rs` (new) | Blend based on transition type and progress |
| Export with transitions | `renderer/video.rs`, `gif.rs` | Composite during export |
| GUI preview transitions | `preview_surface.rs` | Composite in preview |

### Phase 8: Cross-File Scenes (Phase 3 Feature) (Future)

| Task | Files | Notes |
|---|---|---|
| Qualified scene names | `module.rs` | `module.SceneName` resolution |
| Import scene references | `parser.rs`, `module.rs` | Treat imported files as scene libraries |
| GUI project explorer | `app/panels/` | Show referenced scene files |

---

## 8. Testing Strategy

### Parser Tests
- Single-scene file parses as before
- Multi-scene file with `# SceneName` extracts correct scene bodies
- `#` inside container still parses as keyframe
- `play` statement parsing with and without modifiers
- Invalid transition type reports diagnostic

### Composition Tests
- Two scenes with no `play` — declaration order
- Scene with `play` to specific scene
- Transition duration reduces total composition duration
- Cycle detection emits diagnostic
- Scene duration computed from timeline

### Integration Tests
- Export multi-scene file produces correct frame count
- Global time → local time mapping is correct
- Active scene selection works in GUI
- Source write-back preserves formatting

### Regression Tests
- All existing examples still build and export identically
- Single-scene files follow exact same path as before

---

## 9. Risk Assessment

| Risk | Likelihood | Mitigation |
|---|---|---|
| Parser ambiguity between `# Scene` and `#0s` | Low | `#` + ident vs `#` + digit/`+` is unambiguous |
| Breaking existing files | Low | No scenes = single-scene path, untouched |
| Performance: cloning many timelines | Medium | `Timeline::clone()` already exists; test with large compositions |
| GUI complexity: dual rendering | Medium | Phase 1 avoids this; Phase 2 reuses offscreen infrastructure |
| Source write-back for flat scenes | Medium | Similar to keyframe write-back; leverage existing patterns |

---

## 10. Example Files

### Simple Multi-Scene

```animatix
config { resolution: (1280, 720) }

# Intro
config { colorscheme: "editorial-dark" }
#0s
title: Text, text: "Welcome", font_size: 48, anchor: scene.center
#1s
fade-in title [500ms]

play Diagram [fade, 300ms]

# Diagram
#0s
graph: Graph, x_domain: (-5, 5), y_domain: (-10, 30), size: (400, 400)
curve: CartesianPlot, func: (x) => x^2, color: red

play Outro [wipe-left, 200ms]

# Outro
#0s
thanks: Text, text: "Thank you!", anchor: scene.center
#1s
fade-in thanks [400ms]
```

### With Imports

```animatix
import "./theme.amx" as theme

# Intro
#0s
title: Text, text: "Welcome", color: theme.accent

# Diagram
#0s
graph: CartesianPlot, func: (x) => x^2
```

### Single Scene (Backward Compatible)

```animatix
config { resolution: (1280, 720) }

#0s
title: Text, text: "Hello"
#1s
fade-in title [500ms]
```

---

*End of document.*
