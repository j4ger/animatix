//! Per-entity metadata that is not a keyframed property tier: visibility,
//! authoring lock, and camera participation.

/// Header data every entity carries.
#[derive(Clone, Debug, PartialEq)]
pub struct EntityMeta {
    /// Whether the actor is visible in the preview and export.
    pub visible: bool,
    /// Whether the actor is locked (prevents selection and drag in the GUI).
    pub locked: bool,
    /// Whether the scene camera wraps this actor.
    pub camera_follow: bool,
    /// Continuous parallax multiplier for camera movement on root actors.
    pub parallax: f32,
    /// Whether the build hid this actor until an entrance action.
    pub hidden_by_default: bool,
    /// First frame (ms) this actor appears; `u64::MAX` until seen.
    pub first_seen_ms: u64,
}

impl EntityMeta {
    /// Default header data for an entity that has no component slot yet.
    pub const DEFAULT: Self = Self {
        visible: true,
        locked: false,
        camera_follow: true,
        parallax: 1.0,
        hidden_by_default: false,
        first_seen_ms: u64::MAX,
    };
}

impl Default for EntityMeta {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl super::component::ComponentStore<EntityMeta> {
    /// Record the first sighting of `id`; later calls keep the first time.
    pub fn record_first_seen(&mut self, id: super::EntityId, time_ms: u64) {
        let meta = self.get_or_insert_with(id, EntityMeta::default);
        if meta.first_seen_ms == u64::MAX {
            meta.first_seen_ms = time_ms;
        }
    }
}
