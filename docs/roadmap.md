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

## 2. Architecture / Cleanup Debt

*(Currently clear.)*

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
| 1 | Multi-Scene GUI scene list / composition timeline (1.2) | Medium–High | High |
| 2 | GUI Inspector: point / path command editors (1.1) | High | Medium |
| 3 | Green tree / trivia AST (3.2) | Very High | Low (polish) |
