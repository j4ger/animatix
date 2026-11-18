use super::*;

#[test]
fn test_expand_preserves_forloop_index_var() {
    // Regression: for-loop must preserve index_var through component expansion.
    // The `for v, k in {50, 150, 250} { ... }` form must keep `k` after expansion.
    let source = "for v, k in {50, 150, 250} { a[k]: Rect, size: (20,40), at: (v, 100) }";
    let (stmts, _errors) = crate::parser::parse_source(source);
    let stmts = stmts.unwrap();
    let components = std::collections::HashMap::new();
    let expanded = expand_statements(&stmts, &components).statements;
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
        assert_eq!(act.as_deref(), Some("k"), "array_index must be preserved through expansion");
    }
}

#[test]
fn indexed_component_instances_keep_the_loop_index_on_the_expanded_root() {
    let source = r#"
pub component MetricCard(value: Num) {
  frame: Rect, size: (120, 80)
  value_text: Text, text: value
}
for value, i in {10, 20, 30} {
  cards[i]: MetricCard, value: value
}
"#;
    let (stmts, errors) = crate::parser::parse_source(source);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
    let stmts = stmts.unwrap();
    let mut components = std::collections::HashMap::new();
    for stmt in &stmts {
        if let Stmt::ComponentDef(def, _) = stmt {
            components.insert(
                def.name.clone(),
                super::super::ComponentEntry {
                    definition: def.clone(),
                    methods: Default::default(),
                    source_path: std::path::PathBuf::new(),
                },
            );
        }
    }
    let expanded = expand_statements(&stmts, &components).statements;
    let Stmt::ForLoop { body, .. } =
        expanded.iter().find(|stmt| matches!(stmt, Stmt::ForLoop { .. })).unwrap()
    else {
        panic!("expected for loop");
    };
    let roots: Vec<_> = body
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::ActorDecl {
                label, array_index, ..
            } if label == "cards" => Some(array_index.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(roots.len(), 1);
    assert!(matches!(roots[0], Some(Expr::Ident(index)) if index == "i"));
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
                    methods: Default::default(),
                    source_path: std::path::PathBuf::new(),
                },
            );
        }
    }
    let expanded = expand_statements(&stmts, &components).statements;

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
        Some((vec![crate::ast::TargetSegment::Static("p".to_string())], "rotation".to_string())),
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
                    methods: Default::default(),
                    source_path: std::path::PathBuf::new(),
                },
            );
        }
    }
    let expanded = expand_statements(&stmts, &components).statements;
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
                    methods: Default::default(),
                    source_path: std::path::PathBuf::new(),
                },
            );
        }
    }
    let expanded = expand_statements(&stmts, &components).statements;
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
fn test_instance_opacity_multiplies_with_root_opacity() {
    // An instance `opacity` is a group opacity, not a style override: a root
    // actor already authored `opacity: 0.5` must receive `0.5 * 0.5`, not a
    // replacement `0.5`.
    let source = r#"
pub component Box() {
  r: Rect, size: (100, 100), opacity: 0.5
}

b: Box, opacity: 0.5
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
                    methods: Default::default(),
                    source_path: std::path::PathBuf::new(),
                },
            );
        }
    }
    let expanded = expand_statements(&stmts, &components).statements;
    let root = expanded
        .iter()
        .find_map(|s| match s {
            Stmt::ActorDecl { label, props, .. } if label == "b" => Some(props),
            _ => None,
        })
        .expect("expanded root actor 'b'");
    let opacities: Vec<&Expr> =
        root.iter().filter(|p| p.name == "opacity").map(|p| &p.value).collect();
    assert_eq!(opacities.len(), 1, "the root must carry exactly one composed opacity");
    match opacities[0] {
        Expr::Binary(left, crate::ast::BinaryOp::Mul, right) => {
            assert!(matches!(left.as_ref(), Expr::Num(n) if (*n - 0.5).abs() < 1e-9));
            assert!(matches!(right.as_ref(), Expr::Num(n) if (*n - 0.5).abs() < 1e-9));
        },
        other => panic!("expected `root_opacity * instance_opacity`, got {other:?}"),
    }
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
                    methods: std::collections::HashMap::new(),
                },
            );
        }
    }

    let expanded = expand_statements(&stmts, &components).statements;
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

#[test]
fn multi_actor_component_preserves_relative_size_on_group_and_fills_inner_root() {
    let source = r#"
pub component Card() {
  root: Col, size: (50%, auto)
  caption: Text
}
card: Card
"#;
    let (stmts, errors) = crate::parser::parse_source(source);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
    let stmts = stmts.unwrap();
    let mut components = std::collections::HashMap::new();
    for stmt in &stmts {
        if let Stmt::ComponentDef(def, _) = stmt {
            components.insert(
                def.name.clone(),
                super::super::ComponentEntry {
                    definition: def.clone(),
                    methods: Default::default(),
                    source_path: std::path::PathBuf::new(),
                },
            );
        }
    }
    let expanded = expand_statements(&stmts, &components).statements;
    let (group_props, children) = expanded
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::ActorDecl {
                label,
                ty,
                props,
                children,
                ..
            } if label == "card" && ty == "Group" => Some((props, children)),
            _ => None,
        })
        .expect("multi-actor component wrapper Group");
    let original_size = Expr::Tuple(vec![Expr::Percent(50.0), Expr::Ident("auto".into())]);
    assert!(
        group_props
            .iter()
            .any(|property| property.name == "size" && property.value == original_size),
        "wrapper must keep the authored relative size: {group_props:?}"
    );

    let root_props = children
        .iter()
        .find_map(|item| match item {
            InlineItem::Labeled { label, props, .. } if label == "card.root" => Some(props),
            _ => None,
        })
        .expect("rewritten inner root actor");
    assert!(
        root_props.iter().any(|property| {
            property.name == "size"
                && property.value
                    == Expr::Tuple(vec![Expr::Ident("fill".into()), Expr::Ident("auto".into())])
        }),
        "relative root width must fill the Group's allocated cell: {root_props:?}"
    );
}

#[test]
fn methods_are_kept_in_the_registry_for_their_own_scene() {
    let source = r#"
pub component FirstCard {
  pub fn pulse() { return #{ self.opacity = 0.25 } }
  root: Rect
}

pub component SecondCard {
  pub fn pulse() { return #{ self.opacity = 0.75 } }
  root: Rect
}
# FirstScene
card: FirstCard
# SecondScene
card: SecondCard
"#;
    let (statements, errors) = crate::parser::parse_source(source);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
    let statements = statements.expect("scene source should parse");
    let components = statements
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::ComponentDef(definition, _) => Some((
                definition.name.clone(),
                crate::module::ComponentEntry {
                    definition: definition.clone(),
                    source_path: std::path::PathBuf::new(),
                    methods: Default::default(),
                },
            )),
            _ => None,
        })
        .collect();

    let expanded = expand_statements(&statements, &components);
    assert!(expanded.instance_methods.is_empty());
    let method_opacity = |scene: &str| {
        let template = expanded.scene_methods[scene]["card"]["pulse"].clone();
        template
            .body
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::Return {
                    value: Some(Expr::Timeline(plan)),
                    ..
                } => plan.statements.iter().find_map(|stmt| match stmt {
                    Stmt::Assignment {
                        property,
                        value: Expr::Num(value),
                        ..
                    } if property == "opacity" => Some(*value),
                    _ => None,
                }),
                _ => None,
            })
            .expect("method should return its timeline plan")
    };
    assert_eq!(method_opacity("FirstScene"), 0.25);
    assert_eq!(method_opacity("SecondScene"), 0.75);
}

#[test]
fn reusable_plan_sites_keep_independent_component_methods_and_expand_components() {
    let source = r#"
component Card(width: Num) {
  box: Rect, size: (width, 20)
  fn reveal(amount: Num = width) {
    #{
      #0s
      box.opacity = amount
    }
  }
}
let first = #{
  local: Card, width: 10
  local.reveal(amount: 1)
}
let second = #{
  local: Card, width: 20
  local.reveal(amount: 2)
}
fn make(width: Num) {
  #{
    local: Card, width: width
    local.reveal()
  }
}
"#;
    let (statements, errors) = crate::parser::parse_source(source);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
    let statements = statements.expect("source should parse");
    let components = statements
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::ComponentDef(definition, _) => Some((
                definition.name.clone(),
                crate::module::ComponentEntry {
                    definition: definition.clone(),
                    source_path: std::path::PathBuf::from("/modules/cards.amx"),
                    methods: crate::module::discovery::collect_component_functions(
                        definition,
                        std::path::Path::new("/modules/cards.amx"),
                    ),
                },
            )),
            _ => None,
        })
        .collect();
    let expanded = expand_statements(&statements, &components);

    let plan = |name: &str| {
        expanded
            .statements
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::LetDecl {
                    name: binding,
                    value: Expr::Timeline(plan),
                    ..
                } if binding == name => Some(plan),
                _ => None,
            })
            .expect("let binding should hold a reusable plan")
    };
    let first = plan("first");
    let second = plan("second");
    assert!(first.statements.iter().any(|stmt| matches!(
        stmt,
        Stmt::ActorDecl { label, .. } if label == "local"
    )));
    assert!(second.statements.iter().any(|stmt| matches!(
        stmt,
        Stmt::ActorDecl { label, .. } if label == "local"
    )));
    assert!(first.instance_methods.contains_key("local"));
    assert!(second.instance_methods.contains_key("local"));
    assert_eq!(
        first.instance_methods["local"]["reveal"].params[0].default,
        Some(Expr::Ident("width".into()))
    );
    assert_eq!(
        second.instance_methods["local"]["reveal"].params[0].default,
        Some(Expr::Ident("width".into()))
    );

    let reexpanded = expand_statements(&expanded.statements, &components);
    for name in ["first", "second"] {
        let plan = reexpanded
            .statements
            .iter()
            .find_map(|stmt| match stmt {
                Stmt::LetDecl {
                    name: binding,
                    value: Expr::Timeline(plan),
                    ..
                } if binding == name => Some(plan),
                _ => None,
            })
            .expect("re-expanded binding should retain its plan");
        assert!(
            plan.instance_methods.contains_key("local"),
            "re-expansion must preserve method scope for plan {name}"
        );
    }

    let make = expanded
        .statements
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::FnDecl { name, body, .. } if name == "make" => Some(body),
            _ => None,
        })
        .expect("make function should remain in expanded statements");
    assert!(matches!(
        make.as_slice(),
        [Stmt::Return {
            value: Some(Expr::Timeline(plan)),
            ..
        }] if plan.instance_methods.contains_key("local")
    ));
}

#[test]
fn component_methods_capture_names_from_instance_parameter_expressions() {
    let source = r#"
component Card(width: Num) {
  box: Rect, size: (width, 20)
  fn scaled(factor: Num) { width * factor }
}
fn make(w: Num) {
  #{
    local: Card, width: w
    local.scaled(2)
  }
}
"#;
    let (statements, errors) = crate::parser::parse_source(source);
    assert!(errors.is_empty(), "unexpected parse errors: {errors:?}");
    let statements = statements.expect("source should parse");
    let components = statements
        .iter()
        .filter_map(|stmt| match stmt {
            Stmt::ComponentDef(definition, _) => Some((
                definition.name.clone(),
                crate::module::ComponentEntry {
                    definition: definition.clone(),
                    source_path: std::path::PathBuf::from("/modules/cards.amx"),
                    methods: crate::module::discovery::collect_component_functions(
                        definition,
                        std::path::Path::new("/modules/cards.amx"),
                    ),
                },
            )),
            _ => None,
        })
        .collect();
    let expanded = expand_statements(&statements, &components);
    let plan = expanded
        .statements
        .iter()
        .find_map(|stmt| match stmt {
            Stmt::FnDecl { name, body, .. } if name == "make" => {
                body.iter().find_map(|stmt| match stmt {
                    Stmt::Return {
                        value: Some(Expr::Timeline(plan)),
                        ..
                    } => Some(plan),
                    _ => None,
                })
            },
            _ => None,
        })
        .expect("make should return its plan");
    assert_eq!(plan.instance_methods["local"]["scaled"].component_captures, ["w"]);
}

#[test]
fn two_component_instances_keep_self_root_indexed_targets_isolated() {
    let source = r#"
pub component Deck() {
    root: Group {
        for i in {0, 1} {
            cells[i]: Group {
                arm: Rect, size: (20, 20)
            }
        }
    }
    #1s
    for i in {0, 1} {
        self.root.cells[i].arm.opacity = 0.5 [1s]
    }
}
a: Deck
b: Deck
"#;
    let (statements, errors) = crate::parser::parse_source(source);
    assert!(errors.is_empty(), "{errors:?}");
    let statements = statements.expect("source parses");
    let mut components = HashMap::new();
    for stmt in &statements {
        if let Stmt::ComponentDef(def, _) = stmt {
            components.insert(
                def.name.clone(),
                ComponentEntry {
                    definition: def.clone(),
                    methods: Default::default(),
                    source_path: std::path::PathBuf::new(),
                },
            );
        }
    }
    let expanded = expand_statements(&statements, &components);
    let mut targets = Vec::new();
    crate::walk::walk_stmts(&expanded.statements, &mut |stmt| {
        if let Stmt::Assignment { target, property, .. } = stmt
            && property == "opacity"
        {
            targets.push(target.clone());
        }
    });
    let expected = ["a", "b"]
        .into_iter()
        .map(|instance| {
            vec![
                TargetSegment::Static(instance.to_string()),
                TargetSegment::Indexed {
                    base: "cells".to_string(),
                    index: Box::new(Expr::Ident("i".to_string())),
                },
                TargetSegment::Static("arm".to_string()),
            ]
        })
        .collect::<Vec<_>>();
    assert_eq!(targets, expected);
}
