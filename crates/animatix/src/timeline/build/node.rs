//! Scene graph node creation: adds actors to the hierarchy (root vs. child).

use super::*;

impl Timeline {
    pub(crate) fn add_node(&mut self, label: String, parent_label: Option<&str>) {
        if let Some(parent) = parent_label {
            self.root_nodes.retain(|root| root != &label);

            // Add child to parent's children list
            let parent_track = self
                .tracks
                .entry(parent.to_string())
                .or_insert_with(|| AnimationTrack::new(parent.to_string()));
            if !parent_track.children.contains(&label) {
                parent_track.children.push(label.clone());
            }

            // If the child track already exists (re-declaration), update its
            // parent back-reference immediately.  For first declarations the
            // track does not exist yet; actor.rs sets `parent` when it creates
            // the entry.
            if let Some(child_track) = self.tracks.get_mut(&label) {
                child_track.parent = Some(parent.to_string());
            }
        } else {
            let already_nested = self
                .tracks
                .values()
                .any(|track| track.children.contains(&label));

            // No parent → root node, unless the actor already belongs to a container
            if !already_nested && !self.root_nodes.contains(&label) {
                self.root_nodes.push(label.clone());
            }
        }
    }
}