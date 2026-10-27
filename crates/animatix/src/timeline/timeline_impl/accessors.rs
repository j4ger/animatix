use crate::timeline::*;

impl Timeline {
    /// Create a new empty timeline with a fresh font context.
    pub fn new() -> Self {
        Self::new_with_font_context(std::sync::Arc::new(crate::renderer::text::FontContext::new()))
    }

    /// Create a new empty timeline with the given font context.
    pub fn new_with_font_context(
        font_context: std::sync::Arc<crate::renderer::text::FontContext>,
    ) -> Self {
        let mut bg_track = PropertyTrack::new([0.0, 0.0, 0.0, 1.0]);
        bg_track.add_keyframe(0, [0.0, 0.0, 0.0, 1.0], Easing::Linear);
        Self {
            world: World::new(),
            background_color: bg_track,
            env: Environment::new(),
            block_scope: Vec::new(),
            primitive_registry: std::sync::Arc::new(crate::primitives::PrimitiveRegistry::new()),
            behavior_registry: std::sync::Arc::new(
                crate::timeline::behavior::BehaviorRegistry::new(),
            ),
            extensions: None,
            has_effect_scopes: false,
            env_base: std::sync::Arc::new(Default::default()),
            env_pool: std::cell::RefCell::new(None),
            modifiers: Vec::new(),
            frame_written_vars: std::collections::HashSet::new(),
            modifier_programs: Vec::new(),
            modifier_written_targets: std::collections::HashSet::new(),
            colorscheme: BuiltInColorscheme::DefaultDark.resolved(),
            external_colorschemes: std::collections::HashMap::new(),
            export_preset: None,
            resolution: None,
            declared_duration_s: None,
            next_auto_color_index: 0,
            layout_children_cache: std::cell::RefCell::new(std::collections::HashMap::new()),
            layout_engine: LayoutEngine::new(),
            dynamic_layout: false,
            seamless_loop: false,
            beat_ms: None,
            cli_defines: std::collections::HashSet::new(),
            asset_cache: std::sync::Arc::new(assets::AssetCache::new()),
            font_context,
            build_quality: BuildQuality::Production,
            default_opacity: 1.0,
            camera_persistent: false,
            pause_points: Vec::new(),
            text_compiler: std::cell::RefCell::new(crate::renderer::text::TextCompiler::new()),
            eval_caches: EvalCaches::default(),
            blend_used: std::cell::Cell::new(false),
            camera: camera::Camera::default(),
            camera_used: std::cell::Cell::new(false),
            camera_exempt_present: std::cell::Cell::new(false),
            variable_tracks: BTreeMap::new(),
            referenced_roots: None,
            stats_used: false,
            is_animating_used: false,
            wildcard_actors: std::collections::HashSet::new(),
            referenced_properties: std::collections::HashMap::new(),
            audio_segments: Vec::new(),
            action_events: Vec::new(),
            plot_path_cache: std::collections::HashMap::new(),
            modifier_hash: 0,
        }
    }

    /// Runtime primitive registry snapshot used by this build.
    pub fn primitive_registry_snapshot(
        &self,
    ) -> std::sync::Arc<crate::primitives::PrimitiveRegistry> {
        std::sync::Arc::clone(&self.primitive_registry)
    }

    /// External property descriptors installed on this build.
    pub fn extension_property_specs(&self) -> Vec<crate::extension_context::ExtensionPropertySpec> {
        self.extensions
            .as_ref()
            .map(|ctx| ctx.property_specs().to_vec())
            .unwrap_or_default()
    }

    /// Unified descriptors for extension-only properties.
    pub fn extension_property_descriptors(
        &self,
    ) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        self.extensions
            .as_ref()
            .map(|ctx| ctx.extension_property_descriptors())
            .unwrap_or_default()
    }

    /// Signatures for extension actions installed on this build.
    pub fn extension_action_signatures(&self) -> Vec<animatix_syntax::schema::ActionSignature> {
        self.extensions.as_ref().map(|ctx| ctx.action_signatures()).unwrap_or_default()
    }

    /// Descriptors for extension functions installed on this build.
    pub fn extension_function_descriptors(
        &self,
    ) -> Vec<animatix_syntax::schema::FunctionDescriptor> {
        self.extensions
            .as_ref()
            .map(|ctx| ctx.function_descriptors())
            .unwrap_or_default()
    }

    /// Descriptors for extension services installed on this build.
    pub fn extension_service_descriptors(&self) -> Vec<animatix_syntax::schema::ServiceDescriptor> {
        self.extensions
            .as_ref()
            .map(|ctx| ctx.service_descriptors())
            .unwrap_or_default()
    }

    /// Unified property descriptor view for built-in and extension properties.
    ///
    /// This is the tooling-facing migration point: consumers that need one
    /// property table can use this instead of reading built-in and extension
    /// tables separately.
    pub fn property_descriptors(&self) -> Vec<crate::property_descriptor::PropertyDescriptor> {
        match self.extensions.as_ref() {
            Some(registry) => registry.property_descriptors(),
            None => crate::extension_context::PropertyRegistry::builtin_descriptors(),
        }
    }

    /// Duration of the authored animation in seconds, derived from the latest
    /// keyframe across all tracks, background, and child order animations.
    pub fn duration_seconds(&self) -> f64 {
        let max_ms = self
            .world
            .tracks
            .iter()
            .map(|(_, id)| self.world.max_keyframe_time(id).unwrap_or(0))
            .max()
            .unwrap_or(0)
            .max(self.background_color.max_keyframe_time_ms())
            .max(
                self.world
                    .effects
                    .iter()
                    .filter_map(|(_, chain)| chain.max_keyframe_time())
                    .max()
                    .unwrap_or(0),
            )
            .max(
                self.world
                    .child_order
                    .iter()
                    .map(|(_, component)| component.track.max_keyframe_time_ms())
                    .max()
                    .unwrap_or(0),
            )
            .max(
                self.variable_tracks
                    .values()
                    .map(|t| t.max_keyframe_time_ms())
                    .max()
                    .unwrap_or(0),
            )
            // The scene camera is not a track, but its stamps are content: a
            // scene whose only motion is a push-in ends at the push, not at zero.
            .max(self.camera.max_keyframe_time_ms());
        (max_ms as f64) / 1000.0
    }

    /// Duration declared by `config { duration: N }`, if any.
    pub fn declared_duration_seconds(&self) -> Option<f64> {
        self.declared_duration_s
    }

    /// Interactive pause points in milliseconds marked by `#step` or `pause`.
    pub fn pause_points(&self) -> &[u64] {
        &self.pause_points
    }

    /// Returns the next pause point strictly after `current_time_ms`, if any.
    pub fn step_next(&self, current_time_ms: u64) -> Option<u64> {
        self.pause_points.iter().copied().find(|&t| t > current_time_ms)
    }

    /// Returns the previous pause point strictly before `current_time_ms`, if any.
    pub fn step_prev(&self, current_time_ms: u64) -> Option<u64> {
        self.pause_points.iter().copied().rfind(|&t| t < current_time_ms)
    }

    /// Collects actor labels with non-empty `shared_id` at `time_ms`, keyed by `shared_id`.
    pub fn shared_id_actors(&self, time_ms: u64) -> std::collections::HashMap<String, String> {
        let mut map = std::collections::HashMap::new();
        for (label, entity_id) in self.world.tracks.iter() {
            if let Some(crate::timeline::property_engine::PropertyValue::String(id)) =
                self.world.read_property_value(
                    entity_id,
                    crate::timeline::ActorField::Tagged("shared_id"),
                    time_ms,
                )
            {
                if !id.is_empty() {
                    map.insert(id, label.clone());
                }
            }
        }
        map
    }

    /// How long this timeline *plays*: the declared duration when the file set
    /// one, otherwise the keyframe-inferred length.
    ///
    /// A declared duration overrides rather than extends, so keyframes past it
    /// are never reached — `docs/spec.md` ("sets explicit scene duration
    /// (overrides keyframe-inferred duration)") and the per-scene composition
    /// path both say so, and this is the single-scene equivalent. Build emits a
    /// `duration-shorter-than-content` warning in that case so the truncation is
    /// not silent.
    ///
    /// Editing surfaces that must reach every keyframe — the GUI timeline strip,
    /// the keyframe table — keep using [`Timeline::duration_seconds`], which
    /// stays the inferred extent.
    pub fn playback_duration_seconds(&self) -> f64 {
        self.declared_duration_s.unwrap_or_else(|| self.duration_seconds())
    }

    /// Returns all keyframe time positions across all tracks, in seconds.
    /// Used by the GUI timeline scrubber to show keyframe markers.
    ///
    /// Uses the property registry to discover all applicable fields per actor kind,
    /// plus cross-track sources (background_color, child-order tracks,
    /// variable_tracks)
    /// and dynamic plot parameter tracks that the registry cannot enumerate statically.
    pub fn keyframe_times_s(&self) -> Vec<f64> {
        let mut times_ms: BTreeSet<u64> = BTreeSet::new();
        for (_, entity_id) in self.world.tracks.iter() {
            // Registry-driven: covers all applicable fields for this actor kind
            for &time_s in
                collect_all_keyframe_times(entity_id, self.world.effects(entity_id), &self.world)
                    .iter()
            {
                let ms = (time_s * 1000.0) as u64;
                times_ms.insert(ms);
            }
            // D4 exception: dynamic plot parameter tracks (not statically representable)
            if let Some(param_tracks) = self.world.plot_param_tracks(entity_id) {
                for pt in param_tracks.values() {
                    times_ms.extend(pt.keyframes.keys().copied());
                }
            }
            // Tagged union tracks are registry-representable, but include them
            // directly as a defensive catch for properties without a schema.
            if let Some(tagged) = self.world.tagged_tracks.try_get(entity_id) {
                for pt in tagged.values().flatten() {
                    times_ms.extend(pt.keyframes.keys().copied());
                }
            }
        }
        // Cross-track sources
        times_ms.extend(self.background_color.keyframes.keys().copied());
        for (_, component) in self.world.child_order.iter() {
            times_ms.extend(component.track.keyframes.keys().copied());
        }
        for t in self.variable_tracks.values() {
            times_ms.extend(t.keyframes.keys().copied());
        }
        times_ms.into_iter().map(|ms| ms as f64 / 1000.0).collect()
    }

    /// Returns true if an actor with the given label exists.
    pub fn has_actor(&self, label: &str) -> bool {
        self.world.tracks.contains_key(label)
    }

    /// Returns an iterator over all track labels.
    pub fn actor_labels(&self) -> impl Iterator<Item = &String> {
        self.world.tracks.keys()
    }

    /// Labels of `label`'s direct children, in authored/insertion order.
    ///
    /// Label-boundary helper: build diagnostics, action target resolution and
    /// the GUI use it; the frame path walks `world.hierarchy(id).children`.
    pub fn child_labels(&self, label: &str) -> impl Iterator<Item = &str> {
        self.world.get_id(label).into_iter().flat_map(|id| self.world.child_labels(id))
    }

    /// Finds the common parent container of two child labels.
    /// Returns `None` if no shared parent exists.
    pub fn find_common_parent(&self, child_a: &str, child_b: &str) -> Option<String> {
        let child_a = self.world.get_id(child_a)?;
        let child_b = self.world.get_id(child_b)?;
        self.world
            .tracks
            .iter()
            .find(|(_, entity_id)| {
                let children = &self.world.hierarchy(*entity_id).children;
                children.contains(&child_a) && children.contains(&child_b)
            })
            .map(|(label, _)| label.clone())
    }

    /// Returns the effective child order for a container at the given time,
    /// as actor labels.
    ///
    /// Label-boundary helper: the animated order lives on the container's
    /// [`ChildOrderComponent`] as [`EntityId`]s; ids are mapped to labels here
    /// so the GUI and action diagnostics keep their label contract.
    /// [`Self::get_child_order_ids`] is the id-preserving form used by layout.
    pub fn get_child_order(&self, container_label: &str, time_ms: u64) -> Vec<String> {
        self.get_child_order_ids(container_label, time_ms)
            .into_iter()
            .filter_map(|id| self.world.label(id))
            .map(str::to_string)
            .collect()
    }

    /// Returns the effective child order for a container at the given time, as
    /// entity ids.
    ///
    /// Samples the container's [`ChildOrderComponent`]; an absent component or
    /// an empty track falls back to the authored
    /// [`ContainerMetadata::child_order`] (`Vec<EntityId>` too). The frame path
    /// keeps ids and resolves labels only at the Taffy boundary; the GUI/action
    /// boundary uses [`Self::get_child_order`].
    pub fn get_child_order_ids(&self, container_label: &str, time_ms: u64) -> Vec<EntityId> {
        if let Some(component) = self.child_order(container_label) {
            let value = component.track.evaluate(time_ms);
            if !value.is_empty() {
                return value;
            }
        }
        self.container_layout(container_label)
            .map(|metadata| metadata.child_order.clone())
            .unwrap_or_default()
    }

    /// Borrow a container's animated child-order component, if it has one.
    pub fn child_order(&self, container_label: &str) -> Option<&ChildOrderComponent> {
        self.entity(container_label)?.child_order()
    }

    /// Mutably borrow a container's animated child-order component.
    ///
    /// Invalidates the frame cache before returning, because reordering writes
    /// change layout output.
    pub fn child_order_mut(&mut self, container_label: &str) -> Option<&mut ChildOrderComponent> {
        self.invalidate_frame_cache();
        let id = self.world.get_id(container_label)?;
        self.world.child_order.get_mut(id)
    }

    /// Inserts or replaces a container's animated child-order track.
    ///
    /// Returns `false` when no such entity exists (the action targeted a label
    /// with no track). Invalidates the frame cache, like every component write.
    pub fn set_child_order_track(
        &mut self,
        container_label: &str,
        track: PropertyTrack<Vec<EntityId>>,
    ) -> bool {
        self.invalidate_frame_cache();
        let Some(id) = self.world.get_id(container_label) else {
            tracing::warn!("child-order track for '{container_label}' dropped: no such actor");
            return false;
        };
        self.world.child_order.insert(id, ChildOrderComponent { track });
        true
    }

    /// Iterate every animated child-order track, keyed by entity id.
    pub fn child_orders_iter(&self) -> impl Iterator<Item = (EntityId, &ChildOrderComponent)> + '_ {
        self.world.child_order.iter()
    }
}

impl Timeline {
    /// Whether the named actor is visible in the preview/export.
    pub fn is_actor_visible(&self, label: &str) -> bool {
        self.world
            .tracks
            .get_id(label)
            .is_some_and(|id| self.world.meta.get(id).is_some_and(|m| m.visible))
    }

    /// Whether the named actor is locked against selection/dragging.
    pub fn is_actor_locked(&self, label: &str) -> bool {
        self.world
            .tracks
            .get_id(label)
            .is_some_and(|id| self.world.meta.get(id).is_some_and(|m| m.locked))
    }
}
