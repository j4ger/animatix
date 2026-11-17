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
    function_context: Option<&crate::module::FunctionContext>,
    instance_methods: Option<&crate::module::InstanceFnRegistry>,
) -> (Environment, Vec<Diagnostic>) {
    let mut env = crate::timeline::builtins::standard_environment();
    let mut diagnostics = Vec::new();
    let fallback_context = function_context
        .is_none()
        .then(|| fn_eval::context_from_ast(statements, namespaces));
    let function_context = function_context
        .map(|context| std::sync::Arc::new(context.clone()))
        .or_else(|| fallback_context.as_ref().map(|(context, _)| context.clone()));
    env.set_function_context(function_context.clone());
    if let Some(methods) = instance_methods {
        install_input_methods(&mut env, methods, function_context.as_deref());
    }
    let fallback_path = fallback_context.as_ref().map(|(_, path)| path.clone());
    let mut seeded = std::collections::HashSet::new();
    if let Some(context) = &function_context {
        for (name, template) in context.visible.iter() {
            env.set(name, fn_eval::user_function_value(name, template, Some(context.clone())));
            seeded.insert(name.clone());
        }
        for (alias, namespace) in namespaces {
            seed_input_namespace_functions(
                &mut env,
                &format!("{alias}::"),
                namespace,
                context,
                &mut seeded,
            );
        }
    }
    for (name, params, return_type, body) in fn_eval::collect_user_fns(statements) {
        if seeded.contains(&name) {
            continue;
        }
        env.set(
            &name,
            Value::UserFn {
                name: name.clone(),
                params: std::sync::Arc::new(params),
                return_type,
                body: std::sync::Arc::new(body),
                function_context: function_context.clone(),
                definition_path: fallback_path.clone(),
                component_scope: None,
            },
        );
        seeded.insert(name);
    }
    BuiltInColorscheme::DefaultDark.resolved().seed_environment(&mut env);
    let dims = SceneDimensions::default();
    env.set_host_global("scene_width", Value::Num(dims.width as f64));
    env.set_host_global("scene_height", Value::Num(dims.height as f64));
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
            Ok(value) => env.set_host_global(&define.name, value),
            Err(error) => diagnostics.push(Diagnostic::error(
                DiagnosticCode::InvalidPropertyValue,
                DiagnosticPhase::Build,
                format!("override '{}': {error}", define.name),
            )),
        }
    }
    (env, diagnostics)
}

/// Install per-instance component methods into a preparation environment.
/// The table is callable-resolution metadata for resource discovery only; it
/// is not stored in the timeline ECS or frame actor state.
pub fn install_input_methods(
    env: &mut Environment,
    methods: &crate::module::InstanceFnRegistry,
    function_context: Option<&crate::module::FunctionContext>,
) {
    env.clear_component_methods();
    extend_input_methods(env, methods, function_context);
}

/// Add one lexical method registry over the current scope. Timeline-plan
/// mounting uses this to overlay its private local labels temporarily.
pub(crate) fn extend_input_methods(
    env: &mut Environment,
    methods: &crate::module::InstanceFnRegistry,
    function_context: Option<&crate::module::FunctionContext>,
) {
    let function_context = function_context.map(|context| std::sync::Arc::new(context.clone()));
    for (instance_label, templates) in methods {
        let has_private_actor_identity = matches!(
            env.get(instance_label),
            Some(Value::ActorRef(actor)) if !actor.is_caller()
        );
        if !has_private_actor_identity {
            env.set(
                instance_label,
                Value::ActorRef(crate::timeline::plan_value::ActorRef::caller(
                    instance_label.clone(),
                )),
            );
        }
        let scope = fn_eval::component_method_scope(instance_label, templates);
        for (name, template) in templates {
            env.set_component_method(
                instance_label,
                name,
                fn_eval::component_user_function_value(
                    name,
                    template,
                    function_context.clone(),
                    std::sync::Arc::clone(&scope),
                ),
            );
        }
    }
}

fn seed_input_namespace_functions(
    env: &mut Environment,
    prefix: &str,
    namespace: &crate::module::Namespace,
    context: &std::sync::Arc<crate::module::FunctionContext>,
    seeded: &mut std::collections::HashSet<String>,
) {
    for (name, template) in &namespace.function_exports {
        let qualified = format!("{prefix}{name}");
        env.set(&qualified, fn_eval::user_function_value(name, template, Some(context.clone())));
        seeded.insert(qualified);
    }
    for (alias, nested) in &namespace.namespaces {
        seed_input_namespace_functions(env, &format!("{prefix}{alias}::"), nested, context, seeded);
    }
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

        let (prepared_env, diagnostics) =
            prepare_input_environment(&ast, &namespaces, &overrides, None, None);
        assert!(diagnostics.is_empty(), "{diagnostics:?}");
        let expression = Expr::Binary(
            Box::new(Expr::call("add_one", vec![Expr::Ident("gain".to_string())])),
            BinaryOp::Add,
            Box::new(Expr::QualifiedPath(vec!["theme".to_string(), "factor".to_string()])),
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
        let (mut env, diagnostics) =
            prepare_input_environment(&[], &Default::default(), &[], None, None);
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
