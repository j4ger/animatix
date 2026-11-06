use std::collections::HashMap;

use crate::ast::{Action, Expr, Modifier, Stmt, TargetSegment, array_actor_label};
use crate::module::{FnTemplate, InstanceFnRegistry};

// NOTE: This module takes ownership of Vec<Stmt> and produces a new
// Vec<Stmt> (owned tree transformation with potential 1→N expansion).

// `tests.rs` reaches `expand_stmt_list` through `use super::*`; rustc's
// unused-import analysis does not follow that glob chain, so it looks unused.
#[allow(unused_imports)]
use expand::*;
use params::*;
use substitute::*;

mod cycles;
mod entry;
mod expand;
mod params;
mod substitute;
#[cfg(test)]
mod tests;

pub(super) use entry::expand_fn_calls;
