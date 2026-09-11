//! Scene graph node creation: adds actors to the hierarchy (root vs. child).

use super::*;

impl Timeline {
    pub(crate) fn add_node(&mut self, label: String, parent_label: Option<&str>) {
        if let Some(parent) = parent_label {
            self.root_nodes.retain(|root| root != &label);

            // Add child to parent's children list. The parent's own declaration
            // creates its track before processing children (the generic path's
            // early track and `process_plot_actor_dispatch`), so a missing
            // parent here is a bug — never default in a track with no identity.
            let Some(parent_track) = self.tracks.get_mut(parent) else {
                tracing::warn!(
                    "child '{label}' registered before parent '{parent}' has a track; \
                     skipping hierarchy link"
                );
                return;
            };
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
            let already_nested = self.tracks.values().any(|track| track.children.contains(&label));

            // No parent → root node, unless the actor already belongs to a container
            if !already_nested && !self.root_nodes.contains(&label) {
                self.root_nodes.push(label.clone());
            }
        }
    }
}
