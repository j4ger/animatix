# Audit and Measurement

Animatix provides a unified validation and measurement framework designed for both human authors and LLM agents. It verifies layout geometry, visual appearance, typography, and temporal continuity, using a "visual-first" deterministic pipeline.

**Core principle:** Facts are derived from the scene by the tool, verdicts are produced by rules, visual review provides intent, and regression is carried by PNG baselines. Authors and agents write `.amx` only — they never write assertion files, guess sample times, or tune ink thresholds.

## 三类使用者与判定权

在排版与动画视觉质量的验证中，不用想着把所有问题都由机器规则自动兜住，主要依然依靠开发者目视审阅。这与 Hyperframes 的设计哲学深度对齐：

> *"Look at the PNGs before tuning automated warnings: your eye catches what the auditor misses, and the auditor catches what the eye misses."*
> — Hyperframes ([DeepWiki](https://deepwiki.com/heygen-com/hyperframes/2.5-diagnostics-validation-and-check-gate))

| User | Workflow Focus |
|---|---|
| **LLM Agent** | 视觉循环 + `audit --delta` 自动感知新增变更 |
| **Human Author** | 视觉接触表 (Storyboard) + Diff 图片审阅 + 硬错误修复 |
| **CI Gate** | 硬错误检测 + Budget 预算控制 + Baseline 回归防护 |

Animatix 采用「视觉优先」的设计，将 Remotion `still` / Manim MCP 的经验内化为工作流：`storyboard` 返回信息量最大的节拍/区间，`audit` 提供硬事实（Error）、像素度量以及基于视觉的候选警告（Warning/Info）。

## 五步视觉工作流

1. **编写场景**：编写 `.amx` 源码。禁止写外挂断言文件或手写时间采样表。
2. **先看图（一等步骤）**：运行 `animatix storyboard <file>` 先生成接触表；或者运行 `animatix audit <file> --evidence <dir>` 渲染每帧与 per-finding 证据裁切图。**先直接看 PNG 图片**——你的眼睛能一眼发现重叠是否破坏可读性、过渡是否自然、静止帧是否完整。
3. **审阅 Findings（事实与候选）**：查看 `animatix audit <file> --brief --format json` 输出。
    - **硬事实**（如 `build.error`、`frame.blank`、`build.never-revealed`）：属于确定性缺陷，必须修复。
    - **观察候选**（如 `overlap.undeclared`、`actor.clipped`、`text.overflow`）：结合证据裁切图 (`crop_path`) 与源码位置定位。
4. **意图声明（源码标记）**：只有在**开发者目视确认某处重叠或溢出是刻意设计**之后，才在对应的 actor 声明前添加 `// layout: allow-* - <原因>`。
    - **标记的作用是「让下次报告更干净」，绝不是「为了让门禁满意而猜标记」。**
5. **基线回归与增量审计**：使用 `animatix baseline <file>` 防护。在人工确认变更符合意图后执行 `animatix baseline <file> --update`。通过 `--delta` 模式，在审计时只关注新增的问题。

## 命令面

| 命令 | 职责 | 关键 Flags |
|---|---|---|
| `check` / `lint` | 解析/构建诊断与静态 lint | - |
| `measure` | 纯事实 JSON | `kind: "measure"` |
| `audit` | 事实 + 缺陷 + 有界 anchors | `--policy`, `--anchors`, `--candidates`, `--evidence`, `--delta`, `--budget`, `--fail-on` |
| `storyboard` | 节拍感知 contact sheet | `--cols`, `--times`, `--max-frames`, `--debug-bounds` |
| `image --time <t>` | 单帧预览 (Remotion still) | - |
| `baseline` | PNG 回归防护 | `--update` |

### 退出码
- `0`: Clean (或 baseline 匹配)。
- `1`: 检测到符合 `--fail-on` 阈值的问题 (或 baseline 不匹配)。
- `2`: 运行失败 (构建错误、输入无效、基线缺失或无适配器)。

## audit 选项

- `--policy default|site`: 验证策略。`site` 追加静止构图约束。
- `--anchors <N>`: `--brief` 模式下显示的 anchors 数量（默认 10）。
- `--candidates`: 包含 Info 级观察候选（默认过滤）。
- `--evidence <dir>`: 产出 per-finding 裁切 `<code>_<idx>_<t>s.png` 与标注图（全帧 + 2px 红框）。
- `--delta <FILE>`: 比较上一次 `audit --format json` 输出，只显示增量 finding；`--fail-on` 仅评估新增项。
- `--budget <FILE>` / `--update-budget`: 棘轮预算，统计 Error+Warning。超限则 exit 1。
- `--fail-on error|warning|never`: Exit 1 判定阈值。
- `--render auto|always|never`: 渲染策略。无 GPU 时 `auto` 会自动降级，`evidence` 为 `unavailable`。

## Schema v3

`schema_version: 3` 用于统一 `audit`、`audit.brief`、`measure`、`baseline` 输出。

### 核心结构
- **Top-level**: `schema_version, kind, input, target, render, sample_times_s, frames, coverage, findings, summary`。可选 `delta`, `budget`。
- **Finding 新增**: `anchor{time_s,frame_path,crop_path,annotated_path}`, `fix{kind,target,source_line,snippet,marker,applicability}`, `baseline_state` (仅 `--delta` 时)。
- **`--brief`**: `kind: "audit.brief"`。用 `anchors` 取代 `findings`，默认最多 10 条（优先显示有证据图的），按 severity→时间→code 排序；`omitted` 为被截断数。

## 规则目录

| Severity | Code | 可豁免 |
|---|---|---|
| **Error** | `build.error`, `build.never-revealed`, `scene.duration`, `actor.draw-missing`, `actor.ink-missing` | 否（`NON_WAIVABLE`） |
| **Warning** | `frame.blank`（**所有**采样帧都空白时）, `entrance.not-visible`（入场结束后从未可见）, `actor.never-on-frame`, `actor.opacity-never-positive`, `actor.reveal-missing`, `text.occluded`（覆盖率 ≥25%）, `text.occluded-full`（≥95%）, `text.overflow`, `layout.marker-invalid`, `layout.marker-abuse`, `rest.*`（`--policy site`） | `frame.blank` / `text.occluded-full` / `layout.marker-invalid` 否，其余是 |
| **Info** | `overlap.undeclared`, `overlap.cluster`, `overlap.explained`, `actor.clipped`（非文本/Plot）, `keyframe.redundant`, `beat.static`, `transition.endpoint`, `layout.marker-unused` | 是 |

*注意：`actor.opacity-never-positive` / `actor.reveal-missing` / `actor.never-on-frame` 会跳过背景层与 `for` 循环生成的子 actor（label 含 `__`）；`actor.clipped` 仅对文本/Plot 保持 Warning，其余降为 Info 观察。*

## 源码豁免标记

```animatix
// layout: allow-overlap - <reason>
badge: Rect, size: (40, 40)
```
- **原则**: 标记传播至子树；应用至最小元素（禁止场景根节点标记）。
- **卫生**: `layout.marker-unused` 会在标记不再匹配任何问题时生成 Info 提示。

## 预算与增量

- **Budget**: `.animatix/audit-budget.json` (记录文件 × code 限制)。
- **Commands**: `scripts/audit-budget.sh --check|--update`。
- **Gate**: `scene-audit` gate 强制棘轮（只降不升）。

## Known Limitations

- **机器判定边界**: 机器无法判断刻意与否，仅输出事实与观察。
- **精度依赖**: 细描边/小字号的 ink 探针不可靠，以引擎 draw commands 为准。
- **选帧**: 接触表选帧依赖渲染器；无 GPU 时退化为采样区间中点。
