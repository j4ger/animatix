//! Shared token-role classification for syntax highlighting.
//!
//! GUI highlighting and LSP semantic tokens both consume this module so label,
//! type, property, parameter, function, and variable classification is defined
//! once instead of drifting between consumers.

use std::collections::HashSet;

use crate::ast::{InlineItem, Stmt, is_array_member_label};
use crate::symbol_table::{LabelKind, SymbolTable};
use crate::token::{Token, TokenKind};

/// Classify every token in `tokens`, returning one role name per token.
pub fn classify_tokens(
    tokens: &[Token],
    symbols: &SymbolTable,
    label_names: &HashSet<String>,
    property_names: &HashSet<String>,
    param_names: &HashSet<String>,
) -> Vec<&'static str> {
    tokens
        .iter()
        .enumerate()
        .map(|(idx, token)| {
            classify_token(idx, token, tokens, symbols, label_names, property_names, param_names)
        })
        .collect()
}

/// Classify a single token by lexical kind plus, for identifiers, its AST
/// symbol role and neighboring-token context.
pub fn classify_token(
    idx: usize,
    token: &Token,
    tokens: &[Token],
    symbols: &SymbolTable,
    label_names: &HashSet<String>,
    property_names: &HashSet<String>,
    param_names: &HashSet<String>,
) -> &'static str {
    match &token.kind {
        TokenKind::Keyword(_) => "keyword",
        TokenKind::Number(_) | TokenKind::Time { .. } | TokenKind::Percent(_) => "number",
        TokenKind::Bool(_) => "boolean",
        TokenKind::Str(_) | TokenKind::Typst(_) => "string",
        TokenKind::Comment(_) => "comment",
        TokenKind::Null => "keyword",
        TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::PercentOp
        | TokenKind::Caret
        | TokenKind::Eq
        | TokenKind::Neq
        | TokenKind::Lt
        | TokenKind::Gt
        | TokenKind::Le
        | TokenKind::Ge
        | TokenKind::And
        | TokenKind::Or
        | TokenKind::Not
        | TokenKind::Assign
        | TokenKind::ReactiveAssign
        | TokenKind::Arrow
        | TokenKind::RangeInclusive
        | TokenKind::Pipe
        | TokenKind::ColonColon => "operator",
        TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::LBracket
        | TokenKind::RBracket
        | TokenKind::LBrace
        | TokenKind::RBrace
        | TokenKind::Colon
        | TokenKind::Comma
        | TokenKind::Dot
        | TokenKind::Hash
        | TokenKind::At
        | TokenKind::AtSlot
        | TokenKind::Underscore => "punctuation",
        TokenKind::Ident(name) => {
            classify_ident(name, idx, tokens, symbols, label_names, property_names, param_names)
        },
    }
}

fn classify_ident(
    name: &str,
    idx: usize,
    tokens: &[Token],
    symbols: &SymbolTable,
    label_names: &HashSet<String>,
    property_names: &HashSet<String>,
    param_names: &HashSet<String>,
) -> &'static str {
    if let Some(info) = symbols.labels.get(name) {
        return match info.kind {
            LabelKind::Actor | LabelKind::Component => "label",
            LabelKind::Let | LabelKind::For | LabelKind::Always => "variable",
        };
    }
    if label_names.contains(name) || symbols.scenes.contains_key(name) {
        return "label";
    }
    if symbols.types.contains(name)
        || symbols.components.contains_key(name)
        || symbols.type_aliases.contains_key(name)
    {
        return "type";
    }
    if param_names.contains(name) {
        return "parameter";
    }

    // Function and method calls are structural, not name-based: any identifier
    // immediately followed by `(` is a call, whether it is builtin (`cos`) or
    // user-defined.
    if next_significant(tokens, idx).is_some_and(|k| matches!(k, TokenKind::LParen)) {
        return "function";
    }

    // Property names in declaration lists (`size: ...`) and access paths
    // (`actor.size`) are distinguishable from action verbs that share a name
    // (e.g. `scale`).
    if next_significant(tokens, idx).is_some_and(|k| matches!(k, TokenKind::Colon))
        || prev_significant(tokens, idx).is_some_and(|k| matches!(k, TokenKind::Dot))
    {
        return "property";
    }

    if symbols.actions.contains(name) {
        return "function";
    }
    if property_names.contains(name) {
        return "property";
    }

    "variable"
}

/// Collect labels used in action targets and assignment/reactive target bases.
///
/// Indexed targets are stored resolved (`card__0`), so the array suffix is
/// stripped back to the source-level base label. Dotted paths contribute only
/// their first segment, matching tree-sitter's `@label` behavior.
pub fn collect_label_names(stmts: &[Stmt]) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in stmts {
        collect_stmt_label_names(stmt, &mut names);
    }
    names
}

fn collect_stmt_label_names(stmt: &Stmt, names: &mut HashSet<String>) {
    match stmt {
        Stmt::ActorDecl {
            label, children, ..
        } => {
            names.insert(label.clone());
            for child in children {
                collect_inline_label_names(child, names);
            }
        },
        Stmt::Action(action, _) => {
            for target in &action.targets {
                if let Some(base) = label_base(target) {
                    names.insert(base.to_string());
                }
            }
        },
        Stmt::Assignment { target, .. } | Stmt::ReactiveBinding { target, .. } => {
            if let Some(first) = target.first() {
                if let Some(base) = label_base(first.label_str()) {
                    names.insert(base.to_string());
                }
            }
        },
        Stmt::Scene { name, body, .. } => {
            names.insert(name.clone());
            for child in body {
                collect_stmt_label_names(child, names);
            }
        },
        Stmt::Play { scene_name, .. } => {
            names.insert(scene_name.clone());
        },
        Stmt::Keyframe { body, .. }
        | Stmt::RelativeKeyframe { body, .. }
        | Stmt::Sequence { body, .. }
        | Stmt::Stagger { body, .. }
        | Stmt::Always { body, .. }
        | Stmt::ForLoop { body, .. }
        | Stmt::ComponentAction { body, .. } => {
            for child in body {
                collect_stmt_label_names(child, names);
            }
        },
        Stmt::ComponentDef(def, _) => {
            for child in &def.body {
                collect_stmt_label_names(child, names);
            }
        },
        Stmt::Conditional {
            then_branch,
            else_branch,
            ..
        } => {
            for child in then_branch {
                collect_stmt_label_names(child, names);
            }
            if let Some(else_body) = else_branch {
                for child in else_body {
                    collect_stmt_label_names(child, names);
                }
            }
        },
        Stmt::Match { arms, .. } => {
            for (_, body) in arms {
                for child in body {
                    collect_stmt_label_names(child, names);
                }
            }
        },
        _ => {},
    }
}

fn collect_inline_label_names(item: &InlineItem, names: &mut HashSet<String>) {
    match item {
        InlineItem::Labeled {
            label, children, ..
        } => {
            names.insert(label.clone());
            for child in children {
                collect_inline_label_names(child, names);
            }
        },
        InlineItem::Anonymous { children, .. } => {
            for child in children {
                collect_inline_label_names(child, names);
            }
        },
        InlineItem::ForLoop { body, .. } => {
            for child in body {
                collect_inline_label_names(child, names);
            }
        },
        InlineItem::SlotFill { items, .. } => {
            for child in items {
                collect_inline_label_names(child, names);
            }
        },
        InlineItem::SlotMarker => {},
    }
}

/// Return the source-level label base for a resolved target string.
fn label_base(target: &str) -> Option<&str> {
    let first = target.split('.').next().unwrap_or(target);
    is_array_member_label(first).or(Some(first))
}

fn prev_significant(tokens: &[Token], idx: usize) -> Option<&TokenKind> {
    tokens[..idx]
        .iter()
        .rev()
        .find(|t| !matches!(t.kind, TokenKind::Comment(_)))
        .map(|t| &t.kind)
}

fn next_significant(tokens: &[Token], idx: usize) -> Option<&TokenKind> {
    tokens.get(idx + 1..).and_then(|rest| {
        rest.iter().find(|t| !matches!(t.kind, TokenKind::Comment(_))).map(|t| &t.kind)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::tokenize;

    fn classify_all(source: &str) -> Vec<(String, &'static str)> {
        let tokens = tokenize(source);
        let stmts = crate::parser::parse_source(source).0.unwrap_or_default();
        let symbols = SymbolTable::build_from_ast(&stmts);
        let label_names = collect_label_names(&stmts);
        let property_names: HashSet<String> =
            symbols.properties.values().flat_map(|props| props.iter().cloned()).collect();
        let param_names: HashSet<String> = symbols
            .components
            .values()
            .flat_map(|c| c.params.iter().map(|p| p.name.clone()))
            .collect();
        classify_tokens(&tokens, &symbols, &label_names, &property_names, &param_names)
            .into_iter()
            .zip(tokens.iter())
            .map(|(role, token)| (token_text(token), role))
            .collect()
    }

    fn token_text(token: &Token) -> String {
        match &token.kind {
            TokenKind::Ident(s) | TokenKind::Keyword(s) => s.clone(),
            _ => String::new(),
        }
    }

    #[test]
    fn resolves_scale_as_property_and_action_by_context() {
        let roles = classify_all("card[0].scale = 1.0\nscale card to 2\n");
        let scale_prop = roles.iter().find(|(text, _)| text == "scale");
        assert_eq!(scale_prop.map(|(_, role)| *role), Some("property"));
        let action_scale = roles
            .iter()
            .filter(|(text, role)| text == "scale" && *role == "function")
            .count();
        assert_eq!(action_scale, 1);
    }

    #[test]
    fn labels_indexed_target_base() {
        let roles = classify_all("fade-in card[0], named [1s]\n");
        assert!(roles.iter().any(|(text, role)| text == "card" && *role == "label"));
        assert!(roles.iter().any(|(text, role)| text == "named" && *role == "label"));
    }

    #[test]
    fn classifies_builtin_functions() {
        let roles = classify_all("let x = cos(0) + rgb(255, 0, 0)\n");
        assert!(roles.iter().any(|(text, role)| text == "cos" && *role == "function"));
        assert!(roles.iter().any(|(text, role)| text == "rgb" && *role == "function"));
    }
}
