//! Scene graph node creation: adds actors to the hierarchy (root vs. child).

use super::*;

impl Timeline {
    pub(crate) fn add_node(&mut self, label: String, parent_label: Option<&str>) {
        // `World::set_parent_label` is the single hierarchy writer. It can write
        // the link now when both entities exist, and buffers it until the
        // child's entity is created otherwise (adopted by `World::spawn`).
        self.world.set_parent_label(&label, parent_label);
    }
}
