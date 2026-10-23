use super::*;

mod tests {
    use std::path::PathBuf;

    use super::*;

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
        graph.upsert_source(
            PathBuf::from("/virtual/lib.amx"),
            "pub component Card {}\n".to_string(),
        );
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
}
