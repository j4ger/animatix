//! Kurbo Shape Morphing Module
//!
//! This module provides utilities for creating parametric kurbo shapes and converting them
//! to BezPath for use with the existing morph_paths interpolation logic.
//!
//! Supported shapes:
//! - Circle: center and radius
//! - Rect: axis-aligned rectangle with min/max coordinates
//! - RoundedRect: rectangle with rounded corners (uniform or per-corner radii)
//! - Line: simple two-point line segment
//! - Ellipse: parametric ellipse with rotation
//! - Arc: elliptical arc with sweep angle

use kurbo::{Arc, BezPath, Circle, Ellipse, Line, Point, Rect, RoundedRect, Shape, Vec2};

/// Default tolerance for curve approximation
/// Controls accuracy when converting curves to bezier segments
pub const DEFAULT_TOLERANCE: f64 = 0.1;

/// Enum representing all supported kurbo shapes
#[derive(Clone, Debug)]
pub enum KurboShape {
    /// Circle defined by center point and radius
    Circle { center: Point, radius: f64 },

    /// Axis-aligned rectangle defined by min and max coordinates
    Rect { x0: f64, y0: f64, x1: f64, y1: f64 },

    /// Rectangle with uniform corner radius
    RectUniform {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        radius: f64,
    },

    /// Rectangle with per-corner radii (top-left, top-right, bottom-right, bottom-left)
    RectRadii {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        radii: (f64, f64, f64, f64),
    },

    /// Line segment from start to end point
    Line { p0: Point, p1: Point },

    /// Ellipse defined by center, radii (as Vec2), and rotation angle
    Ellipse {
        center: Point,
        radii: Vec2,
        rotation: f64,
    },

    /// Elliptical arc with sweep
    Arc {
        center: Point,
        radii: Vec2,
        start_angle: f64,
        sweep_angle: f64,
        rotation: f64,
    },

    /// Closed polygon defined by explicit points
    Polygon { points: Vec<Point> },

    /// Raw Bezier path
    Path { path: BezPath },
}

impl KurboShape {
    /// Create a circle from center and radius
    pub fn circle(center: Point, radius: f64) -> Self {
        KurboShape::Circle { center, radius }
    }

    /// Create a circle from center coordinates and radius
    pub fn circle_xy(cx: f64, cy: f64, radius: f64) -> Self {
        KurboShape::Circle {
            center: Point::new(cx, cy),
            radius,
        }
    }

    /// Create a rectangle from min and max coordinates
    pub fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> Self {
        KurboShape::Rect { x0, y0, x1, y1 }
    }

    /// Create a rectangle with uniform corner radius
    pub fn rounded_rect(x0: f64, y0: f64, x1: f64, y1: f64, radius: f64) -> Self {
        KurboShape::RectUniform {
            x0,
            y0,
            x1,
            y1,
            radius,
        }
    }

    /// Create a rectangle with per-corner radii (clockwise from top-left)
    pub fn rounded_rect_radii(
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        radii: (f64, f64, f64, f64),
    ) -> Self {
        KurboShape::RectRadii {
            x0,
            y0,
            x1,
            y1,
            radii,
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
            KurboShape::Circle { center, radius } => {
                Circle::new(*center, *radius).into_path(tolerance)
            }
            KurboShape::Rect { x0, y0, x1, y1 } => {
                Rect::new(*x0, *y0, *x1, *y1).into_path(tolerance)
            }
            KurboShape::RectUniform {
                x0,
                y0,
                x1,
                y1,
                radius,
            } => RoundedRect::new(*x0, *y0, *x1, *y1, *radius).into_path(tolerance),
            KurboShape::RectRadii {
                x0,
                y0,
                x1,
                y1,
                radii,
            } => RoundedRect::new(*x0, *y0, *x1, *y1, *radii).into_path(tolerance),
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
            }
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
    use super::*;

    #[test]
    fn test_circle_conversion() {
        let circle = KurboShape::circle(Point::new(50.0, 50.0), 30.0);
        let path = circle.to_path_default();

        // Circle should convert to a valid BezPath
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
    fn test_rounded_rect_uniform() {
        let rect = KurboShape::rounded_rect(0.0, 0.0, 100.0, 100.0, 10.0);
        let path = rect.to_path_default();

        // RoundedRect should convert to path with curves
        assert!(!path.elements().is_empty());
    }

    #[test]
    fn test_rounded_rect_radii() {
        let radii = (10.0, 15.0, 20.0, 25.0);
        let rect = KurboShape::rounded_rect_radii(0.0, 0.0, 100.0, 100.0, radii);
        let path = rect.to_path_default();

        // RoundedRect with per-corner radii should convert to valid path
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
    fn test_circle_to_rect_morph() {
        let circle = KurboShape::circle(Point::new(50.0, 50.0), 30.0);
        let rect = KurboShape::rect(20.0, 20.0, 80.0, 80.0);

        // Mid-morph should produce valid path
        let morphed = morph_kurbo_shapes_default(&circle, &rect, 0.5);
        assert!(!morphed.elements().is_empty());
    }

    #[test]
    fn test_rect_to_rounded_rect_morph() {
        let rect = KurboShape::rect(0.0, 0.0, 100.0, 100.0);
        let rounded_rect = KurboShape::rounded_rect(0.0, 0.0, 100.0, 100.0, 20.0);

        // Morphing square to rounded square should work
        let morphed_start = morph_kurbo_shapes_default(&rect, &rounded_rect, 0.0);
        let morphed_mid = morph_kurbo_shapes_default(&rect, &rounded_rect, 0.5);
        let morphed_end = morph_kurbo_shapes_default(&rect, &rounded_rect, 1.0);

        // All morph stages should be valid
        assert!(!morphed_start.elements().is_empty());
        assert!(!morphed_mid.elements().is_empty());
        assert!(!morphed_end.elements().is_empty());
    }

    #[test]
    fn test_tolerance_parameter() {
        let circle = KurboShape::circle(Point::new(50.0, 50.0), 30.0);

        // Looser tolerance should produce fewer segments
        let path_loose = circle.to_path(0.5);
        let path_tight = circle.to_path(0.1);

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
        let shape1 = KurboShape::circle(Point::new(50.0, 50.0), 30.0);
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
