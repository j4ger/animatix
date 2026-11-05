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
//! - **AST-based**: delegates to [`format_core`] for the actual serialization,
//!   with formatting options applied on top.
//! - **Comment-preserving**: [`crate::formatter::Formatter::format_source`] reinserts every `//` comment from
//!   the source token stream; [`crate::formatter::Formatter::format`] alone sees only the AST and cannot.
//!
//! # Examples
//!
//! ```
//! use animatix_syntax::formatter::{FormatConfig, Formatter};
//! use animatix_syntax::parser::parse_source;
//!
//! let source = r#"#0s
//! title: Text, text: "Hello"
//! "#;
//!
//! let stmts = parse_source(source).0.unwrap();
//! let formatter = Formatter::default();
//! let formatted = formatter.format(&stmts);
//! ```

use crate::ast::*;
use crate::format_core;
use crate::token::{Token, TokenKind};

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

    /// Format a list of top-level statements.
    ///
    /// This is the AST-only entry point: it cannot emit comments that live only
    /// in the source text. Use [`format_source`](Self::format_source) when the
    /// original source is available (the CLI, GUI, and LSP all do).
    pub fn format(&self, stmts: &[Stmt]) -> String {
        if stmts.is_empty() {
            return String::new();
        }

        let separator = "\n".repeat(1 + self.config.blank_lines_between_top_level);
        let mut result: Vec<String> = stmts
            .iter()
            .map(|s| format_core::format_stmt_raw(s, 0, self.config.indent_size))
            .collect();

        // Remove empty lines at the start
        let first_non_empty = result.iter().position(|s| !s.is_empty()).unwrap_or(result.len());
        result.drain(..first_non_empty);

        let mut output = result.join(&separator);

        if self.config.trailing_newline && !output.ends_with('\n') {
            output.push('\n');
        }

        output
    }

    /// Format source text while preserving every `//` comment verbatim.
    ///
    /// [`format`](Self::format) rebuilds text from the AST, which cannot carry
    /// free-standing comments. This entry point formats the AST first, then
    /// reinserts the comments from the lossless token stream at the formatted
    /// positions of their neighbouring code tokens. File headers, free-standing
    /// comment lines, comments inside keyframe/block bodies, trailing comments,
    /// and comment-only files are all preserved.
    ///
    /// `source` must be the text `stmts` was parsed from.
    pub fn format_source(&self, source: &str, stmts: &[Stmt]) -> String {
        // The AST serializer prints `Property::trailing_comment` itself, which
        // can swallow code when a commented property shares a line with later
        // properties. Strip every AST-carried comment and reinsert all comments
        // from the token stream instead, so there is exactly one source of truth.
        let mut without_comments = stmts.to_vec();
        clear_trailing_comments(&mut without_comments);
        let formatted = self.format(&without_comments);

        let source_tokens = crate::token::tokenize(source);
        let has_code = source_tokens.iter().any(|t| !is_comment_token(t));
        if !has_code {
            return comment_only_file(source, self.config.trailing_newline);
        }

        reinsert_comments(source, &source_tokens, &formatted, self.config.indent_size)
    }
}

/// Whether a token is a `//` comment.
fn is_comment_token(token: &Token) -> bool {
    matches!(token.kind, TokenKind::Comment(_))
}

/// Preserve a file that contains only comments (the AST is empty, so there is
/// nothing to normalise). Indentation and blank lines are left verbatim.
fn comment_only_file(source: &str, trailing_newline: bool) -> String {
    let trimmed = source.trim_end_matches(['\n', '\r']);
    if trimmed.is_empty() {
        return String::new();
    }
    if trailing_newline {
        format!("{trimmed}\n")
    } else {
        trimmed.to_string()
    }
}

// ---------------------------------------------------------------------------
// Comment reinsertion
// ---------------------------------------------------------------------------

/// One comment from the source, positioned relative to the code tokens.
#[derive(Debug)]
struct SourceComment {
    /// Text after `//` (the tokenizer drops the leading slashes).
    text: String,
    /// Number of non-comment tokens that appear before this comment.
    code_before: usize,
    /// True when the comment shares a line with preceding code.
    trailing: bool,
    /// Leading whitespace of the comment's source line, used as a fallback
    /// when the AST serializer emits a line without indentation (for example
    /// container inline children).
    source_indent: String,
}

/// A pending text insertion into the formatted document.
#[derive(Debug)]
struct CommentEdit {
    position: usize,
    order: usize,
    text: String,
}

/// Reinsert every source comment into the formatted document.
fn reinsert_comments(
    source: &str,
    source_tokens: &[Token],
    formatted: &str,
    indent_size: usize,
) -> String {
    let source_code: Vec<&Token> = source_tokens.iter().filter(|t| !is_comment_token(t)).collect();
    if source_code.is_empty() {
        return formatted.to_string();
    }
    let formatted_tokens = crate::token::tokenize(formatted);
    let formatted_code: Vec<&Token> =
        formatted_tokens.iter().filter(|t| !is_comment_token(t)).collect();
    let mapping = align_code_tokens(&source_code, &formatted_code);

    let mut comments = Vec::new();
    let mut code_before = 0usize;
    for token in source_tokens {
        match &token.kind {
            TokenKind::Comment(text) => {
                let line_start = source[..token.span.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
                let trailing =
                    source[line_start..token.span.start].chars().any(|ch| !ch.is_whitespace());
                comments.push(SourceComment {
                    text: text.clone(),
                    code_before,
                    trailing,
                    source_indent: leading_indent(source, line_start).to_string(),
                });
            },
            _ => code_before += 1,
        }
    }

    let mut edits = Vec::with_capacity(comments.len());
    for (order, comment) in comments.into_iter().enumerate() {
        let edit = if comment.trailing {
            trailing_comment_edit(
                formatted,
                &formatted_code,
                &mapping,
                &comment,
                order,
                indent_size,
            )
        } else {
            standalone_comment_edit(
                formatted,
                &formatted_code,
                &mapping,
                &comment,
                order,
                indent_size,
            )
        };
        if let Some(edit) = edit {
            edits.push(edit);
        }
    }

    apply_comment_edits(formatted, edits)
}

/// Build the edit for a trailing comment (one that follows code on its line).
fn trailing_comment_edit(
    formatted: &str,
    formatted_code: &[&Token],
    mapping: &[usize],
    comment: &SourceComment,
    order: usize,
    indent_size: usize,
) -> Option<CommentEdit> {
    let previous = previous_formatted_token(mapping, comment.code_before, formatted_code.len())?;
    let token = formatted_code[previous];
    let position = token.span.end;
    let (line_start, line_end) = line_bounds(formatted, position);
    let rest_of_line = &formatted[position..line_end];
    if rest_of_line.trim().is_empty() {
        Some(CommentEdit {
            position: line_end,
            order,
            text: format!("  //{}", comment.text),
        })
    } else {
        // The formatted line continues past this token (the AST collapsed a
        // multi-line declaration). Break the line so the `//` cannot swallow
        // the remaining code.
        let base = leading_indent(formatted, line_start).to_string();
        let continuation = format!("{base}{}", " ".repeat(indent_size));
        Some(CommentEdit {
            position,
            order,
            text: format!("  //{}\n{continuation}", comment.text),
        })
    }
}

/// Build the edit for a free-standing comment line.
fn standalone_comment_edit(
    formatted: &str,
    formatted_code: &[&Token],
    mapping: &[usize],
    comment: &SourceComment,
    order: usize,
    indent_size: usize,
) -> Option<CommentEdit> {
    let anchor = next_formatted_token(mapping, comment.code_before, formatted_code.len());
    if anchor >= formatted_code.len() {
        let indent = previous_non_empty_line_start(formatted, formatted.len())
            .map(|line_start| leading_indent(formatted, line_start).to_string())
            .unwrap_or_default();
        let mut text = String::new();
        if !formatted.is_empty() && !formatted.ends_with('\n') {
            text.push('\n');
        }
        text.push_str(&format!("{indent}//{}", comment.text));
        text.push('\n');
        return Some(CommentEdit {
            position: formatted.len(),
            order,
            text,
        });
    }

    let token = formatted_code[anchor];
    let (line_start, _) = line_bounds(formatted, token.span.start);
    if formatted[line_start..token.span.start].trim().is_empty() {
        let mut indent = if matches!(token.kind, TokenKind::RBrace) {
            brace_comment_indent(formatted, line_start, indent_size)
        } else {
            leading_indent(formatted, line_start).to_string()
        };
        if indent.is_empty() {
            indent = comment.source_indent.clone();
        }
        return Some(CommentEdit {
            position: line_start,
            order,
            text: format!("{indent}//{}\n", comment.text),
        });
    }

    // The anchor is mid-line: split before it so the comment stays on its own
    // line and the remaining code cannot be commented out.
    let base = leading_indent(formatted, line_start).to_string();
    let continuation = format!("{base}{}", " ".repeat(indent_size));
    Some(CommentEdit {
        position: token.span.start,
        order,
        text: format!("\n{continuation}//{}\n{continuation}", comment.text),
    })
}

/// Indentation for a comment inserted immediately before a closing brace.
fn brace_comment_indent(formatted: &str, line_start: usize, indent_size: usize) -> String {
    let line_indent = leading_indent(formatted, line_start).to_string();
    if line_start == 0 {
        return " ".repeat(indent_size);
    }
    let mut search = line_start - 1;
    loop {
        let previous_start = formatted[..search].rfind('\n').map(|i| i + 1).unwrap_or(0);
        let previous_line = formatted[previous_start..search].trim_end();
        if !previous_line.is_empty() {
            if previous_line.ends_with('{') {
                return format!("{line_indent}{}", " ".repeat(indent_size));
            }
            let previous_indent = leading_indent(formatted, previous_start);
            return if previous_indent.len() >= line_indent.len() {
                previous_indent.to_string()
            } else {
                line_indent
            };
        }
        if previous_start == 0 {
            break;
        }
        search = previous_start.saturating_sub(1);
    }
    line_indent
}

/// Apply edits in ascending position order. Edits at the same position keep
/// source order, which keeps consecutive comment lines consecutive.
fn apply_comment_edits(formatted: &str, mut edits: Vec<CommentEdit>) -> String {
    edits.sort_by_key(|edit| (edit.position, edit.order));
    let extra: usize = edits.iter().map(|edit| edit.text.len()).sum();
    let mut output = String::with_capacity(formatted.len() + extra);
    let mut cursor = 0usize;
    for edit in edits {
        if edit.position < cursor {
            continue;
        }
        output.push_str(&formatted[cursor..edit.position]);
        output.push_str(&edit.text);
        cursor = edit.position;
    }
    output.push_str(&formatted[cursor..]);
    output
}

/// Map each source code token to its formatted counterpart. Equal token *kinds*
/// are aligned; when the serializer drops or introduces tokens, the nearest
/// match within a small window is used so later comments keep their anchors.
fn align_code_tokens(source: &[&Token], formatted: &[&Token]) -> Vec<usize> {
    let mut mapping = vec![formatted.len(); source.len()];
    let mut source_index = 0usize;
    let mut formatted_index = 0usize;
    while source_index < source.len() && formatted_index < formatted.len() {
        if same_token_kind(source[source_index], formatted[formatted_index]) {
            mapping[source_index] = formatted_index;
            source_index += 1;
            formatted_index += 1;
            continue;
        }
        let source_ahead = (1usize..=8).find(|&skip| {
            source_index + skip < source.len()
                && same_token_kind(source[source_index + skip], formatted[formatted_index])
        });
        let formatted_ahead = (1usize..=8).find(|&skip| {
            formatted_index + skip < formatted.len()
                && same_token_kind(source[source_index], formatted[formatted_index + skip])
        });
        match (source_ahead, formatted_ahead) {
            (Some(source_skip), Some(formatted_skip)) if source_skip <= formatted_skip => {
                source_index += source_skip;
            },
            (Some(_), Some(formatted_skip)) => formatted_index += formatted_skip,
            (Some(source_skip), None) => source_index += source_skip,
            (None, Some(formatted_skip)) => formatted_index += formatted_skip,
            (None, None) => {
                source_index += 1;
                formatted_index += 1;
            },
        }
    }
    mapping
}

/// Compare token kinds without comparing literal values (formatting may
/// normalise `1.0` to `1` while keeping the token sequence aligned).
fn same_token_kind(left: &Token, right: &Token) -> bool {
    std::mem::discriminant(&left.kind) == std::mem::discriminant(&right.kind)
}

/// The formatted token index for a source token, falling back to the next
/// aligned token when the serializer dropped the original.
fn next_formatted_token(mapping: &[usize], code_before: usize, formatted_len: usize) -> usize {
    mapping
        .iter()
        .skip(code_before)
        .copied()
        .find(|&index| index < formatted_len)
        .unwrap_or(formatted_len)
}

/// The previous formatted token index before a source position.
fn previous_formatted_token(
    mapping: &[usize],
    code_before: usize,
    formatted_len: usize,
) -> Option<usize> {
    mapping[..code_before.min(mapping.len())]
        .iter()
        .rev()
        .copied()
        .find(|&index| index < formatted_len)
}

/// Byte bounds of the line containing `position`.
fn line_bounds(text: &str, position: usize) -> (usize, usize) {
    let start = text[..position].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let end = text[position..].find('\n').map(|i| position + i).unwrap_or(text.len());
    (start, end)
}

/// Start offset of the last non-empty line before `before`.
fn previous_non_empty_line_start(text: &str, before: usize) -> Option<usize> {
    if before == 0 {
        return None;
    }
    let mut search = before.saturating_sub(1);
    loop {
        let line_start = text[..search].rfind('\n').map(|i| i + 1).unwrap_or(0);
        if !text[line_start..search].trim().is_empty() {
            return Some(line_start);
        }
        if line_start == 0 {
            return None;
        }
        search = line_start.saturating_sub(1);
    }
}

/// Leading spaces/tabs of the line beginning at `line_start`.
fn leading_indent(text: &str, line_start: usize) -> &str {
    let rest = &text[line_start..];
    let length = rest.bytes().take_while(|byte| *byte == b' ' || *byte == b'\t').count();
    &rest[..length]
}

// ---------------------------------------------------------------------------
// AST comment scrubbing
// ---------------------------------------------------------------------------

/// Clear every AST-carried comment so `format_source` can reinsert comments
/// from the token stream without duplication.
fn clear_trailing_comments(stmts: &mut [Stmt]) {
    for stmt in stmts {
        clear_stmt_comments(stmt);
    }
}

fn clear_props_comments(props: &mut [Property]) {
    for prop in props {
        prop.trailing_comment = None;
        clear_expr_comments(&mut prop.value);
    }
}

fn clear_inline_comments(items: &mut [InlineItem]) {
    for item in items {
        match item {
            InlineItem::Anonymous {
                props, children, ..
            }
            | InlineItem::Labeled {
                props, children, ..
            } => {
                clear_props_comments(props);
                clear_inline_comments(children);
            },
            InlineItem::ForLoop { iterable, body, .. } => {
                clear_expr_comments(iterable);
                clear_inline_comments(body);
            },
            InlineItem::SlotFill { items, .. } => clear_inline_comments(items),
            InlineItem::SlotMarker => {},
        }
    }
}

fn clear_modifiers_comments(modifiers: &mut [Modifier]) {
    for modifier in modifiers {
        clear_expr_comments(&mut modifier.value);
    }
}

fn clear_target_comments(target: &mut [TargetSegment]) {
    for segment in target {
        if let TargetSegment::Indexed { index, .. } = segment {
            clear_expr_comments(index);
        }
    }
}

fn clear_params_comments(params: &mut [ParamDef]) {
    for param in params {
        if let Some(default) = &mut param.default {
            clear_expr_comments(default);
        }
    }
}

fn clear_expr_comments(expr: &mut Expr) {
    match expr {
        Expr::Index(target, index) => {
            clear_expr_comments(target);
            clear_expr_comments(index);
        },
        Expr::Tuple(items) | Expr::List(items) => {
            for item in items {
                clear_expr_comments(item);
            }
        },
        Expr::Binary(left, _, right) => {
            clear_expr_comments(left);
            clear_expr_comments(right);
        },
        Expr::Unary(_, inner) => clear_expr_comments(inner),
        Expr::Call(_, args) => {
            for arg in args {
                clear_expr_comments(arg);
            }
        },
        Expr::Method(receiver, _, args) => {
            clear_expr_comments(receiver);
            for arg in args {
                clear_expr_comments(arg);
            }
        },
        Expr::Closure(_, body) => clear_expr_comments(body),
        Expr::LetChain(bindings, tail) => {
            for (_, value) in bindings {
                clear_expr_comments(value);
            }
            clear_expr_comments(tail);
        },
        Expr::Conditional(condition, then_expr, else_expr) => {
            clear_expr_comments(condition);
            clear_expr_comments(then_expr);
            clear_expr_comments(else_expr);
        },
        Expr::Match(scrutinee, arms) => {
            clear_expr_comments(scrutinee);
            for (_, arm) in arms {
                clear_expr_comments(arm);
            }
        },
        Expr::Construct(_, props) => clear_props_comments(props),
        _ => {},
    }
}

fn clear_stmt_comments(stmt: &mut Stmt) {
    match stmt {
        Stmt::Action(action, _) => {
            for arg in &mut action.args {
                clear_expr_comments(arg);
            }
            for index in action.target_index.iter_mut().flatten() {
                clear_expr_comments(index);
            }
            clear_modifiers_comments(&mut action.modifiers);
        },
        Stmt::LetDecl { value, .. }
        | Stmt::Return {
            value: Some(value), ..
        }
        | Stmt::Expr(value, _) => clear_expr_comments(value),
        Stmt::ActorDecl {
            array_index,
            props,
            children,
            ..
        } => {
            if let Some(index) = array_index {
                clear_expr_comments(index);
            }
            clear_props_comments(props);
            clear_inline_comments(children);
        },
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::Block { body, .. } => clear_trailing_comments(body),
        Stmt::Assignment {
            target,
            value,
            modifiers,
            ..
        } => {
            clear_target_comments(target);
            clear_expr_comments(value);
            clear_modifiers_comments(modifiers);
        },
        Stmt::ReactiveBinding { target, value, .. } => {
            clear_target_comments(target);
            clear_expr_comments(value);
        },
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            clear_expr_comments(condition);
            clear_trailing_comments(then_branch);
            if let Some(else_body) = else_branch {
                clear_trailing_comments(else_body);
            }
        },
        Stmt::Match {
            scrutinee, arms, ..
        } => {
            clear_expr_comments(scrutinee);
            for (_, body) in arms {
                clear_trailing_comments(body);
            }
        },
        Stmt::ForLoop {
            iterable,
            body,
            modifiers,
            ..
        } => {
            clear_expr_comments(iterable);
            clear_modifiers_comments(modifiers);
            clear_trailing_comments(body);
        },
        Stmt::ComponentDef(def, _) => {
            clear_params_comments(&mut def.params);
            clear_trailing_comments(&mut def.body);
        },
        Stmt::FnDecl { params, body, .. } => {
            clear_params_comments(params);
            clear_trailing_comments(body);
        },
        Stmt::Config { settings, .. } => clear_props_comments(settings),
        Stmt::Scene { config, body, .. } => {
            clear_props_comments(config);
            clear_trailing_comments(body);
        },
        Stmt::TypeAlias { .. }
        | Stmt::Import { .. }
        | Stmt::Play { .. }
        | Stmt::Step { .. }
        | Stmt::Return { value: None, .. }
        | Stmt::Comment(..) => {},
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_source;

    fn parse(source: &str) -> Vec<Stmt> {
        parse_source(source).0.expect("failed to parse")
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

    // ── Comment preservation ────────────────────────────────────────────

    fn format_source(source: &str) -> String {
        let stmts = parse(source);
        Formatter::default().format_source(source, &stmts)
    }

    fn comment_texts(source: &str) -> Vec<String> {
        crate::token::tokenize(source)
            .into_iter()
            .filter_map(|token| match token.kind {
                TokenKind::Comment(text) => Some(text),
                _ => None,
            })
            .collect()
    }

    const COMMENT_FIXTURES: &[&str] = &[
        // File header + leading/free-standing comments between top-level statements.
        "// file header one\n// file header two\n\nlet a = 1\n\n// about b\nlet b = 2\n",
        // Free-standing and trailing comments inside a keyframe block.
        "#0s\n// leading in keyframe\ntitle.opacity = 0 // trailing assignment\n// tail in keyframe\n",
        // Trailing comment after a property.
        "title: Text, text: \"Hello\" // trailing property\n",
        // Trailing comments after non-property statements.
        "let a = 1 // trailing let\n#1s\nfade-in a [1s] // trailing action\n",
        // Comment-only file.
        "// only a comment\n// second line\n",
        // Comment inside a multi-line expression that formatting collapses.
        "let x = foo(\n  1, // first arg\n  2\n)\n",
        // Comment inside a container's inline children block.
        "row: Row, gap: 8 {\n  // child comment\n  a: Rect, size: (10, 10)\n}\n",
        // Comments at both ends of a sequence block, plus a trailing action.
        "sequence {\n  // before action\n  fade-in a [1s] // after action\n  // after action line\n}\n",
        // Trailing comments inside a type constructor's property list.
        "circ = Circle {\n  at: (0, 0), // center\n  radius: 100, // size\n}\n",
    ];

    #[test]
    fn preserves_every_comment_class() {
        for source in COMMENT_FIXTURES {
            let formatted = format_source(source);
            assert_eq!(
                comment_texts(source),
                comment_texts(&formatted),
                "comments changed for source:\n{source}\nformatted:\n{formatted}"
            );
        }
        let inline =
            format_source("row: Row, gap: 8 {\n  // child comment\n  a: Rect, size: (10, 10)\n}\n");
        assert!(
            inline.contains("  // child comment"),
            "inline comment lost its indent:\n{inline}"
        );
    }

    #[test]
    fn comment_only_file_is_preserved_verbatim() {
        let source = "// first\n\n// second\n";
        assert_eq!(format_source(source), source);
    }

    #[test]
    fn property_comment_cannot_swallow_following_property() {
        let source =
            "row: Row, gap: 8 {\n  a: Rect, size: (10, 10), // half extents\n  color: red\n}\n";
        let formatted = format_source(source);
        assert_eq!(comment_texts(source), comment_texts(&formatted));
        // The `color` property must still be parsed after formatting.
        let reparsed = parse(&formatted);
        let Stmt::ActorDecl { children, .. } = &reparsed[0] else {
            panic!("expected actor declaration, got {:?}", reparsed[0]);
        };
        let InlineItem::Labeled { props, .. } = &children[0] else {
            panic!("expected labeled inline child");
        };
        assert_eq!(props.len(), 2, "formatted output lost a property:\n{formatted}");
    }

    #[test]
    fn comment_formatting_is_idempotent() {
        for source in COMMENT_FIXTURES {
            let first = format_source(source);
            let second = format_source(&first);
            assert_eq!(
                first, second,
                "comment formatting is not idempotent for:\n{source}\nfirst:\n{first}\nsecond:\n{second}"
            );
        }
    }
}
