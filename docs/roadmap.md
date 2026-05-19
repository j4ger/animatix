# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

## 1. Transition & Easing GUI

Shipped: inline transition editor with type/duration/easing, transition badge readout, easing registry.
Known gaps from usability analysis.

### 1.1 Transition Editor Redesign

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:653-720`

**Issues:**
- The inline editor squeezes 3 dropdowns + 2 buttons into a single horizontal row. At typical sidebar width (~250px) each control is cramped.
- Duration uses seconds (`0.0..=10.0`, speed `0.1`) — far too coarse for 100–1000ms transitions. Users overshoot constantly.
- **Target scene is read-only.** The badge shows `→ Outro [fade · 300ms]`, but the editor can only mutate type/duration/easing. To change `Outro`, users must hand-edit source. This breaks the mental model.
- No easing preview — the dropdown is text-only (`linear`, `elastic`, `back`). Users cannot see what a curve looks like without playing the animation.
- Inline expansion pushes subsequent scenes down, which is fine for renaming (single field) but jarring for a 5-control form.

**Fix:** Refactor to a vertical popover/card layout:
```
Target scene: [Scene B ▼]
Transition:   [Fade    ▼]  Duration: [500] ms
Easing:       [Ease Out ▼]  [~curve preview~]
```
- Add target scene dropdown (wire up existing `SourceEdit::SetPlayTarget`)
- Change duration to milliseconds (`clamp_range(0..=10_000)`, `speed(10)`)
- Add mini easing curve preview (sample `apply_easing` at 20 points, draw 40×20px curve)
- Keep ✓/✕ buttons; support Enter/Escape

**Effort:** Medium.

---

### 1.2 Reusable EasingPicker Component

**Location:** New file `crates/animatix-gui/src/app/components/easing_picker.rs`

**Issue:** Easing selection is duplicated inline in the transition editor. Every context that needs easing (transitions, keyframes, actions) will reimplement the same dropdown.

**Fix:** Extract a reusable `easing_picker` component that renders:
- `ComboBox` populated from `animatix::easing::EASING_REGISTRY`
- 40×20px mini curve preview next to the label (sampled from `apply_easing`)

Wire into: transition editor, keyframe dope sheet, future action blocks.

**Effort:** Medium.

---

### 1.3 Keyframe Easing Editor

**Location:** `crates/animatix-gui/src/app/panels/inspector/keyframe_table.rs`, `crates/animatix-gui/src/source_edit.rs`

**Issue:** Every `PropertyTrack<T>` stores `(value, Easing)` per keyframe (`timeline/track.rs:285`), but `PropertyTrackInfo` throws the easing away. The dope sheet shows time + value only. Users cannot see or edit which easing a keyframe uses.

**Fix:**
- Extend `PropertyTrackInfo` to carry `easing: Easing`
- Add `SourceEdit::SetKeyframeEasing { actor, property, time_s, easing }` variant
- Render easing in the dope sheet (compact: small curve icon on dot hover; expanded: easing column)
- Right-click on keyframe dot → "Easing" submenu with the 8 options

**Effort:** Medium.

---

### 1.4 Scrubber Transition Interaction

**Location:** `crates/animatix-gui/src/app/shell/transport_bar.rs:402-437`

**Issue:** The transport bar draws transition overlaps with diagonal hatching, but they are inert. Hovering or clicking an overlap does nothing. There's no bidirectional linkage between the scrubber and the transition editor.

**Fix:**
- Hover tooltip on overlap: `Fade to "Outro" — 300ms — Ease Out`
- Click overlap → `select_scene` on the source scene + open its transition card
- Distinguish transition types visually (e.g. small icon glyph for wipe direction)

**Effort:** Low.

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

## 3. GUI & Rendering Polish

Shipped: basic preview, inspector, layer tree, keyframe table, grid snap, selection overlay.
Known gaps from end-user workflow analysis.

### 3.1 Preview Canvas Zoom & Pan

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:1898-1913`

**Issue:** The preview canvas is fixed 1:1. At typical scene resolutions (1920×1080), the canvas overflows the available panel space on laptops. Users cannot zoom in to align elements precisely or zoom out to see the full composition.

**Fix:** Add mouse-wheel zoom (centered on cursor) and middle-click pan. Maintain a `preview_zoom: f32` and `preview_pan: Vec2` in `PreviewPaneState`. Apply transform before `fit_preview`.

**Effort:** Low.

---

### 3.2 Playback Speed Control

**Location:** `crates/animatix-gui/src/app/shell/transport_bar.rs`

**Issue:** Playback is always 1×. Reviewing timing at half or double speed is essential for polish work.

**Fix:** Add a speed dropdown/button group (0.25×, 0.5×, 1×, 2×) in the transport bar. Scale `delta` in `PreviewPaneState::tick` by the speed factor.

**Effort:** Low.

---

### 3.3 Loop Region (A-B Repeat)

**Location:** `crates/animatix-gui/src/app/shell/transport_bar.rs`

**Issue:** Users cannot loop a subsection of the timeline. To polish a 2-second segment, they must wait for the full timeline to replay.

**Fix:** Add A/B markers on the scrubber. Click "A" to set start, "B" to set end. When active, playback loops between the two points. Render the loop region with a subtle highlight on the scrubber track.

**Effort:** Low.

---

### 3.4 Copy / Paste Actors

**Location:** `crates/animatix-gui/src/app/mod.rs`, `crates/animatix-gui/src/source_edit.rs`

**Issue:** Users cannot duplicate actors via clipboard. Re-creating a complex actor with children, colors, and keyframes is tedious.

**Fix:**
- `Ctrl+C` on selected actor(s): serialize their AST declarations to a clipboard buffer
- `Ctrl+V`: insert copied declarations at current time, auto-generating unique labels
- Preserve relative timing: if copied actor has keyframes at 0s and 2s, pasted actor gets keyframes at `current_time` and `current_time + 2s`

**Effort:** Medium.

---

### 3.5 Rulers & Guides

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:1900-1973`

**Issue:** No visual reference for alignment. Users eyeball positions or use grid snap, but there's no way to set custom alignment lines.

**Fix:**
- Horizontal + vertical rulers around the preview canvas (pixel units)
- Drag from ruler to create a guide line
- Snap actors to guides (extend existing grid snap system)
- Store guides in `PreviewPaneState`

**Effort:** Low.

---

### 3.6 Snap to Other Actors

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:1110-1130`

**Issue:** Grid snap exists, but there's no snap to edges/centers of nearby actors. Aligning two rectangles precisely requires manual coordinate entry.

**Fix:** During drag, compute candidate snap positions from all other actors' bounds (edges, centers, corners). Show a temporary snap line when within threshold. Extend the existing grid snap logic.

**Effort:** Low.

---

### 3.7 Drop Shadow & Glow Filters

**Location:** `crates/animatix/src/renderer/core.rs`, `crates/animatix/src/primitives/`

**Issue:** Actors look flat. Drop shadows and glow are table stakes for modern motion graphics.

**Fix:** Add `shadow_offset`, `shadow_blur`, `shadow_color`, `glow_radius`, `glow_color` properties to the actor property registry. Implement in the renderer as a post-processing pass or via Vello's shadow/glow support. Start with simple drop shadow (offset + Gaussian blur + color).

**Effort:** Medium.

---

### 3.8 Blur Backdrop

**Location:** `crates/animatix/src/renderer/core.rs`

**Issue:** Frosted-glass panels are impossible without blur. This is a common UI animation pattern.

**Fix:** Add a `backdrop_blur: f32` property. Requires rendering the scene behind the actor to a texture, then applying a Gaussian blur. Can reuse the transition compositor's blur infrastructure or implement a simple box blur in WGSL.

**Effort:** Medium.

---

### 3.9 Container Opacity

**Location:** `crates/animatix/src/timeline/mod.rs`

**Issue:** Individual actors have opacity tracks, but containers do not. Fading out an entire group of children requires animating each child's opacity separately.

**Fix:** Add `opacity` to container tracks (Row, Col, Grid, Stack, Group). During evaluation, multiply the container's opacity into each child's computed opacity. This is a single multiplicative factor in the transform cascade.

**Effort:** Low.

---

### 3.10 Mask / Clip Container

**Location:** `crates/animatix/src/primitives/`, `crates/animatix-gui/src/app/panels/mod.rs`

**Issue:** No way to clip content to a shape (e.g., photo inside a circle, scrolling text inside a rectangle).

**Fix:** Add a `Mask` container. First child defines the clipping geometry; subsequent children are clipped to it. Use Vello's `push_layer` / `pop_layer` or stencil-based clipping. In the GUI, render the mask child with a dashed outline to indicate its special role.

**Effort:** Medium.

---

### 3.11 `Typst` Primitive

**Location:** New file `crates/animatix/src/primitives/typst.rs`

**Issue:** The `Text` primitive is plain. No bold, italic, or inline color spans. `Math` is separate. Users want rich text without switching primitives.

**Fix:** Add a `Typst` primitive that renders typst markup. Leverages existing `typst = "0.14.2"` dependency. Accepts `content: "*bold* and _italic_"` or `content: "# emph(\"emphasis\")"`. Outputs to Vello via typst's built-in renderer.

**Effort:** Medium.

---

### 3.12 Audio Tracks

**Location:** `crates/animatix/src/renderer/video.rs`, `crates/animatix/src/timeline/mod.rs`

**Issue:** Exports are silent. Even a simple background track would make outputs feel finished.

**Fix:**
- Add `Audio` primitive: `track: Audio, source: "bgm.mp3"`
- Store audio segments in timeline (start time, duration, volume)
- During video export, decode audio and mux into the output via ffmpeg (`-i audio.mp3 -shortest`)
- No real-time audio playback in GUI needed for MVP

**Effort:** Medium.

---

### 3.13 GUI Layer Tree Reparenting

**Location:** `crates/animatix-gui/src/app/panels/mod.rs:1991-2101`

**Issue:** Parenting is already possible via `Group` containers (transform inheritance without layout), but the GUI layer tree doesn't support drag-and-drop reparenting. Users must edit source to restructure hierarchies.

**Fix:** Enable drag-and-drop in the layer tree: drag an actor onto another actor to make it a child. If target is not a container, wrap both in a new `Group`. Update source AST via `SourceEdit::Reparent`.

**Effort:** Medium.

---

### 3.14 Static SVG Import

**Location:** `crates/animatix/src/ast.rs`, `crates/animatix/src/timeline/mod.rs`

**Issue:** Users have existing SVG assets (icons, illustrations) and want to bring them into Animatix as editable actors.

**Fix:** Import SVG as a static tree of actors: `<g>` → `Group`, `<path>` → `Path`, `<rect>` → `Rect`, etc. Preserve hierarchy and transforms. Do not convert SVG animations — import structure only, animate in AMX afterward.

**Effort:** Medium-High.

---

## 4. Design Notes

## 5. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Transition editor redesign: target scene + vertical card + ms + easing preview (1.1) | Medium | High |
| 2 | Preview zoom & pan (3.1) | Low | High |
| 3 | Playback speed control (3.2) | Low | High |
| 4 | Loop region / A-B repeat (3.3) | Low | High |
| 5 | Copy/paste actors (3.4) | Medium | High |
| 6 | Reusable EasingPicker component with curve preview (1.2) | Medium | Medium |
| 7 | Keyframe easing editor in dope sheet (1.3) | Medium | Medium |
| 8 | Drop shadow / glow filters (3.7) | Medium | Medium |
| 9 | Blur backdrop (3.8) | Medium | Medium |
| 10 | Mask/Clip container (3.10) | Medium | Medium |
| 11 | `Typst` primitive (3.11) | Medium | Medium |
| 12 | Audio tracks (3.12) | Medium | Medium |
| 13 | GUI layer tree reparenting (3.13) | Medium | Medium |
| 14 | Container opacity (3.9) | Low | Medium |
| 15 | Rulers & guides (3.5) | Low | Medium |
| 16 | Snap to other actors (3.6) | Low | Medium |
| 17 | Scrubber transition hover/click interaction (1.4) | Low | Low |
| 18 | Static SVG import (3.14) | Medium-High | Low |
| 19 | Green tree / trivia AST (2.2) | Very High | Low (polish) |
