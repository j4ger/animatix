//! Shared by native engine and SDK tests. This is a semantic, not pixel oracle.
use animatix::timeline::{SceneDimensions, Timeline, TrackAccessor, Value};
use std::collections::HashMap;

pub const SOURCE: &str = include_str!("sorting.amx");

pub fn faults() -> Vec<(&'static str, String, &'static str)> {
    vec![
        (
            "lost-order-state",
            SOURCE
                .replace("let order = list_swap(order, key - j, key - j - 1)", "let order = order"),
            "position",
        ),
        ("inner-clock", SOURCE.replace("step: 300ms", "step: 600ms"), "timing"),
        (
            "wrong-flag",
            SOURCE.replace("list_set(flags, (key - 1)*key/2+j, 1)", "list_set(flags, 0, 1)"),
            "readout",
        ),
        ("bar-height", SOURCE.replace("v*25", "v*20"), "size"),
        (
            "extra-actor",
            SOURCE.replace(
                "readout: Text",
                "extra: Rect, at: (240, 20), size: (10, 10), opacity: 1\nreadout: Text",
            ),
            "count",
        ),
        ("duration", SOURCE.replace("duration: 5.0", "duration: 6.0"), "duration"),
        (
            "constant-readout",
            SOURCE.replace("format(\"{:.0}/{:.0}\", comps, swaps)", "\"6/4\""),
            "readout",
        ),
    ]
}

pub fn violations(timeline: &Timeline) -> Vec<&'static str> {
    let mut failures = Vec::new();
    if timeline.world().len() != 5 {
        failures.push("count");
    }
    if timeline.declared_duration_seconds() != Some(5.0) {
        failures.push("duration");
    }
    // Actor identity never changes: y is tied to its original value, x to slot.
    for (time, xs, kind) in [
        (0, [100., 180., 260., 340.], "position"),
        (1100, [180., 100., 260., 340.], "timing"),
        (2300, [260., 100., 180., 340.], "timing"),
        (3800, [340., 100., 260., 180.], "timing"),
        (4900, [340., 100., 260., 180.], "position"),
    ] {
        for (i, (x, value)) in xs.into_iter().zip([4., 1., 3., 2.]).enumerate() {
            let id = timeline.world().get_id(&format!("bar__{i}")).expect("bar exists");
            let geometry = timeline.world().geometry(id);
            // Geometry tracks store half-extents; the fixture authors full dimensions.
            let half_size = geometry.size.get(time, [0., 0.]);
            let size = [half_size[0] * 2., half_size[1] * 2.];
            if (size[0] - 40.).abs() > 0.01 || (size[1] - value * 25.).abs() > 0.01 {
                failures.push("size");
            }
            let actual = geometry.position.get(time, [0., 0.]);
            if (actual[0] - x).abs() > 0.01 || (actual[1] - (180. - value * 12.5)).abs() > 0.01 {
                failures.push(kind);
            }
        }
    }
    let dims = SceneDimensions {
        width: 480,
        height: 270,
    };
    // Deliberately not exact step boundaries: avoid float-boundary ambiguity.
    for (time, expected) in [
        (900, "0/0"),
        (1110, "1/1"),
        (2310, "2/2"),
        (3810, "5/4"),
        (4110, "6/4"),
    ] {
        let mut overrides = HashMap::new();
        let mut env = timeline.build_frame_env(time, dims, &overrides);
        for program in &timeline.modifier_programs {
            timeline
                .apply_modifier_program(program, time, dims, &mut env, &mut overrides)
                .expect("modifier execution");
        }
        if overrides.get("readout").and_then(|props| props.get("text"))
            != Some(&Value::Str(expected.into()))
        {
            failures.push("readout");
        }
    }
    failures
}
