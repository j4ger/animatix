//! Resolved child layout positions, keyed by entity id and actor label.

/// Resolved child positions for one container, keyed by child label.
///
/// Layout positions calculated for container children, indexed by either `EntityId`
/// (for hot per-frame rendering without string lookups) or actor label string.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutPositions {
    by_id: std::collections::HashMap<crate::timeline::EntityId, [f32; 2]>,
    by_label: std::collections::HashMap<String, [f32; 2]>,
}

impl LayoutPositions {
    pub fn new() -> Self {
        Self {
            by_id: std::collections::HashMap::new(),
            by_label: std::collections::HashMap::new(),
        }
    }

    #[inline(always)]
    pub fn insert(&mut self, label: String, pos: [f32; 2]) {
        self.by_label.insert(label, pos);
    }

    #[inline(always)]
    pub fn insert_id(&mut self, id: crate::timeline::EntityId, pos: [f32; 2]) {
        self.by_id.insert(id, pos);
    }

    #[inline(always)]
    pub fn insert_with_id(&mut self, id: crate::timeline::EntityId, label: String, pos: [f32; 2]) {
        self.by_id.insert(id, pos);
        self.by_label.insert(label, pos);
    }

    #[inline(always)]
    pub fn get(&self, label: &str) -> Option<&[f32; 2]> {
        self.by_label.get(label)
    }

    #[inline(always)]
    pub fn get_by_id(&self, id: crate::timeline::EntityId) -> Option<&[f32; 2]> {
        self.by_id.get(&id)
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.by_label.is_empty() && self.by_id.is_empty()
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.by_label.len().max(self.by_id.len())
    }
}

/// One shared empty `Arc<LayoutPositions>`.
///
/// "This actor has no layout-managed children" paths used to allocate a fresh
/// empty map per call — once per non-container node per frame on typical
/// scenes (~60 allocations/frame on a 60-actor scene, `alloc_driver`
/// 2026-09-04). Callers only ever read through the `Arc`, so sharing one
/// instance is equivalent and allocation-free.
pub(crate) fn empty_layout_positions() -> std::sync::Arc<LayoutPositions> {
    use std::sync::{Arc, OnceLock};
    static EMPTY: OnceLock<Arc<LayoutPositions>> = OnceLock::new();
    Arc::clone(EMPTY.get_or_init(|| Arc::new(LayoutPositions::new())))
}
