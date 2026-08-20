//! Gradual type checker for Animatix.
//!
//! Validates component instantiation properties and action invocation arguments
//! against parameter type annotations.
//!
//! This is a lightweight, single-pass checker with no inference and no unification.
//! Unannotated parameters (`param_type: None`) accept any value.

use std::collections::HashMap;

use crate::ast::{ComponentDef, Expr, MatchPattern, Modifier, ParamDef, Property, Stmt};
use crate::diagnostics::{Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::typing::{self, Type as TypedType, TypeEnv as TypedEnv};

/// Type-checking environment.
pub struct TypeEnv<'a> {
    /// Component name → component definition.
    components: &'a HashMap<String, crate::module::ComponentEntry>,
    /// Module-scoped actions: action_name → template.
    module_actions: &'a HashMap<String, crate::module::ActionTemplate>,
    /// Actor label → component type name (accumulated during AST walk).
    labels: HashMap<String, String>,
    /// Symbol-aware type environment used for expression inference.
    typed: TypedEnv,
    /// When true, unannotated parameters produce warnings.
    strict_types: bool,
}

impl<'a> TypeEnv<'a> {
    /// Create a new type environment from a component registry and module actions.
    pub fn new(
        components: &'a HashMap<String, crate::module::ComponentEntry>,
        module_actions: &'a HashMap<String, crate::module::ActionTemplate>,
    ) -> Self {
        let mut typed = TypedEnv::with_stdlib();
        for (name, entry) in components {
            let mut signature = typing::ComponentSignature::default();
            for param in &entry.definition.params {
                if let Some(annotation) = &param.param_type {
                    signature
                        .params
                        .insert(param.name.clone(), TypedType::from_annotation(annotation));
                } else if let Some(default) = &param.default {
                    signature
                        .params
                        .insert(param.name.clone(), typing::infer_expr_type(default, &typed));
                }
            }
            typed.register_component(name, signature);
        }
        Self {
            components,
            module_actions,
            labels: HashMap::new(),
            typed,
            strict_types: false,
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
        fn register_ns(prefix: &str, ns: &crate::module::Namespace, typed: &mut TypedEnv) {
            for (name, annotation) in &ns.type_exports {
                typed.register_alias(&format!("{prefix}::{name}"), annotation);
            }
            for (name, nested) in &ns.namespaces {
                register_ns(&format!("{prefix}.{name}"), nested, typed);
            }
        }
        for (alias, ns) in namespaces {
            register_ns(alias, ns, &mut self.typed);
        }
    }

    /// Check all statements in a program, returning any type errors.
    pub fn check_statements(&mut self, stmts: &[Stmt]) -> Vec<Diagnostic> {
        for stmt in stmts {
            if let Stmt::TypeAlias {
                name, annotation, ..
            } = stmt
            {
                self.typed.register_alias(name, annotation);
            }
        }
        let mut diagnostics = Vec::new();
        for stmt in stmts {
            if let Stmt::TypeAlias {
                name,
                annotation,
                span,
                is_pub: _,
            } = stmt
            {
                if let Some(alias) = self.typed.unresolved_alias(annotation) {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::UnknownTypeAlias,
                            DiagnosticPhase::Build,
                            format!("Type alias '{name}' references unknown type alias '{alias}'"),
                        )
                        .with_ast_span(*span)
                        .with_subject(alias),
                    );
                }
            }
        }
        for stmt in stmts {
            self.check_stmt(stmt, &mut diagnostics);
        }
        diagnostics
    }

    fn check_stmt(&mut self, stmt: &Stmt, diagnostics: &mut Vec<Diagnostic>) {
        match stmt {
            Stmt::ActorDecl {
                label,
                array_index,
                ty,
                props,
                children,
                ..
            } => {
                // Track label → type for action invocation validation
                self.labels.insert(label.clone(), ty.clone());
                if array_index.is_some() {
                    self.typed.declare_array(label, TypedType::Actor(ty.clone()));
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
            },
            Stmt::LetDecl { name, value, .. } => {
                let ty = typing::infer_expr_type(value, &self.typed);
                self.typed.bind(name, ty);
            },
            Stmt::Action(action, _span) => {
                for target in &action.targets {
                    // Try component action first
                    let mut checked = false;
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
                                checked = true;
                            }
                        }
                    }
                    // Fall back to module-scoped action
                    if !checked {
                        if let Some(template) = self.module_actions.get(&action.verb) {
                            self.check_action_invocation(
                                &action.verb,
                                target,
                                "module",
                                &template.params,
                                &action.modifiers,
                                diagnostics,
                            );
                        }
                    }
                }
            },
            Stmt::Keyframe { body, .. }
            | Stmt::RelativeKeyframe { body, .. }
            | Stmt::Sequence { body, .. }
            | Stmt::Stagger { body, .. }
            | Stmt::Always { body, .. } => {
                for stmt in body {
                    self.check_stmt(stmt, diagnostics);
                }
            },
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
            },
            Stmt::Match { arms, .. } => {
                // Enforce `_` wildcard arm (required by spec — Amendment 1)
                let has_wildcard =
                    arms.iter().any(|(pat, _)| matches!(pat, MatchPattern::Wildcard));
                if !has_wildcard {
                    diagnostics.push(Diagnostic::error(
                        DiagnosticCode::MissingWildcardArm,
                        DiagnosticPhase::Parse,
                        "match must have a `_` wildcard arm (required for exhaustive matching)",
                    ));
                }
                for (_, body) in arms {
                    for stmt in body {
                        self.check_stmt(stmt, diagnostics);
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
                let iterable_ty = typing::infer_expr_type(iterable, &self.typed);
                let element_ty = match iterable_ty {
                    TypedType::List(inner) => *inner,
                    _ => TypedType::Any,
                };
                self.typed.push_scope();
                match var {
                    crate::ast::LoopPattern::Single(name) => {
                        self.typed.bind(name, element_ty.clone());
                    },
                    crate::ast::LoopPattern::Tuple(names) => {
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
            },
            Stmt::Scene { body, .. } => {
                for stmt in body {
                    self.check_stmt(stmt, diagnostics);
                }
            },
            Stmt::ComponentDef(def, _span) => {
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
                            param
                                .default
                                .as_ref()
                                .map(|value| typing::infer_expr_type(value, &self.typed))
                        })
                        .unwrap_or(TypedType::Any);
                    self.typed.bind(&param.name, ty);
                }
                for stmt in &def.body {
                    self.check_stmt(stmt, diagnostics);
                }
                self.typed.pop_scope();
            },
            Stmt::ComponentAction {
                name, params, body, ..
            } => {
                if self.strict_types {
                    self.check_param_annotations(name, "action", params, diagnostics);
                }
                self.check_param_type_aliases(name, "action", params, diagnostics);
                self.typed.push_scope();
                for param in params {
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
                    self.typed.bind(&param.name, ty);
                }
                for stmt in body {
                    self.check_stmt(stmt, diagnostics);
                }
                self.typed.pop_scope();
            },
            Stmt::Config { .. } => {
                // Config is handled at the program level, not per-statement
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
                    let actual = typing::infer_expr_type(value, &self.typed);
                    let expected = self.typed.resolve_annotation(expected);
                    if !typing::is_subtype(&actual, &expected) {
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

    /// Check that all parameters have type annotations when strict_types is enabled.
    fn check_param_annotations(
        &self,
        owner_name: &str,
        owner_kind: &str,
        params: &[ParamDef],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for param in params {
            if param.param_type.is_none() {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::TypeMismatch,
                        DiagnosticPhase::Build,
                        format!(
                            "Parameter '{}' of {} '{}' is missing a type annotation (required in strict mode)",
                            param.name, owner_kind, owner_name
                        ),
                    )
                    .with_subject(format!("{}.{}.{}", owner_kind, owner_name, param.name)),
                );
            }
        }
    }

    /// Warn when a parameter annotation references an alias that was never
    /// declared. The gradual checker still treats it as `Any`, so this does not
    /// reject the file, but it prevents the alias from silently weakening types.
    fn check_param_type_aliases(
        &self,
        owner_name: &str,
        owner_kind: &str,
        params: &[ParamDef],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        for param in params {
            if let Some(annotation) = &param.param_type {
                if let Some(alias) = self.typed.unresolved_alias(annotation) {
                    diagnostics.push(
                        Diagnostic::warning(
                            DiagnosticCode::UnknownTypeAlias,
                            DiagnosticPhase::Build,
                            format!(
                                "Parameter '{}' of {} '{}' references unknown type alias '{}'",
                                param.name, owner_kind, owner_name, alias
                            ),
                        )
                        .with_subject(format!("{}.{}.{}", owner_kind, owner_name, param.name)),
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
                    let actual = typing::infer_expr_type(&prop.value, &self.typed);
                    let expected = self.typed.resolve_annotation(expected);
                    if !typing::is_subtype(&actual, &expected) {
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

        // Warn on extra properties that don't match any defined parameter.
        for prop in props {
            if !param_map.contains_key(prop.name.as_str()) {
                diagnostics.push(
                    Diagnostic::warning(
                        DiagnosticCode::UnknownComponentProperty,
                        DiagnosticPhase::Build,
                        format!(
                            "Unknown property '{}' for component '{}'; this has no effect and may be a typo",
                            prop.name, component_name
                        ),
                    )
                    .with_subject(format!("{}.{}", component_name, prop.name)),
                );
            }
        }
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
        Expr::List(items) => format!("list of {} items", items.len()),
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

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::ast::{ComponentDef, ParamDef, Property, Stmt, TypeAnnotation};
    use crate::module::{ActionTemplate, ComponentEntry};

    fn make_env() -> TypeEnv<'static> {
        let mut components = HashMap::new();
        components.insert(
            "Button".to_string(),
            ComponentEntry {
                definition: ComponentDef {
                    name: "Button".to_string(),
                    params: vec![
                        ParamDef {
                            name: "size".to_string(),
                            param_type: Some(TypeAnnotation::Vec2),
                            default: Some(Expr::Tuple(vec![Expr::Num(100.0), Expr::Num(40.0)])),
                        },
                        ParamDef {
                            name: "color".to_string(),
                            param_type: Some(TypeAnnotation::Color),
                            default: Some(Expr::Ident("blue".to_string())),
                        },
                    ],
                    body: vec![],
                    is_pub: false,
                },
                source_path: std::path::PathBuf::new(),
                actions: HashMap::new(),
            },
        );
        TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())))
    }

    #[test]
    fn typecheck_accepts_valid_props() {
        let env = make_env();
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "btn".to_string(),
            array_index: None,
            ty: "Button".to_string(),
            props: vec![Property {
                name: "size".to_string(),
                value: Expr::Tuple(vec![Expr::Num(200.0), Expr::Num(60.0)]),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let mut env = env;
        let diagnostics = env.check_statements(&stmts);
        assert!(diagnostics.is_empty(), "Expected no errors, got: {:?}", diagnostics);
    }

    #[test]
    fn typecheck_rejects_mismatched_prop() {
        let env = make_env();
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "btn".to_string(),
            array_index: None,
            ty: "Button".to_string(),
            props: vec![Property {
                name: "size".to_string(),
                value: Expr::Str("too big".to_string()),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let mut env = env;
        let diagnostics = env.check_statements(&stmts);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Type mismatch"));
    }

    #[test]
    fn strict_types_warns_on_missing_annotations() {
        let mut env = make_env();
        env.strict_types = true;
        let stmts = vec![Stmt::ComponentDef(
            ComponentDef {
                name: "Card".to_string(),
                params: vec![ParamDef {
                    name: "title".to_string(),
                    param_type: None,
                    default: Some(Expr::Str("Untitled".to_string())),
                }],
                body: vec![],
                is_pub: false,
            },
            None,
        )];
        let diagnostics = env.check_statements(&stmts);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("missing a type annotation"));
        assert!(diagnostics[0].message.contains("strict mode"));
    }

    #[test]
    fn strict_types_no_warning_when_annotated() {
        let mut env = make_env();
        env.strict_types = true;
        let stmts = vec![Stmt::ComponentDef(
            ComponentDef {
                name: "Card".to_string(),
                params: vec![ParamDef {
                    name: "title".to_string(),
                    param_type: Some(TypeAnnotation::Str),
                    default: Some(Expr::Str("Untitled".to_string())),
                }],
                body: vec![],
                is_pub: false,
            },
            None,
        )];
        let diagnostics = env.check_statements(&stmts);
        assert!(diagnostics.is_empty());
    }

    #[test]
    fn typecheck_validates_action_param_types() {
        use crate::ast::Action;
        let mut components = HashMap::new();
        components.insert(
            "Button".to_string(),
            ComponentEntry {
                definition: ComponentDef {
                    name: "Button".to_string(),
                    params: vec![],
                    body: vec![],
                    is_pub: false,
                },
                source_path: std::path::PathBuf::new(),
                actions: {
                    let mut actions = HashMap::new();
                    actions.insert(
                        "pulse".to_string(),
                        ActionTemplate {
                            params: vec![ParamDef {
                                name: "scale".to_string(),
                                param_type: Some(TypeAnnotation::Num),
                                default: Some(Expr::Num(1.15)),
                            }],
                            body: vec![],
                        },
                    );
                    actions
                },
            },
        );
        let mut env =
            TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));

        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "btn".to_string(),
                array_index: None,
                ty: "Button".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::Action(
                Action {
                    verb: "pulse".to_string(),
                    targets: vec!["btn".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: Some("scale".to_string()),
                        value: Expr::Str("too big".to_string()),
                    }],
                    byte_span: None,
                    target_index: vec![],
                },
                None,
            ),
        ];
        let diagnostics = env.check_statements(&stmts);
        assert_eq!(diagnostics.len(), 1);
        assert!(diagnostics[0].message.contains("Type mismatch"));
        assert!(diagnostics[0].message.contains("scale"));
    }

    #[test]
    fn typecheck_accepts_valid_action_params() {
        use crate::ast::Action;
        let mut components = HashMap::new();
        components.insert(
            "Button".to_string(),
            ComponentEntry {
                definition: ComponentDef {
                    name: "Button".to_string(),
                    params: vec![],
                    body: vec![],
                    is_pub: false,
                },
                source_path: std::path::PathBuf::new(),
                actions: {
                    let mut actions = HashMap::new();
                    actions.insert(
                        "pulse".to_string(),
                        ActionTemplate {
                            params: vec![ParamDef {
                                name: "scale".to_string(),
                                param_type: Some(TypeAnnotation::Num),
                                default: Some(Expr::Num(1.15)),
                            }],
                            body: vec![],
                        },
                    );
                    actions
                },
            },
        );
        let mut env =
            TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));

        let stmts = vec![
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "btn".to_string(),
                array_index: None,
                ty: "Button".to_string(),
                props: vec![],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
            Stmt::Action(
                Action {
                    verb: "pulse".to_string(),
                    targets: vec!["btn".to_string()],
                    args: vec![],
                    modifiers: vec![Modifier {
                        name: Some("scale".to_string()),
                        value: Expr::Num(1.5),
                    }],
                    byte_span: None,
                    target_index: vec![],
                },
                None,
            ),
        ];
        let diagnostics = env.check_statements(&stmts);
        assert!(diagnostics.is_empty(), "Expected no errors, got: {:?}", diagnostics);
    }

    #[test]
    fn type_alias_resolves_in_component_param() {
        let def = ComponentDef {
            name: "Card".to_string(),
            params: vec![ParamDef {
                name: "value".to_string(),
                param_type: Some(TypeAnnotation::Alias("Metric".to_string())),
                default: None,
            }],
            body: vec![],
            is_pub: false,
        };
        let mut components = HashMap::new();
        components.insert(
            "Card".to_string(),
            ComponentEntry {
                definition: def.clone(),
                source_path: std::path::PathBuf::new(),
                actions: HashMap::new(),
            },
        );
        let empty_actions = HashMap::new();
        let mut env = TypeEnv::new(&components, &empty_actions);
        let stmts = vec![
            Stmt::TypeAlias {
                is_pub: true,
                name: "Metric".to_string(),
                annotation: TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str]),
                span: None,
            },
            Stmt::ComponentDef(def.clone(), None),
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "card".to_string(),
                array_index: None,
                ty: "Card".to_string(),
                props: vec![Property {
                    name: "value".to_string(),
                    value: Expr::Str("Revenue".to_string()),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let diagnostics = env.check_statements(&stmts);
        assert!(diagnostics.is_empty(), "Expected no errors, got: {:?}", diagnostics);
    }

    #[test]
    fn example_component_actions_demo_parses_and_typechecks() {
        let source = include_str!("../../../examples/components/09_components.amx");
        let (ast, errors) = crate::parser::parse_source(source);
        assert!(errors.is_empty(), "Parse errors: {:?}", errors);
        let ast = ast.expect("parsed AST");

        // Typecheck directly without module loading (avoids path resolution)
        let mut components = HashMap::new();
        for def in crate::module::discovery::collect_component_defs(&ast) {
            let actions = crate::module::discovery::collect_component_actions(&def);
            components.insert(
                def.name.clone(),
                ComponentEntry {
                    definition: def,
                    source_path: std::path::PathBuf::new(),
                    actions,
                },
            );
        }
        let module_actions = crate::module::ModuleGraph::collect_module_actions(&ast);
        let mut env = TypeEnv::new(&components, &module_actions);
        let type_errors = env.check_statements(&ast);
        assert!(type_errors.is_empty(), "Type errors: {:?}", type_errors);
    }

    #[test]
    fn colorscheme_paths_infer_color_in_strict_mode() {
        use crate::ast::Expr;
        use crate::typing::{Type, TypeEnv, infer_expr_type};
        let env = TypeEnv::with_stdlib();
        // Known colorscheme namespaces with ≥2 segments → Color
        for ns in &["accent", "text", "surface", "stroke"] {
            let path = Expr::Path(vec![ns.to_string(), "primary".to_string()]);
            assert_eq!(infer_expr_type(&path, &env), Type::Color, "{ns}.primary should be Color");
        }
        // scene.* stays Any (mixes colors and anchors)
        let scene = Expr::Path(vec!["scene".to_string(), "background".to_string()]);
        assert_eq!(infer_expr_type(&scene, &env), Type::Any);
        // single-segment stays Any
        let single = Expr::Path(vec!["accent".to_string()]);
        assert_eq!(infer_expr_type(&single, &env), Type::Any);
    }

    #[test]
    fn named_color_list_accepted_for_list_color_param() {
        let mut components = HashMap::new();
        components.insert(
            "Swatches".to_string(),
            ComponentEntry {
                definition: ComponentDef {
                    name: "Swatches".to_string(),
                    params: vec![ParamDef {
                        name: "colors".to_string(),
                        param_type: Some(TypeAnnotation::List(Box::new(TypeAnnotation::Color))),
                        default: None,
                    }],
                    body: vec![],
                    is_pub: false,
                },
                source_path: std::path::PathBuf::new(),
                actions: HashMap::new(),
            },
        );
        let mut env =
            TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "s".to_string(),
            array_index: None,
            ty: "Swatches".to_string(),
            props: vec![Property {
                name: "colors".to_string(),
                value: Expr::List(vec![
                    Expr::Ident("red".to_string()),
                    Expr::Ident("blue".to_string()),
                ]),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let diagnostics = env.check_statements(&stmts);
        assert!(
            diagnostics.is_empty(),
            "named color list should satisfy List<Color>, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn rich_type_annotations_accept_matching_values() {
        let mut components = HashMap::new();
        components.insert(
            "Probe".to_string(),
            ComponentEntry {
                definition: ComponentDef {
                    name: "Probe".to_string(),
                    params: vec![
                        ParamDef {
                            name: "p".to_string(),
                            param_type: Some(TypeAnnotation::Vec3),
                            default: None,
                        },
                        ParamDef {
                            name: "pair".to_string(),
                            param_type: Some(TypeAnnotation::Tuple(vec![
                                TypeAnnotation::Str,
                                TypeAnnotation::Num,
                            ])),
                            default: None,
                        },
                        ParamDef {
                            name: "mapper".to_string(),
                            param_type: Some(TypeAnnotation::Function {
                                params: vec![TypeAnnotation::Num],
                                ret: Box::new(TypeAnnotation::Num),
                            }),
                            default: None,
                        },
                    ],
                    body: vec![],
                    is_pub: false,
                },
                source_path: std::path::PathBuf::new(),
                actions: HashMap::new(),
            },
        );
        let mut env =
            TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "probe".to_string(),
            array_index: None,
            ty: "Probe".to_string(),
            props: vec![
                Property {
                    name: "p".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "pair".to_string(),
                    value: Expr::Tuple(vec![Expr::Str("x".to_string()), Expr::Num(1.0)]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "mapper".to_string(),
                    value: Expr::Closure(vec!["x".to_string()], Box::new(Expr::Num(1.0))),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let diagnostics = env.check_statements(&stmts);
        assert!(
            diagnostics.is_empty(),
            "matching rich annotations should typecheck, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn rich_type_annotations_reject_mismatched_values() {
        let mut components = HashMap::new();
        components.insert(
            "Probe".to_string(),
            ComponentEntry {
                definition: ComponentDef {
                    name: "Probe".to_string(),
                    params: vec![
                        ParamDef {
                            name: "p".to_string(),
                            param_type: Some(TypeAnnotation::Vec3),
                            default: None,
                        },
                        ParamDef {
                            name: "pair".to_string(),
                            param_type: Some(TypeAnnotation::Tuple(vec![
                                TypeAnnotation::Str,
                                TypeAnnotation::Num,
                            ])),
                            default: None,
                        },
                        ParamDef {
                            name: "mapper".to_string(),
                            param_type: Some(TypeAnnotation::Function {
                                params: vec![TypeAnnotation::Num],
                                ret: Box::new(TypeAnnotation::Num),
                            }),
                            default: None,
                        },
                    ],
                    body: vec![],
                    is_pub: false,
                },
                source_path: std::path::PathBuf::new(),
                actions: HashMap::new(),
            },
        );
        let mut env =
            TypeEnv::new(Box::leak(Box::new(components)), Box::leak(Box::new(HashMap::new())));
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "probe".to_string(),
            array_index: None,
            ty: "Probe".to_string(),
            props: vec![
                Property {
                    name: "p".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(1.0), Expr::Num(2.0)]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "pair".to_string(),
                    value: Expr::Tuple(vec![Expr::Num(1.0), Expr::Str("x".to_string())]),
                    value_span: None,
                    trailing_comment: None,
                },
                Property {
                    name: "mapper".to_string(),
                    value: Expr::Closure(
                        vec!["x".to_string()],
                        Box::new(Expr::Str("bad".to_string())),
                    ),
                    value_span: None,
                    trailing_comment: None,
                },
            ],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let diagnostics = env.check_statements(&stmts);
        assert_eq!(diagnostics.len(), 3, "expected three type mismatches, got: {:?}", diagnostics);
        assert!(diagnostics.iter().all(|d| d.code == DiagnosticCode::TypeMismatch));
    }

    #[test]
    fn colorscheme_path_accepted_for_color_param() {
        // accent.primary should be accepted where a Color param is expected
        let env = make_env(); // Button has a 'color: Color' param
        let stmts = vec![Stmt::ActorDecl {
            is_pub: false,
            is_anonymous: false,
            label: "btn".to_string(),
            array_index: None,
            ty: "Button".to_string(),
            props: vec![Property {
                name: "color".to_string(),
                value: Expr::Path(vec!["accent".to_string(), "primary".to_string()]),
                value_span: None,
                trailing_comment: None,
            }],
            modifiers: vec![],
            children: vec![],
            span: None,
        }];
        let mut env = env;
        let diagnostics = env.check_statements(&stmts);
        assert!(
            diagnostics.is_empty(),
            "accent.primary should be accepted for Color param, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn type_alias_resolves_when_declared_after_component() {
        let def = ComponentDef {
            name: "Card".to_string(),
            params: vec![ParamDef {
                name: "value".to_string(),
                param_type: Some(TypeAnnotation::Alias("Metric".to_string())),
                default: None,
            }],
            body: vec![],
            is_pub: false,
        };
        let mut components = HashMap::new();
        components.insert(
            "Card".to_string(),
            ComponentEntry {
                definition: def.clone(),
                source_path: std::path::PathBuf::new(),
                actions: HashMap::new(),
            },
        );
        let empty_actions = HashMap::new();
        let mut env = TypeEnv::new(&components, &empty_actions);
        let stmts = vec![
            Stmt::ComponentDef(def.clone(), None),
            Stmt::TypeAlias {
                is_pub: true,
                name: "Metric".to_string(),
                annotation: TypeAnnotation::Union(vec![TypeAnnotation::Bool, TypeAnnotation::Str]),
                span: None,
            },
            Stmt::ActorDecl {
                is_pub: false,
                is_anonymous: false,
                label: "card".to_string(),
                array_index: None,
                ty: "Card".to_string(),
                props: vec![Property {
                    name: "value".to_string(),
                    value: Expr::Str("Revenue".to_string()),
                    value_span: None,
                    trailing_comment: None,
                }],
                modifiers: vec![],
                children: vec![],
                span: None,
            },
        ];
        let diagnostics = env.check_statements(&stmts);
        assert!(
            diagnostics.is_empty(),
            "alias declared after the component should resolve, got: {:?}",
            diagnostics
        );
    }

    #[test]
    fn unresolved_type_alias_is_reported() {
        let def = ComponentDef {
            name: "Card".to_string(),
            params: vec![ParamDef {
                name: "value".to_string(),
                param_type: Some(TypeAnnotation::Alias("Missing".to_string())),
                default: None,
            }],
            body: vec![],
            is_pub: false,
        };
        let mut components = HashMap::new();
        components.insert(
            "Card".to_string(),
            ComponentEntry {
                definition: def.clone(),
                source_path: std::path::PathBuf::new(),
                actions: HashMap::new(),
            },
        );
        let empty_actions = HashMap::new();
        let mut env = TypeEnv::new(&components, &empty_actions);
        let stmts = vec![Stmt::ComponentDef(def, None)];
        let diagnostics = env.check_statements(&stmts);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::UnknownTypeAlias);
        assert!(diagnostics[0].message.contains("Missing"));
    }

    #[test]
    fn unresolved_top_level_type_alias_is_reported() {
        let empty_components = HashMap::new();
        let empty_actions = HashMap::new();
        let mut env = TypeEnv::new(&empty_components, &empty_actions);
        let stmts = vec![Stmt::TypeAlias {
            is_pub: true,
            name: "Broken".to_string(),
            annotation: TypeAnnotation::Alias("Missing".to_string()),
            span: None,
        }];
        let diagnostics = env.check_statements(&stmts);
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].code, DiagnosticCode::UnknownTypeAlias);
        assert!(diagnostics[0].message.contains("Missing"));
    }
}
