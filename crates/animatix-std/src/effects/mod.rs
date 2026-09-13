//! Built-in effect definitions. Each file is the single source for one
//! effect: parameters (name, kind, identity), WGSL passes, `pack`, and
//! `support`.

mod blur;
mod chromatic_aberration;
mod color_grade;

pub use blur::BLUR;
pub use chromatic_aberration::CHROMATIC_ABERRATION;
pub use color_grade::{COLOR_GRADE, compose_color_matrix};
