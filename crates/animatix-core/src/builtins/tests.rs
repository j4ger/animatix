use crate::effect::{
    EffectParamKind, EffectParamSpec, EffectParamValue, EffectPassSpec, uniform_size_for,
};

use super::*;

/// Declared parameters follow the host layout rule (each offset aligned to
/// its kind: scalars 4, vec2 8, vec4 16; sequential with alignment gaps),
/// so packers can rely on the offsets and the derived uniform size fills a
/// valid buffer.
#[test]
fn declared_param_layout_follows_host_rule() {
    for effect in EFFECTS {
        let mut offset = 0u32;
        for spec in effect.params() {
            let (size, align) = crate::effect::kind_layout(spec.kind);
            offset = offset.next_multiple_of(align);
            assert_eq!(spec.offset, offset, "{}.{} offset", effect.type_name(), spec.name);
            assert_eq!(spec.size, size, "{}.{} size", effect.type_name(), spec.name);
            offset += size;
        }
        assert_eq!(effect.author_uniform_size() % 16, 0);
        // The uniform buffer must cover every declared parameter; effects
        // with hand-padded WGSL structs (ColorGrade's 4×4 matrix) may be
        // larger than the parameter layout.
        assert!(
            effect.author_uniform_size() >= uniform_size_for(effect.params()),
            "{} uniform size under-covers its parameters",
            effect.type_name()
        );
    }
}

#[test]
fn every_effect_is_addressable_by_id_and_type() {
    for declared in EFFECTS {
        let id = crate::effect::EffectId::new(declared.type_name());
        assert_eq!(
            effect(&id).map(|effect| effect.type_name().to_owned()).as_deref(),
            Some(declared.type_name()),
            "{} is not reachable by id",
            declared.type_name()
        );
        assert_eq!(
            effect_for_type(declared.type_name())
                .map(|effect| effect.type_name().to_owned())
                .as_deref(),
            Some(declared.type_name()),
            "{} is not reachable by type name",
            declared.type_name()
        );
    }
}

/// The persisted identity is the authored name, so a serialized stage kind
/// round-trips through serde as a bare string — including for plugin
/// effects, which have no host-assigned slot to drift between runs.
#[test]
#[cfg(feature = "serde")]
fn effect_id_serializes_as_the_authored_name() {
    let id = crate::effect::EffectId::new("Blur");
    let json = serde_json::to_string(&id).expect("serialize EffectId");
    assert_eq!(json, "\"Blur\"");
    let round: crate::effect::EffectId = serde_json::from_str(&json).expect("deserialize EffectId");
    assert_eq!(round, id);
}

#[test]
fn built_in_type_names_are_unique() {
    let mut seen = std::collections::HashSet::new();
    for effect in EFFECTS {
        assert!(seen.insert(effect.type_name()), "duplicate type name {}", effect.type_name());
    }
}

#[test]
fn plugin_effect_registers_by_name_and_rejects_builtin_collision() {
    let params = vec![EffectParamSpec::new(
        "size",
        EffectParamKind::F32,
        EffectParamValue::F32(0.0),
        0,
        4,
    )];
    let passes = vec![EffectPassSpec::new(
        "std-test-pixelate",
        "@compute fn main() {}",
        "main",
    )];

    let registered = register_extension_effect(PluginEffectData {
        type_name: "StdTestPixelate".into(),
        display_name: "Std Test Pixelate".into(),
        params: params.clone(),
        passes: passes.clone(),
        author_uniform_size: 16,
        support_px: 0.0,
    })
    .expect("plugin effect registers");
    assert_eq!(&*registered, "StdTestPixelate");
    assert!(effect_for_type("StdTestPixelate").is_some());

    // A built-in name is rejected.
    assert!(
        register_extension_effect(PluginEffectData {
            type_name: "Blur".into(),
            display_name: "".into(),
            params,
            passes,
            author_uniform_size: 16,
            support_px: 0.0,
        })
        .is_none(),
        "built-in name collision must be rejected"
    );

    unregister_extension_effect("StdTestPixelate");
    assert!(effect_for_type("StdTestPixelate").is_none());
}

/// A plugin effect's data is owned by the registry: unregistering drops it,
/// so a register/unregister churn cannot leak (the old `Box::leak`
/// registration could never reclaim anything).
#[test]
fn unregister_reclaims_plugin_effect_data() {
    fn register(name: &str) {
        let _ = register_extension_effect(PluginEffectData {
            type_name: name.into(),
            display_name: name.into(),
            params: vec![],
            passes: vec![],
            author_uniform_size: 16,
            support_px: 0.0,
        });
    }

    for i in 0..64 {
        let name = format!("ChurnEffect{i}");
        register(&name);
        assert!(effect_for_type(&name).is_some());
        unregister_extension_effect(&name);
        assert!(effect_for_type(&name).is_none(), "{name} survived unregister");
    }
    assert!(plugin_effects().is_empty(), "churn left registered effects behind");
}
