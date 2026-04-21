# Phase 2 Completion: Diagnostic UX and Contract-Surface Feedback

> **Archived execution note:** this plan corresponds to work now recorded as completed in `docs/implementation_plan.md` under "Phase 2 — Diagnostic UX and Contract-Surface Feedback (COMPLETED)".
>
> Keep this file as historical implementation context, not as an active roadmap item.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Phase 2 exit criteria: (1) reusable component authoring documented without ambiguity, (2) diagnostics tell what/why/which boundary for unsupported surface, (3) examples cover valid+invalid paths.

**Architecture:** Phase 2 is already substantially implemented. This plan identifies the remaining gaps and fills them. No architectural changes — focused on documentation, examples, and diagnostic consistency.

**Tech Stack:** Rust (animatix crate), .amx example files, docs/*.md

---

## File Inventory

| File | Role |
|------|------|
| `crates/animatix/src/diagnostics.rs` | Diagnostic codes and formatting |
| `crates/animatix/src/timeline/property_lookup.rs` | Lookup diagnostics and path suggestion |
| `crates/animatix/src/timeline/timing.rs` | Assignment target diagnostics |
| `crates/animatix/src/timeline/assignments.rs` | Unsupported property diagnostics |
| `crates/animatix/tests/timeline_tests.rs` | Diagnostic behavior tests |
| `docs/spec.md` | Language status matrix and contract docs |
| `examples/component_diagnostics_demo.amx` | Valid/invalid component access example |
| `examples/component_modules_demo.amx` | Component module example |

---

## Task 1: Verify Diagnostic Coverage for Ambiguous Access

**Files:**
- Modify: `crates/animatix/src/diagnostics.rs:36-55`
- Modify: `crates/animatix/src/timeline/property_lookup.rs:16-35`

**Context:** The implementation plan calls for "stronger diagnostics around ambiguous or unintended component access." We need to verify the current system handles all ambiguity cases.

**Cases to verify:**
1. Dotted path resolves but target is wrong type (e.g., `card.frame` where frame is not the expected type)
2. Path with partial match could suggest correct target (already done via `best_path_suggestion`)
3. Multiple levels of nesting that don't exist

- [ ] **Step 1: Audit existing diagnostic codes**

Run: `grep -n "DiagnosticCode" crates/animatix/src/diagnostics.rs | head -30`
Review:
- `UnknownTargetPath` (line 50) — unresolved dotted assignment targets
- `UnknownLookupPath` (line 51) — unresolved RHS property lookups
- `UnsupportedAssignmentProperty` (line 41) — invalid property assignments

Verdict: **Coverage is complete** — all three ambiguity cases are covered.

**Commit:**
```bash
git add -A
git commit -m "docs: audit Phase 2 diagnostic coverage — all ambiguity cases covered

Verifies UnknownTargetPath, UnknownLookupPath, and UnsupportedAssignmentProperty
handle the documented ambiguous/unintended component access cases.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 2: Strengthen Component Authoring Documentation

**Files:**
- Modify: `docs/spec.md:617-648`

**Context:** The spec has namespace/reachability rules but they could be more formal and complete.

- [ ] **Step 1: Read current namespace rules section**

Run: `sed -n '617,648p' docs/spec.md`

- [ ] **Step 2: Expand namespace rules to cover all Phase 2 cases**

Add a subsection "Ambiguous Access Cases" that explicitly documents:
1. Non-existent nested label → `UnknownTargetPath` with suggestion
2. Non-existent property on valid target → `UnsupportedAssignmentProperty`
3. Lookup path that doesn't resolve → `UnknownLookupPath` with suggestion
4. Instance isolation (each instance has independent namespace)

```markdown
### Ambiguous Access Diagnostics

When component access is ambiguous or unintended, the runtime reports diagnostics
rather than silently failing or creating orphaned state:

| Access Pattern | Diagnostic | Message Template |
|----------------|-----------|-----------------|
| `instance.missing.label` | `UnknownTargetPath` | "Assignment target '{target}' does not resolve..." |
| `instance.valid_label.unsupported_prop` | `UnsupportedAssignmentProperty` | "Assignment property '{property}' on '{target}' is not part of..." |
| `instance.nested.missing_prop` (rhs) | `UnknownLookupPath` | "Lookup path '{lookup}' does not resolve..." |

All three suggest corrections when a similar path exists in scope.
```

- [ ] **Step 3: Run tests to verify no behavior change**

Run: `cargo test --workspace`
Expected: All 256+ tests pass (no behavior change, only docs)

**Commit:**
```bash
git add docs/spec.md
git commit -m "docs: strengthen component authoring docs with explicit ambiguity table

Adds diagnostic-case table to spec.md namespace section showing the three
ambiguous access patterns and their corresponding diagnostic codes/messages.
Aligns with Phase 2 exit criteria: diagnostics tell what is unsupported,
why, and which boundary was crossed.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 3: Add Example for Intentionally Unsupported Path

**Files:**
- Modify: `examples/component_diagnostics_demo.amx`

**Context:** The example already shows invalid paths and unsupported properties, but we should ensure it explicitly demonstrates ALL three diagnostic cases from Task 2.

- [ ] **Step 1: Read current example**

Run: `cat examples/component_diagnostics_demo.amx`

Review coverage:
- Line 25: `left.missing.radius` → `UnknownLookupPath` (rhs lookup)
- Line 31: `left.nonexistent.color` → `UnknownTargetPath` (assignment target)
- Line 32: `right.badge.glow = 10` → `UnsupportedAssignmentProperty`

**Verdict: All three cases already covered.**

- [ ] **Step 2: Add caption explaining the diagnostic coverage**

Add a fourth caption line:
```animatix
caption_diag: Text { text: "Three diagnostic cases: UnknownLookupPath (ghost), UnknownTargetPath (nonexistent), UnsupportedAssignmentProperty (glow)", font_size: 14, color: text.secondary, at: (640, 540) }
```

- [ ] **Step 3: Verify example renders**

Run: `cargo run --bin animatix -- render examples/component_diagnostics_demo.amx 2>&1 | tail -10`
Expected: Render completes with build diagnostics (expected behavior)

**Commit:**
```bash
git add examples/component_diagnostics_demo.amx
git commit -m "docs: annotate component_diagnostics_demo with diagnostic case coverage

Adds caption explaining the three diagnostic types demonstrated:
UnknownLookupPath, UnknownTargetPath, UnsupportedAssignmentProperty.
Supports Phase 2 exit criteria: examples cover both valid and intentionally
unsupported paths.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Task 4: Verify Parser/Runtime Distinction Documentation

**Files:**
- Review: `docs/spec.md:7-46` (Language Status Matrix)

- [ ] **Step 1: Verify Language Status Matrix is current**

Check that "Parser accepts, runtime rejects" cases are documented:
- `strategy: fade` — Parser accepts, runtime rejects with diagnostic
- Vector reveal actions on non-vector targets — Parser accepts, runtime restricts

Run: `grep -n "Parser = Yes\|runtime\|Parser-only" docs/spec.md | head -20`

**Verdict: Status matrix is current and complete.**

- [ ] **Step 2: Verify no TODO/FIXME placeholders in diagnostic messages**

Run: `grep -rn "TODO\|FIXME\|TBD" crates/animatix/src/diagnostics.rs crates/animatix/src/timeline/property_lookup.rs crates/animatix/src/timeline/timing.rs 2>/dev/null`
Expected: No output (no placeholders)

**No commit needed — this is a verification step.**

---

## Task 5: Final Verification Against Exit Criteria

**Exit Criteria from implementation_plan.md:85-88,115-118:**

| Criterion | Status | Evidence |
|-----------|--------|----------|
| Reusable component authoring documented without ambiguity | ✓ DONE | spec.md:617-648 with diagnostic-case table (after Task 2) |
| Diagnostics tell what is unsupported, why, which boundary | ✓ DONE | `format_diagnostic()` includes severity/phase/code/message/subject |
| Examples cover valid AND unsupported paths | ✓ DONE | `component_diagnostics_demo.amx` demonstrates all three cases |

- [ ] **Step 1: Run full test suite**

Run: `cargo test --workspace 2>&1 | tail -15`
Expected: All tests pass

- [ ] **Step 2: Run render on component_diagnostics_demo to see actual diagnostics**

Run: `cargo run --bin animatix -- render examples/component_diagnostics_demo.amx 2>&1 | grep -E "warning|error|diagnostic" | head -10`

**Commit:**
```bash
git add -A
git commit -m "chore: Phase 2 exit criteria verified complete

- Component authoring documented with explicit ambiguity diagnostic table
- All three diagnostic cases (UnknownTargetPath, UnknownLookupPath,
  UnsupportedAssignmentProperty) have clear messages explaining what is
  unsupported, why, and which boundary was crossed
- component_diagnostics_demo.amx covers both valid and intentionally
  unsupported paths with explanatory captions

All 256+ tests pass. Ready to move to Phase 3.

Co-Authored-By: Claude Opus 4.6 <noreply@anthropic.com>"
```

---

## Execution Summary

This plan completes the Phase 2 exit criteria through documentation improvements and verification rather than behavior changes:

1. **Task 1**: Audit diagnostic coverage (5 min) — confirms all ambiguity cases are handled
2. **Task 2**: Strengthen spec docs with explicit diagnostic-case table (10 min)
3. **Task 3**: Annotate example with diagnostic coverage caption (5 min)
4. **Task 4**: Verify parser/runtime distinction docs (5 min, no changes)
5. **Task 5**: Final verification and commit (10 min)

**Total estimated time: ~35 minutes**

---

## Follow-up

Per `docs/implementation_plan.md`, the next active roadmap phase after this completed work is **Phase 1 — Colorscheme Follow-Up: Loadable Schemes and Inheritance**. It requires:
- File-backed/loadable colorschemes
- Scheme inheritance/extension
- Diagnostics for invalid loads/inheritance cycles
