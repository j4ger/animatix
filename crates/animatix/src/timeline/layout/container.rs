//! Container layout application: admission, specs, and the Taffy pass.

use super::*;

impl Timeline {
    /// Reflow nested containers after their parent has resolved final child
    /// widths. Build order is bottom-up, but wrapped text needs constraints
    /// from the final parent cell; this walk applies those widths top-down and
    /// publishes the resulting intrinsic heights bottom-up.
    pub(crate) fn reflow_nested_layouts(
        &mut self,
        container_label: &str,
        time_ms: u64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // One mutation transaction covers the recursive geometry/text writes,
        // including callers that reflow an already-evaluated timeline.
        self.invalidate_frame_cache();
        let Some(container_id) = self.world.get_id(container_label) else {
            return;
        };
        self.reflow_nested_children(container_id, time_ms, diagnostics, 0);
        self.remeasure_container_extent(container_id, time_ms);
        self.apply_container_layout(container_label, time_ms as f64, diagnostics);
    }

    pub(super) fn remeasure_container_extent(
        &mut self,
        container_id: crate::timeline::EntityId,
        time_ms: u64,
    ) {
        let Some(label) = self.world.label(container_id).map(str::to_string) else {
            return;
        };
        let Some(metadata) = self.container_layout(&label).cloned() else {
            return;
        };
        let children = self.layout_children_for(container_id);
        let extents: Vec<super::ChildExtent> = children
            .iter()
            .filter_map(|child| {
                self.world.geometry(child.id).layout_size_last().map(|half_size| {
                    super::ChildExtent {
                        half_size,
                        placement_mode: child.placement_mode,
                    }
                })
            })
            .collect();
        let (specs, constraints, parent_content_size) =
            self.collect_child_layout_specs(container_id, &children, time_ms);
        let mut measured_size = super::compute_container_size_with_specs(
            &extents,
            &metadata,
            &specs,
            &constraints,
            parent_content_size,
        );
        let geometry = self.world.geometry(container_id);
        if let (Some(spec), Some(current_half)) = (geometry.size_spec, geometry.layout_size_last())
        {
            if matches!(
                spec.width,
                crate::timeline::taffy_layout::SizeSpec::Fill
                    | crate::timeline::taffy_layout::SizeSpec::Percent(_)
            ) {
                measured_size[0] = current_half[0] * 2.0;
            }
            if matches!(
                spec.height,
                crate::timeline::taffy_layout::SizeSpec::Fill
                    | crate::timeline::taffy_layout::SizeSpec::Percent(_)
            ) {
                measured_size[1] = current_half[1] * 2.0;
            }
        }
        self.publish_measured_container_extent(&label, measured_size, time_ms);
    }

    pub(super) fn reflow_nested_children(
        &mut self,
        parent_id: crate::timeline::EntityId,
        time_ms: u64,
        diagnostics: &mut Vec<Diagnostic>,
        depth: usize,
    ) {
        // The scene hierarchy is acyclic. This hard bound protects builds from
        // pathological nesting and makes the sizing policy finite: each node
        // gets one final-width text measure and one bottom-up size publication.
        if depth >= MAX_NESTED_LAYOUT_DEPTH {
            let label = self.world.label(parent_id).unwrap_or("<unknown>");
            tracing::warn!(
                "nested layout reflow stopped at depth {MAX_NESTED_LAYOUT_DEPTH} for '{label}'"
            );
            diagnostics.push(
                Diagnostic::warning(
                    DiagnosticCode::LayoutSizeFallback,
                    DiagnosticPhase::Build,
                    format!(
                        "Nested layout reflow stopped at depth {MAX_NESTED_LAYOUT_DEPTH} for '{label}'; descendants keep their pre-parent measurements."
                    ),
                )
                .with_subject(label),
            );
            return;
        }

        let child_ids = self.world.hierarchy(parent_id).children.clone();
        for child_id in child_ids {
            let Some(child_label) = self.world.label(child_id).map(str::to_string) else {
                continue;
            };
            if self.world.caps(child_id).is_group_like() {
                if self.group_needs_layout_reflow(child_id) {
                    self.reflow_group_children(
                        child_id,
                        time_ms,
                        diagnostics,
                        depth + 1,
                        [true; 2],
                    );
                }
                continue;
            }
            if self.container_layout(&child_label).is_none() {
                continue;
            }

            // Parent layout has already written the child's resolved slot size
            // when its size spec is auto/fill/percent. Fixed dimensions remain
            // authored and are retained by the same geometry track.
            let child_size = {
                let geometry = self.world.geometry(child_id);
                geometry
                    .layout_size_last()
                    .or_else(|| geometry.size.as_ref().map(|track| track.last_value()))
                    .unwrap_or([0.0, 0.0])
                    .map(|half| half * 2.0)
            };

            self.propagate_text_child_widths(&child_label, child_size, time_ms, diagnostics);
            self.apply_container_layout(&child_label, time_ms as f64, diagnostics);
            self.reflow_nested_children(child_id, time_ms, diagnostics, depth + 1);

            // Descendants may have acquired wrapped heights. Recompute this
            // container from those final child extents before returning to its
            // parent, then place its children once against the final sizes.
            self.remeasure_container_extent(child_id, time_ms);
            self.apply_container_layout(&child_label, time_ms as f64, diagnostics);
        }
    }

    fn push_layout_size_exclusion_diagnostic(
        diagnostics: &mut Vec<Diagnostic>,
        container_label: &str,
        container_ty: &str,
        child_label: &str,
    ) {
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::LayoutSizeFallback,
                DiagnosticPhase::Build,
                format!(
                    "Layout-managed child '{child_label}' in {container_ty} container '{container_label}' had no seeded layout_size and was excluded from layout admission."
                ),
            )
            .with_subject(child_label),
        );
    }

    /// Warn when a container has `Percent`/`Fill` children but no declared
    /// `size`, so the percentage has no parent box to resolve against.
    fn push_percent_without_container_size_diagnostic(
        diagnostics: &mut Vec<Diagnostic>,
        container_ty: &str,
        container_label: &str,
    ) {
        diagnostics.push(
            Diagnostic::warning(
                DiagnosticCode::LayoutSizeFallback,
                DiagnosticPhase::Build,
                format!(
                    "{container_ty} container '{container_label}' sizes children by percentage but declares no `size`; percentages resolve against the content-driven box, which is the children themselves. Declare `size: (w, h)` on the container."
                ),
            )
            .with_subject(container_label),
        );
    }

    /// Publish a container's measured extent into its `layout_size` track.
    ///
    /// Nested layout containers must report the extent their content actually
    /// covers, not the default 100×100 box: when an unsized `Col` sits inside a
    /// `Stack`, or a `Row` inside a `Col`, the parent has to reserve the real
    /// content box or the child's rows spill into its neighbours. The same
    /// write implements the documented `size: auto` / `size: fit` shrink-wrap
    /// for Row/Col/Grid, and a Stack's union extent (it overlaps its layers, so
    /// its box is the largest layer per axis) is just the special case where
    /// `compute_container_size` returns a union instead of a flow size.
    ///
    /// An explicit fixed `size` stays authoritative: the author declared a box.
    pub(crate) fn publish_measured_container_extent(
        &mut self,
        container_label: &str,
        size: [f32; 2],
        time_ms: u64,
    ) {
        if size[0] <= 0.0 && size[1] <= 0.0 {
            return;
        }
        let Some(container_id) = self.world.get_id(container_label) else {
            return;
        };
        let size_spec = self.world.geometry(container_id).size_spec;
        if size_spec.is_some_and(|spec| spec.is_fixed()) {
            return;
        }
        let half = size_spec.map_or([size[0] / 2.0, size[1] / 2.0], |spec| {
            [
                match spec.width {
                    crate::timeline::taffy_layout::SizeSpec::Fixed(width) => width / 2.0,
                    _ => size[0] / 2.0,
                },
                match spec.height {
                    crate::timeline::taffy_layout::SizeSpec::Fixed(height) => height / 2.0,
                    _ => size[1] / 2.0,
                },
            ]
        });
        self.world
            .geometry
            .get_mut(container_id)
            .ensure_layout_size(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
            .add_keyframe(time_ms, half, Easing::Linear);
    }

    /// Publish the union of a `Group`'s children as the group's own extent.
    ///
    /// `Group` is a structural wrapper: it lays nothing out and paints nothing,
    /// so auto axes use the placed extent of its children. Definite authored
    /// or parent-assigned axes retain their resolved box. A
    /// group nested in a Row/Col otherwise publishes the default 100×100 box,
    /// which inflates the main axis and reports a cross-extent unrelated to the
    /// content it wraps (`*_wrap: Group` around a single text line).
    pub(crate) fn publish_group_measured_extent(&mut self, label: &str, time_ms: u64) {
        let Some(group_id) = self.world.get_id(label) else {
            return;
        };
        self.publish_group_extent_in_box(group_id, time_ms, [None; 2]);
    }

    pub(super) fn publish_group_extent_in_box(
        &mut self,
        group_id: crate::timeline::EntityId,
        time_ms: u64,
        assigned_box: [Option<f32>; 2],
    ) {
        let children: Vec<crate::timeline::EntityId> =
            self.world.hierarchy(group_id).children.clone();
        let mut half = [0.0f32, 0.0f32];
        for child in children {
            let geometry = self.world.geometry(child);
            let Some(child_half) = geometry.layout_size_last() else {
                continue;
            };
            let position = geometry.position.last([0.0, 0.0]);
            half[0] = half[0].max(position[0].abs() + child_half[0]);
            half[1] = half[1].max(position[1].abs() + child_half[1]);
        }
        if let Some(spec) = self.world.geometry(group_id).size_spec {
            for (axis, dimension) in [spec.width, spec.height].into_iter().enumerate() {
                match dimension {
                    super::super::taffy_layout::SizeSpec::Fixed(size) => half[axis] = size / 2.0,
                    super::super::taffy_layout::SizeSpec::Fill
                    | super::super::taffy_layout::SizeSpec::Percent(_) => {
                        if let Some(size) = assigned_box[axis] {
                            half[axis] = size / 2.0;
                        }
                    },
                    _ => {}, // Auto/Fit axes publish the placed content union.
                }
            }
        }
        for (axis, value) in half.iter_mut().enumerate() {
            *value = super::group::clamp_group_axis(
                self.world.geometry(group_id),
                time_ms,
                axis,
                *value * 2.0,
            ) / 2.0;
        }
        self.world
            .geometry
            .get_mut(group_id)
            .ensure_layout_size(crate::timeline::DEFAULT_LAYOUT_HALF_SIZE)
            .add_keyframe(time_ms, half, Easing::Linear);
    }

    pub(crate) fn build_layout_children(
        &self,
        container_label: &str,
        container_ty: &str,
        child_order: &[String],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<ContainerLayoutChild> {
        let mut layout_children = Vec::with_capacity(child_order.len());

        for child_label in child_order {
            let Some(child_id) = self.world.get_id(child_label) else {
                warn!("layout: child '{}' not found in tracks, skipping", child_label);
                continue;
            };

            let placement_mode =
                self.world.geometry(child_id).placement_mode.last(PlacementMode::LayoutManaged);

            // Only children with seeded layout_size are admitted into the layout set.
            // Manual children may still exist in the scene graph via the hierarchy,
            // but they are excluded from layout spacing/placement when unmeasured.
            let is_admitted = if !self.world.geometry(child_id).has_layout_size() {
                if placement_mode == PlacementMode::LayoutManaged {
                    Self::push_layout_size_exclusion_diagnostic(
                        diagnostics,
                        container_label,
                        container_ty,
                        child_label,
                    );
                }
                false
            } else {
                true
            };

            if is_admitted {
                layout_children.push(ContainerLayoutChild {
                    id: child_id,
                    placement_mode,
                });
            }
        }

        layout_children
    }

    /// Collect size specs and constraints for a container's children from tracks.
    pub(crate) fn collect_child_layout_specs(
        &self,
        container_id: crate::timeline::EntityId,
        children: &[ContainerLayoutChild],
        time_ms: u64,
    ) -> (Vec<Option<ChildSizeSpec>>, Vec<SizeConstraints>, [f32; 2]) {
        let mut size_specs: Vec<Option<ChildSizeSpec>> = Vec::with_capacity(children.len());
        let mut constraints: Vec<SizeConstraints> = Vec::with_capacity(children.len());

        // The parent content box drives `Percent`/`Fill` child sizing. During
        // nested reflow, the parent may have just assigned a narrower Grid
        // cell than the container's earlier build-time `size` track. The
        // resolved layout size is the final slot in that case; use authored
        // size only when no resolved extent exists yet.
        let parent_content_size = {
            let geometry = self.world.geometry(container_id);
            geometry
                .layout_size_get(time_ms)
                .or_else(|| geometry.layout_size_last())
                .or_else(|| geometry.size.as_ref().map(|t| t.evaluate(time_ms)))
                .or_else(|| geometry.size.as_ref().map(|t| t.last_value()))
                .map(|s| [s[0] * 2.0, s[1] * 2.0])
                .unwrap_or([0.0, 0.0])
        };

        for cl in children {
            let geometry = self.world.geometry(cl.id);
            size_specs.push(geometry.size_spec.map(|spec| {
                if self.dynamic_layout {
                    spec.measured_dimensions()
                } else {
                    spec
                }
            }));
            let min_w = geometry.min_width.get(time_ms, 0.0);
            let max_w = geometry.max_item_width.get(time_ms, f32::INFINITY);
            let min_h = geometry.min_height.get(time_ms, 0.0);
            let max_h = geometry.max_height.get(time_ms, f32::INFINITY);
            constraints.push(SizeConstraints {
                min_width: if min_w > 0.0 { Some(min_w) } else { None },
                max_width: if !max_w.is_infinite() && !max_w.is_nan() {
                    Some(max_w)
                } else {
                    None
                },
                min_height: if min_h > 0.0 { Some(min_h) } else { None },
                max_height: if !max_h.is_infinite() && !max_h.is_nan() {
                    Some(max_h)
                } else {
                    None
                },
            });
        }
        (size_specs, constraints, parent_content_size)
    }

    pub(crate) fn apply_container_layout(
        &mut self,
        container_label: &str,
        time_ms: f64,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(container_id) = self.world.get_id(container_label) else {
            return;
        };
        let Some(metadata) = self.container_layout(container_label).cloned() else {
            return;
        };

        let children = self.layout_children_for(container_id);

        // A `Percent`/`Fill` child needs a parent content box. Without an
        // authored `size` the box would otherwise be derived from the very
        // children being sized, so `size: (25%)` resolved against a container
        // that shrink-wrapped to that same child. The `size` *track* cannot
        // tell us this: the layout pass writes it too, so an authored fixed
        // size and a derived one look identical — check the authored spec.
        let declared_size = self.world.geometry(container_id).size_spec;
        if declared_size.is_none()
            && children.iter().any(|cl| {
                self.world.geometry(cl.id).size_spec.is_some_and(|spec| spec.needs_parent_box())
            })
        {
            let container_ty = self.world.actor_type(container_id).to_string();
            Self::push_percent_without_container_size_diagnostic(
                diagnostics,
                &container_ty,
                container_label,
            );
        }

        // Layout containers consume child-local half-extents from the dedicated
        // layout-size track. Authored shapes seed this track from declared geometry;
        // text, math, code, image, and SVG paths seed it from measured or
        // intrinsic bounds. Placement is declaration-time and does not promise
        // sampled per-frame relayout when those tracks animate later.
        //
        // A child whose size is `Percent`/`Fill` has no `layout_size` yet (the
        // layout pass below is what resolves it), so it enters at zero extent
        // rather than being dropped: dropping it left the container shrinking
        // to the fixed siblings, which is what made percentages resolve
        // against a near-zero parent box.
        let child_extents: Vec<ChildExtent> = children
            .iter()
            .map(|cl| ChildExtent {
                half_size: self.world.geometry(cl.id).layout_size_last().unwrap_or([0.0, 0.0]),
                placement_mode: cl.placement_mode,
            })
            .collect();

        let t_ms = time_ms as u64;

        // Sample child baselines for baseline alignment
        let child_baselines: Vec<f32> = children
            .iter()
            .map(|cl| self.world.text_or_default(cl.id).baseline_get(t_ms))
            .collect();

        // Collect size specs and constraints for the layout pass.
        let (size_specs, constraints, parent_content_size) =
            self.collect_child_layout_specs(container_id, &children, t_ms);

        let layout_results = LayoutEngine::compute_layout_with_specs(
            &metadata,
            &child_extents,
            &child_baselines,
            &size_specs,
            &constraints,
            parent_content_size,
        );

        // Write positions and resolved sizes to tracks, only for LayoutManaged children
        for (i, child) in children.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                let geometry = self.world.geometry.get_mut(child.id);
                geometry.position.ensure([0.0, 0.0]).add_keyframe(
                    t_ms,
                    layout_results[i].position,
                    Easing::Linear,
                );

                let resolved_half_size = [
                    layout_results[i].size[0] / 2.0,
                    layout_results[i].size[1] / 2.0,
                ];
                let has_dynamic_sizing =
                    size_specs.get(i).copied().flatten().is_some_and(|spec| !spec.is_fixed());
                if has_dynamic_sizing && resolved_half_size[0] > 0.0 && resolved_half_size[1] > 0.0
                {
                    geometry.ensure_layout_size(resolved_half_size).add_keyframe(
                        t_ms,
                        resolved_half_size,
                        Easing::Linear,
                    );
                    geometry.size.ensure(resolved_half_size).add_keyframe(
                        t_ms,
                        resolved_half_size,
                        Easing::Linear,
                    );
                }
            }
        }
    }
}
