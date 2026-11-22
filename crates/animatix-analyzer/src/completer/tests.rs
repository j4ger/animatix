use super::*;

use crate::symbol_table::SymbolTable;

#[test]
fn callable_position_offers_functions_after_unknown_name() {
    // `fly` is not a real callable; the caret sits after it in a call slot.
    let source = "#0s\nfly \n";
    let symbols = SymbolTable::build_from_ast(&[]);
    let tokens = animatix_syntax::token::tokenize(source);
    let items = completions_at(&symbols, &tokens, source, 1, 4, &ExtensionManifest::default());

    let callables: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Callable)
        .map(|i| i.label.as_str())
        .collect();
    assert!(
        callables.contains(&"fade_in") && callables.contains(&"move"),
        "unknown callable in call position must still offer callables: {callables:?}"
    );
}

#[test]
fn partial_callable_at_line_start_offers_callables() {
    let source = "#0s\nfa\n";
    let symbols = SymbolTable::build_from_ast(&[]);
    let tokens = animatix_syntax::token::tokenize(source);
    let items = completions_at(&symbols, &tokens, source, 1, 2, &ExtensionManifest::default());

    assert!(
        items.iter().any(|i| i.label == "fade_in" && i.kind == CompletionKind::Callable),
        "partial callable at line start must offer functions"
    );
}

#[test]
fn known_callable_completes_actor_arguments() {
    let source = "box: Rect, size: (10, 10)\n#0s\nfade_in(target: box, duration: 1s)\n";
    let (stmts, errors) = animatix_syntax::parser::parse_source(source);
    assert!(errors.is_empty(), "test source must parse: {errors:?}");
    let symbols = SymbolTable::build_from_ast(stmts.as_deref().unwrap_or(&[]));
    let tokens = animatix_syntax::token::tokenize(source);
    // Caret on the value of the named Actor target.
    let items = completions_at(&symbols, &tokens, source, 2, 16, &ExtensionManifest::default());

    assert!(
        items.iter().any(|i| i.label == "box" && i.kind == CompletionKind::Label),
        "Actor parameters must complete actor targets"
    );
}

#[test]
fn color_property_offers_hex_color_constructor() {
    let items = value_for_property(
        &Some("color".to_string()),
        Some("Rect"),
        &SymbolTable::build_from_ast(&[]),
    );
    assert!(items.iter().any(|item| {
        item.label == "hex_color(...)"
            && item.insert_text.as_deref() == Some(r#"hex_color("${1:fff1d6}")"#)
    }));
}

#[test]
fn top_level_completions_include_all_builtin_kinds() {
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

    let types: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Type)
        .map(|i| i.label.as_str())
        .collect();

    assert!(types.contains(&"Text"));
    assert!(types.contains(&"Ellipse"));
    assert!(types.contains(&"Button"));

    let snippets: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Snippet)
        .map(|i| i.label.as_str())
        .collect();

    assert!(snippets.contains(&"actor"));
    assert!(snippets.contains(&"keyframe"));
    assert!(snippets.contains(&"component"));

    let callables: Vec<_> = items
        .iter()
        .filter(|i| i.kind == CompletionKind::Callable)
        .map(|i| i.label.as_str())
        .collect();

    assert!(callables.contains(&"fade_in"));
    assert!(callables.contains(&"move"));
    assert!(callables.contains(&"scale"));
}

#[test]
fn role_completion_follows_the_actor_family() {
    let source = "#0s\ncurve: PlotCurve, func: (x) => x, role: ";
    let tokens = animatix_syntax::token::tokenize(source);
    let symbols = SymbolTable::build_from_ast(&[]);
    let items = completions_at(
        &symbols,
        &tokens,
        source,
        1,
        source.lines().last().unwrap().len(),
        &ExtensionManifest::default(),
    );
    let choices = items
        .iter()
        .filter(|item| item.kind == CompletionKind::Value)
        .map(|item| item.label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(choices, ["reference", "focus"]);
}
