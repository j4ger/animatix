//! Borrowed bundles of the per-entity property tiers.
//!
//! The property engine still reads and writes the tier structs through
//! `AnimationTrack` today. [`TrackFields`] / [`TrackFieldsMut`] are the seam
//! that lets a caller assemble the same view from `World` components once a
//! tier moves out of the track: `World::fields(id)` is the only constructor,
//! so moving a tier changes this file and nothing else.
//!
//! See `docs/ecs.md` §6 ("Tier-migration prerequisite").

use crate::primitives::PrimitivePayload;
use crate::timeline::actor_caps::ActorCaps;
use crate::timeline::animation_track::{
    GeometryTracks, HighlightTracks, ShapeTracks, StyleTracks, TextTracks,
};
use crate::timeline::plan::PropertyPlan;
use crate::timeline::property_engine::PropertyValue;
use crate::timeline::property_track::PropertyTrack;

use super::{EntityId, World};

/// Immutable view of one entity's property tiers.
pub struct TrackFields<'a> {
    /// Geometry tier (at/position/size/rotation/…).
    pub geometry: &'a GeometryTracks,
    /// Style tier (color/opacity/stroke/…).
    pub style: &'a StyleTracks,
    /// Shape tier (shape_type/line_from/…).
    pub shape: &'a ShapeTracks,
    /// Text tier.
    pub text: &'a TextTracks,
    /// Equation-fragment highlight tier.
    pub highlight: Option<&'a HighlightTracks>,
    /// Tagged union tracks keyed by property name.
    pub tagged_tracks: Option<&'a super::TaggedTracksComponent>,
    /// Primitive payload (plot/SVG/image).
    pub payload: Option<&'a PrimitivePayload>,
    /// Actor label (diagnostics).
    pub label: &'a str,
    /// Capability projection for this entity.
    pub caps: ActorCaps,
    /// Registry-derived property plan for this entity.
    pub property_plan: Option<&'a PropertyPlan>,
}

/// Mutable view of one entity's property tiers.
pub struct TrackFieldsMut<'a> {
    /// Geometry tier.
    pub geometry: &'a mut GeometryTracks,
    /// Style tier.
    pub style: &'a mut StyleTracks,
    /// Shape tier.
    pub shape: &'a mut ShapeTracks,
    /// Text tier.
    pub text: &'a mut TextTracks,
    /// Highlight tier.
    pub highlight: Option<&'a mut HighlightTracks>,
    /// Tagged union tracks.
    pub tagged_tracks: Option<&'a mut super::TaggedTracksComponent>,
    /// Primitive payload.
    pub payload: Option<&'a mut PrimitivePayload>,
    /// Actor label (diagnostics).
    pub label: &'a str,
    /// Capability projection for this entity.
    pub caps: ActorCaps,
    /// Registry-derived property plan for this entity.
    pub property_plan: Option<&'a mut PropertyPlan>,
}

impl World {
    /// Assemble the immutable tier bundle for `id`, when the entity exists.
    pub fn fields(&self, id: EntityId) -> Option<TrackFields<'_>> {
        let track = self.tracks.get_by_id(id)?;
        Some(TrackFields {
            geometry: &track.geometry,
            style: &track.style,
            shape: &track.shape,
            text: &track.text,
            highlight: self.highlight.try_get(id),
            tagged_tracks: self.tagged_tracks.try_get(id),
            payload: self.payload.get(id),
            label: &track.label,
            caps: track.caps,
            property_plan: self.property_plan.try_get(id),
        })
    }

    /// Assemble the mutable tier bundle for `id`, when the entity exists.
    pub fn fields_mut(&mut self, id: EntityId) -> Option<TrackFieldsMut<'_>> {
        self.ensure_len();
        let track = self.tracks.get_by_id_mut(id)?;
        Some(TrackFieldsMut {
            geometry: &mut track.geometry,
            style: &mut track.style,
            shape: &mut track.shape,
            text: &mut track.text,
            highlight: self.highlight.try_get_mut(id),
            tagged_tracks: self.tagged_tracks.try_get_mut(id),
            payload: self.payload.get_mut(id),
            label: &track.label,
            caps: track.caps,
            property_plan: self.property_plan.try_get_mut(id),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timeline::Timeline;

    #[test]
    fn fields_expose_the_entity_tiers() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        timeline.world_mut().tracks.get_by_id_mut(id).unwrap().style.opacity =
            Some(PropertyTrack::new(0.5));

        let fields = timeline.world().fields(id).expect("entity fields");
        assert_eq!(fields.style.opacity.as_ref().unwrap().default_value, 0.5);
        assert!(timeline.world().fields(EntityId(999)).is_none());
    }

    #[test]
    fn mutable_fields_write_through_to_the_track() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");

        {
            let fields = timeline.world_mut().fields_mut(id).expect("entity fields");
            fields.style.opacity = Some(PropertyTrack::new(0.25));
            fields.geometry.size = Some(PropertyTrack::new([10.0, 20.0]));
        }

        let track = timeline.world().tracks.get_by_id(id).unwrap();
        assert_eq!(track.style.opacity.as_ref().unwrap().default_value, 0.25);
        assert_eq!(track.geometry.size.as_ref().unwrap().default_value, [10.0, 20.0]);
    }
}

impl<'a> TrackFields<'a> {
    /// Bundle the tiers of one track.
    pub fn from_track(track: &'a crate::timeline::AnimationTrack) -> Self {
        Self {
            geometry: &track.geometry,
            style: &track.style,
            shape: &track.shape,
            text: &track.text,
            highlight: None,
            tagged_tracks: None,
            payload: None,
            label: &track.label,
            caps: track.caps,
            property_plan: None,
        }
    }

    /// Immutable reference to one registry-backed property field.
    pub fn field_ref(
        &self,
        field: crate::timeline::property_registry::ActorField,
    ) -> Option<crate::timeline::dispatch::TrackFieldRef<'a>> {
        use crate::timeline::dispatch::TrackFieldRef;
        use crate::timeline::property_registry::ActorField::*;
        Some(match field {
            Position => TrackFieldRef::Vec2(&self.geometry.position),
            MotionOffset => TrackFieldRef::Vec2(&self.geometry.motion_offset),
            Size => TrackFieldRef::Vec2(&self.geometry.size),
            LayoutSize => TrackFieldRef::Vec2(&self.geometry.layout_size),
            Rotation => TrackFieldRef::F32(&self.geometry.rotation),
            Scale => TrackFieldRef::F32(&self.geometry.scale),
            Transform => TrackFieldRef::Transform(&self.geometry.transform),
            Color => TrackFieldRef::Vec4(&self.style.color),
            Opacity => TrackFieldRef::F32(&self.style.opacity),
            StrokeWidth => TrackFieldRef::F32(&self.style.stroke_width),
            StrokeColor => TrackFieldRef::Vec4(&self.style.stroke_color),
            StrokeProgress => TrackFieldRef::F32(&self.style.stroke_progress),
            FillOpacity => TrackFieldRef::F32(&self.style.fill_opacity),
            ShapeType => TrackFieldRef::ShapeType(&self.shape.shape_type),
            LineFrom => TrackFieldRef::Vec2(&self.shape.line_from),
            LineTo => TrackFieldRef::Vec2(&self.shape.line_to),
            HeadSize => TrackFieldRef::F32(&self.shape.head_size),
            LineCap => TrackFieldRef::U32(&self.style.line_cap),
            LineJoin => TrackFieldRef::U32(&self.style.line_join),
            ArcAngles => TrackFieldRef::Vec2(&self.shape.arc_angles),
            CornerRadius => TrackFieldRef::F32(&self.shape.corner_radius),
            Points => TrackFieldRef::PointList(&self.shape.points),
            DashPattern => TrackFieldRef::F32List(&self.style.dash_pattern),
            FillGradient => TrackFieldRef::Gradient(&self.style.fill_gradient),
            StrokeGradient => TrackFieldRef::Gradient(&self.style.stroke_gradient),
            GradientExtend => TrackFieldRef::String(&self.style.gradient_extend),
            GradientSpace => TrackFieldRef::String(&self.style.gradient_space),
            DashOffset => TrackFieldRef::F32(&self.style.dash_offset),
            Blend => TrackFieldRef::String(&self.style.blend),
            Commands => TrackFieldRef::CommandList(&self.shape.commands),
            TextContent => TrackFieldRef::String(&self.text.text_content),
            TextMaxWidth => TrackFieldRef::F32(&self.text.text_max_width),
            TextAlign => TrackFieldRef::String(&self.text.text_align),
            TextAnchor => TrackFieldRef::String(&self.text.text_anchor),
            Overflow => TrackFieldRef::String(&self.text.overflow),
            Language => TrackFieldRef::String(&self.text.language),
            FontFamily => TrackFieldRef::String(&self.text.font_family),
            FontSize => TrackFieldRef::F32(&self.text.font_size),
            CharProgress => TrackFieldRef::F32(&self.text.char_progress),
            PlacementMode => TrackFieldRef::PlacementMode(&self.geometry.placement_mode),
            MorphOptions => TrackFieldRef::MorphOptions(&self.style.morph_options),
            Ascent => TrackFieldRef::F32(&self.text.ascent),
            Descent => TrackFieldRef::F32(&self.text.descent),
            Baseline => TrackFieldRef::F32(&self.text.baseline),
            HighlightColor => TrackFieldRef::Vec4(&self.highlight?.highlight_color),
            HighlightOpacity => TrackFieldRef::F32(&self.highlight?.highlight_opacity),
            HighlightPadding => TrackFieldRef::F32(&self.highlight?.highlight_padding),
            HighlightRadius => TrackFieldRef::F32(&self.highlight?.highlight_radius),
            FontWeight => TrackFieldRef::F32(&self.text.font_weight),
            FontStyle => TrackFieldRef::String(&self.text.font_style),
            LineHeight => TrackFieldRef::F32(&self.text.line_height),
            LetterSpacing => TrackFieldRef::F32(&self.text.letter_spacing),
            WordSpacing => TrackFieldRef::F32(&self.text.word_spacing),
            MinWidth => TrackFieldRef::F32(&self.geometry.min_width),
            MinHeight => TrackFieldRef::F32(&self.geometry.min_height),
            MaxHeight => TrackFieldRef::F32(&self.geometry.max_height),
            LabelAt => TrackFieldRef::Vec2(&self.geometry.label_at),
            CalloutTarget => TrackFieldRef::String(&self.geometry.callout_target),
            CalloutPlace => TrackFieldRef::CalloutPlace(&self.geometry.callout_place),
            CalloutStandoff => TrackFieldRef::F32(&self.geometry.callout_standoff),
            CalloutToOffset => TrackFieldRef::Vec2(&self.geometry.callout_to_offset),
            VectorPaths => TrackFieldRef::VectorPaths(&self.shape.vector_paths),
            TextPaths => TrackFieldRef::TextPaths(&self.text.text_paths),
            ImageData => return self.payload?.as_image().map(|p| TrackFieldRef::Image(&p.image)),
            PositionBinding => TrackFieldRef::PositionBinding(&self.geometry.position_binding),
            Tagged(name) => {
                return self
                    .tagged_tracks?
                    .get(name)
                    .map(|track| TrackFieldRef::Tagged(name, track));
            },
            AudioVolume => TrackFieldRef::F32(&self.geometry.volume),
            SvgPaths | AudioSource | PositionBindingGroup | VectorShapeGroup | PlotDomainGroup
            | ContainerLayoutGroup | NoStorage => return None,
        })
    }
}

impl<'a> TrackFieldsMut<'a> {
    /// Bundle the tiers of one track, mutably.
    pub fn from_track(track: &'a mut crate::timeline::AnimationTrack) -> Self {
        Self {
            geometry: &mut track.geometry,
            style: &mut track.style,
            shape: &mut track.shape,
            text: &mut track.text,
            highlight: None,
            tagged_tracks: None,
            payload: None,
            caps: track.caps,
            label: &track.label,
            property_plan: None,
        }
    }

    /// Mutable reference to one registry-backed property field.
    pub fn field_mut(
        &mut self,
        field: crate::timeline::property_registry::ActorField,
    ) -> Option<crate::timeline::dispatch::TrackFieldMut<'_>> {
        use crate::timeline::dispatch::TrackFieldMut;
        use crate::timeline::property_registry::ActorField;
        if let ActorField::Tagged(name) = field {
            let tracks = self.tagged_tracks.as_mut()?;
            return Some(TrackFieldMut::Tagged(name, tracks.entry(name.to_string()).or_default()));
        }
        use ActorField::*;
        Some(match field {
            Position => TrackFieldMut::Vec2(&mut self.geometry.position),
            MotionOffset => TrackFieldMut::Vec2(&mut self.geometry.motion_offset),
            Size => TrackFieldMut::Vec2(&mut self.geometry.size),
            LayoutSize => TrackFieldMut::Vec2(&mut self.geometry.layout_size),
            Rotation => TrackFieldMut::F32(&mut self.geometry.rotation),
            Scale => TrackFieldMut::F32(&mut self.geometry.scale),
            Transform => TrackFieldMut::Transform(&mut self.geometry.transform),
            Color => TrackFieldMut::Vec4(&mut self.style.color),
            Opacity => TrackFieldMut::F32(&mut self.style.opacity),
            StrokeWidth => TrackFieldMut::F32(&mut self.style.stroke_width),
            StrokeColor => TrackFieldMut::Vec4(&mut self.style.stroke_color),
            StrokeProgress => TrackFieldMut::F32(&mut self.style.stroke_progress),
            FillOpacity => TrackFieldMut::F32(&mut self.style.fill_opacity),
            ShapeType => TrackFieldMut::ShapeType(&mut self.shape.shape_type),
            LineFrom => TrackFieldMut::Vec2(&mut self.shape.line_from),
            LineTo => TrackFieldMut::Vec2(&mut self.shape.line_to),
            HeadSize => TrackFieldMut::F32(&mut self.shape.head_size),
            LineCap => TrackFieldMut::U32(&mut self.style.line_cap),
            LineJoin => TrackFieldMut::U32(&mut self.style.line_join),
            ArcAngles => TrackFieldMut::Vec2(&mut self.shape.arc_angles),
            CornerRadius => TrackFieldMut::F32(&mut self.shape.corner_radius),
            Points => TrackFieldMut::PointList(&mut self.shape.points),
            DashPattern => TrackFieldMut::F32List(&mut self.style.dash_pattern),
            FillGradient => TrackFieldMut::Gradient(&mut self.style.fill_gradient),
            StrokeGradient => TrackFieldMut::Gradient(&mut self.style.stroke_gradient),
            GradientExtend => TrackFieldMut::String(&mut self.style.gradient_extend),
            GradientSpace => TrackFieldMut::String(&mut self.style.gradient_space),
            DashOffset => TrackFieldMut::F32(&mut self.style.dash_offset),
            Blend => TrackFieldMut::String(&mut self.style.blend),
            Commands => TrackFieldMut::CommandList(&mut self.shape.commands),
            TextContent => TrackFieldMut::String(&mut self.text.text_content),
            TextMaxWidth => TrackFieldMut::F32(&mut self.text.text_max_width),
            TextAlign => TrackFieldMut::String(&mut self.text.text_align),
            TextAnchor => TrackFieldMut::String(&mut self.text.text_anchor),
            Overflow => TrackFieldMut::String(&mut self.text.overflow),
            Language => TrackFieldMut::String(&mut self.text.language),
            FontFamily => TrackFieldMut::String(&mut self.text.font_family),
            FontSize => TrackFieldMut::F32(&mut self.text.font_size),
            CharProgress => TrackFieldMut::F32(&mut self.text.char_progress),
            PlacementMode => TrackFieldMut::PlacementMode(&mut self.geometry.placement_mode),
            MorphOptions => TrackFieldMut::MorphOptions(&mut self.style.morph_options),
            Ascent => TrackFieldMut::F32(&mut self.text.ascent),
            Descent => TrackFieldMut::F32(&mut self.text.descent),
            Baseline => TrackFieldMut::F32(&mut self.text.baseline),
            HighlightColor => TrackFieldMut::Vec4(&mut self.highlight.as_mut()?.highlight_color),
            HighlightOpacity => TrackFieldMut::F32(&mut self.highlight.as_mut()?.highlight_opacity),
            HighlightPadding => TrackFieldMut::F32(&mut self.highlight.as_mut()?.highlight_padding),
            HighlightRadius => TrackFieldMut::F32(&mut self.highlight.as_mut()?.highlight_radius),
            FontWeight => TrackFieldMut::F32(&mut self.text.font_weight),
            FontStyle => TrackFieldMut::String(&mut self.text.font_style),
            LineHeight => TrackFieldMut::F32(&mut self.text.line_height),
            LetterSpacing => TrackFieldMut::F32(&mut self.text.letter_spacing),
            WordSpacing => TrackFieldMut::F32(&mut self.text.word_spacing),
            MinWidth => TrackFieldMut::F32(&mut self.geometry.min_width),
            MinHeight => TrackFieldMut::F32(&mut self.geometry.min_height),
            MaxHeight => TrackFieldMut::F32(&mut self.geometry.max_height),
            LabelAt => TrackFieldMut::Vec2(&mut self.geometry.label_at),
            CalloutTarget => TrackFieldMut::String(&mut self.geometry.callout_target),
            CalloutPlace => TrackFieldMut::CalloutPlace(&mut self.geometry.callout_place),
            CalloutStandoff => TrackFieldMut::F32(&mut self.geometry.callout_standoff),
            CalloutToOffset => TrackFieldMut::Vec2(&mut self.geometry.callout_to_offset),
            VectorPaths => TrackFieldMut::VectorPaths(&mut self.shape.vector_paths),
            TextPaths => TrackFieldMut::TextPaths(&mut self.text.text_paths),
            ImageData => {
                let payload = self.payload.as_mut()?;
                TrackFieldMut::Image(&mut payload.ensure_image().image)
            },
            PositionBinding => TrackFieldMut::PositionBinding(&mut self.geometry.position_binding),
            AudioVolume => TrackFieldMut::F32(&mut self.geometry.volume),
            SvgPaths | AudioSource | PositionBindingGroup | VectorShapeGroup | PlotDomainGroup
            | ContainerLayoutGroup | NoStorage => return None,
            ActorField::Tagged(_) => unreachable!(),
        })
    }
}

#[cfg(test)]
mod field_access_tests {
    use super::*;
    use crate::timeline::AnimationTrack;
    use crate::timeline::dispatch::{TrackFieldMut, TrackFieldRef};
    use crate::timeline::property_registry::ActorField;
    use crate::timeline::property_track::PropertyTrack;

    #[test]
    fn bundle_field_ref_matches_the_track_shape() {
        let track = AnimationTrack::new("box".into(), "Rect");
        let fields = TrackFields::from_track(&track);
        assert!(matches!(fields.field_ref(ActorField::Opacity), Some(TrackFieldRef::F32(_))));
        assert!(matches!(fields.field_ref(ActorField::Size), Some(TrackFieldRef::Vec2(_))));
        assert!(fields.field_ref(ActorField::NoStorage).is_none());
    }

    #[test]
    fn bundle_field_mut_writes_through() {
        let mut track = AnimationTrack::new("box".into(), "Rect");
        TrackFieldsMut::from_track(&mut track)
            .field_mut(ActorField::Opacity)
            .and_then(|field| match field {
                TrackFieldMut::F32(slot) => {
                    *slot = Some(PropertyTrack::new(0.5));
                    Some(())
                },
                _ => None,
            })
            .expect("opacity field");
        assert_eq!(track.style.opacity.as_ref().unwrap().default_value, 0.5);
    }
}

impl<'a> TrackFields<'a> {
    /// Read one property value at `time_ms`, preferring the registry plan for
    /// tagged fields and falling back to the tier track.
    pub fn read_property_value(
        &self,
        field: crate::timeline::property_registry::ActorField,
        time_ms: u64,
    ) -> Option<PropertyValue> {
        use crate::timeline::property_registry::ActorField;
        if let ActorField::Tagged(name) = field
            && name != "legend"
            && name != "callout_place"
            && let Some(id) = crate::timeline::property_registry::property_id(name)
            && let Some(slot) = self.property_plan?.get(id)
            && let Some(value) = slot.track.sample(time_ms)
        {
            return Some(value);
        }
        self.field_ref(field).and_then(|f| f.evaluate_value(time_ms))
    }
}

impl World {
    /// Read one property value for an entity at `time_ms`.
    pub fn read_property_value(
        &self,
        id: EntityId,
        field: crate::timeline::property_registry::ActorField,
        time_ms: u64,
    ) -> Option<PropertyValue> {
        self.fields(id)?.read_property_value(field, time_ms)
    }
}

#[cfg(test)]
mod read_tests {
    use crate::timeline::Timeline;
    use crate::timeline::property_registry::ActorField;
    use crate::timeline::property_track::PropertyTrack;

    #[test]
    fn world_read_property_value_matches_the_track() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        let mut track = PropertyTrack::new(0.25_f32);
        track.add_keyframe(0, 0.25, crate::easing::Easing::Linear);
        track.add_keyframe(100, 0.75, crate::easing::Easing::Linear);
        timeline.world_mut().tracks.get_by_id_mut(id).unwrap().style.opacity = Some(track);

        assert_eq!(
            timeline.world().read_property_value(id, ActorField::Opacity, 0),
            Some(crate::timeline::PropertyValue::F32(0.25))
        );
        assert_eq!(
            timeline.world().read_property_value(id, ActorField::Opacity, 100),
            Some(crate::timeline::PropertyValue::F32(0.75))
        );
    }
}

impl World {
    /// Write one property value for an entity, routing through the tier bundle.
    pub fn write_property_field(
        &mut self,
        id: EntityId,
        field: crate::timeline::property_registry::ActorField,
        value: PropertyValue,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: crate::easing::Easing,
        diagnostics: &mut Vec<crate::diagnostics::Diagnostic>,
    ) {
        if let Some(mut fields) = self.fields_mut(id) {
            crate::timeline::property_engine::write_property_fields(
                &mut fields,
                field,
                value,
                t_start_ms,
                t_end_ms,
                easing,
                diagnostics,
            );
        }
    }
}

#[cfg(test)]
mod write_tests {
    use crate::timeline::Timeline;
    use crate::timeline::property_registry::ActorField;

    #[test]
    fn world_write_property_field_writes_through() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        let mut diagnostics = Vec::new();
        timeline.world_mut().write_property_field(
            id,
            ActorField::Opacity,
            crate::timeline::PropertyValue::F32(0.6),
            0,
            100,
            crate::easing::Easing::Linear,
            &mut diagnostics,
        );
        assert!(diagnostics.is_empty());
        let track = timeline.world().tracks.get_by_id(id).unwrap();
        assert_eq!(track.style.opacity.as_ref().unwrap().evaluate(100), 0.6);
    }
}

impl<'a> TrackFields<'a> {
    fn svg_paths_track_for(
        &self,
        field: crate::timeline::property_registry::ActorField,
    ) -> Option<&'a PropertyTrack<Option<Vec<crate::timeline::VelloPath>>>> {
        if self.caps.vector_paths
            && field == crate::timeline::property_registry::ActorField::ImageData
        {
            self.payload?.as_svg().and_then(|p| p.svg_paths_track.as_ref())
        } else {
            None
        }
    }

    /// Whether the property has any keyframes.
    pub fn property_has_keyframes(
        &self,
        field: crate::timeline::property_registry::ActorField,
    ) -> bool {
        self.property_keyframe_count(field) > 0
    }

    /// Whether the property has a keyframe at exactly `time_ms`.
    pub fn property_has_keyframe_at(
        &self,
        field: crate::timeline::property_registry::ActorField,
        time_ms: u64,
    ) -> bool {
        if let Some(svg) = self.svg_paths_track_for(field) {
            return svg.keyframes.contains_key(&time_ms);
        }
        if let Some(id) = plan_id_for_field(&field)
            && self.property_plan.is_some_and(|plan| plan.has_keyframe_at(id, time_ms))
        {
            return true;
        }
        self.field_ref(field).is_some_and(|f| f.has_keyframe_at(time_ms))
    }

    /// Number of keyframes on the property.
    pub fn property_keyframe_count(
        &self,
        field: crate::timeline::property_registry::ActorField,
    ) -> usize {
        if let Some(svg) = self.svg_paths_track_for(field) {
            return svg.keyframes.len();
        }
        if let Some(id) = plan_id_for_field(&field) {
            let count = self.property_plan.map_or(0, |plan| plan.keyframe_count(id));
            if count > 0 {
                return count;
            }
        }
        self.field_ref(field).map_or(0, |f| f.keyframe_count())
    }

    /// Sorted keyframe times on the property.
    pub fn property_keyframe_times(
        &self,
        field: crate::timeline::property_registry::ActorField,
    ) -> Vec<u64> {
        if let Some(svg) = self.svg_paths_track_for(field) {
            let mut times: Vec<u64> = svg.keyframes.keys().copied().collect();
            times.sort_unstable();
            return times;
        }
        if let Some(id) = plan_id_for_field(&field) {
            let times = self.property_plan.map_or(Vec::new(), |plan| plan.keyframe_times(id));
            if !times.is_empty() {
                return times;
            }
        }
        self.field_ref(field).map_or(Vec::new(), |f| f.keyframe_times())
    }

    /// Easing at one keyframe.
    pub fn property_keyframe_easing(
        &self,
        field: crate::timeline::property_registry::ActorField,
        time_ms: u64,
    ) -> Option<crate::easing::Easing> {
        if let Some(svg) = self.svg_paths_track_for(field) {
            return svg.keyframes.get(&time_ms).map(|(_, easing)| *easing);
        }
        if let Some(id) = plan_id_for_field(&field)
            && let Some(easing) =
                self.property_plan.and_then(|plan| plan.keyframe_easing(id, time_ms))
        {
            return Some(easing);
        }
        self.field_ref(field).and_then(|f| f.keyframe_easing(time_ms))
    }
}

fn plan_id_for_field(
    field: &crate::timeline::property_registry::ActorField,
) -> Option<animatix_syntax::schema::PropertyId> {
    match field {
        crate::timeline::property_registry::ActorField::Tagged(name)
            if *name != "legend" && *name != "callout_place" =>
        {
            crate::timeline::property_registry::property_id(name)
        },
        _ => None,
    }
}

impl World {
    /// Whether one property has any keyframes for an entity.
    pub fn property_has_keyframes(
        &self,
        id: EntityId,
        field: crate::timeline::property_registry::ActorField,
    ) -> bool {
        self.fields(id).is_some_and(|f| f.property_has_keyframes(field))
    }

    /// Easing at one property keyframe.
    pub fn property_keyframe_easing(
        &self,
        id: EntityId,
        field: crate::timeline::property_registry::ActorField,
        time_ms: u64,
    ) -> Option<crate::easing::Easing> {
        self.fields(id).and_then(|f| f.property_keyframe_easing(field, time_ms))
    }

    /// Whether one property has a keyframe at exactly `time_ms`.
    pub fn property_has_keyframe_at(
        &self,
        id: EntityId,
        field: crate::timeline::property_registry::ActorField,
        time_ms: u64,
    ) -> bool {
        self.fields(id).is_some_and(|f| f.property_has_keyframe_at(field, time_ms))
    }

    /// Number of keyframes on one property for an entity.
    pub fn property_keyframe_count(
        &self,
        id: EntityId,
        field: crate::timeline::property_registry::ActorField,
    ) -> usize {
        self.fields(id).map_or(0, |f| f.property_keyframe_count(field))
    }

    /// Sorted keyframe times on one property for an entity.
    pub fn property_keyframe_times(
        &self,
        id: EntityId,
        field: crate::timeline::property_registry::ActorField,
    ) -> Vec<u64> {
        self.fields(id).map_or(Vec::new(), |f| f.property_keyframe_times(field))
    }
}

#[cfg(test)]
mod keyframe_tests {
    use crate::easing::Easing;
    use crate::timeline::Timeline;
    use crate::timeline::property_registry::ActorField;
    use crate::timeline::property_track::PropertyTrack;

    #[test]
    fn world_keyframe_introspection_matches_the_track() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("box", "Rect");
        let mut track = PropertyTrack::new(0.0_f32);
        track.add_keyframe(0, 0.0, Easing::Linear);
        track.add_keyframe(100, 1.0, Easing::Linear);
        timeline.world_mut().tracks.get_by_id_mut(id).unwrap().style.opacity = Some(track);

        assert!(timeline.world().property_has_keyframes(id, ActorField::Opacity));
        assert_eq!(timeline.world().property_keyframe_count(id, ActorField::Opacity), 2);
        assert_eq!(timeline.world().property_keyframe_times(id, ActorField::Opacity), vec![0, 100]);
    }
}

#[cfg(test)]
mod tagged_field_tests {
    use crate::timeline::Timeline;
    use crate::timeline::dispatch::{TrackFieldMut, TrackFieldRef};
    use crate::timeline::property_registry::ActorField;

    #[test]
    fn tagged_fields_use_the_tagged_store() {
        let mut timeline = Timeline::new();
        let id = timeline.world_mut().spawn("legend", "Rect");
        {
            let mut fields = timeline.world_mut().fields_mut(id).expect("entity");
            match fields.field_mut(ActorField::Tagged("legend")) {
                Some(TrackFieldMut::Tagged(name, slot)) => {
                    assert_eq!(name, "legend");
                    *slot = Some(crate::timeline::PropertyTrack::new(
                        crate::timeline::PropertyValue::Bool(true),
                    ));
                },
                _ => panic!("expected a tagged field"),
            }
        }

        let fields = timeline.world().fields(id).expect("entity");
        assert!(matches!(
            fields.field_ref(ActorField::Tagged("legend")),
            Some(TrackFieldRef::Tagged("legend", _))
        ));
    }
}
