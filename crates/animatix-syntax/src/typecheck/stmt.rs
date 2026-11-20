//! Statement walking, function return inference, and call binding.

use super::{CallableParamType, TypeEnv};
use crate::ast::{
    CallArgument, ComponentDef, Expr, InlineItem, LoopPattern, MatchPattern, ParamDef, Property,
    Stmt,
};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::typing::{self, Type as TypedType};
use animatix_core::callable::{ArgumentParameter, ArgumentSlot};

impl<'a> TypeEnv<'a> {
    pub(super) fn check_stmt(&mut self, stmt: &Stmt, diagnostics: &mut Vec<Diagnostic>) {
        if self.function_depth > 0
            && let Some((kind, span)) = ordinary_fn_statement_violation(stmt)
        {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::InvalidFunctionBodyStatement,
                    DiagnosticPhase::Build,
                    format!(
                        "{kind} is a timeline statement; put it inside `#{{ ... }}` and return the plan from this function"
                    ),
                )
                .with_ast_span(span)
                .with_subject(kind),
            );
            return;
        }
        match stmt {
            Stmt::ActorDecl {
                label,
                array_index,
                ty,
                props,
                children,
                ..
            } => {
                self.check_actor_decl(
                    label,
                    array_index.is_some(),
                    ty,
                    props,
                    children,
                    diagnostics,
                );
            },
            Stmt::LetDecl { name, value, .. } => {
                let ty = typing::infer_expr_type(value, &self.typed);
                self.typed.bind(name, ty);
                if let Some(source_name) = value.callable_name()
                    && matches!(value, Expr::Ident(_) | Expr::Path(_) | Expr::QualifiedPath(_))
                {
                    let source_name =
                        self.function_aliases.get(&source_name).cloned().unwrap_or(source_name);
                    if self.module_fns.contains_key(&source_name)
                        || self.namespace_fns.contains_key(&source_name)
                        || self.callables.contains_key(&source_name)
                        || super::standard_callable_signature(&source_name).is_some()
                    {
                        self.function_aliases.insert(name.clone(), source_name);
                        self.callables.remove(name);
                    } else {
                        self.callables.remove(name);
                        self.function_aliases.remove(name);
                    }
                } else {
                    self.callables.remove(name);
                    self.function_aliases.remove(name);
                }
                self.check_expr_calls(value, diagnostics);
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::Scene { body, .. } => {
                self.check_stmt_body(body, diagnostics);
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.check_expr_calls(condition, diagnostics);
                self.check_stmt_body(then_branch, diagnostics);
                if let Some(else_body) = else_branch {
                    self.check_stmt_body(else_body, diagnostics);
                }
            },
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                self.check_expr_calls(scrutinee, diagnostics);
                self.check_match(arms, diagnostics);
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                ..
            } => {
                self.check_expr_calls(iterable, diagnostics);
                self.check_for_loop(var, index_var, iterable, body, diagnostics);
            },
            Stmt::Expr(expr, span) => {
                self.check_expr_calls(expr, diagnostics);
                if self.function_depth > 0
                    && typing::infer_expr_type(expr, &self.typed) == TypedType::Timeline
                {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::DiscardedTimelineValue,
                            DiagnosticPhase::Build,
                            "Timeline value is discarded in a function; put the statement inside `#{ ... }` or return the value",
                        )
                        .with_ast_span(*span),
                    );
                }
            },
            Stmt::Return {
                value: Some(expr), ..
            } => self.check_expr_calls(expr, diagnostics),
            Stmt::Assignment {
                target,
                property,
                value,
                value_span,
                ..
            }
            | Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                ..
            } => {
                self.check_expr_calls(value, diagnostics);
                self.check_color_assignment(target, property, value, *value_span, diagnostics);
            },
            Stmt::ComponentDef(def, _span) => {
                self.check_component_def(def, diagnostics);
            },
            Stmt::FnDecl {
                name,
                params,
                return_type,
                body,
                ..
            } => {
                self.check_fn_decl(name, params, return_type, body, diagnostics);
            },
            Stmt::Config { .. } => {
                // Config is handled at the program level, not per-statement
            },
            _ => {},
        }
    }

    /// Check the statements of a body-bearing variant in source order.
    fn check_stmt_body(&mut self, body: &[Stmt], diagnostics: &mut Vec<Diagnostic>) {
        let previous_aliases = self.function_aliases.clone();
        let previous_callables = self.callables.clone();
        for stmt in body {
            self.check_stmt(stmt, diagnostics);
        }
        self.function_aliases = previous_aliases;
        self.callables = previous_callables;
    }

    /// Check an actor declaration: label/type bookkeeping, component props, children.
    fn check_actor_decl(
        &mut self,
        label: &str,
        has_array_index: bool,
        ty: &str,
        props: &[Property],
        children: &[InlineItem],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Track label → type for actor method lookup.
        self.labels.insert(label.to_string(), ty.to_string());
        for prop in props {
            self.check_expr_calls(&prop.value, diagnostics);
        }
        if has_array_index {
            self.typed.declare_array(label, TypedType::Actor(ty.to_string()));
        } else if self.components.contains_key(ty) {
            self.typed.declare_component_instance(label, ty);
        } else {
            self.typed.declare_actor(label, ty);
        }
        // Check if this actor decl instantiates a component
        if let Some(entry) = self.components.get(ty) {
            self.check_component_props(ty, &entry.definition, props, diagnostics);
        } else {
            self.check_primitive_color_props(ty, props, diagnostics);
        }
        // Recurse into children
        for child in children {
            self.check_inline_item(child, diagnostics);
        }
    }

    /// Check a `match` block: require the wildcard arm, then walk every arm body.
    fn check_match(
        &mut self,
        arms: &[(MatchPattern, Vec<Stmt>)],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Enforce `_` wildcard arm (required by spec — Amendment 1)
        let has_wildcard = arms.iter().any(|(pat, _)| matches!(pat, MatchPattern::Wildcard));
        if !has_wildcard {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::MissingWildcardArm,
                DiagnosticPhase::Parse,
                "match must have a `_` wildcard arm (required for exhaustive matching)",
            ));
        }
        for (_, body) in arms {
            self.check_stmt_body(body, diagnostics);
        }
    }

    /// Check a `for` loop: bind the loop variables in a scope, then walk the body.
    fn check_for_loop(
        &mut self,
        var: &LoopPattern,
        index_var: &Option<String>,
        iterable: &crate::ast::Expr,
        body: &[Stmt],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let iterable_ty = typing::infer_expr_type(iterable, &self.typed);
        let element_ty = match iterable_ty {
            TypedType::List(inner) => *inner,
            _ => TypedType::Any,
        };
        self.typed.push_scope();
        match var {
            LoopPattern::Single(name) => {
                self.typed.bind(name, element_ty.clone());
            },
            LoopPattern::Tuple(names) => {
                for name in names {
                    self.typed.bind(name, element_ty.clone());
                }
            },
        }
        if let Some(index_var) = index_var {
            self.typed.bind(index_var, TypedType::Num);
        }
        let previous_aliases = self.function_aliases.clone();
        let previous_callables = self.callables.clone();
        for stmt in body {
            self.check_stmt(stmt, diagnostics);
        }
        self.function_aliases = previous_aliases;
        self.callables = previous_callables;
        self.typed.pop_scope();
    }

    /// Check a component definition: parameters, then the body in a fresh scope.
    fn check_component_def(&mut self, def: &ComponentDef, diagnostics: &mut Vec<Diagnostic>) {
        if self.strict_types {
            self.check_param_annotations(&def.name, "component", &def.params, diagnostics);
        }
        self.check_param_type_aliases(&def.name, "component", &def.params, diagnostics);
        self.typed.push_scope();
        let previous_aliases = self.function_aliases.clone();
        let previous_callables = self.callables.clone();
        for param in &def.params {
            let ty = param
                .param_type
                .as_ref()
                .map(|annotation| self.typed.resolve_annotation(annotation))
                .or_else(|| {
                    param.default.as_ref().map(|value| typing::infer_expr_type(value, &self.typed))
                })
                .unwrap_or(TypedType::Any);
            self.typed.bind(&param.name, ty);
        }
        for stmt in &def.body {
            self.check_stmt(stmt, diagnostics);
        }
        self.typed.pop_scope();
        self.function_aliases = previous_aliases;
        self.callables = previous_callables;
    }

    /// Check a function declaration: parameters, body and declared result.
    fn check_fn_decl(
        &mut self,
        name: &str,
        params: &[ParamDef],
        return_type: &Option<crate::ast::TypeAnnotation>,
        body: &[Stmt],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if self.strict_types {
            self.check_param_annotations(name, "function", params, diagnostics);
        }
        self.check_param_type_aliases(name, "function", params, diagnostics);
        self.typed.push_scope();
        let previous_aliases = self.function_aliases.clone();
        let previous_callables = self.callables.clone();
        for param in params {
            let ty = param
                .param_type
                .as_ref()
                .map(|annotation| self.typed.resolve_annotation(annotation))
                .or_else(|| {
                    param.default.as_ref().map(|value| typing::infer_expr_type(value, &self.typed))
                })
                .unwrap_or(TypedType::Any);
            self.typed.bind(&param.name, ty);
        }
        self.function_depth += 1;
        for stmt in body {
            // Full type-checking with the parameter scope active, so
            // body expressions resolve parameter types.
            self.check_stmt(stmt, diagnostics);
        }
        if let Some(annotation) = return_type {
            self.check_fn_result(name, annotation, body, diagnostics);
        } else {
            self.check_fn_result_union_annotation(name, body, diagnostics);
        }
        self.function_depth = self.function_depth.saturating_sub(1);
        self.typed.pop_scope();
        self.function_aliases = previous_aliases;
        self.callables = previous_callables;
    }

    /// Validate the inferred reachable result against the declared annotation.
    fn check_fn_result(
        &mut self,
        name: &str,
        annotation: &crate::ast::TypeAnnotation,
        body: &[Stmt],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let expected = self.typed.resolve_annotation(annotation);
        let (actual, span) = infer_function_result(body, &self.typed);
        if !typing::is_subtype(&actual, &expected) {
            diagnostics.push(
                Diagnostic::error(
                    DiagnosticCode::TypeMismatch,
                    DiagnosticPhase::Build,
                    format!(
                        "Function '{}' declares return type {}, but returns {}",
                        name, expected, actual
                    ),
                )
                .with_ast_span(span)
                .with_subject(name.to_string()),
            );
        }
    }

    /// Require a declared union when reachable returns cannot share one inferred type.
    fn check_fn_result_union_annotation(
        &self,
        name: &str,
        body: &[Stmt],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let (actual, span) = infer_function_result(body, &self.typed);
        if !requires_explicit_return_union(&actual) {
            return;
        }

        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::TypeMismatch,
                DiagnosticPhase::Build,
                format!(
                    "Function '{name}' has incompatible inferred return types {actual}; declare an explicit union return type"
                ),
            )
            .with_ast_span(span)
            .with_subject(name.to_string()),
        );
    }

    fn check_call(&self, callee: &Expr, args: &[CallArgument], diagnostics: &mut Vec<Diagnostic>) {
        let name = callee.callable_name();
        let display_name = name.as_deref();
        let function_name = name
            .as_deref()
            .map(|name| self.function_aliases.get(name).map(String::as_str).unwrap_or(name));
        let has_user_function = function_name.is_some_and(|name| {
            self.module_fns.contains_key(name) || self.namespace_fns.contains_key(name)
        });
        if !has_user_function && let Some(name) = function_name {
            if let Some(descriptor) = self.callables.get(name) {
                let params = descriptor
                    .params
                    .iter()
                    .map(|param| CallableParamType {
                        name: param.name.as_str(),
                        ty: TypedType::from_callable_type(&param.ty),
                        optional: param.optional || param.default.is_some(),
                        variadic: param.variadic,
                    })
                    .collect::<Vec<_>>();
                self.check_call_arguments(display_name.unwrap_or(name), &params, args, diagnostics);
                return;
            }
            if let Some(signature) = super::standard_callable_signature(name) {
                self.check_call_arguments_with_binding(
                    display_name.unwrap_or(name),
                    &signature.params,
                    &signature.binding_params,
                    args,
                    diagnostics,
                );
                return;
            }
        }

        if let Some(name) = function_name
            && let Some(template) = self.module_fns.get(name)
        {
            let params = template
                .params
                .iter()
                .map(|param| {
                    let ty = param
                        .param_type
                        .as_ref()
                        .map(|annotation| self.typed.resolve_annotation(annotation))
                        .or_else(|| {
                            param
                                .default
                                .as_ref()
                                .map(|value| typing::infer_expr_type(value, &self.typed))
                        })
                        .unwrap_or(TypedType::Any);
                    CallableParamType {
                        name: param.name.as_str(),
                        ty,
                        optional: param.default.is_some(),
                        variadic: false,
                    }
                })
                .collect::<Vec<_>>();
            self.check_call_arguments(name, &params, args, diagnostics);
            return;
        }

        if let Some(name) = function_name
            && let Some(template) = self.namespace_fns.get(name)
        {
            let params = template
                .params
                .iter()
                .map(|param| {
                    let ty = param
                        .param_type
                        .as_ref()
                        .map(|annotation| self.typed.resolve_annotation(annotation))
                        .or_else(|| {
                            param
                                .default
                                .as_ref()
                                .map(|value| typing::infer_expr_type(value, &self.typed))
                        })
                        .unwrap_or(TypedType::Any);
                    CallableParamType {
                        name: param.name.as_str(),
                        ty,
                        optional: param.default.is_some(),
                        variadic: false,
                    }
                })
                .collect::<Vec<_>>();
            self.check_call_arguments(name, &params, args, diagnostics);
            return;
        }

        if let Expr::Field(receiver, method) = callee
            && let TypedType::Component(component) = typing::infer_expr_type(receiver, &self.typed)
            && let Some(template) =
                self.components.get(&component).and_then(|entry| entry.methods.get(method))
        {
            let params = template
                .params
                .iter()
                .map(|param| {
                    let ty = param
                        .param_type
                        .as_ref()
                        .map(|annotation| self.typed.resolve_annotation(annotation))
                        .or_else(|| {
                            param
                                .default
                                .as_ref()
                                .map(|value| typing::infer_expr_type(value, &self.typed))
                        })
                        .unwrap_or(TypedType::Any);
                    CallableParamType {
                        name: param.name.as_str(),
                        ty,
                        optional: param.default.is_some(),
                        variadic: false,
                    }
                })
                .collect::<Vec<_>>();
            self.check_call_arguments(method, &params, args, diagnostics);
        }
    }

    fn check_expr_calls(&self, expr: &Expr, diagnostics: &mut Vec<Diagnostic>) {
        crate::walk::walk_expr(expr, &mut |node| {
            if let Expr::Call(callee, args) = node {
                self.check_call(callee, args, diagnostics);
            }
        });
    }

    fn check_call_arguments(
        &self,
        name: &str,
        params: &[CallableParamType<'_>],
        args: &[CallArgument],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let binding_params = params
            .iter()
            .map(|param| ArgumentParameter {
                name: param.name,
                variadic: param.variadic,
            })
            .collect::<Vec<_>>();
        self.check_call_arguments_with_binding(name, params, &binding_params, args, diagnostics);
    }

    fn check_call_arguments_with_binding(
        &self,
        name: &str,
        params: &[CallableParamType<'_>],
        binding_params: &[ArgumentParameter<'_>],
        args: &[CallArgument],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let bound = animatix_core::callable::bind_arguments(
            name,
            binding_params,
            args.iter().map(|argument| (argument.name.clone(), argument)),
        );
        let slots = match bound {
            Ok(slots) => slots,
            Err(error) => {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::TypeMismatch,
                        DiagnosticPhase::Build,
                        format!("Invalid arguments for function '{name}': {error}"),
                    )
                    .with_subject(name.to_string()),
                );
                return;
            },
        };

        for (param, slot) in params.iter().zip(slots) {
            let param_name = param.name;
            let arguments = match slot {
                ArgumentSlot::Single(Some(argument)) => vec![argument],
                ArgumentSlot::Single(None) => {
                    if !param.optional && !param.variadic {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::TypeMismatch,
                                DiagnosticPhase::Build,
                                format!("Missing required parameter '{param_name}' for function '{name}'"),
                            )
                            .with_subject(format!("{name}.{param_name}")),
                        );
                    }
                    continue;
                },
                ArgumentSlot::Variadic(arguments) => arguments,
            };

            for argument in arguments {
                let actual = typing::infer_expr_type(&argument.value, &self.typed);
                if !typing::is_subtype(&actual, &param.ty) {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::TypeMismatch,
                            DiagnosticPhase::Build,
                            format!(
                                "Type mismatch: function '{}' parameter '{}' expects {}, got {} (from {})",
                                name,
                                param_name,
                                param.ty,
                                actual,
                                super::expr::expr_summary(&argument.value)
                            ),
                        )
                        .with_subject(format!("{name}.{param_name}"))
                        .with_byte_span(argument.byte_span),
                    );
                }
            }
        }
    }

    fn check_inline_item(
        &mut self,
        item: &crate::ast::InlineItem,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match item {
            crate::ast::InlineItem::Anonymous {
                ty,
                props,
                children,
                ..
            } => {
                if let Some(entry) = self.components.get(ty) {
                    self.check_component_props(ty, &entry.definition, props, diagnostics);
                } else {
                    self.check_primitive_color_props(ty, props, diagnostics);
                }
                for child in children {
                    self.check_inline_item(child, diagnostics);
                }
            },
            crate::ast::InlineItem::Labeled {
                label,
                array_index,
                ty,
                props,
                children,
                ..
            } => {
                if array_index.is_some() {
                    self.typed.declare_array(label, TypedType::Actor(ty.clone()));
                } else if self.components.contains_key(ty) {
                    self.typed.declare_component_instance(label, ty);
                } else {
                    self.typed.declare_actor(label, ty);
                }
                if let Some(entry) = self.components.get(ty) {
                    self.check_component_props(ty, &entry.definition, props, diagnostics);
                } else {
                    self.check_primitive_color_props(ty, props, diagnostics);
                }
                for child in children {
                    self.check_inline_item(child, diagnostics);
                }
            },
            crate::ast::InlineItem::SlotFill { items, .. } => {
                for item in items {
                    self.check_inline_item(item, diagnostics);
                }
            },
            crate::ast::InlineItem::Expr(expr, _) => {
                self.check_expr_calls(expr, diagnostics);
            },
            _ => {},
        }
    }
}

fn ordinary_fn_statement_violation(
    stmt: &Stmt,
) -> Option<(&'static str, Option<crate::ast::Span>)> {
    match stmt {
        Stmt::ActorDecl { span, .. } => Some(("Actor declaration", *span)),
        Stmt::Keyframe { span, .. } => Some(("Keyframe", *span)),
        Stmt::RelativeKeyframe { span, .. } => Some(("Relative keyframe", *span)),
        Stmt::Assignment { span, .. } => Some(("Property assignment", *span)),
        Stmt::ReactiveBinding { span, .. } => Some(("Reactive binding", *span)),
        Stmt::Sequence { span, .. } => Some(("Sequence block", *span)),
        Stmt::Stagger { span, .. } => Some(("Stagger block", *span)),
        Stmt::Always { span, .. } => Some(("Always block", *span)),
        Stmt::ForLoop {
            modifiers, span, ..
        } if modifiers.iter().any(|modifier| modifier.name.as_deref() == Some("step")) => {
            Some(("Timed for loop", *span))
        },
        Stmt::Scene { span, .. } => Some(("Scene declaration", *span)),
        Stmt::Config { span, .. } => Some(("Config block", *span)),
        Stmt::Step { span, .. } => Some(("Step marker", *span)),
        _ => None,
    }
}

/// Infer a normal function result from return statements.
/// The parser rewrites a final bare expression to `Return`; an unhandled path is `Unit`.
pub(super) fn infer_function_result(
    body: &[Stmt],
    env: &typing::TypeEnv,
) -> (TypedType, Option<crate::ast::Span>) {
    let flow = infer_body(body, &mut env.clone());
    let mut results = flow.returns;
    if results.is_empty() {
        return (TypedType::Unit, flow.last_span);
    }
    if flow.falls_through {
        results.push(TypedType::Unit);
    }
    results.sort_by_key(|ty| format!("{ty:?}"));
    results.dedup();
    let result = match results.as_slice() {
        [only] => only.clone(),
        many => TypedType::Union(many.to_vec()),
    };
    (result, flow.last_span)
}

fn requires_explicit_return_union(result: &TypedType) -> bool {
    let TypedType::Union(members) = result else {
        return false;
    };

    !members
        .iter()
        .any(|candidate| members.iter().all(|member| typing::is_subtype(member, candidate)))
}

#[derive(Default)]
struct FunctionFlow {
    returns: Vec<TypedType>,
    falls_through: bool,
    last_span: Option<crate::ast::Span>,
}

fn infer_body(body: &[Stmt], env: &mut typing::TypeEnv) -> FunctionFlow {
    let mut flow = FunctionFlow {
        falls_through: true,
        ..Default::default()
    };
    for stmt in body {
        if !flow.falls_through {
            break;
        }
        match stmt {
            Stmt::LetDecl { name, value, .. } => {
                let ty = typing::infer_expr_type(value, env);
                env.bind(name, ty);
            },
            Stmt::Return { value, span } => {
                flow.returns.push(
                    value
                        .as_ref()
                        .map_or(TypedType::Unit, |expr| typing::infer_expr_type(expr, env)),
                );
                flow.last_span = *span;
                flow.falls_through = false;
            },
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                let then_flow = infer_body(then_branch, &mut env.clone());
                let else_flow = else_branch
                    .as_ref()
                    .map(|body| infer_body(body, &mut env.clone()))
                    .unwrap_or_else(|| FunctionFlow {
                        falls_through: true,
                        ..Default::default()
                    });
                flow.returns.extend(then_flow.returns);
                flow.returns.extend(else_flow.returns);
                flow.falls_through = then_flow.falls_through || else_flow.falls_through;
                flow.last_span = then_flow.last_span.or(else_flow.last_span);
            },
            Stmt::Match { arms, .. } => {
                let mut branch_falls = false;
                for (_, body) in arms {
                    let branch = infer_body(body, &mut env.clone());
                    flow.returns.extend(branch.returns);
                    branch_falls |= branch.falls_through;
                    flow.last_span = branch.last_span.or(flow.last_span);
                }
                flow.falls_through = branch_falls;
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                ..
            } => {
                let mut loop_env = env.clone();
                loop_env.push_scope();
                let element = match typing::infer_expr_type(iterable, env) {
                    TypedType::List(inner) => *inner,
                    _ => TypedType::Any,
                };
                match var {
                    LoopPattern::Single(name) => loop_env.bind(name, element),
                    LoopPattern::Tuple(names) => {
                        for name in names {
                            loop_env.bind(name, TypedType::Any);
                        }
                    },
                }
                if let Some(name) = index_var {
                    loop_env.bind(name, TypedType::Num);
                }
                let inner = infer_body(body, &mut loop_env);
                flow.returns.extend(inner.returns);
                flow.falls_through = true;
            },
            Stmt::Block { body, .. } => {
                let inner = infer_body(body, &mut env.clone());
                flow.returns.extend(inner.returns);
                flow.falls_through = inner.falls_through;
                flow.last_span = inner.last_span;
            },
            Stmt::Expr(_expr, span) => {
                flow.last_span = *span;
            },
            _ => {},
        }
    }
    flow
}
