//! Gradual type checker for Animatix.
//!
//! Validates component instantiation properties and action invocation arguments
//! against parameter type annotations.
//!
//! This is a lightweight, single-pass checker with no inference and no unification.
//! Unannotated parameters (`param_type: None`) accept any value.

use crate::ast::{ComponentDef, Expr, Modifier, ParamDef, Property, Stmt, TypeAnnotation};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use std::collections::HashMap;

/// Type-checking environment.
pub struct TypeEnv<'a> {
    /// Component name → component definition.
    components: &'a HashMap<String, crate::module::ComponentEntry>,
    /// Actor label → component type name (accumulated during AST walk).
    labels: HashMap<String, String>,
}

impl<'a> TypeEnv<'a> {
    /// Create a new type environment from a component registry.
    pub fn new(components: &'a HashMap<String, crate::module::ComponentEntry>) -> Self {
        Self {
            components,
            labels: HashMap::new(),
        }
    }

    /// Check all statements in a program, returning any type errors.
    pub fn check_statements(&mut self, stmts: &[Stmt]) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();
        for stmt in stmts {
            self.check_stmt(stmt, &mut diagnostics);
        }
        diagnostics
    }

    fn check_stmt(&mut self, stmt: &Stmt, diagnostics: &mut Vec<Diagnostic>) {
        match stmt {
            Stmt::ActorDecl { label, ty, props, children, .. } => {
                // Track label → type for action invocation validation
                self.labels.insert(label.clone(), ty.clone());
                // Check if this actor decl instantiates a component
                if let Some(entry) = self.components.get(ty) {
                    self.check_component_props(ty, &entry.definition, props, diagnostics);
                }
                // Recurse into children
                for child in children {
                    self.check_inline_item(child, diagnostics);
                }
            }
            Stmt::Action(action, _span) => {
                for target in &action.targets {
                    if let Some(component_name) = self.labels.get(target) {
                        if let Some(entry) = self.components.get(component_name) {
                            if let Some(template) = entry.actions.get(&action.verb) {
                                self.check_action_invocation(
                                    &action.verb,
                                    target,
                                    component_name,
                                    &template.params,
                                    &action.modifiers,
                                    diagnostics,
                                );
                            }
                        }
                    }
                }
            }
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. }
            | Stmt::Drive { body, .. } => {
                for stmt in body {
                    self.check_stmt(stmt, diagnostics);
                }
            }
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                for stmt in then_branch {
                    self.check_stmt(stmt, diagnostics);
                }
                if let Some(else_body) = else_branch {
                    for stmt in else_body {
                        self.check_stmt(stmt, diagnostics);
                    }
                }
            }
            Stmt::ForLoop { body, .. } => {
                for stmt in body {
                    self.check_stmt(stmt, diagnostics);
                }
            }
            Stmt::Scene { body, .. } => {
                for stmt in body {
                    self.check_stmt(stmt, diagnostics);
                }
            }
            _ => {}
        }
    }

    fn check_inline_item(
        &self,
        item: &crate::ast::InlineItem,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        match item {
            crate::ast::InlineItem::Anonymous { ty, children, .. }
            | crate::ast::InlineItem::Labeled { ty, children, .. } => {
                if let Some(entry) = self.components.get(ty) {
                    // Inline items don't carry properties in the same way;
                    // they have props field but we need to extract them.
                    let props = match item {
                        crate::ast::InlineItem::Anonymous { props, .. } => props,
                        crate::ast::InlineItem::Labeled { props, .. } => props,
                        _ => unreachable!(),
                    };
                    self.check_component_props(ty, &entry.definition, props, diagnostics);
                }
                for child in children {
                    self.check_inline_item(child, diagnostics);
                }
            }
            crate::ast::InlineItem::SlotFill { items, .. } => {
                for item in items {
                    self.check_inline_item(item, diagnostics);
                }
            }
            _ => {}
        }
    }

    fn check_action_invocation(
        &self,
        action_name: &str,
        target: &str,
        component_name: &str,
        params: &[ParamDef],
        modifiers: &[Modifier],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Build a set of provided param names from named modifiers
        let provided: HashMap<&str, &Expr> = modifiers
            .iter()
            .filter_map(|m| m.name.as_ref().map(|name| (name.as_str(), &m.value)))
            .collect();

        for param in params {
            if let Some(expected) = &param.param_type {
                if let Some(value) = provided.get(param.name.as_str()) {
                    let actual = expr_type(value);
                    if !is_subtype(&actual, expected) {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::TypeMismatch,
                                DiagnosticPhase::Build,
                                format!(
                                    "Type mismatch: parameter '{}' of action '{}.{}' expects {}, got {} (from {})",
                                    param.name,
                                    component_name,
                                    action_name,
                                    expected,
                                    actual,
                                    expr_summary(value)
                                ),
                            )
                            .with_subject(format!("{}.{}.{}", component_name, action_name, param.name)),
                        );
                    }
                } else if param.default.is_none() {
                    diagnostics.push(
                        Diagnostic::error(
                            DiagnosticCode::TypeMismatch,
                            DiagnosticPhase::Build,
                            format!(
                                "Missing required parameter '{}' for action '{}.{}' on '{}'",
                                param.name, component_name, action_name, target
                            ),
                        )
                        .with_subject(format!("{}.{}.{}", component_name, action_name, param.name)),
                    );
                }
            }
        }
    }

    fn check_component_props(
        &self,
        component_name: &str,
        def: &ComponentDef,
        props: &[Property],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Build a map of param name → ParamDef for quick lookup
        let param_map: HashMap<&str, &ParamDef> =
            def.params.iter().map(|p| (p.name.as_str(), p)).collect();

        for prop in props {
            if let Some(param) = param_map.get(prop.name.as_str()) {
                if let Some(expected) = &param.param_type {
                    let actual = expr_type(&prop.value);
                    if !is_subtype(&actual, expected) {
                        diagnostics.push(
                            Diagnostic::error(
                                DiagnosticCode::TypeMismatch,
                                DiagnosticPhase::Build,
                                format!(
                                    "Type mismatch: parameter '{}' of component '{}' expects {}, got {} (from {})",
                                    prop.name,
                                    component_name,
                                    expected,
                                    actual,
                                    expr_summary(&prop.value)
                                ),
                            )
                            .with_subject(format!("{}.{}", component_name, prop.name)),
                        );
                    }
                }
            }
        }
    }
}

/// Determine the syntactic type of an expression.
fn expr_type(expr: &Expr) -> TypeAnnotation {
    match expr {
        Expr::Num(_) => TypeAnnotation::Num,
        Expr::Percent(_) => TypeAnnotation::Num,
        Expr::Str(_) => TypeAnnotation::Str,
        Expr::Bool(_) => TypeAnnotation::Bool,
        Expr::Null => TypeAnnotation::Any,
        Expr::Tuple(items) => match items.len() {
            2 if items.iter().all(|e| matches!(e, Expr::Num(_) | Expr::Percent(_))) => {
                TypeAnnotation::Vec2
            }
            4 if items.iter().all(|e| matches!(e, Expr::Num(_) | Expr::Percent(_))) => {
                TypeAnnotation::Vec4
            }
            _ => TypeAnnotation::Any,
        },
        Expr::Ident(_) => {
            // Actor label references resolve to Actor type at build time
            // (we can't know for sure without symbol table, so we guess)
            TypeAnnotation::Any
        }
        Expr::Path(parts) if parts.len() == 1 => {
            TypeAnnotation::Any
        }
        Expr::Construct(name, _) => {
            // Construct expressions like Colorscheme "name" { ... }
            // or Point { x: 10, y: 20 }
            // We map known constructor names to types
            match name.as_str() {
                "Color" | "Colorscheme" => TypeAnnotation::Color,
                _ => TypeAnnotation::Any,
            }
        }
        // Binary operations on numeric types produce Num
        Expr::Binary(left, _, right) => {
            let left_ty = expr_type(left);
            let right_ty = expr_type(right);
            if left_ty == TypeAnnotation::Num && right_ty == TypeAnnotation::Num {
                TypeAnnotation::Num
            } else {
                TypeAnnotation::Any
            }
        }
        Expr::Unary(_, inner) => expr_type(inner),
        Expr::Call(name, _) => {
            // Known functions
            match name.as_str() {
                "rgb" | "rgba" | "hsv" | "hsl" => TypeAnnotation::Color,
                _ => TypeAnnotation::Any,
            }
        }
        _ => TypeAnnotation::Any,
    }
}

/// Check if `actual` is a subtype of `expected`.
fn is_subtype(actual: &TypeAnnotation, expected: &TypeAnnotation) -> bool {
    match (actual, expected) {
        (_, TypeAnnotation::Any) => true,
        (a, b) if a == b => true,
        // Color is a subtype of Vec4 (same runtime representation)
        (TypeAnnotation::Color, TypeAnnotation::Vec4) => true,
        // Numeric literal is subtype of Num
        (TypeAnnotation::Num, TypeAnnotation::Num) => true,
        // List subtyping: List<A> <: List<B> if A <: B
        (TypeAnnotation::List(a), TypeAnnotation::List(b)) => is_subtype(a, b),
        _ => false,
    }
}

/// Short summary of an expression for error messages.
fn expr_summary(expr: &Expr) -> String {
    match expr {
        Expr::Num(n) => format!("number {}", n),
        Expr::Percent(p) => format!("percent {}%", p),
        Expr::Str(s) => format!("string {:?}", s),
        Expr::Bool(b) => format!("boolean {}", b),
        Expr::Null => "null".to_string(),
        Expr::Tuple(items) => format!("tuple of {} items", items.len()),
        Expr::Ident(name) => format!("identifier '{}'", name),
        Expr::Path(parts) if parts.len() == 1 => format!("identifier '{}'", parts[0]),
        Expr::Construct(name, _) => format!("construct '{}'", name),
        Expr::Binary(_, op, _) => format!("binary {:?} expression", op),
        Expr::Unary(op, _) => format!("unary {:?} expression", op),
        Expr::Call(name, args) => format!("call '{}({})'", name, args.len()),
        _ => "expression".to_string(),
    }
}
