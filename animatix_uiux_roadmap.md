# Animatix GUI/UX 改动清单（项目组版）

> **日期**: 2026-05-22  
> **来源**: GUI 代码审阅 + UX 愿景设计合并  
> **原则**: POC 阶段，不考虑向后兼容。先做架构，再做交互。

---

## 一、P0 — 架构层（不做完，上层全是沙上建塔）

### 1. `LiveDocument` 统一文档模型

**问题**: `EditorBuffer` 和 `DocumentSession` 双轨运行。用户改代码 → 需手动触发热重载；`HotReloader` 监听文件 → 但编辑器内容已不同步。**最容易丢工作。**

**方案**: 编辑器就是唯一事实来源。放弃文件系统中间层。

```rust
pub struct LiveDocument {
    cells: Vec<Cell>,                    // 编辑器持有
    last_valid_timeline: Timeline,       // 上次成功解析的缓存
    diagnostics: Vec<Diagnostic>,        // 当前错误列表
}

// 可视化编辑（拖拽改属性）必须同步回 Cell 文本，
// 用 AST Span 精确替换 token，不是粗暴查找替换。
```

**关键文件**: `editor.rs`, `document.rs`, `app/mod.rs`

---

### 2. Command 模式 + 撤销系统

**问题**: `UiActions` 有 40+ 个 `Option<T>` 字段，每帧 `if let Some(x)` 串行处理。无优先级、无时序保障、无法扩展。

**方案**: 统一 Command 枚举，每帧消费 `VecDeque<Command>`。Undo Stack 存 Command 而非文本快照。

```rust
pub enum Command {
    Actor(ActorCommand),      // Select/Rename/Delete/Add
    Scene(SceneCommand),      // Add/Reorder
    Timeline(TimelineCommand), // Scrub/SetKeyframe
    Property(PropertyCommand), // SetValue
}
```

**关键文件**: `app/mod.rs`（`update()` 循环末尾的批量处理逻辑）

---

### 3. Store 架构拆分

**问题**: `AppState` 同时持有文档/运行时/UI/持久化/编辑器/文件系统，任何子模块改动触发全量重编译。

**方案**: 拆为四个独立 Store，通过 Command dispatch 修改。

```rust
pub struct Stores {
    pub document: RwLock<DocumentStore>,   // cells + 懒解析 timeline
    pub runtime:  RwLock<RuntimeStore>,   // 预览 + 播放器
    pub ui:       RwLock<UiStore>,        // 面板、选中项
    pub workspace: RwLock<WorkspaceStore>,// 文件树、设置
}
```

**关键文件**: `app/mod.rs`（`AppState` 结构体）

---

### 4. Source Span 系统

**问题**: 画布拖拽 actor 改属性后，无法精确定位回代码中的 token 位置，导致"画布→代码"反向编辑不可行。

**方案**: `animatix_analyzer` 输出每个 AST 节点的精确 source location（行、列、长度）。

**用途**: 所有反向编辑的基础——拖拽改 position 时，精确替换 Cell 中对应 token。

**关键文件**: `animatix_analyzer` crate（需新增输出）

---

## 二、P1 — 画布与直接操作（体验核心）

### 5. 画布中心制布局重构

**当前**: Editor 55% | Preview + 三栏堆叠面板 45%。Editor 占太大，右边太拥挤。

**新布局**:
```
┌─────────────────────────────────────────┐
│ Toolbar                                  │
├─────────────────────────────────────────┤
│                                          │
│              CANVAS (60-70%)              │  ← Preview + 浮动卡片 + Gizmo
│                                          │
├────────────────────┬────────────────────┤
│ Property Stream    │ Timeline Panel      │  ← 底部双窄栏
│ (可折叠)            │ (Dope/Graph/Strip)  │
└────────────────────┴────────────────────┘
```

**响应式**: 
- >1600px: Canvas 70% | Stream 15% | Timeline 15%
- 1200-1600px: Canvas 65% | Stream 20% (可折叠) | Timeline 15%
- <1200px: Canvas 100% | Stream 隐藏(Tab召唤) | Timeline 压缩 strip

**关键文件**: `app/mod.rs`（egui_tiles 布局配置）

---

### 6. 浮动属性卡片

**取代**右侧 Inspector 面板。

选中 actor → 旁边弹出半透明属性卡片 → 直接操作（色轮、XY滑块、旋转拨盘）→ 代码实时更新 → `Esc` 消失。

**为什么**: 用户注意力在画布上，不需要左右扫视。

**关键文件**: 新增 `app/preview/floating_card.rs`

---

### 7. 2D Gizmo System

**缺失**: 无变换手柄、无包围盒、无吸附线视觉反馈、多选无法批量操作。

**方案**:
```rust
pub enum Handle {
    TranslateX, TranslateY, TranslateXY,
    Rotate,
    ScaleCorner, ScaleEdge,
}
```

交互:
- 拖拽 actor = 直接移动（同步改 Cell position）
- Shift + 角点 = 等比缩放
- 显示对齐线/网格吸附反馈
- 多选时联合包围盒

**关键文件**: 新增 `app/preview/gizmo.rs`，修改 `app/preview/mod.rs`

---

### 8. 时间透镜 — 按住 Space 拖拽

**问题**: 时间轴面板常年占底部空间，但 scrub 时间是"频繁但短暂"的操作。

**方案**: 时间是**按需召唤的 HUD**。

交互:
```
按住 Space → 光标处弹出圆形时间透镜
           → 圆环上标着 keyframe（小圆点）
           → 拖拽改变时间，中心显示时间码
           → 滚轮缩放时间范围
           → 松开 Space → 透镜消失
```

类似游戏 radial menu，不是 Blender timeline scrub。

**关键文件**: 新增 `app/preview/time_lens.rs`

---

### 9. 全局 Timeline Panel

**缺失**: 时间轴功能分散在 Transport Bar、Keyframe Table、Dope Sheet 三个地方，无全局视图。

**新增**: 底部右侧独立面板，分层设计：
- Scene Track: scene 排列
- Actor Track: 每 actor 一行
- Keyframe: 不同形状区分类型
- Playhead: 可拖拽，联动预览
- 范围滑块: 工作区/导出范围
- Markers: 标记功能

**关键文件**: 新增 `app/panels/timeline_panel.rs`

---

### 10. 时间感知 Inspector

**问题**: 用户不知道当前修改的是"默认值"还是"某个 keyframe 的值"。

**方案**: 属性行旁显示 diamond 状态。

```
Position  [ 100 │ 200 ]  ◆ 0.0s     ← ◆ = 当前时间有 keyframe
Rotation  [ 45° ]        ◆ 0.5s
Scale     [ 1.0 ]        ○          ← ○ = 无 keyframe，改默认值
```

规则:
- 点击 `○` → 在当前时间创建 keyframe
- 点击 `◆` → 弹出 keyframe 操作菜单
- Keyframe Mode 开启时，不在 keyframe 时刻修改 → 自动创建 keyframe

**关键文件**: `app/panels/inspector/mod.rs`（`property_widget`）

---

### 11. Property Stream

**不是**按 Transform/Style/Shape/Text/Media 语义分组。
**而是**按**动画强度**排序。

```
🔥 position     ◆◆◆○○○○○○○○  (12 kf)  ← 高动画强度，置顶
🔥 rotation     ◆◆◆○○○○○○○○   (8 kf)
─────────────────────────────────────
  color          ○○○○○○○○○○○   (0 kf)  ← 静态属性，折叠
  scale          ○○○○○○○○○○○   (0 kf)
```

- 默认按动画强度排序
- `Tab` 切换为语义分类视图

**关键文件**: `app/panels/inspector/mod.rs`

---

### 12. Graph Editor（F-Curve）

**缺失**: 只有 Dope Sheet 列表，看不到值随时间变化的曲线，无法直观调 easing 强弱。

**方案**: Inspector Keyframe 区域增加视图切换：List | Curve | Strip。

简化版先支持单个 float property（position.x / rotation），其他类型后续扩展。

**关键文件**: 新增 `app/panels/inspector/graph_editor.rs`

---

### 13. Ghost Edit / Onion Skin

**不是**手动开关 onion skin。
**而是**选中 keyframe 自动显示上下文。

```
选中 keyframe #2s：
  → 显示 #0s 轮廓（绿色虚线，30%透明度）
  → 显示 #4s 轮廓（蓝色虚线，30%透明度）
  → 运动路径连线
  → 拖拽时 ghost 保持不动作为参考系
```

**关键文件**: `app/preview/mod.rs`（渲染层叠加）

---

## 三、P2 — 差异化功能（超越 AE/Blender）

### 14. 自然语言指令栏 (NL Command Bar)

顶部常驻轻量输入栏：
```
File  Edit  View  │  [让 Circle_1 绕中心旋转一周]  │  ⌘K
```

- `⌘K` 聚焦
- 实时预览 Agent 打算做什么（代码 diff）
- `Enter` 确认，`Esc` 取消
- 指令历史支持上下回溯

**关键文件**: 新增 `app/shell/nl_command_bar.rs`

---

### 15. Agent 内联建议

Agent 以四种形态嵌入交互：

| 形态 | 示例 |
|------|------|
| 内联建议 | `position = (100, 200)` 下方 `"← 试试 (120, 200) 以对齐 Circle_2?"` |
| 轻量 Toast | "检测到循环运动，是否添加 oscillate()？" |
| 差异卡片 | 显示代码 diff，一键接受/拒绝 |
| 指令栏 | 复杂请求入口 |

**关键文件**: `app/components/`（新增内联建议组件）

---

### 16. 差异预览 (Diff Preview)

修改属性时自动 A/B 分屏：
```
┌─────────────┬─────────────┐
│  Before     │  After      │
│  [ ○ ]      │  [ ○  ]     │
└─────────────┴─────────────┘
```

技术：利用 AMX fast reparse，编译两个 timeline 版本分别渲染。

**关键文件**: `app/preview/mod.rs`

---

### 17. 智能吸附 (Smart Snap)

**不是**像素级吸附。
**而是**语义级吸附。

拖拽时自动吸附到：
- 其他 actor 边界（几何）
- 其他 actor 的 position 值（数值 → `position = Circle_2.position`）
- 布局容器 alignment 线（语义）
- 上一个 keyframe 位置（时间）

吸附时 HUD 提示具体目标。

**关键文件**: `app/preview/mod.rs`

---

### 18. 场景切片 (Scene Slices)

类似 Figma Variants / Photoshop Artboards，用于动画场景 A/B/C 对比。

操作: Duplicate 副本、拖拽跨 Slice 迁移、`1`/`2`/`3` 切换、批量导出。

**关键文件**: 新增 `app/panels/scene_slices.rs`

---

## 四、P3 — 视觉与打磨

### 19. Design Token 系统

**现状**: 颜色/间距/圆角硬编码混杂。

规范四组 Token：
```rust
color:   SURFACE_BASE/ELEVATED/WIDGET, TEXT_PRIMARY/SECONDARY, ACCENT, SUCCESS/WARNING/ERROR
spacing: XS/S/M/L/XL/XXL
radius:  NONE/SM/MD/LG/FULL
typography: H1/H2/BODY/CAPTION/MONO
```

辅助: `lint_theme.py` 扫描硬编码值。

**关键文件**: `app/theme.rs` → `app/design_tokens.rs`

---

### 20. Cell Editor 视觉重设计

- 获得焦点时左侧 accent 边框
- Keyframe 时间戳大号醒目（accent 色，可点击编辑）
- Cell 级别折叠/展开
- Cell 级别上移/下移/删除按钮（hover 显示）
- Code cell 与 Keyframe cell 视觉区分度增强

**关键文件**: `cell_editor/render.rs`

---

### 21. Preview Overlay 系统

```rust
pub struct PreviewOverlay {
    show_scene_bounds: bool,    // Scene 边界框
    show_grid: bool,
    show_guides: bool,
    show_actor_labels: bool,
    show_safe_area: bool,
}
```

**关键文件**: 新增 `app/preview/overlay.rs`

---

### 22. 语义着色 + 重构工具

深度集成 `animatix_analyzer`：
- `SymbolTable`: actor/scene/component 定义表
- 语义着色: ActorName / PropertyName / SceneName / Invalid（红色波浪线）
- 基础重构: RenameActor、ExtractScene、MoveToScene

**关键文件**: `cell_editor/highlighting.rs`, `completion_popup.rs`

---

## 五、当前代码问题速查表

| 文件 | 行号/范围 | 问题 | 优先级 |
|------|----------|------|--------|
| `app/mod.rs` | 全局 | `AppState` 上帝对象 | P0 |
| `app/mod.rs` | `update()` | `UiActions` 40+ 字段批量处理 | P0 |
| `editor.rs` | 全局 | `EditorBuffer` 与 `DocumentSession` 双轨制 | P0 |
| `document.rs` | 全局 | 没有 LiveDocument 统一模型 | P0 |
| `animatix_analyzer` | 需新增 | Source Span 输出 | P0 |
| `app/panels/inspector/mod.rs` | `property_widget()` | Keyframe Mode 状态与输入框行为混乱 | P1 |
| `app/panels/inspector/keyframe_table.rs` | 全局 | 只有 dope sheet，无曲线编辑器 | P1 |
| `app/preview/mod.rs` | 全局 | 无 Gizmo / 无包围盒 / 无吸附线 | P1 |
| `app/shell/transport_bar.rs` | 全局 | 时间轴功能过于简陋 | P1 |
| `cell_editor/render.rs` | 全局 | Cell 视觉粗糙 | P1 |
| `app/panels/inspector/property_groups.rs` | `transform_property_widget()` | Transform 矩阵与分解属性 UI 未实现 | P1 |
| `app/components/widgets.rs` | 全局 | 硬编码颜色和间距 | P2 |
| `app/theme.rs` | 全局 | Design token 系统不完整 | P2 |
| `completion_popup.rs` | 全局 | 补全数据源不明确 | P2 |
| `highlighting.rs` | 全局 | 缺少语义着色 | P2 |

---

## 六、实现排期

| Phase | 周期 | 内容 | 产出物 |
|-------|------|------|--------|
| **Phase 1** | 2 周 | Source Span + Command 系统 + LiveDocument | 底层可编译，撤销可用 |
| **Phase 2** | 2 周 | 画布布局重构 + 浮动卡片 + Gizmo | 可直接拖拽操作 |
| **Phase 3** | 2 周 | 时间透镜 + Property Stream + Timeline Panel + Ghost Edit | 时间交互完整 |
| **Phase 4** | 2 周 | NL 指令栏 + Graph Editor + 差异预览 + Agent 建议 | 智能层上线 |
| **Phase 5** | 持续 | Design Token + 语义工具链 + 场景切片 + 智能吸附 | 品质打磨 |

---

## 七、执行原则

1. **先做架构，再做皮肤**。P0 不做完，P1 的体验会反复崩。
2. **画布是主角**。任何可以在画布上直接做的事，不应该要求去面板里输数值。
3. **时间不是面板，是 HUD**。scrub 是频繁但短暂的操作，快闪店优于常驻租户。
4. **快捷键不超过 10 个**。复杂操作走 NL 指令栏。
5. **可视化编辑必须回写代码**。否则文本和可视化会再次分叉。
