use std::collections::HashSet;

use crate::ast::{Action, Expr, InlineItem, LoopPattern, ParamDef, Stmt, TargetSegment};
use crate::module::{FnTemplate, InstanceFnRegistry};
use crate::visit::{AstFolder, walk_fold_expr, walk_fold_inline_item, walk_fold_stmt};

/// Rebind the declaration-time index captured by a component method to the
/// indexed instance at the call site. Lexical bindings in the method take
/// precedence over the captured index.
pub(super) fn rebind_template_index(
    body: Vec<Stmt>,
    root: &str,
    declaration_index: &Expr,
    index: &Expr,
    call_position: Option<usize>,
    call_span: Option<crate::ast::Span>,
    params: &[ParamDef],
) -> Vec<Stmt> {
    let parameters: HashSet<String> = params.iter().map(|param| param.name.clone()).collect();
    let mut folder = TemplateIndexRebinder {
        root,
        root_parts: root.split('.').collect(),
        declaration_index,
        index,
        call_position,
        call_span,
        scopes: vec![parameters],
        allow_expression_rebind: contains_identifier(declaration_index),
        property_read_prefix: None,
    };
    folder.fold_stmts(body)
}

/// Rewrite source-root property reads in an indexed method body to its
/// concrete component instance. Caller prologue bindings seed lexical scope.
pub fn specialize_instance_property_reads(
    body: Vec<Stmt>,
    source_root: &str,
    concrete_root: &str,
    bound_names: &[String],
) -> Vec<Stmt> {
    let no_index = Expr::Null;
    let mut folder = TemplateIndexRebinder {
        root: source_root,
        root_parts: source_root.split('.').collect(),
        declaration_index: &no_index,
        index: &no_index,
        call_position: None,
        call_span: None,
        scopes: vec![bound_names.iter().cloned().collect()],
        allow_expression_rebind: false,
        property_read_prefix: Some((
            source_root.split('.').map(str::to_string).collect(),
            concrete_root.split('.').map(str::to_string).collect(),
        )),
    };
    folder.fold_stmts(body)
}

fn contains_identifier(expr: &Expr) -> bool {
    match expr {
        Expr::Ident(_) | Expr::Path(_) => true,
        Expr::Index(target, index) | Expr::Binary(target, _, index) => {
            contains_identifier(target) || contains_identifier(index)
        },
        Expr::Tuple(items) | Expr::List(items) | Expr::Call(_, items) => {
            items.iter().any(contains_identifier)
        },
        Expr::Unary(_, value) => contains_identifier(value),
        Expr::Method(target, _, args) => {
            contains_identifier(target) || args.iter().any(contains_identifier)
        },
        Expr::Closure(_, body) => contains_identifier(body),
        Expr::LetChain(bindings, tail) => {
            bindings.iter().any(|(_, value)| contains_identifier(value))
                || contains_identifier(tail)
        },
        Expr::Conditional(condition, then_value, else_value) => {
            contains_identifier(condition)
                || contains_identifier(then_value)
                || contains_identifier(else_value)
        },
        Expr::Match(scrutinee, arms) => {
            contains_identifier(scrutinee) || arms.iter().any(|(_, body)| contains_identifier(body))
        },
        Expr::Construct(_, properties) => {
            properties.iter().any(|property| contains_identifier(&property.value))
        },
        Expr::Num(_) | Expr::Percent(_) | Expr::Str(_) | Expr::Bool(_) | Expr::Null => false,
    }
}

/// Look up a component method for an action target. Constant-index targets
/// are lowered by the parser to labels such as `cards__2`; map that concrete
/// label back to the template registry key and carry the index into expansion.
pub(super) fn instance_method_for_target<'a>(
    registry: &'a InstanceFnRegistry,
    target: &str,
    verb: &str,
    target_index: Option<&Expr>,
    target_index_position: Option<usize>,
) -> Option<(String, &'a FnTemplate, Option<Expr>, Option<usize>)> {
    if let Some(template) = registry.get(target).and_then(|methods| methods.get(verb)) {
        return Some((target.to_string(), template, target_index.cloned(), target_index_position));
    }

    let mut segments: Vec<&str> = target.split('.').collect();
    for position in 0..segments.len() {
        let Some((base, suffix)) = segments[position].rsplit_once("__") else {
            continue;
        };
        let Ok(index) = suffix.parse::<usize>() else {
            continue;
        };
        segments[position] = base;
        let template_root = segments.join(".");
        let Some(template) = registry.get(&template_root).and_then(|methods| methods.get(verb))
        else {
            segments[position] = target.split('.').nth(position).unwrap_or_default();
            continue;
        };
        return Some((template_root, template, Some(Expr::Num(index as f64)), Some(position)));
    }
    None
}

struct TemplateIndexRebinder<'a> {
    root: &'a str,
    root_parts: Vec<&'a str>,
    declaration_index: &'a Expr,
    index: &'a Expr,
    call_position: Option<usize>,
    call_span: Option<crate::ast::Span>,
    scopes: Vec<HashSet<String>>,
    allow_expression_rebind: bool,
    property_read_prefix: Option<(Vec<String>, Vec<String>)>,
}

impl TemplateIndexRebinder<'_> {
    fn is_shadowed(&self) -> bool {
        self.scopes
            .iter()
            .any(|scope| scope.iter().any(|name| self.declaration_index.references_ident(name)))
    }

    fn push_loop_bindings(&mut self, var: &LoopPattern, index_var: Option<&str>) {
        let mut names = HashSet::new();
        match var {
            LoopPattern::Single(name) => {
                names.insert(name.clone());
            },
            LoopPattern::Tuple(tuple) => names.extend(tuple.iter().cloned()),
        }
        if let Some(index) = index_var {
            names.insert(index.to_string());
        }
        self.scopes.push(names);
    }

    fn fold_scoped_stmts(&mut self, statements: Vec<Stmt>) -> Vec<Stmt> {
        self.scopes.push(HashSet::new());
        let folded = self.fold_stmts(statements);
        self.scopes.pop();
        folded
    }

    fn is_root_target(&self, target: &str) -> bool {
        target == self.root
            || target.strip_prefix(self.root).is_some_and(|rest| rest.starts_with('.'))
    }

    fn rebind_assignment_target(
        &self,
        mut target: Vec<TargetSegment>,
    ) -> (Vec<TargetSegment>, bool) {
        if self.property_read_prefix.is_some() {
            return (target, false);
        }
        if target.len() < self.root_parts.len() {
            return (target, false);
        }
        let matches_root = target
            .iter()
            .take(self.root_parts.len())
            .zip(&self.root_parts)
            .all(|(segment, part)| segment.label_str() == *part);
        if !matches_root {
            return (target, false);
        }
        let root_position = self.call_position.unwrap_or_else(|| self.root_parts.len() - 1);
        if root_position >= self.root_parts.len() {
            return (target, false);
        }
        target[root_position] = TargetSegment::Indexed {
            base: self.root_parts[root_position].to_string(),
            index: Box::new(self.index.clone()),
        };
        (target, true)
    }

    fn rewrite_property_read(&self, parts: &[String]) -> Option<Expr> {
        let (source, concrete) = self.property_read_prefix.as_ref()?;
        if parts.len() <= source.len()
            || !parts.starts_with(source)
            || self.scopes.iter().any(|scope| scope.contains(&source[0]))
        {
            return None;
        }
        Some(Expr::Path(
            concrete.iter().chain(parts[source.len()..].iter()).cloned().collect(),
        ))
    }
}

impl AstFolder for TemplateIndexRebinder<'_> {
    fn fold_stmts(&mut self, statements: Vec<Stmt>) -> Vec<Stmt> {
        statements.into_iter().map(|stmt| self.fold_stmt(stmt)).collect()
    }

    fn fold_expr(&mut self, expr: Expr) -> Expr {
        if let Expr::Path(parts) = &expr
            && let Some(rewritten) = self.rewrite_property_read(parts)
        {
            rewritten
        } else if self.allow_expression_rebind
            && &expr == self.declaration_index
            && !self.is_shadowed()
        {
            self.index.clone()
        } else {
            match expr {
                Expr::Closure(params, body) => {
                    let scope = params.iter().cloned().collect::<HashSet<_>>();
                    self.scopes.push(scope);
                    let body = self.fold_expr(*body);
                    self.scopes.pop();
                    Expr::Closure(params, Box::new(body))
                },
                Expr::LetChain(bindings, tail) => {
                    self.scopes.push(HashSet::new());
                    let mut folded = Vec::with_capacity(bindings.len());
                    for (name, value) in bindings {
                        let value = self.fold_expr(value);
                        if let Some(scope) = self.scopes.last_mut() {
                            scope.insert(name.clone());
                        }
                        folded.push((name, value));
                    }
                    let tail = self.fold_expr(*tail);
                    self.scopes.pop();
                    Expr::LetChain(folded, Box::new(tail))
                },
                Expr::Match(scrutinee, arms) => {
                    let scrutinee = self.fold_expr(*scrutinee);
                    let arms = arms
                        .into_iter()
                        .map(|(pattern, body)| (pattern, Box::new(self.fold_expr(*body))))
                        .collect();
                    Expr::Match(Box::new(scrutinee), arms)
                },
                other => walk_fold_expr(self, other),
            }
        }
    }

    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        match stmt {
            Stmt::LetDecl {
                is_pub,
                name,
                value,
                span,
            } => {
                let value = self.fold_expr(value);
                if let Some(scope) = self.scopes.last_mut() {
                    scope.insert(name.clone());
                }
                Stmt::LetDecl {
                    is_pub,
                    name,
                    value,
                    span,
                }
            },
            Stmt::ForLoop {
                var,
                index_var,
                iterable,
                body,
                modifiers,
                span,
            } => {
                let iterable = self.fold_expr(iterable);
                let modifiers =
                    modifiers.into_iter().map(|item| self.fold_modifier(item)).collect();
                self.push_loop_bindings(&var, index_var.as_deref());
                let body = self.fold_stmts(body);
                self.scopes.pop();
                Stmt::ForLoop {
                    var,
                    index_var,
                    iterable,
                    body,
                    modifiers,
                    span,
                }
            },
            Stmt::Block {
                body,
                instance_scope,
                span,
            } => {
                let instance_scope = instance_scope.map(|mut scope| {
                    scope.index = self.fold_expr(scope.index);
                    scope
                });
                let body = if self.property_read_prefix.is_some()
                    && let Some(scope) = instance_scope.as_ref()
                {
                    let prologue_len = scope.prologue_len.min(body.len());
                    let (prologue, rest) = body.split_at(prologue_len);
                    let mut folded = self.fold_scoped_stmts(prologue.to_vec());
                    folded.extend_from_slice(rest);
                    folded
                } else {
                    self.fold_scoped_stmts(body)
                };
                Stmt::Block {
                    body,
                    instance_scope,
                    span,
                }
            },
            Stmt::Conditional {
                condition,
                then_branch,
                else_branch,
                span,
            } => Stmt::Conditional {
                condition: self.fold_expr(condition),
                then_branch: self.fold_scoped_stmts(then_branch),
                else_branch: else_branch.map(|body| self.fold_scoped_stmts(body)),
                span,
            },
            Stmt::Match {
                scrutinee,
                arms,
                span,
            } => Stmt::Match {
                scrutinee: self.fold_expr(scrutinee),
                arms: arms
                    .into_iter()
                    .map(|(pattern, body)| (pattern, self.fold_scoped_stmts(body)))
                    .collect(),
                span,
            },
            Stmt::Assignment {
                target,
                property,
                value,
                modifiers,
                easing,
                value_span,
                span,
            } => {
                let (target, rebound) = self.rebind_assignment_target(target);
                let target =
                    target.into_iter().map(|segment| self.fold_target_segment(segment)).collect();
                Stmt::Assignment {
                    target,
                    property,
                    value: self.fold_expr(value),
                    modifiers: modifiers.into_iter().map(|item| self.fold_modifier(item)).collect(),
                    easing,
                    value_span,
                    span: if rebound {
                        self.call_span.or(span)
                    } else {
                        span
                    },
                }
            },
            Stmt::ReactiveBinding {
                target,
                property,
                value,
                value_span,
                span,
            } => {
                let (target, rebound) = self.rebind_assignment_target(target);
                Stmt::ReactiveBinding {
                    target: target
                        .into_iter()
                        .map(|segment| self.fold_target_segment(segment))
                        .collect(),
                    property,
                    value: self.fold_expr(value),
                    value_span,
                    span: if rebound {
                        self.call_span.or(span)
                    } else {
                        span
                    },
                }
            },
            Stmt::Keyframe { time, body, span } => Stmt::Keyframe {
                time,
                body: self.fold_scoped_stmts(body),
                span,
            },
            Stmt::RelativeKeyframe { offset, body, span } => Stmt::RelativeKeyframe {
                offset,
                body: self.fold_scoped_stmts(body),
                span,
            },
            Stmt::Sequence { body, span } => Stmt::Sequence {
                body: self.fold_scoped_stmts(body),
                span,
            },
            Stmt::Stagger {
                modifiers,
                body,
                span,
            } => Stmt::Stagger {
                modifiers: modifiers.into_iter().map(|item| self.fold_modifier(item)).collect(),
                body: self.fold_scoped_stmts(body),
                span,
            },
            Stmt::Always { body, span } => Stmt::Always {
                body: self.fold_scoped_stmts(body),
                span,
            },
            Stmt::FnDecl {
                is_pub,
                name,
                params,
                return_type,
                body,
                span,
            } => {
                let names = params.iter().map(|param| param.name.clone()).collect::<HashSet<_>>();
                self.scopes.push(names);
                let body = self.fold_stmts(body);
                self.scopes.pop();
                Stmt::FnDecl {
                    is_pub,
                    name,
                    params,
                    return_type,
                    body,
                    span,
                }
            },
            other => walk_fold_stmt(self, other),
        }
    }

    fn fold_action(&mut self, mut action: Action) -> Action {
        if action.target_index.len() < action.targets.len() {
            action.target_index.resize(action.targets.len(), None);
        }
        if action.target_index_position.len() < action.targets.len() {
            action.target_index_position.resize(action.targets.len(), None);
        }
        for (position, target) in action.targets.iter().enumerate() {
            let root_position = self.call_position.unwrap_or_else(|| self.root_parts.len() - 1);
            let indexed_position = action.target_index_position[position]
                .unwrap_or_else(|| target.split('.').count().saturating_sub(1));
            let target_has_own_index =
                action.target_index[position].is_some() && indexed_position != root_position;
            if self.property_read_prefix.is_none()
                && self.is_root_target(target)
                && !target_has_own_index
            {
                action.target_index[position] = Some(self.index.clone());
                action.target_index_position[position] = Some(root_position);
            }
        }
        action.args = action.args.into_iter().map(|arg| self.fold_expr(arg)).collect();
        action.modifiers =
            action.modifiers.into_iter().map(|item| self.fold_modifier(item)).collect();
        action.target_index = action
            .target_index
            .into_iter()
            .map(|index| index.map(|expr| self.fold_expr(expr)))
            .collect();
        action
    }

    fn fold_inline_item(&mut self, item: InlineItem) -> InlineItem {
        match item {
            InlineItem::ForLoop {
                var,
                index_var,
                iterable,
                body,
            } => {
                let iterable = self.fold_expr(iterable);
                self.push_loop_bindings(&var, index_var.as_deref());
                let body = body.into_iter().map(|item| self.fold_inline_item(item)).collect();
                self.scopes.pop();
                InlineItem::ForLoop {
                    var,
                    index_var,
                    iterable,
                    body,
                }
            },
            other => walk_fold_inline_item(self, other),
        }
    }
}
