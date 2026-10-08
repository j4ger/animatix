//! Built-in effect definitions. Each file is the single source for one
//! effect: parameters (name, kind, identity), WGSL passes, `pack`, and
//! `support`.

mod bloom;
mod blur;
mod chromatic_aberration;
mod color_grade;
mod drop_shadow;
mod duotone;
mod edge;
mod grain;
mod lens_distortion;
mod levels;
mod motion_blur;
mod posterize;
mod sharpen;
mod vignette;

pub use bloom::{BLOOM, Bloom};
pub use blur::{BLUR, Blur};
pub use chromatic_aberration::{CHROMATIC_ABERRATION, ChromaticAberration};
pub use color_grade::{COLOR_GRADE, ColorGrade, compose_color_matrix};
pub use drop_shadow::{DROP_SHADOW, DropShadow};
pub use duotone::{DUOTONE, Duotone};
pub use edge::{EDGE, Edge};
pub use grain::{GRAIN, Grain};
pub use lens_distortion::{LENS_DISTORTION, LensDistortion};
pub use levels::{LEVELS, Levels};
pub use motion_blur::{MOTION_BLUR, MotionBlur};
pub use posterize::{POSTERIZE, Posterize};
pub use sharpen::{SHARPEN, Sharpen};
pub use vignette::{VIGNETTE, Vignette};

#[cfg(test)]
mod tests {
    use animatix_core::effect::EffectParamSpec;

    use super::*;

    /// Every parameter the built-in effects author, with the effect's name on
    /// it. Adding an effect file without adding its table here is a test
    /// failure on purpose: this is the only place the whole set is enumerable,
    /// and it is what the raster-scaling guard below reads.
    fn all_params() -> Vec<(&'static str, &'static [EffectParamSpec])> {
        vec![
            ("Bloom", bloom::BLOOM_PARAMS),
            ("Blur", blur::BLUR_PARAMS),
            ("ChromaticAberration", chromatic_aberration::CHROMATIC_ABERRATION_PARAMS),
            ("ColorGrade", color_grade::COLOR_GRADE_PARAMS),
            ("DropShadow", drop_shadow::DROP_SHADOW_PARAMS),
            ("Duotone", duotone::DUOTONE_PARAMS),
            ("Edge", edge::EDGE_PARAMS),
            ("Grain", grain::GRAIN_PARAMS),
            ("LensDistortion", lens_distortion::LENS_DISTORTION_PARAMS),
            ("Levels", levels::LEVELS_PARAMS),
            ("MotionBlur", motion_blur::MOTION_BLUR_PARAMS),
            ("Posterize", posterize::POSTERIZE_PARAMS),
            ("Sharpen", sharpen::SHARPEN_PARAMS),
            ("Vignette", vignette::VIGNETTE_PARAMS),
        ]
    }

    /// A parameter measured in scene pixels has to shrink when the chain runs
    /// below scene resolution, or the effect visually grows as the frame
    /// shrinks. The renderer scales whatever is marked, so the mark has to be
    /// deliberate — this pins the set, and the WGSL of each entry is why it is
    /// on the list (`params.radius` / `params.offset` stepping source pixels).
    #[test]
    fn pixel_unit_parameters_are_exactly_the_known_six() {
        let mut marked: Vec<String> = all_params()
            .iter()
            .flat_map(|(effect, params)| {
                params
                    .iter()
                    .filter(|p| p.is_pixel())
                    .map(move |p| format!("{effect}.{}", p.name))
            })
            .collect();
        marked.sort();
        assert_eq!(
            marked,
            vec![
                "Blur.radius",
                "ChromaticAberration.offset",
                "DropShadow.offset",
                "DropShadow.softness",
                "MotionBlur.length",
                "Sharpen.radius",
            ]
        );
    }

    /// `Vignette.radius` and `LensDistortion.amount` are fractions of the
    /// frame, so marking them by their name would be wrong — they must stay
    /// untouched when the raster shrinks.
    #[test]
    fn normalized_lookalikes_are_not_marked_as_pixels() {
        for (effect, name) in [
            ("Vignette", "radius"),
            ("LensDistortion", "amount"),
            ("Grain", "seed"),
        ] {
            let params = all_params()
                .into_iter()
                .find(|(e, _)| *e == effect)
                .unwrap_or_else(|| panic!("{effect} is not in the table list"))
                .1;
            let spec = params
                .iter()
                .find(|p| p.name == name)
                .unwrap_or_else(|| panic!("{effect}.{name} is gone"));
            assert!(!spec.is_pixel(), "{effect}.{name} is a fraction, not scene pixels");
        }
    }
}
