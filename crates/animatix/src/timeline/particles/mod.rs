//! Deterministic world-space particles. The emitter owns the schedule in the ECS.
mod build;
mod render;
mod schedule;
mod template;

use crate::timeline::{ActorPoint, EntityId};
pub(crate) use build::{build, emit, set_rate};
pub(crate) use render::evaluate;
use schedule::{Schedule, hash};

#[derive(Clone, Debug)]
pub(crate) struct Emitter {
    schedule: Schedule,
    origin: Option<(EntityId, ActorPoint)>,
    seed: u64,
    life: f64,
    speed: f64,
    radius: f64,
    spread: f64,
    heading: f64,
    jitter: f64,
    style: Style,
    template: Option<std::sync::Arc<crate::scene::VectorArtwork>>,
    motion: Option<Motion>,
    scale: Option<[f64; 2]>,
    angle: [f64; 2],
    spin: [f64; 2],
}
impl Emitter {
    pub(super) fn end_hint(&self) -> f64 {
        self.schedule.end_hint(self.life)
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
enum Style {
    Bubble,
    Dust,
    Spark,
}

#[derive(Clone, Copy, Debug)]
enum Motion {
    Linear,
    Drift,
    Ballistic,
    Bubble,
    Dust,
}

#[cfg(test)]
mod tests;
