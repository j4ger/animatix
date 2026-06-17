//! View model for the sidebar panel.
//!
//! Constructed by the shell before each frame, consumed by the panel.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::app::FileTreeEntry;
use crate::app::PreviewPaneState;
use crate::app::panels::SidebarTab;
use animatix::timeline::{SceneDimensions, Timeline};
use animatix_syntax::diagnostics::Diagnostic;

/// Immutable view model for the sidebar panel.
#[allow(dead_code)]
/// View model for panel migration; panels still use mutable context.
pub struct SidebarModel<'a> {
    pub active_scene: Option<&'a str>,
    pub is_composition: bool,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub current_file: &'a Path,
    pub file_tree: &'a [FileTreeEntry],
    pub preview: &'a PreviewPaneState,
    pub timeline: Option<&'a Timeline>,
    pub selected_actors: &'a HashSet<String>,
    pub collapsed_actors: &'a HashSet<String>,
    pub sidebar_tab: SidebarTab,
    pub diagnostics: &'a [Diagnostic],
    pub is_playing: bool,
    pub components: &'a HashMap<String, animatix_syntax::module::ComponentEntry>,
    pub asset_cache: Option<&'a animatix::timeline::assets::AssetCache>,
    pub scene_dimensions: SceneDimensions,
}
