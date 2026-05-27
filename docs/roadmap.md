# Animatix Roadmap

> Forward-looking view of known gaps, planned features, and deferred work.  
> For the current language surface, see [`spec.md`](spec.md). For architecture, see [`architecture.md`](architecture.md).  
> For detailed implementation plans, see [`typing-plan.md`](typing-plan.md) and [`design/gui-redesign-2026.md`](design/gui-redesign-2026.md).

---

## Bug Fixes

No open bugs at this time.

---

## Phase 2 — Action Parameters & Module-Scoped Actions

**Current phase.** These are the largest remaining language features.

| Phase | Item | What | Effort | Files |
|-------|------|------|--------|-------|
| **P8** | **Action parameter syntax** | Allow named arguments in action modifiers: `pulse btn [200ms, scale: 1.2]`. Parser must disambiguate from existing modifier syntax. Define `ActionParam` AST node. | 2 days | `animatix-syntax/src/parser/`, `animatix-syntax/src/ast.rs` |
| **P9** | **Action parameter expansion** | Inline action parameters at call sites (like component params). `self` in action body refers to invocation target. | 2 days | `animatix-syntax/src/module/inline_actions.rs` |
| **P10** | **Module-scoped actions** | Parser: `action Name(params) { ... }` at module scope. Module: `action_registry: HashMap<String, ActionDef>`. Expansion: inline at call sites. Type checker: validate invocations. | 2 days | `animatix-syntax/src/`, `animatix-syntax/src/module.rs` |
| **P11** | **Action param type checking** | Validate named arguments in action invocations against action parameter types. | 4 hours | `animatix-syntax/src/typecheck.rs` |

**Post-requisite:** All examples using component actions must continue to work.

---

## Phase 3 — GUI Redesign: "Timeline-First Direct Manipulation"

**Do not start until Phase 2 is complete.** Full design specification: [`docs/design/gui-redesign-2026.md`](design/gui-redesign-2026.md)

| Phase | Item | What | Effort | Files |
|-------|------|------|--------|-------|
| **P12** | **Kill the Bars** | Delete transport bar, preview header, NL command bar. Simplify top bar to filename + play + settings + command palette. Move playback into timeline. | 1 session | `app/shell/`, `app/panels/preview_canvas/mod.rs`, `app/mod.rs` |
| **P13** | **Unified Gizmo & Property Popup** | Replace 8 handles + rotation with unified transform gizmo (move/scale/rotate). Add measurement lines. Replace floating card with property popup (4 essentials + tabs). Per-property diamond keyframe toggles. Auto-keyframe with undo toast. | 2 sessions | `app/preview/`, `app/panels/preview_canvas/`, `app/components/` |
| **P14** | **Draggable Timeline & Actions** | Make keyframe diamonds draggable with snap. Multi-select. Action palette (`A` key). Action blocks in timeline. Drag to resize duration. | 2 sessions | `app/panels/timeline_panel.rs`, `app/commands.rs`, `app/command_handlers.rs` |
| **P15** | **Multi-Scene Integration** | Scene blocks in timeline row. Click to enter local editing mode. Drag to reorder/adjust timing. Transition regions + inline editor. Context HUD. Breadcrumb. `G` key toggle. | 2 sessions | `app/panels/timeline_panel.rs`, `app/panels/mod.rs`, `app/mod.rs`, `app/preview/` |
| **P16** | **Polish** | Preview hover HUD. Toast notifications. Time lens trigger `Space` → `T`. Shortcut cheat sheet. Smooth animations. | 1 session | `app/design_tokens.rs`, `app/components/`, `app/preview/` |

---

## Phase 4 — Future Features (Post-GUI Redesign)

| Phase | Item | What | Effort | Blocker |
|-------|------|------|--------|---------|
| **P17** | **Audio multi-segment muxing** | Concatenate multiple audio files via ffmpeg | 3 days | — |
| **P18** | **NL command bar dispatch** | Connect NL input to actual AI service backend | 1 week | External AI service |
| **P19** | **Agent suggestion UI** | Integrate agent suggestion widget | 3 days | External AI service |
| **P20** | **Multi-viewport / PiP** | Explicit `Viewport` type, viewport tracks in timeline, composite rendering | 2–3 months | GUI redesign |
| **P21** | **Green tree (rowan)** | Immutable syntax tree with cheap clones | 3–6 months | Stable syntax |
| **P22** | **Trivia-inspired AST** | Whitespace/comment preservation in AST | 2–3 months | Green tree |
| **P23** | **Web canvas / WASM** | Alternative renderer backend for browser export | Very high | Alternative renderer |

---

## Completion Criteria

**Phase 2 (Action Params) is complete when:**
- [ ] `pulse btn [200ms, scale: 1.2]` parses and runs
- [ ] Action parameters are substituted at call sites
- [ ] Module-scoped `action Name(params) { ... }` works
- [ ] Type checker validates action named arguments

**Phase 3 (GUI Redesign) is complete when:**
- [ ] Only 28px top bar persists
- [ ] Canvas has unified gizmo + measurement lines
- [ ] Property popup replaces inspector + floating card
- [ ] Timeline keyframes are draggable
- [ ] Action palette + blocks work
- [ ] Multi-scene editing is timeline-first
