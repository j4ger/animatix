use super::*;

use std::path::PathBuf;

#[test]
fn sources_only_mode_never_reads_disk() {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(PathBuf::from("/virtual/a.amx"), "pub let x = 1".to_string());
    let program = graph.load_program(Path::new("/virtual/a.amx")).expect("load in-memory");
    assert_eq!(program.statements.len(), 1);

    let missing = graph.load_program(Path::new("/definitely/missing.amx"));
    assert!(matches!(missing, Err(ModuleError::FileNotFound(_))));
}

#[test]
fn upsert_source_invalidates_only_replaced_path() {
    let mut graph = ModuleGraph::new();
    graph.upsert_source(PathBuf::from("/virtual/lib.amx"), "pub let x = 1".to_string());
    graph.upsert_source(
        PathBuf::from("/virtual/main.amx"),
        "import \"./lib.amx\"\nlet y = 2".to_string(),
    );
    let before = graph.load_program(Path::new("/virtual/main.amx")).expect("load");
    assert_eq!(before.statements.len(), 2);

    graph.upsert_source(PathBuf::from("/virtual/lib.amx"), "pub let x = 3".to_string());
    let after = graph.load_program(Path::new("/virtual/main.amx")).expect("reload");
    assert_eq!(after.statements.len(), 2);
    assert_eq!(
        graph
            .file_symbols(Path::new("/virtual/lib.amx"))
            .expect("lib symbols")
            .labels
            .len(),
        1
    );
}

#[test]
fn file_symbols_and_imports_are_available_after_load() {
    let mut graph = ModuleGraph::new();
    graph.upsert_source(PathBuf::from("/virtual/lib.amx"), "pub component Card {}\n".to_string());
    graph.upsert_source(
        PathBuf::from("/virtual/main.amx"),
        "import \"./lib.amx\"\n# Scene\n".to_string(),
    );
    graph.load_program(Path::new("/virtual/main.amx")).expect("load");

    assert!(graph.file_ast(Path::new("/virtual/main.amx")).is_some());
    assert_eq!(graph.file_imports(Path::new("/virtual/main.amx")).unwrap().len(), 1);
    assert!(
        graph
            .file_symbols(Path::new("/virtual/lib.amx"))
            .unwrap()
            .components
            .contains_key("Card")
    );
}

#[test]
fn loaded_entry_keyframes_keep_source_spans_but_imported_spans_do_not_leak() {
    let main_path = PathBuf::from("/virtual/main.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        PathBuf::from("/virtual/lib.amx"),
        "#8s\nlet imported_tick = 1\n".to_string(),
    );
    graph.upsert_source(
        main_path.clone(),
        "import \"./lib.amx\"\n#0.1s\nlet local_tick = 1\n".to_string(),
    );

    let program = graph.load_program(&main_path).expect("source should load");
    assert!(program.statements.iter().any(|stmt| matches!(
        stmt,
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(time),
            span: Some(span),
            ..
        } if (*time - 0.1).abs() < f64::EPSILON && span.start_line == 2
    )));
    assert!(program.statements.iter().any(|stmt| matches!(
        stmt,
        Stmt::Keyframe {
            time: crate::ast::Time::Seconds(time),
            span: None,
            ..
        } if (*time - 8.0).abs() < f64::EPSILON
    )));
}

#[test]
fn imported_scene_expansion_uses_its_private_component_scope() {
    let main_path = PathBuf::from("/virtual/main.amx");
    let lib_path = PathBuf::from("/virtual/lib.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(main_path.clone(), "import \"./lib.amx\" as lib\n".to_string());
    graph.upsert_source(
        lib_path,
        r#"
component Card(width: Num = 10) {
  root: Rect, size: (width, 20)
  fn pulse() { #{ root.opacity = width } }
}
# Demo
card: Card, width: 42
"#
        .to_string(),
    );
    let program = graph.load_program(&main_path).expect("modules should load");
    let scene = &program.namespaces["lib"].scenes["Demo"];

    let expanded = program
        .function_context
        .expand_scene(scene)
        .expect("scene source module scope should be available");
    assert!(expanded.statements.iter().any(|stmt| matches!(
        stmt,
        Stmt::ActorDecl { label, ty, .. } if label == "card" && ty == "Rect"
    )));
    assert!(expanded.instance_methods["card"].contains_key("pulse"));
    assert!(expanded.expanded_function_context.is_some());
}

#[test]
fn imported_scene_keeps_computed_marker_spans_for_play_timing() {
    let main_path = PathBuf::from("/virtual/main.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(main_path.clone(), "import \"./scenes.amx\" as scenes\n".to_string());
    graph.upsert_source(
        PathBuf::from("/virtual/scenes.amx"),
        r#"
let hold = 700ms
# Timed
#(hold)
  let first = 1
#+(hold)
  let second = 2
play Done
# Done
"#
        .to_string(),
    );

    let program = graph.load_program(&main_path).expect("modules should load");
    let scene = &program.namespaces["scenes"].scenes["Timed"];
    let expanded = program
        .function_context
        .expand_scene(scene)
        .expect("scene source module scope should be available");
    let computed_markers = expanded
        .statements
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::Keyframe {
                time: crate::ast::Time::Expression(_),
                span,
                ..
            }
            | Stmt::RelativeKeyframe {
                offset: crate::ast::Time::Expression(_),
                span,
                ..
            } => Some(*span),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(computed_markers.len(), 2, "both computed markers must survive expansion");
    assert!(computed_markers.iter().all(Option::is_some));
    assert!(
        expanded
            .statements
            .iter()
            .any(|stmt| matches!(stmt, Stmt::Play { scene_name, .. } if scene_name == "Done"))
    );
}

#[test]
fn returned_function_plans_keep_expanded_component_method_scope() {
    let path = PathBuf::from("/virtual/main.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        path.clone(),
        r#"
component Card(width: Num) {
  box: Rect, size: (width, 20)
  fn scaled(factor: Num) { width * factor }
  pub fn resize(factor: Num = 2) { #{ self.size = (scaled(factor), 20) } }
}
fn card(width: Num) {
  #{ local: Card, width: width
    local.resize()
  }
}
"#,
    );

    let program = graph.load_program(&path).expect("source should load");
    let expanded = program.expand_components().expect("component expansion");
    let context = expanded
        .expanded_function_context
        .as_ref()
        .expect("loaded program should carry expanded function scope");
    let template = &context.visible["card"];
    let plan = template
        .body
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::Return {
                value: Some(Expr::Timeline(plan)),
                ..
            } => Some(plan),
            _ => None,
        })
        .expect("card should return a timeline plan");

    assert!(plan.statements.iter().any(|stmt| matches!(
        stmt,
        Stmt::ActorDecl { label, ty, .. } if label == "local" && ty == "Rect"
    )));
    assert!(plan.instance_methods["local"].contains_key("resize"));
    let resize = &plan.instance_methods["local"]["resize"];
    assert!(resize.body.iter().any(|stmt| matches!(
        stmt,
        Stmt::Return {
            value: Some(Expr::Timeline(method_plan)),
            ..
        } if method_plan.statements.iter().any(|inner| matches!(
            inner,
            Stmt::Assignment { property, .. } if property == "size"
        ))
    )));
}

#[test]
fn standalone_load_keeps_local_symbols_when_import_missing() {
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(
        PathBuf::from("/virtual/main.amx"),
        "import \"./lib.amx\"\nlet local = 1".to_string(),
    );
    graph
        .load_file_standalone(Path::new("/virtual/main.amx"))
        .expect("standalone load");
    let local = graph
        .file_symbols(Path::new("/virtual/main.amx"))
        .expect("local symbols")
        .labels
        .contains_key("local");
    assert!(local, "standalone load should keep local symbols with a missing import");

    graph.upsert_source(PathBuf::from("/virtual/lib.amx"), "pub let x = 2".to_string());
    let program = graph.load_program(Path::new("/virtual/main.amx")).expect("full load");
    assert_eq!(program.statements.len(), 2);
}

#[test]
fn resolve_symbols_merges_aliased_imports() {
    let mut graph = ModuleGraph::new();
    graph.upsert_source(PathBuf::from("/virtual/lib.amx"), "pub let x = 1\n".to_string());
    graph.upsert_source(
        PathBuf::from("/virtual/main.amx"),
        "import \"./lib.amx\" as lib\nlet y = lib.x\n".to_string(),
    );
    graph.load_program(Path::new("/virtual/main.amx")).expect("load");

    let resolved = graph.resolve_symbols(Path::new("/virtual/main.amx"));
    assert!(resolved.namespaces.contains_key("lib"));
    assert!(resolved.namespaces.get("lib").is_some_and(|ns| ns.labels.contains_key("x")));
}

/// A direct import flattens the library's **labels** into the host table, so
/// the bookkeeping that makes those labels intelligible has to travel with
/// them. Without `referenced_labels` and the component/array exemptions, every
/// private name inside an imported library looks unused *in the importing
/// file* — which is how `check examples/animation/36_light_pack.amx` came to
/// report `unused-label: Unused binding: 'p'` at `1:1` for a `let` inside
/// `Ticker` that the very next line reads.
#[test]
fn resolve_symbols_carries_the_references_of_what_it_flattens() {
    let mut graph = ModuleGraph::new();
    graph.upsert_source(
            PathBuf::from("/virtual/lib.amx"),
            "pub component Ticker(to: Num = 100) {\n  value: Text, text: \"0\"\n  always {\n    let p = clamp(t / 0.9, 0, 1)\n    self.value.text = format(\"{}\", floor(to * p))\n  }\n}\n"
                .to_string(),
        );
    graph.upsert_source(
        PathBuf::from("/virtual/main.amx"),
        "import \"./lib.amx\"\nticker: Ticker, to: 42, at: (10, 10)\n".to_string(),
    );
    graph.load_program(Path::new("/virtual/main.amx")).expect("load");

    let resolved = graph.resolve_symbols(Path::new("/virtual/main.amx"));
    assert!(resolved.labels.contains_key("p"), "the library's `let` is flattened in");
    assert!(
        resolved.referenced_labels.contains("p"),
        "so its references must be flattened too, or `p` reads as dead code"
    );
    assert!(
        resolved.component_internal_labels.contains("value"),
        "a component's internal actor is not a scene actor in either file"
    );
}

#[test]
fn acyclic_component_chain_at_depth_limit_keeps_its_leaf() {
    let mut source = String::from(
        "component C0() { leaf: Ellipse, size:(10,10) }
",
    );
    for depth in 1..64 {
        source.push_str(&format!(
            "component C{depth}() {{ root: C{} }}
",
            depth - 1
        ));
    }
    source.push_str(
        "instance: C63
",
    );
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(PathBuf::from("/virtual/depth.amx"), source);
    let program = graph.load_program(Path::new("/virtual/depth.amx")).unwrap();
    let expanded = program.expand_components().expect("component expansion");
    let mut leaf_count = 0;
    crate::walk::walk_stmts(&expanded.statements, &mut |stmt| {
        if matches!(stmt, Stmt::ActorDecl { ty, .. } if ty == "Ellipse") {
            leaf_count += 1;
        }
    });
    assert_eq!(
        leaf_count, 1,
        "complete acyclic expansion must retain exactly one terminal primitive: {expanded:?}"
    );
}

#[test]
fn component_expansion_limit_and_cycle_are_not_silent_successes() {
    let mut deep = String::from(
        "component C0() { leaf: Ellipse, size:(10,10) }
",
    );
    for depth in 1..65 {
        deep.push_str(&format!(
            "component C{depth}() {{ root: C{} }}
",
            depth - 1
        ));
    }
    deep.push_str(
        "instance: C64
",
    );
    let recursive = "component Loop() { root: Loop }
instance: Loop
"
    .to_string();
    for (name, source) in [("acyclic-depth-65", deep), ("cycle", recursive)] {
        let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
        graph.upsert_source(PathBuf::from("/virtual/limit.amx"), source);
        let program = graph.load_program(Path::new("/virtual/limit.amx")).unwrap();
        let report = program.expand_components().expect_err(name);
        assert!(
            report.diagnostics.iter().any(|diagnostic| {
                diagnostic.code
                    == crate::diagnostics::DiagnosticCode::ComponentExpansionDepthExceeded
                    && diagnostic.is_error()
            }),
            "{name}: error must preserve its structured code"
        );
        let cached = report.clone();
        assert_eq!(cached.diagnostics.len(), report.diagnostics.len());
        assert!(cached.diagnostics.iter().all(|diagnostic| diagnostic.is_error()));
    }
}

#[test]
fn finite_same_component_slot_nesting_is_not_a_definition_cycle() {
    let source = r#"
component Box() {
    body: Group { @slot }
}
outer: Box {
    @body {
        inner: Box {
            @body { leaf: Ellipse, size: (10, 10) }
        }
    }
}
"#;
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(PathBuf::from("/virtual/slots.amx"), source.to_string());
    let program = graph.load_program(Path::new("/virtual/slots.amx")).unwrap();
    let expanded = program.expand_components().expect("component expansion");
    struct Leaves(usize);
    impl<'ast> crate::visit::AstVisitor<'ast> for Leaves {
        fn visit_stmt(&mut self, stmt: &'ast Stmt) {
            if matches!(stmt, Stmt::ActorDecl { ty, .. } if ty == "Ellipse") {
                self.0 += 1;
            }
            crate::visit::walk_stmt(self, stmt);
        }
        fn visit_inline_item(&mut self, item: &'ast crate::ast::InlineItem) {
            if matches!(item,
                crate::ast::InlineItem::Labeled { ty, .. }
                | crate::ast::InlineItem::Anonymous { ty, .. } if ty == "Ellipse")
            {
                self.0 += 1;
            }
            crate::visit::walk_inline_item(self, item);
        }
    }
    let mut leaves = Leaves(0);
    for stmt in &expanded.statements {
        crate::visit::AstVisitor::visit_stmt(&mut leaves, stmt);
    }
    assert_eq!(leaves.0, 1, "expanded slot tree: {expanded:?}");
}

#[test]
fn failed_embedded_plan_has_one_diagnostic_across_function_context_copies() {
    let source = "component Loop() { root: Loop }\nfn broken() { return #{ instance: Loop } }\n";
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(PathBuf::from("/virtual/plan.amx"), source.to_string());
    let program = graph.load_program(Path::new("/virtual/plan.amx")).unwrap();
    let report = program.expand_components().expect_err("recursive embedded plan");
    assert_eq!(report.diagnostics.len(), 1);
    assert_eq!(
        report.diagnostics[0].code,
        crate::diagnostics::DiagnosticCode::ComponentExpansionDepthExceeded
    );
}

#[test]
fn imported_scene_expansion_returns_structured_failure_without_output() {
    let main = PathBuf::from("/virtual/depth-main.amx");
    let lib = PathBuf::from("/virtual/depth-lib.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(main.clone(), "import \"./depth-lib.amx\" as lib\n");
    graph.upsert_source(lib.clone(), "component Loop() { root: Loop }\n# Demo\ninstance: Loop\n");
    let program = graph.load_program(&main).unwrap();
    let scene = &program.namespaces["lib"].scenes["Demo"];
    let report = program.function_context.expand_scene(scene).expect_err("recursive scene");
    assert_eq!(report.diagnostics.len(), 1);
    assert!(report.diagnostics[0].is_error());
    assert_eq!(report.diagnostics[0].location.path.as_ref(), Some(&lib));
}

#[test]
fn identical_expansion_failures_in_distinct_files_keep_both_sources() {
    let main = PathBuf::from("/virtual/two-errors.amx");
    let a = PathBuf::from("/virtual/a.amx");
    let b = PathBuf::from("/virtual/b.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(main.clone(), "import \"./a.amx\" as a\nimport \"./b.amx\" as b\n");
    let source = "component Loop() { root: Loop }\nfn broken() { return #{ instance: Loop } }\n";
    graph.upsert_source(a.clone(), source);
    graph.upsert_source(b.clone(), source);
    let program = graph.load_program(&main).unwrap();
    let report = program.expand_components().expect_err("recursive imported plans");
    assert_eq!(report.diagnostics.len(), 2, "{:?}", report.diagnostics);
    for path in [&a, &b] {
        assert!(
            report
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.location.path.as_ref() == Some(path))
        );
    }
    assert_eq!(report.diagnostics[0].message, report.diagnostics[1].message);
}

#[test]
fn empty_component_expansion_is_success_not_an_error_sentinel() {
    let expanded = LoadedProgram::default().expand_components().expect("empty program is valid");
    assert!(expanded.statements.is_empty());
    assert!(expanded.instance_methods.is_empty());
}

#[test]
fn imported_scene_missing_scope_returns_a_structured_error() {
    let main = PathBuf::from("/virtual/missing-scope-main.amx");
    let library = PathBuf::from("/virtual/missing-scope-lib.amx");
    let mut graph = ModuleGraph::new().with_source_access(SourceAccess::SourcesOnly);
    graph.upsert_source(main.clone(), "import \"./missing-scope-lib.amx\" as lib\n");
    graph.upsert_source(library.clone(), "# Demo\nleaf: Rect\n");
    let program = graph.load_program(&main).unwrap();
    let scene = &program.namespaces["lib"].scenes["Demo"];
    let error = FunctionContext::default()
        .expand_scene(scene)
        .expect_err("missing module scope");
    assert_eq!(error.diagnostics.len(), 1);
    let diagnostic = &error.diagnostics[0];
    assert_eq!(diagnostic.code, crate::diagnostics::DiagnosticCode::MissingExpansionScope);
    assert!(diagnostic.is_error());
    assert_eq!(diagnostic.location.path.as_ref(), Some(&library));
}
