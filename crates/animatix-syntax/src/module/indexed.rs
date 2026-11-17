//! Indexed component specialization shared by component expansion and method
//! resolution. This pass rewrites source-root property reads to the concrete
//! instance label while respecting lexical shadowing.

use std::collections::HashSet;

use crate::ast::{Expr, LoopPattern, Stmt};
use crate::visit::{AstFolder, walk_fold_expr, walk_fold_stmt};

/// Rewrite property reads rooted at `source_root` to `concrete_root`.
///
/// `bound_names` contains component parameters already resolved in the outer
/// scope. Inner lets, loop variables and function parameters add their own
/// shadowing scopes while the AST folder walks the body.
pub fn specialize_instance_property_reads(
    body: Vec<Stmt>,
    source_root: &str,
    concrete_root: &str,
    bound_names: &[String],
) -> Vec<Stmt> {
    let mut folder = InstancePathSpecializer {
        source_root: source_root.split('.').map(str::to_owned).collect(),
        concrete_root: concrete_root.split('.').map(str::to_owned).collect(),
        scopes: vec![bound_names.iter().cloned().collect()],
    };
    folder.fold_stmts(body)
}

struct InstancePathSpecializer {
    source_root: Vec<String>,
    concrete_root: Vec<String>,
    scopes: Vec<HashSet<String>>,
}

impl InstancePathSpecializer {
    fn root_is_shadowed(&self) -> bool {
        self.source_root
            .first()
            .is_some_and(|root| self.scopes.iter().any(|scope| scope.contains(root)))
    }

    fn fold_scoped_body(&mut self, body: Vec<Stmt>) -> Vec<Stmt> {
        self.scopes.push(HashSet::new());
        let folded = self.fold_stmts(body);
        self.scopes.pop();
        folded
    }

    fn rewrite_member_path(&self, expr: &Expr) -> Option<Expr> {
        if self.root_is_shadowed() {
            return None;
        }
        let mut parts = member_path(expr)?;
        if parts.len() <= self.source_root.len() || !parts.starts_with(&self.source_root) {
            return None;
        }
        let suffix = parts.split_off(self.source_root.len());
        let mut rewritten = self.concrete_root.clone();
        rewritten.extend(suffix);
        Some(member_path_expr(rewritten))
    }

    fn push_loop_names(&mut self, var: &LoopPattern, index_var: Option<&str>) {
        let mut names = HashSet::new();
        match var {
            LoopPattern::Single(name) => {
                names.insert(name.clone());
            },
            LoopPattern::Tuple(names_in_tuple) => names.extend(names_in_tuple.iter().cloned()),
        }
        if let Some(index) = index_var {
            names.insert(index.to_string());
        }
        self.scopes.push(names);
    }
}

fn member_path(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::Ident(name) => Some(split_dotted_parts(std::iter::once(name.as_str()))),
        Expr::Path(parts) => Some(split_dotted_parts(parts.iter().map(String::as_str))),
        Expr::Field(receiver, field) => {
            let mut parts = member_path(receiver)?;
            parts.extend(split_dotted_parts(std::iter::once(field.as_str())));
            Some(parts)
        },
        _ => None,
    }
}

fn split_dotted_parts<'a>(parts: impl Iterator<Item = &'a str>) -> Vec<String> {
    parts.flat_map(|part| part.split('.')).map(str::to_owned).collect()
}

fn member_path_expr(parts: Vec<String>) -> Expr {
    let mut iter = parts.into_iter();
    let Some(first) = iter.next() else {
        return Expr::Null;
    };
    iter.fold(Expr::Ident(first), |receiver, field| Expr::Field(Box::new(receiver), field))
}

impl AstFolder for InstancePathSpecializer {
    fn fold_expr(&mut self, expr: Expr) -> Expr {
        if let Some(rewritten) = self.rewrite_member_path(&expr) {
            rewritten
        } else {
            match expr {
                Expr::Closure(params, body) => {
                    self.scopes.push(params.iter().cloned().collect());
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
                self.push_loop_names(&var, index_var.as_deref());
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
            Stmt::FnDecl {
                is_pub,
                name,
                params,
                return_type,
                body,
                span,
            } => {
                self.scopes.push(params.iter().map(|param| param.name.clone()).collect());
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
            Stmt::Block {
                body,
                instance_scope,
                span,
            } => {
                let instance_scope = instance_scope.map(|mut scope| {
                    scope.index = self.fold_expr(scope.index);
                    scope
                });
                Stmt::Block {
                    body: self.fold_scoped_body(body),
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
                then_branch: self.fold_scoped_body(then_branch),
                else_branch: else_branch.map(|body| self.fold_scoped_body(body)),
                span,
            },
            other => walk_fold_stmt(self, other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotted_rewrite_chunks_are_specialized_as_member_path_segments() {
        let field_path = Expr::Field(Box::new(Expr::Ident("cards.heading".into())), "color".into());
        let flat_path = Expr::Path(vec!["cards.heading".into(), "color".into()]);
        let body = [field_path, flat_path]
            .into_iter()
            .enumerate()
            .map(|(index, value)| Stmt::LetDecl {
                is_pub: false,
                name: format!("read_{index}"),
                value,
                span: None,
            })
            .collect();

        let specialized = specialize_instance_property_reads(body, "cards", "cards__4", &[]);
        let expected = Expr::Field(
            Box::new(Expr::Field(Box::new(Expr::Ident("cards__4".into())), "heading".into())),
            "color".into(),
        );
        for statement in specialized {
            assert!(matches!(statement, Stmt::LetDecl { value, .. } if value == expected));
        }
    }
}
