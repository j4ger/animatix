use super::*;

use crate::symbol_table::SymbolTable;

#[test]
fn verb_position_offers_actions_after_unknown_verb() {
    // `fly` is not a real action; the caret sits after it in verb slot.
    let source = "#0s\nfly \n";
    let symbols = SymbolTable::build_from_ast(&[]);
    let tokens = animatix_syntax::token::tokenize(source);
    let items = completions_at(&symbols, &tokens, source, 1, 4, &ExtensionManifest::default());

    let actions: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Action)
        .map(|i| i.label.as_str())
        .collect();
    assert!(
        actions.contains(&"fade-in") && actions.contains(&"move"),
        "unknown verb in verb position must still offer the action list: {actions:?}"
    );
}

#[test]
fn partial_verb_at_line_start_offers_actions() {
    let source = "#0s\nfa\n";
    let symbols = SymbolTable::build_from_ast(&[]);
    let tokens = animatix_syntax::token::tokenize(source);
    let items = completions_at(&symbols, &tokens, source, 1, 2, &ExtensionManifest::default());

    assert!(
        items.iter().any(|i| i.label == "fade-in" && i.kind == CompletionKind::Action),
        "partial verb at line start must offer action verbs"
    );
}

#[test]
fn known_verb_still_completes_targets() {
    let source = "box: Rect, size: (10, 10)\n#0s\nfade-in box [1s]\n";
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "test source must parse: {errors:?}");
    let symbols = SymbolTable::build_from_ast(stmts.as_deref().unwrap_or(&[]));
    let tokens = animatix_syntax::token::tokenize(source);
    // Caret right after `fade-in ` on line 2.
    let items = completions_at(&symbols, &tokens, source, 2, 8, &ExtensionManifest::default());

    assert!(
        items.iter().any(|i| i.label == "box" && i.kind == CompletionKind::Label),
        "known action must complete actor targets"
    );
}

#[test]
fn top_level_completions_include_keywords() {
    let source = "";
    let symbols = SymbolTable::build_from_ast(&[]);
    let items = completions_at(&symbols, &[], source, 0, 0, &ExtensionManifest::default());

    let keywords: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Keyword)
        .map(|i| i.label.as_str())
        .collect();

    assert!(keywords.contains(&"let"));
    assert!(keywords.contains(&"import"));
    assert!(keywords.contains(&"if"));
}

#[test]
fn type_completions_include_builtins() {
    let source = "";
    let symbols = SymbolTable::build_from_ast(&[]);
    let items = completions_at(&symbols, &[], source, 0, 0, &ExtensionManifest::default());

    let types: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Type)
        .map(|i| i.label.as_str())
        .collect();

    assert!(types.contains(&"Text"));
    assert!(types.contains(&"Ellipse"));
    assert!(types.contains(&"Button"));
}

#[test]
fn snippet_completions_at_top_level() {
    let source = "";
    let symbols = SymbolTable::build_from_ast(&[]);
    let items = completions_at(&symbols, &[], source, 0, 0, &ExtensionManifest::default());

    let snippets: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Snippet)
        .map(|i| i.label.as_str())
        .collect();

    assert!(snippets.contains(&"actor"));
    assert!(snippets.contains(&"keyframe"));
    assert!(snippets.contains(&"component"));
}

#[test]
fn action_completions_include_builtins() {
    let source = "";
    let symbols = SymbolTable::build_from_ast(&[]);
    let items = completions_at(&symbols, &[], source, 0, 0, &ExtensionManifest::default());

    let actions: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Action)
        .map(|i| i.label.as_str())
        .collect();

    assert!(actions.contains(&"fade-in"));
    assert!(actions.contains(&"move"));
    assert!(actions.contains(&"scale"));
}
