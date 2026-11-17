//! Handle for immutable component-method metadata owned by a scene build.

/// Identifies an immutable component-method table for one expanded actor
/// instance. The handle is stored in the entity's World component slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MethodTableId(u32);

impl MethodTableId {
    /// Construct a method-table handle assigned by the build registry.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Return the registry key.
    pub const fn get(self) -> u32 {
        self.0
    }
}
