//! Owned output of a rebuild, to be converted into a `DocumentSnapshot`.

use std::collections::HashMap;

use animatix::composition::Composition;
use animatix::timeline::{SceneDimensions, Timeline, TimelineIndex};
use animatix_syntax::ast::Stmt;
use animatix_syntax::diagnostics::Diagnostic;
use animatix_syntax::module::{ComponentEntry, FnTemplate, Namespace};
use animatix_syntax::source_index::SourceIndex;

/// Successful rebuild output, owned for transfer across threads.
pub struct RebuildOutput {
    pub raw_statements: Vec<Stmt>,
    pub expanded_statements: Vec<Stmt>,
    pub namespaces: HashMap<String, Namespace>,
    pub components: HashMap<String, ComponentEntry>,
    pub module_fns: HashMap<String, FnTemplate>,
    pub source_index: SourceIndex,
    pub timeline: Option<Timeline>,
    pub composition: Option<Composition>,
    pub diagnostics: Vec<Diagnostic>,
    pub timeline_index: TimelineIndex,
    pub keyframe_lines: Vec<usize>,
    pub duration_s: f64,
    pub scene_dimensions: SceneDimensions,
}

/// Failed rebuild output with partial data.
pub struct RebuildFailure {
    pub error: String,
    pub diagnostics: Vec<Diagnostic>,
    pub partial_source_index: Option<SourceIndex>,
}
