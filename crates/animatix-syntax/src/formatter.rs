//! Configurable source formatter for `.amx` files.
//!
//! The formatter normalizes whitespace, indentation, and blank lines while
//! preserving semantic content. It operates on the AST (not raw text) so
//! it's safe to run after any mutation.
//!
//! # Design
//!
//! - **Idempotent**: formatting already-formatted code produces byte-identical output.
//! - **Configurable**: indent size, blank line rules, trailing commas.
//! - **AST-based**: uses `ToSource` internally, with formatting options applied.
//!
//! # Examples
//!
//! ```
//! use animatix_syntax::formatter::{Formatter, FormatConfig};
//! use animatix_syntax::parser::parser;
//! use chumsky::Parser;
//!
//! let source = r#"#0s
//! title: Text, text: "Hello"
//! "#;
//!
//! let stmts = parser().parse(source).into_result().unwrap();
//! let formatter = Formatter::default();
//! let formatted = formatter.format(&stmts);
//! ```

use crate::ast::*;
use crate::to_source::ToSource;

/// Configuration for the source formatter.
#[derive(Debug, Clone)]
pub struct FormatConfig {
    /// Number of spaces per indentation level. Default: 2.
    pub indent_size: usize,
    /// Number of blank lines between top-level statements. Default: 1.
    pub blank_lines_between_top_level: usize,
    /// Whether to add a trailing newline at the end of the file. Default: true.
    pub trailing_newline: bool,
    /// Whether to normalize inline comments to have exactly 2 spaces before `//`. Default: true.
    pub normalize_comment_spacing: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_size: 2,
            blank_lines_between_top_level: 1,
            trailing_newline: true,
            normalize_comment_spacing: true,
        }
    }
}

/// The source formatter.
///
/// Holds configuration and provides methods to format AST nodes or full files.
pub struct Formatter {
    config: FormatConfig,
}

impl Formatter {
    /// Create a new formatter with the given configuration.
    pub fn new(config: FormatConfig) -> Self {
        Self { config }
    }

    /// Create a formatter with default configuration.
    pub fn default() -> Self {
        Self::new(FormatConfig::default())
    }

    /// Get the current configuration.
    pub fn config(&self) -> &FormatConfig {
        &self.config
    }

    /// Format a list of top-level statements (full file contents).
    ///
    /// This is the main entry point for formatting an entire `.amx` file.
    pub fn format(&self, stmts: &[Stmt]) -> String {
        if stmts.is_empty() {
            return String::new();
        }

        let separator = "\n".repeat(1 + self.config.blank_lines_between_top_level);
        let mut result: Vec<String> = stmts
            .iter()
            .map(|s| self.format_stmt(s, 0))
            .collect();

        // Remove empty lines at the start
        while result.first().is_some_and(|s| s.is_empty()) {
            result.remove(0);
        }

        let mut output = result.join(&separator);

        if self.config.trailing_newline && !output.ends_with('\n') {
            output.push('\n');
        }

        output
    }

    /// Format a single statement at the given indentation depth.
    fn format_stmt(&self, stmt: &Stmt, depth: usize) -> String {
        match stmt {
            Stmt::Action(a, ..) => a.to_source(),
            Stmt::LetDecl { is_pub, name, value, .. } => {
                let pub_kw = if *is_pub { "pub " } else { "" };
                format!("{}let {} = {}", pub_kw, name, self.format_expr(value))
            }
            Stmt::ActorDecl { is_pub, is_anonymous, label, ty, props, modifiers, children, .. } => {
                let s = self.format_actor_like(
                    Some(label),
                    *is_anonymous,
                    ty,
                    props,
                    modifiers,
                    children,
                    depth,
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
                let body_str = self.format_stmts(body, depth);
                format!("#{}\n{}", time.to_source(), body_str)
            }
            Stmt::RelativeKeyframe { offset, body, .. } => {
                let body_str = self.format_stmts(body, depth);
                format!("#+{}\n{}", offset.to_source(), body_str)
            }
            Stmt::Assignment { target, property, value, modifiers, .. } => {
                let assignment_str = if target.is_empty() {
                    format!("{} = {}", property, self.format_expr(value))
                } else {
                    format!("{}.{} = {}", target.join("."), property, self.format_expr(value))
                };
                let mut parts = vec![assignment_str];
                if !modifiers.is_empty() {
                    let mods = modifiers.iter().map(|m| m.to_source()).collect::<Vec<_>>().join(", ");
                    parts.push(format!(" [{}]", mods));
                }
                parts.join("")
            }
            Stmt::Sequence { body, .. } => {
                let body_str = self.format_stmts(body, depth + 1);
                format!("sequence {{\n{}\n{}}}", body_str, self.indent(depth))
            }
            Stmt::Stagger { modifiers, body, .. } => {
                let mut header = "stagger".to_string();
                if !modifiers.is_empty() {
                    let mods = modifiers.iter().map(|m| m.to_source()).collect::<Vec<_>>().join(", ");
                    header.push_str(&format!(" [{}]", mods));
                }
                let body_str = self.format_stmts(body, depth + 1);
                format!("{} {{\n{}\n{}}}", header, body_str, self.indent(depth))
            }
            Stmt::Always { body, .. } => {
                let body_str = self.format_stmts(body, depth + 1);
                format!("always {{\n{}\n{}}}", body_str, self.indent(depth))
            }
            Stmt::Drive { label, body, .. } => {
                let body_str = self.format_stmts(body, depth + 1);
                format!("drive {} {{\n{}\n{}}}", label, body_str, self.indent(depth))
            }
            Stmt::ReactiveBinding { target, property, value, .. } => {
                format!("{}.{} := {}", target.join("."), property, self.format_expr(value))
            }
            Stmt::Conditional { condition, then_branch, else_branch, .. } => {
                let then_str = self.format_stmts(then_branch, depth + 1);
                let mut result = format!("if {} {{\n{}\n{}}}", self.format_expr(condition), then_str, self.indent(depth));
                if let Some(else_body) = else_branch {
                    let else_str = self.format_stmts(else_body, depth + 1);
                    result.push_str(&format!(" else {{\n{}\n{}}}", else_str, self.indent(depth)));
                }
                result
            }
            Stmt::ForLoop { var, iterable, body, .. } => {
                let body_str = self.format_stmts(body, depth + 1);
                format!("for {} in {} {{\n{}\n{}}}", var, self.format_expr(iterable), body_str, self.indent(depth))
            }
            Stmt::ComponentDef(def, ..) => self.format_component_def(def, depth),
            Stmt::ComponentAction { name, params, body, .. } => {
                let params_str = params
                    .iter()
                    .map(|p| p.to_source())
                    .collect::<Vec<_>>()
                    .join(", ");
                let body_str = self.format_stmts(body, depth + 1);
                format!("action {}({}) {{\n{}\n{}}}", name, params_str, body_str, self.indent(depth))
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
                    let body_str = self.format_stmts(body, depth);
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
            Stmt::Comment(text, ..) => {
                format!("//{}", text)
            }
        }
    }

    /// Format a list of statements at the given indentation depth.
    fn format_stmts(&self, stmts: &[Stmt], depth: usize) -> String {
        let mut result = Vec::new();
        for (i, stmt) in stmts.iter().enumerate() {
            let formatted = self.format_stmt(stmt, depth);
            // Indent each line of the formatted statement
            let indented = formatted
                .lines()
                .map(|line| {
                    if line.is_empty() {
                        String::new()
                    } else {
                        format!("{}{}", self.indent(depth), line)
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            
            // Add blank line before keyframes (except the first statement)
            if i > 0 && matches!(stmt, Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. }) {
                result.push(String::new()); // blank line
            }
            
            result.push(indented);
        }
        result.join("\n")
    }

    /// Format an expression.
    fn format_expr(&self, expr: &Expr) -> String {
        expr.to_source()
    }

    /// Format an actor-like declaration.
    fn format_actor_like(
        &self,
        label: Option<&str>,
        is_anonymous: bool,
        ty: &str,
        props: &[Property],
        modifiers: &[Modifier],
        children: &[InlineItem],
        depth: usize,
    ) -> String {
        let mut parts = Vec::new();
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
                .map(|c| self.format_inline_item(c, depth + 1))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!(" {{\n{}\n{}}}", children_str, self.indent(depth)));
        }
        parts.join("")
    }

    /// Format an inline item (child of a container).
    fn format_inline_item(&self, item: &InlineItem, depth: usize) -> String {
        let result = match item {
            InlineItem::Anonymous { ty, props, modifiers, children, .. } => {
                self.format_actor_like(
                    None,
                    true,
                    ty,
                    props,
                    modifiers,
                    children,
                    depth,
                )
            }
            InlineItem::Labeled { label, ty, props, modifiers, children, .. } => {
                self.format_actor_like(
                    Some(label),
                    false,
                    ty,
                    props,
                    modifiers,
                    children,
                    depth,
                )
            }
            InlineItem::SlotMarker => "@slot".into(),
            InlineItem::SlotFill { slot, items } => {
                let items_str = items
                    .iter()
                    .map(|i| self.format_inline_item(i, depth + 1))
                    .collect::<Vec<_>>()
                    .join("\n");
                format!("@{} {{\n{}\n{}}}", slot, items_str, self.indent(depth))
            }
        };
        
        // Add indentation to each line
        result
            .lines()
            .map(|line| {
                if line.is_empty() {
                    String::new()
                } else {
                    format!("{}{}", self.indent(depth), line)
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Format a component definition.
    fn format_component_def(&self, def: &ComponentDef, depth: usize) -> String {
        let params_str = def
            .params
            .iter()
            .map(|p| p.to_source())
            .collect::<Vec<_>>()
            .join(", ");
        let body_str = def
            .body
            .iter()
            .map(|s| self.format_stmt(s, depth + 1))
            .collect::<Vec<_>>()
            .join("\n");
        let pub_kw = if def.is_pub { "pub " } else { "" };
        format!("{}component {}({}) {{\n{}\n{}}}", pub_kw, def.name, params_str, body_str, self.indent(depth))
    }

    /// Generate indentation string for the given depth.
    fn indent(&self, depth: usize) -> String {
        " ".repeat(self.config.indent_size * depth)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parser;
    use chumsky::Parser;

    fn parse(source: &str) -> Vec<Stmt> {
        parser().parse(source).into_result().expect("failed to parse")
    }

    #[test]
    fn format_empty_file() {
        let formatter = Formatter::default();
        let result = formatter.format(&[]);
        assert_eq!(result, "");
    }

    #[test]
    fn format_single_actor() {
        let stmts = parse(r#"title: Text, text: "Hello""#);
        let formatter = Formatter::default();
        let result = formatter.format(&stmts);
        assert_eq!(result.trim(), r#"title: Text, text: "Hello""#);
    }

    #[test]
    fn format_keyframe_with_body() {
        let stmts = parse("#0s\ntitle: Text, text: \"Hello\"");
        let formatter = Formatter::default();
        let result = formatter.format(&stmts);
        assert!(result.contains("#0s"));
        assert!(result.contains("title: Text, text: \"Hello\""));
    }

    #[test]
    fn format_container_with_children() {
        let stmts = parse("row: Row, gap: 8 {\n  a: Rect, size: (10, 10)\n  b: Rect, size: (20, 20)\n}");
        let formatter = Formatter::default();
        let result = formatter.format(&stmts);
        assert!(result.contains("row: Row, gap: 8"));
        assert!(result.contains("a: Rect, size: (10, 10)"));
        assert!(result.contains("b: Rect, size: (20, 20)"));
    }

    #[test]
    fn format_preserves_config() {
        let stmts = parse("config { resolution: (1280, 720) }");
        let formatter = Formatter::default();
        let result = formatter.format(&stmts);
        assert!(result.contains("config { resolution: (1280, 720) }"));
    }

    #[test]
    fn format_custom_indent() {
        let config = FormatConfig {
            indent_size: 4,
            ..Default::default()
        };
        let formatter = Formatter::new(config);
        let stmts = parse("sequence {\nfade-in btn [1s]\n}");
        let result = formatter.format(&stmts);
        // Should use 4 spaces for indentation
        assert!(result.contains("    fade-in btn [1s]"));
    }

    #[test]
    fn format_blank_lines_between_top_level() {
        let stmts = parse("#0s\ntitle: Text\n\n#1s\nfade-in title [1s]");
        let formatter = Formatter::default();
        let result = formatter.format(&stmts);
        // Should have blank line between keyframes
        let lines: Vec<&str> = result.lines().collect();
        let blank_count = lines.iter().filter(|l| l.is_empty()).count();
        assert!(blank_count >= 1, "Expected at least 1 blank line, got {}: {}", blank_count, result);
    }

    #[test]
    fn format_trailing_newline() {
        let stmts = parse("title: Text, text: \"Hello\"");
        let formatter = Formatter::default();
        let result = formatter.format(&stmts);
        assert!(result.ends_with('\n'), "Expected trailing newline");
    }

    #[test]
    fn format_no_trailing_newline() {
        let config = FormatConfig {
            trailing_newline: false,
            ..Default::default()
        };
        let formatter = Formatter::new(config);
        let stmts = parse("title: Text, text: \"Hello\"");
        let result = formatter.format(&stmts);
        assert!(!result.ends_with('\n'), "Expected no trailing newline");
    }

    // ── Idempotency tests ──────────────────────────────────────────────

    fn assert_idempotent(source: &str) {
        let stmts = parse(source);
        let formatter = Formatter::default();
        let first = formatter.format(&stmts);
        let stmts2 = parse(&first);
        let second = formatter.format(&stmts2);
        assert_eq!(first, second, "Formatter is not idempotent for:\n{}\n\nFirst pass:\n{}\n\nSecond pass:\n{}", source, first, second);
    }

    #[test]
    fn idempotent_single_actor() {
        assert_idempotent(r#"title: Text, text: "Hello""#);
    }

    #[test]
    fn idempotent_keyframe() {
        assert_idempotent("#0s\ntitle: Text, text: \"Hello\"");
    }

    #[test]
    fn idempotent_sequence() {
        assert_idempotent("sequence {\nfade-in btn [1s]\nfade-in title [500ms]\n}");
    }

    #[test]
    fn idempotent_config() {
        assert_idempotent("config { resolution: (1280, 720) }");
    }

    #[test]
    fn idempotent_import() {
        assert_idempotent(r#"import "theme.amx""#);
    }

    #[test]
    fn idempotent_assignment() {
        assert_idempotent("btn.color = red");
    }

    #[test]
    fn idempotent_scene() {
        assert_idempotent("# Intro\ntitle: Text, text: \"Hello\"");
    }

    #[test]
    fn idempotent_play() {
        assert_idempotent("# Intro\nplay Outro [fade, 300ms]");
    }
}
