//! The scene camera: one keyframable transform applied to every root node.
//!
//! `camera` is a reserved assignment target rather than an actor: it owns no
//! track, draws nothing and takes part in no hit region. Writing it is how a
//! scene pans, zooms and turns:
//!
//! ```amx
//! #0.5s
//! camera.zoom = 1.6 [800ms, ease: expo-out]
//! camera.at = (60, -20) [800ms]
//! always { camera.at = (noise(t * 1.7) * 6.0, noise(t * 2.3, 9.0) * 6.0) }
//! ```
//!
//! `zoom` and `rotation` act about the scene center; `at` is a pan in screen
//! pixels. The transform composes *outside* every node, so it carries
//! scene-anchored actors too — a camera move is the whole plate moving, which
//! is what a real camera does. The background fill stays un-camerad, and the
//! static-subtree encoding cache is bypassed while a camera is authored (its
//! cached encoding cannot be re-transformed on append).
//!
//! The per-axis tracks are optional, so a scene that never addresses `camera`
//! samples three empty tracks and an identity affine once per frame.

use std::collections::HashMap;

use kurbo::Affine;

use crate::timeline::property_engine::{PropertyValue, write_f32, write_vec2};
use crate::timeline::{Easing, PropertyTrack, SceneDimensions, TrackAccessor, Value};

/// The reserved assignment target that addresses the camera.
///
/// Declared in `animatix-core` because the parser's label check runs above the
/// engine and has to agree with this one.
pub(crate) const CAMERA_TARGET: &str = animatix_core::property::CAMERA_TARGET;

/// The camera's three axes, as keyframe tracks like any actor property.
#[derive(Clone, Debug, Default)]
pub struct Camera {
    pan: Option<PropertyTrack<[f32; 2]>>,
    zoom: Option<PropertyTrack<f32>>,
    spin: Option<PropertyTrack<f32>>,
}

/// Names that address each axis, so `camera.at`, `camera.position` and
/// `camera.pan` are the same write.
const PAN_KEYS: [&str; 3] = ["at", "position", "pan"];
const ZOOM_KEYS: [&str; 2] = ["zoom", "scale"];
const SPIN_KEYS: [&str; 2] = ["rotation", "spin"];

const FOCUS_KEYS: [&str; 2] = ["focus", "focus_on"];

pub(crate) fn is_pan(property: &str) -> bool {
    PAN_KEYS.contains(&property)
}

pub(crate) fn is_zoom(property: &str) -> bool {
    ZOOM_KEYS.contains(&property)
}

pub(crate) fn is_spin(property: &str) -> bool {
    SPIN_KEYS.contains(&property)
}

pub(crate) fn is_focus(property: &str) -> bool {
    FOCUS_KEYS.contains(&property)
}

/// True for the properties a camera accepts.
pub(crate) fn is_camera_property(property: &str) -> bool {
    is_pan(property) || is_zoom(property) || is_spin(property) || is_focus(property)
}

/// The names of the properties a camera accepts, for diagnostics.
pub(crate) const CAMERA_PROPERTIES: &str =
    "at / position / pan, zoom / scale, rotation / spin, focus / focus_on";

fn override_vec2(overrides: Option<&HashMap<String, Value>>, keys: &[&str]) -> Option<[f32; 2]> {
    let value = keys.iter().find_map(|key| overrides?.get(*key))?;
    match value {
        Value::Vec2([x, y]) => Some([*x as f32, *y as f32]),
        // A frame-time expression can produce anything; keeping the keyframed
        // value is the useful half of that contract, so only a wrong *shape*
        // is worth a line of log.
        other => {
            tracing::debug!("camera: override is not a (x, y) pair: {other:?}");
            None
        },
    }
}

fn override_num(overrides: Option<&HashMap<String, Value>>, keys: &[&str]) -> Option<f32> {
    let value = keys.iter().find_map(|key| overrides?.get(*key))?;
    match value {
        Value::Num(n) => Some(*n as f32),
        other => {
            tracing::debug!("camera: override is not a number: {other:?}");
            None
        },
    }
}

impl Camera {
    /// True once any axis has been written by a keyframe assignment.
    ///
    /// `always`-block writes do not appear here — they never touch these tracks
    /// — so the frame-path gate additionally consults the modifier statements
    /// (see `Timeline::refresh_camera_used`).
    pub(crate) fn is_authored(&self) -> bool {
        self.pan.is_some() || self.zoom.is_some() || self.spin.is_some()
    }

    /// Apply a `camera.<property> = value` write to its axis.
    ///
    /// `false` means the property is not a camera axis, or the value is not the
    /// shape that axis takes; the caller reports both.
    pub(crate) fn assign(
        &mut self,
        property: &str,
        value: &Value,
        t_start_ms: u64,
        t_end_ms: u64,
        easing: Easing,
    ) -> bool {
        let has_duration = t_end_ms > t_start_ms;
        let has_delay = t_start_ms > 0 && !has_duration;
        if is_pan(property) {
            let Value::Vec2([x, y]) = value else {
                return false;
            };
            write_vec2(
                &mut self.pan,
                PropertyValue::Vec2([*x as f32, *y as f32]),
                t_start_ms,
                t_end_ms,
                easing,
                [0.0, 0.0],
                has_duration,
                has_delay,
            );
            true
        } else if is_zoom(property) {
            let Value::Num(zoom) = value else {
                return false;
            };
            // A delayed first write preserves the value in effect before its
            // stamp, and for a track that does not exist yet the write helpers
            // fall back to `T::default()` — `0.0` for zoom, which collapsed the
            // whole scene to its center point until the stamp arrived. Camera
            // axes have no declaration to seed their tracks, so the identity is
            // stated here. `pan` and `spin` need no such seeding: their
            // identity is `0.0`, which is what the fallback already yields.
            self.zoom.ensure(1.0);
            write_f32(
                &mut self.zoom,
                PropertyValue::F32(*zoom as f32),
                t_start_ms,
                t_end_ms,
                easing,
                1.0,
                has_duration,
                has_delay,
            );
            true
        } else if is_spin(property) {
            let Value::Num(spin) = value else {
                return false;
            };
            write_f32(
                &mut self.spin,
                PropertyValue::F32(*spin as f32),
                t_start_ms,
                t_end_ms,
                easing,
                0.0,
                has_duration,
                has_delay,
            );
            true
        } else {
            false
        }
    }

    #[allow(dead_code)] // Reserved for tooling inspection of camera pan axis
    /// Access the pan track if authored.
    pub fn pan(&self) -> Option<&PropertyTrack<[f32; 2]>> {
        self.pan.as_ref()
    }

    #[allow(dead_code)] // Reserved for tooling inspection of camera zoom axis
    /// Access the zoom track if authored.
    pub fn zoom(&self) -> Option<&PropertyTrack<f32>> {
        self.zoom.as_ref()
    }

    #[allow(dead_code)] // Reserved for tooling inspection of camera spin axis
    /// Access the spin track if authored.
    pub fn spin(&self) -> Option<&PropertyTrack<f32>> {
        self.spin.as_ref()
    }

    /// The value each axis holds at `time_ms`, identity where unwritten.
    ///
    /// This is what a scene hands to its successor by `persist camera`; an
    /// `always`-driven axis is deliberately not included, because its value is
    /// per-frame state that no track holds (see `Timeline::refresh_camera_used`
    /// for the same distinction).
    pub fn values_at(&self, time_ms: u64) -> ([f32; 2], f32, f32) {
        // `TrackAccessor` is implemented on the `Option`, so an axis that was
        // never written answers with its identity.
        (
            self.pan.get(time_ms, [0.0, 0.0]),
            self.zoom.get(time_ms, 1.0),
            self.spin.get(time_ms, 0.0),
        )
    }

    /// Start this camera from a value another scene ended on.
    ///
    /// Written as the `time_ms` stamp rather than t=0 so the receiving scene's
    /// own authored writes still win from their own stamps onward; identity
    /// axes are skipped, which keeps a scene that only ever inherited an
    /// identity camera out of the frame path entirely.
    pub(crate) fn seed_from(&mut self, pan: [f32; 2], zoom: f32, spin: f32, time_ms: u64) {
        if pan != [0.0, 0.0] {
            self.pan.ensure([0.0, 0.0]).add_keyframe(time_ms, pan, Easing::Linear);
        }
        if zoom != 1.0 {
            self.zoom.ensure(1.0).add_keyframe(time_ms, zoom, Easing::Linear);
        }
        if spin != 0.0 {
            self.spin.ensure(0.0).add_keyframe(time_ms, spin, Easing::Linear);
        }
    }

    /// The last stamp any axis carries, in ms.
    ///
    /// `Timeline::duration_seconds()` walks keyframe times to find the content
    /// end, and the camera lives outside `tracks` — so without this a scene whose
    /// only motion is a camera move measures zero length and stops at frame 0.
    pub(crate) fn max_keyframe_time_ms(&self) -> u64 {
        self.pan
            .last_time()
            .into_iter()
            .chain(self.zoom.last_time())
            .chain(self.spin.last_time())
            .max()
            .unwrap_or(0)
    }

    /// The three axes sampled at two times, for the loop-seam check.
    ///
    /// Camera axes live outside `Timeline::tracks`, so the `seamless_loop` lint
    /// cannot see them by walking actors — and a scene that ends pushed in and
    /// restarts unzoomed jumps at every seam like any other un-wrapped value.
    /// Identity values match [`Camera::affine`]'s fallbacks, so an axis that was
    /// never written wraps trivially.
    pub(crate) fn seam_pairs(
        &self,
        first_ms: u64,
        last_ms: u64,
    ) -> [(&'static str, PropertyValue, PropertyValue); 3] {
        [
            (
                "camera.at",
                PropertyValue::Vec2(self.pan.get(first_ms, [0.0, 0.0])),
                PropertyValue::Vec2(self.pan.get(last_ms, [0.0, 0.0])),
            ),
            (
                "camera.zoom",
                PropertyValue::F32(self.zoom.get(first_ms, 1.0)),
                PropertyValue::F32(self.zoom.get(last_ms, 1.0)),
            ),
            (
                "camera.rotation",
                PropertyValue::F32(self.spin.get(first_ms, 0.0)),
                PropertyValue::F32(self.spin.get(last_ms, 0.0)),
            ),
        ]
    }

    /// The frame's camera transform, in screen space.
    pub(crate) fn affine(
        &self,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: Option<&HashMap<String, Value>>,
    ) -> Affine {
        let pan = override_vec2(overrides, &PAN_KEYS)
            .unwrap_or_else(|| self.pan.get(time_ms, [0.0, 0.0]));
        let zoom =
            override_num(overrides, &ZOOM_KEYS).unwrap_or_else(|| self.zoom.get(time_ms, 1.0));
        let spin =
            override_num(overrides, &SPIN_KEYS).unwrap_or_else(|| self.spin.get(time_ms, 0.0));
        let center = (scene_dimensions.width as f64 / 2.0, scene_dimensions.height as f64 / 2.0);
        // Right-to-left composition: move the world so the scene center is the
        // origin, turn and scale about it, put it back, then pan in screen px.
        Affine::translate((center.0 + pan[0] as f64, center.1 + pan[1] as f64))
            * Affine::rotate(spin as f64)
            * Affine::scale(zoom as f64)
            * Affine::translate((-center.0, -center.1))
    }
}
