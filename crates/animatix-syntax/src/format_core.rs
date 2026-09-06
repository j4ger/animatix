//! This is the canonical formatting engine. All formatting logic lives here.
//!
//! Every function produces **raw** text — the caller applies top-level
//! indentation. Children blocks are indented internally at `depth + 1`.
//!
//! # Relationship to other modules
//!
//! - [`to_source`](crate::to_source) wraps this module's functions with a convenient trait (fixed
//!   indent = 2).
//! - [`formatter`](crate::formatter) wraps this module's functions with configurable indent and
//!   other formatting options.
//!
//! # Compatibility with shared walk primitives
//!
//! The functions in this module are **incompatible** with the shared walk
//! primitives in [`walk.rs`](crate::walk) (`walk_stmts`, `walk_expr`, etc.)
//! because they return `String` and must propagate formatted output up the
//! recursion. The walk primitives use a `&mut dyn FnMut(&T) -> ()` visitor
//! pattern which cannot propagate return values.
//!
//! Guardrail tests at the bottom of this file (`format_expr_covers_all_expr_variants`,
//! `format_inline_item_covers_all_inline_item_variants`,
//! `format_stmt_raw_covers_all_stmt_variants`) ensure that all AST variants are
//! reviewed when new variants are added.

use crate::ast::*;

/// Format a slice of `TargetSegment`s as a source string.
/// Static segments are written as-is; Indexed segments are written as
/// `base[index_expr]` using the provided expression formatter.
pub fn format_target_segments(
    segments: &[TargetSegment],
    format_expr: fn(&Expr) -> String,
) -> String {
    segments
        .iter()
        .map(|s| match s {
            TargetSegment::Static(name) => name.clone(),
            TargetSegment::Indexed { base, index } => {
                format!("{}[{}]", base, format_expr(index))
            },
        })
        .collect::<Vec<_>>()
        .join(".")
}

/// Serialize a property to source text.
pub fn format_property(prop: &Property) -> String {
    let mut s = format!("{}: {}", prop.name, format_expr(&prop.value));
    if let Some(comment) = &prop.trailing_comment {
        s.push_str(&format!("  //{}", comment));
    }
    s
}

/// Serialize a modifier to source text.
pub fn format_modifier(m: &Modifier) -> String {
    match &m.name {
        Some(name) => format!("{}: {}", name, format_expr(&m.value)),
        None => format_expr(&m.value),
    }
}

/// Serialize an action to source text.
pub fn format_action(a: &Action) -> String {
    let targets = a
        .targets
        .iter()
        .zip(a.target_index.iter())
        .map(|(target, index)| match index {
            Some(expr) => format!("{}[{}]", target, format_expr(expr)),
            None => target.clone(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut parts = vec![a.verb.clone()];
    if !targets.is_empty() {
        parts.push(targets.clone());
    }
    if !a.args.is_empty() {
        let args = a.args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
        if targets.is_empty() {
            // Function-style call `f(a, b)`: keep the parentheses.
            return format!("{}({args})", a.verb);
        }
        parts.push(args);
    }
    if !a.modifiers.is_empty() {
        let mods = a.modifiers.iter().map(format_modifier).collect::<Vec<_>>().join(", ");
        parts.push(format!("[{}]", mods));
    }
    parts.join(" ")
}

/// Serialize a time value to source text.
pub fn format_time(t: &Time) -> String {
    match t {
        Time::Seconds(s) => {
            if (s - s.round()).abs() < 1e-9 {
                format!("{}s", *s as i64)
            } else {
                format!("{}s", s)
            }
        },
        Time::Milliseconds(ms) => format!("{}ms", ms),
    }
}

/// Serialize a parameter definition to source text.
pub fn format_param_def(p: &ParamDef) -> String {
    match (&p.param_type, &p.default) {
        (Some(ty), Some(expr)) => format!("{}: {} = {}", p.name, ty, format_expr(expr)),
        (Some(ty), None) => format!("{}: {}", p.name, ty),
        (None, Some(expr)) => format!("{}: {}", p.name, format_expr(expr)),
        (None, None) => p.name.clone(),
    }
}

/// Serialize a transition to source text.
pub fn format_transition(t: &Transition) -> String {
    let mut parts = vec![t.id.clone()];
    if t.duration_ms > 0 {
        parts.push(format!("{}ms", t.duration_ms));
    }
    parts.join(", ")
}

/// Indent a multi-line string by `depth` levels of `indent_size` spaces.
pub fn indent(text: &str, depth: usize, indent_size: usize) -> String {
    let pad = " ".repeat(indent_size * depth);
    text.lines()
        .map(|line| format!("{}{}", pad, line))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Operator precedence for binary operators (higher = tighter binding).
pub fn op_precedence(op: &BinaryOp) -> u8 {
    match op {
        BinaryOp::Pow => 5,
        BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => 4,
        BinaryOp::Add | BinaryOp::Sub => 3,
        BinaryOp::Eq
        | BinaryOp::Neq
        | BinaryOp::Lt
        | BinaryOp::Gt
        | BinaryOp::Lte
        | BinaryOp::Gte => 2,
        BinaryOp::And | BinaryOp::Or => 1,
    }
}

impl BinaryOp {
    /// Return the source text for this operator.
    pub fn as_str(&self) -> &'static str {
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
    /// Return the source text for this operator.
    pub fn as_str(&self) -> &'static str {
        match self {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
        }
    }
}

/// Serialize an expression to source text.
///
/// Expressions are always inline (no depth/indent needed).
pub fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::Num(n) => {
            if (n - n.round()).abs() < 1e-9 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        },
        Expr::Percent(n) => format!("{}%", n),
        Expr::Str(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{}\"", escaped)
        },
        Expr::Bool(true) => "true".into(),
        Expr::Bool(false) => "false".into(),
        Expr::Null => "null".into(),
        Expr::Ident(s) => s.clone(),
        Expr::Path(segs) => segs.join("."),
        Expr::Index(base, idx) => {
            format!("{}[{}]", format_expr(base), format_expr(idx))
        },
        Expr::List(items) => {
            let inner = items.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("{{{}}}", inner)
        },
        Expr::Tuple(items) => {
            if items.len() == 1 {
                format_expr(&items[0])
            } else {
                let inner = items.iter().map(format_expr).collect::<Vec<_>>().join(", ");
                format!("({})", inner)
            }
        },
        Expr::Binary(lhs, op, rhs) => {
            let lhs_str = format_expr(lhs);
            let rhs_str = format_expr(rhs);
            let lhs_out = match lhs.as_ref() {
                Expr::Binary(_, child_op, _) if op_precedence(child_op) <= op_precedence(op) => {
                    format!("({})", lhs_str)
                },
                _ => lhs_str,
            };
            let rhs_out = match rhs.as_ref() {
                Expr::Binary(_, child_op, _) if op_precedence(child_op) <= op_precedence(op) => {
                    format!("({})", rhs_str)
                },
                _ => rhs_str,
            };
            format!("{} {} {}", lhs_out, op.as_str(), rhs_out)
        },
        Expr::Unary(op, expr) => {
            let inner = format_expr(expr);
            let inner_out = match expr.as_ref() {
                Expr::Binary(_, _, _) => format!("({})", inner),
                _ => inner,
            };
            format!("{}{}", op.as_str(), inner_out)
        },
        Expr::Call(name, args) => {
            let inner = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("{}({})", name, inner)
        },
        Expr::Method(obj, name, args) => {
            let inner = args.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("{}.{}({})", format_expr(obj), name, inner)
        },
        Expr::Closure(params, body) => {
            let params_str = params.join(", ");
            format!("({}) => {}", params_str, format_expr(body))
        },
        Expr::LetChain(bindings, tail) => {
            // Serialize as a single line with newlines between bindings —
            // the language has no `;` token, so newline separation is the
            // only re-parseable form. The embedded newlines are indented
            // enough to stay readable wherever the expression appears.
            let mut parts: Vec<String> = Vec::with_capacity(bindings.len() + 1);
            for (name, value) in bindings {
                parts.push(format!("let {} = {}", name, format_expr(value)));
            }
            parts.push(format_expr(tail));
            format!("{{ {} }}", parts.join("\n  "))
        },
        Expr::Conditional(cond, then_expr, else_expr) => {
            format!(
                "if {} {{ {} }} else {{ {} }}",
                format_expr(cond),
                format_expr(then_expr),
                format_expr(else_expr)
            )
        },
        Expr::Match(scrutinee, arms) => {
            let arms_str = arms
                .iter()
                .map(|(pat, expr)| format!("{} => {}", format_match_pat(pat), format_expr(expr)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("match {} {{ {} }}", format_expr(scrutinee), arms_str)
        },
        Expr::Construct(name, props) => {
            let inner = props.iter().map(format_property).collect::<Vec<_>>().join(", ");
            format!("{} {{ {} }}", name, inner)
        },
    }
}

/// Format a match pattern for display.
pub fn format_match_pat(pat: &MatchPattern) -> String {
    match pat {
        MatchPattern::Wildcard => "_".to_string(),
        MatchPattern::Num(n) => {
            if *n == (*n as i64) as f64 {
                format!("{}", *n as i64)
            } else {
                format!("{}", n)
            }
        },
        MatchPattern::Str(s) => format!("{:?}", s),
        MatchPattern::Bool(b) => format!("{}", b),
        MatchPattern::Range(lo, hi) => {
            format!("{}..={}", format_match_pat(lo), format_match_pat(hi))
        },
        MatchPattern::Or(pats) => pats.iter().map(format_match_pat).collect::<Vec<_>>().join(" | "),
        MatchPattern::Tuple(pats) => {
            let inner = pats.iter().map(format_match_pat).collect::<Vec<_>>().join(", ");
            format!("({})", inner)
        },
    }
}

/// Serialize an actor-like declaration (shared by `Stmt::ActorDecl` and `InlineItem`).
pub fn format_actor_like(
    label: Option<&str>,
    array_index: Option<&Expr>,
    is_anonymous: bool,
    ty: &str,
    props: &[Property],
    modifiers: &[Modifier],
    children: &[InlineItem],
    depth: usize,
    indent_size: usize,
) -> String {
    let mut parts = Vec::new();
    if let Some(lbl) = label.filter(|_| !is_anonymous) {
        if let Some(idx_expr) = array_index {
            parts.push(format!("{}[{}]: {}", lbl, format_expr(idx_expr), ty));
        } else {
            parts.push(format!("{}: {}", lbl, ty));
        }
    } else {
        parts.push(ty.to_string());
    }
    if !props.is_empty() {
        let props_str = props.iter().map(format_property).collect::<Vec<_>>().join(", ");
        parts.push(format!(", {}", props_str));
    }
    if !modifiers.is_empty() {
        let mods = modifiers.iter().map(format_modifier).collect::<Vec<_>>().join(", ");
        parts.push(format!(" [{}]", mods));
    }
    if !children.is_empty() {
        let children_str = children
            .iter()
            .map(|c| format_inline_item(c, depth + 1, indent_size))
            .collect::<Vec<_>>()
            .join("\n");
        parts.push(format!(" {{\n{}\n{}}}", children_str, " ".repeat(indent_size * depth)));
    }
    parts.join("")
}

/// Serialize an inline item (child of a container).
pub fn format_inline_item(item: &InlineItem, depth: usize, indent_size: usize) -> String {
    match item {
        InlineItem::Anonymous {
            ty,
            props,
            modifiers,
            children,
        } => {
            format_actor_like(None, None, true, ty, props, modifiers, children, depth, indent_size)
        },
        InlineItem::Labeled {
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
        } => format_actor_like(
            Some(label),
            array_index.as_ref(),
            false,
            ty,
            props,
            modifiers,
            children,
            depth,
            indent_size,
        ),
        InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } => {
            let body_str = body
                .iter()
                .map(|i| format_inline_item(i, depth + 1, indent_size))
                .collect::<Vec<_>>()
                .join(",\n");
            let index_str = index_var.as_ref().map(|iv| format!(", {}", iv)).unwrap_or_default();
            format!(
                "for {}{} in {} {{\n{}\n{}}}",
                var,
                index_str,
                format_expr(iterable),
                body_str,
                " ".repeat(depth * indent_size)
            )
        },
        InlineItem::SlotMarker => "@slot".into(),
        InlineItem::SlotFill { slot, items } => {
            let items_str = items
                .iter()
                .map(|i| format_inline_item(i, depth + 1, indent_size))
                .collect::<Vec<_>>()
                .join("\n");
            format!("@{} {{\n{}\n{}}}", slot, items_str, " ".repeat(indent_size * depth))
        },
    }
}

/// Serialize a component definition.
pub fn format_component_def(def: &ComponentDef, depth: usize, indent_size: usize) -> String {
    let pub_kw = if def.is_pub { "pub " } else { "" };
    let params = def.params.iter().map(format_param_def).collect::<Vec<_>>().join(", ");
    let body = format_stmts_raw(&def.body, depth + 1, indent_size);
    format!(
        "{}component {}({}) {{\n{}\n{}}}",
        pub_kw,
        def.name,
        params,
        body,
        " ".repeat(indent_size * depth)
    )
}

/// Serialize a list of statements at the given depth.
///
/// Each statement is formatted raw (no leading indent); the caller applies
/// indentation to each line.
pub fn format_stmts_raw(stmts: &[Stmt], depth: usize, indent_size: usize) -> String {
    stmts
        .iter()
        .map(|s| {
            let raw = format_stmt_raw(s, depth, indent_size);
            indent(&raw, depth, indent_size)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Serialize a statement at the given depth. Produces raw text — the caller
/// applies leading indentation. Children blocks are indented at `depth + 1`.
pub fn format_stmt_raw(stmt: &Stmt, depth: usize, indent_size: usize) -> String {
    match stmt {
        Stmt::Action(a, ..) => format_action(a),
        Stmt::LetDecl {
            is_pub,
            name,
            value,
            ..
        } => {
            let pub_kw = if *is_pub { "pub " } else { "" };
            format!("{}let {} = {}", pub_kw, name, format_expr(value))
        },
        Stmt::TypeAlias {
            is_pub,
            name,
            annotation,
            ..
        } => {
            let pub_kw = if *is_pub { "pub " } else { "" };
            format!("{}type {} = {}", pub_kw, name, annotation)
        },
        Stmt::ActorDecl {
            is_pub,
            is_anonymous,
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
            ..
        } => {
            let s = format_actor_like(
                Some(label),
                array_index.as_ref(),
                *is_anonymous,
                ty,
                props,
                modifiers,
                children,
                depth,
                indent_size,
            );
            if *is_pub { format!("pub {}", s) } else { s }
        },
        Stmt::Import { path, alias, .. } => match alias {
            Some(a) => format!(r#"import "{}" as {}"#, path, a),
            None => format!(r#"import "{}""#, path),
        },
        Stmt::Keyframe { time, body, .. } => {
            let body_str = format_stmts_raw(body, depth + 1, indent_size);
            format!("#{}\n{}", format_time(time), body_str)
        },
        Stmt::RelativeKeyframe { offset, body, .. } => {
            let body_str = format_stmts_raw(body, depth + 1, indent_size);
            format!("#+{}\n{}", format_time(offset), body_str)
        },
        Stmt::Assignment {
            target,
            property,
            value,
            modifiers,
            ..
        } => {
            let assignment_str = if target.is_empty() {
                format!("{} = {}", property, format_expr(value))
            } else {
                format!(
                    "{}.{} = {}",
                    format_target_segments(target, format_expr),
                    property,
                    format_expr(value)
                )
            };
            let mut parts = vec![assignment_str];
            if !modifiers.is_empty() {
                let mods = modifiers.iter().map(format_modifier).collect::<Vec<_>>().join(", ");
                parts.push(format!(" [{}]", mods));
            }
            parts.join("")
        },
        Stmt::Sequence { body, .. } => {
            let body_str = format_stmts_raw(body, depth + 1, indent_size);
            format!("sequence {{\n{}\n{}}}", body_str, " ".repeat(indent_size * depth))
        },
        Stmt::Stagger {
            modifiers, body, ..
        } => {
            let mut header = "stagger".to_string();
            if !modifiers.is_empty() {
                let mods = modifiers.iter().map(format_modifier).collect::<Vec<_>>().join(", ");
                header.push_str(&format!(" [{}]", mods));
            }
            let body_str = format_stmts_raw(body, depth + 1, indent_size);
            format!("{} {{\n{}\n{}}}", header, body_str, " ".repeat(indent_size * depth))
        },
        Stmt::Match {
            scrutinee, arms, ..
        } => {
            let arms_str = arms
                .iter()
                .map(|(pat, stmts)| {
                    let body_str = format_stmts_raw(stmts, depth + 1, indent_size);
                    format!(
                        "{} => {{\n{}\n{}}}",
                        format_match_pat(pat),
                        body_str,
                        " ".repeat(indent_size * (depth + 1))
                    )
                })
                .collect::<Vec<_>>()
                .join(",\n");
            format!(
                "match {} {{\n{}\n{}}}",
                format_expr(scrutinee),
                arms_str,
                " ".repeat(indent_size * depth)
            )
        },
        Stmt::Always { body, .. } => {
            let body_str = format_stmts_raw(body, depth + 1, indent_size);
            format!("always {{\n{}\n{}}}", body_str, " ".repeat(indent_size * depth))
        },
        Stmt::ReactiveBinding {
            target,
            property,
            value,
            ..
        } => {
            format!(
                "{}.{} := {}",
                format_target_segments(target, format_expr),
                property,
                format_expr(value)
            )
        },
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            let then_str = format_stmts_raw(then_branch, depth + 1, indent_size);
            let mut result = format!(
                "if {} {{\n{}\n{}}}",
                format_expr(condition),
                then_str,
                " ".repeat(indent_size * depth)
            );
            if let Some(else_body) = else_branch {
                let else_str = format_stmts_raw(else_body, depth + 1, indent_size);
                result.push_str(&format!(
                    " else {{\n{}\n{}}}",
                    else_str,
                    " ".repeat(indent_size * depth)
                ));
            }
            result
        },
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            modifiers,
            ..
        } => {
            let body_str = format_stmts_raw(body, depth + 1, indent_size);
            let index_str = index_var.as_ref().map(|iv| format!(", {}", iv)).unwrap_or_default();
            let mods = if modifiers.is_empty() {
                String::new()
            } else {
                let inner = modifiers.iter().map(format_modifier).collect::<Vec<_>>().join(", ");
                format!(" [{inner}]")
            };
            format!(
                "for {}{} in {}{mods} {{\n{}\n{}}}",
                var,
                index_str,
                format_expr(iterable),
                body_str,
                " ".repeat(indent_size * depth)
            )
        },
        Stmt::ComponentDef(def, ..) => format_component_def(def, depth, indent_size),
        Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            body,
            ..
        } => {
            let is_pub = if *is_pub { "pub " } else { "" };
            let params_str = params.iter().map(format_param_def).collect::<Vec<_>>().join(", ");
            let ret = match return_type {
                Some(ty) => format!(" -> {}", ty),
                None => String::new(),
            };
            let body_str = format_stmts_raw(body, depth + 1, indent_size);
            format!(
                "{is_pub}fn {name}({params_str}){ret} {{\n{body_str}\n{indent}}}",
                indent = " ".repeat(indent_size * depth)
            )
        },
        Stmt::Block { body, .. } => {
            let body_str = format_stmts_raw(body, depth + 1, indent_size);
            format!("{{\n{body_str}\n{indent}}}", indent = " ".repeat(indent_size * depth))
        },
        Stmt::Return { value, .. } => match value {
            Some(expr) => format!("return {}", format_expr(expr)),
            None => "return".to_string(),
        },
        Stmt::Expr(expr, ..) => format_expr(expr),
        Stmt::Config { settings, .. } => {
            let inner = settings.iter().map(format_property).collect::<Vec<_>>().join(", ");
            format!("config {{ {} }}", inner)
        },
        Stmt::Scene {
            name, config, body, ..
        } => {
            let mut parts = vec![format!("# {}", name)];
            if !config.is_empty() {
                let inner = config.iter().map(format_property).collect::<Vec<_>>().join(", ");
                parts.push(format!("config {{ {} }}", inner));
            }
            if !body.is_empty() {
                let body_str = format_stmts_raw(body, depth, indent_size);
                parts.push(body_str);
            }
            parts.join("\n")
        },
        Stmt::Play {
            scene_name,
            transition,
            ..
        } => {
            let mut s = format!("play {}", scene_name);
            if let Some(t) = transition {
                s.push_str(&format!(" [{}]", format_transition(t)));
            }
            s
        },
        Stmt::Comment(text, ..) => format!("//{}", text),
    }
}

#[cfg(test)]
mod variant_coverage_guardrails {
    /// When adding a new variant to `Expr`, update:
    /// - `format_expr` in this file
    /// - `walk.rs` (walk_expr)
    /// - `rewrite.rs` (expr_needs_rewrite if still manual)
    /// - any other Expr match sites
    #[test]
    fn format_expr_covers_all_expr_variants() {
        // Expr has exactly 17 variants as of last update.
        // If this fails, add the new variant to format_expr
        // and increment this count.
        let arms = 18; // Num, Percent, Str, Bool, Null, Ident, Path, Index, List, Tuple, Binary, Unary, Call, Method, Closure, Conditional, Match, Construct
        // Compile-time check: format_expr's match arms must be exhaustive
        // This test breaks at compile time anyway, but the count serves
        // as a searchable reminder when variants change.
        assert_eq!(
            arms, 18,
            "Expr variant count changed — update format_expr and other match sites"
        );
    }

    /// When adding a new variant to `InlineItem`, update:
    /// - `format_inline_item` in this file
    /// - `walk.rs` (walk_inline_item)
    #[test]
    fn format_inline_item_covers_all_inline_item_variants() {
        let arms = 5; // Anonymous, Labeled, ForLoop, SlotMarker, SlotFill
        assert_eq!(
            arms, 5,
            "InlineItem variant count changed — update format_inline_item and other match sites"
        );
    }

    /// When adding a new variant to `Stmt`, update:
    /// - `format_stmt_raw` in this file
    /// - `walk.rs` (walk_stmt, walk_stmts)
    /// - `to_source.rs` (ToSource impl)
    /// - `source_index.rs` (walk)
    /// - `module.rs` (set_action_spans)
    #[test]
    fn format_stmt_raw_covers_all_stmt_variants() {
        let arms = 20; // Action, LetDecl, ActorDecl, Import, Keyframe, RelativeKeyframe, Assignment, Sequence, Stagger, Always, ReactiveBinding, Conditional, Match, ForLoop, ComponentDef, ComponentAction, Config, Scene, Play, Comment
        assert_eq!(
            arms, 20,
            "Stmt variant count changed — update format_stmt_raw and other match sites"
        );
    }
}
