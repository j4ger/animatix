# Animatix User Report Analysis

> Based on @report.md (AI training pipeline feedback, 2026-05-28)

---

## Executive Summary

The report identifies **16 issues** across documentation, compiler/CLI, language design, and examples. Fixing the P0+P1 items is projected to improve AI-generated compile rates from **86.5% → 95%+**. Many issues are low-effort doc fixes or known parser limitations. A few require architectural decisions.

---

## Issue Breakdown & Assessment

### P0 — Critical (blocking, high failure rate)

| # | Issue | Assessment | Effort | Action |
|---|-------|------------|--------|--------|
| 1 | **Polygon.points spec error** | Confirmed. Spec shows `[]`, parser/examples require `{}`. | 5 min | **Fixed** in `spec.md` |
| 2 | **Container comments rejected** | Confirmed. `//` inside `{}` blocks fails because chumsky `.padded()` skips whitespace but not comments. | 1–2 days | Add to Phase 6.6 |
| 3 | **Error messages unreadable** | Partially confirmed. `format_diagnostic()` already includes line:col, but CLI wraps it in `tracing` which injects ANSI and timestamps. No JSON output. | 2–3 days | Add to Phase 6.7 |

### P1 — High Impact (frequent guesswork)

| # | Issue | Assessment | Effort | Action |
|---|-------|------------|--------|--------|
| 4 | **Property names undocumented** | `PROPERTY_REGISTRY` exists and is comprehensive (~50 properties). Just needs extraction to docs. | 2–3 hours | Add to Phase 6.5 |
| 5 | **LLM invents nonexistent elements** | No single source of truth for "what exists." Need explicit whitelist + anti-list. | 1 hour | Add to Phase 6.5 |
| 6 | **`check --format json`** | CLI only prints plain text. JSON would help scripting/IDE integration. | 4–6 hours | Add to Phase 6.7 |
| 7 | **Math/Typst syntax reference** | LLM defaults to LaTeX. Need a quick reference table. | 2–3 hours | Add to Phase 6.5 |
| 8 | **Graph nested elements can't animate** | `g.vec.to = (5,2)` fails because dotted assignment targets don't support nested container paths. Requires parser + build changes. | 3–5 days | Add to Phase 6.8 |
| 9 | **Spec/examples inconsistency** | `Polygon.points` is the only confirmed mismatch. Others need audit. | 2 hours | Add to Phase 6.5 |
| 10 | **Examples coverage gaps** | `ContourSet`: 0 examples. `Path`: 1. `VectorField`: 1. `Heatmap`: 1. | 2–3 days | Add to Phase 6.8 |

### P2 — Medium Impact (nice-to-have)

| # | Issue | Assessment | Effort | Action |
|---|-------|------------|--------|--------|
| 11 | **Color system scattered** | Hex colors unsupported, token list incomplete, RGBA value range unclear. | 3–4 hours | Add to Phase 6.5 |
| 12 | **3D support status unclear** | Spec doesn't say "no 3D." LLM naturally invents `Graph3D`. | 30 min | Add to Phase 6.5 |
| 13 | **CLI stdin support** | `animatix-cli check < file.amx` doesn't work. | 2–3 hours | Add to Phase 6.7 |
| 14 | **`lint` / `format` commands** | Requires trivia-aware AST (Phase 10). Blocked. | — | Defer to Phase 10 |
| 15 | **Arrow primitive missing** | Would be useful for vectors/physics. New primitive. | 1–2 days | Add to Phase 6.8 |
| 16 | **`let` variables can't animate** | `x = 5 [1s]` not supported. Needs variable track system. | 1 week | Defer — high effort, niche use case |

---

## Key Architecture Observations

1. **Diagnostics are richer than they appear.** `format_diagnostic()` already outputs `line:col:[severity:phase] message`. The perceived "no line number" issue is because `tracing::error!()` prepends timestamps and ANSI codes. The fix is a `--format` flag that bypasses tracing.

2. **Property registry is the canonical schema.** `PROPERTY_REGISTRY` in `property_registry.rs` has every property name, type, flags, and applicable actor kinds. The spec just needs to re-export this.

3. **Container comments are a parser-wide issue.** The `//` comment parser is only a top-level `Stmt::Comment` alternative. `.padded()` throughout the parser skips whitespace but not comments. Fixing this requires replacing all `.padded()` with `.padded_by(whitespace_or_comment)` — mechanical but touches ~100 call sites.

4. **Graph nesting is a parser/build gap.** `g.vec.to = (5,2)` requires the assignment target parser to accept `container.child.property` paths, and the build pipeline to resolve them. Currently only `actor.property` works.

---

## Roadmap Integration

These items are inserted **before Phase 7** because they directly improve the AI/codegen success rate and should stabilize before audio/PiP work.

| Phase | Theme | Items | Est. Effort |
|-------|-------|-------|-------------|
| **6.5** | Doc Hotfixes | Fix spec syntax, property registry export, element whitelist, color docs, Typst reference, 3D status | 1–2 days |
| **6.6** | Parser Robustness | `//` comments in brackets/lists/blocks, better parse error context | 3–5 days |
| **6.7** | CLI Tooling | `--format json`, stdin support, clean ANSI output | 2–3 days |
| **6.8** | Language & Examples | Graph nested animation, Arrow primitive, underused element examples | 1 week |

**Deferred:** `lint`/`format` (blocked on trivia AST, Phase 10), `let` animation (high effort, niche).

---

## What Was Already Fixed

- **Polygon.points syntax** in `spec.md` — changed `[]` → `{}` to match parser and examples.

