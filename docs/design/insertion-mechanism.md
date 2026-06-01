# Design: Unified Primitive & Action Insertion Mechanism

## Problem Statement

The GUI currently has **three disconnected ways** to put things into the editor:

1. **Action Palette** (`shell/action_palette.rs`) — overlay with hardcoded action buttons. Does **raw text surgery** on `source_text`, bypassing the AST entirely. Only works when an actor is pre-selected.
2. **Completion Popup** (`completion_popup.rs`) — `Ctrl+Space` dropdown driven by the analyzer. Has snippets, but they are **hardcoded in `completer.rs`**, insert raw text, and don't use `SourceEdit`.
3. **Inspector / DocumentController** — uses `SourceEdit` for semantic edits (property changes, actor creation, keyframe insertion) but has **no path for inserting actions** into keyframes.

This means:
- Adding a new primitive or action requires touching **multiple disconnected files**.
- Text surgery in the action palette is fragile and doesn't respect AST structure.
- There's no unified "I want to add something" UX — the user must know which hotkey/panel to use.

---

## Design Goals

1. **One unified insertion UX** — a single palette/command that can insert primitives, actions, or snippets.
2. **Semantic correctness** — all insertions go through `SourceEdit` → AST → `stmts_to_source()`, never raw text surgery.
3. **Auto-extensibility** — new primitives (added to `PRIMITIVES`) and new actions (added to the action registry) automatically appear in the UI without extra wiring.
4. **Context-awareness** — the palette shows different things depending on where the cursor is (top-level vs. inside a keyframe body).
5. **Keyboard-first** — fuzzy-searchable overlay, like VS Code's command palette or Figma's quick actions.

---

## Architecture

### Three Layers

```
┌─────────────────────────────────────────────────────────────┐
│  Layer 3: UI                                                 │
│  ─────────                                                   │
│  InsertionPalette (overlay)                                  │
│  • Fuzzy search across primitives, actions, snippets         │
│  • Context-aware default scope                               │
│  • Keyboard: Ctrl+Shift+P (or / in editor)                   │
├─────────────────────────────────────────────────────────────┤
│  Layer 2: Request Translation                                │
│  ─────────────────────                                       │
│  InsertionRequest enum                                       │
│  • Primitive { type_name, suggested_label }                  │
│  • Action    { verb, targets }                               │
│  • Snippet   { text }                                        │
│  ↓                                                           │
│  GuiShell maps InsertionRequest → SourceEdit                 │
├─────────────────────────────────────────────────────────────┤
│  Layer 1: Semantic Edits                                     │
│  ────────────────                                            │
│  SourceEdit enum (extended)                                  │
│  • InsertActor   (already exists)                            │
│  • InsertAction  (NEW)                                       │
│  ↓                                                           │
│  apply_edit() → AST mutation → stmts_to_source()             │
└─────────────────────────────────────────────────────────────┘
```

**Critical boundary rule:** `source_edit/` knows nothing about the GUI. `app/insertion.rs` (the bridge) knows about both `source_edit/` and the GUI stores, but performs no AST mutation itself — it only builds `SourceEdit` values.

---

## Layer 1: Extend `SourceEdit` with `InsertAction`

### New variant

```rust
// crates/animatix-gui/src/source_edit/mod.rs
pub enum SourceEdit {
    // ... existing variants ...

    /// Insert an action statement at the exact keyframe for `time_s`.
    ///
    /// Semantics:
    /// 1. If a keyframe exists within ε (50ms) of `time_s`, append to it.
    /// 2. Otherwise create a new keyframe at `time_s`.
    /// 3. Existing keyframes' absolute times are NEVER shifted.
    InsertAction {
        verb: String,
        targets: Vec<String>,
        args: Vec<Expr>,
        modifiers: Vec<Modifier>,
        time_s: f64,
    },
}
```

### New module: `source_edit/action_edits.rs`

```rust
/// Tolerance for matching an existing keyframe (50ms).
const TIME_EPSILON_S: f64 = 0.05;

pub(super) fn insert_action(
    stmts: &mut Vec<Stmt>,
    verb: &str,
    targets: &[String],
    args: &[Expr],
    modifiers: &[Modifier],
    time_s: f64,
) -> bool {
    let action = Stmt::Action(
        Action {
            verb: verb.into(),
            targets: targets.to_vec(),
            args: args.to_vec(),
            modifiers: modifiers.to_vec(),
            byte_span: None,
        },
        None,
    );

    // ── 1. Exact match: find keyframe within ε of time_s ──
    let mut current_time = 0.0f64;
    for stmt in stmts.iter_mut() {
        match stmt {
            Stmt::Keyframe { time, body, .. } => {
                current_time = time_to_seconds(time);
                if (current_time - time_s).abs() < TIME_EPSILON_S {
                    body.push(action);
                    return true;
                }
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                current_time += time_to_seconds(offset);
                if (current_time - time_s).abs() < TIME_EPSILON_S {
                    body.push(action);
                    return true;
                }
            }
            _ => {}
        }
    }

    // ── 2. No match — create keyframe at time_s ──
    let prev_time_s = find_prev_keyframe_time(stmts, time_s);
    let delta_s = time_s - prev_time_s;

    // If we're essentially on top of the previous keyframe, append there
    // to avoid micro-fragmentation.
    if delta_s < TIME_EPSILON_S {
        return append_to_keyframe_at_time(stmts, prev_time_s, action);
    }

    // ── 3. Choose keyframe style: inherit from preceding keyframe ──
    let style = keyframe_style_before(stmts, time_s);
    let insert_idx = find_keyframe_insertion_point(stmts, prev_time_s);

    // Wrap leading declarations in #0s if inserting before any keyframe
    wrap_leading_decls_in_zero_keyframe(stmts, insert_idx, prev_time_s);

    match style {
        KeyframeStyle::Absolute => {
            stmts.insert(insert_idx, Stmt::Keyframe {
                time: Time::Seconds(time_s),
                body: vec![action],
                span: None,
            });
            // Following relative keyframes must be adjusted because
            // their base time changed.
            adjust_following_relative_keyframe(stmts, insert_idx + 1, delta_s);
        }
        KeyframeStyle::Relative => {
            let offset = if delta_s < 1.0 {
                Time::Milliseconds((delta_s * 1000.0).round() as u64)
            } else {
                Time::Seconds(delta_s)
            };
            // Adjust next relative keyframe to preserve its absolute time
            adjust_following_relative_keyframe(stmts, insert_idx, delta_s);
            stmts.insert(insert_idx, Stmt::RelativeKeyframe {
                offset,
                body: vec![action],
                span: None,
            });
        }
    }
    true
}

/// Determine the style (absolute vs relative) of the keyframe immediately
/// preceding `time_s`. New keyframes inherit this style so that relative
/// chains stay relative, absolute breakpoints stay absolute.
fn keyframe_style_before(stmts: &[Stmt], time_s: f64) -> KeyframeStyle {
    let mut current_time = 0.0f64;
    let mut style = KeyframeStyle::Absolute; // default

    for stmt in stmts {
        match stmt {
            Stmt::Keyframe { time, .. } => {
                current_time = time_to_seconds(time);
                style = KeyframeStyle::Absolute;
            }
            Stmt::RelativeKeyframe { offset, .. } => {
                current_time += time_to_seconds(offset);
                style = KeyframeStyle::Relative;
            }
            _ => {}
        }
        if current_time > time_s {
            break;
        }
    }
    style
}

enum KeyframeStyle { Absolute, Relative }
```

> **Note:** `find_prev_keyframe_time`, `find_keyframe_insertion_point`, `wrap_leading_decls_in_zero_keyframe`, `adjust_following_relative_keyframe`, and `append_to_keyframe_at_time` are extracted from `keyframe_edits.rs` to `ast_utils.rs` so both modules share one canonical implementation.

---

## Timeline Adjustment Semantics

This is the most critical UX surface. When a user inserts an action at 0.5s and the existing timeline is:

```amx
#0s
    box: Rect

#+2s
    box.color = red
```

What should happen? The old `action_palette.rs` used `line_for_time`, which returns the **nearest preceding keyframe** — silently inserting the action into `#0s` so it executes at 0s instead of 0.5s. That is a bug.

### The Six Rules

| # | Rule | Rationale |
|---|------|-----------|
| 1 | **Exact time, never nearest.** | If no keyframe exists at the target time, create one. Never reuse a nearby keyframe. |
| 2 | **Cursor-in-cell wins over playhead.** | If the user is typing inside `#2s`, the insertion targets 2.0s regardless of where the playhead is. |
| 3 | **Style inheritance.** | New keyframes match the style of the immediately preceding keyframe: relative chains stay relative, absolute breakpoints stay absolute. |
| 4 | **Absolute times are sacred.** | Existing events keep their absolute times. Only raw text offsets may be rewritten to preserve semantics. |
| 5 | **No micro-fragmentation.** | If the target time is within ε (50ms) of an existing keyframe, append to that keyframe instead of creating a redundant one. |
| 6 | **Visual confirmation.** | The new cell appears, the editor scrolls to it, and the status bar says exactly what happened. |

### Style Inheritance Heuristic

New keyframes inherit the syntax style of the keyframe immediately before them:

```rust
fn keyframe_style_before(stmts: &[Stmt], time_s: f64) -> KeyframeStyle {
    // After a RelativeKeyframe  → create RelativeKeyframe
    // After a Keyframe          → create Keyframe (absolute)
    // No previous keyframe      → absolute
}
```

This keeps relative chains intact and makes absolute breakpoints self-documenting.

#### Example A: Between absolute keyframes

Source before:
```amx
#0s
    box: Rect

#2s
    box.color = red
```

User inserts at 0.5s. Previous is `#0s` (absolute) → style = Absolute.

```amx
#0s
    box: Rect

#0.5s
    fade-in box

#2s
    box.color = red
```

No adjustments needed at all. `#2s` stays `#2s`. This is the cleanest case.

#### Example B: After absolute, before relative

Source before:
```amx
#0s
    box: Rect

#+2s
    box.color = red
```

User inserts at 1.5s. Previous is `#0s` (absolute) → style = Absolute.

```amx
#0s
    box: Rect

#1.5s
    fade-in box

#+500ms
    box.color = red
```

`#+2s` becomes `#+500ms` to preserve the 2.0s absolute time. This is the only adjustment needed.

#### Example C: Extending a relative chain

Source before:
```amx
#0s
    box: Rect

#+2s
    box.color = red
```

User scrubs to 2.5s (after the last keyframe), inserts `fade-out box`.

The keyframe before 2.5s is `#+2s` (relative). Style = Relative.

```amx
#0s
    box: Rect

#+2s
    box.color = red

#+500ms
    fade-out box
```

No existing keyframes modified. Clean extension.

### Visual Feedback

When insertion creates or modifies keyframes, the GUI provides immediate feedback:

1. **New cell animates in** — the editor scrolls to the new keyframe cell.
2. **Cell header shows absolute time** — e.g., `0.5s`. The raw syntax is hidden behind the cell abstraction.
3. **Modified keyframes flash briefly** — if an existing relative keyframe had its offset rewritten, its timestamp label briefly glows amber (≈300ms) to indicate "this was adjusted for you."
4. **Status bar message** — e.g., "Inserted `fade-in` at 0.5s. Adjusted next keyframe to `#+1.5s`."
5. **Cursor placement** — for actions, cursor stays at end of inserted line inside the same cell. For primitives, cursor moves to the first editable property.

### Why This Is Elegant

- **Predictable** — users learn: "inserting at time T creates a keyframe at T."
- **Minimal disruption** — at most one existing keyframe's raw text changes.
- **Respects user style** — relative authors stay relative, absolute authors stay absolute.
- **Self-documenting** — the status bar explains any automatic adjustments.
- **Reversible** — deleting the new cell restores the original source text (because `SourceEdit` operates on AST, and re-serialization is deterministic).

---

## Layer 2: `InsertionRequest` — The Bridge

```rust
// crates/animatix-gui/src/app/insertion.rs

/// What the user wants to insert, produced by the palette.
#[derive(Debug, Clone)]
pub enum InsertionRequest {
    /// Insert a primitive actor declaration.
    Primitive {
        type_name: String,
        /// If None, generate a unique label automatically.
        suggested_label: Option<String>,
    },
    /// Insert an action into the current keyframe.
    Action {
        verb: String,
        /// If empty, use the currently selected actor(s).
        targets: Vec<String>,
    },
    /// Insert a raw code snippet.
    Snippet {
        text: String,
    },
}

impl InsertionRequest {
    /// Convert to a `SourceEdit` given current document context.
    pub fn into_source_edit(
        self,
        ctx: &InsertionContext,
    ) -> Option<SourceEdit> {
        match self {
            InsertionRequest::Primitive { type_name, suggested_label } => {
                let label = suggested_label.unwrap_or_else(||
                    ctx.unique_label(&type_name)
                );

                // Delegate to Primitive trait — single source of truth.
                let props = animatix::primitives::find_primitive(&type_name
                )?.default_props(&ctx.scene_dimensions()?);

                match ctx.primitive_target() {
                    InsertionTarget::TopLevel => {
                        Some(SourceEdit::InsertActor {
                            ty: type_name,
                            label,
                            props,
                            container: None,
                            time_s: 0.0,
                        })
                    }
                    InsertionTarget::IntoContainer(container) => {
                        Some(SourceEdit::InsertActor {
                            ty: type_name,
                            label,
                            props,
                            container: Some(container),
                            time_s: 0.0,
                        })
                    }
                    InsertionTarget::KeyframeBody(time_s) => {
                        // Primitives CAN live inside keyframes ("create this actor at time T").
                        // The existing InsertActor::time_s field was designed for this
                        // but never implemented. We now implement it.
                        Some(SourceEdit::InsertActor {
                            ty: type_name,
                            label,
                            props,
                            container: None,
                            time_s,
                        })
                    }
                }
            }
            InsertionRequest::Action { verb, targets } => {
                let targets = if targets.is_empty() {
                    ctx.selected_actors()
                } else {
                    targets
                };
                if targets.is_empty() {
                    return None; // No target — can't insert action
                }

                let time_s = ctx.resolve_action_time();
                Some(SourceEdit::InsertAction {
                    verb,
                    targets,
                    args: vec![],
                    modifiers: vec![default_duration_modifier()],
                    time_s,
                })
            }
            InsertionRequest::Snippet { text } => {
                // Snippets bypass SourceEdit for now — they insert raw text
                // at the cursor position in the cell editor.
                // Future: parse snippet into Stmt list and insert via SourceEdit.
                ctx.insert_snippet(&text);
                None
            }
        }
    }
}
```

### `InsertionTarget` — Where the insertion lands

```rust
pub enum InsertionTarget {
    /// Insert at the top level of the current module/scene.
    TopLevel,
    /// Insert inside the keyframe at the exact given time.
    /// If no keyframe exists, one is created.
    KeyframeBody(f64),
    /// Insert as a child of a container actor.
    IntoContainer(String),
}
```

### `InsertionContext`

```rust
/// Read-only snapshot of everything the insertion system needs.
///
/// This is intentionally cheap to construct — it borrows from stores
/// and contains no owned collections except the actor label set.
pub struct InsertionContext<'a> {
    pub current_time_s: f64,
    pub scene_dimensions: Option<SceneDimensions>,
    pub selected_actors: &'a HashSet<String>,
    pub timeline: Option<&'a Timeline>,
    pub cursor_cell: Option<usize>,
    pub cell_type: Option<CellType>, // Keyframe or Code
}

impl InsertionContext<'_> {
    /// Resolve the target time for an action insertion.
    ///
    /// Priority:
    /// 1. Cursor inside a keyframe cell → that cell's time.
    /// 2. Playback head time.
    ///
    /// Rationale: if the user is typing inside a keyframe, they are
    /// editing THAT moment regardless of where the playhead is.
    pub fn resolve_action_time(&self) -> f64 {
        self.cursor_keyframe_time()
            .unwrap_or(self.current_time_s)
    }

    /// If the cursor is inside a keyframe cell, return its absolute time.
    pub fn cursor_keyframe_time(&self) -> Option<f64> {
        if self.cell_type == Some(CellType::Keyframe) {
            self.cursor_cell.and_then(|idx| self.editor_cell_time_s(idx))
        } else {
            None
        }
    }

    /// Resolve insertion target for a primitive.
    pub fn primitive_target(&self) -> InsertionTarget {
        // If cursor is in a keyframe, place the primitive THERE
        if let Some(time) = self.cursor_keyframe_time() {
            return InsertionTarget::KeyframeBody(time);
        }
        // If a container is selected, place inside it
        if let Some(container) = self.selected_container() {
            return InsertionTarget::IntoContainer(container);
        }
        InsertionTarget::TopLevel
    }

    /// Resolve insertion target for an action.
    pub fn action_target(&self) -> Option<InsertionTarget> {
        let time = self.resolve_action_time();
        Some(InsertionTarget::KeyframeBody(time))
    }

    pub fn selected_container(&self) -> Option<String> {
        self.selected_actors.iter().next().cloned().filter(|sel| {
            self.timeline.is_some_and(|t| {
                t.get_track(sel).is_some_and(|tr| tr.kind.is_container())
            })
        })
    }

    pub fn unique_label(&self, ty: &str) -> String {
        // Delegates to existing helper in app/utils/labels.rs
        crate::app::utils::unique_label(self.timeline, ty)
    }
}
```

---

## Layer 3: `InsertionPalette` — The UI

To avoid a single bloated file, the palette is split into three modules:

```
app/shell/insertion_palette/
├── mod.rs      # Public API, state struct, keyboard event handling
├── items.rs    # Item population from registries, fuzzy filtering, sorting
└── render.rs   # egui rendering: layout, colors, animations
```

### State

```rust
// app/shell/insertion_palette/mod.rs

pub struct InsertionPalette {
    open: bool,
    query: String,
    selected_index: usize,
    mode: PaletteMode,
    /// Items populated once when the palette opens; filtered dynamically.
    all_items: Vec<PaletteItem>,
}

enum PaletteMode {
    Universal,   // Show everything
    Primitives,  // Only primitives
    Actions,     // Only actions
    Snippets,    // Only snippets
}

struct PaletteItem {
    label: String,
    detail: String,
    icon: &'static str,
    color: Color32,
    kind: ItemKind,
    /// Pre-computed fuzzy match score (updated each keystroke).
    score: i64,
}

enum ItemKind {
    Primitive { type_name: &'static str },
    Action { verb: &'static str },
    Snippet { text: String },
}
```

### Item Population (Auto-Extensible)

```rust
// app/shell/insertion_palette/items.rs

fn populate_items() -> Vec<PaletteItem> {
    let mut items = Vec::new();

    // Primitives: single source of truth is PRIMITIVES array in core
    for prim in animatix::primitives::PRIMITIVES.iter() {
        items.push(PaletteItem {
            label: prim.display_name().to_string(),
            detail: format!("{} — {}", prim.type_name(), category_name(prim.category())),
            icon: prim.icon_id(),
            color: category_color(prim.category()),
            kind: ItemKind::Primitive { type_name: prim.type_name() },
            score: 0,
        });
    }

    // Actions: single source of truth is get_action_signatures() in core
    for sig in animatix::timeline::actions::get_action_signatures() {
        items.push(PaletteItem {
            label: sig.name.clone(),
            detail: sig.description.clone(),
            icon: "⚡",
            color: category_color_for_action(&sig.category),
            kind: ItemKind::Action { verb: &sig.name },
            score: 0,
        });
    }

    // Snippets: single source of truth is analyzer's all_snippets()
    for snippet in animatix_analyzer::completer::all_snippets() {
        items.push(PaletteItem {
            label: snippet.label.clone(),
            detail: snippet.detail.unwrap_or_default(),
            icon: egui_phosphor::regular::CODE,
            color: Color32::from_rgb(108, 153, 187),
            kind: ItemKind::Snippet {
                text: snippet.insert_text.unwrap_or(snippet.label),
            },
            score: 0,
        });
    }

    items
}
```

> **Key insight:** No parallel registries, no hardcoded lists. Adding a primitive to `PRIMITIVES`, an action to `get_builtin_actions()`, or a snippet to `all_snippets()` automatically surfaces it in the palette with zero additional wiring.

### Context-Aware Default Mode

```rust
fn default_mode(ctx: &InsertionContext) -> PaletteMode {
    match ctx.cell_type {
        Some(CellType::Keyframe) => PaletteMode::Actions,
        Some(CellType::Code) => PaletteMode::Primitives,
        None => PaletteMode::Universal,
    }
}
```

- If cursor is inside a **keyframe body**, the palette defaults to **Actions** (and the target time is that keyframe's time).
- If cursor is in a **code cell** or at top-level, it defaults to **Primitives**.
- User can tab-switch between modes or prefix query with `>` (primitive), `@` (action), `!` (snippet).

### Keyboard Bindings

| Key | Behavior |
|-----|----------|
| `Ctrl+Shift+P` | Open palette in default mode |
| `/` (in editor) | Open palette in default mode |
| `Tab` | Cycle through modes (Universal → Primitives → Actions → Snippets) |
| `↑ / ↓` | Navigate items |
| `Enter` | Confirm insertion |
| `Esc` | Close |
| `>` | Filter to primitives only |
| `@` | Filter to actions only |
| `!` | Filter to snippets only |

---

## Integration Points

### 1. Remove `action_palette.rs`

Replace `GuiShell::action_palette_ui` with `GuiShell::insertion_palette_ui`. The old action palette's hardcoded `ACTION_CATEGORIES` array becomes unnecessary — the palette is now data-driven from the registries.

### 2. Extend `Command` enum

```rust
// crates/animatix-gui/src/app/commands.rs
pub enum Command {
    // ... existing ...
    OpenInsertionPalette(PaletteMode),
    ExecuteInsertion(InsertionRequest),
}
```

### 3. Wire into `DocumentController`

```rust
impl DocumentController<'_> {
    pub(crate) fn handle_insertion(&mut self, request: InsertionRequest) {
        let ctx = InsertionContext {
            current_time_s: self.preview_store.preview.playback.current_time_s,
            scene_dimensions: self.document_store.source.document.scene_dimensions,
            selected_actors: &self.ui_store.selection.selected_actors,
            timeline: self.document_store.source.document.timeline.as_ref(),
            cursor_cell: self.document_store.source.editor.focused_cell(),
            cell_type: /* derive from editor */,
        };

        if let Some(edit) = request.into_source_edit(&ctx) {
            if let Some(ref mut stmts) = self.document_store.source.document.raw_statements {
                if source_edit::apply_edit(stmts, edit) {
                    let new_source = animatix::to_source::stmts_to_source(stmts);
                    let source_index = animatix::source_index::SourceIndex::build(stmts);
                    self.apply_source(new_source, source_index);
                    self.show_insertion_feedback(&request, &ctx);
                }
            }
        }
    }

    fn show_insertion_feedback(&mut self, request: &InsertionRequest, ctx: &InsertionContext) {
        match request {
            InsertionRequest::Action { verb, .. } => {
                let time = ctx.resolve_action_time();
                self.preview_store.preview.status =
                    format!("Inserted {verb} at {time:.2}s");
            }
            InsertionRequest::Primitive { type_name, .. } => {
                self.preview_store.preview.status =
                    format!("Inserted {type_name}");
            }
            _ => {}
        }
    }
}
```

### 4. Snippets — Keep in Analyzer

Instead of extracting snippets to a new GUI file, keep the canonical list in `animatix-analyzer/src/completer.rs` and expose it via a new function:

```rust
// animatix-analyzer/src/completer.rs
pub fn all_snippets() -> Vec<CompletionItem> {
    snippet_completions()
}
```

The GUI palette imports `animatix_analyzer::completer::all_snippets` directly. No duplication, no new crate, no dependency cycle.

---

## Reusability, Fragmentation & Maintainability

### Reusability

| What | How | Avoids |
|------|-----|--------|
| Default properties | `Primitive::default_props()` — already exists in core | Duplicating `default_props_for_actor` in GUI |
| Action metadata | `get_action_signatures()` — already exists in core | Hardcoded `ACTION_CATEGORIES` in GUI |
| Snippets | `all_snippets()` — added to existing analyzer | New `SnippetRegistry` crate/file |
| Unique labels | Extract `unique_label(timeline, ty)` to `app/utils/labels.rs` | Duplicating logic in `DocumentController` and `InsertionContext` |
| Keyframe helpers | Extract shared helpers to `source_edit/ast_utils.rs` | Parallel but divergent logic in `insert_keyframe` and `insert_action` |

### Code Fragmentation

| Concern | Solution |
|---------|----------|
| Palette UI too large | Split into `insertion_palette/{mod,items,render}.rs` |
| Bridge logic scattered | Single file: `app/insertion.rs` contains `InsertionRequest`, `InsertionContext`, `InsertionTarget` |
| AST utils scattered | Consolidate in `source_edit/ast_utils.rs` — keyframe discovery, time shifting, traversal |
| Label utilities | New `app/utils/labels.rs` — shared by `DocumentController`, `InsertionContext`, paste logic |

### Maintainability

| Concern | Solution |
|---------|----------|
| Cross-layer dependencies | `source_edit/` knows nothing about GUI. `app/insertion.rs` is the ONLY file that imports from both layers. |
| Backward compatibility | `InsertActor` keeps its existing fields. The unused `time_s` parameter is finally implemented. No breaking changes to other `SourceEdit` variants. |
| Testability | `insert_action` is a pure function on `Vec<Stmt>`. Unit-test all six rules without touching egui or stores. |
| Extensibility | Add `BuiltinAction` → palette auto-updates. Add `Primitive` → palette auto-updates. Add snippet → palette auto-updates. |

---

## Implementation Plan

### Session 1 — Foundation (AST Layer)

**Goal:** `InsertAction` works correctly for all timeline scenarios.

| # | File | Change |
|---|------|--------|
| 1.1 | `source_edit/ast_utils.rs` | Extract shared helpers from `keyframe_edits.rs`: `find_prev_keyframe_time`, `find_keyframe_insertion_point`, `wrap_leading_decls_in_zero_keyframe`, `adjust_following_relative_keyframe`, `append_to_keyframe_at_time`. Ensure existing tests still pass. |
| 1.2 | `source_edit/mod.rs` | Add `InsertAction` variant to `SourceEdit`. |
| 1.3 | `source_edit/action_edits.rs` | Create module. Implement `insert_action` with all six rules. |
| 1.4 | `source_edit/apply.rs` | Wire `InsertAction` dispatch to `action_edits::insert_action`. |
| 1.5 | `source_edit/keyframe_edits.rs` | Refactor to use shared helpers from `ast_utils.rs`. |
| 1.6 | `source_edit/action_edits.rs` | Add unit tests covering Examples A–D + micro-fragmentation + style inheritance. |

**Deliverable:** `cargo test -p animatix-gui source_edit::` passes. New tests green.

---

### Session 2 — Bridge (App Layer)

**Goal:** `InsertionRequest` correctly maps to `SourceEdit` for all contexts.

| # | File | Change |
|---|------|--------|
| 2.1 | `app/utils/labels.rs` | Create module. Extract `unique_label(timeline, ty)` from `DocumentController`. |
| 2.2 | `app/insertion.rs` | Create module. Implement `InsertionRequest`, `InsertionContext`, `InsertionTarget`, `into_source_edit()`. |
| 2.3 | `source_edit/actor_edits.rs` | Implement keyframe-body insertion path for `insert_actor` when `time_s > ε` and `container` is `None`. |
| 2.4 | `app/document_controller.rs` | Add `handle_insertion` + `show_insertion_feedback`. Refactor `unique_label` to call `app/utils/labels`. |
| 2.5 | `app/document_controller.rs` | Update `handle_create_actor` to use `app/utils/labels::unique_label`. |
| 2.6 | `animatix-analyzer/src/completer.rs` | Add `pub fn all_snippets() -> Vec<CompletionItem>`. |

**Deliverable:** `cargo test -p animatix-gui` passes. `DocumentController` compiles.

---

### Session 3 — UI (Palette)

**Goal:** Palette renders, fuzzy-searches, and dispatches insertions.

| # | File | Change |
|---|------|--------|
| 3.1 | `app/shell/insertion_palette/mod.rs` | Create. State struct, keyboard handling, public API. |
| 3.2 | `app/shell/insertion_palette/items.rs` | Create. Item population from registries, fuzzy filtering, mode filtering. |
| 3.3 | `app/shell/insertion_palette/render.rs` | Create. egui rendering, colors, layout. |
| 3.4 | `app/shell/mod.rs` | Wire `insertion_palette_ui` into `GuiShell`. Remove `action_palette_ui` reference. |
| 3.5 | `app/commands.rs` | Add `OpenInsertionPalette(PaletteMode)`, `ExecuteInsertion(InsertionRequest)`. |
| 3.6 | `app/command_handlers.rs` | Route new commands. `OpenInsertionPalette` opens the overlay; `ExecuteInsertion` delegates to `DocumentController::handle_insertion`. |
| 3.7 | `editor.rs` | Bind `/` key (when editor focused) to `OpenInsertionPalette(default_mode)`. |

**Deliverable:** Palette opens, shows items, fuzzy search works. `cargo test -p animatix-gui` passes.

---

### Session 4 — Polish & Cleanup

**Goal:** Visual feedback, deletion of old code, full test suite.

| # | File | Change |
|---|------|--------|
| 4.1 | `cell_editor/render.rs` | Add amber flash (≈300ms) on timestamp labels when their offset was rewritten by `adjust_following_relative_keyframe`. Use `egui::Ui::ctx().animate_value_with_time`. |
| 4.2 | `cell_editor/mod.rs` | Add `rewritten_timestamp_cells: HashSet<usize>` to `CellEditorState` (cleared after flash). |
| 4.3 | `source_edit/ast_utils.rs` | Return metadata about which keyframes were adjusted so the UI can flash them. |
| 4.4 | `app/shell/action_palette.rs` | **Delete.** Remove hardcoded action categories and text surgery. |
| 4.5 | `app/shell/mod.rs` | Clean up any remaining `action_palette` references. |
| 4.6 | `app/stores/ui_store.rs` | Replace `action_palette_open: bool` with `insertion_palette: InsertionPalette` (or keep boolean + separate palette instance in `GuiShell`). |
| 4.7 | All | Run `cargo test -p animatix`, `cargo test -p animatix-gui`, `cargo clippy`. Fix warnings. |
| 4.8 | `docs/design/insertion-mechanism.md` | Mark implemented sections. Move open questions to resolved or new issues. |

**Deliverable:** All tests green. Old action palette gone. Visual feedback works.

---

## Migration Path Summary

| Step | File(s) | Change |
|------|---------|--------|
| 1 | `source_edit/mod.rs` | Add `InsertAction` variant |
| 2 | `source_edit/action_edits.rs` | Create module with `insert_action` |
| 3 | `source_edit/actor_edits.rs` | Extend `insert_actor` for `KeyframeBody` |
| 4 | `source_edit/ast_utils.rs` | Extract shared keyframe helpers |
| 5 | `source_edit/keyframe_edits.rs` | Refactor to use shared helpers |
| 6 | `app/utils/labels.rs` | Extract `unique_label` |
| 7 | `app/insertion.rs` | Create bridge layer |
| 8 | `app/shell/insertion_palette/` | Create palette UI (3 files) |
| 9 | `app/shell/mod.rs` | Wire palette, remove old one |
| 10 | `app/commands.rs` | Add new commands |
| 11 | `app/command_handlers.rs` | Route commands |
| 12 | `app/document_controller.rs` | Add `handle_insertion` + feedback |
| 13 | `animatix-analyzer/src/completer.rs` | Add `all_snippets()` |
| 14 | `cell_editor/render.rs` | Amber flash on rewritten timestamps |
| 15 | `editor.rs` | Bind `/` key |
| 16 | Delete `app/shell/action_palette.rs` | Remove obsolete file |

---

## Why This Is Elegant

1. **No duplication** — Primitives and actions are defined once in the core library; the GUI reads from those registries. No parallel UI-specific lists.
2. **Semantic edits only** — Every insertion mutates the AST through `SourceEdit`, then re-serializes. No more text surgery.
3. **One UI for everything** — The user presses `Ctrl+Shift+P` (or `/`) and fuzzy-searches for whatever they want. No need to remember which panel does what.
4. **Context-aware** — The palette knows whether you're in a keyframe or code cell, defaults accordingly, and uses the cursor's keyframe time over the playhead.
5. **Timeline-safe** — Existing keyframes' absolute times never shift. New keyframes inherit the preceding keyframe's style. The user sees predictable, reversible changes.
6. **Extensible by default** — Adding `&MY_NEW_PRIM` to `PRIMITIVES` or registering a new `BuiltinAction` automatically makes it appear in the palette.
7. **Type-safe bridge** — `InsertionRequest` → `SourceEdit` is a pure function with no side effects; testable and composable.
8. **Maintainable structure** — Clear layer boundaries. No cross-layer leaks. UI split into focused submodules.

---

## Open Questions

1. **Action arguments UI** — Some actions take arguments (e.g., `move target to (100, 100)`). Options:
   - (A) Insert with placeholder syntax: `move target to (${1:100}, ${2:100})` and let the editor's snippet system handle tab-navigation.
   - (B) Inline argument editor in the palette row that expands on selection.
   - (C) Insert with sensible defaults and let the user edit in the cell.
   
   **Recommendation:** Start with (C). It requires no new UI infrastructure. Enhance to (A) once snippet tab-navigation is improved.

2. **Preview on hover** — Should hovering a primitive in the palette show a ghost preview in the canvas? This would require a lightweight "what-if" timeline build. Worth it for discoverability, but complex.
   
   **Recommendation:** Deferred to post-implementation. Add a `// TODO(preview-on-hover)` comment in `insertion_palette/render.rs`.

3. **Snippet AST parsing** — Currently snippets insert raw text. For full semantic correctness we could parse snippet text into `Vec<Stmt>` and insert via `SourceEdit`. This guarantees valid syntax but adds a parse step for every snippet.
   
   **Recommendation:** Keep raw-text insertion for now. Snippets are user-facing templates; invalid syntax is surfaced immediately by the analyzer diagnostics. Revisit after Phase 10 (green tree) when lossless parsing is available.
