//! Stack-safe declaration-time reflow. Tasks preserve the recursive pass's
//! depth-first sibling order and its top-down constraints / bottom-up measures.
use super::*;
use crate::timeline::EntityId;
use crate::timeline::taffy_layout::SizeSpec;
use std::collections::HashSet;

enum Task {
    Group(EntityId, [bool; 2]),
    GroupChild(EntityId, [Option<f32>; 2]),
    GroupExit(EntityId, [Option<f32>; 2]),
    Container(EntityId),
    ContainerChild(EntityId),
    ContainerExit(EntityId),
    FinishContainer(EntityId),
}

impl Timeline {
    pub(super) fn reflow_group_children(
        &mut self,
        id: EntityId,
        time_ms: u64,
        diagnostics: &mut Vec<Diagnostic>,
        assigned_by_parent: [bool; 2],
    ) {
        self.run_layout_reflow(Task::Group(id, assigned_by_parent), time_ms, diagnostics);
    }

    pub(super) fn reflow_nested_children(
        &mut self,
        id: EntityId,
        time_ms: u64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        self.run_layout_reflow(Task::Container(id), time_ms, diagnostics);
    }

    fn run_layout_reflow(&mut self, first: Task, time_ms: u64, diagnostics: &mut Vec<Diagnostic>) {
        let mut tasks = vec![first];
        let mut active = HashSet::new();
        while let Some(task) = tasks.pop() {
            match task {
                Task::Group(id, assigned) => {
                    active.insert(id);
                    let geometry = self.world.geometry(id);
                    let measured =
                        geometry.layout_size_get(time_ms).or_else(|| geometry.layout_size_last());
                    let mut axes = [None; 2];
                    if let Some(spec) = geometry.size_spec {
                        for (axis, dimension) in [spec.width, spec.height].into_iter().enumerate() {
                            if let SizeSpec::Fixed(size) = dimension {
                                axes[axis] = Some(super::group::clamp_group_axis(
                                    geometry, time_ms, axis, size,
                                ));
                            } else if assigned[axis]
                                && matches!(dimension, SizeSpec::Fill | SizeSpec::Percent(_))
                            {
                                axes[axis] = measured.map(|half| half[axis] * 2.0);
                            }
                        }
                    }
                    tasks.push(Task::GroupExit(id, axes));
                    for &child in self.world.hierarchy(id).children.iter().rev() {
                        tasks.push(Task::GroupChild(child, axes));
                    }
                },
                Task::GroupChild(child, axes) => {
                    if self.layout_reflow_cycle(child, &active, diagnostics) {
                        continue;
                    }
                    let assigned =
                        self.resolve_group_relative_child(child, axes, time_ms, diagnostics);
                    if self.world.caps(child).is_group_like() {
                        tasks.push(Task::Group(child, assigned));
                    } else if self.world.container_layout.get(child).is_some() {
                        let size = self
                            .world
                            .geometry(child)
                            .layout_size_get(time_ms)
                            .or_else(|| self.world.geometry(child).layout_size_last())
                            .unwrap_or([0.0; 2])
                            .map(|half| half * 2.0);
                        self.prepare_reflow_container(child, size, time_ms, diagnostics);
                        tasks.push(Task::FinishContainer(child));
                        tasks.push(Task::Container(child));
                    }
                },
                Task::GroupExit(id, axes) => {
                    self.publish_group_extent_in_box(id, time_ms, axes);
                    active.remove(&id);
                },
                Task::Container(id) => {
                    active.insert(id);
                    tasks.push(Task::ContainerExit(id));
                    for &child in self.world.hierarchy(id).children.iter().rev() {
                        tasks.push(Task::ContainerChild(child));
                    }
                },
                Task::ContainerChild(child) => {
                    if self.layout_reflow_cycle(child, &active, diagnostics) {
                        continue;
                    }
                    let Some(label) = self.world.label(child).map(str::to_string) else {
                        continue;
                    };
                    if self.world.caps(child).is_group_like() {
                        if self.group_needs_layout_reflow(child) {
                            tasks.push(Task::Group(child, [true; 2]));
                        }
                    } else if self.container_layout(&label).is_some() {
                        let geometry = self.world.geometry(child);
                        let size = geometry
                            .layout_size_last()
                            .or_else(|| geometry.size.as_ref().map(|track| track.last_value()))
                            .unwrap_or([0.0; 2])
                            .map(|half| half * 2.0);
                        self.prepare_reflow_container(child, size, time_ms, diagnostics);
                        tasks.push(Task::FinishContainer(child));
                        tasks.push(Task::Container(child));
                    }
                },
                Task::ContainerExit(id) => {
                    active.remove(&id);
                },
                Task::FinishContainer(id) => {
                    self.remeasure_container_extent(id, time_ms);
                    let label = self.world.label(id).unwrap_or("<unknown>").to_string();
                    self.apply_container_layout(&label, time_ms as f64, diagnostics);
                },
            }
        }
    }

    fn prepare_reflow_container(
        &mut self,
        id: EntityId,
        size: [f32; 2],
        time_ms: u64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let label = self.world.label(id).unwrap_or("<unknown>").to_string();
        self.propagate_text_child_widths(&label, size, time_ms, diagnostics);
        self.apply_container_layout(&label, time_ms as f64, diagnostics);
    }

    fn layout_reflow_cycle(
        &self,
        id: EntityId,
        active: &HashSet<EntityId>,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> bool {
        if !active.contains(&id) {
            return false;
        }
        diagnostics.push(
            Diagnostic::error(
                DiagnosticCode::LayoutSizeFallback,
                DiagnosticPhase::Build,
                "Layout reflow encountered a cycle in the scene hierarchy.",
            )
            .with_subject(self.world.label(id).unwrap_or("<unknown>")),
        );
        true
    }
}
