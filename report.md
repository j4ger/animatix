# Animatix 语言与工具链问题报告

> 基于 AI 训练数据生成管线的实际使用体验，2026-05-28

---

## 一、Spec 文档问题

### 1. Polygon.points 语法文档错误（P0）

Spec 里写的是：

```amx
poly: Polygon, points: [(0,0), (100,0), (50,100)]
```

但编译器实际要求：

```amx
poly: Polygon, points: {(0,0), (100,0), (50,100)}
```

方括号 `[]` 会报 `expected identifier, expression, found '['`，花括号 `{}` 才正确。LLM 100% 照 spec 写会失败。

### 2. 元素属性文档缺失（P1）

Spec 没有完整的属性列表。以下问题无法从文档中确认：

- `stroke` vs `stroke_color` — 哪个是正确的？（实测 `stroke` 可用）
- `fill` vs `color` — Polygon/Rect 用哪个？
- `width` vs `stroke_width` — Line 的线宽属性名？
- `font_size` vs `font-size` — Text 的字号？
- `opacity` 的值域是 0-1 还是 0-100？

LLM 只能靠猜，猜错就编译失败。

### 3. 可用元素边界不清晰（P1）

Spec 提到了 `Polygon`、`Path`、`PlotCurve` 等，但没有明确列出**不存在**的元素。LLM 会很自然地发明这些"合理"但不存在的元素：

- `Graph3D`、`Line3D`、`Polyhedron` — 3D 场景
- `Circle` — 圆形（实际只有 `Ellipse`）
- `Arrow` — 箭头（只能用 `Line` 代替）
- `Triangle` — 三角形（只能用 `Polygon`）

需要一个明确的"不存在的元素"提示或更醒目的白名单。

### 4. 颜色系统文档分散（P2）

- hex 颜色（`#0f1117`）不支持，但 spec 没有醒目地说明
- `accent.primary`、`text.primary`、`stroke.default`、`surface.primary` 等颜色 token 没有完整列表
- RGBA 元组格式 `(r, g, b, a)` 的值域是 0-1 还是 0-255？

### 5. 3D 支持状态不明（P2）

LLM 经常生成 `Graph3D`、`Line3D` 等——因为数学/物理教学很自然需要 3D。Spec 没有明确说"不支持 3D"。

---

## 二、编译器 / CLI 问题

### 6. 容器内注释不支持（P0）

```amx
Graph {
    // 坐标轴        ← 编译失败：expected expression, '}'
    x_axis: Line, from: (-1,0), to: (5,0)
}
```

这是最自然的写法，LLM 和人类都会这样写。错误信息 `expected expression, '}', found '/'` 非常不直观。

**建议**：支持容器内 `//` 注释，或至少在错误信息中说明"块内不支持注释"。

### 7. 错误信息可读性差（P0）

当前错误格式：

```
2026-05-28T17:24:19.281989Z ERROR animatix: Error: Parse error: expected something else, '-', '=', '(', '.', '^', '*', '/', '%', '+', '>', '<', '!'
```

问题：

- **没有行号列号** — 不知道错误在哪一行
- **没有上下文** — 不知道是哪个元素/属性出错
- **ANSI 转义码混在输出里** — 脚本处理需要额外清理（`\x1b[31m` 等）
- **错误类型不区分** — "注释在容器内"和"未知 token"给的是同一类 parse error

建议格式：

```
error[E001]: 注释不允许出现在 {} 块内
  --> code.amx:15:5
   |
15 |     // 坐标轴
   |     ^^^^^^^^ 移除此注释，或将其移到块外
```

### 8. `animatix-cli check` 不支持 stdin（P2）

```bash
animatix-cli check /path/to/file.amx    # 需要文件路径
animatix-cli check < input.amx          # 不支持 stdin
```

对脚本不友好。建议支持 stdin 或 `-` 参数。

### 9. 没有 `--format json` 输出选项（P1）

编译结果只能从 stderr 的文本中用正则提取，很容易出错。建议：

```bash
animatix-cli check --format json file.amx
# {"passed": false, "errors": [{"line": 15, "col": 5, "message": "...", "code": "E001"}]}
```

### 10. 没有 `animatix-cli lint` 或 `animatix-cli format`（P2）

LLM 生成的代码风格不一致（缩进、空行、注释位置），没有工具可以自动格式化或检查最佳实践。

---

## 三、语言设计问题

### 11. Graph 内元素无法从外部动画（P1）

```amx
g: Graph, x_domain: (0, 5), y_domain: (0, 5) {
    vec: Line, from: (0,0), to: (3,4), color: accent.danger
}

# 外部无法动画 vec：
shift vec [by: (1, 0)]         # ← 编译失败
vec.to = (5, 2) [1s]           # ← 编译失败
```

这在数学动画中非常常见（先画坐标系，再动画里面的向量）。目前只能把需要动画的元素放在 Graph 外面，但这破坏了坐标系的语义绑定。

**建议**：支持 `g.vec.to = (5, 2)` 点路径语法，或支持 `shift g.vec [by: ...]`。

### 12. 缺少常见数学元素（P2）

LLM 在数学/物理场景中经常需要：

- **箭头（Arrow）** — 向量、力的方向，目前只能用 Line
- **角度标注** — 物理力学常用
- **坐标轴标签（axis ticks/labels）** — Graph 没有自动标签

### 13. Math 元素的 Typst 语法不直观（P1）

LLM 默认用 LaTeX 语法（`\frac{a}{b}`、`\lim_{x \to 1}`），但 AMX 用 Typst。没有 Typst 语法参考，LLM 100% 会写 LaTeX。

**建议**：在 spec 中加一个 Typst vs LaTeX 速查表，或者在错误信息中提示"检测到 LaTeX 语法，请改用 Typst 格式"。

### 14. `let` 变量不能被动画（P2）

```amx
let x = 0
#1s
x = 5 [1s]    # ← 不支持
```

这让很多"动态计算"的动画无法实现（比如动态改变参数看函数图像变化）。

---

## 四、代码库 / 架构问题

### 15. Spec 和 Examples 不一致（P1）

- Spec 里的 `points: [(0,0)]` vs Examples 里的 `points: {(0,0)}` — 语法不同
- Spec 里提到的 `Row`、`Col`、`Stack`、`Grid` 在 Examples 中很少出现，LLM 缺乏参考

### 16. Examples 覆盖不均（P1）

| 元素 | Examples 中出现次数 | LLM 参考 |
|---|---|---|
| Text | 60 | ✅ 充足 |
| Rect | 18 | ✅ 充足 |
| Ellipse | 16 | ✅ 充足 |
| Line | 6 | ⚠️ 偏少 |
| Graph | 4 | ⚠️ 偏少 |
| Polygon | 3 | ❌ 严重不足 |
| PlotCurve | ~5 | ⚠️ 偏少 |
| Math | 5 | ⚠️ 偏少 |
| Path | 1 | ❌ 严重不足 |
| VectorField | 1 | ❌ 严重不足 |
| Heatmap | 1 | ❌ 严重不足 |
| ContourSet | 0 | ❌ 完全缺失 |

Polygon、Path、VectorField、Heatmap 这些在数学/物理教学中很重要的元素，Examples 几乎没有，LLM 无法学习正确用法。

---

## 五、优先级建议

| 优先级 | 问题 | 影响 |
|---|---|---|
| **P0** | Polygon.points 语法文档修正 | 100% 写错 |
| **P0** | 错误信息加行号 + 去 ANSI | 修复轮次效率 |
| **P0** | 容器内注释支持 | 80%+ 写错 |
| **P1** | 元素属性完整列表 | 减少属性猜测 |
| **P1** | 不存在元素的明确提示 | 减少幻觉元素 |
| **P1** | `check --format json` | 脚本友好 |
| **P1** | Math/Typst 语法参考 | 数学场景 |
| **P1** | Graph 内元素外部动画 | 动画灵活性 |
| **P2** | 增加 Polygon/Path/PlotCurve Examples | LLM 学习 |
| **P2** | 颜色系统集中文档 | 减少颜色幻觉 |
| **P2** | stdin / lint / format 支持 | 工具链完善 |

修复 P0+P1 后，预计 AI 生成的单轮编译通过率可从当前的 **86.5%** 提升至 **95%+**。
