//! Fragment-stage variants for render-only GPU backends.
//!
//! The regular `Effect::passes` remain compute WGSL for native/WebGPU. These
//! modules implement the same schema and straight-alpha math for WebGL2's
//! full-screen render-pass path. They are deliberately separate from plugin
//! descriptors: the native plugin ABI continues to accept compute WGSL only.

macro_rules! portable_shader {
    ($name:ident, $source:literal) => {
        pub(super) const $name: &str = concat!(
            include_str!("effect_context.wgsl"),
            include_str!("fullscreen_vertex.wgsl"),
            include_str!($source),
        );
    };
}

portable_shader!(BLOOM_FRAGMENT_WGSL, "portable/bloom.wgsl");
portable_shader!(BLUR_FRAGMENT_WGSL, "portable/blur.wgsl");
portable_shader!(CHROMATIC_ABERRATION_FRAGMENT_WGSL, "portable/chromatic_aberration.wgsl");
portable_shader!(COLOR_GRADE_FRAGMENT_WGSL, "portable/color_grade.wgsl");
portable_shader!(DROP_SHADOW_FRAGMENT_WGSL, "portable/drop_shadow.wgsl");
portable_shader!(DUOTONE_FRAGMENT_WGSL, "portable/duotone.wgsl");
portable_shader!(EDGE_FRAGMENT_WGSL, "portable/edge.wgsl");
portable_shader!(GRAIN_FRAGMENT_WGSL, "portable/grain.wgsl");
portable_shader!(LENS_DISTORTION_FRAGMENT_WGSL, "portable/lens_distortion.wgsl");
portable_shader!(LEVELS_FRAGMENT_WGSL, "portable/levels.wgsl");
portable_shader!(MOTION_BLUR_FRAGMENT_WGSL, "portable/motion_blur.wgsl");
portable_shader!(POSTERIZE_FRAGMENT_WGSL, "portable/posterize.wgsl");
portable_shader!(SHARPEN_FRAGMENT_WGSL, "portable/sharpen.wgsl");
portable_shader!(VIGNETTE_FRAGMENT_WGSL, "portable/vignette.wgsl");

#[cfg(test)]
mod tests {
    use crate::builtins::{EFFECTS, effect};

    #[test]
    fn every_builtin_has_a_portable_fragment_shader() {
        assert_eq!(EFFECTS.len(), 14);
        for descriptor in EFFECTS {
            assert!(
                descriptor.fragment_shader().is_some(),
                "{} has no fragment shader",
                descriptor.type_name()
            );
            assert!(
                descriptor
                    .fragment_shader()
                    .is_some_and(|shader| shader.contains("fn fullscreen_vs")
                        && shader.contains("fn fragment_main")),
                "{} is missing the portable entry points",
                descriptor.type_name()
            );
        }

        let pixelate = crate::builtins::PluginEffectData {
            type_name: "PortableTestComputeOnly".into(),
            display_name: "Portable test".into(),
            params: vec![],
            passes: vec![],
            author_uniform_size: 16,
            support_px: 0.0,
        };
        let _ = crate::builtins::register_extension_effect(pixelate);
        let descriptor = effect(&crate::effect::EffectId::new("PortableTestComputeOnly"))
            .expect("test plugin registered");
        assert!(descriptor.fragment_shader().is_none());
        crate::builtins::unregister_extension_effect("PortableTestComputeOnly");
    }
}
