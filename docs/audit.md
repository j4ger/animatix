# Audit and Measurement

Animatix provides a unified validation and measurement framework designed for both human authors and LLM agents. It verifies layout geometry, visual appearance, typography, and temporal continuity, replacing the legacy `review` and `verify` commands with a single deterministic pipeline.

**Core principle:** facts are derived from the scene by the tool, verdicts are produced by rules, visual review provides intent, and regression is carried by PNG baselines. Authors and agents write `.amx` only — they never write assertion files, guess sample times, or tune ink thresholds.

## Core Roles

| Command | Purpose | Primary Flags |
|---|---|---|
| `animatix audit <file>` | **事实 + 证据 + 候选**：自动给出结构/像素事实、确定性缺陷与观察候选，排版与视觉质量由开发者目视审阅判定 | `--policy`, `--format`, `--brief`, `--render`, `--samples`, `--times`, `--max-frames`, `--fail-on`, `--detail`, `--storyboard`, `--frames-dir`, `--evidence`, `--set`, `--plugin`, `--width`, `--height` |
| `animatix measure <file>` | Pure fact query: outputs frame/actor facts as JSON with `findings: []` | `--format json` (default), `--render`, `--samples`, `--times`, `--max-frames` |
| `animatix baseline <file>` | PNG snapshot generation and regression comparison | `--update`, `--times`, `--max-frames`, `--threshold`, `--width`, `--height`, `--format`, `--set`, `--plugin` |
| `animatix storyboard <file>` | Qualitative visual evidence: keyframe beats and resting contact sheet | `-o`, `--times`, `--cols`, `--max-frames` |
| `animatix check` / `lint` | Fast syntax, typing, and static semantic checks (iterative assistance, not a final gate) | Standard compiler flags |

Exit codes across all tooling:
- `0`: Clean. No findings above the `--fail-on` threshold, or baseline matches.
- `1`: Finding detected at or above the threshold, or a baseline diff was detected.
- `2`: Cannot run (build failures, unreadable input, missing baseline, or `--render always` without an adapter).

---

## 1. 判定权与工作流 (Decision Authority & Workflow)

在排版与动画视觉质量的验证中，**不用想着把所有问题都由机器规则自动兜住，主要依然依靠开发者目视审阅**。这与 Hyperframes 的设计哲学深度对齐：

> *"Look at the PNGs before tuning automated warnings: your eye catches what the auditor misses, and the auditor catches what the eye misses."*
> — Hyperframes ([DeepWiki](https://deepwiki.com/heygen-com/hyperframes/2.5-diagnostics-validation-and-check-gate))

在 Animatix 中，`storyboard` / 快照图片是一等步骤，而非可选补充；`audit` 负责给出高确定性的硬事实、结构/像素度量以及排版候选（candidates），而不是越俎代庖替人做视觉美学决策。

### 标准五步工作流

1. **编写场景**：编写 `.amx` 源码，由 DSL 容器、组件与动作组织内容。不要写外挂断言文件或手写时间采样表。
2. **先看图（一等步骤）**：
   运行 `animatix storyboard <file> -o board.png` 生成接触表，或者运行 `animatix audit <file> --evidence <dir>` 渲染每帧与 per-finding 证据裁切图。**先直接看 PNG 图片**——你的眼睛能一眼发现重叠是否破坏可读性、过渡是否自然、静止帧是否完整。
3. **审阅 findings（事实与候选）**：
   查看 `animatix audit <file> --brief --format json` 输出的 findings。
   - **硬事实**（如 `build.error`、`frame.blank`、`build.never-revealed`）：属于确定性缺陷，必须排查并修复源码。
   - **观察候选**（如 `overlap.undeclared`、`actor.clipped`、`text.overflow`）：指示潜在的排版冲突，结合 `--evidence` 裁切图与源码位置定位。
4. **意图声明（源码标记）**：
   只有在**开发者目视确认某处重叠或溢出是刻意设计**（例如徽标悬浮卡片边缘、装饰背景穿插）之后，才在对应的 actor 声明前添加 `// layout: allow-* - <原因>`。
   - **标记的作用是「让下次报告更干净」，绝不是「为了让门禁满意而猜标记」**。
   - 如果重叠是无意的，调整布局容器（`Stack`/`Row`/`Col`）或尺寸，而不是滥加豁免。
5. **基线回归**：
   使用 `animatix baseline <file>` 进行像素回归防护。当检测到 diff 时，人眼审阅 diff 图确认变更完全符合设计意图，才执行 `animatix baseline <file> --update`。

---

## 2. `animatix audit` 命令行选项

- `--policy default|site`:
  - `default`: 标准模式。输出确定性硬事实（Error）与排版观察候选（Warning/Info）。
  - `site`: 嵌入场景验证模式，追加静止构图约束（如首帧 ink 与 held frame 构图）。严格性由 `--fail-on warning` 提供，机器不把几何候选伪装成定论。
- `--format text|json`: Choose between human-readable text or structured JSON (`schema_version: 2`).
- `--brief`: Emits compact JSON (`kind: "audit.brief"`) containing "where to look" fields (`crop_path`, `frame_path`, `time_s`, `actor`, `source_line`) and summary evidence, omitting raw frame dumps and full suggestion texts. Recommended for LLM loops.
- `--render auto|always|never`:
  - `auto` (default): Uses the GPU pixel tier when an offscreen adapter is available; skips it in headless/CPU environments.
  - `always`: Demands a working GPU adapter; exits 2 if unavailable.
  - `never`: Disables the pixel tier; runs structural checks only.
- `--samples auto|keyframes|N`: Time-sampling strategy. `auto` chooses structural landmarks; `keyframes` checks explicit scene keyframes; `N` evaluates a uniform grid of $N$ frames.
- `--times T1,T2,...`: Exact sample times in seconds. Overrides `--samples` when present.
- `--max-frames N`: Caps the maximum number of evaluated frames (default: 24).
- `--fail-on error|warning|never`: Threshold for returning exit code 1.
- `--detail summary|full`: Controls text output verbosity (JSON is always complete).
- `--storyboard <path>`: Also renders a storyboard contact sheet after the audit.
- `--frames-dir <dir>`: Saves individual rendered sample frames to disk.
- `--evidence <dir>`: 按每条 finding 的 bounds 从渲染帧裁切出 `*_*.png`，并将路径写进 `evidence.crop_path` 与 `evidence.frame_path`，供开发者/LLM 直观目视定位问题所在（"show you what the auditor saw"）。
- `--set NAME=VALUE`: Overrides build-time variables (repeatable).
- `--plugin <path>`: Loads a native plugin.
- `--width <px>`, `--height <px>`: Overrides canvas resolution.

---

## 3. Schema v2 (`audit`, `measure`, `baseline`)

All structured output uses a unified format (`schema_version: 2`).

### Top-Level Object
- `schema_version`: `2` (integer).
- `kind`: `"audit"`, `"audit.brief"`, `"measure"`, or `"baseline"`.
- `input`: Source file path.
- `target`: Target description object (`kind`, `duration_s`, `resolution`).
- `render`: Render engine status (`status: "rendered"|"skipped"|"failed"`, `reason`).
- `sample_times_s`: Array of sampled timestamps in seconds.
- `frames`: Array of per-frame facts (omitted in `--brief`).
- `coverage`: Check coverage summary (`structural`, `render`, `complete`, `skipped_rules` (actual rules), `checks_run`, `checks_skipped`).
- `findings`: Array of validation issues. Empty for `measure`.
- `summary`: Issue counts (`errors`, `warnings`, `infos`, `skipped`, `waived`).

### Finding Object (`findings[]`)
- `code`: Stable issue identifier string (e.g., `"overlap.undeclared"`, `"overlap.cluster"`).
- `severity`: `"error"`, `"warning"`, or `"info"`.
- `persistence`: `"transient"` or `"held"` (settled state). 事实标签（标明缺陷是存在于瞬态过渡还是持续到静止状态），不影响 severity。
- `scene`: Active scene identifier.
- `time_s`: Timestamp in seconds where the issue occurs, or `null`.
- `actor`: Target actor label, or `null`.
- `source_line`: Target source line number, or `null`.
- `message`: Human-readable description.
- `evidence`: Supporting geometric or pixel facts, including per-finding crop paths when `--evidence` is set.
- `fix_hint`: Structured suggestion for automated remediation (`kind`, `target`, `source_line`, `snippet`, `marker`), or `null`.
- `waived_by`: Exact text of the source exemption marker that matched, or `null`.
- `suggestion`: Human-readable remediation instruction.

### Evidence Details (`evidence`)
Depending on the rule and execution flags, the `evidence` object provides measured values:
- `crop_path`: (When `--evidence` is provided) Path to per-finding cropped PNG showing the visual anomaly.
- `frame_path`: (When `--evidence` is provided) Path to the full rendered sample frame.
- `relation`: Structural relationship (`"same_stack"`, `"callout_target"`, `"legend_host"`, `"ancestor"`, `"during_transition"`, `"independent"`, `"generated_batch"`).
- `pair_count`: (For `overlap.cluster`) Total count of overlapped pairs.
- `sample_pairs`: (For `overlap.cluster`) First 5 pairs.
- `kept`: (For `overlap.cluster`) Number of pairs kept.
- `pair`: Array of two actor labels involved in a collision or occlusion.
- `overlap_area`: Area of bounding box overlap (pixels squared).
- `ratio_a`, `ratio_b`: Fraction of actor A or B's area that overlaps.
- `bounds_a`, `bounds_b`: Evaluated world-space bounding boxes `[x0, y0, x1, y1]`.
- `occluder`: Label of the covering actor.
- `probe_coverage`, `probed_points`, `covered_points`: Discrete sampling points for occlusion checks. Points without ink are excluded from the denominator.

### Frame Facts (`frames[]`)
- `time_s`: Timestamp of the frame.
- `scene`: Active scene name.
- `cast`: Number of active actors.
- `ink_fraction`: Whole-frame non-background pixel ratio (0..1), or `null` when the render tier was skipped.
- `changed_from_prev`: Fraction of pixels changed from the previous sample (0..1), or `null`.
- `actor_ink`: Map of `"scene:label"` to measured actor pixel coverage (0..1).
- `actors`: Array of actor states (`label`, `bounds` as `[x0,y0,x1,y1]` or `null`, `opacity`, `on_frame_fraction`, `draw_commands`).

---

## 4. Rule Catalog (硬事实与观察候选)

规则按判定性质分为**硬事实（Hard Errors）**与**观察候选（Observations / Candidates）**。机器只在硬事实上断言错误，不把几何观察候选伪装成定论；严格性由 `--fail-on warning` 提供。

### 确定性硬事实 (Hard Errors)
- `build.error`: Scene fails to compile or resolve names (non-exemptible).
- `build.never-revealed`: An actor declared before keyframe 0 lacks an entrance and remains permanently invisible (non-exemptible).
- `frame.blank`: Entire canvas has zero ink during an active playback window (non-exemptible).
- `scene.duration`: Declared duration does not match timeline requirements or contains invalid timing values.
- `actor.draw-missing`, `actor.ink-missing`: Build model says an actor is on-screen, but evaluation or pixel readback has no ink/draw data.
- `text.occluded-full`: Text is entirely hidden behind another actor (non-exemptible).
- `layout.marker-invalid`: Malformed exemption syntax or a marker that could not bind to an actor (non-exemptible).
- `layout.marker-abuse`: More than half of the visual actors carry waivers; blanket opt-outs hide real defects.
- `baseline.mismatch`: Reported by `animatix baseline`, not `audit`. Pixel diff must be visually reviewed and accepted with `--update`.

### 排版与视觉观察候选 (Observations / Warnings / Infos)
- `overlap.undeclared`: Two independent actors overlap without an explicit structural relationship or exemption.
- `overlap.cluster`: Aggregated cluster of multiple independent actor overlaps. Informative Warning; allows diagnosing dense collisions without flood.
- `overlap.explained`: An overlap explained by DSL structure (same `Stack`, `Callout`, `Legend`, transition, or `generated_batch`). Informational; never waived because it is an intentional structural relationship.
- `actor.clipped`: An actor's rendered bounds extend beyond the canvas.
- `text.overflow`: Text bounds exceed the container dimensions allocated by the layout engine.
- `text.occluded`: Text is partially covered by another opaque actor.
- `actor.opacity-never-positive`, `actor.never-on-frame`, `actor.reveal-missing`: Actor lifecycle signals.
- `entrance.not-visible`, `keyframe.redundant`, `beat.static`, `transition.endpoint`: Timing observations.
- `rest.ink-floor`, `rest.cast-erased`: Held resting frame lost its composition (evaluated under `--policy site`).

---

## 5. Source Exemptions (源码豁免标记)

When an overlap or visual overflow is confirmed by visual review to be an intentional design choice, mark it directly in the `.amx` source. Exemptions must appear immediately before the actor declaration and must include a human-readable justification.

```animatix
// layout: allow-overlap - Badge floats over the card margin
badge: Rect, size: (40, 40)
```

**标记的准则**：
- 先看图确认确实是意图内的排版设计，再加标记；**不要为了让门禁通过而盲目加标记**。
- 标记应加在最小的具体叶子元素/装饰 wrapper 上，**切勿加在场景根节点或大容器上**（"never a scene/root wrapper"）。

### Exemption Syntax

| Marker | Exactly waives |
|---|---|
| `// layout: allow-overlap - <reason>` | `overlap.undeclared` |
| `// layout: allow-occlusion - <reason>` | `text.occluded` |
| `// layout: allow-overflow - <reason>` | `actor.clipped` and `text.overflow` |
| `// layout: allow-rest - <reason>` | `rest.ink-floor` and `rest.cast-erased` |
| `// layout: ignore - <reason>` | All layout-class findings (`overlap.*`, `text.occluded`, `actor.clipped`, `text.overflow`) |

### Scoping Rules
- **Subtree Propagation**: A marker applies to the declared actor and its entire descending subtree. Apply markers to the smallest containing element to limit the blast radius.
- **Structural Explanations**: Elements in the same `Stack`/`Row`/`Col`/`Grid`/`Group` container, within one `Filter`/`Glass` scope, `Callout` targets, `Legend` hosts, or elements undergoing active transitions are automatically classified as `overlap.explained` and do not require markers.
- **Compatibility**: Legacy `// rest-check: exempt - <reason>` is treated as an alias for `allow-rest`. Comments matching `// house-style: <note>` are non-binding notes and do not exempt rules.

### Non-Exemptible Codes
The following codes represent structural, compiler, or severe integrity failures and **cannot be waived** by any marker:
- `build.error`
- `build.never-revealed`
- `frame.blank`
- `text.occluded-full`
- `baseline.mismatch`
- `layout.marker-invalid`

---

## 6. 如何为语料生成基线

为确保渲染结果的稳定性，请按照以下步骤生成并维护基线：

1. **生成基线**：运行 `animatix baseline <file>` 为场景生成 PNG 快照基线。
2. **确认变更**：当基线比对失败时，先运行 `animatix storyboard <file> -o diff_check.png` 目视确认 diff 是否为意图内的 visual update。
3. **更新基线**：确认意图后，使用 `animatix baseline <file> --update` 更新基线资产。
4. **批量脚本**：对于大型项目或语料，使用 `scripts/baseline-scenes.sh` 批量处理。
5. **CI 门禁**：`scripts/transformer-verify.sh` 与 `scripts/dogfood-verify.sh` 会在本地执行全量基线校验，确保变更不破坏已验证的基线。

---

## 7. Known Limitations

- **机器判定边界**：机器无法判断「这个重叠/越界是不是开发者的刻意设计」，这是设计边界而非缺陷。工具链的目标不是通过规则兜住所有排版问题，而是自动给出确定性事实与观察候选。Per-finding 证据裁切图与源码标记是配套辅助手段，最终判定权始终属于开发者目视审阅。
- **No Per-Actor Isolated Pass**: Ink is measured inside an actor's bounds on the composed frame, so overlapping actors contribute to each other's measurement. Occlusion uses a 27-point probe grid over the composed frame instead of a separate actor render.
- **Clip Path Approximation**: The engine does not expose effective clip paths to tooling. Probe points without ink are excluded from the denominator, which approximates clipped-to-empty regions but does not model arbitrary clip geometry.
- **Modal Colour**: The reference colour is the frame's own dominant colour; a scene where one actor covers most of the canvas can under-report that actor's ink.
- **Headless Environments**: Without a GPU/Vulkan/Metal adapter, the pixel tier is skipped under `--render auto` and `frames[].ink_fraction` is `null`. Structural rules still run. `baseline` cannot run without an adapter.
