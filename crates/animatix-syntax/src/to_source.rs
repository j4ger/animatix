//! Serialization of AST nodes back to source text.
//!
//! This module provides [`ToSource`] — the inverse of the parser. When the GUI
//! inspector mutates the AST, the entire tree is re-serialized to produce the
//! new source text. Formatting (extra spaces, blank lines, inline comments) is
//! normalized; semantic content is preserved.

use crate::ast::*;

/// Indent a multi-line string by `depth` levels of 2 spaces.
fn indent(text: &str, depth: usize) -> String {
    let pad = "  ".repeat(depth);
    text.lines()
        .map(|line| format!("{}{}", pad, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Trait for serializing AST nodes back to `.amx` source text.
pub trait ToSource {
    /// Serialize to a source text string.
    fn to_source(&self) -> String;
}

impl ToSource for Expr {
    fn to_source(&self) -> String {
        match self {
            Expr::Num(n) => {
                if n.fract() == 0.0 {
                    format!("{}", *n as i64)
                } else {
                    format!("{}", n)
                }
            }
            Expr::Percent(n) => format!("{}%", n),
            Expr::Str(s) => {
                let escaped = s.replace('"', "\\\"");
                format!("\"{}\"", escaped)
            }
            Expr::Bool(true) => "true".into(),
            Expr::Bool(false) => "false".into(),
            Expr::Null => "null".into(),
            Expr::Ident(s) => s.clone(),
            Expr::Path(segs) => segs.join("."),
            Expr::Index(base, idx) => {
                format!("{}[{}]", base.to_source(), idx.to_source())
            }
            Expr::Tuple(items) => {
                if items.len() == 1 {
                    items[0].to_source()
                } else {
                    let inner = items
                        .iter()
                        .map(|i| i.to_source())
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({})", inner)
                }
            }
            Expr::Binary(lhs, op, rhs) => {
                let lhs_str = lhs.to_source();
                let rhs_str = rhs.to_source();
                // Parenthesize child binary expressions that have lower or equal
                // precedence to preserve meaning.
                let lhs_out = match lhs.as_ref() {
                    Expr::Binary(_, child_op, _) if op_precedence(child_op) <= op_precedence(op) => {
                        format!("({})", lhs_str)
                    }
                    _ => lhs_str,
                };
                let rhs_out = match rhs.as_ref() {
                    Expr::Binary(_, child_op, _) if op_precedence(child_op) <= op_precedence(op) => {
                        format!("({})", rhs_str)
                    }
                    _ => rhs_str,
                };
                format!("{} {} {}", lhs_out, op.as_str(), rhs_out)
            }
            Expr::Unary(op, expr) => {
                let inner = expr.to_source();
                // Parenthesize if the inner expr is a binary (to avoid ambiguity)
                let inner_out = match expr.as_ref() {
                    Expr::Binary(_, _, _) => format!("({})", inner),
                    _ => inner,
                };
                format!("{}{}", op.as_str(), inner_out)
            }
            Expr::Call(name, args) => {
                let inner = args
                    .iter()
                    .map(|a| a.to_source())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({})", name, inner)
            }
            Expr::Method(obj, name, args) => {
                let inner = args
                    .iter()
                    .map(|a| a.to_source())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}.{}({})", obj.to_source(), name, inner)
            }
            Expr::Closure(params, body) => {
                let params_str = params.join(", ");
                format!("({}) => {}", params_str, body.to_source())
            }
            Expr::Conditional(cond, then_expr, else_expr) => {
                format!(
                    "if {} {{ {} }} else {{ {} }}",
                    cond.to_source(),
                    then_expr.to_source(),
                    else_expr.to_source()
                )
            }
            Expr::Construct(name, props) => {
                let inner = props
                    .iter()
                    .map(|p| p.to_source())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{} {{ {} }}", name, inner)
            }
        }
    }
}

impl BinaryOp {
    fn as_str(&self) -> &'static str {
        match self {
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Mod => "%",
            BinaryOp::Pow => "^",
            BinaryOp::Eq => "==",
            BinaryOp::Neq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::Gt => ">",
            BinaryOp::Lte => "<=",
            BinaryOp::Gte => ">=",
            BinaryOp::And => "&&",
            BinaryOp::Or => "||",
        }
    }
}

impl UnaryOp {
    fn as_str(&self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
        }
    }
}

fn op_precedence(op: &BinaryOp) -> u8 {
    match op {
        BinaryOp::Pow => 5,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 4,
        BinaryOp::Add | BinaryOp::Sub => 3,
        BinaryOp::Eq | BinaryOp::Neq | BinaryOp::Lt | BinaryOp::Gt | BinaryOp::Lte
        | BinaryOp::Gte => 2,
        BinaryOp::And | BinaryOp::Or => 1,
    }
}

impl ToSource for Property {
    fn to_source(&self) -> String {
        let mut s = format!("{}: {}", self.name, self.value.to_source());
        if let Some(comment) = &self.trailing_comment {
            s.push_str(&format!("  //{}", comment));
        }
        s
    }
}

impl ToSource for Modifier {
    fn to_source(&self) -> String {
        match &self.name {
            Some(name) => format!("{}: {}", name, self.value.to_source()),
            None => self.value.to_source(),
        }
    }
}

impl ToSource for Action {
    fn to_source(&self) -> String {
        let targets = self.targets.join(" ");
        let mut parts = vec![self.verb.clone()];
        if !targets.is_empty() {
            parts.push(targets);
        }
        if !self.args.is_empty() {
            let args = self.args.iter().map(|a| a.to_source()).collect::<Vec<_>>().join(", ");
            parts.push(args);
        }
        if !self.modifiers.is_empty() {
            let mods = self
                .modifiers
                .iter()
                .map(|m| m.to_source())
                .collect::<Vec<_>>()
                .join(", ");
            parts.push(format!("[{}]", mods));
        }
        parts.join(" ")
    }
}

impl ToSource for Time {
    fn to_source(&self) -> String {
        match self {
            Time::Seconds(s) => {
                if s.fract() == 0.0 {
                    format!("{}s", *s as i64)
                } else {
                    format!("{}s", s)
                }
            }
            Time::Milliseconds(ms) => format!("{}ms", ms),
        }
    }
}

impl ToSource for ParamDef {
    fn to_source(&self) -> String {
        match (&self.param_type, &self.default) {
            (Some(ty), Some(expr)) => format!("{}: {} = {}", self.name, ty, expr.to_source()),
            (Some(ty), None) => format!("{}: {}", self.name, ty),
            (None, Some(expr)) => format!("{}: {}", self.name, expr.to_source()),
            (None, None) => self.name.clone(),
        }
    }
}

impl ToSource for ComponentDef {
    fn to_source(&self) -> String {
        let pub_kw = if self.is_pub { "pub " } else { "" };
        let params = self
            .params
            .iter()
            .map(|p| p.to_source())
            .collect::<Vec<_>>()
            .join(", ");
        let body = self
            .body
            .iter()
            .map(|s| s.to_source())
            .collect::<Vec<_>>()
            .join("\n");
        let body_indented = indent(&body, 1);
        format!(
            "{}component {}({}) {{\n{}\n}}",
            pub_kw, self.name, params, body_indented
        )
    }
}

impl ToSource for Transition {
    fn to_source(&self) -> String {
        let mut parts = vec![self.id.clone()];
        if self.duration_ms > 0 {
            parts.push(format!("{}ms", self.duration_ms));
        }
        parts.join(", ")
    }
}

impl ToSource for InlineItem {
    fn to_source(&self) -> String {
        match self {
            InlineItem::Anonymous { ty, props, modifiers, children } => {
                let mut parts = vec![ty.clone()];
                if !props.is_empty() {
                    let props_str = props.iter().map(|p| p.to_source()).collect::<Vec<_>>().join(", ");
                    parts.push(format!(", {}", props_str));
                }
                if !modifiers.is_empty() {
                    let mods = modifiers.iter().map(|m| m.to_source()).collect::<Vec<_>>().join(", ");
                    parts.push(format!(" [{}]", mods));
                }
                if !children.is_empty() {
                    let children_str = children
                        .iter()
                        .map(|c| c.to_source())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let indented = indent(&children_str, 1);
                    parts.push(format!(" {{\n{}\n}}", indented));
                }
                parts.join("")
            }
            InlineItem::Labeled { label, ty, props, modifiers, children } => {
                let mut parts = vec![format!("{}: {}", label, ty)];
                if !props.is_empty() {
                    let props_str = props.iter().map(|p| p.to_source()).collect::<Vec<_>>().join(", ");
                    parts.push(format!(", {}", props_str));
                }
                if !modifiers.is_empty() {
                    let mods = modifiers.iter().map(|m| m.to_source()).collect::<Vec<_>>().join(", ");
                    parts.push(format!(" [{}]", mods));
                }
                if !children.is_empty() {
                    let children_str = children
                        .iter()
                        .map(|c| c.to_source())
                        .collect::<Vec<_>>()
                        .join("\n");
                    let indented = indent(&children_str, 1);
                    parts.push(format!(" {{\n{}\n}}", indented));
                }
                parts.join("")
            }
            InlineItem::SlotMarker => "@slot".into(),
            InlineItem::SlotFill { slot, items } => {
                let items_str = items
                    .iter()
                    .map(|i| i.to_source())
                    .collect::<Vec<_>>()
                    .join("\n");
                let indented = indent(&items_str, 1);
                format!("@{} {{\n{}\n}}", slot, indented)
            }
        }
    }
}

impl ToSource for Stmt {
    fn to_source(&self) -> String {
        match self {
            Stmt::Action(a, ..) => a.to_source(),
            Stmt::LetDecl { is_pub, name, value, .. } => {
                let pub_kw = if *is_pub { "pub " } else { "" };
                format!("{}let {} = {}", pub_kw, name, value.to_source())
            }
            Stmt::ActorDecl { is_pub, is_anonymous, label, ty, props, modifiers, children, .. } => {
                let s = serialize_actor_like_stmt(
                    Some(label),
                    *is_anonymous,
                    ty,
                    props,
                    modifiers,
                    children,
                );
                if *is_pub { format!("pub {}", s) } else { s }
            }
            Stmt::Import { path, alias, .. } => match alias {
                Some(a) => format!(r#"import "{}" as {}"#, path, a),
                None => format!(r#"import "{}""#, path),
            },
            Stmt::Use { path, items, .. } => {
                let items_str = items.join(", ");
                format!("use {}.{{{}}}", path, items_str)
            }
            Stmt::Keyframe { time, body, .. } => {
                let body_str = body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                format!("#{}\n{}", time.to_source(), body_str)
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                let body_str = body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                format!("#+{}\n{}", offset.to_source(), body_str)
            }
            Stmt::Assignment { target, property, value, modifiers, .. } => {
                let assignment_str = if target.is_empty() {
                    format!("{} = {}", property, value.to_source())
                } else {
                    format!("{}.{} = {}", target.join("."), property, value.to_source())
                };
                let mut parts = vec![assignment_str];
                if !modifiers.is_empty() {
                    let mods = modifiers.iter().map(|m| m.to_source()).collect::<Vec<_>>().join(", ");
                    parts.push(format!(" [{}]", mods));
                }
                parts.join("")
            }
            Stmt::Sequence { body, .. } => {
                let body_str = body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                let indented = indent(&body_str, 1);
                format!("sequence {{\n{}\n}}", indented)
            }
            Stmt::Stagger { modifiers, body, .. } => {
                let mut header = "stagger".to_string();
                if !modifiers.is_empty() {
                    let mods = modifiers.iter().map(|m| m.to_source()).collect::<Vec<_>>().join(", ");
                    header.push_str(&format!(" [{}]", mods));
                }
                let body_str = body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                let indented = indent(&body_str, 1);
                format!("{} {{\n{}\n}}", header, indented)
            }
            Stmt::Always { body, .. } => {
                let body_str = body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                let indented = indent(&body_str, 1);
                format!("always {{\n{}\n}}", indented)
            }
            Stmt::Drive { label, body, .. } => {
                let body_str = body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                let indented = indent(&body_str, 1);
                format!("drive {} {{\n{}\n}}", label, indented)
            }
            Stmt::ReactiveBinding { target, property, value, .. } => {
                format!("{}.{} := {}", target.join("."), property, value.to_source())
            }
            Stmt::Conditional { condition, then_branch, else_branch, .. } => {
                let then_str = then_branch.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                let then_indented = indent(&then_str, 1);
                let mut result = format!("if {} {{\n{}\n}}", condition.to_source(), then_indented);
                if let Some(else_body) = else_branch {
                    let else_str = else_body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                    let else_indented = indent(&else_str, 1);
                    result.push_str(&format!(" else {{\n{}\n}}", else_indented));
                }
                result
            }
            Stmt::ForLoop { var, iterable, body, .. } => {
                let body_str = body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                let indented = indent(&body_str, 1);
                format!("for {} in {} {{\n{}\n}}", var, iterable.to_source(), indented)
            }
            Stmt::ComponentDef(def, ..) => def.to_source(),
            Stmt::ComponentAction { name, params, body, .. } => {
                let params_str = params
                    .iter()
                    .map(|p| p.to_source())
                    .collect::<Vec<_>>()
                    .join(", ");
                let body_str = body.iter().map(|s| s.to_source()).collect::<Vec<_>>().join("\n");
                let indented = indent(&body_str, 1);
                format!("action {}({}) {{\n{}\n}}", name, params_str, indented)
            }
            Stmt::Config { settings, .. } => {
                let inner = settings
                    .iter()
                    .map(|s| s.to_source())
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("config {{ {} }}", inner)
            }
            Stmt::Scene { name, config, body, .. } => {
                let mut parts = vec![format!("# {}", name)];
                if !config.is_empty() {
                    let inner = config
                        .iter()
                        .map(|s| s.to_source())
                        .collect::<Vec<_>>()
                        .join(", ");
                    parts.push(format!("config {{ {} }}", inner));
                }
                if !body.is_empty() {
                    let body_str = body
                        .iter()
                        .map(|s| s.to_source())
                        .collect::<Vec<_>>()
                        .join("\n");
                    parts.push(body_str);
                }
                parts.join("\n")
            }
            Stmt::Play { scene_name, transition, .. } => {
                let mut s = format!("play {}", scene_name);
                if let Some(t) = transition {
                    s.push_str(&format!(" [{}]", t.to_source()));
                }
                s
            }
            Stmt::ViewportDecl { label, position, size, opacity, border, border_color, scene, mask, .. } => {
                let mut parts = vec![
                    format!("viewport {}", label),
                    format!("at {}", position.to_source()),
                    format!("size {}", size.to_source()),
                    format!("scene \"{}\"", scene),
                ];
                if let Some(o) = opacity {
                    parts.push(format!("opacity {}", o.to_source()));
                }
                if let Some(b) = border {
                    parts.push(format!("border {}", b.to_source()));
                }
                if let Some(c) = border_color {
                    parts.push(format!("border_color {}", c.to_source()));
                }
                if let Some(m) = mask {
                    parts.push(format!("mask \"{}\"", m));
                }
                parts.join(" ")
            }
            Stmt::Comment(text, ..) => format!("//{}", text),
        }
    }
}

/// Serialize actor-like statements (Text, Math, Code, Svg, Image, ActorDecl).
fn serialize_actor_like_stmt(
    label: Option<&str>,
    is_anonymous: bool,
    ty: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
) -> String {
    let mut parts = Vec::new();
    // Anonymous items have synthetic labels — emit without label.
    if let Some(lbl) = label.filter(|_| !is_anonymous) {
        parts.push(format!("{}: {}", lbl, ty));
    } else {
        parts.push(ty.to_string());
    }
    if !props.is_empty() {
        let props_str = props.iter().map(|p| p.to_source()).collect::<Vec<_>>().join(", ");
        parts.push(format!(", {}", props_str));
    }
    if !modifiers.is_empty() {
        let mods = modifiers.iter().map(|m| m.to_source()).collect::<Vec<_>>().join(", ");
        parts.push(format!(" [{}]", mods));
    }
    if !children.is_empty() {
        let children_str = children
            .iter()
            .map(|c| c.to_source())
            .collect::<Vec<_>>()
            .join("\n");
        let indented = indent(&children_str, 1);
        parts.push(format!(" {{\n{}\n}}", indented));
    }
    parts.join("")
}

/// Serialize a top-level statement list (the contents of an `.amx` file).
///
/// Keyframe blocks are separated by a single blank line for readability.
///
/// This is the main entry-point for GUI write-back: after the inspector mutates
/// the AST, the entire tree is re-serialized via this function to produce the
/// updated source text.
pub fn stmts_to_source(stmts: &[Stmt]) -> String {
    stmts
        .iter()
        .map(|s| s.to_source())
        .collect::<Vec<_>>()
        .join("\n\n")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{Expr, Stmt, Time};
    use chumsky::Parser;

    const SHOWCASE_FIXTURE: &str = r#"// Animatix Showcase
// A 5-second demonstration of layout, animation, primitives, and composition.

config { colorscheme: "editorial-dark", resolution: (1280, 720), dynamic_layout: true }

#0s
backdrop: Rect, size: (1280, 720), color: scene.background, anchor: scene.center
title: Text, text: "Animatix", font_size: 96, color: text.primary, anchor: scene.top, offset: (0, 120)
subtitle: Text, text: "Layout-first animation", font_size: 24, color: text.secondary, anchor: scene.top, offset: (0, 200)

#0.2s
fade-in backdrop [800ms, ease: ease-out]

#0.4s
fade-in title [600ms, ease: ease-out]

#0.8s
fade-in subtitle [600ms, ease: ease-in-out]

#1.0s
features: Col, anchor: scene.left, offset: (180, 0), align: "center", gap: 16 {
  label: Text, text: "Features", font_size: 28, color: accent.warning
  a: Text, text: "Rich typography", font_size: 18, color: text.secondary
  b: Text, text: "Math expressions", font_size: 18, color: text.secondary
  c: Text, text: "Code highlighting", font_size: 18, color: text.secondary
  d: Text, text: "Vector graphics", font_size: 18, color: text.secondary
}

#1.2s
shift features [by: (60, 0), 800ms, ease: ease-out]

#1.4s
stagger [100ms] {
  fade-in label [400ms]
  fade-in a [400ms]
  fade-in b [400ms]
  fade-in c [400ms]
  fade-in d [400ms]
}

#2.6s
primitives: Row, anchor: scene.bottom, offset: (0, -140), gap: 80, align: "center" {
  orb: Ellipse, size: (0, 0), color: accent.danger
  connector: Line, from: (0, 0), to: (0, 0), stroke: accent.primary, stroke_width: 4
  box: Rect, size: (0, 0), color: accent.success
  equation: Math, math: "x", font_size: 48, color: accent.primary
}

#2.6s
fade-in primitives [400ms]

#2.8s
orb.size = (120, 120) [600ms, ease: bounce]

#2.9s
connector.from = (-50, 0) [400ms, ease: ease-out]
connector.to = (50, 0) [400ms, ease: ease-out]

#3.0s
box.size = (120, 80) [500ms, ease: ease-out]

#3.2s
equation: Math, math: "E = mc^2", font_size: 48, color: accent.primary [800ms, ease: ease-in-out]

#3.4s
orb.color = accent.success [600ms, ease: ease-in-out]

#3.6s
box.color = accent.warning [600ms, ease: ease-in-out]

#4.2s
panel: Rect, size: (400, 180), color: (0.15, 0.17, 0.22, 1.0), anchor: scene.right, offset: (-220, -40)

#4.2s
shift panel [by: (-40, 0), 600ms, ease: ease-out]

#4.2s
fade-in panel [400ms]

#4.3s
code: Code, code: "let velocity = x + 1", font_size: 20, color: text.primary, anchor: scene.right, offset: (-220, -40)
shift code [by: (-40, 0), 600ms, ease: ease-out]
fade-in code [800ms, ease: ease-in-out]

#4.6s
ending_halo: Ellipse, size: (0, 0), color: accent.primary, at: (1040, 540)
ending_logo: Svg, url: "logo.svg", at: (1040, 540), scale: 0.0
ending_beam_a: Line, from: (1040, 540), to: (1040, 540), stroke: accent.success, stroke_width: 4
ending_beam_b: Line, from: (1040, 540), to: (1040, 540), stroke: accent.danger, stroke_width: 4
ending_kicker: Text, text: "Scene → Story", font_size: 20, color: accent.warning, anchor: scene.right, offset: (-180, 84)
ending_caption: Text, text: "Primitives composed in time", font_size: 16, color: text.secondary, anchor: scene.right, offset: (-180, 116)

#4.6s
stagger [80ms] {
  fade-in ending_halo [260ms]
  fade-in ending_logo [260ms]
  fade-in ending_kicker [320ms]
  fade-in ending_caption [320ms]
}

#4.68s
ending_halo.size = (108, 108) [520ms, ease: ease-out]
scale ending_logo [by: 1.0, 480ms, ease: bounce]

#4.78s
ending_beam_a.from = (990, 500) [420ms, ease: ease-out]
ending_beam_a.to = (1090, 580) [420ms, ease: ease-out]
ending_beam_b.from = (990, 580) [420ms, ease: ease-out]
ending_beam_b.to = (1090, 500) [420ms, ease: ease-out]

#4.92s
rotate ending_logo [by: 0.12, 700ms, ease: ease-in-out]

#5.0s
sequence {
  scale title [by: 1.02, 400ms, ease: ease-in-out]
  scale title [by: 0.98, 400ms, ease: ease-in-out]
}

#5.1s
pulse ending_halo [500ms, intensity: 0.18]

#5.2s
shift primitives [by: (0, -10), 2s, ease: ease-in-out]

#5.2s
shift ending_halo [by: (0, -8), 2s, ease: ease-in-out]
shift ending_logo [by: (0, -8), 2s, ease: ease-in-out]
shift ending_beam_a [by: (0, -8), 2s, ease: ease-in-out]
shift ending_beam_b [by: (0, -8), 2s, ease: ease-in-out]

#5.5s
equation: Math, math: "E = mc^2", font_size: 48, color: accent.danger [1s, ease: ease-in-out]
"#;

    const REORDER_FIXTURE: &str = r#"// Reorder Demo
// Demonstrates the reorder action for explicit full-order animation.

config { colorscheme: "editorial-dark", resolution: (1280, 720), dynamic_layout: true }

#0s
title: Text, text: "Reorder Action", font_size: 48, color: text.primary, anchor: scene.top, offset: (0, 60)
subtitle: Text, text: "Explicit full-order container animation", font_size: 20, color: text.secondary, anchor: scene.top, offset: (0, 120)

#0s
blocks: Row, anchor: scene.center, gap: 16, align: "center" {
  red: Rect, size: (80, 80), color: accent.danger
  yellow: Rect, size: (80, 80), color: accent.warning
  green: Rect, size: (80, 80), color: accent.success
  blue: Rect, size: (80, 80), color: accent.primary
}

#0.5s
fade-in title [800ms, ease: ease-out]
fade-in subtitle [600ms, delay: 200ms, ease: ease-out]

#1.0s
fade-in blocks [600ms, ease: ease-out]

// Reverse the row
#2.0s
reorder blocks [order: (blue, green, yellow, red), 800ms, ease: ease-in-out]

// Shuffle: every other
#3.5s
reorder blocks [order: (yellow, blue, red, green), 800ms, ease: ease-in-out]

// Back to original
#5.0s
reorder blocks [order: (red, yellow, green, blue), 800ms, ease: ease-in-out]

// Highlight completion
#6.5s
red.color = accent.success [300ms]
yellow.color = accent.success [300ms]
green.color = accent.success [300ms]
blue.color = accent.success [300ms]

#7.0s
done: Text, text: "Done!", font_size: 32, color: accent.success, anchor: scene.bottom, offset: (0, -80)

#7.0s
fade-in done [400ms, ease: ease-out]
"#;

    #[test]
    fn roundtrip_simple_actor_decl() {
        let source = r#"backdrop: Rect, size: (1280, 720), color: scene.background"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        // Re-parse the serialized output
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_keyframe_block() {
        let source = r#"#2s
fade-in title [600ms, ease: ease-out]"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
        // Verify the keyframe time is preserved
        if let Stmt::Keyframe { time, .. } = &reparsed[0] {
            assert_eq!(*time, Time::Seconds(2.0));
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn roundtrip_actor_with_children() {
        let source = r#"#0s
features: Col, anchor: scene.left {
    label: Text, text: "Features", font_size: 28
}"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_assignment() {
        let source = r#"orb.color = accent.success [600ms, ease: ease-in-out]"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_sequence() {
        let source = r#"sequence {
    fade-in a [400ms]
    fade-in b [400ms]
}"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_config() {
        let source = r#"config { resolution: (1280, 720), dynamic_layout: true }"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_complex_expressions() {
        let source = r#"let x = (a + b) * c"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn serialize_vec2() {
        let expr = Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]);
        assert_eq!(expr.to_source(), "(100, 200)");
    }

    #[test]
    fn serialize_color_rgb() {
        let expr = Expr::Call(
            "rgb".into(),
            vec![Expr::Num(255.0), Expr::Num(128.0), Expr::Num(0.0)],
        );
        assert_eq!(expr.to_source(), "rgb(255, 128, 0)");
    }

    #[test]
    fn serialize_string_with_quotes() {
        let expr = Expr::Str(r#"Say "hello""#.into());
        assert_eq!(expr.to_source(), r#""Say \"hello\"""#);
    }

    #[test]
    fn roundtrip_comment() {
        let source = r#"// This is a comment"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn serialize_property() {
        let prop = Property {
            name: "size".into(),
            value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
            value_span: None,
            trailing_comment: None,
        };
        assert_eq!(prop.to_source(), "size: (100, 200)");
    }

    #[test]
    fn roundtrip_full_showcase_file() {
        let source = SHOWCASE_FIXTURE;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        // Re-parsing should succeed
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        // Same number of top-level statements
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn trailing_comment_preserved_on_property() {
        let source = r#"#0s
btn: Rect, size: (100, 200) // half-extents, in scene space"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        // The trailing comment should be preserved in the serialized output
        assert!(
            serialized.contains("// half-extents, in scene space"),
            "trailing comment lost in serialization: {}",
            serialized
        );
    }

    #[test]
    fn trailing_comment_roundtrips() {
        let source = r#"#0s
btn: Rect, size: (100, 200) // half-extents"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();

        // Extract the property and verify the trailing comment survived
        if let Stmt::Keyframe { body, .. } = &reparsed[0] {
            if let Stmt::ActorDecl { props, .. } = &body[0] {
                let size_prop = props.iter().find(|p| p.name == "size").unwrap();
                assert_eq!(
                    size_prop.trailing_comment.as_deref(),
                    Some(" half-extents")
                );
            } else {
                panic!("Expected ActorDecl");
            }
        } else {
            panic!("Expected Keyframe");
        }
    }

    #[test]
    fn block_comment_rejected() {
        let source = r#"/* block comment */ btn: Rect"#;
        let result = crate::parser::parser().parse(source);
        let (_, errors) = result.into_output_errors();
        // Parsing must fail — block comments are not supported.
        assert!(!errors.is_empty(), "block comment should be rejected");
    }

    #[test]
    fn serialize_property_with_trailing_comment() {
        let prop = Property {
            name: "size".into(),
            value: Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(200.0)]),
            value_span: None,
            trailing_comment: Some(" half-extents".into()),
        };
        assert_eq!(prop.to_source(), "size: (100, 200)  // half-extents");
    }

    #[test]
    fn parse_reorder_demo() {
        let source = REORDER_FIXTURE;
        let parsed = crate::parser::parser().parse(source).unwrap();
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_drive_block() {
        let source = r#"drive tracker {
    at = (640 + 100 * cos(t), 360 + 100 * sin(t))
}"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        assert_eq!(parsed.len(), 1);
        // Top-level statements are wrapped in a default keyframe
        if let Stmt::Keyframe { body, .. } = &parsed[0] {
            if let Stmt::Drive { label, body: drive_body, .. } = &body[0] {
                assert_eq!(label, "tracker");
                assert_eq!(drive_body.len(), 1);
            } else {
                panic!("Expected Drive statement");
            }
        } else {
            panic!("Expected Keyframe wrapper");
        }
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

    #[test]
    fn roundtrip_reactive_binding() {
        let source = r#"orbiter.at := tracker.at + (200 * cos(3 * t), 200 * sin(3 * t))"#;
        let parsed = crate::parser::parser().parse(source).unwrap();
        assert_eq!(parsed.len(), 1);
        if let Stmt::Keyframe { body, .. } = &parsed[0] {
            if let Stmt::ReactiveBinding { target, property, .. } = &body[0] {
                assert_eq!(target, &["orbiter"]);
                assert_eq!(property, "at");
            } else {
                panic!("Expected ReactiveBinding");
            }
        } else {
            panic!("Expected Keyframe wrapper");
        }
        let serialized = stmts_to_source(&parsed);
        let reparsed = crate::parser::parser().parse(&serialized).unwrap();
        assert_eq!(parsed.len(), reparsed.len());
    }

}
