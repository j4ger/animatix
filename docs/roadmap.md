# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).

---

## 1. Deferred Features

### 1.1 GUI Inspector: Animated Geometry Editing

**Status:** `Polygon.points` and `Path.commands` assignments work at source level. GUI widgets deferred.
**Location:** `crates/animatix-gui/src/app/panels/inspector/`.

No widget exists for editing variable-length lists of `Vec2` points or path commands. The inspector currently displays `"[N pts]"` / command string as read-only labels.

**Effort:** High (custom multi-point / command editor).

---

### 1.2 Multi-Scene GUI: Scene List & Composition Timeline

**Status:** Pending. Hard cuts supported; transition blending deferred.
**Location:** `crates/animatix-gui/src/app/panels/`.

The runtime supports multi-scene composition (`# SceneName`, `play SceneName [transition, duration]`), but the GUI lacks a scene list / composition timeline panel. Transition blending (dual render) is Phase 7; only hard cuts work in Phase 1.

**Effort:** Medium–High.

---

### 1.3 Image / Svg Source Assignment

**Status:** Re-declaration required. Property assignment not yet supported.

Changing `Image.url` or `Svg.url` currently requires a full actor re-declaration at a keyframe. Direct assignment (`photo.url = "new.png"`) is not supported.

**Effort:** Low–Medium.

---

## 2. Architecture / Cleanup Debt

### 2.1 Docs Out of Sync with Unified Primitives

**Status:** `spec.md`, `primitives.md`, and `contributing.md` still reference deleted types (`Circle`, `Dot`, `Arc`, `Arrow`, `Square`, `RegularPolygon`).

The primitives were unified (11 → 5: `Rect`, `Ellipse`, `Line`, `Polygon`, `Path`) but docs still describe the old surface.

**Effort:** Low.

---

### 2.2 `KurboShape` Dead Variants

**Status:** `RectUniform` and `RectRadii` variants exist but are never constructed in production or tests.

These were intended for corner rounding on `Rect` but are unused. Either wire them to a `radius` property or remove them.

**Effort:** Low.

---

### 2.3 `ShapeKind` / `ShapeType` Duality

**Status:** Two enums cover the same 5 shape variants.

`ShapeKind` (in `track.rs`) categorizes actors; `ShapeType` (in `shapes/mod.rs`) is an animatable property value. Consider a `From<ShapeType> for ShapeKind` impl to reduce manual mapping duplication.

**Effort:** Low.

---

## 3. Long-Term / Speculative

### 3.1 FFI / Web Canvas Integration

Enable web deployment by targeting HTML5 Canvas or WebGPU via wasm-bindgen.

**Effort:** Very High. Alternative renderer backend.

---

### 3.2 Lossless Syntax Tree (Green Tree)

**Location:** `docs/architecture.md` §Source Write-Back.

Adopt a `rowan`-style green-tree architecture for full-fidelity source preservation (every space, newline, comment).

**Effort:** Very High. 3-6 month project. Not justified at current scale.

---

### 3.3 Trivia-Inspired AST

**Location:** `docs/architecture.md` §Source Write-Back.

Add leading/trailing trivia (comments, whitespace) to AST nodes for better formatting preservation during GUI write-back.

**Effort:** High. Massive parser rewrite.

---

## 4. Quick Reference: Priority Order

| Priority | Item | Effort | Impact |
|----------|------|--------|--------|
| 1 | Docs sync with unified primitives (2.1) | Low | High |
| 2 | Multi-Scene GUI scene list / composition timeline (1.2) | Medium–High | High |
| 3 | GUI Inspector: point / path command editors (1.1) | High | Medium |
| 4 | `KurboShape` dead variant cleanup (2.2) | Low | Low |
| 5 | Green tree / trivia AST (3.2) | Very High | Low (polish) |
