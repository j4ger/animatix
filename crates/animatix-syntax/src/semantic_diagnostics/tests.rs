use super::*;

mod tests {
    use super::*;
    use crate::ast::Expr;

    fn unused_labels(stmts: &[Stmt]) -> Vec<String> {
        let symbols = SymbolTable::build_from_ast(stmts);
        collect_semantic_diagnostics(stmts, &symbols, &[], "")
            .into_iter()
            .filter(|d| d.code == DiagnosticCode::UnusedLabel)
            .map(|d| d.message.clone())
            .collect()
    }

    fn unused_label_severity(stmts: &[Stmt], name: &str) -> Option<DiagnosticSeverity> {
        let symbols = SymbolTable::build_from_ast(stmts);
        collect_semantic_diagnostics(stmts, &symbols, &[], "")
            .into_iter()
            .find(|d| d.code == DiagnosticCode::UnusedLabel && d.message.contains(name))
            .map(|d| d.severity)
    }

    fn bare_actor(ty: &str, label: &str, props: Vec<crate::ast::Property>) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: label.to_string(),
            array_index: None,
            ty: ty.to_string(),
            props,
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    }

    /// A standing caption is content, not a loose end: nothing animates it and
    /// nothing has to. It stays a hint so editors can still offer the removal,
    /// but it must not sit in the warning column with real defects — 347 of
    /// this repo's own files said "Unused actor" and the lint stopped being
    /// readable.
    #[test]
    fn an_unreferenced_but_visible_actor_is_a_hint_not_a_warning() {
        let stmts = vec![bare_actor(
            "Text",
            "col_cap",
            vec![
                crate::ast::Property::new("text", Expr::Str("column space".into())),
                crate::ast::Property::new("opacity", Expr::Num(1.0)),
            ],
        )];
        assert_eq!(
            unused_label_severity(&stmts, "'col_cap'"),
            Some(DiagnosticSeverity::Hint),
            "a static caption must not be reported as a warning"
        );
    }

    /// The counterpart: a binding nothing reads has no defence, and this is the
    /// case the diagnostic was actually built for.
    #[test]
    fn an_unread_binding_is_still_a_warning() {
        let stmts = vec![Stmt::LetDecl {
            is_pub: false,
            name: "leftover".to_string(),
            value: Expr::Num(3.0),
            span: None,
        }];
        assert_eq!(
            unused_label_severity(&stmts, "'leftover'"),
            Some(DiagnosticSeverity::Warning),
            "demoting actors must not have demoted the case that means something"
        );
    }

    #[test]
    fn structural_container_with_children_is_not_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "cards".to_string(),
            array_index: None,
            ty: "Row".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "card".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(
            !labels.iter().any(|m| m.contains("'cards'")),
            "structural container label should be exempt: {labels:?}"
        );
    }

    #[test]
    fn empty_container_is_still_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "holder".to_string(),
            array_index: None,
            ty: "Group".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(labels.iter().any(|m| m.contains("holder")));
    }

    #[test]
    fn non_container_with_children_is_still_unused() {
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "wrapper".to_string(),
            array_index: None,
            ty: "Rect".to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![InlineItem::Labeled {
                label: "child".to_string(),
                array_index: None,
                ty: "Rect".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
            }],
            span: None,
        }];
        let labels = unused_labels(&stmts);
        assert!(labels.iter().any(|m| m.contains("wrapper")));
    }

    fn unknown_types(stmts: &[Stmt], symbols: &SymbolTable) -> Vec<String> {
        collect_semantic_diagnostics(stmts, symbols, &[], "")
            .into_iter()
            .filter(|d| d.code == DiagnosticCode::UnknownType)
            .map(|d| d.message.clone())
            .collect()
    }

    fn actor_decl_of_type(ty: &str) -> Stmt {
        Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "instance".to_string(),
            array_index: None,
            ty: ty.to_string(),
            props: vec![],
            modifiers: vec![],
            children: vec![],
            span: None,
        }
    }

    #[test]
    fn imported_component_instance_is_not_unknown_type() {
        // Regression: a statement-position instance of an imported `pub
        // component` used to warn `unknown-type` because only builtin types
        // were consulted; Group-wrapping "fixed" it only by escaping the lint.
        let mut symbols = SymbolTable::build_from_ast(&[]);
        symbols.components.insert(
            "MetricCard".to_string(),
            crate::symbol_table::ComponentInfo {
                name: "MetricCard".to_string(),
                is_pub: true,
                params: vec![],
                line: 1,
                col: 1,
                span: None,
            },
        );
        let stmts = vec![actor_decl_of_type("MetricCard")];
        let diags = unknown_types(&stmts, &symbols);
        assert!(
            diags.is_empty(),
            "imported component instance must not warn unknown-type: {diags:?}"
        );
    }

    #[test]
    fn namespaced_component_instance_is_not_unknown_type() {
        // `import "lib/ui.amx" as ui` + `ui.MetricCard` resolves through the
        // namespace tables, not `types`.
        let mut symbols = SymbolTable::build_from_ast(&[]);
        symbols.namespaces.insert(
            "ui".to_string(),
            SymbolTable {
                components: [(
                    "MetricCard".to_string(),
                    crate::symbol_table::ComponentInfo {
                        name: "MetricCard".to_string(),
                        is_pub: true,
                        params: vec![],
                        line: 1,
                        col: 1,
                        span: None,
                    },
                )]
                .into_iter()
                .collect(),
                ..Default::default()
            },
        );
        let stmts = vec![actor_decl_of_type("ui.MetricCard")];
        let diags = unknown_types(&stmts, &symbols);
        assert!(
            diags.is_empty(),
            "namespaced component instance must not warn unknown-type: {diags:?}"
        );
    }

    #[test]
    fn genuinely_unknown_type_still_warns() {
        let symbols = SymbolTable::build_from_ast(&[]);
        let stmts = vec![actor_decl_of_type("NotAType")];
        let diags = unknown_types(&stmts, &symbols);
        assert!(
            diags.iter().any(|m| m.contains("NotAType")),
            "bogus type must still warn unknown-type: {diags:?}"
        );
    }
}
