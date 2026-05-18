# Animatix (AMX) 语法改进建议汇总

> 基于三个教育动画示例（傅里叶变换、梯度下降、线性变换）的编写实践整理。

---

## 类别一：时间系统与状态管理

### 1. `always` 块的时间语义不明确

**背景**

多 scene 教育动画是 AMX 的核心目标场景之一。每个 scene 需要独立的时间线和动画逻辑。但在编写过程中，`always { ... }` 中的时间变量 `t` 究竟是 **scene-local（从当前 scene 开始计时）** 还是 **全局时间（从视频第 0 秒开始）**，没有任何文档或示例明确说明。

**问题**

- 如果 `t` 是全局时间，那么 scene 2 中写 `if t < 3.0` 会永远为 false（因为进入 scene 2 时全局时间早已超过 3 秒），导致所有时间驱动动画逻辑错乱。
- 如果 `t` 是 scene-local，那么 `always` 块在 scene 切换时会重置，但同样没有文档确认这一点。
- 这种不确定性迫使作者只能“猜测并祈祷”，无法可靠地编写跨 scene 的连续动画。

**可能解决方案**

- **方案 A**：明确 `t` 为 **scene-local**，并在文档中显式声明。每个 scene 开始时 `t = 0`。
- **方案 B**：引入双时间变量：`t`（scene-local）和 `global_t`（全局），由用户按需选择。
- **方案 C**：允许 `always` 块接收参数，如 `always(start_after: 0s) { ... }`，明确其与 scene 时间轴的绑定关系。

---

### 2. `always` 块不支持局部变量与状态声明

**背景**

在傅里叶变换示例中，需要让“缠绕频率”随时间切换（前 3 秒为 1.0，之后变为 1.7）。这是一个简单的状态机：存在一个随时间变化的变量 `freq`，多个 actor（tracker、com_dot 等）都依赖它。

**问题**

- `always` 中无法声明局部变量，只能把 `if/else` 重复内联到每个属性表达式中：
  ```amx
  always {
    tracker.at = if t < 3.0 { (640 + 100 * cos(t), ...) } else { (640 + 100 * cos(1.7 * t), ...) }
    com_dot.at = if t < 3.0 { (1060, 360) } else { ... }
    // 频率一变，要改四五个地方
  }
  ```
- 没有 `let freq = if t < 3.0 { 1.0 } else { 1.7 }` 这种写法，导致代码冗长、易错、难维护。
- 没有“状态”概念。无法表达“系统当前处于模式 A/B/C”这种离散状态。

**可能解决方案**

- **方案 A**：在 `always` 中支持 `let` 绑定：
  ```amx
  always {
    let freq = if t < 3.0 { 1.0 } else { 1.7 }
    tracker.at = (640 + 100 * cos(freq * t), 360 + 100 * sin(freq * t))
  }
  ```
- **方案 B**：引入 `state` 块，允许声明跨帧保持的变量（类似寄存器）：
  ```amx
  state {
    let mut freq = 1.0
  }
  always {
    if t >= 3.0 { freq = 1.7 }
    tracker.at = (640 + 100 * cos(freq * t), ...)
  }
  ```
- **方案 C**：允许在 scene 顶部定义 `pub let` 表达式，并在 `always` 中引用（目前 `pub let` 似乎只在 scene 间共享，不支持动态表达式）。

---

### 3. `always` 与 keyframe 动画的冲突机制缺失

**背景**

理想的动画系统应该允许“底层持续驱动”（如 always 让物体绕圈）与“上层关键帧覆盖”（如 `#3s tracker.at = (100, 100)`）共存。但在实际编写中，这两者会互相覆盖，行为不可预测。

**问题**

- `always` 每帧都在写属性，而 keyframe（`#3s actor.at = ...`）也在写同一属性，**优先级规则未定义**。
- 作者无法判断：keyframe 播放时是否会临时压制 always？还是 always 会覆盖 keyframe 的插值结果？
- 这导致“时变系统”的可控性非常弱。例如，想让物体先 keyframe 移动到一个位置，然后再 always 绕圈——目前无法实现。

**可能解决方案**

- **方案 A**：引入 **updater 优先级栈**。`always` 是基础层（priority 0），keyframe 是覆盖层（priority 100）。keyframe 播放期间自动压制低优先级驱动。
- **方案 B**：允许在 `always` 中检测“是否处于 keyframe 插值中”：
  ```amx
  always {
    if !is_animating(tracker.at) {
      tracker.at = (640 + 100 * cos(t), ...)
    }
  }
  ```
- **方案 C**：显式语法区分“覆盖型 keyframe”和“叠加型 keyframe”。

---

## 类别二：几何坐标系统

### 4. 缺少坐标系/网格组件（NumberPlane 等价物）

**背景**

数学教育动画的核心是“在笛卡尔坐标系中展示几何对象”。Manim 中 `NumberPlane()` 是几乎所有数学场景的基础。

**问题**

- AMX 没有任何内置的 **坐标网格、坐标轴、刻度线、标签** 组件。
- 画一个 3×3 网格需要手动声明 6 条 `Line`（linear_transform.amx 中已实现）。如果要画 10×10 网格并做动画，需要 20 条 `Line` 声明。
- 坐标轴标签、刻度数字完全无法表达（`Text` 只能放固定内容，不能批量生成数字）。
- 这导致**任何需要坐标系的数学场景**都面临极高的代码冗余。

**可能解决方案**

- **方案 A**：新增 `NumberPlane` 或 `Grid` 组件：
  ```amx
  grid: NumberPlane, x_range: (-5, 5, 1), y_range: (-3, 3, 1), at: scene.center, size: (600, 400)
  ```
  自动包含轴线、网格线、刻度标签。支持 `fade-in grid` 整体控制。
- **方案 B**：至少新增 `Axis` 组件，可配合 `Graph` 使用，让 `Graph` 自动绘制轴线和刻度。
- **方案 C**：允许 `Graph` 的子节点使用**数据坐标**而非屏幕坐标，由 `Graph` 自动映射。这样可以在 `Graph` 内直接放 `Circle`、`Arrow` 等对象，而不需要手动换算像素。

---

### 5. 屏幕像素坐标与数学坐标的映射全靠手动估算

**背景**

`Graph` 组件有 `x_domain`/`y_domain` 和 `size`/`at`，但两者之间的精确映射关系未文档化。在 gradient_descent.amx 中，需要把损失函数坐标 `(2, 2)` 映射到屏幕像素 `(806, 180)`，作者只能靠目测反推。

**问题**

- 单位映射比例（像素/单位）在不同 `size` 和 `size` 下不同，改分辨率会导致所有坐标错位。
- `Graph` 内部的 `CartesianPlot` 使用数学坐标，但 `Graph` 外部的 `Circle`、`Text`、`Arrow` 必须使用屏幕像素坐标——**同一套数学场景中混杂两套坐标系**。
- 没有 API 可以把数学坐标转换为屏幕坐标，反之亦然。

**可能解决方案**

- **方案 A**：允许 `Graph` 作为**坐标容器**，其内部所有子 actor 使用数学坐标：
  ```amx
  graph: Graph, x_domain: (-3, 3), y_domain: (-3, 3), size: (500, 500), at: (640, 380) {
    point: Circle, radius: 6, at: (2, 2)   // 自动映射为数学坐标
    arrow: Arrow, from: (2, 2), to: (1.2, 1.2)  // 同样使用数学坐标
  }
  ```
- **方案 B**：提供坐标转换函数或表达式：
  ```amx
  let screen_pos = graph.to_screen((2, 2))
  ```
- **方案 C**：`Graph` 暴露 `origin`、`scale_x`、`scale_y` 等只读属性，供外部计算使用。

---

### 6. 缺少“组变换”或“容器级矩阵变换”

**背景**

线性变换/特征向量这个选题的核心视觉效果是**整个坐标网格在矩阵作用下一起变形**。Manim 中可以通过 `ApplyMatrix` 直接对整个 `NumberPlane` 做变换动画。

**问题**

- AMX 只能对**单个 actor** 的属性做动画（`i_vec.to = ...`）。
- 无法对一组 actor（如整个网格的 20 条线）应用统一的矩阵变换。
- 这意味着“整个空间被拉伸”这种核心几何直觉，在 AMX 中**几乎不可行**——作者被迫只能动画三个箭头，而放弃网格变形。
- 即使手动计算每条线的端点，动画代码量也会爆炸（20 条线 × 2 个端点 × 每帧插值）。

**可能解决方案**

- **方案 A**：新增 `Group` 容器，支持 `transform` 属性：
  ```amx
  grid: Group, at: scene.center {
    // 包含数十条线
  }
  #2s
  grid.transform = [[1.5, 0.3], [0.3, 1.5]] [2s, ease: ease-in-out]
  ```
- **方案 B**：对 `Graph` 支持 `transform` 属性，允许对整个坐标系做矩阵动画。
- **方案 C**：新增 `MapPoints` 动作，允许对任意 actor 的顶点（如 Line 的 from/to、Rect 的四个角）批量应用函数映射。

---

## 类别三：组件缺失与验证

### 7. `Arrow` 组件属性未在示例中验证

**背景**

向量是数学动画的核心元素。architecture.md 提到 `Arrow` 是 Graphic Primitive，但在全部 12 个示例文件中从未出现。

**问题**

- `Arrow` 的属性语法完全未知。梯度下降和线性变换示例中，只能**假设**它与 `Line` 一样用 `from`/`to`/`stroke`/`stroke_width`。
- 但实际上它可能用 `position` + `direction` + `head_size`，或者 `start`/`end`/`tip_length` 等完全不同的属性名。
- **风险**：包含 `Arrow` 的文件可能完全无法渲染，作者无从调试。

**可能解决方案**

- **方案 A**：补充一个 `arrow_demo.amx` 官方示例，明确属性列表（`from`、`to`、`stroke`、`stroke_width`、`tip_length`、`tip_angle` 等）。
- **方案 B**：如果 `Arrow` 尚未实现，应尽快实现或在文档中标注为“尚未可用”。
- **方案 C**：允许 `Line` 附加 `arrow_head: true` 属性，作为轻量级替代，减少新组件的学习成本。

---

### 8. 缺少高级数学可视化组件（VectorField、Heatmap、ContourSet）

**背景**

梯度下降需要展示 2D 向量场（每个点的梯度方向），傅里叶变换可以受益于热力图展示能量密度，线性变换可以用等高线展示二次型曲面。

**问题**

- **VectorField**：完全没有。梯度下降只能用单根 `Arrow` 表示一个点的梯度，无法展示“整个场的流动”。
- **ContourSet / 等高线**：只能用 `ImplicitPlot` 逐条手搓。`ImplicitPlot` 性能开销大（需要迭代求解），且不支持批量声明。
- **Heatmap**：完全不支持。无法展示损失函数的“颜色深浅 = 损失大小”。
- 这导致 AMX 在**场论、优化、微积分**等高等数学场景中的表达能力严重不足。

**可能解决方案**

- **方案 A（短期）**：对 `Graph` 新增 `VectorField` 子组件：
  ```amx
  graph: Graph, x_domain: (-3, 3), y_domain: (-3, 3) {
    field: VectorField, func: (x, y) => (2*x, 4*y), color: accent.primary
  }
  ```
- **方案 B（中期）**：新增 `ContourSet` 组件，支持一次声明多条等高线：
  ```amx
  contours: ContourSet, func: (x, y) => x*x + 2*y*y, levels: [1, 2, 4, 8], color: accent.primary
  ```
- **方案 C（长期）**：引入 `Heatmap` 或 `DensityPlot`，支持像素级颜色映射。

---

## 类别四：组合与复用抽象

### 9. 缺少 `Group` 容器与批量操作

**背景**

复杂 scene 包含大量 actor。Winding scene 退场时需要逐条 fade-out 9 个 actor，写了 9 行几乎相同的代码。

**问题**

- 没有 `Group` 概念，无法把多个 actor 打包成一个逻辑整体。
- 没有批量操作语法。Manim 中 `self.play(FadeOut(group))` 一句话，AMX 中要写 9 行 `fade-out actor [300ms]`。
- 没有**模板复用**。如果多个 scene 都需要“标题 + 副标题 + 退场”这个模式，每次都要重写。

**可能解决方案**

- **方案 A**：新增 `Group` 容器：
  ```amx
  scene_content: Group {
    header: Text, ...
    graph: Graph, ...
    formula: Math, ...
  }
  #2.5s
  fade-out scene_content [400ms]   // 批量退场
  ```
- **方案 B**：允许 `fade-out` 接受列表：
  ```amx
  #2.5s
  fade-out [header, graph, formula] [400ms]
  ```
- **方案 C**：引入**模板/子程序**（类似 Manim 的 `VGroup` 或函数）：
  ```amx
  def title_slide(title_text, subtitle_text) {
    title: Text, text: title_text, ...
    subtitle: Text, text: subtitle_text, ...
    // 返回 group 引用
  }
  ```

---

## 类别五：数学表达式能力

### 10. `CartesianPlot` 闭包不支持条件与复杂表达式

**背景**

在 Spectrum scene 中，原本想用 `CartesianPlot` 画一个脉冲函数（在 f=1.0 处出现尖峰，其余为 0），以精确展示频谱。

**问题**

- `CartesianPlot` 的 `func` 是否支持 `if/else` 表达式完全不确定。plotting.amx 中所有 func 都是纯数学表达式（`x*x`、`sin(2*t)`）。
- 即使支持，parser 对条件表达式的语法规则未知（括号规则、返回值类型是否必须统一等）。
- **结果**：被迫放弃 plot 的灵活性，用 `Rect` 手动拼了一个频谱柱，既不精确也不美观。

**可能解决方案**

- **方案 A**：在文档中明确 `func` 支持完整的表达式语法，包括 `if/else`、`let`、逻辑运算符。
- **方案 B**：提供 `PiecewisePlot` 或 `ParametricPlot` 的扩展，允许分段定义：
  ```amx
  spectrum: CartesianPlot, pieces: [
    { domain: (0, 0.9), func: (x) => 0 },
    { domain: (0.9, 1.1), func: (x) => 1 },
    { domain: (1.1, 3), func: (x) => 0 }
  ], color: accent.warning
  ```
- **方案 C**：允许 `func` 引用外部 `pub let` 定义的函数或参数化表达式。

---

### 11. 缺少参数化定义与函数抽象

**背景**

三个示例中有大量重复模式：标题进场 → 内容展示 → 退场。如果能定义一个可复用的“标题幻灯片”模板，代码量可以减少 50%。

**问题**

- `pub let` 只能定义常量（如 `pub let accent_color = accent.primary`），不能定义参数化的 actor 模板。
- 无法定义“函数”来生成一组 actor。
- 这导致每个 scene 都要从头写一遍 `title`、`subtitle`、`fade-in`、`fade-out` 的样板代码。

**可能解决方案**

- **方案 A**：引入 `template` 或 `macro`：
  ```amx
  template title_card(title, subtitle) {
    t: Text, text: title, font_size: 48, anchor: scene.center, at: scene.center
    s: Text, text: subtitle, font_size: 22, anchor: scene.center, at: scene.center, offset: (0, 70)
    // 自动生成 fade-in / fade-out 绑定
  }
  # Intro
  title_card("傅里叶变换的直觉", "将信号分解为旋转圆盘的叠加")
  ```
- **方案 B**：允许 `pub let` 绑定 lambda / 匿名函数（如果表达式语言支持）。
- **方案 C**：引入 scene 继承或混入（mixin）机制，让 scene 可以“继承”一个基础模板并覆盖部分内容。

---

## 类别六：动态系统与交互

### 12. 缺少真正的 `Updater`（逐帧回调）机制

**背景**

Manim 的核心优势之一是 `.add_updater(lambda m, dt: ...)`，它允许任意 actor 每帧根据任意逻辑更新自己。AMX 的 `always` 是这个机制的近似，但过于粗糙。

**问题**

- `always` 是“全局覆盖”的——一旦进入，该 actor 的所有属性都被它接管，keyframe 无法介入。
- 没有 `dt`（delta time）概念，无法做基于物理的积分（如速度 → 位置）。
- 无法让不同的 actor 拥有**不同的、独立的**更新逻辑。`always` 更像一个全局回调块，而不是每个 actor 的独立 updater。

**可能解决方案**

- **方案 A**：允许为单个 actor 附加 `updater`：
  ```amx
  orbiter: Circle, radius: 12, color: accent.primary
  updater orbiter {
    let angle = t * 2.0
    at = (640 + 200 * cos(angle), 360 + 120 * sin(angle))
  }
  ```
- **方案 B**：在 `always` 中提供 `dt` 变量，支持物理积分：
  ```amx
  state { let mut velocity = (0, 0) }
  always {
    velocity = velocity + acceleration * dt
    ball.at = ball.at + velocity * dt
  }
  ```
- **方案 C**：引入 `on_frame` 事件钩子，与 `always` 并存，提供更细粒度的控制。

---

## 优先级汇总

| 优先级 | 问题 | 影响范围 |
|--------|------|----------|
| **P0（阻塞）** | `always` 的 `t` 语义不明确 | 所有时间驱动动画 |
| **P0（阻塞）** | `Arrow` 组件未验证/未示例化 | 向量相关数学场景 |
| **P1（高）** | 缺少坐标系/网格组件 | 所有数学可视化 |
| **P1（高）** | 无组变换/容器矩阵变换 | 线性代数、几何 |
| **P1（高）** | `always` 无局部变量 | 状态驱动动画 |
| **P2（中）** | 数学坐标↔像素映射手动估算 | 精确数学布局 |
| **P2（中）** | 缺少 Group/批量操作 | 代码冗余、维护成本 |
| **P2（中）** | `CartesianPlot` 表达式能力弱 | 高级函数可视化 |
| **P3（低）** | 缺少 VectorField/Heatmap/ContourSet | 场论、优化可视化 |
| **P3（低）** | 缺少 Updater/物理积分 | 交互式/物理动画 |
| **P3（低）** | 缺少模板复用 | 开发效率 |

---

## 总体结论

AMX 目前非常适合：
- **布局驱动的 UI 动画**
- **文本揭示与简单图形变换**
- **声明式时序编排**

但在**精确数学坐标系统、批量几何变换、场可视化、状态管理**等方面，与 Manim 相比仍有显著差距。如果要让 AMX 真正承担起“数学教育动画”的替代角色，**P0 和 P1 级别的问题**（时间语义、坐标系统、组变换）是最优先需要补强的基础设施。

