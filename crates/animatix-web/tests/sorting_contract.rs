#[path = "../../animatix/tests/contracts/sorting_oracle.rs"]
mod contract;

use animatix::composition::BuildTarget;
use animatix::renderer::text::FontContext;
use animatix::timeline::{BuildQuality, Timeline};
use animatix_web::host::build_document_with_modules;
use std::sync::Arc;

fn build(source: &str) -> Timeline {
    let doc = build_document_with_modules(
        source,
        &[],
        Arc::new(FontContext::new()),
        BuildQuality::Draft,
        None,
    );
    assert!(doc.result.ok, "{:?}", doc.result.diagnostics);
    assert!(doc.result.diagnostics.is_empty(), "{:?}", doc.result.diagnostics);
    assert!(doc.result.missing_imports.is_empty());
    let BuildTarget::SingleScene(timeline) = doc.target.expect("SDK target") else {
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
