//! Motion tokens — duration and easing primitives.

/// Duration constants for egui animations (seconds).
pub const INSTANT: f32 = 0.0;
pub const FAST: f32 = 0.10;
pub const NORMAL: f32 = 0.20;
pub const SLOW: f32 = 0.40;

/// Representation of a cubic bezier easing curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

/// Standard easing (ease-in-out).
pub const STANDARD: CubicBezier = CubicBezier {
    x1: 0.4,
    y1: 0.0,
    x2: 0.2,
    y2: 1.0,
};

/// Decelerate (ease-out).
pub const DECELERATE: CubicBezier = CubicBezier {
    x1: 0.0,
    y1: 0.0,
    x2: 0.2,
    y2: 1.0,
};

/// Accelerate (ease-in).
pub const ACCELERATE: CubicBezier = CubicBezier {
    x1: 0.4,
    y1: 0.0,
    x2: 1.0,
    y2: 1.0,
};

/// Spring with slight overshoot.
pub const SPRING_OVERSHOOT: CubicBezier = CubicBezier {
    x1: 0.34,
    y1: 1.56,
    x2: 0.64,
    y2: 1.0,
};
