use crate::timeline::*;

impl Timeline {
    /// Label-keyed hit regions for the current frame.
    pub fn hit_regions(&self) -> Vec<(String, kurbo::Rect)> {
        let regions = self.eval_caches.hit_regions.borrow();
        regions
            .iter()
            .filter_map(|(id, bounds)| {
                self.world.label(*id).map(|label| (label.to_string(), *bounds))
            })
            .collect()
    }

    /// Returns the hit regions from the last evaluate call, keyed by entity id.
    pub fn hit_regions_by_id(&self) -> Vec<(EntityId, kurbo::Rect)> {
        self.eval_caches.hit_regions.borrow().clone()
    }

    /// Returns a reference to the track for the given label, if it exists.
    pub fn get_track(&self, label: &str) -> Option<&AnimationTrack> {
        self.world.tracks.get(label)
    }

    /// Returns a mutable reference to the track for the given label, if it exists.
    ///
    /// Invalidates the frame cache before returning, because any mutation to
    /// the track can change the rendered scene at the cached time.
    pub fn get_track_mut(&mut self, label: &str) -> Option<&mut AnimationTrack> {
        self.invalidate_frame_cache();
        self.world.tracks.get_mut(label)
    }

    /// Return the entity ID corresponding to the given actor label.
    #[inline(always)]
    pub fn get_entity_id(&self, label: &str) -> Option<EntityId> {
        self.world.tracks.get_id(label)
    }

    /// Return the actor label corresponding to the given entity ID.
    #[inline(always)]
    pub fn get_entity_label(&self, id: EntityId) -> Option<&str> {
        self.world.tracks.get_label(id)
    }

    /// Returns a reference to the track for the given entity ID.
    #[inline(always)]
    pub fn get_track_by_id(&self, id: EntityId) -> Option<&AnimationTrack> {
        self.world.tracks.get_by_id(id)
    }

    /// Returns a mutable reference to the track for the given entity ID.
    #[inline(always)]
    pub fn get_track_by_id_mut(&mut self, id: EntityId) -> Option<&mut AnimationTrack> {
        self.invalidate_frame_cache();
        self.world.tracks.get_by_id_mut(id)
    }

    /// Check if a keyframe exists for the given actor and property at `time_ms`.
    pub fn has_keyframe_at(&self, actor: &str, property: &str, time_ms: u64) -> bool {
        self.world
            .tracks
            .get(actor)
            .map(|t| t.has_keyframe_at(property, time_ms))
            .unwrap_or(false)
    }

    /// List all keyframe times (in ms) for the given actor and property.
    pub fn list_keyframes(&self, actor: &str, property: &str) -> Vec<u64> {
        self.world
            .tracks
            .get(actor)
            .map(|t| t.list_keyframes(property))
            .unwrap_or_default()
    }

    /// Evaluate the background color at the given time.
    pub fn background_color_at(&self, time_ms: u64) -> [f32; 4] {
        self.background_color.evaluate(time_ms)
    }

    /// Returns a reference to all tracks.
    pub fn tracks(&self) -> &TrackStore {
        &self.world.tracks
    }

    /// Returns a mutable reference to all tracks.
    pub fn tracks_mut(&mut self) -> &mut TrackStore {
        self.invalidate_frame_cache();
        &mut self.world.tracks
    }

    /// Returns the ECS world (entities + component stores).
    #[inline(always)]
    pub fn world(&self) -> &World {
        &self.world
    }

    /// Returns the ECS world for build-time mutation.
    ///
    /// Invalidates the frame cache first: any component write can change the
    /// rendered scene at the cached time.
    pub fn world_mut(&mut self) -> &mut World {
        self.invalidate_frame_cache();
        &mut self.world
    }

    /// Read-only entity view for a label.
    #[inline(always)]
    pub fn entity(&self, label: &str) -> Option<EntityRef<'_>> {
        self.world.entity(label)
    }

    /// Read-only entity view for an id.
    #[inline(always)]
    pub fn entity_by_id(&self, id: EntityId) -> Option<EntityRef<'_>> {
        self.world.view(id)
    }

    /// Mutable entity view for a label. Invalidates the frame cache first.
    pub fn entity_mut(&mut self, label: &str) -> Option<EntityMut<'_>> {
        self.invalidate_frame_cache();
        self.world.entity_mut(label)
    }

    /// Mutable entity view for an id. Invalidates the frame cache first.
    pub fn entity_by_id_mut(&mut self, id: EntityId) -> Option<EntityMut<'_>> {
        self.invalidate_frame_cache();
        self.world.view_mut(id)
    }

    /// Create an entity with a blank track, or return the existing one.
    ///
    /// Build code, tests and plugins use this instead of inserting into
    /// `TrackStore` by hand. Invalidates the frame cache first.
    pub fn spawn_entity(&mut self, label: &str, actor_type: impl Into<String>) -> EntityId {
        self.invalidate_frame_cache();
        self.world.spawn(label, actor_type)
    }

    /// Read the speech transcript attached to `label`, if any.
    #[inline(always)]
    pub fn transcript(&self, label: &str) -> Option<&animatix_text::transcript::SpeechTranscript> {
        let id = self.world.get_id(label)?;
        self.world.transcript.get(id).map(|component| &component.0)
    }

    /// Iterate every actor that carries a speech transcript, keyed by entity id.
    pub fn transcripts_iter(
        &self,
    ) -> impl Iterator<Item = (EntityId, &animatix_text::transcript::SpeechTranscript)> + '_ {
        self.world.transcript.iter().map(|(id, component)| (id, &component.0))
    }

    /// Attach a parsed speech transcript to `label`.
    ///
    /// Returns `false` when no such entity exists (the transcript is dropped;
    /// the pre-pass only collects labels the build then creates).
    pub fn set_transcript(
        &mut self,
        label: &str,
        transcript: animatix_text::transcript::SpeechTranscript,
    ) -> bool {
        self.invalidate_frame_cache();
        let Some(id) = self.world.get_id(label) else {
            tracing::warn!("speech transcript for '{label}' dropped: no such actor");
            return false;
        };
        self.world.transcript.insert(id, TranscriptComponent(transcript));
        true
    }

    /// Whether `label` is marked `persistent` by a `persist` action.
    ///
    /// An entity that was never targeted resolves `false`, matching the old
    /// label-keyed map's default. `camera` is not an entity and reads the
    /// scene-level flag instead.
    pub fn persistence_flag(&self, label: &str) -> bool {
        if label == camera::CAMERA_TARGET {
            return self.camera_persistent;
        }
        self.entity(label)
            .and_then(|entity| entity.persistence())
            .is_some_and(|component| component.persistent)
    }

    /// Set (or clear) `label`'s persistence flag.
    ///
    /// Invalidates the frame cache first, like every component write.
    pub fn set_persistence_flag(&mut self, label: &str, persistent: bool) {
        self.invalidate_frame_cache();
        if label == camera::CAMERA_TARGET {
            self.camera_persistent = persistent;
            return;
        }
        let Some(id) = self.world.get_id(label) else {
            tracing::warn!("persistence flag for '{label}' dropped: no such actor");
            return;
        };
        self.world.persistence.insert(id, PersistenceComponent { persistent });
    }

    /// Iterate every explicit persistence flag as `(actor_label, flag)`.
    ///
    /// Includes `false` flags written by `remove` (so callers can tell
    /// "never persisted" from "explicitly removed") and the reserved `camera`
    /// target when it is marked persistent.
    pub fn persistence_flags_iter(&self) -> impl Iterator<Item = (&str, bool)> + '_ {
        let actors = self.world.persistence.iter().filter_map(|(id, component)| {
            self.world.label(id).map(|label| (label, component.persistent))
        });
        let camera = self.camera_persistent.then_some((camera::CAMERA_TARGET, true));
        actors.chain(camera)
    }

    /// Read the auto-color slot assigned to `label`, if any.
    ///
    /// Also reports a slot still buffered for an entity that has not been
    /// spawned yet (the build assigns `color: auto` before the primitive
    /// creates the track; the world attaches the buffered slot on spawn).
    pub fn auto_color_slot(&self, label: &str) -> Option<usize> {
        if let Some((pending_label, slot)) = &self.world.pending_auto_color {
            if pending_label == label {
                return Some(*slot);
            }
        }
        let id = self.world.get_id(label)?;
        self.world.auto_color.get(id).map(|component| component.slot)
    }

    /// Record `label`'s auto-color slot.
    ///
    /// Invalidates the frame cache first. During the build the entity may not
    /// exist yet; the slot is then buffered by [`World`] and attached when the
    /// declaration's track is spawned.
    pub fn set_auto_color_slot(&mut self, label: &str, slot: usize) {
        self.invalidate_frame_cache();
        if let Some(id) = self.world.get_id(label) {
            self.world.auto_color.insert(id, AutoColorComponent { slot });
            return;
        }
        if let Some((pending_label, _)) = &self.world.pending_auto_color {
            if pending_label != label {
                tracing::warn!(
                    "auto-color slot for '{label}' overwrote a pending slot for '{pending_label}'"
                );
            }
        }
        self.world.pending_auto_color = Some((label.to_string(), slot));
    }

    /// Map each child label to its parent label, derived from the hierarchy
    /// component. Used by build diagnostics (never-revealed ancestors).
    pub fn parent_of(&self) -> std::collections::HashMap<&str, &str> {
        let mut parents = std::collections::HashMap::new();
        for (parent, hierarchy) in self.world.hierarchy.iter() {
            let Some(parent_label) = self.world.label(parent) else {
                continue;
            };
            for child in &hierarchy.children {
                if let Some(child_label) = self.world.label(*child) {
                    parents.insert(child_label, parent_label);
                }
            }
        }
        parents
    }

    /// Returns the layout metadata of a container actor, if it is one.
    ///
    /// Label-boundary helper: the frame path resolves `EntityId`s once and
    /// then uses [`crate::timeline::EntityRef::container_layout`].
    pub fn container_layout(&self, label: &str) -> Option<&ContainerMetadata> {
        self.entity(label)?.container_layout()
    }

    /// Returns the layout metadata of a container actor mutably.
    ///
    /// Invalidates the frame cache before returning, because layout metadata
    /// changes affect rendered positions.
    pub fn container_layout_mut(&mut self, label: &str) -> Option<&mut ContainerMetadata> {
        self.invalidate_frame_cache();
        let id = self.world.get_id(label)?;
        self.world.container_layout.get_mut(id)
    }

    /// Inserts or replaces a container's layout metadata.
    ///
    /// Returns `false` when no such entity exists (the caller registered a
    /// container before its track was created, which the build never does).
    /// Invalidates the frame cache, like every component write.
    pub fn set_container_layout(&mut self, label: &str, metadata: ContainerMetadata) -> bool {
        self.invalidate_frame_cache();
        let Some(id) = self.world.get_id(label) else {
            tracing::warn!("container layout for '{label}' dropped: no such actor");
            return false;
        };
        self.world.container_layout.insert(id, metadata);
        true
    }

    /// Iterate every container's layout metadata, keyed by entity id.
    ///
    /// Slot order (insertion order), not label order: callers that need a
    /// label map ids through [`Timeline::get_entity_label`].
    pub fn container_layouts(&self) -> impl Iterator<Item = (EntityId, &ContainerMetadata)> + '_ {
        self.world.container_layout.iter()
    }

    /// Entity ids of every container, in slot order.
    pub fn container_layout_ids(&self) -> Vec<EntityId> {
        self.world.container_layout.iter().map(|(id, _)| id).collect()
    }

    /// Returns a reference to the asset cache.
    pub fn asset_cache(&self) -> &assets::AssetCache {
        &self.asset_cache
    }

    /// Returns a cloneable handle to the asset cache for carrying into rebuilds.
    pub fn asset_cache_arc(&self) -> std::sync::Arc<assets::AssetCache> {
        self.asset_cache.clone()
    }

    /// Configure document/workspace base directories for relative asset URLs.
    pub fn set_asset_base_dirs(
        &mut self,
        document_dir: Option<&std::path::Path>,
        workspace_root: Option<&std::path::Path>,
    ) {
        std::sync::Arc::make_mut(&mut self.asset_cache).set_base_dirs(document_dir, workspace_root);
    }

    /// Iterate over asset path → actor labels that reference it.
    pub fn asset_usage(&self) -> impl Iterator<Item = (&String, &BTreeSet<String>)> {
        self.asset_cache.asset_usage()
    }

    /// Returns a reference to the environment.
    pub fn env(&self) -> &Environment {
        &self.env
    }

    /// Returns a mutable reference to the environment.
    ///
    /// Invalidates the frame cache before returning, because environment
    /// changes can affect modifier/plot evaluation on the next frame.
    pub fn env_mut(&mut self) -> &mut Environment {
        self.invalidate_frame_cache();
        &mut self.env
    }
}
