//! Container layout application: admission, specs, and the Taffy pass.

use super::*;

impl Timeline {
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
    /// so its layout extent is exactly the placed extent of its children. A
    /// group nested in a Row/Col otherwise publishes the default 100×100 box,
    /// which inflates the main axis and reports a cross-extent unrelated to the
    /// content it wraps (`*_wrap: Group` around a single text line).
    pub(crate) fn publish_group_measured_extent(&mut self, label: &str, time_ms: u64) {
        let Some(group_id) = self.world.get_id(label) else {
            return;
        };
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
        if half[0] <= 0.0 && half[1] <= 0.0 {
            return;
        }
        // A declared fixed size stays authoritative.
        if self.world.geometry(group_id).size_spec.is_some_and(|spec| spec.is_fixed()) {
            return;
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

        // The parent content box drives `Percent`/`Fill` child sizing. Prefer
        // the container's *declared* size: on the first pass no child has a
        // resolved `layout_size` yet, so deriving the box from the resolved
        // track would resolve percentages against zero (or against the
        // container's own content-driven size, a circle). Only when the author
        // declared no size does the resolved track apply.
        let parent_content_size = {
            let geometry = self.world.geometry(container_id);
            geometry
                .size
                .as_ref()
                .map(|t| t.last_value())
                .or_else(|| geometry.layout_size_get(time_ms))
                .or_else(|| geometry.layout_size_last())
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
