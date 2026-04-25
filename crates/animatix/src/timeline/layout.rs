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

use super::{AnimationTrack, Easing, PlacementMode, SceneNode, Timeline, TrackAccessor, DEFAULT_LAYOUT_HALF_SIZE};

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

/// Pure layout computation for Row/Col containers.
/// Returns the positions of each child relative to the container center.
fn compute_linear_layout(
    children: &[ChildExtent],
    is_row: bool,
    gap: f32,
    align: &str,
) -> Vec<[f32; 2]> {
    let mut total_extent = 0.0f32;
    let mut max_cross_extent = 0.0f32;

    for half_size in children {
        let (w, h) = (half_size.half_size[0] * 2.0, half_size.half_size[1] * 2.0);
        if is_row {
            total_extent += w;
            if max_cross_extent < h {
                max_cross_extent = h;
            }
        } else {
            total_extent += h;
            if max_cross_extent < w {
                max_cross_extent = w;
            }
        }
    }

    if !children.is_empty() && children.len() > 1 {
        total_extent += gap * (children.len() as f32 - 1.0);
    }

    let cross_offset = match align {
        "start" => -max_cross_extent / 2.0,
        "end" => max_cross_extent / 2.0,
        _ => 0.0,
    };

    let main_start = -total_extent / 2.0;
    let mut offset = 0.0f32;
    let mut positions = Vec::with_capacity(children.len());

    for (i, child) in children.iter().enumerate() {
        let (child_w, child_h) = (child.half_size[0] * 2.0, child.half_size[1] * 2.0);

        let (x, y) = if is_row {
            let cx = main_start + offset + child_w / 2.0;
            offset += child_w;
            if i < children.len() - 1 {
                offset += gap;
            }
            let cy = match align {
                "start" => cross_offset + child_h / 2.0,
                "end" => cross_offset - child_h / 2.0,
                _ => cross_offset,
            };
            (cx, cy)
        } else {
            let cy = main_start + offset + child_h / 2.0;
            offset += child_h;
            if i < children.len() - 1 {
                offset += gap;
            }
            let cx = match align {
                "start" => cross_offset + child_w / 2.0,
                "end" => cross_offset - child_w / 2.0,
                _ => cross_offset,
            };
            (cx, cy)
        };
        positions.push([x, y]);
    }

    positions
}

/// Pure layout computation for Grid containers.
/// Returns the positions of each child relative to the container center.
fn compute_grid_layout(
    children: &[ChildExtent],
    gap: f32,
    cols: usize,
) -> Vec<[f32; 2]> {
    let cols = cols.max(1);
    let rows = children.len().div_ceil(cols);
    let mut col_widths = vec![0.0f32; cols];
    let mut row_heights = vec![0.0f32; rows.max(1)];

    for (index, child) in children.iter().enumerate() {
        let (child_w, child_h) = (child.half_size[0] * 2.0, child.half_size[1] * 2.0);
        let row = index / cols;
        let col = index % cols;
        col_widths[col] = col_widths[col].max(child_w);
        row_heights[row] = row_heights[row].max(child_h);
    }

    let total_width =
        col_widths.iter().sum::<f32>() + gap * (col_widths.len().saturating_sub(1) as f32);
    let total_height = row_heights.iter().sum::<f32>()
        + gap * (row_heights.len().saturating_sub(1) as f32);

    let mut row_starts = Vec::with_capacity(row_heights.len());
    let mut current_y = -total_height / 2.0;
    for row_height in &row_heights {
        row_starts.push(current_y);
        current_y += *row_height + gap;
    }

    let mut col_starts = Vec::with_capacity(col_widths.len());
    let mut current_x = -total_width / 2.0;
    for col_width in &col_widths {
        col_starts.push(current_x);
        current_x += *col_width + gap;
    }

    let mut positions = Vec::with_capacity(children.len());
    for index in 0..children.len() {
        let row = index / cols;
        let col = index % cols;
        if row >= row_heights.len() || col >= col_widths.len() {
            positions.push([0.0, 0.0]);
            continue;
        }

        let x = col_starts[col] + col_widths[col] / 2.0;
        let y = row_starts[row] + row_heights[row] / 2.0;
        positions.push([x, y]);
    }

    positions
}

fn layout_full_extents(track: &AnimationTrack) -> (f32, f32) {
    let half_extents = track.size.last(DEFAULT_LAYOUT_HALF_SIZE);

    (half_extents[0] * 2.0, half_extents[1] * 2.0)
}

/// Returns the full extents (width, height) of a track at a specific time.
fn layout_full_extents_at_time(track: &AnimationTrack, time_ms: u64) -> (f32, f32) {
    let half_extents = track.size.get(time_ms, DEFAULT_LAYOUT_HALF_SIZE);

    (half_extents[0] * 2.0, half_extents[1] * 2.0)
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
        nodes: &BTreeMap<String, SceneNode>,
    ) -> BTreeMap<String, [f32; 2]> {
        let children = if let Some(node) = nodes.get(container_label) {
            node.children.clone()
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
    pub(super) fn apply_container_layout(
        &mut self,
        container_label: &str,
        container_ty: &str,
        time_ms: f64,
        gap: f32,
        align: Option<&str>,
        cols: Option<usize>,
    ) {
        let children = if let Some(node) = self.nodes.get(container_label) {
            node.children.clone()
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
                self.tracks.get(cl).map(|track| ChildExtent {
                    label: cl.clone(),
                    half_size: track.size.last(DEFAULT_LAYOUT_HALF_SIZE),
                    placement_mode: track.placement_mode.last(PlacementMode::LayoutManaged),
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
