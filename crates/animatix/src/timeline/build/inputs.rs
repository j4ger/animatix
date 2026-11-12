//! Input evaluation shared with resource preparation, without a Timeline.

use super::*;

/// Bind a preparation-time loop exactly as the timeline builder does, including
/// tuple, vector and color destructuring and its arity diagnostics.
pub fn bind_input_loop(
    env: &mut Environment,
    pattern: &crate::ast::LoopPattern,
    value: Value,
    index: usize,
) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    process::bind_loop_var(env, pattern, value, index, &mut diagnostics);
    diagnostics
}

/// Seed the standard expression environment, namespace exports and overrides.
///
/// Resource preparation uses the same parsers and VM as timeline construction;
/// callers then walk authored `let` declarations, skipping overridden names.
pub fn prepare_input_environment(
    statements: &[Stmt],
    namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
    overrides: &[(String, String)],
) -> (Environment, Vec<Diagnostic>) {
    let mut env = Environment::new();
    let mut diagnostics = Vec::new();
    load_standard_library(&mut env);
    for (name, params, body) in fn_eval::collect_pure_fns(statements) {
        env.set(
            &name,
            Value::UserFn {
                name: name.clone(),
                params: std::sync::Arc::new(params),
                body: std::sync::Arc::new(body),
            },
        );
    }
    BuiltInColorscheme::DefaultDark.resolved().seed_environment(&mut env);
    let dims = SceneDimensions::default();
    env.set("scene_width", Value::Num(dims.width as f64));
    env.set("scene_height", Value::Num(dims.height as f64));
    let external_colorschemes =
        super::color_inputs::load_colorscheme_declarations(statements, &mut diagnostics);
    for (alias, namespace) in namespaces {
        post::seed_namespace_environment(&mut env, &mut diagnostics, alias, namespace);
    }
    for statement in statements {
        let Stmt::Config { settings, .. } = statement else {
            continue;
        };
        for setting in settings.iter().filter(|setting| setting.name == "colorscheme") {
            let Some(name) = config_string_value(&setting.value) else {
                continue;
            };
            if let Some(colorscheme) =
                super::color_inputs::select_colorscheme(&name, &external_colorschemes)
            {
                colorscheme.seed_environment(&mut env);
            }
        }
    }
    for define in defines::parse_defines(overrides, &mut diagnostics) {
        match evaluate_expr(&define.value, &env) {
            Ok(value) => env.set(&define.name, value),
            Err(error) => diagnostics.push(Diagnostic::error(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!("override '{}': {error}", define.name),
            )),
        }
    }
    (env, diagnostics)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{BinaryOp, Expr, LoopPattern};

    fn parse(source: &str) -> Vec<Stmt> {
        let (ast, errors) = animatix_syntax::parser::parse_source(source);
        assert!(errors.is_empty(), "parse errors: {errors:?}");
        ast.expect("parsed source")
    }

    fn color_at(env: &Environment, path: &[&str]) -> [f64; 4] {
        let value =
            evaluate_expr(&Expr::Path(path.iter().map(|part| (*part).to_string()).collect()), env)
                .expect("palette color resolves");
        let Value::Color(color) = value else {
            panic!("expected palette color, got {value:?}");
        };
        color
    }

    #[test]
    fn prepared_inputs_seed_overrides_namespaces_pure_functions_and_native_palette() {
        let source = r#"
fn add_one(value: Num) -> Num { return value + 1 }
let palette = Colorscheme {
  extends: "editorial-dark",
  accent.primary: (0.91, 0.12, 0.23, 1),
  text.primary: (0.21, 0.32, 0.43, 1)
}
config { colorscheme: "palette" }
"#;
        let ast = parse(source);
        let mut namespace = crate::module::Namespace::default();
        namespace.exports.insert("factor".to_string(), Expr::Num(2.0));
        let namespaces = std::collections::HashMap::from([("theme".to_string(), namespace)]);
        let overrides = vec![("gain".to_string(), "4".to_string())];

        let (prepared_env, diagnostics) = prepare_input_environment(&ast, &namespaces, &overrides);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let expression = Expr::Binary(
            Box::new(Expr::Call("add_one".to_string(), vec![Expr::Ident("gain".to_string())])),
            BinaryOp::Add,
            Box::new(Expr::Path(vec!["theme".to_string(), "factor".to_string()])),
        );
        assert!(matches!(evaluate_expr(&expression, &prepared_env), Ok(Value::Num(7.0))));

        let native = Timeline::build_with_diagnostics(&ast, &namespaces);
        assert!(native.diagnostics.is_empty(), "{:?}", native.diagnostics);
        assert_eq!(
            color_at(&prepared_env, &["accent", "primary"]),
            color_at(&native.output.env, &["accent", "primary"])
        );
        assert_eq!(
            color_at(&prepared_env, &["text", "primary"]),
            color_at(&native.output.env, &["text", "primary"])
        );
    }

    #[test]
    fn preparation_loop_binding_matches_native_vec2_and_color_destructuring() {
        let (mut env, diagnostics) = prepare_input_environment(&[], &Default::default(), &[]);
        assert!(diagnostics.is_empty());
        let names = |suffix: &str, count: usize| {
            LoopPattern::Tuple((0..count).map(|index| format!("{suffix}{index}")).collect())
        };

        assert!(bind_input_loop(&mut env, &names("v", 2), Value::Vec2([3.0, 5.0]), 0,).is_empty());
        assert!(matches!(env.get("v0"), Some(Value::Num(3.0))));
        assert!(matches!(env.get("v1"), Some(Value::Num(5.0))));

        assert_eq!(
            bind_input_loop(&mut env, &names("c", 4), Value::Color([0.1, 0.2, 0.3, 1.0]), 1,).len(),
            0
        );
        assert!(matches!(env.get("c0"), Some(Value::Num(0.1))));
        assert!(matches!(env.get("c1"), Some(Value::Num(0.2))));
        assert!(matches!(env.get("c2"), Some(Value::Num(0.3))));
        assert!(matches!(env.get("c3"), Some(Value::Num(1.0))));
    }
}
