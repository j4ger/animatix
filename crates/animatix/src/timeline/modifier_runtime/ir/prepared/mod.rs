//! Scene-bound optimization of modifier loops and indexed assignments.

mod fold;
mod plan;
mod runtime;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;
use std::sync::Arc;

use super::exec::ModifierGeometry;
use super::fast_program::FastStmt;
use super::{CompiledExpr, ModifierIrProgram, ModifierIrStmt, ModifierOverrides};
use crate::ast::LoopPattern;
use crate::timeline::{EntityId, Value};

pub(crate) struct PreparedModifiers {
    pub(crate) source: Arc<ModifierIrProgram>,
    guards: Vec<(String, Value)>,
    statements: Vec<Statement>,
}

enum Statement {
    Generic(ModifierIrStmt),
    Fast(Box<FastStmt>),
    Branch {
        condition: CompiledExpr,
        yes: Vec<Statement>,
        no: Vec<Statement>,
    },
    Loop {
        pattern: LoopPattern,
        index_var: Option<String>,
        items: CompiledExpr,
        body: LoopBody,
    },
    Indexed {
        base: Arc<IndexPath>,
        property: String,
        value: CompiledExpr,
        index: Index,
    },
}

enum LoopBody {
    Fixed(Vec<Vec<Statement>>),
    Dynamic(Vec<Statement>),
}

enum Index {
    Bound(Arc<Target>),
    Fixed(usize),
    Dynamic(CompiledExpr),
}

struct IndexPath {
    base: String,
    position: usize,
    targets: BTreeMap<usize, Arc<Target>>,
}

struct Target {
    entity: EntityId,
    label: String,
    env_key: String,
}

pub(crate) use plan::prepare;
