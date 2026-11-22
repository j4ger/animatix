use std::collections::HashMap;

use super::support::build_clean;
use crate::ast::Expr;
use crate::timeline::builtins::standard_environment;
use crate::timeline::modifier_runtime::ir::{ModifierOverrides, execute_modifier_ir};
use crate::timeline::utils::evaluate_expr;
use crate::timeline::{SceneDimensions, Value};

#[test]
fn range_returns_a_list_with_exclusive_end() {
    let env = standard_environment();
    let cases: &[(&[f64], &[f64])] = &[
        (&[0.0, 4.0], &[0.0, 1.0, 2.0, 3.0]),
        (&[-3.0, 4.0, 2.0], &[-3.0, -1.0, 1.0, 3.0]),
        (&[5.0, 0.0, -2.0], &[5.0, 3.0, 1.0]),
        (&[4.0, 0.0], &[]),
        (&[0.0, 4.0, -1.0], &[]),
        (&[2.0, 2.0], &[]),
        (
            &[9_007_199_254_740_989.0, 9_007_199_254_740_991.0],
            &[9_007_199_254_740_989.0, 9_007_199_254_740_990.0],
        ),
    ];
    for &(args, expected) in cases {
        let call = Expr::call("range", args.iter().copied().map(Expr::Num).collect());
        let Value::List(items) = evaluate_expr(&call, &env).unwrap() else {
            panic!("range({args:?}) must return a list");
        };
        assert_eq!(items.len(), expected.len(), "range({args:?})");
        for (item, &expected) in items.iter().zip(expected) {
            assert!(matches!(item, Value::Num(value) if *value == expected), "got {item:?}");
        }
    }
}

#[test]
fn range_rejects_invalid_arguments_and_large_lists() {
    let env = standard_environment();
    let cases = [
        ([0.5, 4.0, 1.0], "start must be an integer"),
        ([0.0, f64::INFINITY, 1.0], "end must be an integer"),
        ([0.0, 9_007_199_254_740_992.0, 1.0], "end must be an integer"),
        ([0.0, 4.0, 1.5], "step must be an integer"),
        ([0.0, 4.0, 0.0], "step must not be zero"),
        ([0.0, 100_001.0, 1.0], "cannot generate more than 100,000 items"),
    ];
    for (args, expected) in cases {
        let call = Expr::call("range", args.into_iter().map(Expr::Num).collect());
        let error = evaluate_expr(&call, &env).unwrap_err().to_string();
        assert!(error.contains(expected), "range({args:?}): {error}");
    }
}

#[test]
fn range_drives_build_and_frame_loops() {
    let timeline = build_clean(
        r#"
        config { resolution: (320, 180), duration: 2 }

        #0s
        for index in range(0, 3) {
            dot[index]: Rect, at: (40 + 40 * index, 90), size: (20, 20)
        }

        always {
            for index in std::range(start: 2, end: floor(t) - 1, step: -1) {
                dot[index].rotation = t + index
            }
        }
        "#,
    );
    for label in ["dot__0", "dot__1", "dot__2"] {
        assert!(timeline.world.tracks.get_id(label).is_some(), "missing actor {label}");
    }
    assert!(timeline.world.tracks.get_id("dot__3").is_none());

    let frame_overrides = |time_ms| {
        let mut env = timeline.build_frame_env(
            time_ms,
            SceneDimensions {
                width: 320,
                height: 180,
            },
            &HashMap::new(),
        );
        let mut overrides = ModifierOverrides::new();
        for program in &timeline.modifier_programs {
            execute_modifier_ir(program, &mut env, &mut overrides).expect("modifier execution");
        }
        overrides
    };
    let overrides = frame_overrides(500);
    for (label, expected) in [("dot__0", 0.5), ("dot__1", 1.5), ("dot__2", 2.5)] {
        let rotation = overrides.get(label).and_then(|props| props.get("rotation"));
        assert!(
            matches!(rotation, Some(Value::Num(value)) if *value == expected),
            "{rotation:?}"
        );
    }
    assert!(!frame_overrides(1500).contains_key("dot__0"));
}
