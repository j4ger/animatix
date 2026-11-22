use std::collections::HashMap;

use super::support::build_clean;
use crate::ast::Expr;
use crate::timeline::builtins::standard_environment;
use crate::timeline::modifier_runtime::ir::{ModifierOverrides, execute_modifier_ir};
use crate::timeline::utils::evaluate_expr;
use crate::timeline::{SceneDimensions, Value};

#[test]
fn linear_progress_clamps_both_directions_and_handles_extreme_bounds() {
    let env = standard_environment();
    for (args, expected) in [
        ([0.0, 1.0, 5.0], 0.0),
        ([2.0, 1.0, 5.0], 0.25),
        ([6.0, 1.0, 5.0], 1.0),
        ([2.0, 5.0, 1.0], 0.75),
        ([0.0, -f64::MAX, f64::MAX], 0.5),
    ] {
        let call = Expr::call("linear_progress", args.into_iter().map(Expr::Num).collect());
        assert_eq!(evaluate_expr(&call, &env).unwrap(), Value::Num(expected));
    }
}

#[test]
fn linspace_places_endpoints_and_handles_empty_and_singleton_lists() {
    let env = standard_environment();
    let cases: &[(f64, f64, f64, &[f64])] = &[
        (0.0, 1.0, 5.0, &[0.0, 0.25, 0.5, 0.75, 1.0]),
        (3.0, -1.0, 3.0, &[3.0, 1.0, -1.0]),
        (4.0, 9.0, 0.0, &[]),
        (4.0, 9.0, 1.0, &[4.0]),
        (-f64::MAX, f64::MAX, 3.0, &[-f64::MAX, 0.0, f64::MAX]),
    ];
    for &(start, end, count, expected) in cases {
        let call = Expr::call("linspace", vec![Expr::Num(start), Expr::Num(end), Expr::Num(count)]);
        let expected = Value::List(expected.iter().copied().map(Value::Num).collect());
        assert_eq!(evaluate_expr(&call, &env).unwrap(), expected);
    }
}

#[test]
fn sequence_helpers_reject_bad_counts_or_bounds() {
    let env = standard_environment();
    let cases: &[(&str, &[f64], &str)] = &[
        ("linear_progress", &[0.0, 1.0, 1.0], "distinct bounds"),
        ("linear_progress", &[f64::NAN, 1.0, 2.0], "finite values"),
        ("linspace", &[0.0, f64::INFINITY, 1.0], "finite bounds"),
        ("linspace", &[0.0, 1.0, -1.0], "non-negative integer"),
        ("repeat", &[42.0, 1.5], "non-negative integer"),
        ("repeat", &[42.0, 100_001.0], "100,000 items"),
    ];
    for &(name, args, expected) in cases {
        let call = Expr::call(name, args.iter().copied().map(Expr::Num).collect());
        let error = evaluate_expr(&call, &env).unwrap_err().to_string();
        assert!(error.contains(expected), "{name} returned {error}");
    }
}

#[test]
fn list_slice_validates_bounds_and_repeat_preserves_values() {
    let env = standard_environment();
    let values = Expr::call("repeat", vec![Expr::Str("cue".into()), Expr::Num(3.0)]);
    let sliced = Expr::call("list_slice", vec![values.clone(), Expr::Num(1.0)]);
    assert_eq!(
        evaluate_expr(&sliced, &env).unwrap(),
        Value::List(vec![Value::Str("cue".into()); 2].into())
    );
    for (start, end) in [(-1.0, 2.0), (0.5, 2.0), (2.0, 1.0), (0.0, 4.0)] {
        let call = Expr::call("list_slice", vec![values.clone(), Expr::Num(start), Expr::Num(end)]);
        let error = evaluate_expr(&call, &env).unwrap_err().to_string();
        assert!(error.contains("integer bounds"), "{error}");
    }
}

#[test]
fn sequence_helpers_compose_in_amx_and_frame_evaluation() {
    let timeline = build_clean(
        r#"
        config { resolution: (320, 180), duration: 4 }
        let values = linspace(20, 100, 4, endpoint: false)
        let angles = list_reverse(list_concat(list_slice(values, end: 3), repeat(80, 1)))

        #0s
        for angle, i in angles {
            dot[i]: Rect, at: (angle, 90), size: (10, 10)
        }
        always {
            let p = std::linear_progress(t, from: 1, to: 3)
            for angle, i in list_slice(angles, 1) {
                dot[i].rotation = angle * p
            }
        }
        "#,
    );
    assert!(timeline.world.tracks.get_id("dot__3").is_some());
    assert!(timeline.world.tracks.get_id("dot__4").is_none());

    let mut env = timeline.build_frame_env(
        2000,
        SceneDimensions {
            width: 320,
            height: 180,
        },
        &HashMap::new(),
    );
    let mut overrides = ModifierOverrides::new();
    for program in &timeline.modifier_programs {
        execute_modifier_ir(program, &mut env, &mut overrides).unwrap();
    }
    for (label, rotation) in [("dot__0", 30.0), ("dot__1", 20.0), ("dot__2", 10.0)] {
        assert_eq!(overrides[label]["rotation"], Value::Num(rotation));
    }
    assert!(!overrides.contains_key("dot__3"));
}

#[test]
fn list_callbacks_generate_map_and_scan_typed_values() {
    let timeline = build_clean(
        r#"
        config { resolution: (320, 180), duration: 4 }
        fn triple(value: Num) -> Num { value * 3 }
        let inputs = std::tabulate(function: (i) => i + 1, count: 4)
        let sizes = list_map(inputs, triple)
        let distances = scan(0, sizes, (total, size) => total + size)
        #0s
        for (x, size) in zip(distances, sizes) {
            dot[x]: Rect, at: (x, 90), size: (size, size)
        }
        always {
            let rotated = list_map(sizes, (size) => size * t)
            dot[3].rotation = rotated[2]
        }
        "#,
    );
    for index in [3, 9, 18, 30] {
        assert!(timeline.world.tracks.get_id(&format!("dot__{index}")).is_some());
    }
    let mut env = timeline.build_frame_env(
        2000,
        SceneDimensions {
            width: 320,
            height: 180,
        },
        &HashMap::new(),
    );
    let mut overrides = ModifierOverrides::new();
    for program in &timeline.modifier_programs {
        execute_modifier_ir(program, &mut env, &mut overrides).unwrap();
    }
    assert_eq!(overrides["dot__3"]["rotation"], Value::Num(18.0));
}

#[test]
fn zip_rejects_different_list_lengths() {
    let call = Expr::call("zip", vec![Expr::List(vec![Expr::Num(1.0)]), Expr::List(vec![])]);
    let error = evaluate_expr(&call, &standard_environment()).unwrap_err().to_string();
    assert!(error.contains("equal length"), "{error}");
}

#[test]
fn generated_actor_list_staggers_in_list_order() {
    use crate::timeline::TrackAccessor;
    let timeline = build_clean(
        r#"
        config { resolution: (320, 180), duration: 4 }
        #0s
        for i in range(0, 3) {
            dot[i]: Ellipse, at: (40 + 80 * i, 90), size: (24, 24), opacity: 0
        }
        #1s
        let dots = tabulate(3, (i) => dot[2 - i])
        stagger_each(dots, 250ms, (item) => fade_in(item, 200ms))
        "#,
    );
    for (index, start) in [(2, 1000), (1, 1250), (0, 1500)] {
        let id = timeline.world.tracks.get_id(&format!("dot__{index}")).unwrap();
        let opacity = &timeline.world.style(id).opacity;
        assert_eq!(opacity.get(start, 0.0), 0.0);
        assert!(opacity.get(start + 100, 0.0) > 0.0);
        assert_eq!(opacity.get(start + 200, 0.0), 1.0);
    }
}
