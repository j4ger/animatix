//! Build-time property plans and dynamic property tracks.
//!
//! This module is the performance-oriented counterpart to the shared schema.
//! Names are resolved to [`animatix_syntax::schema::PropertyId`] once at build time. Frame-time access
//! then walks a compact plan and dispatches through a finite [`DynTrack`] enum
//! instead of doing String hash lookups.

use animatix_syntax::schema::{PropertyId, PropertyValueKind};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::easing::Easing;
use crate::timeline::property_registry::{PROPERTY_REGISTRY, ValueType};
use crate::timeline::property_track::PropertyTrack;
use crate::timeline::{ActorKindId, PropertyValue};

/// Finite value kinds understood by dynamic property tracks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PropertyKind {
    /// 32-bit float.
    F32,
    /// 32-bit unsigned integer.
    U32,
    /// Boolean flag.
    Bool,
    /// 2D vector.
    Vec2,
    /// 4D vector / color.
    Vec4,
    /// String.
    String,
    /// List of 2D points.
    PointList,
    /// Any finite property value.
    Generic,
}

impl From<PropertyValueKind> for PropertyKind {
    fn from(kind: PropertyValueKind) -> Self {
        match kind {
            PropertyValueKind::F32 => Self::F32,
            PropertyValueKind::U32 => Self::U32,
            PropertyValueKind::Bool => Self::Bool,
            PropertyValueKind::Vec2 => Self::Vec2,
            PropertyValueKind::Vec4 => Self::Vec4,
            PropertyValueKind::String => Self::String,
            PropertyValueKind::PointList => Self::PointList,
            PropertyValueKind::Generic => Self::Generic,
        }
    }
}

/// A property slot in an actor plan.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PropertySlot {
    /// Stable property id.
    pub id: PropertyId,
    /// Declared value kind.
    pub kind: PropertyKind,
    /// Dynamic track storage.
    pub track: DynTrack,
}

/// A compact per-actor property plan built once at compile time.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PropertyPlan {
    slots: Vec<PropertySlot>,
}

impl PropertyPlan {
    /// Create a plan and sort slots by id for binary search.
    pub fn new(mut slots: Vec<PropertySlot>) -> Self {
        slots.sort_by_key(|slot| slot.id);
        Self { slots }
    }

    /// Number of slots in the plan.
    pub fn len(&self) -> usize {
        self.slots.len()
    }

    /// Returns `true` when the plan has no slots.
    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    /// Copy slots that are not part of a freshly rebuilt built-in plan.
    ///
    /// Actor re-declarations rebuild the common-property plan, but extension
    /// properties must survive so keyframes and always-blocks keep working.
    pub fn preserve_extension_slots(&mut self, previous: &PropertyPlan) {
        for slot in &previous.slots {
            if self.get(slot.id).is_none() {
                self.ensure_slot(slot.id, slot.kind).track = slot.track.clone();
            }
        }
    }

    /// Build the default plan for an actor kind from the property registry.
    ///
    /// This is the first registry-driven producer for plans. Later phases can
    /// replace it with per-primitive descriptors while keeping the same
    /// `PropertyId`/slot access model.
    pub fn for_actor_kind(kind: ActorKindId) -> Self {
        let slots = PROPERTY_REGISTRY
            .iter()
            .filter(|schema| schema.applicable.includes(kind))
            .filter_map(|schema| {
                let id = crate::timeline::property_id(schema.name)?;
                let kind = property_kind_from_value_type(schema.value_type);
                Some(PropertySlot {
                    id,
                    kind,
                    track: DynTrack::empty(kind),
                })
            })
            .collect();
        Self::new(slots)
    }

    /// Look up a slot by stable id.
    pub fn get(&self, id: PropertyId) -> Option<&PropertySlot> {
        self.slots
            .binary_search_by_key(&id, |slot| slot.id)
            .ok()
            .map(|i| &self.slots[i])
    }

    /// Mutably look up a slot by stable id.
    pub fn get_mut(&mut self, id: PropertyId) -> Option<&mut PropertySlot> {
        self.slots
            .binary_search_by_key(&id, |slot| slot.id)
            .ok()
            .map(|i| &mut self.slots[i])
    }

    /// Get a slot by id, creating it with the requested kind if missing.
    ///
    /// This is the extension entry point for properties that are not present
    /// in the built-in registry.
    pub fn ensure_slot(&mut self, id: PropertyId, kind: PropertyKind) -> &mut PropertySlot {
        match self.slots.binary_search_by_key(&id, |slot| slot.id) {
            Ok(index) => &mut self.slots[index],
            Err(index) => {
                self.slots.insert(
                    index,
                    PropertySlot {
                        id,
                        kind,
                        track: DynTrack::empty(kind),
                    },
                );
                &mut self.slots[index]
            },
        }
    }

    /// Iterate slots in id order.
    pub fn iter(&self) -> impl Iterator<Item = &PropertySlot> {
        self.slots.iter()
    }

    /// Mutably iterate slots in id order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut PropertySlot> {
        self.slots.iter_mut()
    }

    /// Keyframe metadata for a slot, if present.
    pub fn keyframe_count(&self, id: PropertyId) -> usize {
        self.get(id).map_or(0, |slot| slot.track.keyframe_count())
    }

    /// Keyframe times for a slot, if present.
    pub fn keyframe_times(&self, id: PropertyId) -> Vec<u64> {
        self.get(id).map_or_else(Vec::new, |slot| slot.track.keyframe_times())
    }

    /// Returns `true` when a slot has a keyframe at exactly `time_ms`.
    pub fn has_keyframe_at(&self, id: PropertyId, time_ms: u64) -> bool {
        self.get(id).is_some_and(|slot| slot.track.has_keyframe_at(time_ms))
    }

    /// Easing at a slot keyframe time.
    pub fn keyframe_easing(&self, id: PropertyId, time_ms: u64) -> Option<Easing> {
        self.get(id).and_then(|slot| slot.track.keyframe_easing(time_ms))
    }

    /// Maximum keyframe time across all slots.
    pub fn max_keyframe_time(&self) -> Option<u64> {
        self.slots.iter().filter_map(|slot| slot.track.max_keyframe_time()).max()
    }

    /// Returns `true` when any slot has non-static keyframes.
    pub fn has_any_keyframes(&self) -> bool {
        self.slots.iter().any(|slot| slot.track.has_any_keyframes())
    }
}

/// Map a registry value type to the finite dynamic track kind.
fn property_kind_from_value_type(value_type: ValueType) -> PropertyKind {
    match value_type {
        ValueType::F32 => PropertyKind::F32,
        ValueType::U32 => PropertyKind::U32,
        ValueType::Vec2 => PropertyKind::Vec2,
        ValueType::Vec4 | ValueType::Color => PropertyKind::Vec4,
        ValueType::String => PropertyKind::String,
        ValueType::PointList => PropertyKind::PointList,
        _ => PropertyKind::Generic,
    }
}

/// Type-erased animated track for one dynamic property.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum DynTrack {
    /// Float track.
    F32(Option<PropertyTrack<f32>>),
    /// Unsigned integer track.
    U32(Option<PropertyTrack<u32>>),
    /// Boolean track.
    Bool(Option<PropertyTrack<bool>>),
    /// 2D vector track.
    Vec2(Option<PropertyTrack<[f32; 2]>>),
    /// 4D vector / color track.
    Vec4(Option<PropertyTrack<[f32; 4]>>),
    /// String track.
    String(Option<PropertyTrack<String>>),
    /// Point list track.
    PointList(Option<PropertyTrack<Vec<[f32; 2]>>>),
    /// Generic finite value track.
    Generic(Option<PropertyTrack<PropertyValue>>),
}

impl DynTrack {
    /// Create an empty track of the requested kind.
    pub fn empty(kind: PropertyKind) -> Self {
        match kind {
            PropertyKind::F32 => Self::F32(None),
            PropertyKind::U32 => Self::U32(None),
            PropertyKind::Bool => Self::Bool(None),
            PropertyKind::Vec2 => Self::Vec2(None),
            PropertyKind::Vec4 => Self::Vec4(None),
            PropertyKind::String => Self::String(None),
            PropertyKind::PointList => Self::PointList(None),
            PropertyKind::Generic => Self::Generic(None),
        }
    }

    /// Value kind of this track.
    pub fn kind(&self) -> PropertyKind {
        match self {
            Self::F32(_) => PropertyKind::F32,
            Self::U32(_) => PropertyKind::U32,
            Self::Bool(_) => PropertyKind::Bool,
            Self::Vec2(_) => PropertyKind::Vec2,
            Self::Vec4(_) => PropertyKind::Vec4,
            Self::String(_) => PropertyKind::String,
            Self::PointList(_) => PropertyKind::PointList,
            Self::Generic(_) => PropertyKind::Generic,
        }
    }

    /// Insert an instant linear keyframe, or `None` when the value kind mismatches.
    pub fn add_keyframe(&mut self, time_ms: u64, value: PropertyValue) -> Option<()> {
        self.add_keyframe_eased(time_ms, value, Easing::Linear)
    }

    /// Insert an eased keyframe, or `None` when the value kind mismatches.
    pub fn add_keyframe_eased(
        &mut self,
        time_ms: u64,
        value: PropertyValue,
        easing: Easing,
    ) -> Option<()> {
        match (self, value) {
            (Self::F32(track), PropertyValue::F32(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(0.0));
                track.add_keyframe(time_ms, value, easing);
                Some(())
            },
            (Self::U32(track), PropertyValue::U32(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(0));
                track.add_keyframe(time_ms, value, easing);
                Some(())
            },
            (Self::Bool(track), PropertyValue::Bool(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(false));
                track.add_keyframe(time_ms, value, easing);
                Some(())
            },
            (Self::Vec2(track), PropertyValue::Vec2(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new([0.0, 0.0]));
                track.add_keyframe(time_ms, value, easing);
                Some(())
            },
            (Self::Vec4(track), PropertyValue::Vec4(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new([0.0; 4]));
                track.add_keyframe(time_ms, value, easing);
                Some(())
            },
            (Self::String(track), PropertyValue::String(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(String::new()));
                track.add_keyframe(time_ms, value, easing);
                Some(())
            },
            (Self::PointList(track), PropertyValue::PointList(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(Vec::<[f32; 2]>::new()));
                track.add_keyframe(time_ms, value, easing);
                Some(())
            },
            (Self::Generic(track), value) => {
                let track =
                    track.get_or_insert_with(|| PropertyTrack::new(PropertyValue::F32(0.0)));
                track.add_keyframe(time_ms, value, easing);
                Some(())
            },
            _ => None,
        }
    }

    /// Sample the track at `time_ms`, or `None` when the track has no storage.
    pub fn sample(&self, time_ms: u64) -> Option<PropertyValue> {
        match self {
            Self::F32(Some(track)) => Some(PropertyValue::F32(track.evaluate(time_ms))),
            Self::U32(Some(track)) => Some(PropertyValue::U32(track.evaluate(time_ms))),
            Self::Bool(Some(track)) => Some(PropertyValue::Bool(track.evaluate(time_ms))),
            Self::Vec2(Some(track)) => Some(PropertyValue::Vec2(track.evaluate(time_ms))),
            Self::Vec4(Some(track)) => Some(PropertyValue::Vec4(track.evaluate(time_ms))),
            Self::String(Some(track)) => Some(PropertyValue::String(track.evaluate(time_ms))),
            Self::PointList(Some(track)) => Some(PropertyValue::PointList(track.evaluate(time_ms))),
            Self::Generic(Some(track)) => Some(track.evaluate(time_ms)),
            _ => None,
        }
    }

    /// Number of keyframes, if the track has storage.
    pub fn keyframe_count(&self) -> usize {
        match self {
            Self::F32(Some(track)) => track.keyframes.len(),
            Self::U32(Some(track)) => track.keyframes.len(),
            Self::Bool(Some(track)) => track.keyframes.len(),
            Self::Vec2(Some(track)) => track.keyframes.len(),
            Self::Vec4(Some(track)) => track.keyframes.len(),
            Self::String(Some(track)) => track.keyframes.len(),
            Self::PointList(Some(track)) => track.keyframes.len(),
            Self::Generic(Some(track)) => track.keyframes.len(),
            _ => 0,
        }
    }

    /// Keyframe times, sorted.
    pub fn keyframe_times(&self) -> Vec<u64> {
        let mut times = match self {
            Self::F32(Some(track)) => track.keyframes.keys().copied().collect(),
            Self::U32(Some(track)) => track.keyframes.keys().copied().collect(),
            Self::Bool(Some(track)) => track.keyframes.keys().copied().collect(),
            Self::Vec2(Some(track)) => track.keyframes.keys().copied().collect(),
            Self::Vec4(Some(track)) => track.keyframes.keys().copied().collect(),
            Self::String(Some(track)) => track.keyframes.keys().copied().collect(),
            Self::PointList(Some(track)) => track.keyframes.keys().copied().collect(),
            Self::Generic(Some(track)) => track.keyframes.keys().copied().collect(),
            _ => Vec::new(),
        };
        times.sort_unstable();
        times
    }

    /// Returns `true` when a keyframe exists at exactly `time_ms`.
    pub fn has_keyframe_at(&self, time_ms: u64) -> bool {
        match self {
            Self::F32(Some(track)) => track.keyframes.contains_key(&time_ms),
            Self::U32(Some(track)) => track.keyframes.contains_key(&time_ms),
            Self::Bool(Some(track)) => track.keyframes.contains_key(&time_ms),
            Self::Vec2(Some(track)) => track.keyframes.contains_key(&time_ms),
            Self::Vec4(Some(track)) => track.keyframes.contains_key(&time_ms),
            Self::String(Some(track)) => track.keyframes.contains_key(&time_ms),
            Self::PointList(Some(track)) => track.keyframes.contains_key(&time_ms),
            Self::Generic(Some(track)) => track.keyframes.contains_key(&time_ms),
            _ => false,
        }
    }

    /// Easing at a specific keyframe time.
    pub fn keyframe_easing(&self, time_ms: u64) -> Option<Easing> {
        match self {
            Self::F32(Some(track)) => track.keyframes.get(&time_ms).map(|(_, easing)| *easing),
            Self::U32(Some(track)) => track.keyframes.get(&time_ms).map(|(_, easing)| *easing),
            Self::Bool(Some(track)) => track.keyframes.get(&time_ms).map(|(_, easing)| *easing),
            Self::Vec2(Some(track)) => track.keyframes.get(&time_ms).map(|(_, easing)| *easing),
            Self::Vec4(Some(track)) => track.keyframes.get(&time_ms).map(|(_, easing)| *easing),
            Self::String(Some(track)) => track.keyframes.get(&time_ms).map(|(_, easing)| *easing),
            Self::PointList(Some(track)) => {
                track.keyframes.get(&time_ms).map(|(_, easing)| *easing)
            },
            Self::Generic(Some(track)) => track.keyframes.get(&time_ms).map(|(_, easing)| *easing),
            _ => None,
        }
    }

    /// Latest keyframe time, if any.
    pub fn max_keyframe_time(&self) -> Option<u64> {
        match self {
            Self::F32(Some(track)) => track.last_keyframe_time(),
            Self::U32(Some(track)) => track.last_keyframe_time(),
            Self::Bool(Some(track)) => track.last_keyframe_time(),
            Self::Vec2(Some(track)) => track.last_keyframe_time(),
            Self::Vec4(Some(track)) => track.last_keyframe_time(),
            Self::String(Some(track)) => track.last_keyframe_time(),
            Self::PointList(Some(track)) => track.last_keyframe_time(),
            Self::Generic(Some(track)) => track.last_keyframe_time(),
            _ => None,
        }
    }

    /// Returns `true` when the track has keyframes that could change value.
    pub fn has_any_keyframes(&self) -> bool {
        match self {
            Self::F32(Some(track)) => !track.is_effectively_static(),
            Self::U32(Some(track)) => !track.is_effectively_static(),
            Self::Bool(Some(track)) => !track.is_effectively_static(),
            Self::Vec2(Some(track)) => !track.is_effectively_static(),
            Self::Vec4(Some(track)) => !track.is_effectively_static(),
            Self::String(Some(track)) => !track.is_effectively_static(),
            Self::PointList(Some(track)) => !track.is_effectively_static(),
            Self::Generic(Some(track)) => !track.is_effectively_static(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DynTrack, PropertyKind, PropertyPlan, PropertySlot};
    use crate::timeline::{ActorKindId, PropertyValue, property_id};

    #[test]
    fn plan_binary_searches_by_property_id() {
        let id_a = animatix_syntax::schema::PropertyId(3);
        let id_b = animatix_syntax::schema::PropertyId(10);
        let plan = PropertyPlan::new(vec![
            PropertySlot {
                id: id_b,
                kind: PropertyKind::String,
                track: DynTrack::empty(PropertyKind::String),
            },
            PropertySlot {
                id: id_a,
                kind: PropertyKind::F32,
                track: DynTrack::empty(PropertyKind::F32),
            },
        ]);

        assert_eq!(plan.get(id_a).map(|slot| slot.kind), Some(PropertyKind::F32));
        assert_eq!(plan.get(id_b).map(|slot| slot.kind), Some(PropertyKind::String));
        assert!(plan.get(animatix_syntax::schema::PropertyId(99)).is_none());
    }

    #[test]
    fn dyn_track_adds_and_samples_matching_kind() {
        let mut track = DynTrack::empty(PropertyKind::Vec2);
        assert_eq!(track.add_keyframe(100, PropertyValue::Vec2([1.0, 2.0])), Some(()));
        assert_eq!(track.sample(100), Some(PropertyValue::Vec2([1.0, 2.0])));

        assert_eq!(track.add_keyframe(100, PropertyValue::String("wrong".to_string())), None);
        assert_eq!(track.kind(), PropertyKind::Vec2);
    }

    #[test]
    fn ensure_slot_creates_extension_property_slots() {
        let mut plan = PropertyPlan::default();
        let id = animatix_syntax::schema::PropertyId(999);
        let slot = plan.ensure_slot(id, PropertyKind::String);
        assert_eq!(slot.kind, PropertyKind::String);
        assert!(slot.track.add_keyframe(0, PropertyValue::String("ext".to_string())).is_some());
        assert_eq!(
            plan.get(id).and_then(|slot| slot.track.sample(0)),
            Some(PropertyValue::String("ext".to_string()))
        );
    }

    #[cfg(feature = "serde")]
    #[test]
    fn property_plan_serde_round_trip() {
        let mut plan = PropertyPlan::default();
        let id = animatix_syntax::schema::PropertyId(999_999);
        let slot = plan.ensure_slot(id, PropertyKind::String);
        assert!(slot.track.add_keyframe(0, PropertyValue::String("ext".to_string())).is_some());

        let json = serde_json::to_string(&plan).expect("serialize property plan");
        let decoded: PropertyPlan = serde_json::from_str(&json).expect("deserialize property plan");
        assert_eq!(
            decoded.get(id).and_then(|slot| slot.track.sample(0)),
            Some(PropertyValue::String("ext".to_string()))
        );
    }

    #[test]
    fn actor_kind_plan_maps_registry_properties_by_id() {
        let mut plan =
            PropertyPlan::for_actor_kind(ActorKindId::Shape(crate::timeline::ShapeKind::Rect));
        let position = property_id("position").expect("position is registered");

        let slot = plan.get(position).expect("position is applicable to Rect");
        assert_eq!(slot.kind, PropertyKind::Vec2);

        let slot = plan.get_mut(position).expect("mutable position slot");
        assert_eq!(slot.track.add_keyframe(0, PropertyValue::Vec2([10.0, 20.0])), Some(()));
        assert_eq!(slot.track.sample(0), Some(PropertyValue::Vec2([10.0, 20.0])));
    }
}
