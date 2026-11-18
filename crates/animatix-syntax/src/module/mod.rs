//! Module system for the Animatix language: multi-file programs, imports,
//! component collection, and namespace resolution.

pub mod discovery;
pub(crate) mod expand;
mod indexed;
mod rewrite;
pub mod source_map;
mod types;

pub use indexed::specialize_instance_property_reads;
pub use types::{
    ComponentEntry, ExpandedProgram, ExpansionError, FileId, FnTemplate, FunctionContext, FunctionModule,
    InstanceFnRegistry, LoadedProgram, ModuleGraph, Namespace, SceneData, SourceAccess,
};

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::{fmt, fs};

use discovery::{
    collect_component_defs, collect_component_functions, collect_imports,
    collect_scenes_from_stmts, strip_imports, strip_imports_preserving_keyframe_spans,
};

use crate::ast::{
    ComponentDef, Expr, InlineItem, MatchPattern, ParamDef, Property, Stmt, TargetSegment,
    TypeAnnotation,
};
use crate::parser::{ParseError, parse_source_complete};

mod file_id;
mod graph;
mod helpers;
mod program;
#[cfg(test)]
mod tests;

pub use helpers::ModuleError;
