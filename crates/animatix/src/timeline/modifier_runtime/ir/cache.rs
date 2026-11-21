use std::sync::Arc;

use animatix_syntax::visit::{AstFolder, walk_fold_expr, walk_fold_stmt};

use super::ModifierIrProgram;
use crate::ast::{Expr, Property, Stmt};

/// Previously lowered modifier source and its scene-independent IR. A rebuild
/// can reuse it even when actor identities, inputs or source line numbers change.
pub struct ModifierIrCache {
    source: Vec<Stmt>,
    numbers: Vec<u64>,
    pub(crate) program: Arc<ModifierIrProgram>,
}

impl ModifierIrCache {
    pub(crate) fn lower(
        body: &[Stmt],
        candidates: &[Arc<Self>],
    ) -> Result<Arc<Self>, super::IrLowerError> {
        let mut key = CacheKey::default();
        let source = key.fold_stmts(body.to_vec());
        if let Some(hit) = candidates
            .iter()
            .find(|entry| entry.numbers == key.numbers && entry.source == source)
        {
            return Ok(Arc::clone(hit));
        }
        let _stage = crate::perf::ScopedStage::new(crate::perf::stage::MODIFIER_LOWER);
        Ok(Arc::new(Self {
            program: Arc::new(super::lower_modifier_body(body)?),
            source,
            numbers: key.numbers,
        }))
    }
}

#[derive(Default)]
struct CacheKey {
    // AST equality treats signed zeroes as equal. Preserve numeric bits as
    // well, since values can reach operations that distinguish them.
    numbers: Vec<u64>,
}

impl AstFolder for CacheKey {
    fn fold_stmts(&mut self, stmts: Vec<Stmt>) -> Vec<Stmt> {
        stmts
            .into_iter()
            .filter(|stmt| !matches!(stmt, Stmt::Comment(..)))
            .map(|stmt| self.fold_stmt(stmt))
            .collect()
    }

    fn fold_stmt(&mut self, stmt: Stmt) -> Stmt {
        let mut stmt = walk_fold_stmt(self, stmt);
        match &mut stmt {
            Stmt::Assignment {
                span, value_span, ..
            } => {
                *span = None;
                *value_span = None;
            },
            Stmt::LetDecl { span, .. }
            | Stmt::ForLoop { span, .. }
            | Stmt::Conditional { span, .. }
            | Stmt::Match { span, .. } => *span = None,
            _ => {}, // Preserve other statement nodes as part of the key.
        }
        stmt
    }

    fn fold_expr(&mut self, expr: Expr) -> Expr {
        let mut expr = walk_fold_expr(self, expr);
        match &mut expr {
            Expr::Num(value) | Expr::Percent(value) | Expr::Duration(value) => {
                self.numbers.push(value.to_bits());
            },
            Expr::Call(_, args) => {
                for arg in args {
                    arg.byte_span = None;
                }
            },
            _ => {}, // Child expressions were normalized by the traversal.
        }
        expr
    }

    fn fold_property(&mut self, property: Property) -> Property {
        Property::new(property.name, self.fold_expr(property.value))
    }
}
