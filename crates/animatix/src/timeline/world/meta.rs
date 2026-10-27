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

impl super::World {
    /// Borrow an entity's metadata.
    #[inline(always)]
    pub fn meta(&self, id: super::EntityId) -> &EntityMeta {
        self.meta.get(id).unwrap_or(&EntityMeta::DEFAULT)
    }

    /// First frame this entity appears, or `u64::MAX` when unseen.
    #[inline(always)]
    pub fn first_seen(&self, id: super::EntityId) -> u64 {
        self.meta.get(id).map_or(u64::MAX, |meta| meta.first_seen_ms)
    }

    /// Record the first frame this entity appears.
    #[inline(always)]
    pub fn set_first_seen(&mut self, id: super::EntityId, time_ms: u64) {
        let meta = self.meta.get_or_insert_with(id, EntityMeta::default);
        if meta.first_seen_ms == u64::MAX {
            meta.first_seen_ms = time_ms;
        }
    }

    /// Mutably borrow an entity's metadata.
    #[inline(always)]
    pub fn meta_mut(&mut self, id: super::EntityId) -> &mut EntityMeta {
        self.meta.get_or_insert_with(id, EntityMeta::default)
    }
}
