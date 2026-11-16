#[path = "contracts/sorting_oracle.rs"]
mod contract;

use animatix::composition::BuildTarget;
use animatix::timeline::{BuildOptions, Timeline};
use animatix_syntax::module::ModuleGraph;

fn build(source: &str) -> Timeline {
    let mut graph = ModuleGraph::new();
    let program = graph
        .load_program_with_source(std::path::Path::new("sorting.amx"), Some(source))
        .expect("fixture parses");
    let mut diagnostics = Vec::new();
    let ast = program.expand_components(&mut diagnostics);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let report =
        BuildTarget::build_with_options(&ast, &Default::default(), None, BuildOptions::default());
    assert!(report.diagnostics.is_empty(), "{:?}", report.diagnostics);
    let BuildTarget::SingleScene(timeline) = report.output else {
        panic!("single scene")
    };
    timeline
}

#[test]
fn sorting_positions_timing_and_live_counts() {
    let failures = contract::violations(&build(contract::SOURCE));
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn sorting_oracle_rejects_targeted_faults() {
    for (name, source, expected) in contract::faults() {
        assert_ne!(source, contract::SOURCE, "mutation must apply: {name}");
        let failures = contract::violations(&build(&source));
        assert!(failures.contains(&expected), "{name}: expected {expected}, got {failures:?}");
    }
}
