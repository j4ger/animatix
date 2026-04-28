//!
//! # Layout Design Contract
//!
//! Layout is a **declaration-time measure/place contract**, not a per-frame reflow.
//! It is evaluated once when a layout container (Row, Col, Grid, Stack) is applied,
//! and does not re-sample when animated tracks change later.
//!
//! 1. **Declaration-time measure**: Layout-managed children publish their local
//!    half-extents into the shared `size` track. Authored shapes seed this from
//!    declared geometry; text, math, code, image, and SVG seed it from measured
//!    or intrinsic bounds.
//!
//! 2. **Container placement**: Containers (Row, Col, Grid, Stack) consume child
//!    half-extents and place children deterministically based on declared order,
//!    gap, and alignment. Placement is frozen at declaration time.
//!
//! 3. **Authored `at` opts out**: A child with an authored `at` value is skipped
//!    by container placement; the author takes full responsibility for its position.
//!
//! 4. **Root default**: Root layout containers that are layout-managed with no `at`
//!    default to scene center (0, 0).
//!
//! 5. **Visual transforms independent**: Visual transforms (scale, rotation) do
//!    not affect layout size under the current contract; they are purely presentational.
//!
//! 6. **No sampled relayout**: When `size` or `position` tracks animate later, layout
//!    does not re-evaluate. This is a deliberate trade-off for predictability.

use std::collections::BTreeMap;

use super::{AnimationTrack, Diagnostic, Easing, PlacementMode, Timeline, TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};

use super::taffy_layout::{compute_taffy_linear_layout, compute_taffy_grid_layout};

/// Represents a child's layout-relevant size at a specific point in time.
#[derive(Clone, Debug)]
pub struct ChildExtent {
    pub label: String,
    pub half_size: [f32; 2],
    pub placement_mode: PlacementMode,
}

/// Pure layout computation for a Stack container.
/// Stack places all children at origin (0, 0).
fn compute_stack_layout(children: &[ChildExtent]) -> Vec<[f32; 2]> {
    children.iter().map(|_| [0.0, 0.0]).collect()
}

/// Pure layout computation for Row/Col containers using Taffy.
/// Returns the positions of each child relative to the container center.
fn compute_linear_layout(
    children: &[ChildExtent],
    is_row: bool,
    gap: f32,
    align: &str,
) -> Vec<[f32; 2]> {
    let layout_type = if is_row {
        super::LayoutType::Row
    } else {
        super::LayoutType::Col
    };

    let results = compute_taffy_linear_layout(children, layout_type, gap, align);
    results.into_iter().map(|r| r.position).collect()
}

/// Pure layout computation for Grid containers using Taffy.
/// Returns the positions of each child relative to the container center.
fn compute_grid_layout(
    children: &[ChildExtent],
    gap: f32,
    cols: usize,
) -> Vec<[f32; 2]> {
    let results = compute_taffy_grid_layout(children, gap, cols);
    results.into_iter().map(|r| r.position).collect()
}

use super::LayoutEngine;

use super::ContainerMetadata;

impl LayoutEngine {
    /// Computes layout positions for all children of a container at a specific time.
    /// Returns a BTreeMap mapping child labels to their computed positions.
    pub fn compute_layout_for_time(
        &self,
        container_label: &str,
        metadata: &ContainerMetadata,
        time_ms: u64,
        tracks: &BTreeMap<String, AnimationTrack>,
    ) -> BTreeMap<String, [f32; 2]> {
        let children = if let Some(track) = tracks.get(container_label) {
            track.children.clone()
        } else {
            return BTreeMap::new();
        };

        let is_row = metadata.layout_type == super::LayoutType::Row;
        let is_col = metadata.layout_type == super::LayoutType::Col;
        let is_stack = metadata.layout_type == super::LayoutType::Stack;
        let is_grid = metadata.layout_type == super::LayoutType::Grid;

        if !is_row && !is_col && !is_stack && !is_grid {
            return BTreeMap::new();
        }

        // Sample child extents at current time
        let child_extents: Vec<ChildExtent> = children
            .iter()
            .filter_map(|cl| {
                tracks.get(cl).map(|track| ChildExtent {
                    label: cl.clone(),
                    half_size: track.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE),
                    placement_mode: track.placement_mode.get(time_ms, PlacementMode::LayoutManaged),
                })
            })
            .collect();

        // Compute positions using pure functions
        let positions: Vec<[f32; 2]> = if is_stack {
            compute_stack_layout(&child_extents)
        } else if is_grid {
            compute_grid_layout(&child_extents, metadata.gap, metadata.cols.unwrap_or(1).max(1))
        } else {
            compute_linear_layout(
                &child_extents,
                is_row,
                metadata.gap,
                &metadata.align,
            )
        };

        // Build result BTreeMap, only including LayoutManaged children
        let mut result = BTreeMap::new();
        for (i, child) in child_extents.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                result.insert(child.label.clone(), positions[i]);
            }
        }

        result
    }
}

impl Timeline {
    fn push_layout_size_fallback_diagnostic(
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
                    "Layout-managed child '{child_label}' in {container_ty} container '{container_label}' had no seeded size at layout time; using default half-size [50, 50]."
                ),
            )
            .with_subject(child_label),
        );
    }

    pub(super) fn apply_container_layout(
        &mut self,
        container_label: &str,
        container_ty: &str,
        time_ms: f64,
        gap: f32,
        align: Option<&str>,
        cols: Option<usize>,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let children = if let Some(track) = self.tracks.get(container_label) {
            track.children.clone()
        } else {
            return;
        };

        let is_row = container_ty == "Row";
        let is_col = container_ty == "Col";
        let is_stack = container_ty == "Stack";
        let is_grid = container_ty == "Grid";

        if !is_row && !is_col && !is_stack && !is_grid {
            return;
        }

        // Layout containers consume child-local half-extents from the shared
        // size track. Authored shapes seed this track from declared geometry;
        // text, math, code, image, and SVG paths seed it from measured or
        // intrinsic bounds. Placement is declaration-time and does not promise
        // sampled per-frame relayout when those tracks animate later.
        let child_extents: Vec<ChildExtent> = children
            .iter()
            .filter_map(|cl| {
                self.tracks.get(cl).map(|track| {
                    let placement_mode = track.placement_mode.last(PlacementMode::LayoutManaged);
                    if placement_mode == PlacementMode::LayoutManaged && track.size.is_none() {
                        Self::push_layout_size_fallback_diagnostic(
                            diagnostics,
                            container_label,
                            container_ty,
                            cl,
                        );
                    }

                    ChildExtent {
                        label: cl.clone(),
                        half_size: track.size.last(DEFAULT_LAYOUT_HALF_SIZE),
                        placement_mode,
                    }
                })
            })
            .collect();

        let t_ms = time_ms as u64;

        // Compute positions using pure functions
        let positions: Vec<[f32; 2]> = if is_stack {
            compute_stack_layout(&child_extents)
        } else if is_grid {
            compute_grid_layout(&child_extents, gap, cols.unwrap_or(1).max(1))
        } else {
            compute_linear_layout(
                &child_extents,
                is_row,
                gap,
                align.unwrap_or("center"),
            )
        };

        // Write positions to tracks, only for LayoutManaged children
        for (i, child_label) in children.iter().enumerate() {
            if let Some(track) = self.tracks.get_mut(child_label) {
                if track.placement_mode.last(PlacementMode::LayoutManaged) == PlacementMode::LayoutManaged {
                    track.position.ensure([0.0, 0.0]).add_keyframe(t_ms, positions[i], Easing::Linear);
                }
            }
        }
    }
}
