use std::collections::{HashMap, HashSet};

use crate::ast::Stmt;
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::typing::{self, TypeEnv as TypedEnv};

use super::TypeEnv;
use animatix_core::callable::CallableDescriptor;

#[derive(Default)]
struct FunctionCalls(HashSet<String>);

impl FunctionCalls {
    fn in_body(body: &[Stmt]) -> HashSet<String> {
        let mut calls = Self::default();
        for stmt in body {
            crate::visit::AstVisitor::visit_stmt(&mut calls, stmt);
        }
        calls.0
    }
}

impl<'ast> crate::visit::AstVisitor<'ast> for FunctionCalls {
    fn visit_expr(&mut self, expr: &'ast crate::ast::Expr) {
        if let crate::ast::Expr::Call(callee, _) = expr
            && let Some(name) = callee.callable_name()
        {
            self.0.insert(name);
        }
        crate::visit::walk_expr(self, expr);
    }
}

fn function_is_recursive(name: &str, graph: &HashMap<String, HashSet<String>>) -> bool {
    fn reaches_start(
        current: &str,
        start: &str,
        graph: &HashMap<String, HashSet<String>>,
        visited: &mut HashSet<String>,
    ) -> bool {
        for next in graph.get(current).into_iter().flatten() {
            if next == start {
                return true;
            }
            if graph.contains_key(next)
                && visited.insert(next.clone())
                && reaches_start(next, start, graph, visited)
            {
                return true;
            }
        }
        false
    }

    reaches_start(name, name, graph, &mut HashSet::from([name.to_string()]))
}

impl<'a> TypeEnv<'a> {
    /// Create a new type environment from a component registry and module actions.
    pub fn new(
        components: &'a HashMap<String, crate::module::ComponentEntry>,
        module_fns: &'a HashMap<String, crate::module::FnTemplate>,
    ) -> Self {
        let mut typed = TypedEnv::with_stdlib();
        for (name, entry) in components {
            let signature = crate::symbol_table::component_signature(
                entry.definition.params.iter().map(|param| {
                    (param.name.as_str(), param.param_type.as_ref(), param.default.as_ref())
                }),
                &mut |default| Some(typing::infer_expr_type(default, &typed)),
            );
            typed.register_component(name, signature);
        }
        Self {
            components,
            module_fns,
            namespace_fns: HashMap::new(),
            callables: HashMap::new(),
            function_aliases: HashMap::new(),
            labels: HashMap::new(),
            typed,
            strict_types: false,
            function_depth: 0,
        }
    }

    /// Enable or disable strict type mode.
    pub fn with_strict_types(mut self, strict: bool) -> Self {
        self.strict_types = strict;
        self
    }

    /// Register type aliases exported by aliased module imports.
    pub fn register_module_aliases(
        &mut self,
        namespaces: &HashMap<String, crate::module::Namespace>,
    ) {
        fn register_ns(
            prefix: &str,
            ns: &crate::module::Namespace,
            typed: &mut TypedEnv,
            functions: &mut HashMap<String, crate::module::FnTemplate>,
        ) {
            for (name, annotation) in &ns.type_exports {
                typed.register_alias(&format!("{prefix}::{name}"), annotation);
            }
            for (name, template) in &ns.function_exports {
                functions.insert(format!("{prefix}::{name}"), template.clone());
            }
            for (name, nested) in &ns.namespaces {
                register_ns(&format!("{prefix}::{name}"), nested, typed, functions);
            }
        }
        for (alias, ns) in namespaces {
            register_ns(alias, ns, &mut self.typed, &mut self.namespace_fns);
        }
    }

    /// Register a callable contract from an extension manifest.
    pub fn register_callable(&mut self, descriptor: &CallableDescriptor) {
        self.callables.insert(descriptor.qualified_name(), descriptor.clone());
        self.callables.insert(descriptor.name.clone(), descriptor.clone());
        let function = typing::Type::Function {
            params: descriptor
                .params
                .iter()
                .map(|param| typing::Type::from_callable_type(&param.ty))
                .collect(),
            ret: Box::new(typing::Type::from_callable_type(&descriptor.return_type)),
        };
        self.typed.register_function(&descriptor.qualified_name(), function.clone());
        self.typed.register_function(&descriptor.name, function);
    }

    /// Register function signatures before checking bodies so forward calls
    /// and inferred return values have a stable type.
    fn register_function_signatures(&mut self, stmts: &[Stmt], diagnostics: &mut Vec<Diagnostic>) {
        let mut templates = crate::module::ModuleGraph::collect_module_fns(stmts);
        templates.extend(self.module_fns.clone());
        templates.extend(self.namespace_fns.clone());
        for (component_name, component) in self.components {
            for (method_name, method) in &component.methods {
                templates.insert(format!("{component_name}.{method_name}"), method.clone());
            }
        }

        let call_graph = templates
            .iter()
            .map(|(name, template)| (name.clone(), FunctionCalls::in_body(&template.body)))
            .collect::<HashMap<_, _>>();
        for (name, template) in &templates {
            if template.return_type.is_none() && function_is_recursive(name, &call_graph) {
                diagnostics.push(
                    Diagnostic::error(
                        DiagnosticCode::RecursiveFunctionNeedsAnnotation,
                        DiagnosticPhase::Build,
                        format!("Recursive function '{name}' must declare its return type"),
                    )
                    .with_ast_span(template.span)
                    .with_subject(name.clone()),
                );
            }
        }

        for (name, template) in &templates {
            let return_type = template
                .return_type
                .as_ref()
                .map(|annotation| self.typed.resolve_annotation(annotation))
                .unwrap_or(typing::Type::Any);
            self.typed
                .register_function(name, self.template_signature(&template.params, return_type));
        }

        for _ in 0..templates.len().max(1) {
            let mut changed = false;
            for (name, template) in &templates {
                if template.return_type.is_some() {
                    continue;
                }
                let inferred = self.infer_template_result(template);
                if inferred == typing::Type::Any {
                    continue;
                }
                self.typed
                    .register_function(name, self.template_signature(&template.params, inferred));
                changed = true;
            }
            if !changed {
                break;
            }
        }
    }

    fn template_signature(
        &self,
        params: &[crate::ast::ParamDef],
        return_type: typing::Type,
    ) -> typing::Type {
        let mut env = self.typed.clone();
        env.push_scope();
        let mut param_types = Vec::with_capacity(params.len());
        for param in params {
            let ty = param
                .param_type
                .as_ref()
                .map(|annotation| env.resolve_annotation(annotation))
                .or_else(|| {
                    param.default.as_ref().map(|value| typing::infer_expr_type(value, &env))
                })
                .unwrap_or(typing::Type::Any);
            env.bind(&param.name, ty.clone());
            param_types.push(ty);
        }
        typing::Type::Function {
            params: param_types,
            ret: Box::new(return_type),
        }
    }

    fn infer_template_result(&self, template: &crate::module::FnTemplate) -> typing::Type {
        let mut env = self.typed.clone();
        env.push_scope();
        for param in &template.params {
            let ty = param
                .param_type
                .as_ref()
                .map(|annotation| env.resolve_annotation(annotation))
                .or_else(|| {
                    param.default.as_ref().map(|value| typing::infer_expr_type(value, &env))
                })
                .unwrap_or(typing::Type::Any);
            env.bind(&param.name, ty);
        }
        super::stmt::infer_function_result(&template.body, &env).0
    }

    /// Check all statements in a program, returning any type errors.
    ///
    /// Two passes only: a top-level alias pre-registration (so forward alias
    /// references resolve) and one recursive check. Unknown-alias warnings are
    /// collected separately and emitted first, preserving the diagnostic order
    /// the previous three-loop implementation produced.
    pub fn check_statements(&mut self, stmts: &[Stmt]) -> Vec<Diagnostic> {
        let mut function_diagnostics = Vec::new();
        self.register_function_signatures(stmts, &mut function_diagnostics);
        for stmt in stmts {
            if let Stmt::TypeAlias {
                name, annotation, ..
            } = stmt
            {
                self.typed.register_alias(name, annotation);
            }
        }
        let mut alias_diagnostics = Vec::new();
        let mut diagnostics = function_diagnostics;
        for stmt in stmts {
            if let Stmt::TypeAlias {
                name,
                annotation,
                span,
                is_pub: _,
            } = stmt
                && let Some(alias) = self.typed.unresolved_alias(annotation)
            {
                alias_diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownTypeAlias,
                        DiagnosticPhase::Build,
                        format!("Type alias '{name}' references unknown type alias '{alias}'"),
                    )
                    .with_ast_span(*span)
                    .with_subject(alias),
                );
            }
            self.check_stmt(stmt, &mut diagnostics);
        }
        alias_diagnostics.extend(diagnostics);
        alias_diagnostics
    }
}
