//! Statement walking and function-body purity checks.

use super::TypeEnv;
use crate::ast::{ComponentDef, InlineItem, LoopPattern, MatchPattern, ParamDef, Property, Stmt};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::typing::{self, Type as TypedType};

impl<'a> TypeEnv<'a> {
    pub(super) fn check_stmt(&mut self, stmt: &Stmt, diagnostics: &mut Vec<Diagnostic>) {
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
            },
            Stmt::Action(action, _span) => {
                self.check_action_stmt(action, diagnostics);
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
                then_branch,
                else_branch,
                ..
            } => {
                self.check_stmt_body(then_branch, diagnostics);
                if let Some(else_body) = else_branch {
                    self.check_stmt_body(else_body, diagnostics);
                }
            },
            Stmt::Match { arms, .. } => {
                self.check_match(arms, diagnostics);
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                ..
            } => {
                self.check_for_loop(var, index_var, iterable, body, diagnostics);
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
        for stmt in body {
            self.check_stmt(stmt, diagnostics);
        }
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
        // Track label → type for action invocation validation
        self.labels.insert(label.to_string(), ty.to_string());
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
        }
        // Recurse into children
        for child in children {
            self.check_inline_item(child, diagnostics);
        }
    }

    /// Check an action invocation against component-timeline then module templates.
    fn check_action_stmt(
        &mut self,
        action: &crate::ast::Action,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for target in &action.targets {
            // Try component timeline function first
            let mut checked = false;
            if let Some(component_name) = self.labels.get(target)
                && let Some(entry) = self.components.get(component_name)
                && let Some(template) = entry.actions.get(&action.verb)
            {
                self.check_action_invocation(
                    &action.verb,
                    target,
                    component_name,
                    &template.params,
                    &action.modifiers,
                    action.byte_span,
                    diagnostics,
                );
                checked = true;
            }
            // Fall back to module-scoped action
            if !checked && let Some(template) = self.module_fns.get(&action.verb) {
                self.check_action_invocation(
                    &action.verb,
                    target,
                    "module",
                    &template.params,
                    &action.modifiers,
                    action.byte_span,
                    diagnostics,
                );
            }
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
        for stmt in body {
            self.check_stmt(stmt, diagnostics);
        }
        self.typed.pop_scope();
    }

    /// Check a component definition: parameters, then the body in a fresh scope.
    fn check_component_def(&mut self, def: &ComponentDef, diagnostics: &mut Vec<Diagnostic>) {
        if self.strict_types {
            self.check_param_annotations(&def.name, "component", &def.params, diagnostics);
        }
        self.check_param_type_aliases(&def.name, "component", &def.params, diagnostics);
        self.typed.push_scope();
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
    }

    /// Check a function declaration: parameters, purity, then the body.
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
        let pure = return_type.is_some();
        for stmt in body {
            // Purity rules (timeline ops forbidden in pure bodies).
            self.check_fn_body_stmt(name, stmt, pure, diagnostics);
            // Full type-checking with the parameter scope active, so
            // body expressions resolve parameter types.
            self.check_stmt(stmt, diagnostics);
        }
        self.typed.pop_scope();
    }

    /// Check one statement inside a function body against the purity rules.
    ///
    /// Pure functions (`-> Type`) allow computation only: `let`, `if`/`match`,
    /// `for`, and `return`. Timeline functions may emit events but must not
    /// declare scenes, keyframes, or actors.
    fn check_fn_body_stmt(
        &mut self,
        name: &str,
        stmt: &Stmt,
        pure: bool,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let allowed = match stmt {
            Stmt::LetDecl { .. }
            | Stmt::Conditional { .. }
            | Stmt::Match { .. }
            | Stmt::ForLoop { .. }
            | Stmt::Block { .. }
            | Stmt::Expr(..)
            | Stmt::Return { .. } => true,
            Stmt::Action { .. } | Stmt::Assignment { .. } if !pure => true,
            _ => false,
        };
        if !allowed {
            diagnostics.push(crate::diagnostics::Diagnostic::error(
                crate::diagnostics::DiagnosticCode::InvalidPropertyValue,
                crate::diagnostics::DiagnosticPhase::Build,
                format!(
                    "{}function '{}' body contains a statement that is not allowed{}",
                    if pure { "pure " } else { "timeline " },
                    name,
                    if pure {
                        "; pure functions may only compute (let/if/match/for/return)"
                    } else {
                        "; timeline functions cannot declare scenes, keyframes, or actors"
                    }
                ),
            ));
        }
        // Recurse into nested statement bodies so the check is structural.
        match stmt {
            Stmt::Conditional {
                then_branch,
                else_branch,
                ..
            } => {
                for s in then_branch {
                    self.check_fn_body_stmt(name, s, pure, diagnostics);
                }
                if let Some(eb) = else_branch {
                    for s in eb {
                        self.check_fn_body_stmt(name, s, pure, diagnostics);
                    }
                }
            },
            Stmt::Match { arms, .. } => {
                for (_, body) in arms {
                    for s in body {
                        self.check_fn_body_stmt(name, s, pure, diagnostics);
                    }
                }
            },
            Stmt::ForLoop { body, .. } | Stmt::Block { body, .. } => {
                for s in body {
                    self.check_fn_body_stmt(name, s, pure, diagnostics);
                }
            },
            _ => {},
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
            _ => {},
        }
    }
}
