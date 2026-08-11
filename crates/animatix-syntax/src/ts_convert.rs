//! Tree-sitter CST → Animatix AST converter.
//!
//! Walks the tree-sitter concrete syntax tree and produces the [`Stmt`] / [`Expr`]
//! types used by the rest of the pipeline. This enables incremental parsing:
//! tree-sitter can re-parse only the changed region of a file, and this module
//! converts the resulting CST into the same AST the chumsky parser produces.
//!
//! ## Error handling
//!
//! Tree-sitter marks malformed nodes with `is_error()` / `is_missing()`. The
//! converter emits [`Diagnostic`] entries for these nodes and skips them,
//! producing a best-effort AST from the valid portions of the source.

use tree_sitter::{Language, Node, Parser, Tree};

use crate::ast::*;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::parser::top_level::group_scenes;

/// Intermediate representation for items inside a children_block.
/// Used during CST-to-AST conversion to collect inline items before
/// merging properties into their preceding actor.
enum RawItem {
    Item(InlineItem),
    Property(Property),
    Children(Vec<InlineItem>),
}

use std::sync::LazyLock;

static LANGUAGE: LazyLock<Language> = LazyLock::new(tree_sitter_animatix::language);

/// Result of parsing with tree-sitter.
pub struct TsParseResult {
    /// Parsed statements (best-effort — error nodes are skipped).
    pub statements: Vec<Stmt>,
    /// Diagnostics produced during conversion (error nodes, missing fields, etc.).
    pub diagnostics: Vec<Diagnostic>,
    /// The raw tree-sitter tree, kept for incremental re-parsing.
    pub tree: Tree,
}

/// Parse source text using tree-sitter and convert to AST.
///
/// This is the main entry point for tree-sitter based parsing. It parses the
/// full source text, then converts the CST to the same [`Stmt`] type the
/// chumsky parser produces.
///
/// After conversion, statements are post-processed to group them into keyframes
/// (matching the chumsky parser's behavior): statements following a `# time`
/// marker are collected into that keyframe's body, and bare actions/sequences/
/// staggers are wrapped in a default `#0s` keyframe.
pub fn parse_source(source: &str) -> Option<TsParseResult> {
    let mut parser = Parser::new();
    parser.set_language(&LANGUAGE).ok()?;
    let tree = parser.parse(source, None)?;
    let mut converter = TsConverter::new(source);
    let raw_statements = converter.convert_root(tree.root_node());
    let statements = group_scenes(group_keyframes(raw_statements));
    Some(TsParseResult {
        statements,
        diagnostics: converter.diagnostics,
        tree,
    })
}

/// Re-parse with an existing tree (incremental).
///
/// Tree-sitter only re-parses the changed region when given the previous tree.
/// This is the key performance win: for a 1000-line file where one line changed,
/// only that line and its context are re-parsed.
pub fn reparse(source: &str, old_tree: &Tree) -> Option<TsParseResult> {
    let mut parser = Parser::new();
    parser.set_language(&LANGUAGE).ok()?;
    let tree = parser.parse(source, Some(old_tree))?;
    let mut converter = TsConverter::new(source);
    let raw_statements = converter.convert_root(tree.root_node());
    let statements = group_scenes(group_keyframes(raw_statements));
    Some(TsParseResult {
        statements,
        diagnostics: converter.diagnostics,
        tree,
    })
}

/// Post-process flat statements into keyframe-grouped statements.
///
/// The tree-sitter grammar treats `# time` as a standalone marker. This function
/// groups subsequent statements into the preceding keyframe's body, matching the
/// chumsky parser's behavior. Bare actions, sequences, and staggers are wrapped
/// in a default `#0s` keyframe.
fn group_keyframes(stmts: Vec<Stmt>) -> Vec<Stmt> {
    let mut result = Vec::new();
    let mut current_keyframe: Option<Stmt> = None;

    for stmt in stmts {
        match &stmt {
            Stmt::Keyframe { .. } | Stmt::RelativeKeyframe { .. } => {
                // Close the previous keyframe
                if let Some(kf) = current_keyframe.take() {
                    result.push(kf);
                }
                current_keyframe = Some(stmt);
            },
            Stmt::Config { .. }
            | Stmt::Import { .. }
            | Stmt::ComponentDef(..)
            | Stmt::ComponentAction { .. }
            | Stmt::Scene { .. }
            | Stmt::Play { .. }
            | Stmt::Comment(..) => {
                // These are not keyframe body material — emit directly
                if let Some(kf) = current_keyframe.take() {
                    result.push(kf);
                }
                result.push(stmt);
            },
            _ => {
                // Actions, sequences, staggers, actor decls, assignments, etc.
                // go into the current keyframe body
                if let Some(ref mut kf) = current_keyframe {
                    append_to_keyframe_body(kf, stmt);
                } else {
                    // No active keyframe — wrap in a default #0s
                    match &stmt {
                        Stmt::Action(..) | Stmt::Sequence { .. } | Stmt::Stagger { .. } => {
                            current_keyframe = Some(Stmt::Keyframe {
                                time: Time::Seconds(0.0),
                                body: vec![stmt],
                                span: None,
                            });
                        },
                        _ => {
                            result.push(stmt);
                        },
                    }
                }
            },
        }
    }

    // Close the last keyframe
    if let Some(kf) = current_keyframe {
        result.push(kf);
    }

    result
}

/// Append a statement to a keyframe's body.
fn append_to_keyframe_body(kf: &mut Stmt, stmt: Stmt) {
    match kf {
        Stmt::Keyframe { body, .. } | Stmt::RelativeKeyframe { body, .. } => {
            body.push(stmt);
        },
        _ => {},
    }
}

/// Converter state threaded through the tree-sitter CST walk.
struct TsConverter<'a> {
    source: &'a str,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> TsConverter<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            diagnostics: Vec::new(),
        }
    }

    /// Get the text of a node from the source.
    fn node_text(&self, node: Node) -> &str {
        node.utf8_text(self.source.as_bytes()).unwrap_or("")
    }

    /// Push a diagnostic for an error/missing node.
    fn push_error(&mut self, node: Node, message: String) {
        let start = node.start_position();
        let _end = node.end_position();
        let span = node.start_byte()..node.end_byte();
        self.diagnostics.push(
            Diagnostic::error(DiagnosticCode::ParseError, DiagnosticPhase::Parse, message)
                .with_location(start.row + 1, start.column + 1, span),
        );
    }

    /// Convert the root `source_file` node into a list of statements.
    fn convert_root(&mut self, root: Node) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        let mut cursor = root.walk();
        for child in root.named_children(&mut cursor) {
            if child.is_error() || child.is_missing() {
                self.push_error(child, format!("syntax error near '{}'", self.node_text(child)));
                continue;
            }
            if child.kind() == "comment" {
                stmts.push(Stmt::Comment(self.comment_text(child), node_span(child)));
                continue;
            }
            if let Some(stmt) = self.convert_statement(child) {
                stmts.push(stmt);
            }
        }
        stmts
    }

    /// Convert a named node into a statement.
    fn convert_statement(&mut self, node: Node) -> Option<Stmt> {
        match node.kind() {
            "config" => Some(self.convert_config(node)),
            "import_statement" => Some(self.convert_import(node)),
            "let_declaration" => Some(self.convert_let(node)),
            "type_alias" => Some(self.convert_type_alias(node)),
            "component_definition" => Some(self.convert_component_def(node)),
            "action_definition" => Some(self.convert_action_def(node)),
            "scene_declaration" => Some(self.convert_scene_decl(node)),
            "keyframe" => Some(self.convert_keyframe(node)),
            "actor_declaration" => Some(self.convert_actor_decl(node)),
            "text_shorthand" => Some(self.convert_text_shorthand(node)),
            "typst_shorthand" => Some(self.convert_typst_shorthand(node)),
            "property_assignment" => Some(self.convert_assignment(node)),
            "reactive_binding" => Some(self.convert_reactive_binding(node)),
            "action_invocation" => Some(self.convert_action_invocation(node)),
            "sequence_block" => Some(self.convert_sequence(node)),
            "stagger_block" => Some(self.convert_stagger(node)),
            "always_block" => Some(self.convert_always(node)),
            "for_block" => Some(self.convert_for_loop(node)),
            "if_expression" => Some(self.convert_if_stmt(node)),
            "match_expression" => Some(self.convert_match_stmt(node)),
            "play_statement" => Some(self.convert_play(node)),
            "comment" => Some(Stmt::Comment(self.comment_text(node), node_span(node))),
            _ => {
                if node.is_error() || node.is_missing() {
                    self.push_error(node, format!("unexpected '{}'", self.node_text(node)));
                }
                None
            },
        }
    }

    // ── Statement converters ────────────────────────────────────────────

    fn convert_config(&mut self, node: Node) -> Stmt {
        let settings = self.convert_children_properties(node);
        Stmt::Config {
            settings,
            span: node_span(node),
        }
    }

    fn convert_import(&mut self, node: Node) -> Stmt {
        let path = node
            .child_by_field_name("path")
            .map(|n| self.strip_quotes(self.node_text(n)))
            .unwrap_or_default();
        let alias = node.child_by_field_name("alias").map(|n| self.node_text(n).to_string());
        Stmt::Import {
            path,
            alias,
            span: node_span(node),
        }
    }

    fn convert_let(&mut self, node: Node) -> Stmt {
        let is_pub = self.has_child_text(node, "pub");
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let value = node
            .child_by_field_name("value")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        Stmt::LetDecl {
            is_pub,
            name,
            value,
            span: node_span(node),
        }
    }

    fn convert_type_alias(&mut self, node: Node) -> Stmt {
        let is_pub = self.has_child_text(node, "pub");
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let annotation = node
            .child_by_field_name("annotation")
            .map(|n| self.convert_type_annotation(n))
            .unwrap_or(TypeAnnotation::Any);
        Stmt::TypeAlias {
            is_pub,
            name,
            annotation,
            span: node_span(node),
        }
    }

    fn convert_type_annotation(&self, node: Node) -> TypeAnnotation {
        let text = self.node_text(node).to_string();
        if text.is_empty() {
            return TypeAnnotation::Any;
        }
        if let Some(stmts) = crate::parser::parse_source(&format!("type __alias = {text}\n")).0 {
            if let Some(Stmt::TypeAlias { annotation, .. }) = stmts.first() {
                return annotation.clone();
            }
        }
        TypeAnnotation::Alias(text)
    }

    fn convert_component_def(&mut self, node: Node) -> Stmt {
        let is_pub = self.has_child_text(node, "pub");
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let params = self.convert_parameter_list(node);
        let body = self.convert_block_body(node);
        Stmt::ComponentDef(
            ComponentDef {
                is_pub,
                name,
                params,
                body,
            },
            node_span(node),
        )
    }

    fn convert_action_def(&mut self, node: Node) -> Stmt {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let params = self.convert_parameter_list(node);
        let body = self.convert_block_body(node);
        Stmt::ComponentAction {
            name,
            params,
            body,
            span: node_span(node),
        }
    }

    fn convert_scene_decl(&mut self, node: Node) -> Stmt {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        Stmt::Scene {
            name,
            config: Vec::new(),
            body: Vec::new(),
            span: node_span(node),
        }
    }

    fn convert_keyframe(&mut self, node: Node) -> Stmt {
        let text = self.node_text(node);
        // Parse "# 2s" or "#+1s" — the grammar has `#` optional `+` time_literal/number
        let is_relative = text.contains('+');
        let time_str = text.trim_start_matches('#').trim_start_matches('+').trim();
        let time = parse_time_from_text(time_str);

        // Keyframe body is in the next block (the grammar doesn't include it in the keyframe node).
        // Actually, looking at the grammar, keyframe is just `# time` — the body follows as
        // sibling statements. We need to collect subsequent statements until the next keyframe
        // or end of scope.
        //
        // BUT: looking at the chumsky parser, keyframes DO have bodies enclosed in { }.
        // The tree-sitter grammar defines keyframe as just `# time` without a block.
        // This means the tree-sitter grammar treats the keyframe time as a standalone node,
        // and the body statements are siblings.
        //
        // For now, return an empty body — the body will be collected by the parent scope.
        let body = Vec::new();

        if is_relative {
            Stmt::RelativeKeyframe {
                offset: time,
                body,
                span: node_span(node),
            }
        } else {
            Stmt::Keyframe {
                time,
                body,
                span: node_span(node),
            }
        }
    }

    fn convert_actor_decl(&mut self, node: Node) -> Stmt {
        let label = node
            .child_by_field_name("label")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let ty = node
            .child_by_field_name("type")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let array_index =
            node.child_by_field_name("array_index").and_then(|n| self.convert_expr(n));
        let props = self.convert_children_properties(node);
        let modifiers = self.convert_modifier_block_node(node);
        let children = self.convert_children_block_items(node);
        Stmt::ActorDecl {
            is_pub: self.has_child_text(node, "pub"),
            is_anonymous: false,
            label,
            array_index,
            ty,
            props,
            modifiers,
            children,
            span: node_span(node),
        }
    }

    /// Convert `label: "text"` shorthand into a `Text` actor declaration.
    fn convert_text_shorthand(&mut self, node: Node) -> Stmt {
        let label = node
            .child_by_field_name("label")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let text = node
            .child_by_field_name("text")
            .map(|n| self.strip_quotes(self.node_text(n)).to_string())
            .unwrap_or_default();
        let modifiers = self.convert_modifier_block_node(node);
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label,
            array_index: None,
            ty: "Text".to_string(),
            props: vec![Property {
                name: "text".to_string(),
                value: Expr::Str(text),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers,
            children: vec![],
            span: node_span(node),
        }
    }

    /// Convert `label: $$content$$` shorthand into a `Typst` actor declaration.
    fn convert_typst_shorthand(&mut self, node: Node) -> Stmt {
        let label = node
            .child_by_field_name("label")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let content = node
            .child_by_field_name("content")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_else(|| {
                let raw = self.node_text(node);
                let start = raw.find("$$").map(|i| i + 2).unwrap_or(0);
                let end = raw.rfind("$$").unwrap_or(raw.len());
                raw[start..end.max(start)].to_string()
            });
        let modifiers = self.convert_modifier_block_node(node);
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label,
            array_index: None,
            ty: "Typst".to_string(),
            props: vec![Property {
                name: "content".to_string(),
                value: Expr::Str(content),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers,
            children: vec![],
            span: node_span(node),
        }
    }

    fn convert_assignment(&mut self, node: Node) -> Stmt {
        let target_node = node.child_by_field_name("target");
        let target = target_node.map(|n| self.convert_target_segments(n)).unwrap_or_default();
        let (target, property) = self.split_target_property(target);
        let value = node
            .child_by_field_name("value")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        let value_span = node.child_by_field_name("value").map(|n| node_byte_span(n));
        let modifiers = self.convert_modifier_block_node(node);
        let mut modifiers = modifiers;
        let easing = crate::parser::common::extract_easing(&mut modifiers);
        Stmt::Assignment {
            target,
            property,
            value,
            modifiers,
            easing,
            value_span,
            span: node_span(node),
        }
    }

    fn convert_reactive_binding(&mut self, node: Node) -> Stmt {
        let target_node = node.child_by_field_name("target");
        let target = target_node.map(|n| self.convert_target_segments(n)).unwrap_or_default();
        let (target, property) = self.split_target_property(target);
        let value = node
            .child_by_field_name("value")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        let value_span = node.child_by_field_name("value").map(|n| node_byte_span(n));
        Stmt::ReactiveBinding {
            target,
            property,
            value,
            value_span,
            span: node_span(node),
        }
    }

    fn split_target_property(
        &self,
        mut target: Vec<TargetSegment>,
    ) -> (Vec<TargetSegment>, String) {
        let Some(last) = target.pop() else {
            return (target, String::new());
        };
        let property = match last {
            TargetSegment::Static(s) => s,
            TargetSegment::Indexed { .. } => String::new(),
        };
        (target, property)
    }

    fn convert_action_invocation(&mut self, node: Node) -> Stmt {
        let verb = node
            .child_by_field_name("verb")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let targets = self.convert_target_list(node);
        let modifiers = self.convert_modifier_block_node(node);
        let args = self.convert_action_args(node);
        Stmt::Action(
            Action {
                verb,
                targets,
                args,
                modifiers,
                byte_span: Some(node_byte_span(node)),
            },
            node_span(node),
        )
    }

    /// Convert positional action arguments. The grammar currently keeps the
    /// existing comma-separated named-modifier surface, so positional args stay
    /// empty until an explicit grammar field is added.
    fn convert_action_args(&mut self, _node: Node) -> Vec<Expr> {
        Vec::new()
    }

    fn convert_sequence(&mut self, node: Node) -> Stmt {
        let body = self.convert_block_body(node);
        Stmt::Sequence {
            body,
            span: node_span(node),
        }
    }

    fn convert_stagger(&mut self, node: Node) -> Stmt {
        // The grammar places the shared interval before the block as a bare
        // expression (e.g. `stagger [150ms]`). Convert that into the same
        // positional modifier shape Chumsky produces, then append any
        // modifier_block children (named modifiers).
        let mut modifiers = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "block" || child.kind() == "comment" {
                continue;
            }
            modifiers.push(Modifier {
                name: None,
                value: self.convert_modifier_value(child),
            });
        }
        modifiers.extend(self.convert_modifier_block_node(node));
        let body = self.convert_block_body(node);
        Stmt::Stagger {
            modifiers,
            body,
            span: node_span(node),
        }
    }

    fn convert_always(&mut self, node: Node) -> Stmt {
        let body = self.convert_block_body(node);
        Stmt::Always {
            body,
            span: node_span(node),
        }
    }

    fn convert_for_loop(&mut self, node: Node) -> Stmt {
        let mut variables = Vec::new();
        let mut cursor = node.walk();
        for child in node.children_by_field_name("variable", &mut cursor) {
            if child.kind() == "identifier" {
                variables.push(self.node_text(child).to_string());
            }
        }
        let var = match variables.as_slice() {
            [] => LoopPattern::Single(String::new()),
            [name] => LoopPattern::Single(name.clone()),
            _ => LoopPattern::Tuple(variables),
        };
        let index_var = node
            .child_by_field_name("index_variable")
            .map(|n| self.node_text(n).to_string());
        let iterable = node
            .child_by_field_name("iterable")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        let body = self.convert_block_body(node);
        Stmt::ForLoop {
            var,
            index_var,
            iterable,
            body,
            span: node_span(node),
        }
    }

    fn convert_if_stmt(&mut self, node: Node) -> Stmt {
        let condition = node
            .child_by_field_name("condition")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        let then_branch = node
            .child_by_field_name("consequence")
            .map(|n| self.convert_block_or_expr_stmts(n))
            .unwrap_or_default();
        let else_branch = node
            .child_by_field_name("alternative")
            .map(|n| self.convert_block_or_expr_stmts(n));
        Stmt::Conditional {
            condition,
            then_branch,
            else_branch,
            span: node_span(node),
        }
    }

    fn convert_match_stmt(&mut self, node: Node) -> Stmt {
        let scrutinee = node
            .child_by_field_name("scrutinee")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        let arms = self.convert_match_arms(node);
        Stmt::Match {
            scrutinee,
            arms,
            span: node_span(node),
        }
    }

    fn convert_match_expr(&mut self, node: Node) -> Option<Expr> {
        let scrutinee = node
            .child_by_field_name("scrutinee")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        // Expression form: each arm's value is an expression (not a block).
        let mut arms = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "match_arm" => {
                    let pattern = self.convert_match_pattern(child);
                    let value_node = child.child_by_field_name("value");
                    if let Some(vn) = value_node {
                        if let Some(expr) = self.convert_expr(vn) {
                            arms.push((pattern, Box::new(expr)));
                        }
                    }
                },
                "match_statement_arm" => {
                    self.push_error(
                        child,
                        "block-valued match arms are only valid in match statements".to_string(),
                    );
                },
                _ => {},
            }
        }
        Some(Expr::Match(Box::new(scrutinee), arms))
    }

    fn convert_match_arms(&mut self, node: Node) -> Vec<(MatchPattern, Vec<Stmt>)> {
        let mut arms = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "match_statement_arm" => {
                    let pattern = self.convert_match_pattern(child);
                    let value_node = child.child_by_field_name("value");
                    let stmts = value_node
                        .map(|vn| self.convert_block_or_expr_stmts(vn))
                        .unwrap_or_default();
                    arms.push((pattern, stmts));
                },
                "match_arm" => {
                    self.push_error(
                        child,
                        "expression-valued match arms are only valid in match expressions"
                            .to_string(),
                    );
                },
                _ => {},
            }
        }
        arms
    }

    fn convert_match_pattern(&mut self, node: Node) -> MatchPattern {
        // A match_arm node contains a child match_pattern node. That node may
        // wrap the actual pattern (match_literal / match_range / match_or /
        // match_tuple / match_wildcard), so recurse through wrapper nodes.
        let mut current = node.child_by_field_name("pattern").unwrap_or(node);
        loop {
            match current.kind() {
                "match_pattern" | "match_literal" | "match_range" | "match_or"
                | "match_tuple" | "match_wildcard" => break,
                _ => {
                    let mut cursor = current.walk();
                    match current.named_children(&mut cursor).next() {
                        Some(child) => current = child,
                        None => return MatchPattern::Wildcard,
                    }
                },
            }
        }
        self.convert_single_match_pattern(current)
    }

    fn convert_single_match_pattern(&mut self, node: Node) -> MatchPattern {
        match node.kind() {
            "match_pattern" => {
                let mut cursor = node.walk();
                match node.named_children(&mut cursor).next() {
                    Some(child) => self.convert_single_match_pattern(child),
                    None => MatchPattern::Wildcard,
                }
            },
            "match_wildcard" => MatchPattern::Wildcard,
            "match_literal" => {
                let mut cursor = node.walk();
                if let Some(child) = node.named_children(&mut cursor).next() {
                    return self.convert_match_literal(child);
                }
                MatchPattern::Wildcard
            },
            "match_range" => {
                let low = node.child_by_field_name("low");
                let high = node.child_by_field_name("high");
                match (low, high) {
                    (Some(lo), Some(hi)) => {
                        let lo_pat = self.convert_match_literal(lo);
                        let hi_pat = self.convert_match_literal(hi);
                        MatchPattern::Range(Box::new(lo_pat), Box::new(hi_pat))
                    },
                    _ => MatchPattern::Wildcard,
                }
            },
            "match_or" => {
                let mut pats = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    pats.push(self.convert_single_match_pattern(child));
                }
                MatchPattern::Or(pats)
            },
            "match_tuple" => {
                let mut pats = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    pats.push(self.convert_single_match_pattern(child));
                }
                MatchPattern::Tuple(pats)
            },
            _ => {
                // Fallback: try to recurse into named children
                let mut cursor = node.walk();
                if let Some(child) = node.named_children(&mut cursor).next() {
                    return self.convert_single_match_pattern(child);
                }
                MatchPattern::Wildcard
            },
        }
    }

    fn convert_match_literal(&mut self, node: Node) -> MatchPattern {
        match node.kind() {
            "number" | "match_num" => {
                let text = self.node_text(node);
                MatchPattern::Num(text.parse::<f64>().unwrap_or(0.0))
            },
            "string" => MatchPattern::Str(self.strip_quotes(self.node_text(node)).to_string()),
            "boolean" => MatchPattern::Bool(self.node_text(node) == "true"),
            _ => MatchPattern::Wildcard,
        }
    }

    fn convert_play(&mut self, node: Node) -> Stmt {
        let scene_name = node
            .child_by_field_name("scene")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let modifiers = self.convert_modifier_block_node(node);
        let transition = crate::parser::top_level::parse_transition_from_modifiers(&modifiers);
        Stmt::Play {
            scene_name,
            transition,
            span: node_span(node),
        }
    }

    // ── Expression converters ───────────────────────────────────────────

    /// Convert a tree-sitter node into an AST expression.
    fn convert_expr(&mut self, node: Node) -> Option<Expr> {
        match node.kind() {
            "number" => {
                let text = self.node_text(node);
                text.parse::<f64>().ok().map(Expr::Num)
            },
            "percentage" => {
                // percentage child is a number node
                let num_node = node.child(0);
                let text = num_node.map(|n| self.node_text(n)).unwrap_or("0");
                text.parse::<f64>().ok().map(Expr::Percent)
            },
            "time_literal" => {
                // Time literals like "2s" or "500ms" — convert to seconds as a number
                let text = self.node_text(node);
                let time = parse_time_from_text(text);
                match time {
                    Time::Seconds(s) => Some(Expr::Num(s)),
                    Time::Milliseconds(ms) => Some(Expr::Num(ms as f64 / 1000.0)),
                }
            },
            "string" => Some(Expr::Str(self.strip_quotes(self.node_text(node)).to_string())),
            "boolean" => Some(Expr::Bool(self.node_text(node) == "true")),
            "null_literal" => Some(Expr::Null),
            "identifier" => Some(Expr::Ident(self.node_text(node).to_string())),
            "path_expression" => Some(self.convert_path_expr(node)),
            "unary_expression" => self.convert_unary(node),
            "binary_expression" => self.convert_binary(node),
            "call_expression" => self.convert_call(node),
            "method_call_expression" => self.convert_method_call(node),
            "index_expression" => self.convert_index(node),
            "tuple_expression" => self.convert_tuple(node),
            "array_expression" => self.convert_array(node),
            "set_expression" => self.convert_set(node),
            "closure_expression" => self.convert_closure(node),
            "object_expression" => self.convert_object(node),
            "parenthesized_expression" => {
                // Return the inner expression
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if let Some(expr) = self.convert_expr(child) {
                        return Some(expr);
                    }
                }
                None
            },
            "if_expression" => {
                // If expression used as an expression (not statement)
                let condition = node
                    .child_by_field_name("condition")
                    .and_then(|n| self.convert_expr(n))
                    .unwrap_or(Expr::Null);
                let then_expr = node
                    .child_by_field_name("consequence")
                    .and_then(|n| self.convert_block_or_expr_expr(n))
                    .unwrap_or(Expr::Null);
                let else_expr = node
                    .child_by_field_name("alternative")
                    .and_then(|n| self.convert_block_or_expr_expr(n))
                    .unwrap_or(Expr::Null);
                Some(Expr::Conditional(
                    Box::new(condition),
                    Box::new(then_expr),
                    Box::new(else_expr),
                ))
            },
            "match_expression" => self.convert_match_expr(node),
            _ => {
                if node.is_error() {
                    self.push_error(
                        node,
                        format!("unexpected expression '{}'", self.node_text(node)),
                    );
                }
                None
            },
        }
    }

    fn convert_path_expr(&mut self, node: Node) -> Expr {
        let segments = self.convert_path_segments(node);
        match segments.as_slice() {
            [segment] => Expr::Ident(segment.clone()),
            _ => Expr::Path(segments),
        }
    }

    fn convert_unary(&mut self, node: Node) -> Option<Expr> {
        let op_text =
            node.child_by_field_name("operator").map(|n| self.node_text(n)).unwrap_or("-");
        let op = match op_text {
            "-" => UnaryOp::Neg,
            "!" => UnaryOp::Not,
            _ => UnaryOp::Neg,
        };
        let operand = node
            .child_by_field_name("operand")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Num(0.0));
        Some(Expr::Unary(op, Box::new(operand)))
    }

    fn convert_binary(&mut self, node: Node) -> Option<Expr> {
        // binary_expression has children: left_expr, operator, right_expr
        let mut children: Vec<Node> = Vec::new();
        let mut op_text = "+";
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            children.push(child);
        }
        // Also collect anonymous children for the operator
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                if !child.is_named() {
                    let text = self.node_text(child);
                    if matches!(
                        text,
                        "+" | "-"
                            | "*"
                            | "/"
                            | "%"
                            | "^"
                            | "=="
                            | "!="
                            | "<"
                            | ">"
                            | "<="
                            | ">="
                            | "&&"
                            | "||"
                    ) {
                        op_text = text;
                    }
                }
            }
        }

        let op = match op_text {
            "+" => BinaryOp::Add,
            "-" => BinaryOp::Sub,
            "*" => BinaryOp::Mul,
            "/" => BinaryOp::Div,
            "%" => BinaryOp::Mod,
            "^" => BinaryOp::Pow,
            "==" => BinaryOp::Eq,
            "!=" => BinaryOp::Neq,
            "<" => BinaryOp::Lt,
            ">" => BinaryOp::Gt,
            "<=" => BinaryOp::Lte,
            ">=" => BinaryOp::Gte,
            "&&" => BinaryOp::And,
            "||" => BinaryOp::Or,
            _ => BinaryOp::Add,
        };

        if children.len() >= 2 {
            let lhs = self.convert_expr(children[0]).unwrap_or(Expr::Num(0.0));
            let rhs = self.convert_expr(children[1]).unwrap_or(Expr::Num(0.0));
            Some(Expr::Binary(Box::new(lhs), op, Box::new(rhs)))
        } else {
            None
        }
    }

    fn convert_call(&mut self, node: Node) -> Option<Expr> {
        let func = node
            .child_by_field_name("function")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let args = self.convert_argument_list(node);
        Some(Expr::Call(func, args))
    }

    fn convert_method_call(&mut self, node: Node) -> Option<Expr> {
        let object = node
            .child_by_field_name("object")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        let method = node
            .child_by_field_name("method")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let args = self.convert_argument_list(node);
        Some(Expr::Method(Box::new(object), method, args))
    }

    fn convert_index(&mut self, node: Node) -> Option<Expr> {
        let object = node
            .child_by_field_name("object")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        let index = node
            .child_by_field_name("index")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Num(0.0));
        Some(Expr::Index(Box::new(object), Box::new(index)))
    }

    fn convert_tuple(&mut self, node: Node) -> Option<Expr> {
        let items = self.collect_named_exprs(node);
        match items.as_slice() {
            [] => Some(Expr::Tuple(Vec::new())),
            // The PEG parser treats `(expr)` as a parenthesized expression.
            [item] => Some(item.clone()),
            _ => Some(Expr::Tuple(items)),
        }
    }

    fn convert_array(&mut self, node: Node) -> Option<Expr> {
        let items = self.collect_named_exprs(node);
        Some(Expr::Tuple(items))
    }

    fn convert_set(&mut self, node: Node) -> Option<Expr> {
        let items = self.collect_named_exprs(node);
        Some(Expr::List(items))
    }

    fn convert_closure(&mut self, node: Node) -> Option<Expr> {
        let mut params = Vec::new();
        let mut body_expr = None;
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            match child.kind() {
                "identifier" => params.push(self.node_text(child).to_string()),
                _ => {
                    if body_expr.is_none() {
                        body_expr = self.convert_expr(child);
                    }
                },
            }
        }
        let body = Box::new(body_expr.unwrap_or(Expr::Null));
        Some(Expr::Closure(params, body))
    }

    fn convert_object(&mut self, node: Node) -> Option<Expr> {
        let type_name = node
            .child_by_field_name("type")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let props = self.convert_children_properties(node);
        Some(Expr::Construct(type_name, props))
    }

    // ── Helpers ─────────────────────────────────────────────────────────

    /// Collect all named expression children of a node.
    fn collect_named_exprs(&mut self, node: Node) -> Vec<Expr> {
        let mut exprs = Vec::new();
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            // Skip non-expression named children (like "comment")
            if child.kind() == "comment" {
                continue;
            }
            if let Some(expr) = self.convert_expr(child) {
                exprs.push(expr);
            }
        }
        exprs
    }

    /// Convert a `property_list` node's children into `Vec<Property>`.
    fn convert_children_properties(&mut self, parent: Node) -> Vec<Property> {
        let mut props = Vec::new();
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            match child.kind() {
                "property_list" => {
                    let mut inner_cursor = child.walk();
                    for prop_node in child.named_children(&mut inner_cursor) {
                        if prop_node.kind() == "property" {
                            props.push(self.convert_property(prop_node));
                        }
                    }
                },
                "property" => {
                    props.push(self.convert_property(child));
                },
                _ => {},
            }
        }
        props
    }

    fn convert_property(&mut self, node: Node) -> Property {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.strip_quotes(self.node_text(n)).to_string())
            .unwrap_or_default();
        let value = node
            .child_by_field_name("value")
            .and_then(|n| self.convert_expr(n))
            .unwrap_or(Expr::Null);
        let value_span = node.child_by_field_name("value").map(|n| node_byte_span(n));
        Property {
            name,
            value,
            value_span,
            trailing_comment: None,
        }
    }

    /// Convert a `parameter_list` node's children into `Vec<ParamDef>`.
    fn convert_parameter_list(&mut self, parent: Node) -> Vec<ParamDef> {
        let mut params = Vec::new();
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            if child.kind() == "parameter_list" {
                let mut inner_cursor = child.walk();
                for param_node in child.named_children(&mut inner_cursor) {
                    if param_node.kind() == "parameter" {
                        params.push(self.convert_parameter(param_node));
                    }
                }
            }
        }
        params
    }

    fn convert_parameter(&mut self, node: Node) -> ParamDef {
        let name = node
            .child_by_field_name("name")
            .map(|n| self.node_text(n).to_string())
            .unwrap_or_default();
        let type_node = node.child_by_field_name("type");
        let default_node = node.child_by_field_name("default");
        let param_type = if let Some(type_node) = type_node {
            Some(self.convert_type_annotation(type_node))
        } else if let Some(default_node) = default_node
            && self.is_dotted_type_alias(default_node)
        {
            Some(TypeAnnotation::Alias(self.node_text(default_node).replace('.', "::")))
        } else {
            None
        };
        let default = default_node.and_then(|n| self.convert_expr(n));
        ParamDef {
            name,
            param_type,
            default,
        }
    }

    fn is_dotted_type_alias(&self, node: Node) -> bool {
        if node.kind() != "path_expression" {
            return false;
        }
        let text = self.node_text(node);
        text.contains('.')
            && text
                .rsplit('.')
                .next()
                .is_some_and(|last| last.chars().next().is_some_and(|c| c.is_uppercase()))
    }

    /// Normalize tree-sitter comment text to the PEG convention (text after `//`).
    fn comment_text(&self, node: Node) -> String {
        self.node_text(node).trim_start_matches("//").to_string()
    }

    /// Convert a modifier_block node's modifiers into `Vec<Modifier>`.
    fn convert_modifier_block_node(&mut self, parent: Node) -> Vec<Modifier> {
        let mut modifiers = Vec::new();
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            match child.kind() {
                "modifier_block" => {
                    let mut inner_cursor = child.walk();
                    for mod_child in child.named_children(&mut inner_cursor) {
                        if mod_child.kind() == "modifier_list" {
                            let mut list_cursor = mod_child.walk();
                            for mod_node in mod_child.named_children(&mut list_cursor) {
                                if mod_node.kind() == "modifier" {
                                    modifiers.push(self.convert_modifier(mod_node));
                                }
                            }
                        }
                    }
                },
                "modifier_list" => {
                    let mut list_cursor = child.walk();
                    for mod_node in child.named_children(&mut list_cursor) {
                        if mod_node.kind() == "modifier" {
                            modifiers.push(self.convert_modifier(mod_node));
                        }
                    }
                },
                _ => {},
            }
        }
        modifiers
    }

    fn convert_modifier(&mut self, node: Node) -> Modifier {
        let name = node.child_by_field_name("key").map(|n| self.node_text(n).to_string());
        let value = if let Some(value_node) = node.child_by_field_name("value") {
            self.convert_modifier_value(value_node)
        } else {
            // Positional modifiers have no named value field; the first named
            // child is the value (identifier, time literal, number, etc.).
            let mut cursor = node.walk();
            node.named_children(&mut cursor)
                .next()
                .map(|child| self.convert_modifier_value(child))
                .unwrap_or(Expr::Num(0.0))
        };
        Modifier { name, value }
    }

    /// Convert a modifier value. Time literals keep their source form so the
    /// runtime can parse duration shorthand exactly like the PEG parser does.
    fn convert_modifier_value(&mut self, node: Node) -> Expr {
        if node.kind() == "time_literal" {
            Expr::Ident(self.node_text(node).to_string())
        } else {
            self.convert_expr(node).unwrap_or(Expr::Num(0.0))
        }
    }

    /// Convert a target_list node into `Vec<String>`.
    fn convert_target_list(&mut self, parent: Node) -> Vec<String> {
        let mut targets = Vec::new();
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            if child.kind() == "target_list" {
                let mut inner_cursor = child.walk();
                for target_node in child.named_children(&mut inner_cursor) {
                    targets.push(self.convert_target_path_string(target_node));
                }
            }
        }
        if targets.is_empty() {
            // Try direct identifier children (for single target)
            let mut cursor = parent.walk();
            for child in parent.named_children(&mut cursor) {
                if child.kind() == "identifier" {
                    // Only add if it's the verb's target, not the verb itself
                    // The verb is in the "verb" field, so named children that are
                    // identifiers but not the verb are targets
                    if parent.child_by_field_name("verb").map(|v| v.id()) != Some(child.id()) {
                        targets.push(self.node_text(child).to_string());
                    }
                }
            }
        }
        targets
    }

    fn convert_target_path_string(&self, node: Node) -> String {
        let mut segments = Vec::new();
        self.collect_target_path_string(node, &mut segments);
        segments.join(".")
    }

    fn collect_target_path_string(&self, node: Node, segments: &mut Vec<String>) {
        match node.kind() {
            "identifier" => segments.push(self.node_text(node).to_string()),
            "path_expression" | "target_path" | "indexed_target_path" => {
                if let Some(base) = node.child_by_field_name("base") {
                    self.collect_target_path_string(base, segments);
                }
                if let Some(index) = node.child_by_field_name("index") {
                    let base = segments.pop().unwrap_or_default();
                    let index_text = self.index_literal_text(index);
                    segments.push(format!("{base}__{index_text}"));
                }
                if let Some(name) = node.child_by_field_name("name") {
                    self.collect_target_path_string(name, segments);
                }
            },
            "index_expression" => {
                if let Some(object) = node.child_by_field_name("object") {
                    self.collect_target_path_string(object, segments);
                }
                if let Some(index) = node.child_by_field_name("index") {
                    let base = segments.pop().unwrap_or_default();
                    let index_text = self.index_literal_text(index);
                    segments.push(format!("{base}__{index_text}"));
                }
            },
            _ => {},
        }
    }

    fn index_number_text(&self, node: Node) -> Option<String> {
        if node.kind() == "number" {
            return Some(self.node_text(node).to_string());
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if child.kind() == "number" {
                return Some(self.node_text(child).to_string());
            }
        }
        None
    }

    fn index_literal_text(&self, node: Node) -> String {
        if node.kind() == "number" || node.kind() == "identifier" {
            return self.node_text(node).to_string();
        }
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            let text = self.index_literal_text(child);
            if !text.is_empty() {
                return text;
            }
        }
        String::new()
    }

    /// Convert the body of a block node into `Vec<Stmt>`.
    fn convert_block_body(&mut self, parent: Node) -> Vec<Stmt> {
        let mut stmts = Vec::new();
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            match child.kind() {
                "block" | "children_block" => {
                    let mut inner_cursor = child.walk();
                    for stmt_node in child.named_children(&mut inner_cursor) {
                        if stmt_node.is_error() || stmt_node.is_missing() {
                            self.push_error(
                                stmt_node,
                                format!("syntax error near '{}'", self.node_text(stmt_node)),
                            );
                            continue;
                        }
                        if stmt_node.kind() == "comment" {
                            stmts.push(Stmt::Comment(
                                self.comment_text(stmt_node),
                                node_span(stmt_node),
                            ));
                            continue;
                        }
                        if let Some(stmt) = self.convert_statement(stmt_node) {
                            stmts.push(stmt);
                        }
                    }
                },
                _ => {},
            }
        }
        stmts
    }

    /// Convert a block or expression_block into statements (for if/else branches).
    fn convert_block_or_expr_stmts(&mut self, node: Node) -> Vec<Stmt> {
        match node.kind() {
            "block" => {
                let mut stmts = Vec::new();
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if child.is_error() || child.is_missing() {
                        self.push_error(
                            child,
                            format!("syntax error near '{}'", self.node_text(child)),
                        );
                        continue;
                    }
                    if let Some(stmt) = self.convert_statement(child) {
                        stmts.push(stmt);
                    }
                }
                stmts
            },
            "expression_block" => {
                // expression_block is `{ expr }` — wrap in a synthetic statement
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if let Some(expr) = self.convert_expr(child) {
                        // Wrap expression in a let declaration as a statement
                        return vec![Stmt::LetDecl {
                            is_pub: false,
                            name: "_".to_string(),
                            value: expr,
                            span: node_span(node),
                        }];
                    }
                }
                Vec::new()
            },
            _ => Vec::new(),
        }
    }

    /// Convert a block or expression_block into an expression (for if expressions).
    fn convert_block_or_expr_expr(&mut self, node: Node) -> Option<Expr> {
        match node.kind() {
            "block" => {
                // Try to find a single expression in the block
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if let Some(expr) = self.convert_expr(child) {
                        return Some(expr);
                    }
                }
                None
            },
            "expression_block" => {
                let mut cursor = node.walk();
                for child in node.named_children(&mut cursor) {
                    if let Some(expr) = self.convert_expr(child) {
                        return Some(expr);
                    }
                }
                None
            },
            _ => self.convert_expr(node),
        }
    }

    /// Convert a path node (path_expression) into `Vec<String>` segments.
    fn convert_path_segments(&self, node: Node) -> Vec<String> {
        let mut segments = Vec::new();
        self.collect_path_segments(node, &mut segments);
        segments
    }

    fn convert_target_segments(&mut self, node: Node) -> Vec<TargetSegment> {
        let mut segments = Vec::new();
        self.collect_target_segments(node, &mut segments);
        segments
    }

    fn collect_target_segments(&mut self, node: Node, segments: &mut Vec<TargetSegment>) {
        match node.kind() {
            "identifier" => {
                segments.push(TargetSegment::Static(self.node_text(node).to_string()));
            },
            "path_expression" | "target_path" | "indexed_target_path" => {
                if let Some(base) = node.child_by_field_name("base") {
                    self.collect_target_segments(base, segments);
                }
                if let Some(index) = node.child_by_field_name("index") {
                    let base = match segments.pop() {
                        Some(TargetSegment::Static(label)) => label,
                        _ => String::new(),
                    };
                    if let Some(number) = self.index_number_text(index) {
                        segments.push(TargetSegment::Static(format!("{base}__{number}")));
                    } else if let Some(index_expr) = self.convert_index_value_expr(index) {
                        segments.push(TargetSegment::Indexed {
                            base,
                            index: Box::new(index_expr),
                        });
                    }
                }
                if let Some(name) = node.child_by_field_name("name") {
                    self.collect_target_segments(name, segments);
                }
            },
            _ => {},
        }
    }

    fn convert_index_value_expr(&mut self, node: Node) -> Option<Expr> {
        let mut cursor = node.walk();
        for child in node.named_children(&mut cursor) {
            if let Some(expr) = self.convert_expr(child) {
                return Some(expr);
            }
        }
        None
    }

    fn collect_path_segments(&self, node: Node, segments: &mut Vec<String>) {
        match node.kind() {
            "path_expression" => {
                if let Some(base) = node.child_by_field_name("base") {
                    self.collect_path_segments(base, segments);
                }
                if let Some(name) = node.child_by_field_name("name") {
                    segments.push(self.node_text(name).to_string());
                }
            },
            "identifier" => {
                segments.push(self.node_text(node).to_string());
            },
            _ => {},
        }
    }

    /// Convert an argument_list node into `Vec<Expr>`.
    fn convert_argument_list(&mut self, parent: Node) -> Vec<Expr> {
        let mut args = Vec::new();
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            if child.kind() == "argument_list" {
                let mut inner_cursor = child.walk();
                for arg_node in child.named_children(&mut inner_cursor) {
                    if let Some(expr) = self.convert_expr(arg_node) {
                        args.push(expr);
                    }
                }
            }
        }
        args
    }

    /// Convert children_block items into `Vec<InlineItem>`.
    ///
    /// With the new inline grammar, children_block contains an `inline_items`
    /// node (if non-empty). Each `inline_items` node contains one or more
    /// `inline_item` nodes. Each `inline_item` wraps a single variant node
    /// (inline_actor_declaration, inline_property, etc.).
    ///
    /// Properties and standalone children blocks are post-hoc attached to the
    /// preceding actor (matching the PEG parser's FlatItem merge logic).
    fn convert_children_block_items(&mut self, parent: Node) -> Vec<InlineItem> {
        let mut raw: Vec<RawItem> = Vec::new();
        let mut cursor = parent.walk();
        for child in parent.named_children(&mut cursor) {
            match child.kind() {
                "children_block" | "inline_children_block" => {
                    // Unwrap the optional inline_items layer
                    let mut ic = child.walk();
                    for inline_items_node in child.named_children(&mut ic) {
                        if inline_items_node.kind() == "inline_items" {
                            let mut iic = inline_items_node.walk();
                            for item_node in inline_items_node.named_children(&mut iic) {
                                if item_node.kind() == "inline_item" {
                                    if let Some(raw_item) =
                                        self.collect_inline_item_variant(item_node)
                                    {
                                        raw.push(raw_item);
                                    }
                                }
                            }
                        }
                    }
                },
                "slot_marker" => raw.push(RawItem::Item(InlineItem::SlotMarker)),
                "slot_fill" => {
                    let slot_name = child
                        .child_by_field_name("name")
                        .map(|n| self.node_text(n).to_string())
                        .unwrap_or_default();
                    let fill_items = self.convert_children_block_items(child);
                    raw.push(RawItem::Item(InlineItem::SlotFill {
                        slot: slot_name,
                        items: fill_items,
                    }));
                },
                _ => {},
            }
        }

        // Post-process: attach properties / standalone children to the preceding actor.
        let mut result = Vec::new();
        for item in raw {
            match item {
                RawItem::Item(i) => result.push(i),
                RawItem::Property(p) => {
                    if let Some(
                        InlineItem::Labeled { props, .. } | InlineItem::Anonymous { props, .. },
                    ) = result.last_mut()
                    {
                        props.push(p);
                    }
                },
                RawItem::Children(c) => {
                    if let Some(
                        InlineItem::Labeled { children, .. }
                        | InlineItem::Anonymous { children, .. },
                    ) = result.last_mut()
                    {
                        *children = c;
                    }
                },
            }
        }
        result
    }

    /// Extract the single variant from an `inline_item` wrapper node.
    fn collect_inline_item_variant(&mut self, node: Node) -> Option<RawItem> {
        let mut cursor = node.walk();
        node.named_children(&mut cursor)
            .next()
            .and_then(|variant_node| self.convert_inline_item_variant_to_raw(variant_node))
    }

    /// Convert a single inline variant node to a `RawItem`.
    fn convert_inline_item_variant_to_raw(&mut self, node: Node) -> Option<RawItem> {
        match node.kind() {
            "inline_actor_declaration" => {
                let label = node
                    .child_by_field_name("label")
                    .map(|n| self.node_text(n).to_string())
                    .unwrap_or_default();
                let ty = node
                    .child_by_field_name("type")
                    .map(|n| self.node_text(n).to_string())
                    .unwrap_or_default();
                let array_index =
                    node.child_by_field_name("array_index").and_then(|n| self.convert_expr(n));
                let modifiers = self.convert_modifier_block_node(node);
                let children = self.convert_children_block_items(node);
                Some(RawItem::Item(InlineItem::Labeled {
                    label,
                    array_index,
                    ty,
                    props: Vec::new(),
                    modifiers,
                    children,
                }))
            },
            "inline_anonymous_actor" => {
                let ty = node
                    .child_by_field_name("type")
                    .map(|n| self.strip_quotes(self.node_text(n)))
                    .unwrap_or_default();
                let modifiers = self.convert_modifier_block_node(node);
                let children = self.convert_children_block_items(node);
                Some(RawItem::Item(InlineItem::Anonymous {
                    ty,
                    props: Vec::new(),
                    modifiers,
                    children,
                }))
            },
            "inline_property" => Some(RawItem::Property(self.convert_property(node))),
            "inline_for_loop" => {
                let mut variables = Vec::new();
                let mut cursor = node.walk();
                for child in node.children_by_field_name("variable", &mut cursor) {
                    if child.kind() == "identifier" {
                        variables.push(self.node_text(child).to_string());
                    }
                }
                let var = match variables.as_slice() {
                    [] => LoopPattern::Single(String::new()),
                    [name] => LoopPattern::Single(name.clone()),
                    _ => LoopPattern::Tuple(variables),
                };
                let index_var = node
                    .child_by_field_name("index_variable")
                    .map(|n| self.node_text(n).to_string());
                let iterable = node
                    .child_by_field_name("iterable")
                    .and_then(|n| self.convert_expr(n))
                    .unwrap_or(Expr::Null);
                let body = self.convert_children_block_items(node);
                Some(RawItem::Item(InlineItem::ForLoop {
                    var,
                    index_var,
                    iterable,
                    body,
                }))
            },
            "inline_slot_marker" => Some(RawItem::Item(InlineItem::SlotMarker)),
            "inline_slot_fill" => {
                let slot_name = node
                    .child_by_field_name("name")
                    .map(|n| self.node_text(n).to_string())
                    .unwrap_or_default();
                let fill_items = self.convert_children_block_items(node);
                Some(RawItem::Item(InlineItem::SlotFill {
                    slot: slot_name,
                    items: fill_items,
                }))
            },
            "inline_children_block" => {
                // Standalone children block — attach to preceding actor
                let sub_items = self.convert_children_block_items(node);
                Some(RawItem::Children(sub_items))
            },
            _ => None,
        }
    }

    /// Check if a node has a direct anonymous child with the given text.
    fn has_child_text(&self, node: Node, text: &str) -> bool {
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i as u32) {
                if self.node_text(child) == text {
                    return true;
                }
            }
        }
        false
    }

    /// Strip surrounding quotes from a string literal.
    fn strip_quotes(&self, s: &str) -> String {
        if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
            s[1..s.len() - 1].to_string()
        } else {
            s.to_string()
        }
    }
}

// ── Utility functions ──────────────────────────────────────────────────

/// Convert a tree-sitter node's position to an AST `Span`.
fn node_span(node: Node) -> Option<Span> {
    let start = node.start_position();
    let end = node.end_position();
    Some(Span::new(start.row + 1, start.column + 1, end.row + 1, end.column + 1))
}

/// Convert a tree-sitter node's position to a `ByteSpan`.
fn node_byte_span(node: Node) -> ByteSpan {
    ByteSpan {
        start: node.start_byte(),
        end: node.end_byte(),
    }
}

/// Parse a time literal string like "2s" or "500ms" into a `Time`.
fn parse_time_from_text(text: &str) -> Time {
    let text = text.trim();
    if let Some(ms_str) = text.strip_suffix("ms") {
        if let Ok(ms) = ms_str.parse::<u64>() {
            return Time::Milliseconds(ms);
        }
    }
    if let Some(s_str) = text.strip_suffix('s') {
        if let Ok(s) = s_str.parse::<f64>() {
            return Time::Seconds(s);
        }
    }
    // Bare number — treat as seconds
    if let Ok(s) = text.parse::<f64>() {
        Time::Seconds(s)
    } else {
        Time::Seconds(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_actor() {
        let source = r#"# 0s
title: Text, text: "Hello"
"#;
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        // The actor declaration is grouped into the keyframe body by group_keyframes
        let actor = result.statements.iter().find_map(|s| match s {
            Stmt::Keyframe { body, .. } => body
                .iter()
                .find(|s| matches!(s, Stmt::ActorDecl { label, .. } if label == "title")),
            Stmt::ActorDecl { label, .. } if label == "title" => Some(s),
            _ => None,
        });
        assert!(actor.is_some(), "expected actor 'title', got: {:?}", result.statements);
    }

    #[test]
    fn parse_text_shorthand_creates_text_actor() {
        let source = r#"title: "Hello""#;
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        match &result.statements[0] {
            Stmt::ActorDecl {
                label, ty, props, ..
            } => {
                assert_eq!(label, "title");
                assert_eq!(ty, "Text");
                assert_eq!(props.len(), 1);
                assert_eq!(props[0].name, "text");
                assert_eq!(props[0].value, Expr::Str("Hello".to_string()));
            },
            other => panic!("expected text shorthand actor, got: {:?}", other),
        }
    }

    #[test]
    fn parse_pub_indexed_actor_preserves_fields() {
        let source = "pub bars[2]: Rect, size: (10, 10)\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::ActorDecl {
                is_pub,
                array_index,
                label,
                ..
            } => {
                assert!(*is_pub, "pub flag should be preserved");
                assert_eq!(label, "bars");
                assert!(matches!(array_index, Some(Expr::Num(2.0))));
            },
            other => panic!("expected actor declaration, got: {:?}", other),
        }
    }

    #[test]
    fn parse_comment_normalizes_leading_slashes() {
        let source = "// hello\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::Comment(text, _) => {
                assert_eq!(text, " hello");
            },
            other => panic!("expected comment, got: {:?}", other),
        }
    }

    #[test]
    fn parse_keyframe() {
        let source = "# 2s\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        assert!(
            matches!(&result.statements[0], Stmt::Keyframe { .. }),
            "expected keyframe, got: {:?}",
            result.statements[0]
        );
    }

    #[test]
    fn parse_assignment() {
        let source = "title.opacity = 0.5\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        assert!(
            matches!(&result.statements[0], Stmt::Assignment { .. }),
            "expected assignment, got: {:?}",
            result.statements[0]
        );
    }

    #[test]
    fn parse_always_block() {
        let source = "always { title.rotation = sin(t) }\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        assert!(
            matches!(&result.statements[0], Stmt::Always { .. }),
            "expected always block, got: {:?}",
            result.statements[0]
        );
    }

    #[test]
    fn parse_expression_binary() {
        let source = "let x = 1 + 2\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        match &result.statements[0] {
            Stmt::LetDecl { value, .. } => {
                assert!(
                    matches!(value, Expr::Binary(_, BinaryOp::Add, _)),
                    "expected binary add, got: {:?}",
                    value
                );
            },
            other => panic!("expected let declaration, got: {:?}", other),
        }
    }

    #[test]
    fn parse_config() {
        let source = "config { resolution: (1920, 1080) }\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        assert!(
            matches!(&result.statements[0], Stmt::Config { .. }),
            "expected config, got: {:?}",
            result.statements[0]
        );
    }

    #[test]
    fn parse_import() {
        let source = r#"import "./components.amx""#;
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        match &result.statements[0] {
            Stmt::Import { path, .. } => {
                assert_eq!(path, "./components.amx");
            },
            other => panic!("expected import, got: {:?}", other),
        }
    }

    #[test]
    fn parse_for_loop() {
        let source = "for i in items { }\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        assert!(
            matches!(&result.statements[0], Stmt::ForLoop { .. }),
            "expected for loop, got: {:?}",
            result.statements[0]
        );
    }

    #[test]
    fn parse_play_statement() {
        let source = "play Intro\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        match &result.statements[0] {
            Stmt::Play { scene_name, .. } => {
                assert_eq!(scene_name, "Intro");
            },
            other => panic!("expected play, got: {:?}", other),
        }
    }

    #[test]
    fn parse_component_definition() {
        let source = r#"component MetricCard(title: "Default") {
    frame: Rect, size: (240, 120)
}"#;
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        assert!(
            matches!(&result.statements[0], Stmt::ComponentDef(..)),
            "expected component def, got: {:?}",
            result.statements[0]
        );
    }

    #[test]
    fn parse_reactive_binding() {
        let source = "title.opacity := sin(t)\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        assert!(
            matches!(&result.statements[0], Stmt::ReactiveBinding { .. }),
            "expected reactive binding, got: {:?}",
            result.statements[0]
        );
    }

    #[test]
    fn parse_tuple_expression() {
        let source = "let pos = (100, 200)\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::LetDecl { value, .. } => {
                assert!(matches!(value, Expr::Tuple(_)), "expected tuple, got: {:?}", value);
            },
            other => panic!("expected let, got: {:?}", other),
        }
    }

    #[test]
    fn parse_single_parenthesized_expression_is_not_tuple() {
        let source = "let x = (a + b)\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::LetDecl { value, .. } => {
                assert!(
                    matches!(value, Expr::Binary(_, BinaryOp::Add, _)),
                    "expected parenthesized binary expression, got: {:?}",
                    value
                );
            },
            other => panic!("expected let, got: {:?}", other),
        }
    }

    #[test]
    fn parse_inline_for_loop_index_var() {
        let source = "#0s\nrow: Row {\n  for item, i in {1, 2, 3} {\n    box[i]: Rect, size: (10, item)\n  }\n}\n";
        let result = parse_source(source).expect("parse should succeed");
        let Stmt::Keyframe { body, .. } = &result.statements[0] else {
            panic!("expected keyframe, got: {:?}", result.statements[0]);
        };
        let Stmt::ActorDecl { children, .. } = &body[0] else {
            panic!("expected actor declaration, got: {:?}", body[0]);
        };
        match &children[0] {
            InlineItem::ForLoop { var, index_var, .. } => {
                assert_eq!(var, &LoopPattern::Single("item".to_string()));
                assert_eq!(index_var.as_deref(), Some("i"));
            },
            other => panic!("expected inline for loop, got: {:?}", other),
        }
    }

    #[test]
    fn parse_closure_expression() {
        let source = "let f = (x) => x * 2\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::LetDecl { value, .. } => {
                assert!(matches!(value, Expr::Closure(..)), "expected closure, got: {:?}", value);
            },
            other => panic!("expected let, got: {:?}", other),
        }
    }

    #[test]
    fn parse_scene_declaration() {
        let source = "# Intro\n";
        let result = parse_source(source).expect("parse should succeed");
        assert!(!result.statements.is_empty(), "expected statements");
        // Could be a scene declaration or a keyframe — depends on grammar
        // Scene declarations have a name field
    }

    #[test]
    fn parse_scene_groups_body_and_config() {
        let source = concat!(
            "# Intro\n",
            "config { duration: 5.0 }\n",
            "#0s\n",
            "title: Text, text: \"Welcome\"\n",
        );
        let result = parse_source(source).expect("parse should succeed");
        let stmts = &result.statements;
        assert_eq!(stmts.len(), 1, "scene body should be grouped under the scene");
        let Stmt::Scene {
            name, config, body, ..
        } = &stmts[0]
        else {
            panic!("expected scene, got: {:?}", stmts[0]);
        };
        assert_eq!(name, "Intro");
        assert_eq!(config.len(), 1);
        assert_eq!(config[0].name, "duration");
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0], Stmt::Keyframe { .. }), "expected keyframe, got: {:?}", body[0]);
    }

    #[test]
    fn parse_type_alias() {
        let source = "type LegendMode = Bool | Str\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::TypeAlias {
                name, annotation, ..
            } => {
                assert_eq!(name, "LegendMode");
                assert_eq!(
                    annotation,
                    &TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str])
                );
            },
            other => panic!("expected type alias, got: {:?}", other),
        }
    }

    #[test]
    fn parse_namespaced_type_alias_reference() {
        let source = "pub component Card(value: types::Metric) {}\n";
        let result = parse_source(source).expect("parse should succeed");
        if let Stmt::ComponentDef(def, _) = &result.statements[0] {
            assert_eq!(
                def.params[0].param_type,
                Some(TypeAnnotation::Alias("types::Metric".to_string()))
            );
        } else {
            panic!("expected component definition");
        }
    }

    #[test]
    fn parse_indexed_action_target() {
        let source = "pulse bar[0] [200ms]\n";
        let result = parse_source(source).expect("parse should succeed");
        let action = match &result.statements[0] {
            Stmt::Keyframe { body, .. } => body.iter().find_map(|s| match s {
                Stmt::Action(action, _) => Some(action),
                _ => None,
            }),
            Stmt::Action(action, _) => Some(action),
            _ => None,
        }
        .expect("expected action");
        assert_eq!(action.targets, vec!["bar__0"]);
    }

    #[test]
    fn parse_dotted_action_target() {
        let source = "fade-in parent.child [800ms]\n";
        let result = parse_source(source).expect("parse should succeed");
        let action = match &result.statements[0] {
            Stmt::Keyframe { body, .. } => body.iter().find_map(|s| match s {
                Stmt::Action(action, _) => Some(action),
                _ => None,
            }),
            Stmt::Action(action, _) => Some(action),
            _ => None,
        }
        .expect("expected action");
        assert_eq!(action.targets, vec!["parent.child"]);
    }

    #[test]
    fn parse_duration_modifier_matches_peg_shape() {
        let source = "fade-in label [500ms, ease: bounce]\n";
        let result = parse_source(source).expect("parse should succeed");
        let action = match &result.statements[0] {
            Stmt::Keyframe { body, .. } => body.iter().find_map(|s| match s {
                Stmt::Action(action, _) => Some(action),
                _ => None,
            }),
            Stmt::Action(action, _) => Some(action),
            _ => None,
        }
        .expect("expected action");
        assert_eq!(action.modifiers.len(), 2);
        assert_eq!(action.modifiers[0].name, None);
        assert_eq!(action.modifiers[0].value, Expr::Ident("500ms".to_string()));
        assert_eq!(action.modifiers[1].name.as_deref(), Some("ease"));
    }

    #[test]
    fn parse_play_transition_from_bare_modifiers() {
        let source = "play scenes.FadeIn [fade, 300ms]\n";
        let result = parse_source(source).expect("parse should succeed");
        let Stmt::Play {
            scene_name,
            transition,
            ..
        } = &result.statements[0]
        else {
            panic!("expected play, got: {:?}", result.statements[0]);
        };
        assert_eq!(scene_name, "scenes.FadeIn");
        let transition = transition.as_ref().expect("transition");
        assert_eq!(transition.id, "fade");
        assert_eq!(transition.duration_ms, 300);
    }

    #[test]
    fn parse_assignment_extracts_property() {
        let source = "title.opacity = 0.5\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::Assignment {
                target, property, ..
            } => {
                assert_eq!(target, &[TargetSegment::Static("title".to_string())]);
                assert_eq!(property, "opacity");
            },
            other => panic!("expected assignment, got: {:?}", other),
        }
    }

    #[test]
    fn parse_indexed_runtime_assignment() {
        let source = "bars[i].color = red\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::Assignment {
                target, property, ..
            } => {
                assert_eq!(property, "color");
                assert_eq!(target.len(), 1);
                match &target[0] {
                    TargetSegment::Indexed { base, index } => {
                        assert_eq!(base, "bars");
                        assert!(matches!(index.as_ref(), Expr::Ident(_)));
                    },
                    other => panic!("unexpected target segment: {:?}", other),
                }
            },
            other => panic!("expected assignment, got: {:?}", other),
        }
    }

    #[test]
    fn parse_literal_indexed_assignment() {
        let source = "bars[0].opacity = 0.5\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::Assignment {
                target, property, ..
            } => {
                assert_eq!(property, "opacity");
                assert_eq!(target, &[TargetSegment::Static("bars__0".to_string())]);
            },
            other => panic!("expected assignment, got: {:?}", other),
        }
    }

    #[test]
    fn parse_nested_indexed_assignment() {
        let source = "bars[i].rows[j].x = red\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::Assignment {
                target, property, ..
            } => {
                assert_eq!(property, "x");
                assert_eq!(target.len(), 2);
                match (&target[0], &target[1]) {
                    (
                        TargetSegment::Indexed { base: first, .. },
                        TargetSegment::Indexed { base: second, .. },
                    ) => {
                        assert_eq!(first, "bars");
                        assert_eq!(second, "rows");
                    },
                    other => panic!("unexpected target segments: {:?}", other),
                }
            },
            other => panic!("expected assignment, got: {:?}", other),
        }
    }

    #[test]
    fn parse_indexed_reactive_binding() {
        let source = "a.bars[i].color := red\n";
        let result = parse_source(source).expect("parse should succeed");
        match &result.statements[0] {
            Stmt::ReactiveBinding {
                target, property, ..
            } => {
                assert_eq!(property, "color");
                assert_eq!(target.len(), 2);
                match (&target[0], &target[1]) {
                    (TargetSegment::Static(scope), TargetSegment::Indexed { base, .. }) => {
                        assert_eq!(scope, "a");
                        assert_eq!(base, "bars");
                    },
                    other => panic!("unexpected target segments: {:?}", other),
                }
            },
            other => panic!("expected reactive binding, got: {:?}", other),
        }
    }

    #[test]
    fn incremental_reparse() {
        let source1 = "# 0s\ntitle: Text, text: \"Hello\"\n";
        let source2 = "# 0s\ntitle: Text, text: \"World\"\n";

        let result1 = parse_source(source1).expect("first parse should succeed");
        let result2 = reparse(source2, &result1.tree).expect("reparse should succeed");

        // Both should produce valid ASTs
        assert!(!result1.statements.is_empty());
        assert!(!result2.statements.is_empty());
    }
}
