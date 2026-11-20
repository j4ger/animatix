//! Unresolved-reference detection: bare identifiers that bind to no
//! declaration, and imports that resolve to no file.
//!
//! A bare identifier read as a value (`accent = undefined_color`) that binds
//! to no lexical declaration is almost certainly a typo, and before this
//! check it was silently evaluated to a default at runtime. Import paths that
//! resolve to nothing were equally silent in the editor while the CLI build
//! failed. Both checks are deliberately conservative: names bound by imports
//! (merged into the symbol table), plot runtime parameters, and builtins
//! never flag.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use animatix_syntax::ast::Stmt;
use animatix_syntax::module::Namespace;
use animatix_syntax::occurrence::{Occurrence, OccurrenceKind};
use animatix_syntax::token::{Token, TokenKind, byte_to_line_col};
use animatix_syntax::{builtins, easing};

use crate::diagnostics::{Diagnostic, DiagnosticSeverity};
use crate::references::resolve_scope;
use crate::symbol_table::SymbolTable;

/// Collect errors for imports that resolve to no file.
///
/// The check needs the document path (imports resolve relative to it) and
/// only fires on imports whose string span was recorded, so callers that
/// never call `merge_import_symbols`/position enrichment see no diagnostics
/// rather than wrong ones.
pub fn collect_unresolved_imports(symbols: &SymbolTable, path: Option<&Path>) -> Vec<Diagnostic> {
    let Some(path) = path else {
        return Vec::new();
    };
    let mut diagnostics = Vec::new();
    for import in &symbols.imports {
        let Some(span) = import.span else {
            continue;
        };
        let resolved = crate::Workspace::resolve_import_path(path, &import.path);
        if resolved.exists() {
            continue;
        }
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Error,
            line: span.start_line.saturating_sub(1),
            col: span.start_col.saturating_sub(1),
            end_line: span.end_line.saturating_sub(1),
            end_col: span.end_col.saturating_sub(1),
            message: format!(
                "Imported file '{}' not found (resolved to {})",
                import.path,
                resolved.display()
            ),
            code: Some("unresolved-import".into()),
            subject: Some(import.path.clone().into()),
        });
    }
    diagnostics
}

/// Collect warnings for `Variable`-kind references that bind to no
/// declaration in their lexical scope chain.
pub fn collect_unresolved_variables(
    stmts: &[Stmt],
    occurrences: &[Occurrence],
    symbols: &SymbolTable,
    tokens: &[Token],
    source: &str,
    imported_namespaces: Option<&HashMap<String, Namespace>>,
) -> Vec<Diagnostic> {
    let local_exempt = document_exempt_names(stmts, symbols);
    let assignment_targets = collect_assignment_target_names(stmts);
    let mut diagnostics = Vec::new();

    for occurrence in occurrences {
        if occurrence.kind != OccurrenceKind::Variable || occurrence.declaration {
            continue;
        }
        if STATIC_EXEMPT.contains(occurrence.name.as_str())
            || local_exempt.contains(occurrence.name.as_str())
        {
            continue;
        }
        // A declaration of this name in the occurrence's own scope chain
        // (let/for/closure/parameter, any kind) binds the reference.
        if resolve_scope(occurrences, occurrence).is_some() {
            continue;
        }
        if is_known_callable_namespace_qualifier(tokens, occurrence, symbols) {
            continue;
        }
        if is_resolved_qualified_value(tokens, occurrence, symbols, imported_namespaces) {
            continue;
        }
        // Last resort: the name exists anywhere in this file or its merged
        // imports (actors, lets, components, scenes), or is bound by a bare
        // assignment target (`name = value` acts as a let). File-global, so
        // it can mask a same-named typo in an unrelated scope, but never
        // flags correct code.
        if symbols.labels.contains_key(&occurrence.name)
            || symbols.components.contains_key(&occurrence.name)
            || symbols.scenes.contains_key(&occurrence.name)
            || assignment_targets.contains(&occurrence.name)
        {
            continue;
        }
        // A bare identifier immediately after `name:` is a keyword-argument
        // value (`ease: ease_in_out`, `place: bottom`), and a bare identifier
        // immediately before `:` is a keyword-argument name (`[intensity:
        // 1.05]`) — modifier/enum vocabulary, not variable reads.
        if is_keyword_argument_token(tokens, occurrence) {
            continue;
        }

        let (start_line, start_col) = byte_to_line_col(source, occurrence.span.start);
        let (end_line, end_col) = byte_to_line_col(source, occurrence.span.end);
        diagnostics.push(Diagnostic {
            severity: DiagnosticSeverity::Warning,
            line: start_line,
            col: start_col,
            end_line,
            end_col,
            message: format!(
                "Unresolved name '{}' — no binding in scope; references like this evaluate to a \
                 default at runtime",
                occurrence.name
            ),
            code: Some("unresolved-variable".into()),
            subject: Some(occurrence.name.clone().into()),
        });
    }

    diagnostics
}

/// The document-independent half of the exemption set: standard callable
/// names, colors, time variables, easing curves, transition ids used bare in
/// modifier lists, and anchor/alignment enums.
///
/// Built once per process on purpose. This pass re-runs on every keystroke, and
/// the whole set is vocabulary the language declares — nothing in it can depend
/// on the file being edited, so building it per update allocated ~200 `String`s
/// to answer a constant question.
static STATIC_EXEMPT: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let mut exempt: HashSet<String> = [builtins::COLOR_NAMES, builtins::COLOR_NAMESPACES]
        .concat()
        .into_iter()
        .map(String::from)
        .collect();
    for callable in animatix_core::builtins::callable::callables() {
        exempt.insert(callable.name.clone());
        exempt.insert(callable.qualified_name());
    }
    for name in [
        "t",
        "pi",
        "tau",
        "self",
        "scene",
        // The scene dimensions, readable from `always` and reactive expressions.
        "scene_width",
        "scene_height",
        // The scene camera — a reserved assignment target, not a declared actor.
        "camera",
        "auto",
        "step",
        // Anchor / alignment enum values.
        "center",
        "top",
        "bottom",
        "left",
        "right",
        // Layout fill mode and blend-mode enum values.
        "fill",
        "normal",
        "difference",
        "multiply",
        "screen",
        "overlay",
    ] {
        exempt.insert(name.to_string());
    }
    // Canonical easing identifiers are snake_case.
    for (key, _) in easing::EASING_REGISTRY {
        exempt.insert(key.to_string());
    }
    exempt.insert("linear".to_string());
    // Transition ids used bare inside modifier lists (`[fade, 300ms]`).
    exempt.extend(animatix_syntax::transition_registry::all_ids().iter().map(|s| (*s).to_string()));
    exempt
});

/// The half of the exemption set that depends on *this* document: every
/// primitive type name (a module can add one) and the plot-family runtime
/// parameters its `func` closures inject.
fn document_exempt_names(stmts: &[Stmt], symbols: &SymbolTable) -> HashSet<String> {
    // Colorscheme tokens (`accent.primary` paths are not bare idents, but a
    // scheme may expose bare names through expressions) and every primitive
    // type name.
    let mut exempt: HashSet<String> = symbols.types.iter().cloned().collect();
    // Plot-family runtime parameters (`freq: 2` on `func: (x) => sin(freq*x)`).
    collect_plot_params(stmts, &mut exempt);
    exempt
}

/// True when the occurrence's identifier sits in a keyword-argument position:
/// immediately preceded by `identifier:` (the value, `ease: ease_in_out`) or
/// immediately followed by `:` (the name, `[intensity: 1.05]`).
fn is_keyword_argument_token(tokens: &[Token], occurrence: &Occurrence) -> bool {
    let Some(index) = tokens.iter().position(|t| t.span.start == occurrence.span.start) else {
        return false;
    };
    let preceded_by_name_colon = index >= 2
        && matches!(tokens[index - 1].kind, TokenKind::Colon)
        && matches!(tokens[index - 2].kind, TokenKind::Ident(_));
    let followed_by_colon =
        tokens.get(index + 1).is_some_and(|next| matches!(next.kind, TokenKind::Colon));
    preceded_by_name_colon || followed_by_colon
}

/// A qualified imported value is represented by a variable occurrence for its
/// final segment (`palette::primary`). Resolve the whole path against imported
/// namespaces before treating that segment as an unbound bare variable.
fn is_resolved_qualified_value(
    tokens: &[Token],
    occurrence: &Occurrence,
    symbols: &SymbolTable,
    imported_namespaces: Option<&HashMap<String, Namespace>>,
) -> bool {
    let Some(index) = tokens.iter().position(|token| token.span.start == occurrence.span.start)
    else {
        return false;
    };
    let mut start = index;
    while start >= 2
        && matches!(&tokens[start - 1].kind, TokenKind::ColonColon)
        && matches!(&tokens[start - 2].kind, TokenKind::Ident(_))
    {
        start -= 2;
    }
    if start == index {
        return false;
    }

    let path = tokens[start..=index]
        .iter()
        .filter_map(|token| match &token.kind {
            TokenKind::Ident(name) => Some(name.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("::");
    symbols.resolve_namespaced_label(&path).is_some()
        || imported_namespaces.is_some_and(|namespaces| namespace_exports_value(namespaces, &path))
}

/// A known callable namespace segment is not a variable read when it begins
/// an explicitly qualified path (`std::floor(...)`, `demo::throb(...)`). Keep
/// this tied to registered callable namespaces so unknown prefixes still
/// receive unresolved-name diagnostics.
fn is_known_callable_namespace_qualifier(
    tokens: &[Token],
    occurrence: &Occurrence,
    symbols: &SymbolTable,
) -> bool {
    let Some(index) = tokens.iter().position(|token| token.span.start == occurrence.span.start)
    else {
        return false;
    };
    let mut start = index;
    while start >= 2
        && matches!(&tokens[start - 1].kind, TokenKind::ColonColon)
        && matches!(&tokens[start - 2].kind, TokenKind::Ident(_))
    {
        start -= 2;
    }

    let mut segment_index = start;
    let mut segments = Vec::new();
    let mut occurrence_segment = None;
    while let Some(Token {
        kind: TokenKind::Ident(name),
        ..
    }) = tokens.get(segment_index)
    {
        if segment_index == index {
            occurrence_segment = Some(segments.len());
        }
        segments.push(name.as_str());
        if !matches!(
            tokens.get(segment_index + 1).map(|token| &token.kind),
            Some(TokenKind::ColonColon)
        ) {
            break;
        }
        segment_index += 2;
    }

    let Some(occurrence_segment) = occurrence_segment else {
        return false;
    };
    if occurrence_segment + 1 >= segments.len() {
        return false;
    }
    let namespace = segments[..=occurrence_segment].join("::");
    let namespace_prefix = format!("{namespace}::");
    symbols.functions.iter().any(|name| name.starts_with(&namespace_prefix))
}

fn namespace_exports_value(namespaces: &HashMap<String, Namespace>, path: &str) -> bool {
    let mut parts = path.split("::");
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(mut namespace) = namespaces.get(first) else {
        return false;
    };
    let Some(mut next) = parts.next() else {
        return false;
    };
    loop {
        if let Some(segment) = parts.next() {
            let Some(nested) = namespace.namespaces.get(next) else {
                return false;
            };
            namespace = nested;
            next = segment;
        } else {
            return namespace.exports.contains_key(next);
        }
    }
}

/// Walk all actor declarations, collecting plot runtime-parameter names.
fn collect_plot_params(stmts: &[Stmt], exempt: &mut HashSet<String>) {
    animatix_syntax::walk::walk_stmts(stmts, &mut |stmt| {
        if let Stmt::ActorDecl { ty, props, .. } = stmt {
            for param in animatix_syntax::semantic_diagnostics::plot_runtime_params(ty, props) {
                exempt.insert(param);
            }
        }
    });
}

/// Names bound by bare assignments (`name = value`): the DSL treats these as
/// local bindings, but the parser stores the binding name in `property` with
/// an empty target rather than as a declaration, so collect them here.
fn collect_assignment_target_names(stmts: &[Stmt]) -> HashSet<String> {
    let mut bound = HashSet::new();
    animatix_syntax::walk::walk_stmts(stmts, &mut |stmt| {
        if let Stmt::Assignment {
            target, property, ..
        } = stmt
            && target.is_empty()
        {
            bound.insert(property.clone());
        }
    });
    bound
}

#[cfg(test)]
mod tests {
    use super::collect_unresolved_variables;
    use crate::Analyzer;
    use crate::symbol_table::SymbolTable;

    fn unresolved(source: &str) -> Vec<String> {
        Analyzer::new(source)
            .diagnostics()
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("unresolved-variable"))
            .map(|d| d.message)
            .collect()
    }

    #[test]
    fn undefined_ident_in_always_flagged() {
        let source = "always {\n  accent = undefined_color\n}\n";
        let diags = unresolved(source);
        assert_eq!(diags.len(), 1, "typo'd name must flag: {diags:?}");
        assert!(diags[0].contains("undefined_color"), "{diags:?}");
    }

    #[test]
    fn bound_and_builtin_names_do_not_flag() {
        let source = r#"let accent = rgb(1, 0, 0)
box: Rect, size: (10, 10)
#0s
fade_in(target: box, duration: 1s)
always {
  half = accent + t
  clamped = clamp(half, 0, 1)
  white_mix = lerp(0, 1, 0.5)
  named = RED
}
"#;
        assert!(
            unresolved(source).is_empty(),
            "bound/builtin names must not flag: {diags:?}",
            diags = unresolved(source)
        );
    }

    #[test]
    fn explicit_std_namespace_does_not_flag_as_an_unresolved_variable() {
        let source = "let rounded = std::floor(1.5)\n";
        let diagnostics = unresolved(source);
        assert!(diagnostics.is_empty(), "qualified standard callable flagged: {diagnostics:?}");
    }

    #[test]
    fn registered_extension_namespace_resolves_but_unknown_prefix_still_warns() {
        fn unresolved_with_function(source: &str, function: &str) -> Vec<String> {
            let parsed = animatix_syntax::parser::parse_source_complete(source);
            let statements = parsed.ast.expect("source parses");
            let mut symbols = SymbolTable::default();
            symbols.functions.insert(function.to_string());
            collect_unresolved_variables(
                &statements,
                &parsed.occurrences,
                &symbols,
                &parsed.tokens,
                source,
                None,
            )
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
        }

        assert!(
            unresolved_with_function("let value = demo::scale(2)\n", "demo::scale").is_empty(),
            "a registered callable namespace is not an unresolved variable"
        );
        let unknown = unresolved_with_function("let value = ghost::scale(2)\n", "demo::scale");
        assert_eq!(unknown.len(), 1, "unknown namespace must stay visible: {unknown:?}");
        assert!(unknown[0].contains("ghost"), "{unknown:?}");
    }

    #[test]
    fn component_params_and_plot_runtime_params_do_not_flag() {
        let source = r#"pub component ColorCard(color: Color) {
  frame: Rect, color: color
}
curve: PlotCurve, func: (x) => sin(freq * x), freq: 2
"#;
        assert!(
            unresolved(source).is_empty(),
            "component params and plot runtime params must not flag"
        );
    }

    #[test]
    fn for_and_closure_bindings_do_not_flag() {
        let source = r#"for item in {1, 2, 3} {
  x = item
}
let mapper = (v) => v + 1
always {
  y = mapper(2)
}
"#;
        assert!(unresolved(source).is_empty(), "for/closure bindings must not flag");
    }

    #[test]
    fn imported_pub_let_does_not_flag() {
        // merge_import_symbols resolves imports from disk, so stage the
        // imported module in a real temp directory.
        let dir = std::env::temp_dir().join(format!(
            "animatix-unresolved-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let lib_path = dir.join("lib.amx");
        std::fs::write(&lib_path, "pub let primary = rgb(1, 0, 0)\n").expect("write lib");

        let main_path = dir.join("main.amx");
        let source = "import \"lib.amx\"\nbox: Rect, size: (10, 10), color: primary\n";
        std::fs::write(&main_path, source).expect("write main");
        let mut analyzer = crate::Analyzer::new_with_path(source, Some(main_path.clone()));
        analyzer.merge_import_symbols();
        let diags: Vec<_> = analyzer
            .diagnostics()
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("unresolved-variable"))
            .collect();
        assert!(diags.is_empty(), "imported bindings must not flag: {diags:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn aliased_pub_let_values_resolve_by_qualified_path() {
        let dir = std::env::temp_dir().join(format!(
            "animatix-unresolved-qualified-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        let lib_path = dir.join("lib.amx");
        std::fs::write(&lib_path, "pub let primary = rgb(1, 0, 0)\n").expect("write lib");

        let main_path = dir.join("main.amx");
        let source = r#"import "lib.amx" as palette
valid: Rect, size: (10, 10), color: palette::primary
invalid: Rect, size: (10, 10), color: palette::missing
"#;
        std::fs::write(&main_path, source).expect("write main");
        let mut analyzer = crate::Analyzer::new_with_path(source, Some(main_path));
        analyzer.merge_import_symbols();
        let diags: Vec<_> = analyzer
            .diagnostics()
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("unresolved-variable"))
            .collect();
        assert_eq!(diags.len(), 1, "only missing namespace exports should warn: {diags:?}");
        assert!(diags[0].message.contains("'missing'"), "{diags:?}");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_import_file_flags_unresolved_import() {
        let dir = std::env::temp_dir().join(format!(
            "animatix-missing-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        // Only main.amx exists; lib.amx was never written.
        let main_path = dir.join("main.amx");
        let source = "import \"lib.amx\"\nbox: Rect, size: (10, 10)\n";
        std::fs::write(&main_path, source).expect("write main");
        let mut analyzer = crate::Analyzer::new_with_path(source, Some(main_path));
        analyzer.merge_import_symbols();

        let import_diags: Vec<_> = analyzer
            .diagnostics()
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("unresolved-import"))
            .collect();
        assert_eq!(import_diags.len(), 1, "missing import must error: {import_diags:?}");
        assert!(
            import_diags[0].message.contains("lib.amx"),
            "error names the missing file: {:?}",
            import_diags[0].message
        );
        // Line 0 (0-based), where the import string sits.
        assert_eq!(import_diags[0].line, 0);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn existing_import_file_does_not_flag() {
        let dir = std::env::temp_dir().join(format!(
            "animatix-ok-import-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        std::fs::write(dir.join("lib.amx"), "pub let primary = rgb(1, 0, 0)\n").expect("write lib");
        let main_path = dir.join("main.amx");
        let source = "import \"lib.amx\"\nbox: Rect, size: (10, 10)\n";
        std::fs::write(&main_path, source).expect("write main");
        let mut analyzer = crate::Analyzer::new_with_path(source, Some(main_path));
        analyzer.merge_import_symbols();

        let import_diags: Vec<_> = analyzer
            .diagnostics()
            .into_iter()
            .filter(|d| d.code.as_deref() == Some("unresolved-import"))
            .collect();
        assert!(import_diags.is_empty(), "existing import must not flag: {import_diags:?}");

        std::fs::remove_dir_all(&dir).ok();
    }
}
