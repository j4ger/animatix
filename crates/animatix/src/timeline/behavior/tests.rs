use std::collections::HashSet;
use std::sync::Arc;

use super::*;
use crate::ast::{InlineItem, Modifier, Property};
use crate::diagnostics::Diagnostic;
use crate::primitives::{BUILT_INS, BuildCtx, Primitive};

#[test]
fn all_builtins_have_distinct_behavior_ids_names_and_caps() {
    let registry = BehaviorRegistry::new();
    assert_eq!(registry.len(), BUILT_INS.len());

    let mut seen_ids = HashSet::new();
    let mut seen_names = HashSet::new();

    for (i, builtin) in BUILT_INS.iter().enumerate() {
        let expected_id = BehaviorId(i as u16);
        let name = builtin.behavior.type_name();

        let def = registry
            .get(expected_id)
            .unwrap_or_else(|| panic!("missing BehaviorDef for ID {expected_id:?} ({name})"));

        assert_eq!(def.id, expected_id);
        assert_eq!(def.name.as_ref(), name);
        assert!(matches!(def.source, BehaviorSource::Family(_)));

        let expected_caps = animatix_std::caps_from_info(builtin.info);
        assert_eq!(def.caps, expected_caps, "caps mismatch for primitive '{name}'");

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
    }

    assert_eq!(seen_ids.len(), BUILT_INS.len());
    assert_eq!(seen_names.len(), BUILT_INS.len());
}

/// Regression guard for the single registration table: every `BUILT_INS` row
/// must produce a `Family` behavior, and every catalog primitive must have a
/// row. A primitive added to the catalog without a row fails here instead of
/// panicking while a `BehaviorRegistry` is constructed at runtime.
#[test]
fn every_builtin_row_builds_a_family_behavior_def() {
    let registry = BehaviorRegistry::new();
    assert_eq!(
        registry.len(),
        BUILT_INS.len(),
        "BehaviorRegistry::new must create one def per BUILT_INS row"
    );
    assert_eq!(
        registry.len(),
        animatix_std::CATALOG.len(),
        "catalog primitive missing a BUILT_INS row"
    );

    let catalog_names: HashSet<&str> =
        animatix_std::CATALOG.iter().map(|info| &*info.type_name).collect();
    let row_names: HashSet<&str> =
        BUILT_INS.iter().map(|builtin| builtin.behavior.type_name()).collect();
    assert_eq!(catalog_names, row_names, "catalog and BUILT_INS rows drifted apart");

    for (i, builtin) in BUILT_INS.iter().enumerate() {
        let def = registry
            .get(BehaviorId(i as u16))
            .unwrap_or_else(|| panic!("missing BehaviorDef for BUILT_INS row {i}"));
        assert!(
            matches!(def.source, BehaviorSource::Family(_)),
            "built-in '{}' registered as Extension instead of Family",
            def.name
        );
        assert_eq!(def.name.as_ref(), &*builtin.info.type_name);
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

    let first_extension_id = BehaviorId(BUILT_INS.len() as u16);
    let id = registry.register_extension(ext.clone(), info.clone());
    assert_eq!(id, first_extension_id);
    assert_eq!(registry.len(), BUILT_INS.len() + 1);

    let def = registry.get(id).expect("extension def exists");
    assert_eq!(def.id, first_extension_id);
    assert_eq!(def.name.as_ref(), "MockExt");
    assert_eq!(def.extension_primitive().unwrap().type_name(), "MockExt");

    let found = registry.find("MockExt").expect("find finds extension");
    assert_eq!(found.id, first_extension_id);

    // Re-registering returns the existing id
    let id2 = registry.register_extension(ext, info);
    assert_eq!(id2, first_extension_id);
    assert_eq!(registry.len(), BUILT_INS.len() + 1);
}

#[test]
fn unknown_behavior_id_and_missing_builtin() {
    let registry = BehaviorRegistry::new();
    assert!(!BehaviorId::UNKNOWN.is_valid());
    assert!(registry.get(BehaviorId::UNKNOWN).is_none());
    assert_eq!(behavior_id_for_builtin("NonExistent"), None);
    assert!(registry.find("NonExistent").is_none());
}

#[test]
fn all_builtins_have_family_behavior_and_expected_build_extra() {
    let registry = BehaviorRegistry::new();
    let has_build_extra = [
        "Text",
        "Code",
        "Math",
        "Typst",
        "Counter",
        "Image",
        "Svg",
        "Audio",
        "Graph",
        "PlotCurve",
        "VectorField",
        "Heatmap",
        "ContourSet",
        "NumberPlane",
        "BarChart",
        "Legend",
        "Fragment",
        "Cue",
    ];

    for def in registry.iter() {
        match &def.source {
            BehaviorSource::Family(fam) => {
                let name = def.name.as_ref();
                if has_build_extra.contains(&name) {
                    assert!(fam.build_extra.is_some(), "builtin '{name}' must have build_extra");
                } else {
                    assert!(
                        fam.build_extra.is_none(),
                        "builtin '{name}' should not have build_extra"
                    );
                }
            },
            BehaviorSource::Extension(_) => {
                panic!("built-in '{}' registered as Extension instead of Family", def.name);
            },
        }
    }
}
