use std::collections::HashSet;

use super::*;

/// The "LLM Generation Checklist" in `docs/spec.md` lists the supported
/// primitives by hand; this test pins that list to the catalog so the
/// author-facing name list cannot drift when cards are added or removed.
#[test]
fn spec_llm_checklist_primitives_match_catalog() {
    let spec = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/spec.md"));
    let line = spec
        .lines()
        .find(|l| l.starts_with("- Use supported primitives only:"))
        .expect("docs/spec.md must carry the LLM checklist primitive line");
    let documented: HashSet<&str> = line.split('`').skip(1).step_by(2).collect();
    let catalog: HashSet<&str> = CATALOG.iter().map(|info| &*info.type_name).collect();
    let missing: Vec<&&str> = catalog.difference(&documented).collect();
    let extra: Vec<&&str> = documented.difference(&catalog).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "docs/spec.md LLM checklist out of sync with CATALOG: missing {missing:?}, extra {extra:?}"
    );
}
/// `connector` is the catalog datum that keeps Connector out of the
/// `Primitive::build` dispatch: its declaration properties are written into
/// the connector geometry lanes by the generic actor body. Exactly one card
/// may set it, so a new annotation primitive cannot silently inherit the
/// inline path (or lose Connector's) without this failing first.
#[test]
fn exactly_one_card_declares_connector_geometry() {
    let flagged: Vec<&str> = CATALOG
        .iter()
        .filter(|info| info.connector)
        .map(|info| &*info.type_name)
        .collect();
    assert_eq!(
        flagged,
        vec!["Connector"],
        "the connector-geometry card set changed; update `find_actor_kind` and the \
             generic body's connector block together"
    );
}

/// Every plot card declares its `PlotKind`, and no non-plot card does. The
/// plot builder reads this instead of comparing type-name strings, so a new
/// plot primitive that forgets `plot_kind(...)` would silently fall through
/// to the `Curve` path.
#[test]
fn plot_cards_declare_their_plot_kind() {
    use animatix_core::caps::PlotKind;
    let mut declared: Vec<(&str, PlotKind)> = CATALOG
        .iter()
        .filter_map(|info| info.plot.map(|kind| (&*info.type_name, kind)))
        .collect();
    declared.sort_by_key(|(name, _)| *name);
    assert_eq!(
        declared,
        vec![
            ("BarChart", PlotKind::BarChart),
            ("ContourSet", PlotKind::ContourSet),
            ("Heatmap", PlotKind::Heatmap),
            ("NumberPlane", PlotKind::NumberPlane),
            ("PlotCurve", PlotKind::Curve),
            ("VectorField", PlotKind::VectorField),
        ],
        "the plot-family catalog data drifted from the six plot primitives"
    );
    // `Graph` is a plot *host*, not a family member.
    assert!(
        CATALOG
            .iter()
            .find(|i| &*i.type_name == "Graph")
            .is_some_and(|i| i.plot.is_none()),
        "Graph hosts plot children and must not declare a PlotKind"
    );
}
