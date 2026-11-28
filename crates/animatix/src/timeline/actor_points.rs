//! Geometry queries for named points on an actor's layout box.

#[cfg(test)]
mod tests;
pub(crate) mod track;
use super::{
    EntityId, PositionBinding, SceneAnchor, SceneDimensions, Timeline, TrackAccessor, Value,
};
use crate::primitives::RenderFrame;
use kurbo::{Affine, Point, Rect};

/// A read-only geometry member of an actor.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum ActorPoint {
    /// A named point on the transformed layout box.
    Bounds(SceneAnchor),
    /// The transformed local origin.
    Origin,
    /// The area centroid of a supported filled shape.
    Centroid,
}

#[derive(Clone, Copy)]
pub(crate) struct FixedActorPoint {
    position: Point,
    first_seen_ms: u64,
}

impl ActorPoint {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Bounds(anchor) => anchor.as_str(),
            Self::Origin => "origin",
            Self::Centroid => "centroid",
        }
    }
    pub(crate) fn named(name: &str) -> Option<Self> {
        match name {
            "origin" => Some(Self::Origin),
            "centroid" => Some(Self::Centroid),
            _ => SceneAnchor::from_str(name).map(Self::Bounds),
        }
    }

    pub(crate) fn in_box(self, bounds: Rect, origin: Point) -> Point {
        match self {
            Self::Origin => origin,
            Self::Centroid => bounds.center(),
            Self::Bounds(anchor) => {
                let center = bounds.center();
                let [x, y] = super::callout_geometry::bounds_anchor_point(
                    anchor,
                    [center.x as f32, center.y as f32],
                    [
                        (bounds.width() * 0.5) as f32,
                        (bounds.height() * 0.5) as f32,
                    ],
                );
                Point::new(f64::from(x), f64::from(y))
            },
        }
    }
}

impl Timeline {
    pub(crate) fn bind_declared_line_endpoints(
        &mut self,
        id: EntityId,
        properties: &[crate::ast::Property],
        env: &super::Environment,
        (start, end, easing): (u64, u64, super::Easing),
        diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
        subject: &str,
    ) {
        if matches!(
            self.world.caps(id).shape,
            Some(super::ShapeKind::Line | super::ShapeKind::Arrow)
        ) {
            for property in properties {
                self.write_line_endpoint(
                    id,
                    &property.name,
                    &property.value,
                    env,
                    start,
                    end,
                    easing,
                    diagnostics,
                    subject,
                );
            }
        }
    }

    #[allow(clippy::too_many_arguments)] // Preserve the declaration or assignment's timing and diagnostic context.
    pub(crate) fn write_line_endpoint(
        &mut self,
        id: EntityId,
        property: &str,
        expr: &crate::ast::Expr,
        env: &super::Environment,
        start: u64,
        end: u64,
        easing: super::Easing,
        diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
        subject: &str,
    ) -> bool {
        let from = match property {
            "from" | "line_from" => true,
            "to" | "line_to" => false,
            _ => return false,
        };
        let bound = self.world.shape(id).is_some_and(|shape| {
            if from {
                shape.from_anchor.is_some()
            } else {
                shape.to_anchor.is_some()
            }
        });
        let value = if bound {
            track::PointValue::parse(expr, env, diagnostics, subject)
        } else {
            track::PointValue::reference(expr, env)
        };
        if let Some(value) = value {
            self.world.ensure_shape(id).write_endpoint(from, value, start, end, easing);
            return true;
        }
        bound
    }

    /// Read a folded point or sample the target's ancestor chain.
    pub(crate) fn actor_point(
        &self,
        id: EntityId,
        point: ActorPoint,
        frame: &RenderFrame<'_>,
        projected: bool,
    ) -> Option<Point> {
        if frame.overrides.is_empty()
            && !(projected && self.camera_used.get())
            && let Some(fixed) = self.fixed_actor_point(id, point)
        {
            return (fixed.first_seen_ms <= frame.time_ms).then_some(fixed.position);
        }
        self.actor_point_is_available(id, frame.time_ms)
            .then(|| self.sample_actor_point(id, point, frame, projected))
    }

    /// Fold geometry only when every ancestor is constant and independent of viewport size.
    pub(crate) fn prime_actor_points(
        &self,
        points: impl IntoIterator<Item = (EntityId, ActorPoint)>,
    ) {
        if !self.camera_used.get() && !self.dynamic_layout {
            for (id, point) in points {
                self.fixed_actor_point(id, point);
            }
        }
    }

    fn fixed_actor_point(&self, id: EntityId, point: ActorPoint) -> Option<FixedActorPoint> {
        if self.dynamic_layout {
            return None;
        }
        let key = (id, point);
        if let Some(cached) = self.eval_caches.fixed_actor_points.borrow().get(&key) {
            return *cached;
        }
        let mut node = id;
        let fixed = loop {
            if !self.is_transform_static(node)
                || self.world.geometry(node).position_binding.get(0, PositionBinding::Absolute)
                    != PositionBinding::Absolute
            {
                break None;
            }
            let Some(parent) = self.world.hierarchy(node).parent else {
                let overrides = Default::default();
                let frame = Self::point_query_frame(0, SceneDimensions::default(), &overrides);
                break Some(FixedActorPoint {
                    position: self.sample_actor_point(id, point, &frame, false),
                    first_seen_ms: self.world.first_seen(id),
                });
            };
            node = parent;
        };
        self.eval_caches.fixed_actor_points.borrow_mut().insert(key, fixed);
        fixed
    }

    fn actor_point_is_available(&self, id: EntityId, time_ms: u64) -> bool {
        self.world.exists(id) && self.world.first_seen(id) <= time_ms
    }

    fn sample_actor_point(
        &self,
        id: EntityId,
        point: ActorPoint,
        frame: &RenderFrame<'_>,
        projected: bool,
    ) -> Point {
        let (transform, half_size) = self.actor_point_transform(id, frame, projected);
        let origin = transform * Point::ORIGIN;
        match point {
            ActorPoint::Origin => return origin,
            ActorPoint::Centroid => {
                if let Some(centroid) = self.local_area_centroid(id, frame.time_ms) {
                    return transform * centroid;
                }
            },
            ActorPoint::Bounds(_) => {},
        }
        let bounds = self.actor_point_bounds(id, frame.time_ms, &transform, half_size);
        point.in_box(bounds, origin)
    }

    pub(crate) fn actor_point_geometry(
        &self,
        id: EntityId,
        frame: &RenderFrame<'_>,
        projected: bool,
    ) -> Option<(Rect, Point)> {
        if !self.actor_point_is_available(id, frame.time_ms) {
            return None;
        }
        let (transform, half_size) = self.actor_point_transform(id, frame, projected);
        let bounds = self.actor_point_bounds(id, frame.time_ms, &transform, half_size);
        Some((bounds, transform * Point::ORIGIN))
    }

    fn actor_point_bounds(
        &self,
        id: EntityId,
        time_ms: u64,
        transform: &Affine,
        half_size: [f32; 2],
    ) -> Rect {
        let [w, h] = half_size.map(f64::from);
        let layout_box = Rect::new(-w, -h, w, h);
        let local = self.point_local_bounds(id, time_ms).unwrap_or_else(|| {
            let shift = self.world.text(id).map_or(0.0, |text| {
                match text.text_anchor.get(time_ms, "center".into()).as_str() {
                    "left" => w,
                    "right" => -w,
                    _ => 0.0,
                }
            });
            layout_box + kurbo::Vec2::new(shift, 0.0)
        });
        super::scene_eval::transform_rect_bbox(transform, local)
    }

    fn point_local_bounds(&self, id: EntityId, time_ms: u64) -> Option<Rect> {
        use super::ShapeKind;
        let shape = self.world.shape(id)?;
        match self.world.caps(id).shape? {
            ShapeKind::Path | ShapeKind::Polygon => {
                let paths = shape.evaluate_vector_paths(self.world.style(id), time_ms);
                super::scene_eval::node_local_bounds(&paths, &[], &[], None)
            },
            _ => None,
        }
    }

    fn local_area_centroid(&self, id: EntityId, time_ms: u64) -> Option<Point> {
        use super::ShapeKind;
        match self.world.caps(id).shape? {
            ShapeKind::Rect | ShapeKind::Ellipse => Some(Point::ORIGIN),
            ShapeKind::Polygon => {
                let shape = self.world.shape(id)?;
                let points = shape.points.as_ref()?;
                let keys = points.keyframes.as_slice();
                if keys.len() < 2 {
                    let epoch = shape.vector_paths_epoch.get();
                    if let Some((cached_epoch, centroid)) = shape.polygon_centroid_memo.get()
                        && cached_epoch == epoch
                    {
                        return centroid;
                    }
                    let points = keys.first().map_or(&points.default_value, |key| &key.payload.0);
                    let centroid = polygon_centroid(points);
                    shape.polygon_centroid_memo.set(Some((epoch, centroid)));
                    return centroid;
                }
                polygon_centroid(&points.evaluate(time_ms))
            },
            _ => None, // Unsupported fill geometry uses its layout-box center.
        }
    }

    pub(crate) fn actor_point_transform(
        &self,
        id: EntityId,
        frame: &RenderFrame<'_>,
        projected: bool,
    ) -> (Affine, [f32; 2]) {
        let parent = self.world.hierarchy(id).parent;
        let (parent_transform, position) = if let Some(parent) = parent {
            let (transform, _) = self.actor_point_transform(parent, frame, projected);
            let position = self
                .dynamic_layout
                .then(|| self.compute_animated_layout(parent, frame.time_ms))
                .and_then(|positions| positions.get_by_id(id).copied());
            (transform, position)
        } else if projected && self.camera_used.get() {
            let camera = self.camera.affine(
                frame.time_ms,
                frame.scene_dimensions,
                frame.overrides.get(super::camera::CAMERA_TARGET),
            );
            (self.root_camera_affine(id, camera), None)
        } else {
            (Affine::IDENTITY, None)
        };
        let overrides = if frame.overrides.is_empty() {
            None
        } else {
            self.world.label(id).and_then(|label| frame.overrides.get(label))
        };
        let transform = self.evaluate_node_transform(
            id,
            frame.time_ms,
            1.0,
            parent_transform,
            frame.scene_dimensions,
            position,
            overrides,
        );
        (transform.local_transform, transform.half_size)
    }

    pub(crate) fn root_camera_affine(&self, id: EntityId, camera: Affine) -> Affine {
        if !self.camera_exempt_present.get() {
            return camera;
        }
        let meta = self.world.meta(id);
        if !meta.camera_follow || meta.parallax == 0.0 {
            return Affine::IDENTITY;
        }
        if (meta.parallax - 1.0).abs() < f32::EPSILON {
            return camera;
        }
        let p = f64::from(meta.parallax);
        let [a, b, c, d, e, f] = camera.as_coeffs();
        Affine::new([
            1.0 + p * (a - 1.0),
            p * b,
            p * c,
            1.0 + p * (d - 1.0),
            p * e,
            p * f,
        ])
    }

    pub(crate) fn point_query_frame<'a>(
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &'a std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> RenderFrame<'a> {
        RenderFrame {
            paint_omissions: &[],
            time_ms,
            scene_dimensions,
            overrides,
            frame_env: None,
            debug_options: Default::default(),
        }
    }
}

fn polygon_centroid(points: &[[f32; 2]]) -> Option<Point> {
    let mut area_twice = 0.0;
    let mut first_moment = kurbo::Vec2::ZERO;
    let mut previous = points.last()?.map(f64::from);
    for current in points.iter().map(|point| point.map(f64::from)) {
        let cross = previous[0] * current[1] - current[0] * previous[1];
        area_twice += cross;
        first_moment +=
            kurbo::Vec2::new(previous[0] + current[0], previous[1] + current[1]) * cross;
        previous = current;
    }
    if !area_twice.is_finite() || area_twice.abs() <= f64::EPSILON {
        return None;
    }
    Some(Point::ORIGIN + first_moment / (3.0 * area_twice))
}
