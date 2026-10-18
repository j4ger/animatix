//! Zero-heap compact keyframe storage.
//!
//! Stores keyframes for a property track without heap allocation for 0, 1, or 2 keyframes.
//! Falls back to a flat sorted vector for 3+ keyframes.

use crate::easing::Easing;

/// A single keyframe containing its timestamp and payload `(value, easing)`.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyframeEntry<T> {
    /// Timestamp in milliseconds.
    pub time_ms: u64,
    /// Keyframe value and interpolation easing curve.
    pub payload: (T, Easing),
}

/// Compact keyframe storage:
/// - `Empty`: 0 keyframes (default value used)
/// - `One`: 1 keyframe (covers static / declaration-only properties)
/// - `Two`: 2 keyframes (covers standard A->B animations without heap allocation)
/// - `Many`: 3+ keyframes, kept strictly sorted by timestamp in a flat vector
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Keyframes<T> {
    /// Zero keyframes; track defaults to its initial value.
    #[default]
    Empty,
    /// Exactly one keyframe (zero-heap inline storage).
    One(KeyframeEntry<T>),
    /// Exactly two keyframes (zero-heap inline storage).
    Two([KeyframeEntry<T>; 2]),
    /// Three or more keyframes allocated in a sorted vector.
    Many(Vec<KeyframeEntry<T>>),
}

impl<T> Keyframes<T> {
    /// Creates an empty keyframe container.
    pub const fn new() -> Self {
        Self::Empty
    }

    /// Number of keyframes in the track.
    #[inline(always)]
    pub fn len(&self) -> usize {
        match self {
            Self::Empty => 0,
            Self::One(_) => 1,
            Self::Two(_) => 2,
            Self::Many(v) => v.len(),
        }
    }

    /// Whether there are no keyframes.
    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        matches!(self, Self::Empty)
    }

    /// Clears all keyframes.
    pub fn clear(&mut self) {
        *self = Self::Empty;
    }

    /// View keyframes as a contiguous slice.
    #[inline(always)]
    pub fn as_slice(&self) -> &[KeyframeEntry<T>] {
        match self {
            Self::Empty => &[],
            Self::One(k) => std::slice::from_ref(k),
            Self::Two(arr) => arr.as_slice(),
            Self::Many(v) => v.as_slice(),
        }
    }

    /// View keyframes as a mutable slice.
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [KeyframeEntry<T>] {
        match self {
            Self::Empty => &mut [],
            Self::One(k) => std::slice::from_mut(k),
            Self::Two(arr) => arr.as_mut_slice(),
            Self::Many(v) => v.as_mut_slice(),
        }
    }

    /// Returns true if a keyframe exists at exactly `time_ms`.
    #[inline]
    pub fn contains_key(&self, time_ms: &u64) -> bool {
        match self {
            Self::Empty => false,
            Self::One(k) => k.time_ms == *time_ms,
            Self::Two([k0, k1]) => k0.time_ms == *time_ms || k1.time_ms == *time_ms,
            Self::Many(v) => v.binary_search_by_key(time_ms, |k| k.time_ms).is_ok(),
        }
    }

    /// Lookup a keyframe value and easing at exactly `time_ms`.
    #[inline]
    pub fn get(&self, time_ms: &u64) -> Option<&(T, Easing)> {
        match self {
            Self::Empty => None,
            Self::One(k) => {
                if k.time_ms == *time_ms {
                    Some(&k.payload)
                } else {
                    None
                }
            },
            Self::Two([k0, k1]) => {
                if k0.time_ms == *time_ms {
                    Some(&k0.payload)
                } else if k1.time_ms == *time_ms {
                    Some(&k1.payload)
                } else {
                    None
                }
            },
            Self::Many(v) => {
                v.binary_search_by_key(time_ms, |k| k.time_ms).ok().map(|idx| &v[idx].payload)
            },
        }
    }

    /// Lookup mutable keyframe value and easing at exactly `time_ms`.
    #[inline]
    pub fn get_mut(&mut self, time_ms: &u64) -> Option<&mut (T, Easing)> {
        match self {
            Self::Empty => None,
            Self::One(k) => {
                if k.time_ms == *time_ms {
                    Some(&mut k.payload)
                } else {
                    None
                }
            },
            Self::Two([k0, k1]) => {
                if k0.time_ms == *time_ms {
                    Some(&mut k0.payload)
                } else if k1.time_ms == *time_ms {
                    Some(&mut k1.payload)
                } else {
                    None
                }
            },
            Self::Many(v) => v
                .binary_search_by_key(time_ms, |k| k.time_ms)
                .ok()
                .map(|idx| &mut v[idx].payload),
        }
    }

    /// Returns the timestamp of the first keyframe, if any.
    #[inline]
    pub fn first_keyframe_time(&self) -> Option<u64> {
        self.as_slice().first().map(|k| k.time_ms)
    }

    /// Returns the timestamp of the last keyframe, if any.
    #[inline]
    pub fn last_keyframe_time(&self) -> Option<u64> {
        self.as_slice().last().map(|k| k.time_ms)
    }

    /// Double-ended iterator over timestamps and payloads.
    #[inline]
    pub fn iter(
        &self,
    ) -> impl DoubleEndedIterator<Item = (&u64, &(T, Easing))> + ExactSizeIterator {
        self.as_slice().iter().map(|k| (&k.time_ms, &k.payload))
    }

    /// Double-ended iterator over timestamps.
    #[inline]
    pub fn keys(&self) -> impl DoubleEndedIterator<Item = &u64> + ExactSizeIterator {
        self.as_slice().iter().map(|k| &k.time_ms)
    }

    /// Double-ended iterator over payloads `(value, easing)`.
    #[inline]
    pub fn values(&self) -> impl DoubleEndedIterator<Item = &(T, Easing)> + ExactSizeIterator {
        self.as_slice().iter().map(|k| &k.payload)
    }

    /// Find the last keyframe with timestamp <= `time_ms`.
    #[inline]
    pub fn prev_keyframe(&self, time_ms: u64) -> Option<(&u64, &(T, Easing))> {
        let slice = self.as_slice();
        let idx = slice.partition_point(|k| k.time_ms <= time_ms);
        if idx > 0 {
            let k = &slice[idx - 1];
            Some((&k.time_ms, &k.payload))
        } else {
            None
        }
    }

    /// Find the first keyframe with timestamp >= `time_ms`.
    #[inline]
    pub fn next_keyframe(&self, time_ms: u64) -> Option<(&u64, &(T, Easing))> {
        let slice = self.as_slice();
        let idx = slice.partition_point(|k| k.time_ms < time_ms);
        slice.get(idx).map(|k| (&k.time_ms, &k.payload))
    }

    /// Find the last keyframe with timestamp strictly less than `< time_ms`.
    #[inline]
    pub fn prev_keyframe_before(&self, time_ms: u64) -> Option<(&u64, &(T, Easing))> {
        if time_ms == 0 {
            return None;
        }
        self.prev_keyframe(time_ms - 1)
    }

    /// Find the first keyframe with timestamp strictly greater than `> time_ms`.
    #[inline]
    pub fn next_keyframe_after(&self, time_ms: u64) -> Option<(&u64, &(T, Easing))> {
        self.next_keyframe(time_ms.saturating_add(1))
    }

    /// Truncate all keyframes with timestamp >= `time_ms`.
    pub fn split_off(&mut self, time_ms: &u64) {
        match self {
            Self::Empty => {},
            Self::One(k) => {
                if k.time_ms >= *time_ms {
                    *self = Self::Empty;
                }
            },
            Self::Two([k0, k1]) => {
                if k0.time_ms >= *time_ms {
                    *self = Self::Empty;
                } else if k1.time_ms >= *time_ms {
                    match std::mem::replace(self, Self::Empty) {
                        Self::Two([k0, _]) => {
                            *self = Self::One(k0);
                        },
                        _ => unreachable!(),
                    }
                }
            },
            Self::Many(v) => {
                let idx = v.partition_point(|k| k.time_ms < *time_ms);
                v.truncate(idx);
                if v.is_empty() {
                    *self = Self::Empty;
                } else if v.len() == 1 {
                    *self = Self::One(v.pop().unwrap());
                } else if v.len() == 2 {
                    let k1 = v.pop().unwrap();
                    let k0 = v.pop().unwrap();
                    *self = Self::Two([k0, k1]);
                }
            },
        }
    }
}

impl<T> std::ops::Index<&u64> for Keyframes<T> {
    type Output = (T, Easing);

    #[inline]
    fn index(&self, key: &u64) -> &Self::Output {
        self.get(key).expect("no entry found for key")
    }
}

impl<T> std::ops::Index<u64> for Keyframes<T> {
    type Output = (T, Easing);

    #[inline]
    fn index(&self, key: u64) -> &Self::Output {
        self.get(&key).expect("no entry found for key")
    }
}

impl<T: Clone> Keyframes<T> {
    /// Inserts a keyframe, maintaining timestamp ordering.
    ///
    /// If a keyframe with the same timestamp already exists, its value and easing are updated.
    pub fn insert(&mut self, time_ms: u64, payload: (T, Easing)) -> Option<(T, Easing)> {
        match self {
            Self::Empty => {
                *self = Self::One(KeyframeEntry { time_ms, payload });
                None
            },
            Self::One(k) => {
                if k.time_ms == time_ms {
                    let old = std::mem::replace(&mut k.payload, payload);
                    Some(old)
                } else {
                    let old_entry = std::mem::replace(
                        k,
                        KeyframeEntry {
                            time_ms: 0,
                            payload: payload.clone(),
                        },
                    );
                    if old_entry.time_ms < time_ms {
                        *self = Self::Two([old_entry, KeyframeEntry { time_ms, payload }]);
                    } else {
                        *self = Self::Two([KeyframeEntry { time_ms, payload }, old_entry]);
                    }
                    None
                }
            },
            Self::Two([k0, k1]) => {
                if k0.time_ms == time_ms {
                    let old = std::mem::replace(&mut k0.payload, payload);
                    Some(old)
                } else if k1.time_ms == time_ms {
                    let old = std::mem::replace(&mut k1.payload, payload);
                    Some(old)
                } else {
                    let mut vec = Vec::with_capacity(3);
                    let e = KeyframeEntry { time_ms, payload };
                    if time_ms < k0.time_ms {
                        vec.push(e);
                        vec.push(k0.clone());
                        vec.push(k1.clone());
                    } else if time_ms < k1.time_ms {
                        vec.push(k0.clone());
                        vec.push(e);
                        vec.push(k1.clone());
                    } else {
                        vec.push(k0.clone());
                        vec.push(k1.clone());
                        vec.push(e);
                    }
                    *self = Self::Many(vec);
                    None
                }
            },
            Self::Many(v) => match v.binary_search_by_key(&time_ms, |k| k.time_ms) {
                Ok(idx) => Some(std::mem::replace(&mut v[idx].payload, payload)),
                Err(idx) => {
                    v.insert(idx, KeyframeEntry { time_ms, payload });
                    None
                },
            },
        }
    }

    /// Fast-path interpolation segment search:
    /// - Two: direct single-condition check (no search loop or tree traversal)
    /// - Many: binary search partition_point
    pub fn interpolation_segment(&self, time_ms: u64) -> Option<(u64, &T, &T, f32, &Easing)> {
        match self {
            Self::Empty | Self::One(_) => None,
            Self::Two([k0, k1]) => {
                if time_ms <= k0.time_ms || time_ms > k1.time_ms {
                    return None;
                }
                let duration = (k1.time_ms - k0.time_ms) as f32;
                let elapsed = (time_ms - k0.time_ms) as f32;
                let progress = elapsed / duration;
                Some((k1.time_ms, &k0.payload.0, &k1.payload.0, progress, &k1.payload.1))
            },
            Self::Many(v) => {
                if v.is_empty() || time_ms <= v[0].time_ms {
                    return None;
                }
                let idx = v.partition_point(|k| k.time_ms < time_ms);
                if idx >= v.len() {
                    return None;
                }
                let prev = &v[idx - 1];
                let found = &v[idx];
                let duration = (found.time_ms - prev.time_ms) as f32;
                let elapsed = (time_ms - prev.time_ms) as f32;
                let progress = elapsed / duration;
                Some((found.time_ms, &prev.payload.0, &found.payload.0, progress, &found.payload.1))
            },
        }
    }

    /// Returns the interpolation segment along with the segment duration.
    pub fn interpolation_segment_with_duration(
        &self,
        time_ms: u64,
    ) -> Option<(u64, &T, &T, f32, f32, &Easing)> {
        match self {
            Self::Empty | Self::One(_) => None,
            Self::Two([k0, k1]) => {
                if time_ms <= k0.time_ms || time_ms > k1.time_ms {
                    return None;
                }
                let duration = (k1.time_ms - k0.time_ms) as f32;
                let elapsed = (time_ms - k0.time_ms) as f32;
                let progress = elapsed / duration;
                Some((k1.time_ms, &k0.payload.0, &k1.payload.0, progress, duration, &k1.payload.1))
            },
            Self::Many(v) => {
                if v.is_empty() || time_ms <= v[0].time_ms {
                    return None;
                }
                let idx = v.partition_point(|k| k.time_ms < time_ms);
                if idx >= v.len() {
                    return None;
                }
                let prev = &v[idx - 1];
                let found = &v[idx];
                let duration = (found.time_ms - prev.time_ms) as f32;
                let elapsed = (time_ms - prev.time_ms) as f32;
                let progress = elapsed / duration;
                Some((
                    found.time_ms,
                    &prev.payload.0,
                    &found.payload.0,
                    progress,
                    duration,
                    &found.payload.1,
                ))
            },
        }
    }
}

impl<'a, T> IntoIterator for &'a Keyframes<T> {
    type Item = (&'a u64, &'a (T, Easing));
    type IntoIter = std::iter::Map<
        std::slice::Iter<'a, KeyframeEntry<T>>,
        fn(&'a KeyframeEntry<T>) -> (&'a u64, &'a (T, Easing)),
    >;

    fn into_iter(self) -> Self::IntoIter {
        fn mapper<T>(k: &KeyframeEntry<T>) -> (&u64, &(T, Easing)) {
            (&k.time_ms, &k.payload)
        }
        self.as_slice().iter().map(mapper)
    }
}

#[cfg(feature = "serde")]
impl<T: serde::Serialize> serde::Serialize for Keyframes<T> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (k, v) in self.iter() {
            map.serialize_entry(k, v)?;
        }
        map.end()
    }
}

#[cfg(feature = "serde")]
impl<'de, T: serde::Deserialize<'de> + Clone> serde::Deserialize<'de> for Keyframes<T> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let map = std::collections::BTreeMap::<u64, (T, Easing)>::deserialize(deserializer)?;
        let mut kfs = Keyframes::new();
        for (time, payload) in map {
            kfs.insert(time, payload);
        }
        Ok(kfs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_keyframes() {
        let kfs: Keyframes<f32> = Keyframes::new();
        assert_eq!(kfs.len(), 0);
        assert!(kfs.is_empty());
        assert_eq!(kfs.get(&0), None);
        assert_eq!(kfs.interpolation_segment(100), None);
        assert_eq!(kfs.last_keyframe_time(), None);
    }

    #[test]
    fn test_one_keyframe() {
        let mut kfs = Keyframes::new();
        kfs.insert(100, (10.0, Easing::Linear));
        assert_eq!(kfs.len(), 1);
        assert!(!kfs.is_empty());
        assert_eq!(kfs.get(&100), Some(&(10.0, Easing::Linear)));
        assert_eq!(kfs.get(&50), None);
        assert_eq!(kfs.interpolation_segment(50), None);
        assert_eq!(kfs.interpolation_segment(150), None);
        assert_eq!(kfs.last_keyframe_time(), Some(100));

        // Update in-place
        kfs.insert(100, (20.0, Easing::EaseIn));
        assert_eq!(kfs.len(), 1);
        assert_eq!(kfs.get(&100), Some(&(20.0, Easing::EaseIn)));
    }

    #[test]
    fn test_two_keyframes_interpolation() {
        let mut kfs = Keyframes::new();
        kfs.insert(100, (10.0, Easing::Linear));
        kfs.insert(200, (30.0, Easing::EaseOut));
        assert_eq!(kfs.len(), 2);
        assert_eq!(kfs.last_keyframe_time(), Some(200));

        // Outside segments
        assert_eq!(kfs.interpolation_segment(50), None);
        assert_eq!(kfs.interpolation_segment(100), None);
        assert_eq!(kfs.interpolation_segment(250), None);

        // Inside segment
        let seg = kfs.interpolation_segment(150);
        assert!(seg.is_some());
        let (found_time, v0, v1, progress, easing) = seg.unwrap();
        assert_eq!(found_time, 200);
        assert_eq!(*v0, 10.0);
        assert_eq!(*v1, 30.0);
        assert!((progress - 0.5).abs() < 1e-5);
        assert_eq!(*easing, Easing::EaseOut);
    }

    #[test]
    fn test_many_keyframes_sorted() {
        let mut kfs = Keyframes::new();
        kfs.insert(300, (30.0, Easing::Linear));
        kfs.insert(100, (10.0, Easing::Linear));
        kfs.insert(200, (20.0, Easing::EaseIn));
        assert_eq!(kfs.len(), 3);

        let times: Vec<u64> = kfs.keys().copied().collect();
        assert_eq!(times, vec![100, 200, 300]);

        let seg = kfs.interpolation_segment(250).unwrap();
        assert_eq!(seg.0, 300);
        assert_eq!(*seg.1, 20.0);
        assert_eq!(*seg.2, 30.0);
        assert!((seg.3 - 0.5).abs() < 1e-5);
    }

    #[test]
    fn test_split_off() {
        let mut kfs = Keyframes::new();
        kfs.insert(100, (10.0, Easing::Linear));
        kfs.insert(200, (20.0, Easing::Linear));
        kfs.insert(300, (30.0, Easing::Linear));

        kfs.split_off(&250);
        assert_eq!(kfs.len(), 2);
        assert_eq!(kfs.keys().copied().collect::<Vec<_>>(), vec![100, 200]);

        kfs.split_off(&150);
        assert_eq!(kfs.len(), 1);
        assert_eq!(kfs.keys().copied().collect::<Vec<_>>(), vec![100]);

        kfs.split_off(&50);
        assert_eq!(kfs.len(), 0);
        assert!(kfs.is_empty());
    }
}
