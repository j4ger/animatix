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

use std::collections::HashSet;
use std::path::Path;

use animatix_syntax::ast::Stmt;
use animatix_syntax::builtins;
use animatix_syntax::easing;
use animatix_syntax::occurrence::{Occurrence, OccurrenceKind};
use animatix_syntax::token::{Token, TokenKind, byte_to_line_col};

use crate::diagnostics::{Diagnostic, DiagnosticSeverity};
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
            code: Some("unresolved-import".to_string()),
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
) -> Vec<Diagnostic> {
    let exempt = exempt_names(stmts, symbols);
    let assignment_targets = collect_assignment_target_names(stmts);
    let mut diagnostics = Vec::new();

    for occurrence in occurrences {
        if occurrence.kind != OccurrenceKind::Variable || occurrence.declaration {
            continue;
        }
        if exempt.contains(occurrence.name.as_str()) {
            continue;
        }
        // A declaration of this name in the occurrence's own scope chain
        // (let/for/closure/parameter, any kind) binds the reference.
        if resolve_scope(occurrences, occurrence).is_some() {
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
        // value (`ease: ease-in-out`, `place: bottom`), and a bare identifier
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
            code: Some("unresolved-variable".to_string()),
        });
    }

    diagnostics
}

/// Names the check must never flag: builtins (functions, colors, the time
/// variables), easing curves, transition ids, anchor enums, action particles,
/// and plot runtime parameters injected into `func` closures.
fn exempt_names(stmts: &[Stmt], symbols: &SymbolTable) -> HashSet<String> {
    let mut exempt: HashSet<String> = [
        builtins::MATH_FUNCTIONS,
        builtins::COLOR_CONSTRUCTOR_FUNCTIONS,
        builtins::FORMAT_FUNCTIONS,
        builtins::COLOR_NAMES,
        builtins::COLOR_NAMESPACES,
    ]
    .concat()
    .into_iter()
    .map(String::from)
    .collect();
    for name in [
        "t",
        "pi",
        "tau",
        "self",
        "scene",
        "auto",
        "step",
        // Action particles (`move target to (x, y)`, `rotate target by 90`).
        "to",
        "by",
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
    // Easing names in both hyphenated (`ease-in-out`) and registry
    // (`easeinout`) spellings.
    for (key, _) in easing::EASING_REGISTRY {
        exempt.insert(key.to_string());
    }
    for name in [
        "linear",
        "ease-in",
        "ease-out",
        "ease-in-out",
        "easein",
        "easeout",
        "easeinout",
    ] {
        exempt.insert(name.to_string());
    }
    // Transition ids used bare inside modifier lists (`[fade, 300ms]`).
    exempt.extend(animatix_syntax::transition_registry::all_ids().iter().map(|s| (*s).to_string()));
    // Colorscheme tokens (`accent.primary` paths are not bare idents, but a
    // scheme may expose bare names through expressions) and every primitive
    // type name.
    exempt.extend(symbols.types.iter().cloned());

    // Plot-family runtime parameters (`freq: 2` on `func: (x) => sin(freq*x)`).
    collect_plot_params(stmts, &mut exempt);
    exempt
}

/// True when the occurrence's identifier sits in a keyword-argument position:
/// immediately preceded by `identifier:` (the value, `ease: ease-in-out`) or
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
        {
            if target.is_empty() {
                bound.insert(property.clone());
            }
        }
    });
    bound
}

/// Find the scope that declares `target.name`, walking outward through the
/// parser-recorded scope chain (same resolution as find-references).
fn resolve_scope(occurrences: &[Occurrence], target: &Occurrence) -> Option<u32> {
    let mut scope = target.scope_id;
    let mut parent = target.parent_scope_id;
    while let Some(current) = scope {
        if occurrences
            .iter()
            .any(|o| o.declaration && o.scope_id == Some(current) && o.name == target.name)
        {
            return Some(current);
        }
        scope = parent;
        parent = occurrences.iter().find(|o| o.scope_id == scope).and_then(|o| o.parent_scope_id);
    }
    None
}

#[cfg(test)]
mod tests {
    use crate::Analyzer;

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
fade-in box [1s]
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
