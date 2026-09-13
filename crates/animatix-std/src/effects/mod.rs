//! Built-in effect definitions. Each file is the single source for one
//! effect: parameters (name, kind, identity), WGSL passes, `pack`, and
//! `support`.

mod blur;
mod chromatic_aberration;
mod color_grade;
mod grain;
mod levels;
mod motion_blur;
mod sharpen;
mod vignette;

pub use blur::{BLUR, Blur};
pub use chromatic_aberration::{CHROMATIC_ABERRATION, ChromaticAberration};
pub use color_grade::{COLOR_GRADE, ColorGrade, compose_color_matrix};
pub use grain::{GRAIN, Grain};
pub use levels::{LEVELS, Levels};
pub use motion_blur::{MOTION_BLUR, MotionBlur};
pub use sharpen::{SHARPEN, Sharpen};
pub use vignette::{VIGNETTE, Vignette};
