use super::*;

fn std_env() -> TypeEnv {
    TypeEnv::with_stdlib()
}

#[test]
fn named_colors_are_colors() {
    let env = std_env();
    assert_eq!(infer_expr_type(&Expr::Ident("red".to_string()), &env), Type::Color);
    assert_eq!(infer_expr_type(&Expr::Ident("BLUE".to_string()), &env), Type::Color);
}

#[test]
fn color_namespace_paths_are_colors() {
    let env = std_env();
    for ns in COLOR_NAMESPACES {
        let path = Expr::Path(vec![ns.to_string(), "primary".to_string()]);
        assert_eq!(infer_expr_type(&path, &env), Type::Color);
    }
    let scene = Expr::Path(vec!["scene".to_string(), "background".to_string()]);
    assert_eq!(infer_expr_type(&scene, &env), Type::Color);
    let stats = Expr::Path(vec![
        "scene".to_string(),
        "stats".to_string(),
        "motion".to_string(),
    ]);
    assert_eq!(infer_expr_type(&stats, &env), Type::List(Box::new(Type::Num)));
    let curve_call = Expr::Call("curve_at".to_string(), vec![stats.clone(), Expr::Num(0.5)]);
    assert_eq!(infer_expr_type(&curve_call, &env), Type::Num);
    let smooth_call =
        Expr::Call("curve_smooth".to_string(), vec![stats, Expr::Num(0.5), Expr::Num(0.7)]);
    assert_eq!(infer_expr_type(&smooth_call, &env), Type::Num);
}

#[test]
fn color_functions_return_colors() {
    let env = std_env();
    for name in COLOR_CONSTRUCTOR_FNS {
        let call = Expr::Call(name.to_string(), vec![]);
        assert_eq!(infer_expr_type(&call, &env), Type::Color);
    }
}

#[test]
fn list_infers_common_color_type() {
    let env = std_env();
    let list = Expr::List(vec![
        Expr::Ident("red".to_string()),
        Expr::Ident("blue".to_string()),
    ]);
    assert_eq!(infer_expr_type(&list, &env), Type::List(Box::new(Type::Color)));
}

#[test]
fn list_infers_vec4_for_color_vec4_mix() {
    let env = std_env();
    let list = Expr::List(vec![
        Expr::Ident("red".to_string()),
        Expr::Tuple(vec![
            Expr::Num(0.0),
            Expr::Num(0.0),
            Expr::Num(0.0),
            Expr::Num(1.0),
        ]),
    ]);
    assert_eq!(infer_expr_type(&list, &env), Type::List(Box::new(Type::Vec4)));
}

#[test]
fn let_bindings_shadow_literal_names() {
    let mut env = std_env();
    env.bind("red", Type::Num);
    assert_eq!(infer_expr_type(&Expr::Ident("red".to_string()), &env), Type::Num);
}

#[test]
fn namespace_paths_resolve() {
    let mut env = std_env();
    env.register_namespace_value("theme", "accent", Type::Color);
    let path = Expr::Path(vec!["theme".to_string(), "accent".to_string()]);
    assert_eq!(infer_expr_type(&path, &env), Type::Color);
}

#[test]
fn nested_namespace_paths_resolve() {
    let mut env = std_env();
    let mut nested = HashMap::new();
    nested.insert("accent".to_string(), NamespaceType::Value(Type::Color));
    env.register_namespace("project", NamespaceType::Namespace(nested));
    let path = Expr::Path(vec!["project".to_string(), "accent".to_string()]);
    assert_eq!(infer_expr_type(&path, &env), Type::Color);
}

#[test]
fn actor_property_paths_infer() {
    let mut env = std_env();
    env.declare_actor("box", "Rect");
    let path = Expr::Path(vec!["box".to_string(), "color".to_string()]);
    // `color` is declared `Union[Color, Str]` (a colour or a named/gradient
    // string) by the descriptor table, so a bare `Color` is a subtype of the
    // property's type rather than the whole of it.
    let expected = Type::Union(vec![Type::Color, Type::Str]);
    assert_eq!(infer_expr_type(&path, &env), expected);
    assert!(is_subtype(&Type::Color, &expected));
}

#[test]
fn property_type_is_resolved_per_actor_type() {
    // The table is per `(actor_type, property)`: a property that exists on
    // some other actor is not a property of this one.
    assert_eq!(property_type("Rect", "transform"), Some(transform_type()));
    assert_eq!(property_type("Rect", "color"), Some(Type::Union(vec![Type::Color, Type::Str])));
    assert_eq!(property_type("Code", "code"), Some(Type::Str));
    assert_eq!(property_type("Rect", "code"), None);
    assert_eq!(property_type("Rect", "standoff"), None);
    assert_eq!(property_type("Rect", "url"), None);
    assert_eq!(property_type("Rect", "label"), None);
    // `fill`/`content` were removed from the descriptor table; they must not
    // resolve through a leftover name-only fallback.
    assert_eq!(property_type("Rect", "fill"), None);
    assert_eq!(property_type("Text", "content"), None);
}

#[test]
fn list_common_type_is_subtype_compatible() {
    assert!(is_subtype(&Type::Color, &Type::Vec4));
    assert!(is_subtype(
        &Type::List(Box::new(Type::Color)),
        &Type::List(Box::new(Type::Vec4))
    ));
}

#[test]
fn aliases_resolve_when_declared_later() {
    let mut env = TypeEnv::with_stdlib();
    env.register_alias("Metric", &TypeAnnotation::Alias("Inner".to_string()));
    env.register_alias(
        "Inner",
        &TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str]),
    );
    assert_eq!(
        env.resolve_annotation(&TypeAnnotation::Alias("Metric".to_string())),
        Type::Union(vec![Type::Bool, Type::Str])
    );
    assert!(env.unresolved_alias(&TypeAnnotation::Alias("Metric".to_string())).is_none());
}

#[test]
fn unresolved_alias_is_reported_instead_of_silently_accepted() {
    let env = TypeEnv::with_stdlib();
    assert_eq!(
        env.unresolved_alias(&TypeAnnotation::Alias("Missing".to_string())),
        Some("Missing".to_string())
    );
    assert_eq!(env.resolve_annotation(&TypeAnnotation::Alias("Missing".to_string())), Type::Any);
}

#[test]
fn alias_cycles_are_reported() {
    let mut env = TypeEnv::with_stdlib();
    env.register_alias("A", &TypeAnnotation::Alias("B".to_string()));
    env.register_alias("B", &TypeAnnotation::Alias("A".to_string()));
    assert_eq!(
        env.unresolved_alias(&TypeAnnotation::Alias("A".to_string())),
        Some("A".to_string())
    );
}

#[test]
fn rich_annotations_roundtrip_without_any_degradation() {
    let tuple = TypeAnnotation::Tuple(vec![TypeAnnotation::Str, TypeAnnotation::Num]);
    let function = TypeAnnotation::Function {
        params: vec![TypeAnnotation::Num, TypeAnnotation::Num],
        ret: Box::new(TypeAnnotation::Bool),
    };
    for annotation in [
        TypeAnnotation::Vec3,
        tuple,
        function,
        TypeAnnotation::Scene,
        TypeAnnotation::List(Box::new(TypeAnnotation::Tuple(vec![TypeAnnotation::Num]))),
        TypeAnnotation::Enum(vec!["left".to_string(), "right".to_string(), "top".to_string()]),
    ] {
        assert_eq!(Type::from_annotation(&annotation).to_annotation(), annotation);
    }
}

#[test]
fn rich_annotations_are_subtype_compatible() {
    assert!(is_subtype(
        &Type::Tuple(vec![Type::Str, Type::Num]),
        &Type::Tuple(vec![Type::Str, Type::Num])
    ));
    assert!(!is_subtype(
        &Type::Tuple(vec![Type::Num, Type::Str]),
        &Type::Tuple(vec![Type::Str, Type::Num])
    ));
    let function = Type::Function {
        params: vec![Type::Any],
        ret: Box::new(Type::Num),
    };
    assert!(is_subtype(
        &function,
        &Type::Function {
            params: vec![Type::Num],
            ret: Box::new(Type::Num),
        }
    ));
    assert!(!is_subtype(
        &Type::Function {
            params: vec![Type::Num],
            ret: Box::new(Type::Num),
        },
        &function
    ));
}

#[test]
fn nested_rich_annotations_resolve_aliases() {
    let mut env = TypeEnv::with_stdlib();
    env.register_alias(
        "Pair",
        &TypeAnnotation::Tuple(vec![TypeAnnotation::Str, TypeAnnotation::Num]),
    );
    assert!(env.unresolved_alias(&TypeAnnotation::Alias("Pair".to_string())).is_none());
    env.register_alias(
        "BrokenPair",
        &TypeAnnotation::Tuple(vec![
            TypeAnnotation::Str,
            TypeAnnotation::Alias("Missing".to_string()),
        ]),
    );
    assert_eq!(
        env.unresolved_alias(&TypeAnnotation::Alias("BrokenPair".to_string())),
        Some("Missing".to_string())
    );
    env.register_alias(
        "BrokenFn",
        &TypeAnnotation::Function {
            params: vec![TypeAnnotation::Alias("Missing".to_string())],
            ret: Box::new(TypeAnnotation::Num),
        },
    );
    assert_eq!(
        env.unresolved_alias(&TypeAnnotation::Alias("BrokenFn".to_string())),
        Some("Missing".to_string())
    );
}

#[test]
fn closure_and_call_return_types_infer() {
    let env = std_env();
    let closure = Expr::Closure(vec!["x".to_string()], Box::new(Expr::Num(1.0)));
    let function_ty = Type::Function {
        params: vec![Type::Any],
        ret: Box::new(Type::Num),
    };
    assert_eq!(infer_expr_type(&closure, &env), function_ty);

    let mut env = std_env();
    env.bind("f", function_ty);
    assert_eq!(infer_expr_type(&Expr::Call("f".to_string(), vec![]), &env), Type::Num);
}

#[test]
fn known_properties_are_derived_from_typed_registry() {
    let typed = known_property_types();
    for (ty, properties) in known_properties() {
        for property in properties {
            assert!(
                typed.contains_key(&(ty.clone(), property.clone())),
                "{} is listed for {} but has no expected type",
                property,
                ty
            );
        }
    }
}

#[test]
fn transform_is_known_actor_property() {
    let props = known_properties();
    let rect = props.get("Rect").expect("Rect property list");
    assert!(rect.contains(&"transform".to_string()), "missing transform");

    let typed = known_property_types();
    assert_eq!(
        typed.get(&("Rect".to_string(), "transform".to_string())),
        Some(&transform_type())
    );
}

#[test]
fn transform_shorthand_forms_are_subtypes_of_expected_type() {
    let expected = transform_type();
    for actual in [
        Type::Vec2,
        Type::Vec4,
        Type::Tuple(vec![
            Type::Num,
            Type::Num,
            Type::Num,
            Type::Num,
            Type::Num,
            Type::Num,
        ]),
    ] {
        assert!(is_subtype(&actual, &expected), "unexpected transform mismatch");
    }
}

#[test]
fn typst_shares_common_layout_properties() {
    let properties = known_properties();
    let typst = properties.get("Typst").expect("Typst property list");
    for property in [
        "position", "anchor", "offset", "scale", "rotation", "opacity", "color",
    ] {
        assert!(typst.contains(&property.to_string()), "missing common {property}");
    }
}

#[test]
fn plot_curve_has_graph_ranges() {
    let properties = known_properties();
    let plot_curve = properties.get("PlotCurve").expect("PlotCurve property list");
    for property in ["x_range", "y_range", "func"] {
        assert!(plot_curve.contains(&property.to_string()), "missing {property}");
    }
}
