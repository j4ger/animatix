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

impl CubicBezier {
    /// Sample the curve at normalized time `t` ∈ [0, 1] using Newton-Raphson.
    /// Returns the eased progress value (may overshoot >1 or <0 for spring curves).
    pub fn sample(self, t: f32) -> f32 {
        let t = t.clamp(0.0, 1.0);

        // Fast path: linear curve (0,0,1,1)
        if self.x1 == 0.0 && self.y1 == 0.0 && self.x2 == 1.0 && self.y2 == 1.0 {
            return t;
        }

        // Newton-Raphson: find s such that x(s) = t, then return y(s)
        let mut s = t; // initial guess
        for _ in 0..6 {
            let x = cubic_bezier_component(s, self.x1, self.x2);
            let dx = cubic_bezier_derivative(s, self.x1, self.x2);
            if dx.abs() < 1e-7 {
                break;
            }
            s = s - (x - t) / dx;
            s = s.clamp(0.0, 1.0);
        }

        // Binary search refinement (safe for extreme control points)
        let mut lo = 0.0_f32;
        let mut hi = 1.0_f32;
        for _ in 0..8 {
            let mid = (lo + hi) / 2.0;
            let x = cubic_bezier_component(mid, self.x1, self.x2);
            if (x - t).abs() < 1e-6 {
                return cubic_bezier_component(mid, self.y1, self.y2);
            }
            if x < t {
                lo = mid;
            } else {
                hi = mid;
            }
        }

        cubic_bezier_component(s, self.y1, self.y2)
    }
}

fn cubic_bezier_component(s: f32, c1: f32, c2: f32) -> f32 {
    let s1 = 1.0 - s;
    3.0 * s1 * s1 * s * c1 + 3.0 * s1 * s * s * c2 + s * s * s
}

fn cubic_bezier_derivative(s: f32, c1: f32, c2: f32) -> f32 {
    let s1 = 1.0 - s;
    3.0 * s1 * s1 * c1 + 6.0 * s1 * s * (c2 - c1) + 3.0 * s * s * (1.0 - c2)
}

/// A named transition combining duration and easing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transition {
    pub duration: f32,
    pub easing: CubicBezier,
}

/// Predefined transitions for common interaction patterns.
pub const HOVER: Transition = Transition {
    duration: FAST,
    easing: STANDARD,
};
pub const PANEL: Transition = Transition {
    duration: NORMAL,
    easing: STANDARD,
};
pub const MODAL: Transition = Transition {
    duration: SLOW,
    easing: DECELERATE,
};
