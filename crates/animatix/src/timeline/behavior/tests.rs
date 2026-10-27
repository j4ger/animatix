use std::collections::HashSet;
use std::sync::Arc;

use super::*;
use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BUILT_INS, BuildCtx, Primitive};

#[test]
fn all_36_builtins_have_distinct_behavior_ids_names_families_and_caps() {
    let registry = BehaviorRegistry::new();
    assert_eq!(registry.len(), 36, "registry must hold exactly 36 built-ins");
    assert_eq!(registry.len(), BUILT_INS.len());

    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();
    let mut seen_families = HashSet::new();

    for (i, builtin) in BUILT_INS.iter().enumerate() {
        let expected_id = BehaviorId(i as u16);
        let name = builtin.behavior.type_name();

        let def = registry
            .get(expected_id)
            .unwrap_or_else(|| panic!("missing BehaviorDef for ID {expected_id:?} ({name})"));

        assert_eq!(def.id, expected_id);
        assert_eq!(def.name.as_ref(), name);
        assert_eq!(def.primitive().type_name(), name);

        let expected_caps = animatix_std::caps_from_info(builtin.info);
        assert_eq!(def.caps, expected_caps, "caps mismatch for primitive '{name}'");

        let expected_family = BehaviorFamily::from_caps(&expected_caps);
        assert_eq!(def.family, expected_family, "family mismatch for primitive '{name}'");
        assert_eq!(def.child_processing, expected_caps.child_processing);

        assert_eq!(
            behavior_id_for_builtin(name),
            Some(expected_id),
            "fast builtin lookup mismatch for '{name}'"
        );

        assert_eq!(
            registry.find(name).map(|d| d.id),
            Some(expected_id),
            "registry.find mismatch for '{name}'"
        );

        assert!(seen_ids.insert(def.id), "duplicate BehaviorId: {:?}", def.id);
        assert!(seen_names.insert(def.name.clone()), "duplicate name: {}", def.name);
        seen_families.insert(def.family);
    }

    assert_eq!(seen_ids.len(), 36);
    assert_eq!(seen_names.len(), 36);

    // All 10 families must be represented across the 36 built-ins.
    let all_families = [
        BehaviorFamily::Shape,
        BehaviorFamily::Text,
        BehaviorFamily::Media,
        BehaviorFamily::Plot,
        BehaviorFamily::Container,
        BehaviorFamily::Effect,
        BehaviorFamily::Equation,
        BehaviorFamily::Captions,
        BehaviorFamily::Annotation,
        BehaviorFamily::Group,
    ];
    for family in all_families {
        assert!(
            seen_families.contains(&family),
            "BehaviorFamily::{family:?} not represented among built-ins"
        );
    }
}

struct MockExtension;
impl Primitive for MockExtension {
    fn type_name(&self) -> &str {
        "MockExt"
    }
    fn build(
        &self,
        _ctx: &mut BuildCtx,
        _label: &str,
        _props: &[Property],
        _modifiers: &[Modifier],
        _children: &[InlineItem],
    ) -> Result<(), Vec<Diagnostic>> {
        Ok(())
    }
}

#[test]
fn register_extension_assigns_ids_and_retrieves_def() {
    let mut registry = BehaviorRegistry::new();
    let ext = Arc::new(MockExtension);
    let info = animatix_std::PrimitiveInfo::extension(
        "MockExt",
        animatix_core::caps::ActorCategory::Shape,
    );

    let id = registry.register_extension(ext.clone(), info.clone());
    assert_eq!(id, BehaviorId(36));
    assert_eq!(registry.len(), 37);

    let def = registry.get(id).expect("extension def exists");
    assert_eq!(def.id, BehaviorId(36));
    assert_eq!(def.name.as_ref(), "MockExt");
    assert_eq!(def.family, BehaviorFamily::Shape);
    assert_eq!(def.primitive().type_name(), "MockExt");

    let found = registry.find("MockExt").expect("find finds extension");
    assert_eq!(found.id, BehaviorId(36));

    // Re-registering returns the existing id
    let id2 = registry.register_extension(ext, info);
    assert_eq!(id2, BehaviorId(36));
    assert_eq!(registry.len(), 37);
}

#[test]
fn unknown_behavior_id_and_missing_builtin() {
    let registry = BehaviorRegistry::new();
    assert!(!BehaviorId::UNKNOWN.is_valid());
    assert!(registry.get(BehaviorId::UNKNOWN).is_none());
    assert_eq!(behavior_id_for_builtin("NonExistent"), None);
    assert!(registry.find("NonExistent").is_none());
}
