//! View model for the timeline panel.

use std::collections::HashSet;

use crate::app::PreviewPaneState;

/// Immutable view model for the timeline panel.
#[allow(dead_code)] // View model for panel migration (R7); panels still use mutable context.
/// View model for panel migration (R7); panels still use mutable context.
pub struct TimelinePanelModel<'a> {
    pub preview: &'a PreviewPaneState,
    pub timeline: Option<&'a animatix::timeline::Timeline>,
    pub composition: Option<&'a animatix::composition::Composition>,
    pub active_scene: Option<&'a str>,
    pub collapsed_actors: &'a HashSet<String>,
    pub selected_actors: &'a HashSet<String>,
    pub actor_labels: &'a [String],
    pub actor_keyframes: &'a [(String, Vec<(u64, &'static str)>)],
}
