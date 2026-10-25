//! Frame evaluation environment construction and modifier execution.
//!
//! [`Timeline::build_frame_env`] assembles the per-frame variable environment
//! (`t`, `scene_width`, track properties, overrides) that drives both rendering
//! and modifier evaluation. Modifier IR programs are executed against this
//! environment to produce property overrides.

use super::{
    Environment, SceneAnchor, SceneDimensions, Timeline, Value, scene_anchor_point,
    set_lookup_color_into, set_lookup_vec2_into,
};

thread_local! {
    /// Reusable env-key buffer for `apply_override_incremental` (PF-6): each
    /// modifier-IR assignment `format!`-built its key plus one owned clone per
    /// sub-key; on the pooled env all of those keys already exist, so writing
    /// through one buffer makes the whole write allocation-free. The buffer
    /// never escapes and there is no reentrancy (`env.set` cannot call back
    /// into this function while it is borrowed).
    static OVERRIDE_KEY: std::cell::RefCell<String> =
        std::cell::RefCell::new(String::with_capacity(96));
}

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
    OVERRIDE_KEY.with_borrow_mut(|key| {
        crate::timeline::env_keys::property_into(label, property, key);
        env.set(key, value.clone());

        // Inject typed sub-keys for known compound types (buffer reused).
        match &value {
            Value::Vec2([x, y]) => {
                let base_len = key.len();
                key.push_str(".x");
                env.set(key, Value::Num(*x));
                key.truncate(base_len);
                key.push_str(".y");
                env.set(key, Value::Num(*y));
            },
            Value::Color([r, g, b, a]) => {
                let base_len = key.len();
                for (suffix, channel) in [(".r", *r), (".g", *g), (".b", *b), (".a", *a)] {
                    key.push_str(suffix);
                    env.set(key, Value::Num(channel));
                    key.truncate(base_len);
                }
            },
            _ => {},
        }

        // Recalculate derived values when size changes, but don't overwrite
        // an explicit radius override.
        if property == "size" {
            if let Value::Vec2([w, h]) = value {
                crate::timeline::env_keys::property_into(label, "radius", key);
                if env.get_ref(key).is_none() {
                    env.set(key, Value::Num(w.min(h) / 2.0));
                }
                crate::timeline::env_keys::property_into(label, "radius_x", key);
                env.set(key, Value::Num(w / 2.0));
                crate::timeline::env_keys::property_into(label, "radius_y", key);
                env.set(key, Value::Num(h / 2.0));
            }
        }
    });
}

impl Timeline {
    pub(super) fn build_eval_env(&self, time_ms: u64) -> Environment {
        let mut env = self.env.clone();
        // Reserve up front: this runs once per declaration during build and
        // injects every track's properties, so growing unreserved would rehash
        // the override map repeatedly (O(declarations²) insert work).
        env.reserve_overrides(self.variable_tracks.len() + self.world.tracks.len() * 40);
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
        // Build path: the referenced-roots analysis is not final while the
        // declaration walk is still running, so no cached plan — the
        // injection loop falls back to per-label decisions.
        self.inject_runtime_lookup_values(&mut env, time_ms, None, None, None);
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
        // The capacity estimate and every per-actor injection decision are
        // build-fixed (referenced-roots analysis, wildcard targets, registry
        // property counts). The injection plan computes them once after each
        // invalidation; the old per-frame walk rebuilt them every frame and
        // alone did a `format!` allocation per label for wildcard membership.
        // 120 = measured inserts for one fully-injected Rect track (registry
        // properties with defaults + `_animating_` flags + override sub-keys,
        // 117 in total) rounded up; under-reserving triggers a rehash that
        // costs more than the mmap it avoids, so err generous (see PF-6).
        let plan = self.env_injection_plan();
        let estimated_capacity = 16 + self.variable_tracks.len() + plan.estimated_prop_entries;
        let has_runtime_injection = plan.runtime_injection;
        // PF-6 pool: the override-layer key set is identical frame-to-frame
        // (actor labels × registry properties are build-time-fixed), so the
        // environment is retained and its keys overwritten in place via
        // `set`'s `get_mut` fast path; the only cross-frame key-set variance
        // is a variable track evaluating to `None` before its first keyframe,
        // which removes the stale key below. `invalidate_frame_cache` drops
        // the pool on every mutation, and capacity is re-checked as cheap
        // insurance against a mismatched pooled environment.
        let mut env = match self.env_pool.borrow_mut().take() {
            Some(mut pooled) if pooled.overrides.capacity() >= estimated_capacity => {
                pooled.reset_for_reuse();
                pooled
            },
            Some(stale) => {
                drop(stale);
                let mut env = Environment::with_base(std::sync::Arc::clone(&self.env_base));
                env.overrides.reserve(estimated_capacity);
                env
            },
            None => {
                let mut env = Environment::with_base(std::sync::Arc::clone(&self.env_base));
                env.overrides.reserve(estimated_capacity);
                env
            },
        };
        let env_is_pooled = self.env_pool.borrow().is_none();
        env.set("t", Value::Num(time_ms as f64 / 1000.0));
        env.set("scene_width", Value::Num(scene_dimensions.width as f64));
        env.set("scene_height", Value::Num(scene_dimensions.height as f64));
        // Inject keyframe-scoped variable tracks into the frame environment.
        // A pooled env may carry a key from a frame where the track still
        // evaluated to a value; once it returns `None` (before its first
        // keyframe) that key must be removed so the environment is
        // indistinguishable from a fresh build.
        for (name, track) in &self.variable_tracks {
            match track.evaluate(time_ms) {
                Some(value) => env.set(name, value),
                None => {
                    if env_is_pooled {
                        env.overrides.remove(name);
                    }
                },
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
            // PF-6: one shared key buffer for the whole injection pass — the
            // per-track `String::with_capacity` calls were an allocation each.
            let mut key = String::with_capacity(96);
            for (label, id) in self.world.tracks.iter() {
                crate::timeline::property_engine::inject_extension_properties_into_env(
                    &mut env,
                    &mut key,
                    label,
                    self.world.property_plan(id),
                    time_ms,
                    self.extensions.as_deref(),
                    None,
                    false,
                );
            }
            return env;
        }
        self.inject_runtime_lookup_values(
            &mut env,
            time_ms,
            Some(scene_dimensions),
            Some(overrides),
            Some(&plan),
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
        plan: Option<&crate::timeline::timeline_impl::EnvInjectionPlan>,
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
        // PF-6: one shared key buffer across background color, anchors, and
        // the track loop below — every `format!`/`with_capacity` here was a
        // per-frame allocation. Sub-key writes reuse the same buffer in place;
        // on a pooled env `set`'s get_mut-first path makes them
        // allocation-free.
        let mut key = String::with_capacity(96);
        set_lookup_color_into(env, &mut key, background_color);

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
                key.clear();
                key.push_str("scene.");
                key.push_str(suffix);
                set_lookup_vec2_into(env, &mut key, [point.x, point.y]);
            }
        }

        for (label, id) in self.world.tracks.iter() {
            // Referenced-root filtering, wildcard membership, and the
            // referenced-property filter are all build-fixed; the injection
            // plan (see `Timeline::env_injection_plan`) answers them with one
            // indexed read instead of the per-label `format!`/`split_once`/
            // `HashSet` walk this loop used to run every frame.
            // Over-injection is safe; the filter only skips work.
            let decision = plan
                .and_then(|plan| plan.decision(id))
                .unwrap_or_else(|| self.env_inject_decision(label));
            let (allowed_props, inject_animating_flags) = match decision {
                crate::timeline::timeline_impl::EnvInjectDecision::Skip => continue,
                crate::timeline::timeline_impl::EnvInjectDecision::NoProps => {
                    // Actor's properties are not read by any expression.
                    // Apply explicit node overrides if present, then skip base property
                    // injection.
                    let node_overrides = overrides.and_then(|map| map.get(label));
                    if let Some(overrides) = node_overrides {
                        for (okey, val) in overrides {
                            crate::timeline::env_keys::property_into(label, okey, &mut key);
                            env.set(&key, val.clone());
                            match val {
                                Value::Vec2([x, y]) => {
                                    let base_len = key.len();
                                    key.push_str(".x");
                                    env.set(&key, Value::Num(*x));
                                    key.truncate(base_len);
                                    key.push_str(".y");
                                    env.set(&key, Value::Num(*y));
                                },
                                Value::Color([r, g, b, a]) => {
                                    let base_len = key.len();
                                    key.push_str(".r");
                                    env.set(&key, Value::Num(*r));
                                    key.truncate(base_len);
                                    key.push_str(".g");
                                    env.set(&key, Value::Num(*g));
                                    key.truncate(base_len);
                                    key.push_str(".b");
                                    env.set(&key, Value::Num(*b));
                                    key.truncate(base_len);
                                    key.push_str(".a");
                                    env.set(&key, Value::Num(*a));
                                },
                                _ => {},
                            }
                        }
                    }
                    continue;
                },
                crate::timeline::timeline_impl::EnvInjectDecision::Wildcard(animating) => {
                    (None, animating)
                },
                crate::timeline::timeline_impl::EnvInjectDecision::Props(allowed, animating) => {
                    (Some(allowed), animating)
                },
            };

            // Use the centralized injector for base track values (key buffer
            // shared with the anchors above).
            crate::timeline::property_engine::inject_property_into_env(
                env,
                &mut key,
                label,
                id,
                &self.world,
                time_ms,
                allowed_props,
                inject_animating_flags,
            );
            crate::timeline::property_engine::inject_extension_properties_into_env(
                env,
                &mut key,
                label,
                self.world.property_plan(id),
                time_ms,
                self.extensions.as_deref(),
                allowed_props,
                inject_animating_flags,
            );

            // Apply overrides on top (from `always` blocks or modifiers).
            let node_overrides = overrides.and_then(|map| map.get(label));
            if let Some(overrides) = node_overrides {
                for (okey, val) in overrides {
                    crate::timeline::env_keys::property_into(label, okey, &mut key);
                    env.set(&key, val.clone());
                    // Also inject typed sub-keys for known properties, reusing
                    // the buffer (PF-6: these were `format!` allocations).
                    match val {
                        Value::Vec2([x, y]) => {
                            let base_len = key.len();
                            key.push_str(".x");
                            env.set(&key, Value::Num(*x));
                            key.truncate(base_len);
                            key.push_str(".y");
                            env.set(&key, Value::Num(*y));
                        },
                        Value::Color([r, g, b, a]) => {
                            let base_len = key.len();
                            key.push_str(".r");
                            env.set(&key, Value::Num(*r));
                            key.truncate(base_len);
                            key.push_str(".g");
                            env.set(&key, Value::Num(*g));
                            key.truncate(base_len);
                            key.push_str(".b");
                            env.set(&key, Value::Num(*b));
                            key.truncate(base_len);
                            key.push_str(".a");
                            env.set(&key, Value::Num(*a));
                        },
                        _ => {},
                    }
                }
            }

            // Recalculate derived values after overrides, but only if size
            // was actually overridden and radius wasn't explicitly set.
            if node_overrides.is_some_and(|o| o.contains_key("size")) {
                crate::timeline::env_keys::property_into(label, "radius", &mut key);
                if env.get_ref(&key).is_none() {
                    crate::timeline::env_keys::property_into(label, "size", &mut key);
                    let size_val = env.get(&key);
                    if let Some(Value::Vec2([w, h])) = size_val {
                        crate::timeline::env_keys::property_into(label, "radius", &mut key);
                        env.set(&key, Value::Num(w.min(h) / 2.0));
                    }
                }
                crate::timeline::env_keys::property_into(label, "size", &mut key);
                let size_val = env.get(&key);
                if let Some(Value::Vec2([w, h])) = size_val {
                    crate::timeline::env_keys::property_into(label, "radius_x", &mut key);
                    env.set(&key, Value::Num(w / 2.0));
                    crate::timeline::env_keys::property_into(label, "radius_y", &mut key);
                    env.set(&key, Value::Num(h / 2.0));
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
