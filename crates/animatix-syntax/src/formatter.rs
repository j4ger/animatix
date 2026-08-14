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
//! - **AST-based**: delegates to [`format_core`](crate::format_core) for the actual serialization,
//!   with formatting options applied on top.
//!
//! # Examples
//!
//! ```
//! use animatix_syntax::formatter::{FormatConfig, Formatter};
//! use animatix_syntax::parser::parse_simple;
//!
//! let source = r#"#0s
//! title: Text, text: "Hello"
//! "#;
//!
//! let stmts = parse_simple(source).0.unwrap();
//! let formatter = Formatter::default();
//! let formatted = formatter.format(&stmts);
//! ```

use crate::ast::*;
use crate::format_core;

/// Configuration for the source formatter.
#[derive(Debug, Clone)]
pub struct FormatConfig {
    /// Number of spaces per indentation level. Default: 2.
    pub indent_size: usize,
    /// Number of blank lines between top-level statements. Default: 1.
    pub blank_lines_between_top_level: usize,
    /// Whether to add a trailing newline at the end of the file. Default: true.
    pub trailing_newline: bool,
}

impl Default for FormatConfig {
    fn default() -> Self {
        Self {
            indent_size: 2,
            blank_lines_between_top_level: 1,
            trailing_newline: true,
        }
    }
}

/// The source formatter.
///
/// Holds configuration and provides methods to format AST nodes or full files.
pub struct Formatter {
    config: FormatConfig,
}

impl Default for Formatter {
    fn default() -> Self {
        Self::new(FormatConfig::default())
    }
}

impl Formatter {
    /// Create a new formatter with the given configuration.
    pub fn new(config: FormatConfig) -> Self {
        Self { config }
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
        let mut result: Vec<String> = stmts.iter().map(|s| self.format_stmt(s, 0)).collect();

        // Remove empty lines at the start
        let first_non_empty = result.iter().position(|s| !s.is_empty()).unwrap_or(result.len());
        result.drain(..first_non_empty);

        let mut output = result.join(&separator);

        if self.config.trailing_newline && !output.ends_with('\n') {
            output.push('\n');
        }

        output
    }

    /// Format a single statement at the given indentation depth.
    fn format_stmt(&self, stmt: &Stmt, depth: usize) -> String {
        format_core::format_stmt_raw(stmt, depth, self.config.indent_size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_simple;

    fn parse(source: &str) -> Vec<Stmt> {
        parse_simple(source).0.expect("failed to parse")
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
        let stmts =
            parse("row: Row, gap: 8 {\n  a: Rect, size: (10, 10)\n  b: Rect, size: (20, 20)\n}");
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
        assert!(result.contains("    fade-in btn [1s]"));
    }

    #[test]
    fn format_blank_lines_between_top_level() {
        let stmts = parse("#0s\ntitle: Text\n\n#1s\nfade-in title [1s]");
        let formatter = Formatter::default();
        let result = formatter.format(&stmts);
        let lines: Vec<&str> = result.lines().collect();
        let blank_count = lines.iter().filter(|l| l.is_empty()).count();
        assert!(
            blank_count >= 1,
            "Expected at least 1 blank line, got {}: {}",
            blank_count,
            result
        );
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
        assert_eq!(
            first, second,
            "Formatter is not idempotent for:\n{}\n\nFirst pass:\n{}\n\nSecond pass:\n{}",
            source, first, second
        );
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
