//! Resolved child layout positions, keyed by entity id.

/// Resolved child positions for one container, keyed by child `EntityId`.
///
/// The frame path resolves ids once and never hashes a label per node
/// (`docs/ecs.md` §4); the label view lives only at the GUI/action boundary.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LayoutPositions {
    by_id: std::collections::HashMap<crate::timeline::EntityId, [f32; 2]>,
}

impl LayoutPositions {
    pub fn new() -> Self {
        Self {
            by_id: std::collections::HashMap::new(),
        }
    }

    #[inline(always)]
    pub fn insert_id(&mut self, id: crate::timeline::EntityId, pos: [f32; 2]) {
        self.by_id.insert(id, pos);
    }

    #[inline(always)]
    pub fn get_by_id(&self, id: crate::timeline::EntityId) -> Option<&[f32; 2]> {
        self.by_id.get(&id)
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.by_id.len()
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
