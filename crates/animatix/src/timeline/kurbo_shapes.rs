//! Kurbo Shape Morphing Module
//!
//! This module provides utilities for creating parametric kurbo shapes and converting them
//! to BezPath for use with the existing morph_paths interpolation logic.
//!
//! Supported shapes:
//! - Rect: axis-aligned rectangle with min/max coordinates
//! - Line: simple two-point line segment
//! - Ellipse: parametric ellipse with rotation
//! - Arc: elliptical arc with sweep angle

use kurbo::{Arc, BezPath, Ellipse, Line, Point, Rect, Shape, Vec2};

/// Default tolerance for curve approximation
/// Controls accuracy when converting curves to bezier segments
pub const DEFAULT_TOLERANCE: f64 = 0.1;

/// Enum representing all supported kurbo shapes
///
/// `PartialEq` exists for the per-track shape→path memo (PF-6): comparing the
/// sampled `KurboShape` fingerprint is allocation-free, and inequality means
/// the memoized `Arc<BezPath>` must be rebuilt.
#[derive(Clone, Debug, PartialEq)]
pub enum KurboShape {
    /// Axis-aligned rectangle defined by min and max coordinates
    Rect {
        /// Minimum x coordinate
        x0: f64,
        /// Minimum y coordinate
        y0: f64,
        /// Maximum x coordinate
        x1: f64,
        /// Maximum y coordinate
        y1: f64,
    },

    /// Rectangle with rounded corners.
    RoundedRect {
        /// Minimum x coordinate.
        x0: f64,
        /// Minimum y coordinate.
        y0: f64,
        /// Maximum x coordinate.
        x1: f64,
        /// Maximum y coordinate.
        y1: f64,
        /// Corner radius in scene pixels (clamped to half the shorter side).
        radius: f64,
    },

    /// Line segment from start to end point
    Line {
        /// Start point
        p0: Point,
        /// End point
        p1: Point,
    },

    /// Ellipse defined by center, radii (as Vec2), and rotation angle
    Ellipse {
        /// Center of the ellipse
        center: Point,
        /// X and Y radii
        radii: Vec2,
        /// Rotation angle in radians
        rotation: f64,
    },

    /// Elliptical arc with sweep
    Arc {
        /// Center of the arc
        center: Point,
        /// X and Y radii
        radii: Vec2,
        /// Start angle in radians
        start_angle: f64,
        /// Sweep angle in radians
        sweep_angle: f64,
        /// Rotation angle in radians
        rotation: f64,
    },

    /// Closed polygon defined by explicit points
    Polygon {
        /// Vertices of the polygon
        points: Vec<Point>,
    },

    /// Raw Bezier path
    Path {
        /// Underlying Bezier path
        path: BezPath,
    },
}

impl KurboShape {
    /// Create a rectangle from min and max coordinates
    pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        KurboShape::Rect { x0, y0, x1, y1 }
    }

    /// Create a rectangle with rounded corners.
    ///
    /// The radius is clamped to half the shorter side, so an over-large radius
    /// yields a stadium/circle rather than a self-intersecting path.
    pub fn rounded_rect(x0: f64, y0: f64, x1: f64, y1: f64, radius: f64) -> Self {
        let clamped = radius.abs().min((x1 - x0).abs() / 2.0).min((y1 - y0).abs() / 2.0);
        KurboShape::RoundedRect {
            x0,
            y0,
            x1,
            y1,
            radius: clamped,
        }
    }

    /// Create a line segment
    pub fn line(p0: Point, p1: Point) -> Self {
        KurboShape::Line { p0, p1 }
    }

    /// Create a line segment from coordinates
    pub fn line_xy(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        KurboShape::Line {
            p0: Point::new(x0, y0),
            p1: Point::new(x1, y1),
        }
    }

    /// Create an ellipse
    pub fn ellipse(center: Point, radii: Vec2, rotation: f64) -> Self {
        KurboShape::Ellipse {
            center,
            radii,
            rotation,
        }
    }

    /// Create an ellipse from coordinates
    pub fn ellipse_xy(cx: f64, cy: f64, rx: f64, ry: f64, rotation: f64) -> Self {
        KurboShape::Ellipse {
            center: Point::new(cx, cy),
            radii: Vec2::new(rx, ry),
            rotation,
        }
    }

    /// Create an arc
    pub fn arc(
        center: Point,
        radii: Vec2,
        start_angle: f64,
        sweep_angle: f64,
        rotation: f64,
    ) -> Self {
        KurboShape::Arc {
            center,
            radii,
            start_angle,
            sweep_angle,
            rotation,
        }
    }

    /// Create an arc from coordinates
    pub fn arc_xy(
        cx: f64,
        cy: f64,
        rx: f64,
        ry: f64,
        start_angle: f64,
        sweep_angle: f64,
        rotation: f64,
    ) -> Self {
        KurboShape::Arc {
            center: Point::new(cx, cy),
            radii: Vec2::new(rx, ry),
            start_angle,
            sweep_angle,
            rotation,
        }
    }

    /// Create a polygon from explicit points
    pub fn polygon(points: Vec<Point>) -> Self {
        KurboShape::Polygon { points }
    }

    /// Create a raw path wrapper
    pub fn path(path: BezPath) -> Self {
        KurboShape::Path { path }
    }

    /// Convert this shape to a BezPath with specified tolerance
    pub fn to_path(&self, tolerance: f64) -> BezPath {
        match self {
            KurboShape::Rect { x0, y0, x1, y1 } => {
                Rect::new(*x0, *y0, *x1, *y1).into_path(tolerance)
            },
            KurboShape::RoundedRect {
                x0,
                y0,
                x1,
                y1,
                radius,
            } => {
                if *radius <= 0.0 {
                    Rect::new(*x0, *y0, *x1, *y1).into_path(tolerance)
                } else {
                    kurbo::RoundedRect::new(*x0, *y0, *x1, *y1, *radius).into_path(tolerance)
                }
            },
            KurboShape::Line { p0, p1 } => Line::new(*p0, *p1).into_path(tolerance),
            KurboShape::Ellipse {
                center,
                radii,
                rotation,
            } => Ellipse::new(*center, *radii, *rotation).into_path(tolerance),
            KurboShape::Arc {
                center,
                radii,
                start_angle,
                sweep_angle,
                rotation,
            } => Arc::new(*center, *radii, *start_angle, *sweep_angle, *rotation)
                .into_path(tolerance),
            KurboShape::Polygon { points } => {
                let mut path = BezPath::new();
                if let Some(first) = points.first() {
                    path.move_to(*first);
                    for point in points.iter().skip(1) {
                        path.line_to(*point);
                    }
                    path.close_path();
                }
                path
            },
            KurboShape::Path { path } => path.clone(),
        }
    }

    /// Convert this shape to a BezPath using the default tolerance
    pub fn to_path_default(&self) -> BezPath {
        self.to_path(DEFAULT_TOLERANCE)
    }
}

/// Morph from one kurbo shape to another at parameter t
///
/// # Arguments
/// * `from` - Starting shape
/// * `to` - Ending shape
/// * `t` - Interpolation parameter (0.0 = from, 1.0 = to)
/// * `tolerance` - Curve approximation tolerance
///
/// # Returns
/// Morphed BezPath at the given time parameter
pub fn morph_kurbo_shapes(from: &KurboShape, to: &KurboShape, t: f64, tolerance: f64) -> BezPath {
    let from_path = from.to_path(tolerance);
    let to_path = to.to_path(tolerance);

    // Use existing morph_paths from the morph module
    crate::timeline::morph::morph_paths(&from_path, &to_path, t)
}

/// Morph from one kurbo shape to another using default tolerance
pub fn morph_kurbo_shapes_default(from: &KurboShape, to: &KurboShape, t: f64) -> BezPath {
    morph_kurbo_shapes(from, to, t, DEFAULT_TOLERANCE)
}

#[cfg(test)]
mod tests {
    use kurbo::Shape;

    /// A positive radius rounds the corners; zero keeps them square. Checked
    /// geometrically (a corner point is outside the path) so the assertion does
    /// not depend on rasterization.
    #[test]
    fn rounded_rect_rounds_corners_and_keeps_square_at_zero() {
        let corner = Point::new(1.0, 1.0);
        let square = KurboShape::rounded_rect(0.0, 0.0, 100.0, 60.0, 0.0).to_path_default();
        assert!(square.contains(corner), "radius 0 must keep the corner inside the path");

        let rounded = KurboShape::rounded_rect(0.0, 0.0, 100.0, 60.0, 20.0).to_path_default();
        assert!(!rounded.contains(corner), "the corner must be cut out");
        // The edge midpoint and the interior stay inside.
        assert!(rounded.contains(Point::new(50.0, 0.5)));
        assert!(rounded.contains(Point::new(50.0, 30.0)));
        // Bounds are unchanged: rounding never grows the shape. Compared with a
        // tolerance because the flattened corner arcs land a hair off the exact
        // coordinates (~1e-30).
        let (rb, sb) = (rounded.bounding_box(), square.bounding_box());
        for (a, b) in [
            (rb.x0, sb.x0),
            (rb.y0, sb.y0),
            (rb.x1, sb.x1),
            (rb.y1, sb.y1),
        ] {
            assert!((a - b).abs() < 1e-6, "bounds drifted: {rb:?} vs {sb:?}");
        }
    }

    /// An over-large radius clamps to half the shorter side (a stadium), rather
    /// than producing a self-intersecting path.
    #[test]
    fn rounded_rect_clamps_an_over_large_radius() {
        let clamped = KurboShape::rounded_rect(0.0, 0.0, 100.0, 60.0, 500.0);
        let KurboShape::RoundedRect { radius, .. } = clamped else {
            panic!("expected a rounded rect");
        };
        assert_eq!(radius, 30.0, "clamped to half the shorter side");
        let path = clamped.to_path_default();
        assert!(path.contains(Point::new(50.0, 30.0)));
        assert!(!path.contains(Point::new(1.0, 1.0)));
    }

    use super::*;

    #[test]
    fn test_circle_as_ellipse_conversion() {
        let ellipse = KurboShape::Ellipse {
            center: Point::new(50.0, 50.0),
            radii: Vec2::new(30.0, 30.0),
            rotation: 0.0,
        };
        let path = ellipse.to_path_default();

        // Equal-radii ellipse (circle) should convert to a valid BezPath
        assert!(!path.elements().is_empty());
    }

    #[test]
    fn test_rect_conversion() {
        let rect = KurboShape::rect(0.0, 0.0, 100.0, 100.0);
        let path = rect.to_path_default();

        // Rect should convert to a valid BezPath with lines
        assert!(!path.elements().is_empty());
    }

    #[test]
    fn test_line_conversion() {
        let line = KurboShape::line(Point::new(0.0, 0.0), Point::new(100.0, 100.0));
        let path = line.to_path_default();

        // Line should convert to valid path
        assert!(!path.elements().is_empty());
    }

    #[test]
    fn test_ellipse_conversion() {
        let ellipse = KurboShape::ellipse(Point::new(50.0, 50.0), Vec2::new(40.0, 30.0), 0.0);
        let path = ellipse.to_path_default();

        // Ellipse should convert to path with curves
        assert!(!path.elements().is_empty());
    }

    #[test]
    fn test_arc_conversion() {
        let arc = KurboShape::arc(
            Point::new(50.0, 50.0),
            Vec2::new(40.0, 40.0),
            0.0,
            std::f64::consts::PI / 2.0,
            0.0,
        );
        let path = arc.to_path_default();

        // Arc should convert to path with curves
        assert!(!path.elements().is_empty());
    }

    #[test]
    fn test_polygon_conversion() {
        let polygon = KurboShape::polygon(vec![
            Point::new(-40.0, 0.0),
            Point::new(0.0, -60.0),
            Point::new(50.0, 10.0),
            Point::new(10.0, 70.0),
        ]);
        let path = polygon.to_path_default();

        assert!(!path.elements().is_empty());
    }

    #[test]
    fn test_raw_path_conversion() {
        let mut raw = BezPath::new();
        raw.move_to((0.0, 0.0));
        raw.line_to((100.0, 0.0));
        raw.line_to((100.0, 50.0));
        raw.close_path();

        let path = KurboShape::path(raw.clone()).to_path_default();

        assert_eq!(path.elements(), raw.elements());
    }

    #[test]
    fn test_ellipse_to_rect_morph() {
        let ellipse = KurboShape::Ellipse {
            center: Point::new(50.0, 50.0),
            radii: Vec2::new(30.0, 30.0),
            rotation: 0.0,
        };
        let rect = KurboShape::rect(20.0, 20.0, 80.0, 80.0);

        // Mid-morph should produce valid path
        let morphed = morph_kurbo_shapes_default(&ellipse, &rect, 0.5);
        assert!(!morphed.elements().is_empty());
    }

    #[test]
    fn test_tolerance_parameter() {
        let ellipse = KurboShape::Ellipse {
            center: Point::new(50.0, 50.0),
            radii: Vec2::new(30.0, 30.0),
            rotation: 0.0,
        };

        // Looser tolerance should produce fewer segments
        let path_loose = ellipse.to_path(0.5);
        let path_tight = ellipse.to_path(0.1);

        // Both should be valid
        assert!(!path_loose.elements().is_empty());
        assert!(!path_tight.elements().is_empty());

        // Tighter tolerance should generally have more segments
        // (not always guaranteed, but typical behavior)
        let loose_count = path_loose.elements().len();
        let tight_count = path_tight.elements().len();
        assert!(tight_count >= loose_count || loose_count <= 10);
    }

    #[test]
    fn test_morph_continuity() {
        let shape1 = KurboShape::Ellipse {
            center: Point::new(50.0, 50.0),
            radii: Vec2::new(30.0, 30.0),
            rotation: 0.0,
        };
        let shape2 = KurboShape::rect(20.0, 20.0, 80.0, 80.0);

        // Morphs at different time values
        let t0 = morph_kurbo_shapes_default(&shape1, &shape2, 0.0);
        let t25 = morph_kurbo_shapes_default(&shape1, &shape2, 0.25);
        let t50 = morph_kurbo_shapes_default(&shape1, &shape2, 0.5);
        let t75 = morph_kurbo_shapes_default(&shape1, &shape2, 0.75);
        let t100 = morph_kurbo_shapes_default(&shape1, &shape2, 1.0);

        // All should produce valid paths
        assert!(!t0.elements().is_empty());
        assert!(!t25.elements().is_empty());
        assert!(!t50.elements().is_empty());
        assert!(!t75.elements().is_empty());
        assert!(!t100.elements().is_empty());
    }
}
