use super::*;

impl AnimationTrack {
    /// Get an immutable reference to the track field identified by `field`.
    ///
    /// Every built-in property tier now lives in a `World` component —
    /// geometry, style, shape, text, highlight, tagged unions and the payload —
    /// so a bare track resolves no field. World-aware callers read through
    /// `World::fields(id)`, which bundles the components before dispatching.
    pub fn field_ref(&self, _field: ActorField) -> Option<TrackFieldRef<'_>> {
        None
    }

    /// Get a mutable reference to the track field identified by `field`.
    ///
    /// See [`Self::field_ref`]: property tiers are `World` components, so
    /// world-aware callers write through `World::fields_mut(id)`.
    pub fn field_mut(&mut self, _field: ActorField) -> Option<TrackFieldMut<'_>> {
        None
    }

    /// Returns `true` if the property track for `field` is currently
    /// interpolating between two keyframes at the given time (the next
    /// keyframe after `time_ms` uses a non-Linear easing, distinguishing
    /// real animation targets from build-time snapshot scaffolding).
    ///
    /// This is the frame-time definition of "being driven by keyframes"
    /// used by the `_animating_*` environment flags.
    pub fn is_field_currently_animating(&self, field: ActorField, time_ms: u64) -> bool {
        self.field_ref(field).is_some_and(|field| field.is_currently_animating(time_ms))
    }

    /// The one `property name → ActorField` table.
    ///
    /// Three introspection queries (`has_keyframe_at`, `has_keyframes_for`,
    /// `list_keyframes`) each carried a copy of this mapping, verified identical
    /// apart from the fall-through arm — which is what each caller keeps for itself
    /// below. A fourth copy is how a property ends up answering one query and not
    /// another.
    pub(crate) fn field_for_property(property: &str) -> Option<ActorField> {
        use ActorField::*;
        Some(match property {
            "position" => Position,
            "motion_offset" => MotionOffset,
            "size" => Size,
            "layout_size" => LayoutSize,
            "rotation" => Rotation,
            "scale" => Scale,
            "transform" => Transform,
            "color" => Color,
            "opacity" => Opacity,
            "stroke_width" => StrokeWidth,
            "stroke_progress" => StrokeProgress,
            "dash_offset" => DashOffset,
            "dash_pattern" => DashPattern,
            "fill_gradient" => FillGradient,
            "stroke_gradient" => StrokeGradient,
            "gradient_extend" => GradientExtend,
            "gradient_space" => GradientSpace,
            "blend" => Blend,
            "fill_opacity" => FillOpacity,
            "shape_type" => ShapeType,
            "line_from" => LineFrom,
            "line_to" => LineTo,
            "arc_angles" => ArcAngles,
            "corner_radius" => CornerRadius,
            "points" => Points,
            "commands" => Commands,
            "head_size" => HeadSize,
            "text_content" => TextContent,
            "font_family" => FontFamily,
            "font_size" => FontSize,
            "font_weight" => FontWeight,
            "font_style" => FontStyle,
            "line_height" => LineHeight,
            "letter_spacing" => LetterSpacing,
            "word_spacing" => WordSpacing,
            "max_width" => TextMaxWidth,
            "text_align" => TextAlign,
            "text_anchor" => TextAnchor,
            "overflow" => Overflow,
            "language" => Language,
            "placement_mode" => PlacementMode,
            "morph_options" => MorphOptions,
            "legend" => Tagged("legend"),
            "url" => ImageData,
            _ => return None,
        })
    }

    /// Check if a keyframe exists for the given property at exactly `time_ms`.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframe_at(&self, property: &str, time_ms: u64) -> bool {
        let Some(field) = Self::field_for_property(property) else {
            return false;
        };

        self.field_ref(field).is_some_and(|f| f.has_keyframe_at(time_ms))
    }

    /// Check if the property has any keyframes at all (regardless of time).
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    pub fn has_keyframes_for(&self, property: &str) -> bool {
        let Some(field) = Self::field_for_property(property) else {
            return false;
        };

        self.field_ref(field).is_some_and(|f| f.keyframe_count() > 0)
    }

    /// True when the author animated `property`: the track holds at least two
    /// keyframes that do not all carry the same value.
    ///
    /// [`Self::has_keyframes_for`] cannot answer this. The build stamps a
    /// keyframe for most *declared* properties (`insert_end_keyframes`, plus a
    /// start keyframe when the scene has a duration), so a non-zero keyframe
    /// count says the property was written down, not that anything moves. That
    /// is why `always-overrides-keyframes` fired on scenes that animate nothing:
    /// a `dash_pattern` in a declaration seeds a constant `dash_offset`, and an
    /// `always` writing it was reported as overriding "keyframe animation".
    ///
    /// A single keyframe is treated as not-animated, and that is a deliberate
    /// loss. It would be nice to keep warning on `#1s box.opacity = 0.8` beside
    /// an `always` writing opacity, but a lone keyframe is bit-for-bit what
    /// declaration seeding produces — the seeder writes the *declared* value, not
    /// the registry default, so even `size: (100, 60)` lands as one keyframe at a
    /// value no default matches. Telling the two apart needs keyframe provenance,
    /// which tracks do not carry (the storage is `InlineKeyframes<T>`).
    /// A constant value costs nothing to override, so the false positives cost
    /// more than the case is worth.
    pub(crate) fn is_property_animated(&self, property: &str) -> bool {
        let times = self.list_keyframes(property);
        if times.len() < 2 {
            return false;
        }
        let Some(schema) = crate::timeline::property_registry::lookup_property(property) else {
            return true;
        };
        // Fields whose tracks cannot be sampled as values (path, image and
        // binding tracks) keep the coarser rule: two keyframes is animation.
        let Some(field) = self.field_ref(schema.field) else {
            return true;
        };
        let Some(base) = field.evaluate_value(times[0]) else {
            return true;
        };
        times[1..]
            .iter()
            .any(|time_ms| field.evaluate_value(*time_ms).is_none_or(|value| value != base))
    }

    /// List all keyframe times (in ms) for the given property.
    /// The `property` parameter is a string name like `"position"`, `"opacity"`, etc.
    /// Returns a sorted, deduplicated list of timestamps.
    pub fn list_keyframes(&self, property: &str) -> Vec<u64> {
        let Some(field) = Self::field_for_property(property) else {
            return Vec::new();
        };

        let mut times: Vec<u64> =
            self.field_ref(field).map(|f| f.keyframe_times()).unwrap_or_default();
        times.sort_unstable();
        times.dedup();
        times
    }
}

// ─────────────────────────────────────────────────────────────
// Free functions: Property value reading & keyframe introspection
// ─────────────────────────────────────────────────────────────

/// Read the current value of a property from a track at the given time.
/// Returns `None` if the property has no track (not set on this actor).
pub fn read_property_value(
    track: &AnimationTrack,
    field: ActorField,
    time_ms: u64,
) -> Option<PropertyValue> {
    crate::timeline::world::TrackFields::from_track(track).read_property_value(field, time_ms)
}

/// Read a property value, falling back to the schema default if the track
/// has no value for this property.
pub fn read_property_value_or_default(
    track: &AnimationTrack,
    schema: &PropertySchema,
    time_ms: u64,
) -> PropertyValue {
    read_property_value(track, schema.field, time_ms).unwrap_or_else(|| {
        (schema.default_value)(&animatix_std::caps_for_type(&track.actor_type).unwrap_or_default())
    })
}

/// Returns whether a property has any keyframes on the given track.
pub fn property_has_keyframes(track: &AnimationTrack, field: ActorField) -> bool {
    crate::timeline::world::TrackFields::from_track(track).property_has_keyframes(field)
}

/// Returns whether a property has a keyframe at exactly the given time.
pub fn property_has_keyframe_at(track: &AnimationTrack, field: ActorField, time_ms: u64) -> bool {
    crate::timeline::world::TrackFields::from_track(track).property_has_keyframe_at(field, time_ms)
}

/// Returns the number of keyframes for a property on the given track.
pub fn property_keyframe_count(track: &AnimationTrack, field: ActorField) -> usize {
    crate::timeline::world::TrackFields::from_track(track).property_keyframe_count(field)
}

/// Returns all keyframe times (in ms) for a property, sorted.
pub fn property_keyframe_times(track: &AnimationTrack, field: ActorField) -> Vec<u64> {
    crate::timeline::world::TrackFields::from_track(track).property_keyframe_times(field)
}

/// Returns the easing at a specific keyframe time for a property.
pub fn property_keyframe_easing(
    track: &AnimationTrack,
    field: ActorField,
    time_ms: u64,
) -> Option<Easing> {
    crate::timeline::world::TrackFields::from_track(track).property_keyframe_easing(field, time_ms)
}

#[cfg(test)]
mod payload_tests {
    use super::AnimationTrack;
    use crate::primitives::PrimitivePayload;
    use crate::timeline::Timeline;

    #[test]
    fn rect_track_carries_no_payload() {
        let mut timeline = Timeline::new();
        let id = timeline
            .world
            .spawn_with_track("box", AnimationTrack::new("box".to_string(), "Rect"));
        assert!(timeline.world.payload(id).is_none());
    }

    #[test]
    fn ensure_plot_payload_switches_variant() {
        let mut timeline = Timeline::new();
        let id = timeline
            .world
            .spawn_with_track("curve", AnimationTrack::new("curve".to_string(), "PlotCurve"));
        assert!(timeline.world.payload(id).is_none());

        timeline
            .world
            .ensure_plot_payload(id)
            .plot_param_tracks
            .insert("freq".to_string(), super::PropertyTrack::new(2.0));
        assert!(matches!(timeline.world.payload(id), Some(PrimitivePayload::Plot(_))));
        assert_eq!(timeline.world.plot_param_track(id, "freq").map(|t| t.default_value), Some(2.0));

        // A second `ensure` must hand back the same payload, not reset it.
        assert_eq!(timeline.world.ensure_plot_payload(id).plot_param_tracks.len(), 1);
        assert_eq!(timeline.world.plot_param_track(id, "freq").map(|t| t.default_value), Some(2.0));
    }

    #[test]
    fn svg_and_image_payloads_are_distinct() {
        let mut timeline = Timeline::new();
        let id = timeline
            .world
            .spawn_with_track("icon", AnimationTrack::new("icon".to_string(), "Svg"));
        timeline
            .world
            .ensure_svg_payload(id)
            .svg_paths
            .push(crate::timeline::VelloPath::default());
        assert!(matches!(timeline.world.payload(id), Some(PrimitivePayload::Svg(_))));
        assert!(timeline.world.image_payload(id).is_none());
        assert!(timeline.world.image_track(id).is_none());
        assert_eq!(timeline.world.svg_paths(id).len(), 1);
    }
}
