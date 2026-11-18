//! Layout engine cache and per-container position computation.

use super::grid::compute_grid_layout_results_with_specs;
use super::linear::compute_linear_layout_results_with_specs;
use super::positions::LayoutPositions;
use super::{
    ChildExtent, DEFAULT_GRID_COLS, compute_stack_layout_results, definite_linear_cross_size,
    sampled_container_size,
};
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::taffy_layout::{ChildSizeSpec, SizeConstraints, TaffyLayoutResult};
use crate::timeline::{
    ContainerLayoutChild, ContainerMetadata, LayoutEngine, LayoutType, PlacementMode,
};

/// Cached layout computation result for a single container.
///
/// Positions are shared through an `Arc` so a cache hit hands back a refcount
/// bump instead of cloning the whole per-child `String`-keyed map (one heap
/// allocation per child per frame).
#[derive(Clone, Debug)]
pub(crate) struct LayoutCacheEntry {
    /// Cached output positions.
    positions: std::sync::Arc<LayoutPositions>,
}

/// Per-frame dynamic layout inputs for one container, compared by exact value.
///
/// These inputs vary between frames: the container size and child layout sizes
/// and baselines. Everything else (container identity, metadata strings, child
/// membership/order) is fixed at build time and therefore implied by the
/// per-container cache bucket this key lives in — carrying it here again would
/// allocate 3N+3 `String`s per container per frame just to look up a cached
/// result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayoutDynKey {
    /// Container dimensions used for alignment and relative child sizes.
    container_size: [u32; 2],
    /// Child size/placement fingerprints, in admitted order.
    fingerprints: Vec<([u32; 2], u8)>,
    /// Baseline alignment values (bit representation), in admitted order.
    baselines: Vec<u32>,
}

/// Per-container cache bucket: memoized admitted children plus the results
/// computed for the dynamic input states seen since the last invalidation.
///
/// The container entity id is the bucket key and the static identity: metadata
/// (`gap`/`padding`/`align`/`cols`), admitted membership, and child order are
/// fixed between invalidations — every public mutable accessor funnels through
/// `invalidate_frame_cache`, which clears this cache. Child `layout_size` MAY
/// change per frame and stays in the dynamic key.
#[derive(Clone, Debug)]
pub(crate) struct LayoutContainerCache {
    /// Cached results per dynamic input state (usually one).
    entries: Vec<(LayoutDynKey, LayoutCacheEntry)>,
    /// If the size and baseline tracks are static, serve the same positions
    /// every frame and skip the per-child `tracks.get`
    /// BTreeMap navigation. Cleared by `invalidate_cache`, the same mutation
    /// funnel as `entries`.
    static_result: Option<std::sync::Arc<LayoutPositions>>,
    /// Whether the fast path above has been validated for this bucket.
    inputs_are_static: bool,
}

fn f32_bits(v: f32) -> u32 {
    v.to_bits()
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
    #[cfg(test)]
    pub(crate) fn compute_positions(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
    ) -> Vec<[f32; 2]> {
        Self::compute_positions_with_baselines(metadata, children, &[])
    }

    /// Like `compute_positions` but supports baseline alignment.
    /// `child_baselines` are per-child baseline offsets from text center (f32).
    /// Empty slice means no baseline info available.
    #[cfg(test)]
    pub(crate) fn compute_positions_with_baselines(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
        child_baselines: &[f32],
    ) -> Vec<[f32; 2]> {
        Self::compute_positions_with_specs(
            metadata,
            children,
            child_baselines,
            &[],
            &[],
            [0.0, 0.0],
        )
    }

    /// Like `compute_positions_with_baselines` but supports size specs and constraints.
    #[cfg(test)]
    pub(crate) fn compute_positions_with_specs(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
        child_baselines: &[f32],
        size_specs: &[Option<ChildSizeSpec>],
        constraints: &[SizeConstraints],
        parent_content_size: [f32; 2],
    ) -> Vec<[f32; 2]> {
        Self::compute_layout_with_specs(
            metadata,
            children,
            child_baselines,
            size_specs,
            constraints,
            parent_content_size,
            None,
        )
        .into_iter()
        .map(|r| r.position)
        .collect()
    }

    /// Computes both positions and resolved sizes with size specs and constraints.
    pub(crate) fn compute_layout_with_specs(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
        child_baselines: &[f32],
        size_specs: &[Option<ChildSizeSpec>],
        constraints: &[SizeConstraints],
        parent_content_size: [f32; 2],
        cross_size: Option<f32>,
    ) -> Vec<TaffyLayoutResult> {
        let is_row = metadata.layout_type == LayoutType::Row;
        let is_col = metadata.layout_type == LayoutType::Col;
        let is_stack = metadata.layout_type == LayoutType::Stack;
        let is_grid = metadata.layout_type == LayoutType::Grid;

        if !is_row && !is_col && !is_stack && !is_grid {
            return Vec::new();
        }

        if is_stack {
            compute_stack_layout_results(
                children,
                &metadata.align,
                parent_content_size,
                metadata.padding,
            )
        } else if is_grid {
            compute_grid_layout_results_with_specs(
                children,
                metadata.gap,
                metadata.padding,
                metadata.cols.unwrap_or(DEFAULT_GRID_COLS).max(1),
                size_specs,
                constraints,
                parent_content_size,
            )
        } else {
            compute_linear_layout_results_with_specs(
                metadata,
                children,
                child_baselines,
                size_specs,
                constraints,
                parent_content_size,
                cross_size,
            )
        }
    }

    /// Computes layout positions for all children of a container at a specific time.
    /// Returns positions keyed by child entity id.
    ///
    /// Uses a per-container cache keyed on children's layout sizes. If the
    /// children's half-sizes and placement modes haven't changed since the last
    /// call, the cached positions are returned without recomputing via Taffy.
    pub fn compute_layout_for_time(
        &self,
        container_id: crate::timeline::EntityId,
        metadata: &ContainerMetadata,
        layout_children: &[ContainerLayoutChild],
        time_ms: u64,
        world: &crate::timeline::World,
    ) -> std::sync::Arc<LayoutPositions> {
        // Sample the per-frame dynamic inputs WITHOUT owning labels — the hit
        // path needs no child labels at all, so no `String` is allocated here.
        // Fingerprints and optional baselines follow the admitted order.
        {
            // Static layout inputs can serve the first computed result
            // directly and skip per-child track sampling.
            let cache = self.cache.borrow();
            if let Some(bucket) = cache.get(&container_id)
                && bucket.inputs_are_static
                && let Some(positions) = &bucket.static_result
            {
                return std::sync::Arc::clone(positions);
            }
        }

        let geometry = world.geometry(container_id);
        let parent_content_size = sampled_container_size(geometry, time_ms);
        let mut inputs_are_static = geometry
            .layout_size
            .as_ref()
            .or(geometry.size.as_ref())
            .is_none_or(|track| track.is_effectively_static());
        let mut fingerprints: Vec<([u32; 2], u8)> = Vec::with_capacity(layout_children.len());
        let mut baselines: Vec<f32> = Vec::new();
        for child in layout_children {
            let child_id = child.id;
            let geometry = world.geometry(child_id);
            inputs_are_static &=
                geometry.layout_size.as_ref().is_none_or(|track| track.keyframes.is_empty());
            if let Some(half_size) = geometry.layout_size_get(time_ms) {
                fingerprints.push((half_size.map(f32_bits), child.placement_mode as u8));
                if metadata.vertical_align == "baseline" {
                    let text = world.text_or_default(child_id);
                    inputs_are_static &=
                        text.baseline.as_ref().is_none_or(|track| track.keyframes.is_empty());
                    baselines.push(text.baseline_get(time_ms));
                }
            }
        }
        let dyn_key = LayoutDynKey {
            container_size: parent_content_size.map(f32_bits),
            fingerprints,
            baselines: baselines.iter().map(|v| f32_bits(*v)).collect(),
        };

        {
            let cache = self.cache.borrow();
            if let Some(bucket) = cache.get(&container_id)
                && let Some((_, entry)) = bucket.entries.iter().find(|(key, _)| *key == dyn_key)
            {
                // Hit: share the cached positions (refcount bump, no clone
                // of the child map).
                return std::sync::Arc::clone(&entry.positions);
            }
        }

        // Miss: rebuild extents and compute. Extents drop children without a
        // sampled `layout_size`; the parallel id list keeps the result map
        // aligned with the computed positions.
        let mut admitted_ids: Vec<crate::timeline::EntityId> =
            Vec::with_capacity(layout_children.len());
        let child_extents: Vec<ChildExtent> = layout_children
            .iter()
            .filter_map(|child| {
                let half_size = world.geometry(child.id).layout_size_get(time_ms)?;
                admitted_ids.push(child.id);
                Some(ChildExtent {
                    half_size,
                    placement_mode: child.placement_mode,
                })
            })
            .collect();

        let (size_specs, constraints) = {
            let mut specs = Vec::with_capacity(layout_children.len());
            let mut cons = Vec::with_capacity(layout_children.len());
            for child in &admitted_ids {
                let geometry = world.geometry(*child);
                specs.push(geometry.size_spec.map(|spec| spec.measured_dimensions()));
                let min_w = geometry.min_width.get(time_ms, 0.0);
                let min_h = geometry.min_height.get(time_ms, 0.0);
                let max_h = geometry.max_height.get(time_ms, f32::INFINITY);
                let max_w = geometry.max_item_width.get(time_ms, f32::INFINITY);
                cons.push(SizeConstraints {
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
            (specs, cons)
        };
        let cross_size = definite_linear_cross_size(
            metadata.layout_type,
            geometry.size_spec,
            parent_content_size,
        );

        let positions = Self::compute_layout_with_specs(
            metadata,
            &child_extents,
            &baselines,
            &size_specs,
            &constraints,
            parent_content_size,
            cross_size,
        );

        // Build result map, only including LayoutManaged children
        let mut result = LayoutPositions::new();
        for (i, extent) in child_extents.iter().enumerate() {
            if extent.placement_mode == PlacementMode::LayoutManaged {
                result.insert_id(admitted_ids[i], positions[i].position);
            }
        }
        let positions = std::sync::Arc::new(result);

        // Store in cache. Frame-invariant inputs also get the static fast path.
        let mut cache = self.cache.borrow_mut();
        let bucket = cache.entry(container_id).or_insert_with(|| LayoutContainerCache {
            entries: Vec::new(),
            static_result: None,
            inputs_are_static: false,
        });
        bucket.entries.push((
            dyn_key,
            LayoutCacheEntry {
                positions: std::sync::Arc::clone(&positions),
            },
        ));
        if inputs_are_static {
            bucket.inputs_are_static = true;
            bucket.static_result = Some(std::sync::Arc::clone(&positions));
        }

        positions
    }
}
