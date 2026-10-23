use std::collections::{BTreeMap, HashMap};

use super::{Composition, CompositionScene, SceneEdge};
use crate::ast::{Expr, Property, Stmt, Transition};
use crate::diagnostics::{BuildReport, Diagnostic, DiagnosticCode, DiagnosticPhase};
use crate::module::Namespace;
use crate::timeline::Timeline;

mod builder;
mod timeline_build;
