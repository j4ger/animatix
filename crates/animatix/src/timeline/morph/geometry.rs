//! Path bounds and unit-normalisation helpers.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct PathBounds {
    pub(super) min_x: f64,
    pub(super) max_x: f64,
    pub(super) min_y: f64,
    pub(super) max_y: f64,
}

impl PathBounds {
    pub(super) fn width(self) -> f64 {
        (self.max_x - self.min_x).max(1.0)
    }

    fn height(self) -> f64 {
        (self.max_y - self.min_y).max(1.0)
    }
}

pub(super) fn path_bounds(path: &BezPath) -> PathBounds {
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;

    for element in path.elements() {
        match element {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => {
                update_bounds(*p, &mut min_x, &mut max_x, &mut min_y, &mut max_y)
            },
            PathEl::QuadTo(p1, p2) => {
                update_bounds(*p1, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                update_bounds(*p2, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
            },
            PathEl::CurveTo(p1, p2, p3) => {
                update_bounds(*p1, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                update_bounds(*p2, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
                update_bounds(*p3, &mut min_x, &mut max_x, &mut min_y, &mut max_y);
            },
            PathEl::ClosePath => {},
        }
    }

    if !min_x.is_finite() {
        return PathBounds {
            min_x: 0.0,
            max_x: 1.0,
            min_y: 0.0,
            max_y: 1.0,
        };
    }

    PathBounds {
        min_x,
        max_x,
        min_y,
        max_y,
    }
}

pub(super) fn update_bounds(
    point: Point,
    min_x: &mut f64,
    max_x: &mut f64,
    min_y: &mut f64,
    max_y: &mut f64,
) {
    *min_x = (*min_x).min(point.x);
    *max_x = (*max_x).max(point.x);
    *min_y = (*min_y).min(point.y);
    *max_y = (*max_y).max(point.y);
}

pub(super) fn normalize_path_to_unit_bounds(path: &BezPath, bounds: PathBounds) -> BezPath {
    map_path_points(path, |point| {
        Point::new(
            (point.x - bounds.min_x) / bounds.width(),
            (point.y - bounds.min_y) / bounds.height(),
        )
    })
}

pub(super) fn denormalize_path_from_unit_bounds(path: &BezPath, bounds: PathBounds) -> BezPath {
    map_path_points(path, |point| {
        Point::new(
            bounds.min_x + point.x * bounds.width(),
            bounds.min_y + point.y * bounds.height(),
        )
    })
}

pub(super) fn map_path_points(
    path: &BezPath,
    mut map_point: impl FnMut(Point) -> Point,
) -> BezPath {
    let mut mapped = BezPath::new();

    for element in path.elements() {
        match element {
            PathEl::MoveTo(point) => mapped.push(PathEl::MoveTo(map_point(*point))),
            PathEl::LineTo(point) => mapped.push(PathEl::LineTo(map_point(*point))),
            PathEl::QuadTo(p1, p2) => mapped.push(PathEl::QuadTo(map_point(*p1), map_point(*p2))),
            PathEl::CurveTo(p1, p2, p3) => {
                mapped.push(PathEl::CurveTo(map_point(*p1), map_point(*p2), map_point(*p3)))
            },
            PathEl::ClosePath => mapped.push(PathEl::ClosePath),
        }
    }

    mapped
}

pub(super) fn interpolate_bounds(source: PathBounds, target: PathBounds, t: f64) -> PathBounds {
    PathBounds {
        min_x: source.min_x + (target.min_x - source.min_x) * t,
        max_x: source.max_x + (target.max_x - source.max_x) * t,
        min_y: source.min_y + (target.min_y - source.min_y) * t,
        max_y: source.max_y + (target.max_y - source.max_y) * t,
    }
}
