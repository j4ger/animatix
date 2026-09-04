//! Frame evaluation environment construction and modifier execution.
//!
//! [`Timeline::build_frame_env`] assembles the per-frame variable environment
//! (`t`, `scene_width`, track properties, overrides) that drives both rendering
//! and modifier evaluation. Modifier IR programs are executed against this
//! environment to produce property overrides.

use super::{
    Environment, SceneAnchor, SceneDimensions, Timeline, Value, scene_anchor_point,
    set_lookup_color, set_lookup_vec2,
};

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
    let key = crate::timeline::env_keys::property(label, property);
    env.set(&key, value.clone());

    // Inject typed sub-keys for known compound types
    match &value {
        Value::Vec2([x, y]) => {
            env.set(&format!("{key}.x"), Value::Num(*x));
            env.set(&format!("{key}.y"), Value::Num(*y));
        },
        Value::Color([r, g, b, a]) => {
            env.set(&format!("{key}.r"), Value::Num(*r));
            env.set(&format!("{key}.g"), Value::Num(*g));
            env.set(&format!("{key}.b"), Value::Num(*b));
            env.set(&format!("{key}.a"), Value::Num(*a));
        },
        _ => {},
    }

    // Recalculate derived values when size changes, but don't overwrite
    // an explicit radius override.
    if property == "size" {
        if let Value::Vec2([w, h]) = value {
            let radius_key = crate::timeline::env_keys::property(label, "radius");
            if env.get_ref(&radius_key).is_none() {
                env.set(&radius_key, Value::Num(w.min(h) / 2.0));
            }
            env.set(&crate::timeline::env_keys::property(label, "radius_x"), Value::Num(w / 2.0));
            env.set(&crate::timeline::env_keys::property(label, "radius_y"), Value::Num(h / 2.0));
        }
    }
}

impl Timeline {
    pub(super) fn build_eval_env(&self, time_ms: u64) -> Environment {
        let mut env = self.env.clone();
        // Reserve up front: this runs once per declaration during build and
        // injects every track's properties, so growing unreserved would rehash
        // the override map repeatedly (O(declarations²) insert work).
        env.reserve_overrides(self.variable_tracks.len() + self.tracks.len() * 40);
        // Inject variable tracks so build-time `let` declarations can reference
        // previously declared variables (e.g., `let a = list_swap(a, 0, 2)`).
        // Block-scoped (function-local) bindings shadow scene variable tracks
        // with the same name, so they are skipped here.
        for (name, track) in &self.variable_tracks {
            if self.block_scope.iter().any(|scope| scope.contains(name)) {
                continue;
            }
            if let Some(value) = track.evaluate(time_ms) {
                env.set(name, value);
            }
        }
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
        let _stage = crate::perf::ScopedStage::new(crate::perf::stage::BUILD_FRAME_ENV);
        // Estimate capacity: t/scene_width/scene_height + scene anchors +
        // variable tracks + ~120 properties per *injected* actor. The build
        // env is shared through `with_base` and never copied into `overrides`,
        // so sizing from `self.env.len()` massively over-reserved: the
        // 2026-09-04 allocation profile (`alloc_driver`, PF-6) measured one
        // ~430 KB hashbrown table allocation per frame on a 60-actor scene
        // whose overrides hold 131 entries, because referenced-roots filtering
        // skips 59 of the 60 tracks inside `inject_runtime_lookup_values`.
        // 120 = measured inserts for one Rect track (registry properties with
        // defaults + `_animating_` flags + override sub-keys, 117 in total)
        // rounded up; under-estimating triggers a SipHash rehash that costs
        // more than the mmap it avoids, so err on the generous side while
        // staying ~30× below the old `env.len()`-sized reservation.
        let has_modifiers = !self.modifier_programs.is_empty() || !self.modifiers.is_empty();
        let has_runtime_injection = has_modifiers || self.has_procedural_plots();
        let injected_actors = if has_runtime_injection {
            match &self.referenced_roots {
                Some(roots) => {
                    roots.iter().filter(|label| self.tracks.contains_key(label.as_str())).count()
                },
                None => self.tracks.len(),
            }
        } else {
            0
        };
        let estimated_capacity = 16 + self.variable_tracks.len() + injected_actors * 120;
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
        // `has_runtime_injection` is computed once above and reused here:
        // `has_procedural_plots` walks every track, and a second call was a
        // measurable regression on the env_50/100/200 benches (2026-09-04).
        if !has_runtime_injection {
            for (label, track) in &self.tracks {
                crate::timeline::property_engine::inject_extension_properties_into_env(
                    &mut env,
                    label,
                    track,
                    time_ms,
                    self.extensions.as_deref(),
                );
            }
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
            // Referenced-root filtering: expressions can only reference actor
            // labels that appear textually in the program (see
            // build::referenced_roots), so unreferenced actors' properties are
            // not needed in the environment. Over-injection is safe; the
            // filter only skips work.
            if let Some(roots) = &self.referenced_roots {
                if !roots.contains(label.as_str()) {
                    continue;
                }
            }
            // Use the centralized injector for base track values.
            crate::timeline::property_engine::inject_property_into_env(env, label, track, time_ms);
            crate::timeline::property_engine::inject_extension_properties_into_env(
                env,
                label,
                track,
                time_ms,
                self.extensions.as_deref(),
            );

            // Apply overrides on top (from `always` blocks or modifiers).
            let node_overrides = overrides.and_then(|map| map.get(label));
            if let Some(overrides) = node_overrides {
                for (key, val) in overrides {
                    env.set(&crate::timeline::env_keys::property(label, key), val.clone());
                    // Also inject typed sub-keys for known properties
                    match val {
                        Value::Vec2([x, y]) => {
                            env.set(
                                &crate::timeline::env_keys::property(label, &format!("{key}.x")),
                                Value::Num(*x),
                            );
                            env.set(
                                &crate::timeline::env_keys::property(label, &format!("{key}.y")),
                                Value::Num(*y),
                            );
                        },
                        Value::Color([r, g, b, a]) => {
                            env.set(
                                &crate::timeline::env_keys::property(label, &format!("{key}.r")),
                                Value::Num(*r),
                            );
                            env.set(&format!("{label}.{key}.g"), Value::Num(*g));
                            env.set(&format!("{label}.{key}.b"), Value::Num(*b));
                            env.set(&format!("{label}.{key}.a"), Value::Num(*a));
                        },
                        _ => {},
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

            // G5/G6: Actor anchor points (`{label}.right`, etc.) are now
            // resolved lazily from the frame environment at evaluation time
            // (see `env_anchor_point` in callout_geometry.rs).  This replaces
            // the old eager-injection approach, making anchor points reflect
            // `always`-block position overrides.
        }
    }
}
