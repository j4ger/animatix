//! Plot-host coordinate frame: the timeline access surface.
//!
//! `CoordinateFrameComponent` is the sole source for a hosted child's
//! coordinate resolution. These entry points live outside `timeline/mod.rs` so
//! the frame access surface does not grow the timeline root module: build code
//! registers a frame, carry injects one, and the rebuild path reads one, all
//! through here.
//!
//! Every write goes through [`Timeline::set_coordinate_frame`], which
//! invalidates the frame cache like every other component write.

use super::{CoordinateFrameComponent, CoordinateFrameTarget, EntityId, Timeline};

impl Timeline {
    /// The coordinate frame attached to `id`, when that entity is a plot host.
    ///
    /// This component is the sole source for a hosted child's coordinate
    /// resolution; the `{label}_x_domain` env entries only exist for authored
    /// expressions and are derived from it at build time.
    #[inline(always)]
    pub fn coordinate_frame(&self, id: EntityId) -> Option<&CoordinateFrameComponent> {
        self.world.coordinate_frame(id)
    }

    /// The coordinate frame attached to `label`, when that actor is a plot host.
    ///
    /// Label-boundary helper: the frame path resolves the entity once and then
    /// uses [`Timeline::coordinate_frame`].
    pub fn coordinate_frame_for(&self, label: &str) -> Option<&CoordinateFrameComponent> {
        let id = self.world.get_id(label)?;
        self.world.coordinate_frame(id)
    }

    /// Attach or replace a plot host's coordinate frame.
    ///
    /// `target` is either a stable [`EntityId`] (build code already has one) or
    /// an actor label (action / carry / test boundary). Returns `false` when a
    /// label names no entity, or an id names no live track.
    ///
    /// Invalidates the frame cache first, like every component write.
    pub fn set_coordinate_frame<'a>(
        &mut self,
        target: impl Into<CoordinateFrameTarget<'a>>,
        frame: CoordinateFrameComponent,
    ) -> bool {
        self.invalidate_frame_cache();
        let id = match target.into() {
            CoordinateFrameTarget::Id(id) => {
                if self.world.track(id).is_none() {
                    tracing::warn!("coordinate frame for {id} dropped: no such entity");
                    return false;
                }
                id
            },
            CoordinateFrameTarget::Label(label) => {
                let Some(id) = self.world.get_id(label) else {
                    tracing::warn!("coordinate frame for '{label}' dropped: no such actor");
                    return false;
                };
                id
            },
        };
        self.world.coordinate_frame.insert(id, frame);
        true
    }
}
