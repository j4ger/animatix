//! Primitive property track types and interpolation traits.
//!
//! This module contains the core building blocks for keyframed animation:
//! - [`Interpolate`] trait for value blending
//! - [`PropertyTrack<T>`] keyframed animation track
//! - [`TrackAccessor`] extension trait for ergonomic access
//! - Re-exports of [`Easing`] and [`apply_easing`] from `crate::easing`

use crate::timeline::morph::MorphOptions;
use std::collections::BTreeMap;

// Re-export easing types so track.rs and other modules can get them from here.
pub use crate::easing::{Easing, apply_easing};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Extension trait for lazy property track access.
pub trait TrackAccessor<T: Interpolate> {
    /// Evaluate the track at `time_ms`, falling back to `default` if empty.
    fn get(&self, time_ms: u64, default: T) -> T;
    /// Evaluate the track at `time_ms`, returning `None` when no track exists.
    fn get_or_default(&self, time_ms: u64) -> Option<T>;
    /// Ensure the track exists, creating it with `default` if absent.
    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T>;
    /// Return the value of the last keyframe, or `default` if empty.
    fn last(&self, default: T) -> T;
    /// Return the timestamp of the last keyframe, if any.
    fn last_time(&self) -> Option<u64>;
    /// Check whether a keyframe exists at exactly `time_ms`.
    fn has_keyframe_at(&self, time_ms: u64) -> bool;
}

impl<T: Interpolate> TrackAccessor<T> for Option<PropertyTrack<T>> {
    fn get(&self, time_ms: u64, default: T) -> T {
        self.as_ref().map(|t| t.evaluate(time_ms)).unwrap_or(default)
    }
    fn get_or_default(&self, time_ms: u64) -> Option<T> {
        self.as_ref().map(|t| t.evaluate(time_ms))
    }
    fn ensure(&mut self, default: T) -> &mut PropertyTrack<T> {
        self.get_or_insert_with(|| PropertyTrack::new(default))
    }
    fn last(&self, default: T) -> T {
        self.as_ref().map(|t| t.last_value()).unwrap_or(default)
    }
    fn last_time(&self) -> Option<u64> {
        self.as_ref().and_then(|t| t.last_keyframe_time())
    }
    fn has_keyframe_at(&self, time_ms: u64) -> bool {
        self.as_ref().map(|t| t.keyframes.contains_key(&time_ms)).unwrap_or(false)
    }
}

/// Trait for values that can be interpolated between two states.
pub trait Interpolate: Clone {
    /// Interpolate between `self` and `other` using parameter `t` in `[0, 1]`.
    fn interpolate(&self, other: &Self, t: f32) -> Self;
}

impl Interpolate for f32 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t.clamp(0.0, 1.0)
    }
}

impl Interpolate for f64 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        self + (other - self) * t.clamp(0.0, 1.0) as f64
    }
}

impl Interpolate for [f32; 2] {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        [self[0] + (other[0] - self[0]) * t, self[1] + (other[1] - self[1]) * t]
    }
}

impl Interpolate for [f32; 4] {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        [
            self[0] + (other[0] - self[0]) * t,
            self[1] + (other[1] - self[1]) * t,
            self[2] + (other[2] - self[2]) * t,
            self[3] + (other[3] - self[3]) * t,
        ]
    }
}

impl Interpolate for [f32; 6] {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        let t = t.clamp(0.0, 1.0);
        [
            self[0] + (other[0] - self[0]) * t,
            self[1] + (other[1] - self[1]) * t,
            self[2] + (other[2] - self[2]) * t,
            self[3] + (other[3] - self[3]) * t,
            self[4] + (other[4] - self[4]) * t,
            self[5] + (other[5] - self[5]) * t,
        ]
    }
}

impl Interpolate for u32 {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

impl Interpolate for MorphOptions {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { *self } else { *other }
    }
}

impl Interpolate for String {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { self.clone() } else { other.clone() }
    }
}

impl Interpolate for Vec<String> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if t < 0.5 { self.clone() } else { other.clone() }
    }
}

impl Interpolate for Vec<[f32; 2]> {
    fn interpolate(&self, other: &Self, t: f32) -> Self {
        if self.is_empty() || other.is_empty() || self.len() != other.len() {
            if t < 0.5 { self.clone() } else { other.clone() }
        } else {
            self.iter().zip(other.iter()).map(|(a, b)| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]).collect()
        }
    }
}

/// A keyed animation track holding values of type `T` over time.
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize), serde(bound = "T: Serialize + for<'de2> Deserialize<'de2>"))]
pub struct PropertyTrack<T> {
    /// Map from timestamp (ms) to `(value, easing)` pairs.
    pub(crate) keyframes: BTreeMap<u64, (T, Easing)>,
    /// Value used when no keyframes are defined.
    pub(crate) default_value: T,
    /// P2.20: Memoization cache for repeated time queries.
    #[cfg_attr(feature = "serde", serde(skip))]
    last_evaluated: std::cell::RefCell<Option<(u64, T)>>,
}

impl<T: Interpolate> Clone for PropertyTrack<T> {
    fn clone(&self) -> Self {
        Self {
            keyframes: self.keyframes.clone(),
            default_value: self.default_value.clone(),
            last_evaluated: std::cell::RefCell::new(None),
        }
    }
}

impl<T: Interpolate> PropertyTrack<T> {
    /// Create a new track with the given default value.
    pub fn new(default_value: T) -> Self {
        Self { keyframes: BTreeMap::new(), default_value, last_evaluated: std::cell::RefCell::new(None) }
    }
    /// Insert a keyframe at `time_ms` with `value` and `easing`.
    pub fn add_keyframe(&mut self, time_ms: u64, value: T, easing: Easing) {
        self.keyframes.insert(time_ms, (value, easing));
        // Invalidate memoization cache when keyframes change
        *self.last_evaluated.borrow_mut() = None;
    }
    /// Evaluate the interpolated value at `time_ms`.
    pub fn evaluate(&self, time_ms: u64) -> T {
        self.evaluate_with(time_ms, T::clone)
    }
    /// Optimized evaluate for `Copy` types - avoids heap allocation on clone.
    pub fn evaluate_copy(&self, time_ms: u64) -> T where T: Copy {
        self.evaluate_with(time_ms, |v| *v)
    }
    /// Returns `true` if this property track is currently inside an
    /// interpolation segment - there exists both a previous keyframe at
    /// `time <= time_ms` AND a next keyframe at `time > time_ms`.
    pub fn is_currently_animating(&self, time_ms: u64) -> bool {
        use std::ops::Bound;
        let next = self.keyframes.range((Bound::Excluded(time_ms), Bound::Unbounded)).next();
        let prev = self.keyframes.range(..=time_ms).next_back();
        matches!((prev, next), (Some(_), Some(_)))
    }
    /// Returns the interpolation segment for `time_ms`, if one exists between
    /// two keyframes. Returns `(found_time, prev_val, found_val, progress, found_easing)`
    /// where `progress` is in `(0, 1]`.
    pub(crate) fn interpolation_segment(&self, time_ms: u64) -> Option<(u64, &T, &T, f32, &Easing)> {
        let found = self.keyframes.range(time_ms..).next()?;
        let (&found_time, (found_val, found_easing)) = found;

        // Before or at first keyframe: no interior segment
        if let Some((&first_time, _)) = self.keyframes.iter().next() {
            if time_ms <= first_time {
                return None;
            }
        }

        // Find the previous keyframe before time_ms
        let (prev_time, (prev_val, _)) = self.keyframes.range(..time_ms).next_back()?;

        let duration = (found_time - prev_time) as f32;
        let elapsed = (time_ms - prev_time) as f32;
        let progress = elapsed / duration;

        Some((found_time, prev_val, found_val, progress, found_easing))
    }
    /// Core evaluation logic parameterized by clone strategy.
    fn evaluate_with(&self, time_ms: u64, clone_val: impl Fn(&T) -> T) -> T {
        // P2.20: Memoization - return cached value if time matches
        if let Some((cached_time, cached_value)) = self.last_evaluated.borrow().as_ref() {
            if *cached_time == time_ms {
                return clone_val(cached_value);
            }
        }

        let result = if let Some((_found_time, prev_val, found_val, progress, found_easing)) = self.interpolation_segment(time_ms) {
            let eased_progress = apply_easing(progress, *found_easing);
            prev_val.interpolate(found_val, eased_progress)
        } else {
            // No interior segment - use default or boundary value
            if self.keyframes.is_empty() {
                clone_val(&self.default_value)
            } else if let Some((&first_time, (val, _))) = self.keyframes.iter().next() {
                if time_ms <= first_time {
                    clone_val(val)
                } else {
                    let val = self.last_value_with(&clone_val);
                    *self.last_evaluated.borrow_mut() = Some((time_ms, clone_val(&val)));
                    return val;
                }
            } else {
                clone_val(&self.default_value)
            }
        };

        *self.last_evaluated.borrow_mut() = Some((time_ms, clone_val(&result)));
        result
    }
    /// Return the value of the most recent keyframe, or the default.
    pub fn last_value(&self) -> T {
        self.last_value_with(T::clone)
    }
    /// Return the value of the most recent keyframe, or the default, using a custom clone strategy.
    fn last_value_with(&self, clone_val: impl Fn(&T) -> T) -> T {
        self.keyframes.iter().next_back().map(|(_, (val, _))| clone_val(val))
            .unwrap_or_else(|| clone_val(&self.default_value))
    }
    /// Return the timestamp of the most recent keyframe, if any.
    pub fn last_keyframe_time(&self) -> Option<u64> {
        self.keyframes.keys().next_back().copied()
    }
    /// Returns true if this track has keyframes that could change value over time.
    /// A track with 0 keyframes or 1 keyframe at time 0 is effectively static.
    pub fn is_effectively_static(&self) -> bool {
        match self.keyframes.len() {
            0 => true,
            1 => self.keyframes.keys().next() == Some(&0),
            _ => false,
        }
    }

    /// Sets the default value and invalidates the memoization cache.
    pub fn set_default_value(&mut self, value: T) {
        self.default_value = value;
        *self.last_evaluated.borrow_mut() = None;
    }

    /// Returns a reference to the default value.
    pub fn default_value(&self) -> &T {
        &self.default_value
    }

    /// Returns a reference to the keyframes map.
    pub fn keyframes(&self) -> &BTreeMap<u64, (T, Easing)> {
        self.keyframes_raw()
    }

    /// Returns a mutable reference to the keyframes map and invalidates the cache.
    pub fn keyframes_mut(&mut self) -> &mut BTreeMap<u64, (T, Easing)> {
        *self.last_evaluated.borrow_mut() = None;
        &mut self.keyframes
    }
}

impl<T> PropertyTrack<T> {
    /// Returns raw keyframe data without requiring `T: Interpolate`.
    ///
    /// Use this for read-only access to keyframe timestamps and values
    /// when you don't need interpolation (e.g., displaying keyframe times
    /// in a GUI). Prefer [`keyframes`](Self::keyframes) when `T` implements
    /// [`Interpolate`], as it makes the trait bound explicit at the call site.
    pub fn keyframes_raw(&self) -> &BTreeMap<u64, (T, Easing)> {
        &self.keyframes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A type that does not implement `Interpolate`.
    /// Verifies that `keyframes_raw()` works without the trait bound.
    #[derive(Debug)]
    struct NonInterpolatable(u64);

    #[test]
    fn keyframes_raw_works_without_interpolate() {
        let track: PropertyTrack<NonInterpolatable> = PropertyTrack {
            keyframes: BTreeMap::from([
                (100, (NonInterpolatable(10), Easing::Linear)),
                (200, (NonInterpolatable(20), Easing::EaseInOut)),
            ]),
            default_value: NonInterpolatable(0),
            last_evaluated: std::cell::RefCell::new(None),
        };

        let raw = track.keyframes_raw();
        assert_eq!(raw.len(), 2);
        assert!(raw.contains_key(&100));
        assert!(raw.contains_key(&200));
        assert_eq!(raw.get(&100).unwrap().0.0, 10);
        assert_eq!(raw.get(&200).unwrap().0.0, 20);
    }
}
