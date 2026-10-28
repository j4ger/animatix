use std::collections::BTreeMap;

use super::{Composition, CompositionScene, SceneEdge};
use crate::ast::{Expr, Property, Stmt, Transition};
use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::module::Namespace;
use crate::timeline::{BuildOptions, Timeline};

mod builder;
mod edges;
mod timing;
