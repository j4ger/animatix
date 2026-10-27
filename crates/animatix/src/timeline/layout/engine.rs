//! Layout engine cache and per-container position computation.

use super::grid::compute_grid_layout_results_with_specs;
use super::linear::compute_linear_layout_results_with_specs;
use super::positions::LayoutPositions;
use super::{ChildExtent, compute_stack_layout_results};
use crate::timeline::property_track::TrackAccessor;
use crate::timeline::taffy_layout::{ChildSizeSpec, SizeConstraints, TaffyLayoutResult};
use crate::timeline::{
    ContainerLayoutChild, ContainerMetadata, LayoutEngine, LayoutType, PlacementMode,
};

/// Cached layout computation result for a single container.
///
/// Positions are shared through an `Arc` so a cache hit hands back a
/// refcount bump instead of cloning the whole per-child `String`-keyed map
/// (PF-4: the clone was one heap allocation per child per frame).
#[derive(Clone, Debug)]
pub(crate) struct LayoutCacheEntry {
    /// Cached output positions.
    positions: std::sync::Arc<LayoutPositions>,
}

/// Per-frame dynamic layout inputs for one container, compared by exact value.
///
/// Only these inputs vary between frames: child `layout_size` fingerprints and
/// baselines. Everything else that the previous structured
/// [`LayoutCacheKey`](..) carried (container identity, metadata strings, child
/// membership/order) is fixed at build time and therefore implied by the
/// per-container cache bucket this key lives in — carrying it here again cost
/// 3N+3 `String` allocations per container per frame purely to look up a
/// cached result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct LayoutDynKey {
    /// Child size/placement fingerprints, in admitted order.
    fingerprints: Vec<([u32; 2], u8)>,
    /// Baseline alignment values (bit representation), in admitted order.
    baselines: Vec<u32>,
}

/// Per-container cache bucket: memoized admitted children plus the results
/// computed for the dynamic input states seen since the last invalidation.
///
/// The container label is the bucket key and the static identity: metadata
/// (`gap`/`padding`/`align`/`cols`), admitted membership, and child order are
/// fixed between invalidations — every public mutable accessor funnels through
/// `invalidate_frame_cache`, which clears this cache. Child `layout_size` MAY
/// change per frame and stays in the dynamic key.
#[derive(Clone, Debug)]
pub(crate) struct LayoutContainerCache {
    /// Cached results per dynamic input state (usually one).
    entries: Vec<(LayoutDynKey, LayoutCacheEntry)>,
    /// PF-6 round 13: for containers whose children's `layout_size` and
    /// `baseline` tracks carry no keyframes, the fingerprint pass output is
    /// identical every frame — serve the result directly and skip the
    /// per-child `tracks.get` BTreeMap navigation (60-child containers
    /// measured 1.85% of the evaluate frame). Cleared by
    /// `invalidate_cache`, the same mutation funnel as `entries`.
    static_result: Option<std::sync::Arc<LayoutPositions>>,
    /// Whether the fast path above has been validated for this bucket (all
    /// children keyframe-free at install time).
    children_are_keyframe_free: bool,
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
    pub(crate) fn compute_positions(
        metadata: &ContainerMetadata,
        children: &[ChildExtent],
    ) -> Vec<[f32; 2]> {
        Self::compute_positions_with_baselines(metadata, children, &[])
    }

    /// Like `compute_positions` but supports baseline alignment.
    /// `child_baselines` are per-child baseline offsets from text center (f32).
    /// Empty slice means no baseline info available.
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
    ) -> Vec<TaffyLayoutResult> {
        let is_row = metadata.layout_type == LayoutType::Row;
        let is_col = metadata.layout_type == LayoutType::Col;
        let is_stack = metadata.layout_type == LayoutType::Stack;
        let is_grid = metadata.layout_type == LayoutType::Grid;

        if !is_row && !is_col && !is_stack && !is_grid {
            return Vec::new();
        }

        if is_stack {
            compute_stack_layout_results(children, &metadata.align)
        } else if is_grid {
            compute_grid_layout_results_with_specs(
                children,
                metadata.gap,
                metadata.padding,
                metadata.cols.unwrap_or(1).max(1),
                size_specs,
                constraints,
                parent_content_size,
            )
        } else {
            compute_linear_layout_results_with_specs(
                children,
                is_row,
                metadata.gap,
                metadata.padding,
                &metadata.align,
                metadata,
                child_baselines,
                size_specs,
                constraints,
                parent_content_size,
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
        // PF-4: sample the per-frame dynamic inputs WITHOUT owning labels —
        // the hit path needs no child labels at all, so no `String` is
        // allocated here. Baselines keep the legacy shape: one entry per
        // `layout_children` element (0.0 when the track is missing), while
        // fingerprints cover only admitted children.
        {
            // PF-6 round 13 fast path: for a keyframe-free container the
            // fingerprint pass output is constant, so the first computed
            // result is served directly and the per-child `tracks.get`
            // BTreeMap navigation disappears from the steady-state frame.
            let cache = self.cache.borrow();
            if let Some(bucket) = cache.get(&container_id) {
                if bucket.children_are_keyframe_free {
                    if let Some(positions) = &bucket.static_result {
                        return std::sync::Arc::clone(positions);
                    }
                }
            }
        }

        let mut fingerprints: Vec<([u32; 2], u8)> = Vec::with_capacity(layout_children.len());
        let mut baselines: Vec<f32> = Vec::with_capacity(layout_children.len());
        let mut children_are_keyframe_free = true;
        for child in layout_children {
            let child_id = child.id;
            // `layout_size`/`baseline` without keyframes sample to a constant;
            // any keyframe makes the container dynamic for this fast path.
            children_are_keyframe_free &= world
                .geometry(child_id)
                .layout_size
                .as_ref()
                .is_none_or(|t| t.keyframes.is_empty())
                && world
                    .text_or_default(child_id)
                    .baseline
                    .as_ref()
                    .is_none_or(|t| t.keyframes.is_empty());
            baselines.push(world.text_or_default(child_id).baseline_get(time_ms));
            if let Some(half_size) = world.geometry(child_id).layout_size_get(time_ms) {
                fingerprints.push((half_size.map(f32_bits), child.placement_mode as u8));
            }
        }
        let dyn_key = LayoutDynKey {
            fingerprints,
            baselines: baselines.iter().map(|v| f32_bits(*v)).collect(),
        };

        {
            let cache = self.cache.borrow();
            if let Some(bucket) = cache.get(&container_id) {
                if let Some((_, entry)) = bucket.entries.iter().find(|(key, _)| *key == dyn_key) {
                    // Hit: share the cached positions (refcount bump, no clone
                    // of the child map).
                    return std::sync::Arc::clone(&entry.positions);
                }
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

        let (size_specs, constraints, parent_content_size) = {
            let p_size = {
                let geometry = world.geometry(container_id);
                geometry
                    .layout_size_get(time_ms)
                    .or_else(|| geometry.layout_size_last())
                    .or_else(|| geometry.size.as_ref().map(|t| t.evaluate(time_ms)))
                    .or_else(|| geometry.size.as_ref().map(|t| t.last_value()))
            }
            .map(|s| [s[0] * 2.0, s[1] * 2.0])
            .unwrap_or([0.0, 0.0]);
            let mut specs = Vec::with_capacity(layout_children.len());
            let mut cons = Vec::with_capacity(layout_children.len());
            for child in layout_children {
                let geometry = world.geometry(child.id);
                specs.push(geometry.size_spec);
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
            (specs, cons, p_size)
        };

        let positions = Self::compute_positions_with_specs(
            metadata,
            &child_extents,
            &baselines,
            &size_specs,
            &constraints,
            parent_content_size,
        );

        // Build result map, only including LayoutManaged children
        let mut result = LayoutPositions::new();
        for (i, extent) in child_extents.iter().enumerate() {
            if extent.placement_mode == PlacementMode::LayoutManaged {
                result.insert_id(admitted_ids[i], positions[i]);
            }
        }
        let positions = std::sync::Arc::new(result);

        // Store in cache. Keyframe-free containers also get the static fast
        // path: their inputs (and therefore their result) are frame-invariant.
        let mut cache = self.cache.borrow_mut();
        let bucket = cache.entry(container_id).or_insert_with(|| LayoutContainerCache {
            entries: Vec::new(),
            static_result: None,
            children_are_keyframe_free: false,
        });
        bucket.entries.push((
            dyn_key,
            LayoutCacheEntry {
                positions: std::sync::Arc::clone(&positions),
            },
        ));
        if children_are_keyframe_free {
            bucket.children_are_keyframe_free = true;
            bucket.static_result = Some(std::sync::Arc::clone(&positions));
        }

        positions
    }
}
