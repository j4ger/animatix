//! BehaviorRegistry storing built-in and extension behavior definitions.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

use super::def::{BehaviorDef, BehaviorId, BehaviorSource};

/// Static lookup table mapping built-in primitive type names to their canonical [`BehaviorId`].
///
/// IDs are the row indices of [`crate::primitives::BUILT_INS`], so the table's
/// registration order is the stable serialized order.
pub fn behavior_id_for_builtin(name: &str) -> Option<BehaviorId> {
    static IDS: OnceLock<HashMap<&'static str, BehaviorId>> = OnceLock::new();
    IDS.get_or_init(|| {
        crate::primitives::BUILT_INS
            .iter()
            .enumerate()
            .map(|(index, builtin)| (builtin.behavior.type_name(), BehaviorId(index as u16)))
            .collect()
    })
    .get(name)
    .copied()
}

/// Registry holding behavior definitions indexed by [`BehaviorId`].
///
/// Seeded with one behavior per [`crate::primitives::BUILT_INS`] row.
/// Supports registering extension primitives at runtime.
#[derive(Clone)]
pub struct BehaviorRegistry {
    behaviors: Vec<BehaviorDef>,
}

impl Default for BehaviorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BehaviorRegistry {
    /// Create a new registry pre-seeded with all built-in primitives.
    pub fn new() -> Self {
        let mut behaviors = Vec::with_capacity(crate::primitives::BUILT_INS.len());
        for (i, builtin) in crate::primitives::BUILT_INS.iter().enumerate() {
            behaviors.push(BehaviorDef {
                id: BehaviorId(i as u16),
                name: builtin.behavior.type_name().into(),
                caps: animatix_std::caps_from_info(builtin.info),
                source: BehaviorSource::Family(builtin.family),
            });
        }
        Self { behaviors }
    }

    /// Register a runtime extension primitive with its identity card.
    ///
    /// If an extension with the same type name was already registered, returns its existing ID.
    pub fn register_extension(
        &mut self,
        primitive: Arc<dyn crate::primitives::Primitive>,
        info: animatix_std::PrimitiveInfo,
    ) -> BehaviorId {
        let name = primitive.type_name();
        if let Some(existing) = self.find(name) {
            return existing.id;
        }
        let id = BehaviorId(self.behaviors.len() as u16);
        let caps = animatix_std::caps_from_info(&info);
        self.behaviors.push(BehaviorDef {
            id,
            name: name.into(),
            caps,
            source: BehaviorSource::Extension(primitive),
        });
        id
    }

    /// Look up a behavior definition by ID in O(1) time.
    #[inline(always)]
    pub fn get(&self, id: BehaviorId) -> Option<&BehaviorDef> {
        self.behaviors.get(id.index())
    }

    /// Look up a behavior definition by type name.
    pub fn find(&self, name: &str) -> Option<&BehaviorDef> {
        self.behaviors.iter().find(|b| &*b.name == name)
    }

    /// Fast lookup of a built-in behavior ID by type name without searching.
    #[inline(always)]
    pub fn behavior_id_for_builtin(name: &str) -> Option<BehaviorId> {
        behavior_id_for_builtin(name)
    }

    /// Number of registered behavior definitions.
    #[inline(always)]
    pub fn len(&self) -> usize {
        self.behaviors.len()
    }

    /// True when no behaviors are registered.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.behaviors.is_empty()
    }

    /// Iterate all behavior definitions in registration order.
    pub fn iter(&self) -> impl Iterator<Item = &BehaviorDef> {
        self.behaviors.iter()
    }
}
