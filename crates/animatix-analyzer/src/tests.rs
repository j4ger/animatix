use std::path::{Path, PathBuf};

use super::*;

#[test]
fn stage_param_assignment_is_not_checked_against_host_properties() {
    // `panel.pix.size` addresses an effect-stage parameter. The stage's
    // effect type (`Pixelate`) is unknown here (a plugin effect), and its
    // `size` (Num) must not be compared against the host `Filter.size`
    // (Vec2) — that produced false type-mismatch warnings.
    let source = r#"panel: Filter, size: (100, 200) {
  pix: Pixelate, size: 6
}
box: Rect, size: 42
#0s
fade-in box [1s]
"#;
    let analyzer = Analyzer::new(source);
    let mismatches: Vec<String> = analyzer
        .diagnostics()
        .into_iter()
        .filter(|d| d.code.as_deref() == Some("type-mismatch"))
        .map(|d| d.message)
        .collect();
    assert!(
        !mismatches.iter().any(|m| m.contains("panel.size")),
        "stage parameter must not be validated against the host: {mismatches:?}"
    );
    assert!(
        mismatches.iter().any(|m| m.contains("Rect.size")),
        "a genuine host property mismatch must still be reported: {mismatches:?}"
    );
}

#[test]
fn analyzer_parses_valid_source() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
    position: (400, 300),
}
"#;
    let analyzer = Analyzer::new(source);
    assert!(analyzer.ast().is_some());
    assert!(analyzer.parse_errors().is_empty());
}

#[test]
fn analyzer_handles_parse_errors() {
    let source = "this is not valid @@@ syntax";
    let analyzer = Analyzer::new(source);
    assert!(!analyzer.parse_errors().is_empty());
}

#[test]
fn transform_is_known_actor_property() {
    let source = r#"
config { colorscheme: "editorial-dark", resolution: (640, 360) }
a: Rect, size: (100, 100), transform: (1, 0.5, 0, 1, 0, 0), color: accent.primary, at: (200, 150)
"#;
    let analyzer = Analyzer::new(source);

    let unknown_properties: Vec<_> = analyzer
        .diagnostics()
        .into_iter()
        .filter(|d| d.code.as_deref() == Some("unknown-property"))
        .collect();
    assert!(
        unknown_properties.is_empty(),
        "transform should be a known actor property: {unknown_properties:?}"
    );
}

#[test]
fn extension_manifest_suppresses_unknown_extension_symbols() {
    let source = "g: Gauge, level: 42";
    let manifest = ExtensionManifest::from_toml(
        r#"
[[primitives]]
type_name = "Gauge"

[[properties]]
actor_type = "Gauge"
name = "level"
type = "Num"
"#,
    )
    .expect("parse manifest");
    let analyzer = Analyzer::new(source).with_extension_manifest(manifest);

    assert!(analyzer.symbols().types.contains("Gauge"));
    assert!(analyzer.symbols().properties.get("Gauge").is_some_and(|props| {
        props.contains(&"level".to_string()) && props.contains(&"opacity".to_string())
    }));
    let diagnostics = analyzer.diagnostics();
    assert!(
        !diagnostics.iter().any(|d| d.code.as_deref() == Some("unknown-type")),
        "manifest primitive should suppress unknown-type"
    );
    assert!(
        !diagnostics.iter().any(|d| d.code.as_deref() == Some("unknown-property")),
        "manifest property should suppress unknown-property"
    );
}

#[test]
fn extension_manifest_powers_hover_and_completions() {
    let source = "g: Gauge, level: 42";
    let manifest = ExtensionManifest::from_toml(
        r#"
[[primitives]]
type_name = "Gauge"
display_name = "Gauge Dial"
category = "Plot"

[[properties]]
actor_type = "Gauge"
name = "level"
type = "Num"
"#,
    )
    .expect("parse manifest");
    let analyzer = Analyzer::new(source).with_extension_manifest(manifest);

    let type_hover = analyzer.hover_at(0, 3).expect("hover on Gauge");
    assert!(type_hover.contents.contains("Gauge Dial (Plots)"));

    let property_hover = analyzer.hover_at(0, 10).expect("hover on level");
    assert!(property_hover.contents.contains("Type: Num"));
    assert!(property_hover.contents.contains("Extensions: Gauge"));

    let completions = analyzer.completions_at(0, 3);
    let gauge = completions.iter().find(|item| item.label == "Gauge").expect("Gauge completion");
    assert_eq!(gauge.detail.as_deref(), Some("Plots"));
    assert_eq!(gauge.documentation.as_deref(), Some("Gauge Dial (Plots)"));
}

#[test]
fn analyzer_extracts_symbols() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
btn: Button {
    text: "Click",
}
"#;
    let analyzer = Analyzer::new(source);
    let symbols = analyzer.symbols();

    assert!(symbols.labels.contains_key("title"));
    assert!(symbols.labels.contains_key("btn"));
    assert_eq!(symbols.labels["title"].ty.as_deref(), Some("Text"));
    assert_eq!(symbols.labels["btn"].ty.as_deref(), Some("Button"));
}

#[test]
fn analyzer_update_is_noop_on_same_source() {
    let source = "title: Text {}";
    let mut analyzer = Analyzer::new(source);
    let symbols_before = analyzer.symbols().labels.len();

    analyzer.update(source);
    assert_eq!(analyzer.symbols().labels.len(), symbols_before);
}

#[test]
fn workspace_resolves_cross_file_symbols() {
    let mut workspace = Workspace::new();

    let lib_source = r#"
let accent = rgb(255, 0, 0)
btn: Button {
    text: "Click"
}
"#;
    let main_source = r#"
import "lib.amx"

title: Text {
    content: "Hello"
}
"#;

    workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
    workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

    let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));

    assert!(resolved.labels.contains_key("title"));
    assert!(resolved.labels.contains_key("accent"));
    assert!(resolved.labels.contains_key("btn"));
}

#[test]
fn analyzer_with_workspace_provides_cross_file_completions() {
    let mut workspace = Workspace::new();

    let lib_source = r#"
let shared_color = rgb(0, 255, 0)
"#;
    let main_source = r#"
import "lib.amx"

title: Text {
    content: "Hello"
}
"#;

    workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
    workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

    assert!(workspace.has_file(Path::new("/project/lib.amx")));

    let import_path = Workspace::resolve_import_path(Path::new("/project/main.amx"), "lib.amx");
    assert_eq!(import_path, PathBuf::from("/project/lib.amx"));

    let lib_symbols = workspace.file_symbols(Path::new("/project/lib.amx")).unwrap();
    assert!(lib_symbols.labels.contains_key("shared_color"), "lib should have shared_color ");

    let analyzer = Analyzer::new_with_path(main_source, Some(PathBuf::from("/project/main.amx")));

    // Verify local symbols are available
    let symbols = analyzer.symbols();
    assert!(symbols.labels.contains_key("title"), "main should have title from local source");

    // Verify workspace resolves cross-file symbols
    let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));
    assert!(
        resolved.labels.contains_key("shared_color"),
        "workspace should resolve shared_color from import"
    );
}

#[test]
fn workspace_resolves_aliased_imports_to_namespaces() {
    let mut workspace = Workspace::new();

    let lib_source = r#"
pub let accent = rgb(255, 0, 0)
pub component Button(text: "Click") {}
"#;
    let main_source = r#"
import "lib.amx" as lib

title: Text {
    content: "Hello"
}
"#;

    workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
    workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

    let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));

    // Local symbols should be in the global namespace
    assert!(resolved.labels.contains_key("title"));

    // Imported symbols should NOT be in the global namespace (aliased)
    assert!(!resolved.labels.contains_key("accent"));

    // Aliased imports expose only pub lets, pub components, pub types, scenes.
    let lib_ns = &resolved.namespaces["lib"];
    assert!(lib_ns.labels.contains_key("accent"));
    assert!(lib_ns.components.contains_key("Button"));
    assert!(!lib_ns.labels.contains_key("btn"));

    // Namespace-qualified lookup should work for exports only.
    assert!(resolved.resolve_namespaced_label("lib.accent").is_some());
    assert!(resolved.resolve_namespaced_component("lib.Button").is_some());
    assert!(resolved.resolve_namespaced_label("lib.btn").is_none());
    assert!(resolved.resolve_namespaced_label("lib.unknown").is_none());
}

#[test]
fn workspace_aliased_namespace_only_exposes_pub_symbols() {
    let mut workspace = Workspace::new();

    let lib_source = r#"
let private_value = 1
pub let shared_value = 2
component InternalCard {}
pub component ExternalCard {}
"#;
    let main_source = r#"
import "lib.amx" as lib

title: Text {
    content: "Hello"
}
"#;

    workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
    workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

    let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));
    let lib_ns = &resolved.namespaces["lib"];

    assert!(lib_ns.labels.contains_key("shared_value"));
    assert!(!lib_ns.labels.contains_key("private_value"));
    assert!(lib_ns.components.contains_key("ExternalCard"));
    assert!(!lib_ns.components.contains_key("InternalCard"));
}

#[test]
fn workspace_resolves_nested_namespace_depth() {
    let mut workspace = Workspace::new();

    let inner_source = r#"
pub let accent = rgb(255, 0, 0)
pub component InnerButton {
    frame: Rect
}
"#;
    let lib_source = r#"
import "inner.amx" as inner
"#;
    let main_source = r#"
import "lib.amx" as lib
"#;

    workspace.add_file(PathBuf::from("/project/inner.amx"), inner_source);
    workspace.add_file(PathBuf::from("/project/lib.amx"), lib_source);
    workspace.add_file(PathBuf::from("/project/main.amx"), main_source);

    let resolved = workspace.resolve_symbols(Path::new("/project/main.amx"));
    assert!(resolved.resolve_namespaced_label("lib.inner.accent").is_some());
    assert!(resolved.resolve_namespaced_component("lib.inner.InnerButton").is_some());
    assert!(resolved.resolve_namespaced_label("lib.inner.missing").is_none());
    let mut labels = resolved.namespace_labels("lib.inner");
    labels.sort();
    assert_eq!(labels, vec!["accent"], "component-internal labels should not be exported");
    assert_eq!(resolved.namespace_components("lib.inner"), vec!["InnerButton"]);
}

#[test]
fn workspace_import_path_resolution() {
    let base = Path::new("/project/src/main.amx");
    let resolved = Workspace::resolve_import_path(base, "../lib.amx");
    assert!(resolved.to_string_lossy().contains("lib.amx"));
    assert_eq!(
        Workspace::resolve_import_path(base, "utils.amx"),
        PathBuf::from("/project/src/utils.amx")
    );
}

// ── Hover tests ──

#[test]
fn hover_on_actor_label_returns_label_info() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    // "title" at line 2, col 0 (0-based, after leading newline)
    let info = analyzer.hover_at(2, 0);
    assert!(info.is_some());
    let info = info.unwrap();
    assert!(info.contents.contains("**Actor**"));
    assert!(info.contents.contains("title"));
    assert!(info.contents.contains("Text"));
    assert!(info.range.is_some());
}

#[test]
fn hover_on_type_returns_type_documentation() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    // "Text" at line 2, col 7
    let info = analyzer.hover_at(2, 7);
    assert!(info.is_some());
    let info = info.unwrap();
    assert!(info.contents.contains("**Type**"));
    assert!(info.contents.contains("Text"));
    assert!(info.contents.contains("element"));
}

#[test]
fn hover_builtin_actions_in_symbol_table() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);
    let symbols = analyzer.symbols();
    assert!(symbols.actions.contains("fade-in"));
    assert!(symbols.actions.contains("move"));
    assert!(symbols.actions.contains("rotate"));
}

// ── Go-to-definition tests ──

#[test]
fn definition_at_returns_location_for_label() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
always {
    title.content = "World"
}
"#;
    let analyzer = Analyzer::new(source);

    // Click on "title" in always block (line 6, col 4)
    let loc = analyzer.definition_at(None, 6, 4);
    assert!(loc.is_some());
    let loc = loc.unwrap();
    assert!(loc.file.is_none(), "definition in same file ");
    // Declaration "title:" on line 2 (0-based); Location is 0-based.
    assert_eq!(loc.line, 2);
    assert_eq!(loc.col, 0);
}

#[test]
fn definition_at_on_colon_returns_none() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    // Position on colon ':' at line 2, col 5
    let loc = analyzer.definition_at(None, 2, 5);
    assert!(loc.is_none());
}

#[test]
fn definition_at_out_of_bounds_returns_none() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    let loc = analyzer.definition_at(None, 999, 999);
    assert!(loc.is_none());
}

// ── Find references tests ──

#[test]
fn find_references_finds_all_occurrences() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
always {
    title.content = "World"
}
"#;
    let analyzer = Analyzer::new(source);

    let refs = analyzer.find_references("title");
    // At least 2: declaration (line 2) + reference (line 6)
    assert!(refs.len() >= 2, "expected >= 2 references for title, got {}", refs.len());

    for (sl, _sc, el, _ec) in &refs {
        assert!(*sl <= *el);
    }
}

#[test]
fn find_references_empty_for_unknown() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    let refs = analyzer.find_references("nonexistent");
    assert!(refs.is_empty());
}

#[test]
fn find_references_empty_for_empty_source() {
    let analyzer = Analyzer::new("");
    let refs = analyzer.find_references("anything");
    assert!(refs.is_empty());
}

#[test]
fn find_references_resolves_block_for_and_closure_shadowing() {
    let source = r#"let shared = 1
always {
    let shared = 2
    inner = shared
}
for shared in items {
    loop_ref = shared
}
let mapper = (shared) => shared
outer = shared
"#;
    let analyzer = Analyzer::new(source);

    let inner_block_refs = analyzer.find_references_at(3, 12);
    assert_eq!(
        inner_block_refs.len(),
        2,
        "inner block binding should include its declaration and reference: {inner_block_refs:?}"
    );
    assert_eq!(inner_block_refs[0].0, 2, "inner declaration line");
    assert_eq!(inner_block_refs[1].0, 3, "inner reference line");

    let for_refs = analyzer.find_references_at(6, 15);
    assert_eq!(
        for_refs.len(),
        2,
        "for binding should include its variable and body reference: {for_refs:?}"
    );
    assert_eq!(for_refs[0].0, 5, "for variable line");
    assert_eq!(for_refs[1].0, 6, "for body reference line");

    let closure_refs = analyzer.find_references_at(8, 15);
    assert_eq!(
        closure_refs.len(),
        2,
        "closure binding should include its parameter and body reference: {closure_refs:?}"
    );
    assert_eq!(closure_refs[0].0, 8, "closure parameter line");

    let outer_refs = analyzer.find_references_at(9, 8);
    assert_eq!(
        outer_refs.len(),
        2,
        "outer binding should include its declaration and final reference: {outer_refs:?}"
    );
    assert_eq!(outer_refs[0].0, 0, "outer declaration line");
    assert_eq!(outer_refs[1].0, 9, "outer reference line");
}

// ── Document symbols tests ──

#[test]
fn document_symbols_returns_outline() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
btn: Button {
    text: "Click",
}
"#;
    let analyzer = Analyzer::new(source);
    let symbols = analyzer.document_symbols();

    assert!(symbols.iter().any(|s| s.name == "title"));
    assert!(symbols.iter().any(|s| s.name == "btn"));

    let title_sym = symbols.iter().find(|s| s.name == "title").unwrap();
    assert_eq!(title_sym.kind, SymbolKind::Actor);
    assert_eq!(title_sym.detail.as_deref(), Some("Text"));

    // "title:" is declared on 0-based line 2, col 0.
    assert_eq!(title_sym.line, 2);
    assert_eq!(title_sym.col, 0);
}

#[test]
fn document_symbols_includes_labels() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
# 1s
btn: Button {
    text: "Click",
}
"#;
    let analyzer = Analyzer::new(source);
    let symbols = analyzer.document_symbols();

    // Should include labels
    assert!(symbols.iter().any(|s| s.name == "title"));
    assert!(symbols.iter().any(|s| s.name == "btn"));

    let title_sym = symbols.iter().find(|s| s.name == "title").unwrap();
    assert_eq!(title_sym.kind, SymbolKind::Actor);
}

#[test]
fn document_symbols_sorted_by_line() {
    let source = r#"
# 0s
z_actor: Text { content: "Z", }
a_actor: Text { content: "A", }
"#;
    let analyzer = Analyzer::new(source);
    let symbols = analyzer.document_symbols();

    for i in 1..symbols.len() {
        assert!(
            symbols[i - 1].line <= symbols[i].line,
            "sort order: {} (L{}) before {} (L{})",
            symbols[i - 1].name,
            symbols[i - 1].line,
            symbols[i].name,
            symbols[i].line,
        );
    }
}

// ── Symbol at position tests ──

#[test]
fn symbol_at_returns_label_name() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    // "title" at line 2, col 0
    let sym = analyzer.symbol_at(2, 0);
    assert_eq!(sym.as_deref(), Some("title"));
}

#[test]
fn symbol_at_returns_type_name() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    // "Text" at line 2, col 7
    let sym = analyzer.symbol_at(2, 7);
    assert_eq!(sym.as_deref(), Some("Text"));
}

#[test]
fn symbol_at_on_colon_returns_none() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    // Position on colon at line 2, col 5
    let sym = analyzer.symbol_at(2, 5);
    assert!(sym.is_none());
}

#[test]
fn symbol_at_on_whitespace_returns_none() {
    let source = r#"
# 0s
title: Text {
    content: "Hello",
}
"#;
    let analyzer = Analyzer::new(source);

    // Position on space between colon and Text at line 2, col 6
    let sym = analyzer.symbol_at(2, 6);
    assert!(sym.is_none());
}

#[test]
fn symbol_at_on_empty_source_returns_none() {
    let analyzer = Analyzer::new("");
    let sym = analyzer.symbol_at(0, 0);
    assert!(sym.is_none());
}

// ── Resolver equivalence with ModuleGraph ───────────────────────────

#[test]
fn workspace_and_module_graph_agree_on_aliased_exports() {
    let mut workspace = Workspace::new();
    let mut graph = animatix_syntax::module::ModuleGraph::new();

    let colors = "/project/colors.amx";
    let theme = "/project/theme.amx";
    let main = "/project/main.amx";

    for (path, source) in [
        (
            colors,
            r#"
pub let primary = (0.38, 0.78, 1.0, 1.0)
let hidden = 1
pub type Swatch = Bool | Str
pub component ColorCard(color: Color) { frame: Rect, color: color }
# SceneFromColors
#0s
box: Rect, size: (10, 10)
"#,
        ),
        (
            theme,
            r#"
import "./colors.amx" as colors
pub let accent = colors.primary
"#,
        ),
        (
            main,
            r#"
import "./theme.amx" as theme
title: Text { content: "Hello" }
"#,
        ),
    ] {
        workspace.add_file(std::path::PathBuf::from(path), source);
        graph.add_source(std::path::PathBuf::from(path), source.to_string());
    }

    let resolved = workspace.resolve_symbols(std::path::Path::new(main));
    let program = graph.load_program(std::path::Path::new(main)).expect("program loads");

    let ws_ns = resolved.namespaces.get("theme").expect("workspace theme namespace");
    let graph_ns = program.namespaces.get("theme").expect("graph theme namespace");

    let mut ws_values: Vec<&str> = ws_ns
        .labels
        .iter()
        .filter(|(_, info)| info.is_pub)
        .map(|(name, _)| name.as_str())
        .collect();
    ws_values.sort();
    let mut graph_values: Vec<&str> = graph_ns.exports.keys().map(String::as_str).collect();
    graph_values.sort();
    assert_eq!(ws_values, graph_values, "pub let export sets should agree");

    let mut ws_aliases: Vec<&str> = ws_ns.type_aliases.keys().map(String::as_str).collect();
    ws_aliases.sort();
    let mut graph_aliases: Vec<&str> = graph_ns.type_exports.keys().map(String::as_str).collect();
    graph_aliases.sort();
    assert_eq!(ws_aliases, graph_aliases, "pub type export sets should agree");

    let mut ws_components: Vec<&str> = ws_ns.components.keys().map(String::as_str).collect();
    ws_components.sort();
    let mut graph_components: Vec<&str> =
        graph_ns.component_exports.keys().map(String::as_str).collect();
    graph_components.sort();
    assert_eq!(ws_components, graph_components, "pub component export sets should agree");

    let mut ws_scenes: Vec<&str> = ws_ns.scenes.keys().map(String::as_str).collect();
    ws_scenes.sort();
    let mut graph_scenes: Vec<&str> = graph_ns.scenes.keys().map(String::as_str).collect();
    graph_scenes.sort();
    assert_eq!(ws_scenes, graph_scenes, "scene export sets should agree");

    let mut ws_nested: Vec<&str> = ws_ns.namespaces.keys().map(String::as_str).collect();
    ws_nested.sort();
    let mut graph_nested: Vec<&str> = graph_ns.namespaces.keys().map(String::as_str).collect();
    graph_nested.sort();
    assert_eq!(ws_nested, graph_nested, "nested namespace sets should agree");
}

#[test]
fn workspace_and_module_graph_agree_on_direct_import_exports() {
    let mut workspace = Workspace::new();
    let mut graph = animatix_syntax::module::ModuleGraph::new();

    let lib = "/project/lib.amx";
    let main = "/project/main.amx";

    for (path, source) in [
        (
            lib,
            r#"
pub component PublicCard {}
component PrivateCard {}
# SceneInLib
#0s
box: Rect, size: (10, 10)
"#,
        ),
        (
            main,
            r#"
import "./lib.amx"
title: Text { content: "Hello" }
"#,
        ),
    ] {
        workspace.add_file(std::path::PathBuf::from(path), source);
        graph.add_source(std::path::PathBuf::from(path), source.to_string());
    }

    let resolved = workspace.resolve_symbols(std::path::Path::new(main));
    let program = graph.load_program(std::path::Path::new(main)).expect("program loads");

    let mut ws_components: Vec<&str> = resolved.components.keys().map(String::as_str).collect();
    ws_components.sort();
    let mut graph_components: Vec<&str> = program.components.keys().map(String::as_str).collect();
    graph_components.sort();
    assert_eq!(
        ws_components, graph_components,
        "direct imports should expose the same component set (pub-only)"
    );

    let mut ws_scenes: Vec<&str> = resolved.scenes.keys().map(String::as_str).collect();
    ws_scenes.sort();
    let mut graph_scenes: Vec<&str> = program
        .namespaces
        .values()
        .flat_map(|ns| ns.scenes.keys().map(String::as_str).collect::<Vec<_>>())
        .collect();
    graph_scenes.sort();
    // The direct import is flattened, so its scene is part of the loaded
    // statements rather than a namespace export. Compare by checking the
    // workspace scene set contains it and ModuleGraph still carries it.
    assert!(
        ws_scenes.contains(&"SceneInLib"),
        "workspace should include the direct-import scene"
    );
    assert_eq!(graph_scenes, Vec::<&str>::new(), "direct scene is not a namespace export");
}
#[test]
fn diagnostics_cache_returns_identical_results() {
    let source = "title: Text, text: \"hi\"\ntitle: Text, text: \"dup\"\n";
    let analyzer = Analyzer::new(source);
    let first = analyzer.diagnostics();
    let second = analyzer.diagnostics();
    assert_eq!(first.len(), second.len(), "cached call matches the first");
    for (a, b) in first.iter().zip(second.iter()) {
        assert_eq!(a.code, b.code);
        assert_eq!(a.line, b.line);
        assert_eq!(a.col, b.col);
        assert_eq!(a.message, b.message);
    }
    assert!(!first.is_empty(), "fixture must produce diagnostics");
}

#[test]
fn diagnostics_cache_invalidates_on_update() {
    let mut analyzer = Analyzer::new("title: Text, text: \"hi\"\n");
    let before = analyzer
        .diagnostics()
        .into_iter()
        .filter(|d| d.code.as_deref() == Some("duplicate-label"))
        .count();
    assert_eq!(before, 0);

    // Introducing a duplicate must show up on the next query.
    analyzer.update("title: Text, text: \"hi\"\ntitle: Text, text: \"dup\"\n");
    let after = analyzer
        .diagnostics()
        .into_iter()
        .filter(|d| d.code.as_deref() == Some("duplicate-label"))
        .count();
    assert_eq!(after, 1, "cache must not serve the pre-edit result");
}

#[test]
fn diagnostics_cache_is_keyed_by_lint_config() {
    let source = "title: Text, text: \"hi\"\n";
    let analyzer = Analyzer::new(source);
    let mut config = crate::LintConfig::default();
    let unfiltered = analyzer.diagnostics_with_config(&config).len();

    config.disabled.insert("unused-label".to_string());
    let filtered = analyzer.diagnostics_with_config(&config).len();
    assert!(filtered < unfiltered, "disabled code must drop from the result");

    // Asking again with the first config must not return the filtered set.
    let back = analyzer.diagnostics_with_config(&crate::LintConfig::default()).len();
    assert_eq!(back, unfiltered, "config change invalidates the cache");
}

#[test]
fn diagnostics_cache_invalidates_on_manifest_change() {
    let source = "g: Gauge, level: 42";
    let mut analyzer = Analyzer::new(source);
    let unknown_before = analyzer
        .diagnostics()
        .into_iter()
        .filter(|d| d.code.as_deref() == Some("unknown-type"))
        .count();
    assert_eq!(unknown_before, 1, "Gauge is unknown without a manifest");

    let manifest = ExtensionManifest::from_toml(
            "[[primitives]]\ntype_name = \"Gauge\"\n\n[[properties]]\nactor_type = \"Gauge\"\nname = \"level\"\ntype = \"Num\"\n",
        )
        .expect("parse manifest");
    analyzer.set_extension_manifest(manifest);

    let unknown_after = analyzer
        .diagnostics()
        .into_iter()
        .filter(|d| d.code.as_deref() == Some("unknown-type"))
        .count();
    assert_eq!(unknown_after, 0, "manifest change must invalidate the cache");
}

#[test]
fn lint_config_fingerprint_is_order_independent() {
    let mut a = crate::LintConfig::default();
    a.disabled.insert("unused-label".to_string());
    a.disabled.insert("unknown-type".to_string());
    let mut b = crate::LintConfig::default();
    b.disabled.insert("unknown-type".to_string());
    b.disabled.insert("unused-label".to_string());
    assert_eq!(a.fingerprint(), b.fingerprint(), "set order must not matter");

    let mut c = crate::LintConfig::default();
    c.disabled.insert("unused-label".to_string());
    assert_ne!(a.fingerprint(), c.fingerprint(), "different codes differ");
    assert_ne!(crate::LintConfig::default().fingerprint(), c.fingerprint(),);
}
