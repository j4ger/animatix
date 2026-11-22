//! Evaluation of ordinary user functions (`fn f(...) -> T`).
//!
//! Function bodies use their defining module's lexical environment with host
//! globals and frame inputs shared explicitly, while caller-local bindings stay
//! out of scope. Returned timeline plans remain inert until their caller mounts
//! them into a timeline.

use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

use animatix_core::callable::{ArgumentParameter, ArgumentSlot, bind_arguments};
use animatix_syntax::ast::{Expr, InlineItem, LoopPattern};
use animatix_syntax::visit::{
    AstFolder, AstVisitor, walk_expr, walk_fold_stmt, walk_inline_item, walk_stmt,
};

use crate::ast::Stmt;
use crate::timeline::build::pattern_matches;
use crate::timeline::env::{Environment, Value};
use crate::timeline::lookup::for_iter_values;
use crate::timeline::{EvalError, utils};

thread_local! {
    static USER_FN_DEPTH: Cell<usize> = const { Cell::new(0) };
}

struct UserFnDepthGuard;

const MAX_USER_FN_DEPTH: usize = 32;

impl UserFnDepthGuard {
    fn enter(name: &str) -> Result<Self, EvalError> {
        USER_FN_DEPTH.with(|depth| {
            let current = depth.get();
            if current >= MAX_USER_FN_DEPTH {
                return Err(EvalError::UnsupportedConstruct(format!(
                    "user function call depth exceeded {MAX_USER_FN_DEPTH} while evaluating '{name}'"
                )));
            }
            depth.set(current + 1);
            Ok(Self)
        })
    }
}

impl Drop for UserFnDepthGuard {
    fn drop(&mut self) {
        USER_FN_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

/// Control flow signal produced by a statement block.
enum Flow {
    /// The block finished without a `return`.
    Continue,
    /// A `return` unwound with a value.
    Return(Value),
}

/// Evaluate a user function with ordered positional and named arguments.
pub(crate) fn evaluate_user_fn_named(
    name: &str,
    params: &[crate::ast::ParamDef],
    return_type: Option<&crate::ast::TypeAnnotation>,
    body: &[Stmt],
    arguments: Vec<(Option<String>, Value)>,
    caller_env: &Environment,
    function_context: Option<&Arc<crate::module::FunctionContext>>,
    definition_path: Option<&Path>,
    component_scope: Option<&Arc<crate::timeline::env::ComponentMethodScope>>,
    points: Option<crate::timeline::modifier_runtime::ir::GeometryLookup<'_>>,
) -> Result<Value, EvalError> {
    let _depth = UserFnDepthGuard::enter(name)?;
    let mut local = if let (Some(context), Some(path)) = (function_context, definition_path) {
        function_module_environment(context, path, caller_env, params, body, component_scope)?
    } else {
        let mut local = caller_env.clone();
        local.set_function_context(
            function_context.cloned().or_else(|| caller_env.function_context.clone()),
        );
        seed_component_methods(&mut local, component_scope, function_context.cloned());
        local.set_active_component_scope(component_scope.cloned());
        local
    };
    let argument_parameters = params
        .iter()
        .map(|param| ArgumentParameter {
            name: &param.name,
            variadic: false,
        })
        .collect::<Vec<_>>();
    let slots =
        bind_arguments(name, &argument_parameters, arguments).map_err(EvalError::TypeMismatch)?;
    let mut bound = slots
        .into_iter()
        .map(|slot| match slot {
            ArgumentSlot::Single(value) => Ok(value),
            ArgumentSlot::Variadic(_) => Err(EvalError::UnsupportedConstruct(
                "user-function parameters do not support variadic arguments".into(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    local.push_let_scope([]);
    if let Some(scope) = component_scope
        && let Some(captures) = scope.component_captures.get(name)
    {
        for capture in captures {
            if let Some(value) = scope
                .captured_values
                .get(name)
                .and_then(|values| values.get(capture))
                .cloned()
                .or_else(|| caller_env.get(capture))
            {
                local.set_let_binding(capture, value);
            }
        }
    }
    if let Some((name, index)) = component_scope.and_then(|scope| scope.bound_index.as_ref()) {
        local.set_let_binding(name, Value::Num(*index as f64));
    }
    let eval_result = (|| {
        for (index, param) in params.iter().enumerate() {
            let mut value = if let Some(value) = bound[index].take() {
                value
            } else if let Some(default) = &param.default {
                utils::evaluate_expr_with_points(default, &local, points)?
            } else {
                return Err(EvalError::TypeMismatch(format!(
                    "function '{name}' is missing argument for parameter '{}'",
                    param.name
                )));
            };
            if let Some(expected) = &param.param_type {
                let expected = crate::timeline::callable::type_annotation_to_callable(expected);
                value = crate::timeline::callable::coerce_value(
                    &expected,
                    value,
                    &format!("parameter '{}' of '{name}'", param.name),
                )?;
            }
            local.set_let_binding(&param.name, value);
            if param.name == "t" {
                local.set_authored_t_binding(true);
            }
        }

        if let Some(actor) = component_scope.and_then(|scope| scope.bound_actor.as_ref())
            && !params.iter().any(|param| param.name == "self")
        {
            local.set_let_binding("self", Value::ActorRef(actor.clone()));
        }

        match exec_block(body, &mut local, points)? {
            Flow::Return(value) => Ok(value),
            Flow::Continue => Ok(Value::Unit),
        }
    })();
    local.pop_let_scope();
    let value = eval_result?;
    if contains_private_actor(&value) {
        return Err(EvalError::UnsupportedConstruct(format!(
            "plan-local actor reference cannot escape function '{name}' outside a timeline plan"
        )));
    }
    if caller_env.is_frame_phase() && matches!(value, Value::TimelinePlan(_)) {
        return Err(EvalError::UnsupportedConstruct(format!(
            "function '{name}' returned a timeline plan while sampling a frame"
        )));
    }
    if let Some(return_type) = return_type {
        let expected = crate::timeline::callable::type_annotation_to_callable(return_type);
        crate::timeline::callable::validate_value(
            &expected,
            &value,
            &format!("return value of '{name}'"),
        )?;
    }
    Ok(value)
}

fn contains_private_actor(value: &Value) -> bool {
    match value {
        Value::ActorRef(reference) => !reference.is_caller(),
        Value::List(items) => items.iter().any(contains_private_actor),
        Value::Object(_, fields) => fields.values().any(contains_private_actor),
        _ => false,
    }
}

fn function_module_environment(
    context: &Arc<crate::module::FunctionContext>,
    path: &Path,
    caller_env: &Environment,
    params: &[crate::ast::ParamDef],
    body: &[Stmt],
    component_scope: Option<&Arc<crate::timeline::env::ComponentMethodScope>>,
) -> Result<Environment, EvalError> {
    let module = context.modules.get(path).ok_or_else(|| {
        EvalError::UnsupportedConstruct(format!(
            "function source module '{}' is missing from its loaded context",
            path.display()
        ))
    })?;
    let mut env = crate::timeline::builtins::standard_environment();
    env.inherit_native_callables_from(caller_env);
    if path != Path::new("<animatix-memory-root>") {
        env.set_source_path(Some(Arc::new(path.to_path_buf())));
    }
    crate::timeline::BuiltInColorscheme::DefaultDark
        .resolved()
        .seed_environment(&mut env);
    for (name, template) in &module.visible_functions {
        env.set(name, user_function_value(name, template, Some(Arc::clone(context))));
    }
    env.inherit_host_context_from(caller_env);
    env.set_function_context(Some(Arc::clone(context)));
    for (alias, namespace) in &module.namespaces {
        seed_module_namespace(&mut env, &format!("{alias}::"), namespace, context)?;
    }
    let dims = crate::timeline::SceneDimensions::default();
    if !env.has_host_global("scene_width") {
        env.set_host_global("scene_width", Value::Num(dims.width as f64));
    }
    if !env.has_host_global("scene_height") {
        env.set_host_global("scene_height", Value::Num(dims.height as f64));
    }
    seed_component_methods(&mut env, component_scope, Some(Arc::clone(context)));
    env.set_active_component_scope(component_scope.cloned());
    seed_referenced_module_lets(module, &mut env, params, body)?;
    Ok(env)
}

fn seed_component_methods(
    env: &mut Environment,
    component_scope: Option<&Arc<crate::timeline::env::ComponentMethodScope>>,
    function_context: Option<Arc<crate::module::FunctionContext>>,
) {
    let Some(component_scope) = component_scope else {
        return;
    };
    if let Some(actor) = &component_scope.bound_actor {
        env.set(&component_scope.source_label, Value::ActorRef(actor.clone()));
        env.set(actor.label(), Value::ActorRef(actor.clone()));
    }
    if let Some(table_id) = component_scope
        .bound_actor
        .as_ref()
        .and_then(crate::timeline::plan_value::ActorRef::method_table_id)
    {
        for name in component_scope.methods.keys() {
            if let Some(value) = env.component_method_by_table(table_id, name) {
                env.set(name, value);
            }
        }
        return;
    }
    for (name, template) in component_scope.methods.iter() {
        let value = component_user_function_value(
            name,
            template,
            function_context.clone(),
            Arc::clone(component_scope),
        );
        env.set(name, value.clone());
        env.set_component_method(&component_scope.source_label, name, value);
    }
}

fn seed_referenced_module_lets(
    module: &crate::module::FunctionModule,
    env: &mut Environment,
    params: &[crate::ast::ParamDef],
    body: &[Stmt],
) -> Result<(), EvalError> {
    let module_lets = module
        .statements
        .iter()
        .filter_map(|statement| match statement {
            Stmt::LetDecl { name, value, .. } => Some((name.clone(), value)),
            _ => None,
        })
        .collect::<HashMap<_, _>>();
    let mut needed = referenced_names(body);
    for param in params {
        needed.remove(&param.name);
        if let Some(default) = &param.default {
            needed.extend(referenced_expr_names(default));
        }
    }
    for param in params {
        needed.remove(&param.name);
    }
    let mut loaded = HashSet::new();
    let mut visiting = HashSet::new();
    for name in needed {
        if module_lets.contains_key(&name) {
            seed_module_let(&name, &module_lets, env, &mut loaded, &mut visiting)?;
        }
    }
    Ok(())
}

fn seed_module_let(
    name: &str,
    module_lets: &HashMap<String, &Expr>,
    env: &mut Environment,
    loaded: &mut HashSet<String>,
    visiting: &mut HashSet<String>,
) -> Result<(), EvalError> {
    if env.has_host_global(name) {
        loaded.insert(name.to_string());
        return Ok(());
    }
    if loaded.contains(name) {
        return Ok(());
    }
    if !visiting.insert(name.to_string()) {
        return Err(EvalError::UnsupportedConstruct(format!(
            "cyclic module-level let dependency involving '{name}'"
        )));
    }
    let expression = module_lets[name];
    for dependency in referenced_expr_names(expression) {
        if module_lets.contains_key(&dependency) {
            seed_module_let(&dependency, module_lets, env, loaded, visiting)?;
        }
    }
    let value = utils::evaluate_expr(expression, env)?;
    if name == "t" {
        env.set_authored_t(value);
    } else {
        env.set(name, value);
    }
    visiting.remove(name);
    loaded.insert(name.to_string());
    Ok(())
}

fn referenced_names(body: &[Stmt]) -> HashSet<String> {
    let mut references = IdentifierReferences::default();
    let mut locals = LocalNames::default();
    for statement in body {
        references.visit_stmt(statement);
        locals.visit_stmt(statement);
    }
    references.names.retain(|name| !locals.names.contains(name));
    references.names
}

fn referenced_expr_names(expression: &Expr) -> HashSet<String> {
    let mut references = IdentifierReferences::default();
    references.visit_expr(expression);
    references.names
}

#[derive(Default)]
struct IdentifierReferences {
    names: HashSet<String>,
}

impl<'ast> AstVisitor<'ast> for IdentifierReferences {
    fn visit_expr(&mut self, expression: &'ast Expr) {
        match expression {
            Expr::Ident(name) => {
                self.names.insert(name.clone());
            },
            Expr::Path(parts) => {
                if let Some(root) = parts.first() {
                    self.names.insert(root.clone());
                }
            },
            Expr::QualifiedPath(parts) => {
                self.names.insert(parts.join("::"));
            },
            _ => walk_expr(self, expression),
        }
    }
}

#[derive(Default)]
struct LocalNames {
    names: HashSet<String>,
}

impl<'ast> AstVisitor<'ast> for LocalNames {
    fn visit_stmt(&mut self, statement: &'ast Stmt) {
        match statement {
            Stmt::LetDecl { name, .. } | Stmt::FnDecl { name, .. } => {
                self.names.insert(name.clone());
            },
            Stmt::ForLoop { var, index_var, .. } => {
                self.add_loop_pattern(var);
                if let Some(index) = index_var {
                    self.names.insert(index.clone());
                }
            },
            _ => {},
        }
        walk_stmt(self, statement);
    }

    fn visit_expr(&mut self, expression: &'ast Expr) {
        match expression {
            Expr::Closure(params, _) => self.names.extend(params.iter().cloned()),
            Expr::LetChain(bindings, _) => {
                self.names.extend(bindings.iter().map(|(name, _)| name.clone()));
            },
            _ => {},
        }
        walk_expr(self, expression);
    }

    fn visit_inline_item(&mut self, item: &'ast InlineItem) {
        if let InlineItem::ForLoop { var, index_var, .. } = item {
            self.add_loop_pattern(var);
            if let Some(index) = index_var {
                self.names.insert(index.clone());
            }
        }
        walk_inline_item(self, item);
    }
}

impl LocalNames {
    fn add_loop_pattern(&mut self, pattern: &LoopPattern) {
        match pattern {
            LoopPattern::Single(name) => {
                self.names.insert(name.clone());
            },
            LoopPattern::Tuple(names) => self.names.extend(names.iter().cloned()),
        }
    }
}

fn seed_module_namespace(
    env: &mut Environment,
    prefix: &str,
    namespace: &crate::module::Namespace,
    context: &Arc<crate::module::FunctionContext>,
) -> Result<(), EvalError> {
    for (name, template) in &namespace.function_exports {
        env.set(
            &format!("{prefix}{name}"),
            user_function_value(name, template, Some(Arc::clone(context))),
        );
    }
    for (name, expression) in &namespace.exports {
        let value = utils::evaluate_expr(expression, env)?;
        env.set(&format!("{prefix}{name}"), value);
    }
    for (alias, nested) in &namespace.namespaces {
        seed_module_namespace(env, &format!("{prefix}{alias}::"), nested, context)?;
    }
    Ok(())
}

pub(crate) fn user_function_value(
    name: &str,
    template: &crate::module::FnTemplate,
    function_context: Option<Arc<crate::module::FunctionContext>>,
) -> Value {
    Value::UserFn {
        name: name.to_string(),
        params: Arc::new(template.params.clone()),
        return_type: template.return_type.clone(),
        body: Arc::new(template.body.clone()),
        function_context,
        definition_path: Some(template.definition_path.clone()),
        component_scope: None,
    }
}

pub(crate) fn component_user_function_value(
    name: &str,
    template: &crate::module::FnTemplate,
    function_context: Option<Arc<crate::module::FunctionContext>>,
    component_scope: Arc<crate::timeline::env::ComponentMethodScope>,
) -> Value {
    Value::UserFn {
        name: name.to_string(),
        params: Arc::new(template.params.clone()),
        return_type: template.return_type.clone(),
        body: Arc::new(template.body.clone()),
        function_context,
        definition_path: Some(template.definition_path.clone()),
        component_scope: Some(component_scope),
    }
}

pub(crate) fn component_method_scope(
    source_label: &str,
    methods: &HashMap<String, crate::module::FnTemplate>,
) -> Arc<crate::timeline::env::ComponentMethodScope> {
    Arc::new(crate::timeline::env::ComponentMethodScope {
        source_label: source_label.to_string(),
        methods: Arc::new(methods.clone()),
        component_captures: Arc::new(
            methods
                .iter()
                .map(|(name, method)| (name.clone(), method.component_captures.clone()))
                .collect(),
        ),
        captured_values: Arc::new(HashMap::new()),
        instance_index: methods.values().find_map(|method| method.instance_index.clone()),
        bound_actor: None,
        bound_index: None,
    })
}

/// Create an immutable method table specialized for one World entity. All
/// methods share one scope Arc so private sibling calls retain lexical access.
pub(crate) fn bind_component_method_values(
    methods: &HashMap<String, Value>,
    actor: &crate::timeline::plan_value::ActorRef,
    caller_env: &Environment,
) -> HashMap<String, Value> {
    let Some(source_scope) = methods.values().find_map(|value| match value {
        Value::UserFn {
            component_scope: Some(scope),
            ..
        } => Some(scope),
        _ => None,
    }) else {
        return methods.clone();
    };

    let mut bound_scope = source_scope.as_ref().clone();
    bound_scope.bound_actor = Some(actor.clone());
    bound_scope.bound_index = component_instance_index(source_scope, actor);
    let captured_values = source_scope
        .component_captures
        .iter()
        .map(|(method, names)| {
            let values = names
                .iter()
                .filter_map(|name| caller_env.get(name).map(|value| (name.clone(), value)))
                .collect();
            (method.clone(), values)
        })
        .collect();
    bound_scope.captured_values = Arc::new(captured_values);
    let bound_scope = Arc::new(bound_scope);

    methods
        .iter()
        .map(|(name, value)| {
            let Value::UserFn {
                name: function_name,
                params,
                return_type,
                body,
                function_context,
                definition_path,
                component_scope: Some(_),
            } = value
            else {
                return (name.clone(), value.clone());
            };
            let specialized_body =
                specialize_component_method_body(body, params, source_scope, actor);
            (
                name.clone(),
                Value::UserFn {
                    name: function_name.clone(),
                    params: Arc::clone(params),
                    return_type: return_type.clone(),
                    body: Arc::new(specialized_body),
                    function_context: function_context.clone(),
                    definition_path: definition_path.clone(),
                    component_scope: Some(Arc::clone(&bound_scope)),
                },
            )
        })
        .collect()
}

/// Bind a component method value to the actor used at its call site.
pub(crate) fn bind_component_method_receiver(
    function: Value,
    actor: &crate::timeline::plan_value::ActorRef,
) -> Value {
    let Value::UserFn {
        name,
        params,
        return_type,
        body,
        function_context,
        definition_path,
        component_scope: Some(scope),
    } = function
    else {
        return function;
    };
    let specialized_body = specialize_component_method_body(&body, &params, &scope, actor);
    let mut bound_scope = scope.as_ref().clone();
    bound_scope.bound_actor = Some(actor.clone());
    bound_scope.bound_index = component_instance_index(&scope, actor);
    Value::UserFn {
        name,
        params,
        return_type,
        body: Arc::new(specialized_body),
        function_context,
        definition_path,
        component_scope: Some(Arc::new(bound_scope)),
    }
}

fn component_instance_index(
    scope: &crate::timeline::env::ComponentMethodScope,
    actor: &crate::timeline::plan_value::ActorRef,
) -> Option<(String, usize)> {
    let crate::ast::Expr::Ident(name) = scope.instance_index.as_ref()? else {
        return None;
    };
    let source = scope.source_label.split('.').collect::<Vec<_>>();
    let concrete = actor.label().split('.').collect::<Vec<_>>();
    source.iter().zip(concrete).rev().find_map(|(source_part, concrete_part)| {
        let (base, index) = concrete_part.rsplit_once("__")?;
        if base != *source_part {
            return None;
        }
        index.parse::<usize>().ok().map(|index| (name.clone(), index))
    })
}

fn specialize_component_method_body(
    body: &[Stmt],
    params: &[crate::ast::ParamDef],
    scope: &crate::timeline::env::ComponentMethodScope,
    actor: &crate::timeline::plan_value::ActorRef,
) -> Vec<Stmt> {
    let params = params.iter().map(|param| param.name.clone()).collect::<Vec<_>>();
    let body = animatix_syntax::module::specialize_instance_property_reads(
        body.to_vec(),
        &scope.source_label,
        actor.label(),
        &params,
    );
    let mut folder = ComponentMethodTargetFolder {
        source: scope.source_label.split('.').map(str::to_owned).collect(),
        target: actor.label().split('.').map(str::to_owned).collect(),
    };
    folder.fold_stmts(body)
}

struct ComponentMethodTargetFolder {
    source: Vec<String>,
    target: Vec<String>,
}

impl AstFolder for ComponentMethodTargetFolder {
    fn fold_stmt(&mut self, statement: Stmt) -> Stmt {
        match statement {
            Stmt::Assignment {
                target,
                property,
                value,
                modifiers,
                easing,
                value_span,
                span,
            } => Stmt::Assignment {
                target: self.remap_target(target),
                property,
                value: self.fold_expr(value),
                modifiers: modifiers
                    .into_iter()
                    .map(|modifier| self.fold_modifier(modifier))
                    .collect(),
                easing,
                value_span,
                span,
            },
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                span,
            } => Stmt::ReactiveBinding {
                target: self.remap_target(target),
                property,
                value: self.fold_expr(value),
                value_span,
                span,
            },
            other => walk_fold_stmt(self, other),
        }
    }
}

impl ComponentMethodTargetFolder {
    fn remap_target(
        &self,
        mut target: Vec<crate::ast::TargetSegment>,
    ) -> Vec<crate::ast::TargetSegment> {
        if self.source.len() != self.target.len() || target.len() < self.source.len() {
            return target;
        }
        let matches_source =
            target
                .iter()
                .take(self.source.len())
                .zip(&self.source)
                .all(|(segment, source)| match segment {
                    crate::ast::TargetSegment::Static(label) => label == source,
                    crate::ast::TargetSegment::Indexed { base, .. } => base == source,
                });
        if matches_source {
            for (index, label) in self.target.iter().enumerate() {
                target[index] = crate::ast::TargetSegment::Static(label.clone());
            }
        }
        target
    }
}

/// Execute a statement block, propagating `return` unwinds.
fn exec_block(
    stmts: &[Stmt],
    env: &mut Environment,
    points: Option<crate::timeline::modifier_runtime::ir::GeometryLookup<'_>>,
) -> Result<Flow, EvalError> {
    exec_block_scoped(stmts, env, points)
}

fn exec_block_scoped(
    stmts: &[Stmt],
    env: &mut Environment,
    points: Option<crate::timeline::modifier_runtime::ir::GeometryLookup<'_>>,
) -> Result<Flow, EvalError> {
    for stmt in stmts {
        match stmt {
            Stmt::LetDecl { name, value, .. } => {
                let value = utils::evaluate_expr_with_points(value, env, points)?;
                if name == "t" {
                    env.set_authored_t_binding(true);
                }
                env.set_function_local_binding(name, value);
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let truthy = utils::evaluate_expr_with_points(condition, env, points)?.is_truthy();
                let flow = if truthy {
                    exec_block(then_branch, env, points)?
                } else if let Some(else_branch) = else_branch {
                    exec_block(else_branch, env, points)?
                } else {
                    Flow::Continue
                };
                if let Flow::Return(value) = flow {
                    return Ok(Flow::Return(value));
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                let value = utils::evaluate_expr_with_points(scrutinee, env, points)?;
                for (pat, arm_body) in arms {
                    if pattern_matches(pat, &value) {
                        if let Flow::Return(value) = exec_block(arm_body, env, points)? {
                            return Ok(Flow::Return(value));
                        }
                        break;
                    }
                }
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                ..
            } => {
                for (idx, value) in for_iter_values(iterable, env).into_iter().enumerate() {
                    let authored_t = env.has_authored_t_binding();
                    let mut bindings = function_loop_bindings(var, value)?;
                    if let Some(index_var) = index_var {
                        bindings.push((index_var.clone(), Value::Num(idx as f64)));
                    }
                    env.push_let_scope(bindings);
                    if var_mentions_t(var) || index_var.as_ref().is_some_and(|name| name == "t") {
                        env.set_authored_t_binding(true);
                    }
                    let result = exec_block(body, env, points);
                    env.pop_let_scope();
                    env.set_authored_t_binding(authored_t);
                    if let Flow::Return(value) = result? {
                        return Ok(Flow::Return(value));
                    }
                }
            },
            Stmt::Return { value, .. } => {
                let value = match value {
                    Some(expr) => utils::evaluate_expr_with_points(expr, env, points)?,
                    None => Value::Unit,
                };
                return Ok(Flow::Return(value));
            },
            Stmt::Expr(expr, ..) => {
                let value = utils::evaluate_expr_with_points(expr, env, points)?;
                if !matches!(value, Value::Unit) {
                    return Err(EvalError::UnsupportedConstruct(format!(
                        "function expression statements must return Unit; bind or return {value:?}"
                    )));
                }
            },
            Stmt::Block {
                body,
                instance_scope: None,
                ..
            } => {
                let mut nested = env.clone();
                if let Flow::Return(value) = exec_block(body, &mut nested, points)? {
                    return Ok(Flow::Return(value));
                }
            },
            Stmt::Comment(..) | Stmt::TypeAlias { .. } => {},
            Stmt::ActorDecl { .. }
            | Stmt::Assignment { .. }
            | Stmt::ReactiveBinding { .. }
            | Stmt::Always { .. }
            | Stmt::Keyframe { .. }
            | Stmt::RelativeKeyframe { .. }
            | Stmt::Sequence { .. }
            | Stmt::Stagger { .. }
            | Stmt::Step { .. }
            | Stmt::Play { .. }
            | Stmt::Scene { .. }
            | Stmt::Config { .. }
            | Stmt::Import { .. }
            | Stmt::ComponentDef(..)
            | Stmt::FnDecl { .. }
            | Stmt::Block {
                instance_scope: Some(_),
                ..
            } => {
                return Err(EvalError::UnsupportedConstruct(
                    "timeline declarations and statements must be inside a timeline expression"
                        .into(),
                ));
            },
        }
    }
    Ok(Flow::Continue)
}

fn function_loop_bindings(
    pattern: &animatix_syntax::ast::LoopPattern,
    value: Value,
) -> Result<Vec<(String, Value)>, EvalError> {
    use animatix_syntax::ast::LoopPattern;
    match pattern {
        LoopPattern::Single(name) => Ok(vec![(name.clone(), value)]),
        LoopPattern::Tuple(names) => {
            let values = match value {
                Value::List(items) => items.to_vec(),
                Value::Vec2(items) => items.into_iter().map(Value::Num).collect(),
                Value::Vec3(items) => items.into_iter().map(Value::Num).collect(),
                Value::Vec4(items) => items.into_iter().map(Value::Num).collect(),
                Value::Color(items) => items.into_iter().map(Value::Num).collect(),
                other => {
                    return Err(EvalError::TypeMismatch(format!(
                        "tuple loop pattern needs a list or vector, got {other:?}"
                    )));
                },
            };
            if values.len() != names.len() {
                return Err(EvalError::TypeMismatch(format!(
                    "tuple loop pattern expects {} values, got {}",
                    names.len(),
                    values.len()
                )));
            }
            Ok(names.iter().cloned().zip(values).collect())
        },
    }
}

fn var_mentions_t(pattern: &animatix_syntax::ast::LoopPattern) -> bool {
    match pattern {
        animatix_syntax::ast::LoopPattern::Single(name) => name == "t",
        animatix_syntax::ast::LoopPattern::Tuple(names) => names.iter().any(|name| name == "t"),
    }
}

/// Collect pure function declarations from the AST so the build environment
/// can seed them as callable [`Value::UserFn`] values.
type UserFnSeed =
    (String, Vec<crate::ast::ParamDef>, Option<crate::ast::TypeAnnotation>, Vec<Stmt>);

pub(crate) fn collect_user_fns(ast: &[Stmt]) -> Vec<UserFnSeed> {
    let mut fns = Vec::new();
    for stmt in ast {
        if let Stmt::FnDecl {
            name,
            params,
            return_type,
            body,
            ..
        } = stmt
        {
            fns.push((name.clone(), params.clone(), return_type.clone(), body.clone()));
        }
    }
    fns
}

/// Build a lexical function context for public AST-only build APIs that do
/// not receive a ModuleGraph. This keeps sibling functions and top-level lets
/// tied to their source body instead of accidentally capturing caller locals.
pub(crate) fn context_from_ast(
    ast: &[Stmt],
    namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
) -> (Arc<crate::module::FunctionContext>, std::path::PathBuf) {
    context_from_ast_at(ast, namespaces, None)
}

/// Build the AST-only context while retaining an optional file identity for
/// source-relative assets.
pub(crate) fn context_from_ast_at(
    ast: &[Stmt],
    namespaces: &std::collections::HashMap<String, crate::module::Namespace>,
    source_path: Option<&Path>,
) -> (Arc<crate::module::FunctionContext>, std::path::PathBuf) {
    let path = source_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::path::PathBuf::from("<animatix-memory-root>"));
    let mut functions = std::collections::HashMap::new();
    for statement in ast {
        if let Stmt::FnDecl {
            is_pub,
            name,
            params,
            return_type,
            body,
            span,
        } = statement
        {
            functions.insert(
                name.clone(),
                crate::module::FnTemplate {
                    params: params.clone(),
                    return_type: return_type.clone(),
                    body: body.clone(),
                    is_pub: *is_pub,
                    definition_path: path.clone(),
                    span: *span,
                    instance_index: None,
                    component_captures: Vec::new(),
                },
            );
        }
    }
    let module = crate::module::FunctionModule {
        statements: ast.to_vec(),
        functions: functions.clone(),
        visible_functions: functions.clone(),
        components: Default::default(),
        namespaces: namespaces.clone(),
    };
    let context = crate::module::FunctionContext {
        visible: Arc::new(functions),
        modules: Arc::new(std::collections::HashMap::from([(path.clone(), module)])),
        scene_paths: Arc::new(std::collections::HashMap::new()),
    };
    (Arc::new(context), path)
}
