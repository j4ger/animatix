//! Read-only inspection of the last built scene. Source is the only editor.
use crate::app::PreviewPaneState;
use crate::app::commands::ActionQueue;
use animatix::timeline::Timeline;
use animatix_syntax::ast::Stmt;
use std::collections::HashSet;
use std::path::Path;

mod property_groups;
mod property_groups_legend;
mod read_only;
mod source;
#[cfg(test)]
mod tests;
use property_groups::{PropertyEntry, PropertyKind};
pub(crate) use read_only::inspector_panel_ui;

pub(crate) struct InspectorContext<'a> {
    pub preview: &'a PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub selected_actors: &'a mut HashSet<String>,
    pub commands: &'a mut ActionQueue,
    pub scene_dimensions: animatix::timeline::SceneDimensions,
    pub raw_statements: Option<&'a [Stmt]>,
    pub source_path: &'a Path,
    pub source_text: &'a str,
    /// False while editor text is awaiting rebuild, or the last build failed.
    pub source_current: bool,
}
