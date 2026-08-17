//! Identifier occurrences derived from the parser and lossless token stream.
//!
//! The parser records `(byte_span, name, kind, scope)` entries as it consumes
//! identifiers; GUI and LSP convert those entries into semantic-token roles.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::ast::{ByteSpan, Stmt};
use crate::highlight;
use crate::symbol_table::SymbolTable;
use crate::token::TokenKind;

/// The syntactic role of an identifier occurrence.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OccurrenceKind {
    /// Actor or variable label.
    Label,
    /// Type name.
    Type,
    /// Property name.
    Property,
    /// Component or action parameter name.
    Parameter,
    /// Function or method name.
    Function,
    /// Action verb.
    Action,
    /// Scene name.
    Scene,
    /// Component definition name.
    Component,
    /// Type alias name.
    TypeAlias,
    /// Import alias.
    ImportAlias,
    /// Match wildcard `_`.
    Wildcard,
    /// Any other identifier reference.
    Variable,
}

/// A single identifier occurrence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Occurrence {
    /// Byte range of the identifier.
    pub span: ByteSpan,
    /// Source text of the identifier.
    pub name: String,
    /// Syntactic role.
    pub kind: OccurrenceKind,
    /// Lexical scope identifier, populated by the parser while recording.
    pub scope_id: Option<u32>,
    /// Parent scope of `scope_id`, used to resolve shadowed bindings.
    pub parent_scope_id: Option<u32>,
    /// Whether the identifier is a declaration rather than a reference.
    pub declaration: bool,
}

impl OccurrenceKind {
    /// Convert an occurrence into one of the shared semantic-token roles.
    ///
    /// This intentionally reuses the existing GUI/LSP role vocabulary so both
    /// consumers can stay on a small conversion layer.
    pub fn token_role(self) -> &'static str {
        match self {
            OccurrenceKind::Label | OccurrenceKind::Scene => "label",
            OccurrenceKind::Type | OccurrenceKind::Component | OccurrenceKind::TypeAlias => "type",
            OccurrenceKind::Property => "property",
            OccurrenceKind::Parameter => "parameter",
            OccurrenceKind::Function => "function",
            OccurrenceKind::Action => "action",
            OccurrenceKind::ImportAlias => "importalias",
            OccurrenceKind::Wildcard => "wildcard",
            OccurrenceKind::Variable => "variable",
        }
    }
}

/// Collect occurrences for every identifier token in `source`.
pub fn collect(stmts: &[Stmt], source: &str) -> Vec<Occurrence> {
    let tokens = crate::token::tokenize(source);
    let symbols = SymbolTable::build_from_ast(stmts);
    let label_names = highlight::collect_label_names(stmts);
    let property_names: HashSet<String> =
        symbols.properties.values().flat_map(|props| props.iter().cloned()).collect();
    let param_names: HashSet<String> = symbols
        .components
        .values()
        .flat_map(|c| c.params.iter().map(|p| p.name.clone()))
        .collect();
    let import_aliases = highlight::collect_import_alias_names(stmts);

    tokens
        .iter()
        .enumerate()
        .filter_map(|(idx, token)| {
            let name = match &token.kind {
                TokenKind::Ident(name) => name.clone(),
                _ => return None,
            };
            let role = highlight::classify_token(
                idx,
                token,
                &tokens,
                &symbols,
                &label_names,
                &property_names,
                &param_names,
                &import_aliases,
            );
            Some(Occurrence {
                span: token.span,
                name,
                kind: role_to_kind(role),
                scope_id: None,
                parent_scope_id: None,
                declaration: false,
            })
        })
        .collect()
}

fn role_to_kind(role: &str) -> OccurrenceKind {
    match role {
        "label" => OccurrenceKind::Label,
        "type" => OccurrenceKind::Type,
        "property" => OccurrenceKind::Property,
        "parameter" => OccurrenceKind::Parameter,
        "function" => OccurrenceKind::Function,
        "action" => OccurrenceKind::Action,
        "scene" => OccurrenceKind::Scene,
        "component" => OccurrenceKind::Component,
        "typealias" => OccurrenceKind::TypeAlias,
        "importalias" => OccurrenceKind::ImportAlias,
        "wildcard" => OccurrenceKind::Wildcard,
        _ => OccurrenceKind::Variable,
    }
}

#[derive(Clone, Copy)]
struct ScopeFrame {
    id: u32,
    parent: Option<u32>,
}

struct ParserState {
    occurrences: Vec<Occurrence>,
    scope_stack: Vec<ScopeFrame>,
    next_scope: u32,
}

thread_local! {
    static PARSER_STATE: RefCell<Vec<ParserState>> = const { RefCell::new(Vec::new()) };
}

/// Run `f` while recording parser-side occurrences.
pub(crate) fn with_occurrences<T>(f: impl FnOnce() -> T) -> (T, Vec<Occurrence>) {
    PARSER_STATE.with(|stack| {
        stack.borrow_mut().push(ParserState {
            occurrences: Vec::new(),
            scope_stack: vec![ScopeFrame {
                id: 1,
                parent: None,
            }],
            next_scope: 2,
        });
    });
    let result = f();
    let mut occurrences = PARSER_STATE
        .with(|stack| stack.borrow_mut().pop().map(|state| state.occurrences).unwrap_or_default());

    // Parser alternatives can record the same token more than once. Keep one
    // occurrence per byte range, preferring declaration and more specific
    // roles, then restore source order for span lookups.
    occurrences.sort_by_key(|o| (o.span.start, o.span.end));
    let mut index_by_span: HashMap<(usize, usize), usize> = HashMap::new();
    let mut unique = Vec::with_capacity(occurrences.len());
    for occurrence in occurrences {
        let key = (occurrence.span.start, occurrence.span.end);
        match index_by_span.get(&key).copied() {
            Some(idx) if !is_better_occurrence(&occurrence, &unique[idx]) => {},
            Some(idx) => unique[idx] = occurrence,
            None => {
                index_by_span.insert(key, unique.len());
                unique.push(occurrence);
            },
        }
    }
    (result, unique)
}

/// Enter a new lexical scope while parsing.
pub(crate) fn push_scope() {
    PARSER_STATE.with(|stack| {
        let mut stack = stack.borrow_mut();
        let Some(state) = stack.last_mut() else {
            return;
        };
        let id = state.next_scope;
        state.next_scope += 1;
        let parent = state.scope_stack.last().map(|frame| frame.id);
        state.scope_stack.push(ScopeFrame { id, parent });
    });
}

/// Leave the current lexical scope after its parser body succeeds.
pub(crate) fn pop_scope() {
    PARSER_STATE.with(|stack| {
        let mut stack = stack.borrow_mut();
        let Some(state) = stack.last_mut() else {
            return;
        };
        if state.scope_stack.len() > 1 {
            state.scope_stack.pop();
        }
    });
}

/// Record a parser-side identifier reference occurrence.
pub(crate) fn record(kind: OccurrenceKind, name: String, span: ByteSpan) {
    record_with(kind, name, span, false);
}

/// Record a parser-side identifier declaration occurrence.
pub(crate) fn record_declaration(kind: OccurrenceKind, name: String, span: ByteSpan) {
    record_with(kind, name, span, true);
}

fn is_better_occurrence(new: &Occurrence, old: &Occurrence) -> bool {
    if new.declaration != old.declaration {
        return new.declaration;
    }
    occurrence_priority(new.kind) > occurrence_priority(old.kind)
}

fn occurrence_priority(kind: OccurrenceKind) -> u8 {
    match kind {
        OccurrenceKind::Label => 100,
        OccurrenceKind::Type => 96,
        OccurrenceKind::Function => 94,
        OccurrenceKind::Parameter => 92,
        OccurrenceKind::Variable => 90,
        OccurrenceKind::Property => 70,
        OccurrenceKind::Scene => 65,
        OccurrenceKind::Action => 60,
        OccurrenceKind::Component => 55,
        OccurrenceKind::TypeAlias => 50,
        OccurrenceKind::ImportAlias => 45,
        OccurrenceKind::Wildcard => 40,
    }
}

fn record_with(kind: OccurrenceKind, name: String, span: ByteSpan, declaration: bool) {
    PARSER_STATE.with(|stack| {
        let mut stack = stack.borrow_mut();
        let Some(state) = stack.last_mut() else {
            return;
        };
        let (scope_id, parent_scope_id) = state
            .scope_stack
            .last()
            .map(|frame| (Some(frame.id), frame.parent))
            .unwrap_or((None, None));
        state.occurrences.push(Occurrence {
            span,
            name,
            kind,
            scope_id,
            parent_scope_id,
            declaration,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_source;

    fn occ(source: &str) -> Vec<(String, OccurrenceKind)> {
        let (ast, _) = parse_source(source);
        let stmts = ast.unwrap_or_default();
        collect(&stmts, source).into_iter().map(|o| (o.name, o.kind)).collect()
    }

    #[test]
    fn classifies_actor_label_type_and_property() {
        let roles = occ("title: Text, size: (100, 100)\n");
        assert!(roles.iter().any(|(n, k)| n == "title" && *k == OccurrenceKind::Label));
        assert!(roles.iter().any(|(n, k)| n == "Text" && *k == OccurrenceKind::Type));
        assert!(roles.iter().any(|(n, k)| n == "size" && *k == OccurrenceKind::Property));
    }

    #[test]
    fn classifies_action_target_and_call() {
        let roles = occ("fade-in card[0] [1s]\nlet x = cos(0)\n");
        assert!(roles.iter().any(|(n, k)| n == "fade-in" && *k == OccurrenceKind::Action));
        assert!(roles.iter().any(|(n, k)| n == "card" && *k == OccurrenceKind::Label));
        assert!(roles.iter().any(|(n, k)| n == "cos" && *k == OccurrenceKind::Function));
    }
}

#[test]
fn parser_records_declaration_occurrences() {
    let (ast, _errors, occurrences) =
        crate::parser::parse_source_with_occurrences("title: Text, size: (100, 100)\nlet x = 1\n");
    assert!(ast.is_some());
    let kinds: Vec<(String, OccurrenceKind)> =
        occurrences.iter().map(|o| (o.name.clone(), o.kind)).collect();
    assert!(kinds.iter().any(|(n, k)| n == "title" && *k == OccurrenceKind::Label));
    assert!(kinds.iter().any(|(n, k)| n == "Text" && *k == OccurrenceKind::Type));
    assert!(kinds.iter().any(|(n, k)| n == "x" && *k == OccurrenceKind::Variable));
}

#[test]
fn parser_records_expression_target_and_closure_occurrences() {
    let source = r#"
let f = (x) => x
let result = mix(a, b)
let mapped = graph.map(mx, my)
let widget = Widget { size: (10, 20) }
always {
    ball.color = color
    ball.visible := flag
}
"#;
    let (ast, errors, occurrences) = crate::parser::parse_source_with_occurrences(source);
    assert!(ast.is_some(), "expected source to parse");
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");

    let kind =
        |name: &str, k: OccurrenceKind| occurrences.iter().any(|o| o.name == name && o.kind == k);
    assert!(kind("x", OccurrenceKind::Parameter), "closure parameter should be recorded");
    assert!(kind("mix", OccurrenceKind::Function), "call name should be a function");
    assert!(kind("graph", OccurrenceKind::Variable), "method receiver should be a variable");
    assert!(kind("map", OccurrenceKind::Function), "method name should be a function");
    assert!(kind("Widget", OccurrenceKind::Type), "constructor should be a type");
    assert!(
        kind("size", OccurrenceKind::Property),
        "constructor property should be recorded"
    );
    assert!(kind("ball", OccurrenceKind::Label), "assignment target should be a label");
    assert!(
        kind("color", OccurrenceKind::Property),
        "assignment property should be recorded"
    );
    assert!(
        kind("visible", OccurrenceKind::Property),
        "reactive property should be recorded"
    );
    assert!(
        kind("flag", OccurrenceKind::Variable),
        "reactive value identifier should be recorded"
    );

    let closure_param = occurrences
        .iter()
        .find(|o| o.name == "x" && o.kind == OccurrenceKind::Parameter)
        .expect("closure parameter");
    let closure_body_ref = occurrences
        .iter()
        .find(|o| o.name == "x" && o.kind == OccurrenceKind::Variable)
        .expect("closure body reference");
    assert_eq!(closure_param.scope_id, closure_body_ref.scope_id);
    assert!(closure_param.declaration);
    assert!(!closure_body_ref.declaration);
}

#[test]
fn parser_assigns_distinct_scopes_to_shadowed_block_variables() {
    let source =
        "let value = 1\nalways {\n    let value = 2\n    result = value\n}\nother = value\n";
    let (ast, errors, occurrences) = crate::parser::parse_source_with_occurrences(source);
    assert!(ast.is_some(), "expected source to parse");
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");

    let always_at = source.find("always").unwrap();
    let inner_decl = occurrences
        .iter()
        .find(|o| o.name == "value" && o.declaration && o.span.start > always_at)
        .expect("inner declaration");
    let inner_ref = occurrences
        .iter()
        .find(|o| o.name == "value" && !o.declaration && o.span.start > always_at)
        .expect("inner reference");
    let outer_decl = occurrences
        .iter()
        .find(|o| o.name == "value" && o.declaration && o.span.start < always_at)
        .expect("outer declaration");
    let block_end = source.find('}').unwrap();
    let outer_ref = occurrences
        .iter()
        .find(|o| o.name == "value" && !o.declaration && o.span.start > block_end)
        .expect("outer reference");

    assert_eq!(inner_decl.scope_id, inner_ref.scope_id);
    assert_eq!(outer_decl.scope_id, outer_ref.scope_id);
    assert_ne!(inner_decl.scope_id, outer_decl.scope_id);
    assert!(inner_decl.scope_id.is_some());
    assert!(outer_decl.scope_id.is_some());
}
