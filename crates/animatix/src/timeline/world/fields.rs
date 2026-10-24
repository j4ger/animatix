//! Borrowed bundles of the per-entity property tiers.
//!
//! The property engine still reads and writes the tier structs through
//! `AnimationTrack` today. [`TrackFields`] / [`TrackFieldsMut`] are the seam
//! that lets a caller assemble the same view from `World` components once a
//! tier moves out of the track: `World::fields(id)` is the only constructor,
//! so moving a tier changes this file and nothing else.
//!
//! See `docs/ecs.md` §6 ("Tier-migration prerequisite").

use std::collections::HashMap;

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
    pub highlight: &'a HighlightTracks,
    /// Tagged union tracks keyed by property name.
    pub tagged_tracks: &'a HashMap<String, Option<PropertyTrack<PropertyValue>>>,
    /// Primitive payload (plot/SVG/image).
    pub payload: &'a PrimitivePayload,
    /// Capability projection for this entity.
    pub caps: ActorCaps,
    /// Registry-derived property plan for this entity.
    pub property_plan: &'a PropertyPlan,
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
    pub highlight: &'a mut HighlightTracks,
    /// Tagged union tracks.
    pub tagged_tracks: &'a mut HashMap<String, Option<PropertyTrack<PropertyValue>>>,
    /// Primitive payload.
    pub payload: &'a mut PrimitivePayload,
    /// Capability projection for this entity.
    pub caps: ActorCaps,
    /// Registry-derived property plan for this entity.
    pub property_plan: &'a mut PropertyPlan,
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
            highlight: &track.highlight,
            tagged_tracks: &track.tagged_tracks,
            payload: &track.payload,
            caps: track.caps,
            property_plan: &track.property_plan,
        })
    }

    /// Assemble the mutable tier bundle for `id`, when the entity exists.
    pub fn fields_mut(&mut self, id: EntityId) -> Option<TrackFieldsMut<'_>> {
        let track = self.tracks.get_by_id_mut(id)?;
        Some(TrackFieldsMut {
            geometry: &mut track.geometry,
            style: &mut track.style,
            shape: &mut track.shape,
            text: &mut track.text,
            highlight: &mut track.highlight,
            tagged_tracks: &mut track.tagged_tracks,
            payload: &mut track.payload,
            caps: track.caps,
            property_plan: &mut track.property_plan,
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
            highlight: &track.highlight,
            tagged_tracks: &track.tagged_tracks,
            payload: &track.payload,
            caps: track.caps,
            property_plan: &track.property_plan,
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
            HighlightColor => TrackFieldRef::Vec4(&self.highlight.highlight_color),
            HighlightOpacity => TrackFieldRef::F32(&self.highlight.highlight_opacity),
            HighlightPadding => TrackFieldRef::F32(&self.highlight.highlight_padding),
            HighlightRadius => TrackFieldRef::F32(&self.highlight.highlight_radius),
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
            ImageData => return self.payload.as_image().map(|p| TrackFieldRef::Image(&p.image)),
            PositionBinding => TrackFieldRef::PositionBinding(&self.geometry.position_binding),
            Tagged(name) => {
                return self
                    .tagged_tracks
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
            highlight: &mut track.highlight,
            tagged_tracks: &mut track.tagged_tracks,
            payload: &mut track.payload,
            caps: track.caps,
            property_plan: &mut track.property_plan,
        }
    }

    /// Mutable reference to one registry-backed property field.
    pub fn field_mut(
        self,
        field: crate::timeline::property_registry::ActorField,
    ) -> Option<crate::timeline::dispatch::TrackFieldMut<'a>> {
        use crate::timeline::dispatch::TrackFieldMut;
        use crate::timeline::property_registry::ActorField;
        if let ActorField::Tagged(name) = field {
            return Some(TrackFieldMut::Tagged(
                name,
                self.tagged_tracks.entry(name.to_string()).or_default(),
            ));
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
            HighlightColor => TrackFieldMut::Vec4(&mut self.highlight.highlight_color),
            HighlightOpacity => TrackFieldMut::F32(&mut self.highlight.highlight_opacity),
            HighlightPadding => TrackFieldMut::F32(&mut self.highlight.highlight_padding),
            HighlightRadius => TrackFieldMut::F32(&mut self.highlight.highlight_radius),
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
            ImageData => TrackFieldMut::Image(&mut self.payload.ensure_image().image),
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
            && let Some(slot) = self.property_plan.get(id)
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
