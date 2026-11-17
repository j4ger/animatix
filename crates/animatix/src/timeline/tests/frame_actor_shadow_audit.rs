//! Isolated regression probe for actor-reference freezing and lexical shadowing.

use std::collections::HashMap;
use std::path::Path;

use animatix_syntax::module::ModuleGraph;

use crate::timeline::{BuildOptions, SceneDimensions, Timeline, Value};

#[test]
fn actor_identity_freeze_does_not_replace_a_shadowing_let_in_frames() {
    let source = r#"
component MethodHost() {
  body: Rect, opacity: 1
  pub fn unused() { 1 }
}
#0s
card: Rect, opacity: 1
host: MethodHost
let card = 0.25
output: Rect, opacity: 1
always { output.opacity = card }
#1s
"#;
    let mut graph = ModuleGraph::new();
    let program = graph
        .load_program_with_source(Path::new("virtual:///actor_shadow.amx"), Some(source))
        .expect("load source");
    let mut errors = Vec::new();
    let expanded = program.expand_components(&mut errors);
    assert!(errors.is_empty(), "{errors:?}");
    let report = Timeline::build_with_options(
        &expanded.statements,
        &program.namespaces,
        BuildOptions::default()
            .with_function_context(&program.function_context)
            .with_expanded(&expanded),
    );
    assert!(
        report.diagnostics.iter().all(|diagnostic| !diagnostic.is_error()),
        "{:?}",
        report.diagnostics
    );

    let dimensions = SceneDimensions::default();
    let mut overrides = HashMap::new();
    let mut env = report.output.build_frame_env_internal(0, dimensions, &overrides);
    assert_eq!(env.get("card"), Some(Value::Num(0.25)));
    for modifier in &report.output.modifier_programs {
        report
            .output
            .apply_modifier_program(modifier, 0, dimensions, &mut env, &mut overrides)
            .expect("always expression evaluates");
    }
    assert_eq!(
        overrides.get("output").and_then(|properties| properties.get("opacity")),
        Some(&Value::Num(0.25)),
    );
}
