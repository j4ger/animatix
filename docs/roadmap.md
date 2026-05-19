# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

## 1. Multi-Scene GUI

Shipped: parser, composition engine, CLI export, basic scene list panel, transport scrubber with scene blocks, transition editor.
Known gaps and polish items from UI inspection.

### 1.1 Scene Selection & Navigation

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:591`, `crates/animatix-gui/src/app/shell/transport_bar.rs:289-295`

**Issues:**
- Clicking a scene in the Scenes sidebar only updates `active_scene` but does not jump `current_time_s` to the scene's start. The preview stays at the old global time, often showing the previous scene.
- The preview status bar shows time (`t = 2.34s / 10.5s`) but never indicates which scene is currently playing.
- The Layers tab silently repopulates when the active scene changes, with no indicator of which scene's actors are being shown.

**Fix:** Set `actions.scrub_to` on scene selection. Add scene name to status bar and layers header.

**Effort:** Low.

---

### 1.2 Transition Handling

**Location:** `crates/animatix-gui/src/preview_surface.rs:263-324`, `crates/animatix-gui/src/app/panels/inspector/mod.rs:36-61`

**Issues:**
- During a transition, hit_regions are taken from the "from" scene only. Actors in the "to" scene are not clickable in the preview canvas during the transition.
- The inspector panel receives the active scene's timeline. During a transition both scenes are visible, but the inspector can only see actors from one scene. Clicking an actor in the other scene shows wrong properties or "No actors in scene".
- The transport scrubber scene-click jumps to `scene_start_times[scene]`, ignoring transition overlap. This lands the playhead in the middle of a transition instead of the stable part of the scene.

**Fix:** Merge hit_regions from both scenes during transitions. Make inspector query both timelines during transitions. Account for transition duration when jumping to scene start.

**Effort:** Medium.

---

### 1.3 Keyframe & Timeline Navigation

**Location:** `crates/animatix-gui/src/app/mod.rs:427, 627, 647`, `crates/animatix-gui/src/document.rs:351-369`

**Issue:** `prev_keyframe` / `next_keyframe` call `timeline_keyframe_times_s(Some(timeline), None, None)`, which only looks at the *active scene's* timeline. In multi-scene files, pressing `.` or `,` skips over keyframes in other scenes entirely.

**Fix:** Pass `composition` and `active_scene` to `timeline_keyframe_times_s` so it can collect keyframes from all scenes and navigate globally.

**Effort:** Low.

---

### 1.4 Scene Reorder Drag-and-Drop

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:526`

**Issue:** The drop target calculation uses `(relative_y / row_height)` where `row_height` is a constant `ROW_M`. Scene rows can grow taller when transition badges are displayed beneath them, making the drop feel imprecise.

**Fix:** Use actual row heights from the rendered list instead of a constant.

**Effort:** Low.

---

### 1.5 Scene Deletion

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:692`, `crates/animatix-gui/src/source_edit.rs`

**Issue:** The Scenes tab has "+ Add Scene" but no delete/remove button. Users must manually edit source to remove a scene. There is no `SourceEdit::DeleteScene` variant.

**Fix:** Add a delete button to each scene row. Implement `SourceEdit::DeleteScene` that removes the scene declaration and any `play` edges referencing it.

**Effort:** Medium.

---

### 1.6 Transition Easing Registry

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:620`

**Issue:** The transition editor's easing dropdown uses a hard-coded list: `["linear", "easein", "easeout", ...]`. If new easings are added to the runtime registry, they won't appear in the GUI.

**Fix:** Query the easing registry dynamically instead of hard-coding the list.

**Effort:** Very Low.

---

## 2. Long-Term / Speculative

### 2.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 2.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 2.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 3. Design Notes

## 4. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Scene selection should jump to start time (1.1) | Low | High |
| 2 | Transition hit_regions + inspector dual-scene support (1.2) | Medium | High |
| 3 | Global keyframe navigation across scenes (1.3) | Low | Medium |
| 4 | Scene deletion in GUI (1.5) | Medium | Medium |
| 5 | Scene reorder drop precision (1.4) | Low | Low |
| 6 | Status bar scene name + layers scene indicator (1.1) | Low | Low |
| 7 | Transition easing registry (1.6) | Very Low | Low |
| 8 | Green tree / trivia AST (2.2) | Very High | Low (polish) |
