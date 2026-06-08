//!
//! # Layout Design Contract
//!
//! Layout is a **declaration-time measure/place contract**, not a per-frame reflow.
//! It is evaluated once when a layout container (Row, Col, Grid, Stack) is applied,
//! and does not re-sample when animated tracks change later.
//!
//! 1. **Declaration-time measure**: Layout-managed children publish their local
//!    half-extents into a dedicated layout-size track. Authored shapes seed this from
//!    declared geometry; text, math, code, image, and SVG seed it from measured
//!    or intrinsic bounds.
//!
//! 2. **Container placement**: Containers (Row, Col, Grid, Stack) consume child
//!    half-extents and place children deterministically based on declared order,
//!    gap, padding, and alignment. Placement is frozen at declaration time.
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
//! 6. **No sampled relayout**: When layout-size or `position` tracks animate later, layout
//!    does not re-evaluate. This is a deliberate trade-off for predictability.

use std::collections::BTreeMap;

use super::{AnimationTrack, ContainerLayoutChild, Diagnostic, Easing, PlacementMode, Timeline, TrackAccessor};
use crate::diagnostics::{DiagnosticCode, DiagnosticPhase};
use tracing::warn;

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
    padding: f32,
    align: &str,
) -> Vec<[f32; 2]> {
    let layout_type = if is_row {
        super::LayoutType::Row
    } else {
        super::LayoutType::Col
    };

    let results = compute_taffy_linear_layout(children, layout_type, gap, padding, align);
    results.into_iter().map(|r| r.position).collect()
}

/// Pure layout computation for Grid containers using Taffy.
/// Returns the positions of each child relative to the container center.
fn compute_grid_layout(
    children: &[ChildExtent],
    gap: f32,
    padding: f32,
    cols: usize,
) -> Vec<[f32; 2]> {
    let results = compute_taffy_grid_layout(children, gap, padding, cols);
    results.into_iter().map(|r| r.position).collect()
}

use super::LayoutEngine;

use super::ContainerMetadata;

/// Cached layout computation result for a single container.
#[derive(Clone, Debug)]
pub(crate) struct LayoutCacheEntry {
    /// Input fingerprint: (half_size_x, half_size_y, placement_mode_idx) per child.
    child_fingerprints: Vec<([f32; 2], u8)>,
    /// Cached output positions.
    positions: BTreeMap<String, [f32; 2]>,
}

impl LayoutEngine {
    /// Create a new layout engine with an empty cache.
    pub fn new() -> Self {
        Self {
            cache: std::cell::RefCell::new(std::collections::HashMap::new()),
        }
    }

    /// Invalidate the layout cache (e.g. after a timeline rebuild).
    pub fn invalidate_cache(&self) {
        self.cache.borrow_mut().clear();
    }
    pub(crate) fn compute_positions(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
    ) -> Vec<[f32; 2]> {
        let is_row = metadata.layout_type == super::LayoutType::Row;
        let is_col = metadata.layout_type == super::LayoutType::Col;
        let is_stack = metadata.layout_type == super::LayoutType::Stack;
        let is_grid = metadata.layout_type == super::LayoutType::Grid;

        if !is_row && !is_col && !is_stack && !is_grid {
            return Vec::new();
        }

        if is_stack {
            compute_stack_layout(children)
        } else if is_grid {
            compute_grid_layout(
                children,
                metadata.gap,
                metadata.padding,
                metadata.cols.unwrap_or(1).max(1),
            )
        } else {
            compute_linear_layout(children, is_row, metadata.gap, metadata.padding, &metadata.align)
        }
    }

    /// Computes layout positions for all children of a container at a specific time.
    /// Returns a BTreeMap mapping child labels to their computed positions.
    ///
    /// Uses a per-container cache keyed on children's layout sizes. If the
    /// children's half-sizes and placement modes haven't changed since the last
    /// call, the cached positions are returned without recomputing via Taffy.
    pub fn compute_layout_for_time(
        &self,
        metadata: &ContainerMetadata,
        layout_children: &[ContainerLayoutChild],
        time_ms: u64,
        tracks: &BTreeMap<String, AnimationTrack>,
    ) -> BTreeMap<String, [f32; 2]> {
        // Sample child extents at current time
        let child_extents: Vec<ChildExtent> = layout_children
            .iter()
            .filter_map(|child| {
                let track = tracks.get(&child.label)?;
                let half_size = track.layout_size_get(time_ms)?;
                Some(ChildExtent {
                    label: child.label.clone(),
                    half_size,
                    placement_mode: child.placement_mode,
                })
            })
            .collect();

        // Build fingerprint for cache lookup
        let fingerprints: Vec<([f32; 2], u8)> = child_extents
            .iter()
            .map(|c| (c.half_size, c.placement_mode as u8))
            .collect();

        // Check cache — use container label from first child's parent context.
        // We key on the child labels themselves to detect structural changes.
        let cache_key = metadata.child_order.join("|");
        {
            let cache = self.cache.borrow();
            if let Some(entry) = cache.get(&cache_key) {
                if entry.child_fingerprints == fingerprints {
                    return entry.positions.clone();
                }
            }
        }

        let positions = Self::compute_positions(metadata, &child_extents);

        // Build result BTreeMap, only including LayoutManaged children
        let mut result = BTreeMap::new();
        for (i, child) in child_extents.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                result.insert(child.label.clone(), positions[i]);
            }
        }

        // Store in cache
        self.cache.borrow_mut().insert(
            cache_key,
            LayoutCacheEntry {
                child_fingerprints: fingerprints,
                positions: result.clone(),
            },
        );

        result
    }
}

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

    pub(super) fn build_layout_children(
        &self,
        container_label: &str,
        container_ty: &str,
        child_order: &[String],
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Vec<ContainerLayoutChild> {
        let mut layout_children = Vec::with_capacity(child_order.len());

        for child_label in child_order {
            let Some(track) = self.tracks.get(child_label) else {
                warn!("layout: child '{}' not found in tracks, skipping", child_label);
                continue;
            };

            let placement_mode = track.placement_mode.last(PlacementMode::LayoutManaged);

            // Only children with seeded layout_size are admitted into the layout set.
            // Manual children may still exist in the scene graph via track.children,
            // but they are excluded from layout spacing/placement when unmeasured.
            let is_admitted = if !track.has_layout_size() {
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
                    label: child_label.clone(),
                    placement_mode,
                });
            }
        }

        layout_children
    }

    pub(super) fn apply_container_layout(
        &mut self,
        container_label: &str,
        time_ms: f64,
        _diagnostics: &mut Vec<Diagnostic>,
    ) {
        let Some(metadata) = self.container_metadata.get(container_label).cloned() else {
            return;
        };

        let children = self.layout_children_for(container_label);

        // Layout containers consume child-local half-extents from the dedicated
        // layout-size track. Authored shapes seed this track from declared geometry;
        // text, math, code, image, and SVG paths seed it from measured or
        // intrinsic bounds. Placement is declaration-time and does not promise
        // sampled per-frame relayout when those tracks animate later.
        let child_extents: Vec<ChildExtent> = children
            .iter()
            .filter_map(|cl| {
                let track = self.tracks.get(&cl.label)?;
                let half_size = track.layout_size_last()?;
                Some(ChildExtent {
                    label: cl.label.clone(),
                    half_size,
                    placement_mode: cl.placement_mode,
                })
            })
            .collect();

        let t_ms = time_ms as u64;

        let positions = LayoutEngine::compute_positions(&metadata, &child_extents);

        // Write positions to tracks, only for LayoutManaged children
        for (i, child) in children.iter().enumerate() {
            if child.placement_mode == PlacementMode::LayoutManaged {
                if let Some(track) = self.tracks.get_mut(&child.label) {
                    track.position.ensure([0.0, 0.0]).add_keyframe(t_ms, positions[i], Easing::Linear);
                }
            }
        }
    }
}
