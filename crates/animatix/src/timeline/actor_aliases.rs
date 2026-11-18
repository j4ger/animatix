//! Resolve captured actor identities before reactive bodies become frame IR.

use std::collections::HashMap;

use animatix_syntax::AstFolder;
use animatix_syntax::visit::{walk_fold_expr, walk_fold_stmt};

use crate::ast::{Expr, InlineItem, LoopPattern, Stmt};

pub(crate) fn resolve_actor_aliases(
    body: Vec<Stmt>,
    aliases: HashMap<String, String>,
) -> Vec<Stmt> {
    ActorAliases { aliases }.fold_stmts(body)
}

pub(crate) fn resolve_actor_aliases_expr(expr: Expr, aliases: HashMap<String, String>) -> Expr {
    ActorAliases { aliases }.fold_expr(expr)
}

pub(crate) fn resolve_inline_actor_aliases(
    items: Vec<InlineItem>,
    aliases: HashMap<String, String>,
) -> Vec<InlineItem> {
    let mut resolver = ActorAliases { aliases };
    items.into_iter().map(|item| resolver.fold_inline_item(item)).collect()
}

struct ActorAliases {
    aliases: HashMap<String, String>,
}

impl ActorAliases {
    fn resolve(&self, name: String) -> String {
        if let Some(label) = self.aliases.get(&name) {
            return label.clone();
        }
        for (offset, _) in name.rmatch_indices('.') {
            if let Some(label) = self.aliases.get(&name[..offset]) {
                return format!("{label}{}", &name[offset..]);
            }
        }
        name
    }

    fn shadow_loop(&mut self, pattern: &LoopPattern, index: Option<&str>) {
        match pattern {
            LoopPattern::Single(name) => {
                self.aliases.remove(name);
            },
            LoopPattern::Tuple(names) => {
                for name in names {
                    self.aliases.remove(name);
                }
            },
        }
        if let Some(index) = index {
            self.aliases.remove(index);
        }
    }
}

impl AstFolder for ActorAliases {
    fn fold_stmts(&mut self, body: Vec<Stmt>) -> Vec<Stmt> {
        let previous = self.aliases.clone();
        let body = body.into_iter().map(|stmt| self.fold_stmt(stmt)).collect();
        self.aliases = previous;
        body
    }

    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        match stmt {
            Stmt::FnDecl {
                is_pub,
                name,
                params,
                return_type,
                body,
                span,
            } => {
                let previous = self.aliases.clone();
                self.aliases.remove(&name);
                for param in &params {
                    self.aliases.remove(&param.name);
                }
                let body = self.fold_stmts(body);
                self.aliases = previous;
                self.aliases.remove(&name);
                Stmt::FnDecl {
                    is_pub,
                    name,
                    params,
                    return_type,
                    body,
                    span,
                }
            },
            Stmt::LetDecl {
                is_pub,
                name,
                value,
                span,
            } => {
                let value = self.fold_expr(value);
                self.aliases.remove(&name);
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
                modifiers,
                body,
                span,
            } => {
                let iterable = self.fold_expr(iterable);
                let modifiers = modifiers.into_iter().map(|m| self.fold_modifier(m)).collect();
                let previous = self.aliases.clone();
                self.shadow_loop(&var, index_var.as_deref());
                let body = self.fold_stmts(body);
                self.aliases = previous;
                Stmt::ForLoop {
                    var,
                    index_var,
                    iterable,
                    modifiers,
                    body,
                    span,
                }
            },
            stmt => {
                let mut stmt = walk_fold_stmt(self, stmt);
                if let Stmt::Assignment { target, .. } | Stmt::ReactiveBinding { target, .. } =
                    &mut stmt
                    && let Some(first) = target.first_mut()
                {
                    match first {
                        crate::ast::TargetSegment::Static(label)
                        | crate::ast::TargetSegment::Indexed { base: label, .. } => {
                            *label = self.resolve(std::mem::take(label));
                        },
                    }
                }
                stmt
            },
        }
    }

    fn fold_inline_item(&mut self, item: InlineItem) -> InlineItem {
        if let InlineItem::ForLoop {
            var,
            index_var,
            iterable,
            body,
        } = item
        {
            let iterable = self.fold_expr(iterable);
            let previous = self.aliases.clone();
            self.shadow_loop(&var, index_var.as_deref());
            let body = body.into_iter().map(|item| self.fold_inline_item(item)).collect();
            self.aliases = previous;
            InlineItem::ForLoop {
                var,
                index_var,
                iterable,
                body,
            }
        } else {
            animatix_syntax::visit::walk_fold_inline_item(self, item)
        }
    }

    fn fold_expr(&mut self, expr: Expr) -> Expr {
        match expr {
            Expr::Ident(name) => Expr::Ident(self.resolve(name)),
            Expr::Path(parts) => {
                let source = parts.join(".");
                let target = self.resolve(source.clone());
                if target == source {
                    Expr::Path(parts)
                } else {
                    Expr::Path(target.split('.').map(str::to_owned).collect())
                }
            },
            Expr::Closure(params, body) => {
                let previous = self.aliases.clone();
                for param in &params {
                    self.aliases.remove(param);
                }
                let body = self.fold_expr(*body);
                self.aliases = previous;
                Expr::Closure(params, Box::new(body))
            },
            Expr::LetChain(bindings, tail) => {
                let previous = self.aliases.clone();
                let bindings = bindings
                    .into_iter()
                    .map(|(name, value)| {
                        let value = self.fold_expr(value);
                        self.aliases.remove(&name);
                        (name, value)
                    })
                    .collect();
                let tail = self.fold_expr(*tail);
                self.aliases = previous;
                Expr::LetChain(bindings, Box::new(tail))
            },
            expr => walk_fold_expr(self, expr),
        }
    }
}
