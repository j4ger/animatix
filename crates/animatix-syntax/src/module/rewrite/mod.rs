use super::{
    Action, ComponentDef, Expr, HashMap, HashSet, InlineItem, Modifier, Property, Stmt,
    TargetSegment,
};

/// Recursively check if a statement contains any identifiers that need rewriting.
use apply::*;
use predicates::*;

mod apply;
mod predicates;
mod stmt;
#[cfg(test)]
mod tests;

pub(crate) use stmt::rewrite_stmt;
