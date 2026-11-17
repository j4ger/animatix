//! Ownership of actors declared inside a mounted reusable timeline plan.

/// Identifies the plan placement that owns a private actor.
///
/// The token is component data keyed by `EntityId`; it is not derived from or
/// looked up by the actor's label. Caller-owned actors have no ownership value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlanOwnership {
    mount_id: u64,
}

impl PlanOwnership {
    /// Create an ownership token for one mounted plan instance.
    pub(crate) const fn new(mount_id: u64) -> Self {
        Self { mount_id }
    }

    /// Return the unique plan-mount identity that owns this actor.
    pub const fn mount_id(self) -> u64 {
        self.mount_id
    }
}
