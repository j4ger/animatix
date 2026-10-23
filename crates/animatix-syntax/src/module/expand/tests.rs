use super::*;

mod tests {
    use super::*;

    #[test]
    fn test_expand_preserves_forloop_index_var() {
        // Regression: for-loop must preserve index_var through component expansion.
        // The `for v, k in {50, 150, 250} { ... }` form must keep `k` after expansion.
        let source = "for v, k in {50, 150, 250} { a[k]: Rect, size: (20,40), at: (v, 100) }";
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();
        let components = std::collections::HashMap::new();
        let (expanded, _registry) = expand_statements(&stmts, &components);
        // Find the ForLoop
        let for_stmt = expanded.iter().find(|s| matches!(s, Stmt::ForLoop { .. })).unwrap();
        if let Stmt::ForLoop { index_var, .. } = for_stmt {
            assert_eq!(
                index_var.as_deref(),
                Some("k"),
                "index_var must be preserved through expansion"
            );
        } else {
            panic!("Expected ForLoop");
        }
        // Find `a[k]` ActorDecl inside the ForLoop body
        if let Stmt::ForLoop { body, .. } = for_stmt {
            let act = body.iter().find_map(|s| {
                if let Stmt::ActorDecl {
                    array_index: Some(Expr::Ident(name)),
                    ..
                } = s
                {
                    Some(name.clone())
                } else {
                    None
                }
            });
            assert_eq!(
                act.as_deref(),
                Some("k"),
                "array_index must be preserved through expansion"
            );
        }
    }

    #[test]
    fn test_component_always_survives_container_instantiation() {
        // Regression: a component body's `always` (and other non-declaration
        // statements) were silently dropped when the component was
        // instantiated inside a container, because they have no inline form.
        // They must be hoisted to the enclosing statement list with their
        // targets rewritten to the instance's dotted labels.
        let source = r#"
pub component Pulsar() {
  box: Rect, size: (40, 40)
  always { box.rotation = sin(t() * 2) * 8 }
}

grp: Group {
  p: Pulsar
}
"#;
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();
        let mut components = std::collections::HashMap::new();
        for stmt in &stmts {
            if let Stmt::ComponentDef(def, _) = stmt {
                components.insert(
                    def.name.clone(),
                    super::super::ComponentEntry {
                        definition: def.clone(),
                        actions: Default::default(),
                        source_path: std::path::PathBuf::new(),
                    },
                );
            }
        }
        let (expanded, _registry) = expand_statements(&stmts, &components);

        let always_count = expanded.iter().filter(|s| matches!(s, Stmt::Always { .. })).count();
        assert_eq!(
            always_count, 1,
            "component-internal always must survive container instantiation"
        );
        // The hoisted statement must reference the rewritten target: `box`
        // is the component root, so it renames to the instance label `p`.
        let always = expanded.iter().find(|s| matches!(s, Stmt::Always { .. })).unwrap();
        let Stmt::Always { body, .. } = always else {
            panic!("expected Always");
        };
        let rewritten_target = body.iter().find_map(|s| match s {
            Stmt::Assignment {
                target, property, ..
            } => Some((target.clone(), property.clone())),
            _ => None,
        });
        assert_eq!(
            rewritten_target,
            Some((
                vec![crate::ast::TargetSegment::Static("p".to_string())],
                "rotation".to_string()
            )),
            "hoisted assignment must target the renamed instance root"
        );
    }

    #[test]
    fn test_instance_universal_props_forward_to_root() {
        // opacity/at/anchor/offset written on a component instance must land
        // on the expanded root actor, not be dropped.
        let source = r#"
pub component Badge(text: Str) {
  box: Rect, size: (160, 70)
}

b: Badge, text: "hi", opacity: 1, at: (320, 240)
"#;
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();
        let mut components = std::collections::HashMap::new();
        for stmt in &stmts {
            if let Stmt::ComponentDef(def, _) = stmt {
                components.insert(
                    def.name.clone(),
                    super::super::ComponentEntry {
                        definition: def.clone(),
                        actions: Default::default(),
                        source_path: std::path::PathBuf::new(),
                    },
                );
            }
        }
        let (expanded, _registry) = expand_statements(&stmts, &components);
        let root = expanded
            .iter()
            .find_map(|s| match s {
                Stmt::ActorDecl { label, props, .. } if label == "b" => Some(props),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expanded root actor 'b' not found"));
        for name in ["opacity", "at"] {
            assert!(
                root.iter().any(|p| p.name == name),
                "instance prop '{name}' must be forwarded to the root actor"
            );
        }
    }

    #[test]
    fn test_param_named_like_universal_prop_stays_binding() {
        // A component param that shares a name with a forwarded prop (e.g.
        // `opacity` as a template parameter) must remain a binding, not be
        // double-applied as an actor prop.
        let source = r#"
pub component Tint(opacity: Num = 1) {
  box: Rect, size: (40, 40), opacity: opacity
}

t: Tint, opacity: 0.5
"#;
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();
        let mut components = std::collections::HashMap::new();
        for stmt in &stmts {
            if let Stmt::ComponentDef(def, _) = stmt {
                components.insert(
                    def.name.clone(),
                    super::super::ComponentEntry {
                        definition: def.clone(),
                        actions: Default::default(),
                        source_path: std::path::PathBuf::new(),
                    },
                );
            }
        }
        let (expanded, _registry) = expand_statements(&stmts, &components);
        let root = expanded
            .iter()
            .find_map(|s| match s {
                Stmt::ActorDecl { label, props, .. } if label == "t" => Some(props),
                _ => None,
            })
            .expect("expanded root actor 't'");
        let opacity_count = root.iter().filter(|p| p.name == "opacity").count();
        assert_eq!(opacity_count, 1, "param-bound opacity must not be forwarded a second time");
    }

    #[test]
    fn test_expand_component_inside_scene_body() {
        let source = r#"
pub component Box(size: Num = 20) {
  box: Rect, size: (size, size)
}

# Intro
b: Box, size: 40
"#;
        let (stmts, _errors) = crate::parser::parse_source(source);
        let stmts = stmts.unwrap();

        let mut components = std::collections::HashMap::new();
        for stmt in &stmts {
            if let Stmt::ComponentDef(def, _) = stmt {
                components.insert(
                    def.name.clone(),
                    super::super::ComponentEntry {
                        definition: def.clone(),
                        source_path: std::path::PathBuf::from("scene.amx"),
                        actions: std::collections::HashMap::new(),
                    },
                );
            }
        }

        let (expanded, _registry) = expand_statements(&stmts, &components);
        let scene = expanded
            .iter()
            .find_map(|s| {
                if let Stmt::Scene { body, .. } = s {
                    Some(body)
                } else {
                    None
                }
            })
            .expect("scene body should be expanded");
        let box_actor = scene
            .iter()
            .find_map(|s| {
                if let Stmt::ActorDecl { ty, .. } = s {
                    Some(ty)
                } else {
                    None
                }
            })
            .expect("component instance should expand inside scene body");
        assert_eq!(box_actor, "Rect");
    }
}
