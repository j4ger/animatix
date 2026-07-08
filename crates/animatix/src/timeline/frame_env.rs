//! Frame evaluation environment construction and modifier execution.
//!
//! [`Timeline::build_frame_env`] assembles the per-frame variable environment
//! (`t`, `scene_width`, track properties, overrides) that drives both rendering
//! and modifier evaluation. Modifier statements / IR / bytecode are executed
//! against this environment to produce property overrides.

use super::{
    Environment, SceneAnchor, SceneDimensions, Timeline, Value,
    scene_anchor_point, set_lookup_color,
    set_lookup_vec2,
};
use super::callout_geometry::resolve_anchor_point;

/// Apply a modifier override incrementally to the frame environment.
///
/// Instead of rebuilding the entire environment from scratch (which requires
/// cloning `self.env` and re-evaluating all track properties), this updates
/// only the specific key that changed plus any derived values.
pub(crate) fn apply_override_incremental(
    env: &mut Environment,
    label: &str,
    property: &str,
    value: Value,
) {
    let key = format!("{label}.{property}");
    env.set(&key, value.clone());

    // Inject typed sub-keys for known compound types
    match &value {
        Value::Vec2([x, y]) => {
            env.set(&format!("{key}.x"), Value::Num(*x));
            env.set(&format!("{key}.y"), Value::Num(*y));
        }
        Value::Color([r, g, b, a]) => {
            env.set(&format!("{key}.r"), Value::Num(*r));
            env.set(&format!("{key}.g"), Value::Num(*g));
            env.set(&format!("{key}.b"), Value::Num(*b));
            env.set(&format!("{key}.a"), Value::Num(*a));
        }
        _ => {}
    }

    // Recalculate derived values when size changes, but don't overwrite
    // an explicit radius override.
    if property == "size" {
        if let Value::Vec2([w, h]) = value {
            let radius_key = format!("{label}.radius");
            if env.get_ref(&radius_key).is_none() {
                env.set(&radius_key, Value::Num(w.min(h) / 2.0));
            }
            env.set(&format!("{label}.radius_x"), Value::Num(w / 2.0));
            env.set(&format!("{label}.radius_y"), Value::Num(h / 2.0));
        }
    }
}

impl Timeline {
    pub(super) fn build_eval_env(&self, time_ms: u64) -> Environment {
        let mut env = self.env.clone();
        self.inject_runtime_lookup_values(&mut env, time_ms, None, None);
        env
    }

    /// Internal implementation of [`Timeline::build_frame_env`].
    pub(super) fn build_frame_env_internal(
        &self,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Environment {
        // Estimate capacity: base env + t/scene_width/scene_height + variable tracks +
        // ~35 properties per actor (only if modifiers are present).
        let has_modifiers = !self.modifier_programs.is_empty() || !self.modifiers.is_empty();
        let estimated_capacity = if has_modifiers {
            self.env.len() + 3 + self.variable_tracks.len() + self.tracks.len() * 35
        } else {
            self.env.len() + 3 + self.variable_tracks.len()
        };
        let mut env = Environment::with_base(std::sync::Arc::clone(&self.env_base));
        env.overrides.reserve(estimated_capacity);
        env.set("t", Value::Num(time_ms as f64 / 1000.0));
        env.set("scene_width", Value::Num(scene_dimensions.width as f64));
        env.set("scene_height", Value::Num(scene_dimensions.height as f64));
        // Inject keyframe-scoped variable tracks into the frame environment.
        for (name, track) in &self.variable_tracks {
            if let Some(value) = track.evaluate(time_ms) {
                env.set(name, value);
            }
        }
        // Fast path: no modifiers and no procedural plots means no property
        // lookups at frame time. Skip the per-track property evaluation entirely.
        // Procedural plots still need actor property keys (e.g. `curve.freq`)
        // injected so their closures can resolve runtime parameters.
        if self.modifier_programs.is_empty()
            && self.modifiers.is_empty()
            && !self.has_procedural_plots()
        {
            return env;
        }
        self.inject_runtime_lookup_values(
            &mut env,
            time_ms,
            Some(scene_dimensions),
            Some(overrides),
        );
        env
    }

    /// Build the evaluation environment for a single frame.
    ///
    /// The returned [`Environment`] contains:
    /// - `t` — elapsed time in seconds.
    /// - `scene_width`, `scene_height` — pixel dimensions.
    /// - `scene.background_color` — resolved background color.
    /// - `scene.{top_left,top,…}` — anchor point vectors.
    /// - All actor properties sampled from tracks at `time_ms`.
    /// - Any per-frame overrides applied by modifiers.
    pub fn build_frame_env(
        &self,
        time_ms: u64,
        scene_dimensions: SceneDimensions,
        overrides: &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
    ) -> Environment {
        self.build_frame_env_internal(time_ms, scene_dimensions, overrides)
    }

    pub(super) fn inject_runtime_lookup_values(
        &self,
        env: &mut Environment,
        time_ms: u64,
        scene_dimensions: Option<SceneDimensions>,
        overrides: Option<
            &std::collections::HashMap<String, std::collections::HashMap<String, Value>>,
        >,
    ) {
        let background_color = overrides
            .and_then(|map| map.get("scene"))
            .and_then(|props| props.get("background_color"))
            .and_then(|value| match value {
                Value::Color(c) => Some(*c),
                Value::Vec4(c) => Some(*c),
                _ => None,
            })
            .unwrap_or_else(|| {
                let [r, g, b, a] = self.background_color.evaluate_copy(time_ms);
                [r as f64, g as f64, b as f64, a as f64]
            });
        set_lookup_color(env, "scene.background_color", background_color);

        if let Some(dimensions) = scene_dimensions {
            for (suffix, anchor) in [
                ("top_left", SceneAnchor::TopLeft),
                ("top", SceneAnchor::Top),
                ("top_right", SceneAnchor::TopRight),
                ("left", SceneAnchor::Left),
                ("center", SceneAnchor::Center),
                ("right", SceneAnchor::Right),
                ("bottom_left", SceneAnchor::BottomLeft),
                ("bottom", SceneAnchor::Bottom),
                ("bottom_right", SceneAnchor::BottomRight),
            ] {
                let point = scene_anchor_point(anchor, dimensions);
                set_lookup_vec2(env, &format!("scene.{}", suffix), [point.x, point.y]);
            }
        }

        for (label, track) in &self.tracks {
            // Use the centralized injector for base track values.
            crate::timeline::property_engine::inject_property_into_env(env, label, track, time_ms);

            // Apply overrides on top (from `always` blocks or modifiers).
            let node_overrides = overrides.and_then(|map| map.get(label));
            if let Some(overrides) = node_overrides {
                for (key, val) in overrides {
                    env.set(&format!("{label}.{key}"), val.clone());
                    // Also inject typed sub-keys for known properties
                    match val {
                        Value::Vec2([x, y]) => {
                            env.set(&format!("{label}.{key}.x"), Value::Num(*x));
                            env.set(&format!("{label}.{key}.y"), Value::Num(*y));
                        }
                        Value::Color([r, g, b, a]) => {
                            env.set(&format!("{label}.{key}.r"), Value::Num(*r));
                            env.set(&format!("{label}.{key}.g"), Value::Num(*g));
                            env.set(&format!("{label}.{key}.b"), Value::Num(*b));
                            env.set(&format!("{label}.{key}.a"), Value::Num(*a));
                        }
                        _ => {}
                    }
                }
            }

            // Recalculate derived values after overrides, but only if size
            // was actually overridden and radius wasn't explicitly set.
            if node_overrides.is_some_and(|o| o.contains_key("size")) {
                let radius_key = format!("{label}.radius");
                if env.get_ref(&radius_key).is_none() {
                    let size_val = env.get(&format!("{label}.size"));
                    if let Some(Value::Vec2([w, h])) = size_val {
                        env.set(&radius_key, Value::Num(w.min(h) / 2.0));
                    }
                }
                let size_val = env.get(&format!("{label}.size"));
                if let Some(Value::Vec2([w, h])) = size_val {
                    env.set(&format!("{label}.radius_x"), Value::Num(w / 2.0));
                    env.set(&format!("{label}.radius_y"), Value::Num(h / 2.0));
                }
            }

            // G5/G6: Inject actor-anchor-point lookups (`{label}.right`, etc.)
            // so that `n0.right` resolves as a plain env lookup.
            if let Some(dimensions) = scene_dimensions {
                for anchor in [
                    SceneAnchor::TopLeft,
                    SceneAnchor::Top,
                    SceneAnchor::TopRight,
                    SceneAnchor::Left,
                    SceneAnchor::Center,
                    SceneAnchor::Right,
                    SceneAnchor::BottomLeft,
                    SceneAnchor::Bottom,
                    SceneAnchor::BottomRight,
                ] {
                    if let Some(point) = resolve_anchor_point(
                        self,
                        label,
                        anchor,
                        time_ms,
                        dimensions,
                    ) {
                        let key = format!("{label}.{}", anchor.as_str());
                        set_lookup_vec2(env, &key, [point[0] as f64, point[1] as f64]);
                    }
                }
            }
        }
    }
}
