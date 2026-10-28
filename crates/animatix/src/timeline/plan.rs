//! Build-time property plans and dynamic property tracks.
//!
//! This module is the performance-oriented counterpart to the shared schema.
//! Names are resolved to [`animatix_syntax::schema::PropertyId`] once at build time. Frame-time
//! access then walks a compact plan and dispatches through a finite [`DynTrack`] enum
//! instead of doing String hash lookups.

use animatix_syntax::schema::{PropertyId, PropertyValueKind};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::easing::Easing;
use crate::timeline::property_registry::{PROPERTY_REGISTRY, ValueType};
use crate::timeline::property_track::PropertyTrack;
use crate::timeline::{ActorCaps, PropertyValue};

/// Finite value kinds understood by dynamic property tracks.
///
/// This is the shared schema's [`PropertyValueKind`] under the plan-side name;
/// one enum means the plan, the extension registry, and the analyzer cannot
/// drift, and the serde wire shape (externally tagged variant names) is
/// unchanged by the alias.
pub type PropertyKind = PropertyValueKind;

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

    /// Build the default plan for an actor from the property registry.
    ///
    /// A slot's kind comes from the registry binding; slots are appended for
    /// every applicable property and addressed by `PropertyId`.
    ///
    /// `legend` and `callout_place` are skipped: neither is stored in a plan
    /// slot (the former lives in the tagged tier as a sum, the latter in the
    /// typed `CalloutPlace` track), so a slot for them would stay empty forever
    /// and be serialized into every persisted plan.
    pub fn for_actor(caps: &ActorCaps, actor_type: &str) -> Self {
        let slots = PROPERTY_REGISTRY
            .iter()
            .filter(|schema| schema.applicable.includes(caps, actor_type))
            .filter(|schema| {
                !matches!(
                    schema.field,
                    crate::timeline::property_registry::ActorField::Tagged("legend")
                        | crate::timeline::property_registry::ActorField::Tagged("callout_place")
                )
            })
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

    /// Sample a plan-stored property by canonical name.
    pub fn sample_property_named(&self, name: &str, time_ms: u64) -> Option<PropertyValue> {
        let id = crate::timeline::property_registry::property_id(name)?;
        self.get(id).and_then(|slot| slot.track.sample(time_ms))
    }

    /// Sample an f32 property by name with fallback default.
    pub fn sample_f32_prop(&self, name: &str, time_ms: u64, default: f32) -> f32 {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::F32(v)) => v,
            _ => default,
        }
    }

    /// Sample a Color / Vec4 property by name with fallback default.
    pub fn sample_color_prop(&self, name: &str, time_ms: u64, default: [f32; 4]) -> [f32; 4] {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::Color(c)) | Some(PropertyValue::Vec4(c)) => c,
            _ => default,
        }
    }

    /// Sample a bool property by name with fallback default.
    pub fn sample_bool_prop(&self, name: &str, time_ms: u64, default: bool) -> bool {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::Bool(v)) => v,
            _ => default,
        }
    }

    /// Sample a Vec2 property by name with fallback default.
    pub fn sample_vec2_prop(&self, name: &str, time_ms: u64, default: [f32; 2]) -> [f32; 2] {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::Vec2(v)) => v,
            _ => default,
        }
    }

    /// Sample a string property by name with fallback default.
    pub fn sample_string_prop(&self, name: &str, time_ms: u64, default: &str) -> String {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::String(s)) => s,
            _ => default.to_string(),
        }
    }

    /// Sample a u32 property by name with fallback default.
    pub fn sample_u32_prop(&self, name: &str, time_ms: u64, default: u32) -> u32 {
        match self.sample_property_named(name, time_ms) {
            Some(PropertyValue::U32(v)) => v,
            _ => default,
        }
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

/// Visit the inner track of any `DynTrack` variant.
///
/// Every storage-shaped method on [`DynTrack`] is the same eight-arm match over
/// the variant and its `Option<PropertyTrack<_>>`; the macro keeps the variant
/// list in one place. `$body` sees `$track` bound to `&PropertyTrack<_>`.
macro_rules! with_dyn_track {
    ($this:expr, $missing:expr, |$track:ident| $body:expr) => {
        match $this {
            DynTrack::F32(__track) => __track.as_ref().map_or($missing, |$track| $body),
            DynTrack::U32(__track) => __track.as_ref().map_or($missing, |$track| $body),
            DynTrack::Bool(__track) => __track.as_ref().map_or($missing, |$track| $body),
            DynTrack::Vec2(__track) => __track.as_ref().map_or($missing, |$track| $body),
            DynTrack::Vec4(__track) => __track.as_ref().map_or($missing, |$track| $body),
            DynTrack::String(__track) => __track.as_ref().map_or($missing, |$track| $body),
            DynTrack::PointList(__track) => __track.as_ref().map_or($missing, |$track| $body),
            DynTrack::Generic(__track) => __track.as_ref().map_or($missing, |$track| $body),
        }
    };
}

/// Construct the empty track for a value kind.
macro_rules! empty_dyn_track {
    ($kind:expr) => {
        match $kind {
            PropertyKind::F32 => DynTrack::F32(None),
            PropertyKind::U32 => DynTrack::U32(None),
            PropertyKind::Bool => DynTrack::Bool(None),
            PropertyKind::Vec2 => DynTrack::Vec2(None),
            PropertyKind::Vec4 => DynTrack::Vec4(None),
            PropertyKind::String => DynTrack::String(None),
            PropertyKind::PointList => DynTrack::PointList(None),
            PropertyKind::Generic => DynTrack::Generic(None),
        }
    };
}

/// Read the value kind back out of a track variant.
macro_rules! dyn_track_kind {
    ($this:expr) => {
        match $this {
            DynTrack::F32(_) => PropertyKind::F32,
            DynTrack::U32(_) => PropertyKind::U32,
            DynTrack::Bool(_) => PropertyKind::Bool,
            DynTrack::Vec2(_) => PropertyKind::Vec2,
            DynTrack::Vec4(_) => PropertyKind::Vec4,
            DynTrack::String(_) => PropertyKind::String,
            DynTrack::PointList(_) => PropertyKind::PointList,
            DynTrack::Generic(_) => PropertyKind::Generic,
        }
    };
}

/// Sample the inner track of a `DynTrack`, wrapping the value back into its
/// [`PropertyValue`] variant.
macro_rules! sample_dyn_track {
    ($this:expr, $time_ms:expr) => {
        match $this {
            DynTrack::F32(track) => {
                track.as_ref().map(|track| PropertyValue::F32(track.evaluate($time_ms)))
            },
            DynTrack::U32(track) => {
                track.as_ref().map(|track| PropertyValue::U32(track.evaluate($time_ms)))
            },
            DynTrack::Bool(track) => {
                track.as_ref().map(|track| PropertyValue::Bool(track.evaluate($time_ms)))
            },
            DynTrack::Vec2(track) => {
                track.as_ref().map(|track| PropertyValue::Vec2(track.evaluate($time_ms)))
            },
            DynTrack::Vec4(track) => {
                track.as_ref().map(|track| PropertyValue::Vec4(track.evaluate($time_ms)))
            },
            DynTrack::String(track) => {
                track.as_ref().map(|track| PropertyValue::String(track.evaluate($time_ms)))
            },
            DynTrack::PointList(track) => {
                track.as_ref().map(|track| PropertyValue::PointList(track.evaluate($time_ms)))
            },
            DynTrack::Generic(track) => track.as_ref().map(|track| track.evaluate($time_ms)),
        }
    };
}

/// Insert a keyframe into the variant that stores this value kind, if any.
macro_rules! insert_dyn_track {
    ($this:expr, $time_ms:expr, $value:expr, $easing:expr) => {
        match ($this, $value) {
            (DynTrack::F32(track), PropertyValue::F32(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(0.0));
                track.add_keyframe($time_ms, value, $easing);
                Some(())
            },
            (DynTrack::U32(track), PropertyValue::U32(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(0));
                track.add_keyframe($time_ms, value, $easing);
                Some(())
            },
            (DynTrack::Bool(track), PropertyValue::Bool(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(false));
                track.add_keyframe($time_ms, value, $easing);
                Some(())
            },
            (DynTrack::Vec2(track), PropertyValue::Vec2(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new([0.0, 0.0]));
                track.add_keyframe($time_ms, value, $easing);
                Some(())
            },
            (DynTrack::Vec4(track), PropertyValue::Vec4(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new([0.0; 4]));
                track.add_keyframe($time_ms, value, $easing);
                Some(())
            },
            (DynTrack::String(track), PropertyValue::String(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(String::new()));
                track.add_keyframe($time_ms, value, $easing);
                Some(())
            },
            (DynTrack::PointList(track), PropertyValue::PointList(value)) => {
                let track = track.get_or_insert_with(|| PropertyTrack::new(Vec::<[f32; 2]>::new()));
                track.add_keyframe($time_ms, value, $easing);
                Some(())
            },
            (DynTrack::Generic(track), value) => {
                let track =
                    track.get_or_insert_with(|| PropertyTrack::new(PropertyValue::F32(0.0)));
                track.add_keyframe($time_ms, value, $easing);
                Some(())
            },
            _ => None,
        }
    };
}

impl DynTrack {
    /// Create an empty track of the requested kind.
    pub fn empty(kind: PropertyKind) -> Self {
        empty_dyn_track!(kind)
    }

    /// Value kind of this track.
    pub fn kind(&self) -> PropertyKind {
        dyn_track_kind!(self)
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
        insert_dyn_track!(self, time_ms, value, easing)
    }

    /// Sample the track at `time_ms`, or `None` when the track has no storage.
    pub fn sample(&self, time_ms: u64) -> Option<PropertyValue> {
        sample_dyn_track!(self, time_ms)
    }

    /// Number of keyframes, if the track has storage.
    pub fn keyframe_count(&self) -> usize {
        with_dyn_track!(self, 0, |track| track.keyframes.len())
    }

    /// Keyframe times, sorted.
    pub fn keyframe_times(&self) -> Vec<u64> {
        let mut times = with_dyn_track!(self, Vec::<u64>::new(), |track| track
            .keyframes
            .keys()
            .copied()
            .collect::<Vec<_>>());
        times.sort_unstable();
        times
    }

    /// Returns `true` when a keyframe exists at exactly `time_ms`.
    pub fn has_keyframe_at(&self, time_ms: u64) -> bool {
        with_dyn_track!(self, false, |track| track.keyframes.contains_key(&time_ms))
    }

    /// Easing at a specific keyframe time.
    pub fn keyframe_easing(&self, time_ms: u64) -> Option<Easing> {
        with_dyn_track!(self, None, |track| track
            .keyframes
            .get(&time_ms)
            .map(|(_, easing)| *easing))
    }

    /// Latest keyframe time, if any.
    pub fn max_keyframe_time(&self) -> Option<u64> {
        with_dyn_track!(self, None, |track| track.last_keyframe_time())
    }

    /// Returns `true` when the track has keyframes that could change value.
    pub fn has_any_keyframes(&self) -> bool {
        with_dyn_track!(self, false, |track| !track.is_effectively_static())
    }
}

impl PropertyPlan {
    /// Rebuild in place from an actor identity, preserving extension slots.
    ///
    /// This is the storage-independent form of the rebuild: callers that own a
    /// `PropertyPlan` directly can refresh it after a capability/type change.
    pub fn rebuild_for(&mut self, caps: &animatix_core::caps::ActorCaps, actor_type: &str) {
        let previous = std::mem::take(self);
        *self = Self::for_actor(caps, actor_type);
        self.preserve_extension_slots(&previous);
    }
}

#[cfg(test)]
mod tests {
    use super::{DynTrack, PropertyKind, PropertyPlan, PropertySlot};
    use crate::timeline::{PropertyValue, property_id};

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
        // `PropertyKind` is an alias of `PropertyValueKind`; this pins the wire
        // shape (externally tagged variant name) so a future split back into two
        // enums cannot silently change saved plans.
        assert!(json.contains("\"kind\":\"String\""), "plan kind wire shape changed: {json}");
        let decoded: PropertyPlan = serde_json::from_str(&json).expect("deserialize property plan");
        assert_eq!(
            decoded.get(id).and_then(|slot| slot.track.sample(0)),
            Some(PropertyValue::String("ext".to_string()))
        );
    }

    #[test]
    fn actor_kind_plan_maps_registry_properties_by_id() {
        let mut plan = PropertyPlan::for_actor(
            &animatix_std::caps_for_type("Rect").expect("Rect is a built-in"),
            "Rect",
        );
        let position = property_id("position").expect("position is registered");

        let slot = plan.get(position).expect("position is applicable to Rect");
        assert_eq!(slot.kind, PropertyKind::Vec2);

        let slot = plan.get_mut(position).expect("mutable position slot");
        assert_eq!(slot.track.add_keyframe(0, PropertyValue::Vec2([10.0, 20.0])), Some(()));
        assert_eq!(slot.track.sample(0), Some(PropertyValue::Vec2([10.0, 20.0])));
    }

    /// `legend` (tagged sum) and `callout_place` (typed `CalloutPlace` track)
    /// have no plan storage; a generated slot for them would always be empty.
    #[test]
    fn actor_plan_skips_tagged_only_properties() {
        let plan = PropertyPlan::for_actor(
            &animatix_std::caps_for_type("Callout").expect("Callout is a built-in"),
            "Callout",
        );
        for name in ["legend", "place"] {
            let id = property_id(name).expect("registered property");
            assert!(plan.get(id).is_none(), "`{name}` must not get an empty plan slot");
        }
        assert!(plan.get(property_id("position").expect("position")).is_some());
    }
}
