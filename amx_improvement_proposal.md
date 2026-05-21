# Animatix (AMX) 改进问题清单与解决方案

> 基于源码审计 + 3个教学动画示例编写实践整理
> 审计日期：2026-05-21
> 仓库 commit：6109722

---

## 一、总体状态

| 状态 | 数量 | 说明 |
|------|------|------|
| ✅ 已解决 | 3 | `always let` 局部变量、`Arrow` 组件、`always` 的 `t` 语义（代码层面） |
| ⚠️ 半解决 | 2 | `Group` 容器（存在但无批量操作）、数学坐标↔像素（有 domain 无自动映射） |
| ❌ 未解决 | 12 | 核心语法和组件缺失 |
| 🆕 新增 | 4 | 源码审计中发现的新问题 |

---

## 二、P0 — 阻塞级（需立即处理）

### 1. 编译环境断裂
- 仓库声明 `edition = "2024"`（需 Rust ≥1.85），当前环境 Rust 1.75.0
- rustup 安装因 OOM 被反复 kill
- **解决**：降级到 `edition = "2021"` 或提供 Docker 容器 / 离线安装包

### 2. `always` 的 `t` 语义缺文档
- 代码已确认 `t` 是 **scene-local**（`Composition` 会把 global time 映射到 scene local time 再注入 env）
- 但作者无法从文档确认，只能猜测。进入 scene 2 时 `t` 不会继承 scene 1 的累积时间
- **解决**：在 architecture.md / 示例中明确声明；补充 `global_t` 变量供跨 scene 引用

### 3. `Arrow` 组件无官方示例
- 源码中 `ShapeType::Arrow` + `from`/`to`/`tip_length`/`tip_width` 属性全有，实现完整
- 但全部 12 个示例文件中从未出现
- **解决**：补充一个 `arrow_demo.amx` 官方示例，明确用法

---

## 三、P1 — 高优先级（阻碍数学教学场景）

### 4. 缺少坐标系/网格组件（`NumberPlane`）
- 没有数学坐标系专用的轴线、网格线、刻度标签
- `Grid` 存在但它是 **UI 布局容器**（CSS Grid 语义），不是数学网格
- 当前画坐标系只能手搓 20 条 `Line`
- **解决**：新增 `NumberPlane, x_range: (-5, 5, 1), y_range: (-3, 3, 1)`，自动包含轴线+网格线+刻度标签，支持 `fade-in grid` 整体控制

### 5. 屏幕像素坐标 vs 数学坐标无自动映射
- `Graph` 有 `x_domain`/`y_domain`，但内部子 actor 仍必须用**屏幕像素坐标**
- 同一数学场景中混杂两套坐标系，改分辨率即错位
- 手动估算映射：`(2, 2)` → `(806, 180)` 全靠目测
- **解决**：让 `Graph { ... }` 作为**坐标容器**，内部所有子 actor 使用数学坐标自动映射到屏幕像素

### 6. `rotation`/`scale` 是标量，无矩阵变换能力
- `rotation: f32` 只是绕自身中心的旋转角，`scale: f32` 只是各向同性缩放（`scale_x == scale_y`）
- 无法实现：**错切（shear）**、**非等比缩放**、**绕任意点旋转**、**镜像翻转**、**任意 2×2 线性映射**
- 线性代数教学核心——矩阵作用下的空间变形——**完全不可表达**
- `kurbo::Affine` 底层支持任意矩阵，但 AMX 语法只暴露了 2 个标量
- **解决**：新增 `transform: [f64; 6]` 属性（完整 2D 仿射矩阵 `[a, b, c, d, tx, ty]`），与 `rotation`/`scale` 作为**独立变换层共存**。乘法顺序：`parent × translate(position) × transform(matrix) × rotate(rotation) × scale(scale)`。GUI 默认显示 `rotation`/`scale_x`/`scale_y`/`skew`，高级模式展开 2×3 矩阵编辑器 + 实时预览（单位正方形变形效果）

### 7. `always` 表达式能力严重割裂
- build-time `evaluate_expr` 支持：**条件、函数调用、方法、索引、对象构造**
- always/modifier IR 的 `compile_expr` 只支持：**算术、三元条件、Sin/Cos/Lerp/Format**
- 同一表达式在 keyframe 里能写，在 `always` 里写不了，失败方式还是**静默降级**（`ModifierExpr::Unsupported` 回退到 `evaluate_expr`，性能暴跌）
- **解决**：扩展 modifier IR，至少支持 `Index`（数组索引）、`Method`（方法调用）、`Closure`（闭包）——优先级：`Index` > `Method` > `Closure`

### 8. `always` 不支持 `for` 循环
- `lower_modifier_stmt` 中 `ForLoop` 返回 `UnsupportedStatement`
- 影响：无法批量更新一组对象（如 10 个粒子的位置）
- **解决**：在 modifier IR 中添加 `For` 指令（遍历数组或 Range）

### 9. `always` 与 keyframe 冲突机制缺失
- `always` 的 overrides 和 keyframe track 采样之间**无优先级系统**
- 两者同时写同一属性时行为不可预测
- **解决**：引入优先级栈（`always` 为 base 层 priority 0，keyframe 为 override 层 priority 100），或允许 `always` 中检测 `is_animating(property)` 避让 keyframe 插值区间

### 10. `builtin` 函数库极贫瘠
- 只有 4 个：`Sin`、`Cos`、`Lerp`、`Format`
- 缺少：`tan`、`sqrt`、`exp`、`log`、`atan2`、`clamp`、`abs`、`min`/`max`、`random`、`floor`、`ceil`
- **解决**：批量添加到 `BuiltinFn` enum 和 `CallBuiltin` 处理分支（约 20 行 per function）

---

## 四、P2 — 中优先级（影响开发效率与稳定性）

### 11. Group 容器无批量操作语法
- `ActorKindId::Group` 存在，用于逻辑分组和层级管理
- 但没有 `fade-out group [400ms]` 或 `group.opacity = 0 [500ms]` 这种批量操作
- Winding scene 退场需要逐条 fade-out 9 个 actor，写 9 行相同代码
- **解决**：让 `fade-in`/`fade-out`/`opacity`/`transform` 支持 Group 目标，递归应用到所有子 actor。`Group` 本身也可以设置 `transform` 属性，整体移动/旋转/变形一组对象

### 12. 缺少模板复用/参数化定义
- `pub let` 只能定义常量，不能定义参数化的 actor 组
- 每个 scene 的标题进场→展示→退场模式都要重写
- **解决**：引入 `template` 语法（如 `template title_card(title, subtitle) { ... }`），返回 Group 引用。或扩展 `ComponentDef` 的调用语法，使其像函数一样可复用

### 13. `CartesianPlot` 表达式能力弱
- `func` 是否支持 `if/else` 未文档化（build-time `evaluate_expr` 支持 `Expr::Conditional`，但 `always` 中不支持）
- 不支持分段定义（piecewise），画脉冲函数被迫用 `Rect` 手动拼
- **解决**：文档化 `func` 支持的表达式语法；考虑 `PiecewisePlot` 组件

### 14. 403 处 unwrap/expect/panic 泛滥
- 核心源码中 293 个 `.unwrap()` + 89 个 `.expect()` + 21 个 `panic!`
- CLI 遇到错误输入直接崩溃；长时间视频渲染中途失败丢失全部进度
- **解决**：分层错误处理——parser 层用 `Diagnostic` 报告语法错误；build 层用 `BuildReport` 累积语义错误；runtime 层用 `Result` 传播帧级错误。高频模块（`renderer/*`、`timeline/build.rs`、`parser.rs`）优先消化

---

## 五、P3 — 低优先级（长期增强）

### 15. 缺少高级数学可视化组件
- `VectorField`（向量场）、`Heatmap`（热力图）、`ContourSet`（等高线集）完全缺失
- 影响场论、优化、微积分教学
- **解决**：短期新增 `VectorField` 子组件（`graph: Graph { field: VectorField, func: (x, y) => (2*x, 4*y) }`）；中期 `ContourSet`；长期 `Heatmap`

### 16. 缺少真正的 Updater（逐帧回调 + dt）
- 没有 `dt`（delta time）变量，无法做物理积分（速度 → 位置）
- 没有 per-actor updater，只有全局 `always`
- **解决**：在 `frame_eval_env` 中注入 `dt`；引入 `updater actor { ... }` 语法，让单个 actor 拥有独立的更新逻辑

### 17. video.rs 存在重复 unsafe 代码 + FFI 内存安全风险
- 665 行和 1233 行存在**逐字复制**的 `rgba.as_ptr() as *mut u8` 强转不可变指针为可变指针送入 `rsmpeg::ffi`
- 违反 Rust aliasing rules，video 导出路径可能崩溃
- **解决**：提取共用函数 `fill_rgba_frame(ptr, w, h)` 消除重复；审查 `rsmpeg::AVFrame::fill_arrays` 是否真的需要 `*mut u8`，加 `// SAFETY:` 注释

---

## 六、总结

当前 AMX 在 **UI 动画、文本揭示、简单图形变换** 方面已具备完整能力。

但要承担**数学教育动画**的角色，必须优先补齐：

1. **坐标系基础设施**（`NumberPlane` + 数学坐标自动映射）
2. **矩阵变换**（`transform` 属性，从标量到完整仿射矩阵，与 `rotation`/`scale` 独立共存）
3. **`always` 表达式能力对齐**（补上 `Index`/`Method`/`Closure`，消除与 build-time 的割裂）
4. **编译环境可用**（降级 `edition` 或提供容器）

这四项是"数学动画"和"UI 动画"的分水岭。前两项解决"能不能在坐标系里画画"，第三项解决"能不能在动画中做复杂计算"，第四项解决"能不能编译运行"。

*openShrimp — "代码层面的能力不等于作者层面的能力。引擎里有 kurbo::Affine，但如果语法只给两个标量，那它就不是你的。"*