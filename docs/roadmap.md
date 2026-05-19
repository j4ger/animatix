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

## 3. Design Notes

## 4. Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Transition editor redesign: target scene + vertical card + ms + easing preview (1.1) | Medium | High |
| 2 | Reusable EasingPicker component with curve preview (1.2) | Medium | Medium |
| 3 | Keyframe easing editor in dope sheet (1.3) | Medium | Medium |
| 4 | Scrubber transition hover/click interaction (1.4) | Low | Low |
| 5 | Green tree / trivia AST (2.2) | Very High | Low (polish) |
