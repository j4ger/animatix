//! Actor/scene state helpers on `PreviewContext`.

use crate::app::preview::{self, ActorProps};

use super::PreviewContext;

impl PreviewContext<'_> {
    pub(crate) fn current_theme(&self) -> eparts::Theme {
        self.preview.overlay.current_theme()
    }

    pub(crate) fn get_actor_props(&self, actor: &str) -> Option<ActorProps> {
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        self.get_actor_props_at_time(actor, time_ms)
    }

    pub(crate) fn get_actor_props_at_time(&self, actor: &str, time_ms: u64) -> Option<ActorProps> {
        let timeline = self.timeline?;
        let track = timeline.get_track(actor)?;
        let half = track.geometry.size.as_ref().map(|pt| pt.evaluate(time_ms))?;
        let local_size = [half[0] * 2.0, half[1] * 2.0];
        let world_affine = timeline.actor_world_affine(actor, time_ms, self.scene_dimensions)?;
        let coeffs = world_affine.as_coeffs();
        let position = [coeffs[4] as f32, coeffs[5] as f32];
        let rotation = (coeffs[1] as f32).atan2(coeffs[0] as f32);
        let scale = ((coeffs[0] * coeffs[0] + coeffs[1] * coeffs[1]).sqrt()) as f32;
        let size = [local_size[0] * scale, local_size[1] * scale];
        let pivot_offset = self.pivot_offsets.get(actor).copied().unwrap_or([0.0, 0.0]);
        Some(ActorProps {
            position,
            size,
            rotation,
            pivot_offset,
        })
    }

    /// Union of the selected actors' rotation-aware scene bounds, falling back
    /// to cached hit regions. Shared by the multi-selection overlay and the
    /// group transform gestures so the drawn box and the hit box agree.
    pub(crate) fn selected_union_rect(&self) -> Option<kurbo::Rect> {
        let mut union: Option<kurbo::Rect> = None;
        for actor in self.selected_actors.iter() {
            let bounds = if let Some(props) = self.get_actor_props(actor) {
                let hw = props.size[0] / 2.0;
                let hh = props.size[1] / 2.0;
                let local_corners = [[-hw, -hh], [hw, -hh], [hw, hh], [-hw, hh]];
                let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
                let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
                for corner in &local_corners {
                    let world = preview::local_to_world(*corner, props.position, props.rotation);
                    min_x = min_x.min(world.x);
                    min_y = min_y.min(world.y);
                    max_x = max_x.max(world.x);
                    max_y = max_y.max(world.y);
                }
                kurbo::Rect::new(min_x, min_y, max_x, max_y)
            } else if let Some((_, bounds)) = self.hit_regions.iter().find(|(l, _)| l == actor) {
                *bounds
            } else {
                continue;
            };
            union = Some(match union {
                None => bounds,
                Some(u) => kurbo::Rect::new(
                    u.x0.min(bounds.x0),
                    u.y0.min(bounds.y0),
                    u.x1.max(bounds.x1),
                    u.y1.max(bounds.y1),
                ),
            });
        }
        union
    }

    /// Resize mode and current transform scale for an actor's primitive.
    /// Mirrors the single-actor scale path so group scaling stays consistent.
    pub(crate) fn actor_resize_mode(&self, actor: &str) -> (preview::ResizeMode, f32) {
        use animatix::timeline::TrackAccessor;
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        self.timeline
            .and_then(|t| {
                t.get_track(actor).map(|tr| {
                    let affordances = crate::app::primitives::actor_gizmo_affordances(tr, t);
                    let mode = affordances
                        .iter()
                        .find_map(|a| match a {
                            crate::app::primitives::GizmoAffordance::BoundingBox {
                                resize_mode,
                            } => match resize_mode {
                                animatix::timeline::ResizeMode::Scale => {
                                    Some(preview::ResizeMode::Scale)
                                },
                                _ => Some(preview::ResizeMode::Size),
                            },
                            _ => None,
                        })
                        .unwrap_or(preview::ResizeMode::Size);
                    (mode, tr.geometry.scale.get(time_ms, 1.0))
                })
            })
            .unwrap_or((preview::ResizeMode::Size, 1.0))
    }

    /// Capture per-actor start state for a group transform. Locked actors are
    /// excluded, matching single-actor behaviour.
    pub(crate) fn capture_group_actors(&self) -> Vec<preview::GroupTransformActor> {
        self.selected_actors
            .iter()
            .filter(|label| {
                self.timeline
                    .and_then(|t| t.get_track(label))
                    .map(|tr| !tr.locked)
                    .unwrap_or(true)
            })
            .filter_map(|label| {
                let props = self.get_actor_props(label)?;
                let (resize_mode, scale) = self.actor_resize_mode(label);
                Some(preview::GroupTransformActor {
                    label: label.clone(),
                    position: props.position,
                    size: props.size,
                    rotation: props.rotation,
                    resize_mode,
                    scale,
                })
            })
            .collect()
    }

    pub(crate) fn is_layout_managed(&self, actor: &str) -> bool {
        let Some(timeline) = self.timeline else {
            return false;
        };
        let time_ms = (self.preview.playback.current_time_s() * 1000.0) as u64;
        preview::is_layout_managed(actor, timeline, time_ms)
    }

    pub(crate) fn find_layout_container(
        &self,
        actor: &str,
    ) -> Option<(String, animatix::timeline::LayoutType, usize)> {
        let timeline = self.timeline?;
        let actor_id = timeline.get_entity_id(actor)?;
        let container_id = timeline.world().hierarchy(actor_id).parent?;
        let container = timeline.get_entity_label(container_id)?.to_string();
        let metadata = timeline.container_layout(&container)?;
        let source_index = timeline
            .world()
            .hierarchy(container_id)
            .children
            .iter()
            .position(|child| *child == actor_id)?;
        Some((container, metadata.layout_type, source_index))
    }
}
